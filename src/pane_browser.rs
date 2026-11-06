//! The browser inside a pane (M11b): a `WKWebView` placed over a GPUI placeholder element in
//! fuxx's own window.
//!
//! GPUI draws the whole window into one Metal-backed view; a web view added beside it, above it,
//! in the window's content view covers whatever GPUI draws in that rectangle (and takes its
//! clicks). So the placeholder's `canvas` places the web view on each frame (see
//! [`native_frame`]), and it is hidden whenever something of fuxx's would be drawn under it
//! ([`page_hidden`]).
//!
//! The keyboard: AppKit's first responder is GPUI's view, except while the page has it. The page
//! takes it on a click in it (the `PageView` says so, as `PageEvent::Clicked`) or after a
//! navigation the person started; the browser tab then holds GPUI's focus, so fuxx's shortcuts act
//! on it. It gives the keyboard back to GPUI's view when GPUI's focus leaves the tab, and before
//! it hides.
//!
//! The hidden `fuxx __pane-browser-probe` (Task 0, extended in Task 4) lives in `probe`.

use std::cell::{Cell, RefCell};

use futures::channel::mpsc::UnboundedSender;
use gpui::{Bounds, Global, Pixels};
use objc2::rc::{Retained, Weak};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSEvent, NSEventModifierFlags, NSResponder, NSView,
    NSWindowOrderingMode,
};
use objc2_foundation::{NSObject, NSPoint, NSRect, NSSize};
use objc2_web_kit::{WKWebView, WKWebViewConfiguration};

use crate::browser::{Page, PageEvent};
use crate::reduce_motion;

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

/// What decides whether a browser tab's page shows this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Showing {
    /// Its tab is the active tab of a pane of the workspace on screen.
    pub tab_shown: bool,
    /// One of fuxx's overlays is open (`Overlays::any`).
    pub overlay_open: bool,
    /// A drag is under way (a divider, the sidebar's edge, a sidebar row, a pane tab), or the
    /// sidebar is sliding over the panes.
    pub dragging: bool,
}

/// The hide rule: the page shows only while its tab is on screen and nothing of fuxx's is drawn
/// over it or dragged across it. A native view covers GPUI's drawing and takes its clicks, so
/// anything GPUI draws there must not be hidden under it.
pub fn page_hidden(s: Showing) -> bool {
    !s.tab_shown || s.overlay_open || s.dragging
}

/// Which of fuxx's overlays are open: the boxes, panels, menus and popovers drawn over the
/// window. One open hides every page (`page_hidden`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Overlays {
    /// The rename box (a sidebar row's or a tab's).
    pub rename: bool,
    pub host_picker: bool,
    /// File → Open Browser…'s address box.
    pub address_box: bool,
    /// ⌘P.
    pub switcher: bool,
    /// A sidebar row's right-click menu.
    pub row_menu: bool,
    /// A pane tab's right-click menu.
    pub tab_menu: bool,
    /// The sidebar footer's "+ New" menu.
    pub new_menu: bool,
    /// A browser tab's address suggestions.
    pub suggestions: bool,
    /// The title area's notifications popover.
    pub notifications: bool,
}

impl Overlays {
    pub fn any(&self) -> bool {
        let Overlays {
            rename,
            host_picker,
            address_box,
            switcher,
            row_menu,
            tab_menu,
            new_menu,
            suggestions,
            notifications,
        } = *self;
        rename
            || host_picker
            || address_box
            || switcher
            || row_menu
            || tab_menu
            || new_menu
            || suggestions
            || notifications
    }
}

/// fuxx's window's content view and GPUI's view in it: where browser tabs' pages go. Set once the
/// window has opened (`main`); there is none in the UI screenshots, which make no pages.
pub struct NativeWindow {
    pub content: Retained<NSView>,
    pub gpui: Retained<NSView>,
}

impl Global for NativeWindow {}

impl NativeWindow {
    /// The window GPUI opened since `before` (the app's window numbers then), found the way the
    /// app finds its main window, and its two views. GPUI's is the content view's `GPUIView`.
    pub fn find(before: &[isize]) -> Option<NativeWindow> {
        let window = reduce_motion::window_opened_since(before)?;
        let content = window.contentView()?;
        let gpui = content
            .subviews()
            .iter()
            .find(|v| v.class().name().to_bytes() == b"GPUIView")?;
        Some(NativeWindow { content, gpui })
    }
}

/// A page in fuxx's window, over a pane's placeholder. Main thread only. Dropping it takes it
/// out of the window.
pub struct PaneWebView {
    /// None once `into_page` has given it away.
    page: Option<Page>,
    view: Retained<PageView>,
}

