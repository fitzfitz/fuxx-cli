//! The browser inside a pane (M11b): a `WKWebView` placed over a GPUI placeholder element in
//! fuxx's own window.
//!
//! GPUI draws the whole window into one Metal-backed view; a web view added beside it, above it,
//! in the window's content view covers whatever GPUI draws in that rectangle. The placeholder's
//! `canvas` places the web view on each frame (see [`native_frame`]).
//!
//! The hidden `fuxx __pane-browser-probe` spike (Task 0) lives in `probe`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use futures::channel::oneshot;
use gpui::{Bounds, Pixels};
use objc2::rc::{Retained, Weak};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSEvent, NSResponder, NSView, NSWindowOrderingMode,
};
use objc2_foundation::{NSObject, NSPoint, NSRect, NSSize, NSString, NSURL, NSURLRequest};
use objc2_web_kit::{WKWebView, WKWebViewConfiguration};
use serde_json::Value;

use crate::{browser, browser_js};

mod probe;

pub use probe::run_probe;

/// Where a web view goes, as an AppKit frame in the window's content view, so it covers the GPUI
/// element at `bounds`. GPUI measures from the content's top-left corner and AppKit from its
/// bottom-left, so the top is flipped against `content_height` (the window's content height,
/// GPUI's viewport height). Both count points, so no scale applies: AppKit maps points to the
/// backing pixels itself.
pub fn native_frame(bounds: Bounds<Pixels>, content_height: Pixels) -> NSRect {
    let points = |p: Pixels| f64::from(f32::from(p));
    NSRect::new(
        NSPoint::new(
            points(bounds.origin.x),
            points(content_height - bounds.bottom()),
        ),
        NSSize::new(points(bounds.size.width), points(bounds.size.height)),
    )
}

/// A web view in fuxx's window, over a pane's placeholder. Main thread only. Dropping it takes
/// it out of the window.
pub struct PaneWebView {
    view: Retained<PageView>,
}

impl PaneWebView {
    /// A new, empty web view on the run's in-memory data store (keep-data off), not yet in a
    /// window.
    pub fn new(mtm: MainThreadMarker) -> PaneWebView {
        // SAFETY: main thread (the marker); plain WebKit set-up with valid arguments.
        let config = unsafe {
            let config = WKWebViewConfiguration::new(mtm);
            config.setWebsiteDataStore(&browser::data_store(false, mtm));
            config
        };
        let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(0.0, 0.0));
        PaneWebView {
            view: PageView::new(mtm, frame, &config),
        }
    }

    /// Put the web view in `content` (the window's content view), just above `gpui` (GPUI's
    /// view, also in `content`), so it covers what GPUI draws beneath it. Its top keeps its
    /// distance from the window's top while the window resizes, as GPUI's layout does, until
    /// the next frame places it exactly.
    pub fn attach(&self, content: &NSView, gpui: &NSView) {
        self.view
            .setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
        content.addSubview_positioned_relativeTo(
            &self.view,
            NSWindowOrderingMode::Above,
            Some(gpui),
        );
    }

    /// Cover the placeholder at `bounds` in a window whose content is `content_height` high,
    /// or hide (`hidden`). Called from the placeholder's prepaint, every frame; AppKit is only
    /// told what changed.
    pub fn place(&self, bounds: Bounds<Pixels>, content_height: Pixels, hidden: bool) {
        let frame = native_frame(bounds, content_height);
        if self.view.frame() != frame {
            self.view.setFrame(frame);
        }
        if self.view.isHidden() != hidden {
            self.view.setHidden(hidden);
        }
    }

    /// Go to `url` (already normalised to http or https). Returns at once.
    pub fn load(&self, url: &str) {
        if let Some(address) = NSURL::URLWithString(&NSString::from_str(url)) {
            // SAFETY: main thread; the view copies the request.
            unsafe {
                self.view
                    .loadRequest(&NSURLRequest::requestWithURL(&address))
            };
        }
    }

    /// Run `js` (one expression) in the page and answer with its value as JSON, as the agent
    /// browser's `run_script` does.
    pub fn eval(&self, js: &str) -> oneshot::Receiver<Result<Value, String>> {
        let (tx, rx) = oneshot::channel();
        let script = browser_js::eval_expression_js(js);
        browser::evaluate(&self.view, &script, Rc::new(Cell::new(Some(tx))));
        rx
    }

    /// Offer every key equivalent (⌘ and ⌃ shortcuts) to `gpui` (GPUI's view) before the page,
    /// and pass it the mouse moves AppKit gives the page while the page has the keyboard (AppKit
    /// sends them to the first responder only, so GPUI would see none, anywhere). The probe
    /// found AppKit asks GPUI's view first anyway; this keeps it so whatever the views' order,
    /// at the cost of GPUI being asked twice about a key it doesn't bind.
    pub fn route_events_to(&self, gpui: &NSView) {
        self.view.ivars().gpui.replace(Some(Weak::new(gpui)));
    }

    /// The web view itself.
    pub fn view(&self) -> &WKWebView {
        &self.view
    }

    /// How many key equivalents the web view has handled itself (not passed to GPUI first).
    pub fn key_equivalents_seen(&self) -> usize {
        self.view.ivars().seen.get()
    }
}

