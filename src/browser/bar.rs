//! The browser window's top bar (macOS's own compact toolbar, as in Safari): back, forward,
//! reload / stop and the address field, with a thin loading line under it.
//!
//! The bar acts on the web view itself: its buttons and the field never go through GPUI. Its
//! looks (the buttons' states, the field's text, the line) are brought up to date by `refresh`,
//! which the workspace's browser check calls, and which every navigation calls right away.

use std::cell::{Cell, RefCell};
use std::time::{Duration, Instant};

use block2::RcBlock;
use objc2::rc::{Retained, Weak};
use objc2::runtime::{AnyObject, Bool, ProtocolObject, Sel};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAppearance, NSAppearanceCustomization, NSAppearanceNameAqua, NSAppearanceNameDarkAqua,
    NSAutoresizingMaskOptions, NSBox, NSBoxType, NSColor, NSControl, NSControlTextEditingDelegate,
    NSFont, NSFontAttributeName, NSImage, NSImageView, NSResponder, NSStringDrawing, NSTextField,
    NSTextFieldBezelStyle, NSTextFieldDelegate, NSTextView, NSTitlePosition, NSToolbar,
    NSToolbarDelegate, NSToolbarDisplayMode, NSToolbarItem, NSToolbarItemVisibilityPriorityHigh,
    NSView, NSWindow, NSWindowTitleVisibility, NSWindowToolbarStyle,
};
use objc2_foundation::{
    NSArray, NSDictionary, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString,
};
use objc2_web_kit::WKWebView;

use super::{BarLook, BrowserDelegate, navigate};
use crate::browser_url::normalize_url;

/// The toolbar items' identifiers, in the bar's order.
const BACK: &str = "fuxx.back";
const FORWARD: &str = "fuxx.forward";
const RELOAD: &str = "fuxx.reload";
const ADDRESS: &str = "fuxx.address";
const MARK: &str = "fuxx.mark";

/// The address field: never narrower than this, as wide as the bar allows (the maximum only
/// has to be wider than any screen: AppKit's toolbar gives a far larger one too little room).
const FIELD_MIN_WIDTH: f64 = 280.0;
const FIELD_MAX_WIDTH: f64 = 4_000.0;
const PLACEHOLDER: &str = "Enter an address";
/// The loading line's height, in points.
const LINE_HEIGHT: f64 = 2.0;
/// fuxx's mark (its logo, the person's choice), drawn as a picture the toolbar's icon size.
const MARK_TEXT: &str = "\u{1F595}";
const MARK_SIZE: f64 = 18.0;
/// The emoji's font size: its line fills the picture's height.
const MARK_FONT_SIZE: f64 = 15.0;
/// How long a refused address stays red in the field.
const ERROR_SHOWN: Duration = Duration::from_secs(2);

/// What the page lets the bar's buttons do.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Nav {
    can_back: bool,
    can_forward: bool,
    loading: bool,
}

/// The third button: reload the page, or stop it loading.
#[derive(Clone, Copy, Debug, PartialEq)]
enum ReloadButton {
    Reload,
    Stop,
}

/// The buttons as the bar shows them.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Buttons {
    back: bool,
    forward: bool,
    reload: ReloadButton,
}

/// Back and forward are enabled only when the page can go that way; reload becomes stop while
/// a page loads.
fn buttons(nav: Nav) -> Buttons {
    Buttons {
        back: nav.can_back,
        forward: nav.can_forward,
        reload: if nav.loading {
            ReloadButton::Stop
        } else {
            ReloadButton::Reload
        },
    }
}

/// What the address field shows for the page at `url` (`requested`: the address last asked for):
/// `None` while the person is editing it (their typing stays).
fn field_text(url: &str, requested: Option<&str>, editing: bool) -> Option<String> {
    if editing {
        return None;
    }
    let blank = url.is_empty() || url == "about:blank";
    Some(match (blank, requested) {
        (false, _) => url.to_string(),
        (true, Some(requested)) => requested.to_string(),
        (true, None) => String::new(),
    })
}

/// The loading line's width in a window `width` points wide, `None` (hidden) when nothing loads.
fn loading_line(loading: bool, progress: f64, width: f64) -> Option<f64> {
    loading.then(|| progress.clamp(0.0, 1.0) * width)
}