impl PaneWebView {
    /// A new, empty page, not yet in a window: on the saved data store with `keep_data`, else
    /// the run's in-memory one.
    pub fn new(keep_data: bool, mtm: MainThreadMarker) -> PaneWebView {
        let config = Page::configuration(keep_data, mtm);
        let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(0.0, 0.0));
        let view = PageView::new(mtm, frame, &config);
        let page = Page::new(Retained::into_super(view.clone()), keep_data, mtm);
        PaneWebView {
            page: Some(page),
            view,
        }
    }

    /// The page: loading, scripts, its state.
    pub fn page(&self) -> &Page {
        self.page
            .as_ref()
            .expect("a pane web view has its page until into_page")
    }

    /// Put the web view in `content` (the window's content view), just above `gpui` (GPUI's
    /// view, also in `content`), hidden until it is placed, so it covers what GPUI draws beneath
    /// it. Its top keeps its distance from the window's top while the window resizes, as GPUI's
    /// layout does, until the next frame places it exactly.
    pub fn attach(&self, content: &NSView, gpui: &NSView) {
        self.view.setHidden(true);
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
    /// told what changed. A page that has the keyboard gives it back to GPUI's view before it
    /// hides.
    pub fn place(&self, bounds: Bounds<Pixels>, content_height: Pixels, hidden: bool) {
        if hidden {
            self.hide();
            return;
        }
        let frame = native_frame(bounds, content_height);
        if self.view.frame() != frame {
            self.view.setFrame(frame);
        }
        if self.view.isHidden() {
            self.view.setHidden(false);
        }
    }

    /// Hide it (not placed this frame, or the hide rule), giving the keyboard back to GPUI's
    /// view first if it had it.
    pub fn hide(&self) {
        if self.view.isHidden() {
            return;
        }
        if self.has_keyboard() {
            self.give_keyboard_back();
        }
        self.view.setHidden(true);
    }

    /// Take it out of the window now, the page stopped (its tab closed): it never shows again,
    /// whoever still holds it.
    pub fn remove(&self) {
        self.hide();
        self.page().stop();
        self.view.removeFromSuperview();
    }

    /// Tell `events` about the page's navigations, and about clicks in it.
    pub fn listen(&self, events: UnboundedSender<PageEvent>) {
        self.page().set_events(Some(events.clone()));
        self.view.ivars().events.replace(Some(events));
    }

    /// Offer every key equivalent (⌘ and ⌃ shortcuts) to `gpui` (GPUI's view) before the page,
    /// and pass it ⌃Tab and the mouse moves AppKit gives the page while the page has the
    /// keyboard (AppKit sends them to the first responder only, so GPUI would see none,
    /// anywhere). The probe found AppKit asks GPUI's view first anyway; this keeps it so whatever
    /// the views' order, at the cost of GPUI being asked twice about a key it doesn't bind. It
    /// is also where the keyboard goes back to (`give_keyboard_back`).
    pub fn route_events_to(&self, gpui: &NSView) {
        self.view.ivars().gpui.replace(Some(Weak::new(gpui)));
    }

    /// Whether the page has the keyboard (its window's first responder).
    pub fn has_keyboard(&self) -> bool {
        let Some(window) = self.view.window() else {
            return false;
        };
        let view: &NSResponder = &self.view;
        window
            .firstResponder()
            .is_some_and(|r| std::ptr::eq(&*r, view))
    }

    /// Give the page the keyboard. False (and said) when AppKit refused, or it isn't in a window.
    pub fn take_keyboard(&self) -> bool {
        let Some(window) = self.view.window() else {
            return false;
        };
        let view: &NSResponder = &self.view;
        let took = window.makeFirstResponder(Some(view));
        if !took {
            eprintln!("[fuxx] browser: the page couldn't take the keyboard");
        }
        took
    }

    /// Give the keyboard back to GPUI's view. False (and said) when AppKit refused.
    pub fn give_keyboard_back(&self) -> bool {
        let (Some(window), Some(gpui)) = (self.view.window(), self.view.gpui()) else {
            return false;
        };
        let gpui: &NSResponder = &gpui;
        let gave = window.makeFirstResponder(Some(gpui));
        if !gave {
            eprintln!("[fuxx] browser: the keyboard couldn't go back to fuxx from the page");
        }
        gave
    }

    /// The page was clicked and fuxx hasn't caught up yet (its keyboard isn't to be taken away
    /// meanwhile); `settle_click` once it has.
    pub fn clicked(&self) -> bool {
        self.view.ivars().clicked.get()
    }

    pub fn settle_click(&self) {
        self.view.ivars().clicked.set(false);
    }

    /// The page, out of fuxx's window and no longer talking to GPUI (Pop Out: a browser window
    /// takes it on).
    pub fn into_page(mut self) -> Page {
        // The keyboard back to GPUI's view while the link to it is still there.
        if self.has_keyboard() {
            self.give_keyboard_back();
        }
        let state = self.view.ivars();
        state.gpui.take();
        state.events.take();
        self.view.removeFromSuperview();
        let page = self
            .page
            .take()
            .expect("a pane web view has its page until into_page");
        page.set_events(None);
        page
    }

    /// The web view itself.
    pub fn view(&self) -> &WKWebView {
        &self.view
    }

    /// How many key equivalents the web view has handled itself (not passed to GPUI first).
    #[allow(dead_code)] // the probe's
    pub fn key_equivalents_seen(&self) -> usize {
        self.view.ivars().seen.get()
    }
}