impl Drop for PaneWebView {
    fn drop(&mut self) {
        // SAFETY: main thread (a `PaneWebView` never leaves it).
        unsafe { self.view.stopLoading() };
        self.view.removeFromSuperview();
    }
}

/// What a page view keeps.
#[derive(Default)]
struct PageViewState {
    /// GPUI's view, offered each key equivalent first and given each mouse move
    /// (`route_events_to`).
    gpui: RefCell<Option<Weak<NSView>>>,
    /// Key equivalents the web view handled itself.
    seen: Cell<usize>,
}

define_class!(
    // SAFETY: WKWebView may be subclassed; `PageView` has no `Drop`.
    #[unsafe(super(WKWebView, NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = PageViewState]
    struct PageView;

    impl PageView {
        #[unsafe(method(performKeyEquivalent:))]
        fn perform_key_equivalent(&self, event: &NSEvent) -> bool {
            self.key_equivalent(event)
        }

        #[unsafe(method(mouseMoved:))]
        fn mouse_moved(&self, event: &NSEvent) {
            // SAFETY: WKWebView's own `mouseMoved:`, with the event it was given.
            let _: () = unsafe { msg_send![super(self), mouseMoved: event] };
            if let Some(gpui) = self.gpui() {
                gpui.mouseMoved(event);
            }
        }
    }
);

impl PageView {
    fn new(
        mtm: MainThreadMarker,
        frame: NSRect,
        config: &WKWebViewConfiguration,
    ) -> Retained<PageView> {
        let this = Self::alloc(mtm).set_ivars(PageViewState::default());
        // SAFETY: WKWebView's own initialiser, with valid arguments.
        unsafe { msg_send![super(this), initWithFrame: frame, configuration: config] }
    }

    /// GPUI's view first when routing is on (it answers only the shortcuts fuxx binds), then
    /// WebKit, which sends the key to the page while the page has the keyboard.
    fn key_equivalent(&self, event: &NSEvent) -> bool {
        if self
            .gpui()
            .is_some_and(|gpui| gpui.performKeyEquivalent(event))
        {
            return true;
        }
        self.ivars().seen.set(self.ivars().seen.get() + 1);
        // SAFETY: WKWebView's own `performKeyEquivalent:`, with the event it was given.
        unsafe { msg_send![super(self), performKeyEquivalent: event] }
    }

    /// GPUI's view, when events are routed to it.
    fn gpui(&self) -> Option<Retained<NSView>> {
        self.ivars().gpui.borrow().as_ref().and_then(Weak::load)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{point, px, size};

    fn bounds(x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(x), px(y)), size(px(w), px(h)))
    }

    fn rect(x: f64, y: f64, w: f64, h: f64) -> NSRect {
        NSRect::new(NSPoint::new(x, y), NSSize::new(w, h))
    }

    #[test]
    fn the_top_is_measured_from_the_bottom() {
        // 80 from the top of a 600-high window, 300 high: its bottom edge is 220 up from the
        // window's bottom.
        assert_eq!(
            native_frame(bounds(100.0, 80.0, 400.0, 300.0), px(600.0)),
            rect(100.0, 220.0, 400.0, 300.0)
        );
    }

    #[test]
    fn an_element_at_the_corners() {
        // The top-left corner: AppKit's top edge is the window's height.
        assert_eq!(
            native_frame(bounds(0.0, 0.0, 200.0, 100.0), px(600.0)),
            rect(0.0, 500.0, 200.0, 100.0)
        );
        // The bottom-left corner is AppKit's origin.
        assert_eq!(
            native_frame(bounds(0.0, 500.0, 200.0, 100.0), px(600.0)),
            rect(0.0, 0.0, 200.0, 100.0)
        );
        // The whole window.
        assert_eq!(
            native_frame(bounds(0.0, 0.0, 800.0, 600.0), px(600.0)),
            rect(0.0, 0.0, 800.0, 600.0)
        );
    }

    #[test]
    fn the_same_element_moves_when_the_window_grows() {
        // A window 100 taller: the element keeps its distance from the top, so AppKit's y grows.
        let element = bounds(60.0, 120.0, 520.0, 260.0);
        assert_eq!(
            native_frame(element, px(600.0)),
            rect(60.0, 220.0, 520.0, 260.0)
        );
        assert_eq!(
            native_frame(element, px(700.0)),
            rect(60.0, 320.0, 520.0, 260.0)
        );
    }

    #[test]
    fn fractions_of_a_point_are_kept() {
        // GPUI lays out in fractional points; AppKit takes them as they are (a Retina display
        // shows half points), so nothing is rounded.
        assert_eq!(
            native_frame(bounds(10.5, 20.25, 300.5, 150.75), px(601.5)),
            rect(10.5, 430.5, 300.5, 150.75)
        );
    }
}