/// The address typed into the field, as the browser opens it: the same check agents get.
fn entered_address(text: &str) -> Result<String, String> {
    normalize_url(text)
}

/// The bar's items, in order: the buttons, the address field when it shows, and fuxx's mark
/// last.
fn bar_items(address_bar: bool) -> Vec<&'static str> {
    let mut items = vec![BACK, FORWARD, RELOAD];
    if address_bar {
        items.push(ADDRESS);
    }
    items.push(MARK);
    items
}

/// The reload button's symbol, label and tooltip.
fn reload_face(button: ReloadButton) -> (&'static str, &'static str, &'static str) {
    match button {
        ReloadButton::Reload => ("arrow.clockwise", "Reload", "Reload this page"),
        ReloadButton::Stop => ("xmark", "Stop", "Stop loading this page"),
    }
}

thread_local! {
    /// Numbers each bar's toolbar: AppKit keeps toolbars that share an identifier in step, and
    /// each window's bar is its own.
    static TOOLBARS: Cell<u64> = const { Cell::new(0) };
}

/// What the bar keeps. The web view and the page's delegate belong to the `Browser`, so the bar
/// only points at them; the toolbar holds the bar weakly, and the `Browser` keeps it.
pub(super) struct BarState {
    view: Weak<WKWebView>,
    page: Weak<BrowserDelegate>,
    field: Retained<NSTextField>,
    /// The loading line, over the top edge of the page.
    line: Retained<NSBox>,
    /// The buttons the toolbar holds now (it asks the bar for them).
    back: RefCell<Option<Retained<NSToolbarItem>>>,
    forward: RefCell<Option<Retained<NSToolbarItem>>>,
    reload: RefCell<Option<Retained<NSToolbarItem>>>,
    /// What the reload button shows now (`None`: not set yet).
    reload_shows: Cell<Option<ReloadButton>>,
    /// Whether the address field is in the bar.
    address_bar: Cell<bool>,
    /// Until when the field shows a refused address in red.
    error_until: Cell<Option<Instant>>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and `TopBar` has no `Drop`.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = BarState]
    pub(super) struct TopBar;

    // SAFETY: NSObjectProtocol has no safety requirements.
    unsafe impl NSObjectProtocol for TopBar {}

    impl TopBar {
        // The buttons' actions (the toolbar items' target is the bar).
        #[unsafe(method(goBack:))]
        fn go_back(&self, _sender: Option<&AnyObject>) {
            self.go(|view| {
                // SAFETY: main thread.
                unsafe { view.goBack() };
            });
        }

        #[unsafe(method(goForward:))]
        fn go_forward(&self, _sender: Option<&AnyObject>) {
            self.go(|view| {
                // SAFETY: main thread.
                unsafe { view.goForward() };
            });
        }

        #[unsafe(method(reloadOrStop:))]
        fn reload_or_stop(&self, _sender: Option<&AnyObject>) {
            self.go(|view| {
                // SAFETY: main thread; plain WebKit calls.
                unsafe {
                    if view.isLoading() {
                        view.stopLoading();
                    } else {
                        view.reload();
                    }
                }
            });
        }
    }

    // SAFETY: the signatures match `toolbar:itemForItemIdentifier:willBeInsertedIntoToolbar:`,
    // `toolbarDefaultItemIdentifiers:` and `toolbarAllowedItemIdentifiers:`.
    unsafe impl NSToolbarDelegate for TopBar {
        #[unsafe(method_id(toolbar:itemForItemIdentifier:willBeInsertedIntoToolbar:))]
        fn item_for(
            &self,
            _toolbar: &NSToolbar,
            identifier: &NSString,
            will_be_inserted: bool,
        ) -> Option<Retained<NSToolbarItem>> {
            self.make_item(&identifier.to_string(), will_be_inserted)
        }

        #[unsafe(method_id(toolbarDefaultItemIdentifiers:))]
        fn default_items(&self, _toolbar: &NSToolbar) -> Retained<NSArray<NSString>> {
            identifiers(&bar_items(self.ivars().address_bar.get()))
        }

        #[unsafe(method_id(toolbarAllowedItemIdentifiers:))]
        fn allowed_items(&self, _toolbar: &NSToolbar) -> Retained<NSArray<NSString>> {
            identifiers(&bar_items(true))
        }
    }

    // SAFETY: the signature matches `control:textView:doCommandBySelector:`.
    unsafe impl NSControlTextEditingDelegate for TopBar {
        // Enter opens what was typed; Escape puts the page's address back. Both hand the
        // keyboard back to the page.
        #[unsafe(method(control:textView:doCommandBySelector:))]
        fn do_command(&self, _control: &NSControl, text_view: &NSTextView, command: Sel) -> bool {
            if command == sel!(insertNewline:) {
                self.enter(&text_view.string().to_string());
                true
            } else if command == sel!(cancelOperation:) {
                self.escape();
                true
            } else {
                false
            }
        }
    }

    // SAFETY: NSTextFieldDelegate's methods are all optional; the bar uses none of its own.
    unsafe impl NSTextFieldDelegate for TopBar {}
);