impl Drop for PaneWebView {
    fn drop(&mut self) {
        // Given away (`into_page`): it is another window's now.
        if self.page.is_none() {
            return;
        }
        if self.has_keyboard() {
            self.give_keyboard_back();
        }
        self.view.removeFromSuperview();
    }
}

/// What a page view keeps.
#[derive(Default)]
struct PageViewState {
    /// GPUI's view, offered each key equivalent first and given each mouse move
    /// (`route_events_to`).
    gpui: RefCell<Option<Weak<NSView>>>,
    /// Told about clicks in the page (`listen`).
    events: RefCell<Option<UnboundedSender<PageEvent>>>,
    /// Clicked, and fuxx hasn't caught up yet.
    clicked: Cell<bool>,
    /// Key equivalents the web view handled itself.
    seen: Cell<usize>,
}

/// The key code of Tab (`kVK_Tab`).
const KEY_TAB: u16 = 0x30;

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

        // A click in the page: the person means to type there. fuxx is told (it moves its own
        // focus to the browser tab), then WebKit has the click as usual.
        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            if let Some(events) = &*self.ivars().events.borrow() {
                self.ivars().clicked.set(true);
                let _ = events.unbounded_send(PageEvent::Clicked);
            }
            // SAFETY: WKWebView's own `mouseDown:`, with the event it was given.
            let _: () = unsafe { msg_send![super(self), mouseDown: event] };
        }

        // ⌃Tab / ⌃⇧Tab are the pane's next and previous tab, not the page's (nor AppKit's key
        // view loop): they go to GPUI's view.
        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            if is_ctrl_tab(event.modifierFlags(), event.keyCode())
                && let Some(gpui) = self.gpui()
            {
                gpui.keyDown(event);
                return;
            }
            // SAFETY: WKWebView's own `keyDown:`, with the event it was given.
            let _: () = unsafe { msg_send![super(self), keyDown: event] };
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

/// Whether a key is ⌃Tab or ⌃⇧Tab (Control held, without ⌘ or ⌥).
fn is_ctrl_tab(flags: NSEventModifierFlags, key_code: u16) -> bool {
    key_code == KEY_TAB
        && flags.contains(NSEventModifierFlags::Control)
        && !flags.intersects(NSEventModifierFlags::Command | NSEventModifierFlags::Option)
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
    fn only_control_tab_goes_to_fuxx() {
        let ctrl = NSEventModifierFlags::Control;
        let shift = NSEventModifierFlags::Shift;
        assert!(is_ctrl_tab(ctrl, KEY_TAB));
        assert!(is_ctrl_tab(ctrl | shift, KEY_TAB));
        assert!(!is_ctrl_tab(NSEventModifierFlags::empty(), KEY_TAB));
        assert!(!is_ctrl_tab(shift, KEY_TAB));
        assert!(!is_ctrl_tab(ctrl | NSEventModifierFlags::Command, KEY_TAB));
        assert!(!is_ctrl_tab(ctrl | NSEventModifierFlags::Option, KEY_TAB));
        assert!(!is_ctrl_tab(ctrl, 0x00));
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

    fn showing(tab_shown: bool, overlay_open: bool, dragging: bool) -> Showing {
        Showing {
            tab_shown,
            overlay_open,
            dragging,
        }
    }

    #[test]
    fn the_page_shows_only_on_screen_with_nothing_over_it_and_nothing_dragged() {
        assert!(!page_hidden(showing(true, false, false)));
        // Its tab isn't the active tab of a pane on screen (another tab, another workspace).
        assert!(page_hidden(showing(false, false, false)));
        // An overlay is open: it would be drawn under the page, and the page would take its
        // clicks.
        assert!(page_hidden(showing(true, true, false)));
        // A divider, the sidebar's edge or a row is dragged (or the sidebar slides over it).
        assert!(page_hidden(showing(true, false, true)));
        assert!(page_hidden(showing(false, true, true)));
    }

    #[test]
    fn every_overlay_hides_the_page() {
        let none = Overlays::default();
        assert!(!none.any());
        let each = [
            Overlays {
                rename: true,
                ..none
            },
            Overlays {
                host_picker: true,
                ..none
            },
            Overlays {
                address_box: true,
                ..none
            },
            Overlays {
                switcher: true,
                ..none
            },
            Overlays {
                row_menu: true,
                ..none
            },
            Overlays {
                tab_menu: true,
                ..none
            },
            Overlays {
                new_menu: true,
                ..none
            },
            Overlays {
                suggestions: true,
                ..none
            },
            Overlays {
                notifications: true,
                ..none
            },
        ];
        for overlays in each {
            assert!(overlays.any(), "{overlays:?}");
            assert!(page_hidden(Showing {
                tab_shown: true,
                overlay_open: overlays.any(),
                dragging: false,
            }));
        }
    }
}