impl TopBar {
    /// A bar for `view` (whose page is kept by `page`), looking as `look` says; not yet in a
    /// window (see `install`).
    pub(super) fn new(
        mtm: MainThreadMarker,
        view: &WKWebView,
        page: &BrowserDelegate,
        look: &BarLook,
    ) -> Retained<TopBar> {
        let field = NSTextField::textFieldWithString(&NSString::new(), mtm);
        field.setPlaceholderString(Some(&NSString::from_str(PLACEHOLDER)));
        field.setBezeled(true);
        field.setBezelStyle(NSTextFieldBezelStyle::RoundedBezel);
        field.setUsesSingleLineMode(true);
        // The bar sizes the field from these: at least the minimum, as wide as there is room.
        field.setTranslatesAutoresizingMaskIntoConstraints(false);
        field
            .widthAnchor()
            .constraintGreaterThanOrEqualToConstant(FIELD_MIN_WIDTH)
            .setActive(true);
        field
            .widthAnchor()
            .constraintLessThanOrEqualToConstant(FIELD_MAX_WIDTH)
            .setActive(true);

        let line = NSBox::initWithFrame(
            mtm.alloc(),
            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(0.0, LINE_HEIGHT)),
        );
        line.setBoxType(NSBoxType::Custom);
        line.setTitlePosition(NSTitlePosition::NoTitle);
        line.setBorderWidth(0.0);
        line.setContentViewMargins(NSSize::new(0.0, 0.0));
        // Stays on the top edge as the window resizes; `refresh` sets its width.
        line.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
        line.setHidden(true);

        let this = Self::alloc(mtm).set_ivars(BarState {
            view: Weak::from(view),
            page: Weak::from(page),
            field,
            line,
            back: RefCell::new(None),
            forward: RefCell::new(None),
            reload: RefCell::new(None),
            reload_shows: Cell::new(None),
            address_bar: Cell::new(look.address_bar),
            error_until: Cell::new(None),
        });
        // SAFETY: NSObject's `init` has this signature.
        let bar: Retained<TopBar> = unsafe { msg_send![super(this), init] };
        // SAFETY: main thread; the field holds its delegate weakly, and `detach` clears it.
        unsafe {
            bar.ivars()
                .field
                .setDelegate(Some(ProtocolObject::from_ref(&*bar)))
        };
        bar
    }

    /// The loading line, for the window's content view to hold over the page.
    pub(super) fn line(&self) -> &NSView {
        &self.ivars().line
    }

    /// The address field (whether or not the bar shows it).
    pub(super) fn field(&self) -> &NSTextField {
        &self.ivars().field
    }

    /// Give `window` the bar: a compact toolbar of its own (its title shows only when the
    /// address field doesn't), looking as `look` says.
    pub(super) fn install(&self, window: &NSWindow, look: &BarLook) {
        let mtm = self.mtm();
        let number = TOOLBARS.with(|n| {
            n.set(n.get() + 1);
            n.get()
        });
        let identifier = NSString::from_str(&format!("fuxx.browser.{number}"));
        let toolbar = NSToolbar::initWithIdentifier(mtm.alloc(), &identifier);
        toolbar.setDelegate(Some(ProtocolObject::from_ref(self)));
        toolbar.setDisplayMode(NSToolbarDisplayMode::IconOnly);
        toolbar.setAllowsUserCustomization(false);
        toolbar.setAutosavesConfiguration(false);
        window.setToolbarStyle(NSWindowToolbarStyle::UnifiedCompact);
        window.setToolbar(Some(&toolbar));
        self.set_look(window, look);
        self.refresh();
    }

    /// Follow `look`: the window's light or dark, the line's colour, and the address field
    /// added or taken away (the title shows in its place).
    pub(super) fn set_look(&self, window: &NSWindow, look: &BarLook) {
        // SAFETY: AppKit's own appearance names.
        let name = unsafe {
            if look.dark {
                NSAppearanceNameDarkAqua
            } else {
                NSAppearanceNameAqua
            }
        };
        window.setAppearance(NSAppearance::appearanceNamed(name).as_deref());
        let accent = look.accent;
        let channel = |c: u8| f64::from(c) / 255.0;
        self.ivars()
            .line
            .setFillColor(&NSColor::colorWithSRGBRed_green_blue_alpha(
                channel(accent.r),
                channel(accent.g),
                channel(accent.b),
                1.0,
            ));
        self.show_address_field(window, look.address_bar);
    }

    /// Put the address field in the bar or take it out.
    fn show_address_field(&self, window: &NSWindow, show: bool) {
        window.setTitleVisibility(if show {
            NSWindowTitleVisibility::Hidden
        } else {
            NSWindowTitleVisibility::Visible
        });
        let Some(toolbar) = window.toolbar() else {
            return;
        };
        self.ivars().address_bar.set(show);
        let at = item_index(&toolbar, ADDRESS);
        match (show, at) {
            (true, None) => {
                // Where the bar's order puts it: after the buttons, before the mark.
                let at = bar_items(true).iter().position(|id| *id == ADDRESS);
                let at = at.unwrap_or(0) as isize;
                toolbar.insertItemWithItemIdentifier_atIndex(&NSString::from_str(ADDRESS), at);
                self.refresh();
            }
            (false, Some(at)) => {
                // Typing in it goes back to the page first.
                if self.editing() {
                    self.page_takes_keyboard();
                }
                toolbar.removeItemAtIndex(at as isize);
            }
            _ => {}
        }
    }

    /// Bring the buttons, the field and the loading line up to date with the page, and the
    /// page's light or dark with macOS's.
    pub(super) fn refresh(&self) {
        let Some(view) = self.ivars().view.load() else {
            return;
        };
        // Followed on each refresh, which the browser check runs at least every 500 ms.
        super::follow_system_appearance(&view, self.mtm());
        // SAFETY: main thread; plain property reads.
        let (url, loading, progress, can_back, can_forward) = unsafe {
            (
                view.URL()
                    .and_then(|u| u.absoluteString())
                    .map(|u| u.to_string())
                    .unwrap_or_default(),
                view.isLoading(),
                view.estimatedProgress(),
                view.canGoBack(),
                view.canGoForward(),
            )
        };
        let shown = buttons(Nav {
            can_back,
            can_forward,
            loading,
        });
        let state = self.ivars();
        if let Some(back) = &*state.back.borrow() {
            back.setEnabled(shown.back);
        }
        if let Some(forward) = &*state.forward.borrow() {
            forward.setEnabled(shown.forward);
        }
        if state.reload_shows.get() != Some(shown.reload)
            && let Some(reload) = &*state.reload.borrow()
        {
            let (symbol, label, tip) = reload_face(shown.reload);
            reload.setImage(symbol_image(symbol, label).as_deref());
            reload.setLabel(&NSString::from_str(label));
            reload.setToolTip(Some(&NSString::from_str(tip)));
            state.reload_shows.set(Some(shown.reload));
        }

        if state
            .error_until
            .get()
            .is_some_and(|until| Instant::now() >= until)
        {
            self.clear_error();
        }
        let requested = state
            .page
            .load()
            .and_then(|page| page.ivars().requested.borrow().clone());
        if let Some(text) = field_text(&url, requested.as_deref(), self.editing()) {
            let text = NSString::from_str(&text);
            if state.field.stringValue() != text {
                state.field.setStringValue(&text);
            }
        }

        // SAFETY: main thread; the line's parent is the window's content view.
        let Some(content) = (unsafe { state.line.superview() }) else {
            return;
        };
        let bounds = content.bounds();
        match loading_line(loading, progress, bounds.size.width) {
            Some(width) => {
                state.line.setFrame(NSRect::new(
                    NSPoint::new(0.0, bounds.size.height - LINE_HEIGHT),
                    NSSize::new(width, LINE_HEIGHT),
                ));
                state.line.setHidden(false);
            }
            None => state.line.setHidden(true),
        }
    }

    /// Let go of the toolbar, the field and the buttons before the `Browser` goes (they hold the
    /// bar weakly, and may outlive it).
    pub(super) fn detach(&self, window: &NSWindow) {
        let state = self.ivars();
        // SAFETY: main thread; clearing the weak links to the bar.
        unsafe {
            state.field.setDelegate(None);
            for item in [&state.back, &state.forward, &state.reload] {
                if let Some(item) = item.take() {
                    item.setTarget(None);
                }
            }
        }
        if let Some(toolbar) = window.toolbar() {
            toolbar.setDelegate(None);
        }
        window.setToolbar(None);
    }

    /// A new toolbar item for `identifier` (the toolbar asks for each one it shows). Only one
    /// `going_in` (to be put in the bar) is the bar's: it holds the field, is kept for `refresh`
    /// and acts. Another (a customisation palette's copy) only looks the same.
    fn make_item(&self, identifier: &str, going_in: bool) -> Option<Retained<NSToolbarItem>> {
        let mtm = self.mtm();
        let item =
            NSToolbarItem::initWithItemIdentifier(mtm.alloc(), &NSString::from_str(identifier));
        let state = self.ivars();
        let (symbol, label, tip, action, slot) = match identifier {
            BACK => (
                "chevron.left",
                "Back",
                "Go back",
                sel!(goBack:),
                &state.back,
            ),
            FORWARD => (
                "chevron.right",
                "Forward",
                "Go forward",
                sel!(goForward:),
                &state.forward,
            ),
            RELOAD => {
                let (symbol, label, tip) = reload_face(ReloadButton::Reload);
                (symbol, label, tip, sel!(reloadOrStop:), &state.reload)
            }
            MARK => {
                // A picture, not a button: no action, no bezel.
                item.setLabel(&NSString::from_str("fuxx"));
                item.setBordered(false);
                let picture = NSImageView::imageViewWithImage(&mark_image(), mtm);
                // Not a drop target either: an editable picture would take a dragged-in image.
                picture.setEditable(false);
                item.setView(Some(&picture));
                return Some(item);
            }
            ADDRESS => {
                item.setLabel(&NSString::from_str("Address"));
                if going_in {
                    item.setView(Some(&state.field));
                    item.setVisibilityPriority(NSToolbarItemVisibilityPriorityHigh);
                }
                return Some(item);
            }
            _ => return None,
        };
        item.setLabel(&NSString::from_str(label));
        item.setToolTip(Some(&NSString::from_str(tip)));
        item.setImage(symbol_image(symbol, label).as_deref());
        item.setBordered(true);
        // At the leading edge, beside the window's buttons, as in Safari.
        item.setNavigational(true);
        // The bar says when a button works (`refresh`), not AppKit's validation.
        item.setAutovalidates(false);
        item.setEnabled(false);
        if !going_in {
            return Some(item);
        }
        if identifier == RELOAD {
            state.reload_shows.set(Some(ReloadButton::Reload));
        }
        let target: &AnyObject = self.as_ref();
        // SAFETY: main thread; the item holds its target weakly, and `detach` clears it.
        unsafe {
            item.setTarget(Some(target));
            item.setAction(Some(action));
        }
        slot.replace(Some(item.clone()));
        Some(item)
    }

    /// A button's action: on the page, then the bar follows at once.
    fn go(&self, action: impl FnOnce(&WKWebView)) {
        let Some(view) = self.ivars().view.load() else {
            return;
        };
        if let Some(page) = self.ivars().page.load() {
            page.ivars().failure.take();
        }
        action(&view);
        self.refresh();
    }

    /// Enter in the field: open what was typed, or say in the field why it can't be.
    fn enter(&self, typed: &str) {
        match entered_address(typed) {
            Ok(url) => {
                let (Some(view), Some(page)) = (self.ivars().view.load(), self.ivars().page.load())
                else {
                    return;
                };
                self.clear_error();
                navigate(&view, &page, &url);
                self.page_takes_keyboard();
                self.ivars().field.setStringValue(&NSString::from_str(&url));
                self.refresh();
            }
            Err(why) => self.show_error(&why),
        }
    }

    /// Escape in the field: the page's address back, and the keyboard back to the page.
    fn escape(&self) {
        self.clear_error();
        self.page_takes_keyboard();
        self.refresh();
    }

    /// Whether the person is typing in the field.
    fn editing(&self) -> bool {
        self.ivars().field.currentEditor().is_some()
    }

    /// Give the keyboard back to the page (ending any typing in the field).
    fn page_takes_keyboard(&self) {
        let Some(view) = self.ivars().view.load() else {
            return;
        };
        if let Some(window) = view.window() {
            let view: &NSResponder = &view;
            window.makeFirstResponder(Some(view));
        }
    }

    /// The field's text in red, and why as its tooltip, for a moment (`refresh` ends it).
    fn show_error(&self, why: &str) {
        let field = &self.ivars().field;
        let red = NSColor::systemRedColor();
        field.setTextColor(Some(&red));
        if let Some(editor) = field.currentEditor() {
            editor.setTextColor(Some(&red));
        }
        field.setToolTip(Some(&NSString::from_str(why)));
        self.ivars()
            .error_until
            .set(Some(Instant::now() + ERROR_SHOWN));
    }

    /// The field back to its own colours, without a tooltip.
    fn clear_error(&self) {
        if self.ivars().error_until.take().is_none() {
            return;
        }
        let field = &self.ivars().field;
        let ink = NSColor::controlTextColor();
        field.setTextColor(Some(&ink));
        if let Some(editor) = field.currentEditor() {
            editor.setTextColor(Some(&ink));
        }
        field.setToolTip(None);
    }
}

/// fuxx's mark: the emoji drawn centred in a square the toolbar's icon size. Drawn by a handler,
/// so AppKit draws it afresh at each screen's scale (sharp on Retina), the same in light and dark.
fn mark_image() -> Retained<NSImage> {
    let draw = RcBlock::new(|rect: NSRect| -> Bool {
        let font = NSFont::systemFontOfSize(MARK_FONT_SIZE);
        // SAFETY: AppKit's own attribute name.
        let key = unsafe { NSFontAttributeName };
        let attributes: Retained<NSDictionary<NSString, AnyObject>> =
            NSDictionary::from_slices(&[key], &[font.as_ref()]);
        let text = NSString::from_str(MARK_TEXT);
        // SAFETY: the font attribute takes an NSFont.
        unsafe {
            let size = text.sizeWithAttributes(Some(&attributes));
            let at = NSPoint::new(
                rect.origin.x + (rect.size.width - size.width) / 2.0,
                rect.origin.y + (rect.size.height - size.height) / 2.0,
            );
            text.drawAtPoint_withAttributes(at, Some(&attributes));
        }
        Bool::YES
    });
    let mark = NSImage::imageWithSize_flipped_drawingHandler(
        NSSize::new(MARK_SIZE, MARK_SIZE),
        false,
        &draw,
    );
    mark.setAccessibilityDescription(Some(&NSString::from_str("fuxx")));
    mark
}

/// An SF Symbol, described as `label` for VoiceOver.
fn symbol_image(symbol: &str, label: &str) -> Option<Retained<NSImage>> {
    NSImage::imageWithSystemSymbolName_accessibilityDescription(
        &NSString::from_str(symbol),
        Some(&NSString::from_str(label)),
    )
}

/// Toolbar item identifiers, as AppKit takes them.
fn identifiers(ids: &[&str]) -> Retained<NSArray<NSString>> {
    let ids: Vec<Retained<NSString>> = ids.iter().map(|id| NSString::from_str(id)).collect();
    NSArray::from_retained_slice(&ids)
}

/// Where the item `identifier` sits in `toolbar`, if it is there.
fn item_index(toolbar: &NSToolbar, identifier: &str) -> Option<usize> {
    let identifier = NSString::from_str(identifier);
    toolbar
        .items()
        .iter()
        .position(|item| item.itemIdentifier() == identifier)
}

/// ⌘L: the keyboard to the address field of `window`'s bar, all of it selected (a text field
/// selects what it holds when it takes the keyboard). `false` when the bar shows no field.
pub(super) fn focus_address(window: &NSWindow) -> bool {
    let Some(toolbar) = window.toolbar() else {
        return false;
    };
    let Some(at) = item_index(&toolbar, ADDRESS) else {
        return false;
    };
    match toolbar.items().objectAtIndex(at).view() {
        Some(field) => window.makeFirstResponder(Some(&field)),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nav(can_back: bool, can_forward: bool, loading: bool) -> Nav {
        Nav {
            can_back,
            can_forward,
            loading,
        }
    }

    #[test]
    fn buttons_follow_the_page() {
        assert_eq!(
            buttons(nav(false, false, false)),
            Buttons {
                back: false,
                forward: false,
                reload: ReloadButton::Reload
            }
        );
        assert_eq!(
            buttons(nav(true, false, false)),
            Buttons {
                back: true,
                forward: false,
                reload: ReloadButton::Reload
            }
        );
        assert_eq!(
            buttons(nav(true, true, true)),
            Buttons {
                back: true,
                forward: true,
                reload: ReloadButton::Stop
            }
        );
        assert_eq!(
            buttons(nav(false, true, true)),
            Buttons {
                back: false,
                forward: true,
                reload: ReloadButton::Stop
            }
        );
    }

    #[test]
    fn the_field_shows_the_page_unless_it_is_being_edited() {
        let url = "http://127.0.0.1:3000/x";
        assert_eq!(field_text(url, None, false), Some(url.to_string()));
        // A page that came from a redirect or a link: the page's own address, not the asked one.
        assert_eq!(
            field_text(url, Some("http://127.0.0.1:3000/"), false),
            Some(url.to_string())
        );
        // The person's typing stays.
        assert_eq!(field_text(url, None, true), None);
        assert_eq!(field_text("", Some(url), true), None);
        // No page yet (or a blank one): what was asked for, if it never came (a failed load),
        // else nothing, so the placeholder shows.
        assert_eq!(field_text("", Some(url), false), Some(url.to_string()));
        assert_eq!(
            field_text("about:blank", Some(url), false),
            Some(url.to_string())
        );
        assert_eq!(field_text("", None, false), Some(String::new()));
        assert_eq!(field_text("about:blank", None, false), Some(String::new()));
    }

    #[test]
    fn the_mark_comes_last_with_or_without_the_address_field() {
        assert_eq!(bar_items(true), [BACK, FORWARD, RELOAD, ADDRESS, MARK]);
        assert_eq!(bar_items(false), [BACK, FORWARD, RELOAD, MARK]);
    }

    #[test]
    fn the_loading_line_shows_progress_only_while_loading() {
        assert_eq!(loading_line(false, 0.5, 1100.0), None);
        assert_eq!(loading_line(false, 1.0, 1100.0), None);
        assert_eq!(loading_line(true, 0.5, 1100.0), Some(550.0));
        assert_eq!(loading_line(true, 0.1, 1000.0), Some(100.0));
        // WebKit's estimate stays within 0 and 1; the line within the window anyway.
        assert_eq!(loading_line(true, 1.5, 1000.0), Some(1000.0));
        assert_eq!(loading_line(true, -0.5, 1000.0), Some(0.0));
    }

    #[test]
    fn the_field_opens_only_what_agents_may() {
        assert_eq!(
            entered_address(" 127.0.0.1:8080/x "),
            Ok("http://127.0.0.1:8080/x".to_string())
        );
        assert_eq!(
            entered_address("example.com"),
            Ok("https://example.com".to_string())
        );
        for refused in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "data:text/html,hi",
            "",
        ] {
            assert!(entered_address(refused).is_err(), "{refused}");
        }
    }
}
