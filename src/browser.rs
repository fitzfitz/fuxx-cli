//! The agent browser (M10c): a `WKWebView` in its own `NSWindow`, driven through objc2.
//!
//! A [`Browser`] is one session's browser window. Every WebKit and AppKit call runs on the main
//! thread (a `Browser` can't leave it). WebKit answers scripts and screenshots through
//! completion blocks, also on the main thread; each answer goes into a channel the caller
//! awaits from a GPUI task, so nothing ever blocks the run loop waiting for a page.
//!
//! The hidden `fuxx __browser-probe` self-test lives in `probe`.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::time::Duration;

use block2::RcBlock;
use futures::channel::oneshot;
use futures::future::{self, Either};
use gpui::{App, BackgroundExecutor};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSApplication, NSBackingStoreType, NSBitmapImageFileType, NSBitmapImageRep, NSEvent,
    NSEventModifierFlags, NSImage, NSResponder, NSWindow, NSWindowDelegate, NSWindowStyleMask,
};
use objc2_foundation::{
    NSDictionary, NSError, NSNotification, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize,
    NSString, NSURL, NSURLErrorCancelled, NSURLRequest,
};
use objc2_web_kit::{
    WKErrorCode, WKNavigation, WKNavigationAction, WKNavigationDelegate, WKUIDelegate,
    WKUserScript, WKUserScriptInjectionTime, WKWebView, WKWebViewConfiguration, WKWebsiteDataStore,
    WKWindowFeatures,
};
use serde_json::Value;

use crate::browser_js;

mod probe;

pub use probe::run_probe;

/// A browser window's content size, in points.
const WINDOW_SIZE: NSSize = NSSize::new(1100.0, 800.0);
/// What a pending script or screenshot answers when its browser closes first (and what a
/// command answers when its browser is gone before it could finish).
pub(crate) const CLOSED: &str = "browser closed";
/// Added to the error when a page refuses `eval` (statements need it; one expression doesn't).
const EVAL_REFUSED_HINT: &str =
    "this page allows only an expression here: wrap statements in (() => { … })()";

/// What a browser shows. `url` and `title` are empty until there is a page with them; once
/// nothing is loading, `error` is `the page failed to load: <reason>` if the last navigation
/// failed (WebKit's reason, or the address when a `load` never left the blank page).
#[derive(Debug, Clone, PartialEq)]
pub struct PageState {
    pub url: String,
    pub title: String,
    pub loading: bool,
    pub error: Option<String>,
}

/// One session's browser: a window holding a web view. Main thread only (it isn't `Send`).
/// Dropping it (or `close`) closes the window, and every script or screenshot still waiting
/// answers `browser closed`.
pub struct Browser {
    window: Retained<NSWindow>,
    view: Retained<WKWebView>,
    /// The window's and the web view's delegate (they hold it weakly, so it lives here).
    delegate: Retained<BrowserDelegate>,
    /// The address the last `load` asked for: a page that never came is a failed load.
    requested: RefCell<Option<String>>,
    /// Scripts and screenshots WebKit hasn't answered yet.
    pending: RefCell<Vec<Weak<dyn Unanswered>>>,
}

impl Browser {
    /// A new browser window titled `title`, not yet shown (see `bring_to_front`): 1100 × 800
    /// points, cascaded from the key (or else main) window, centred when there is none. Pages get a `window.open` that
    /// loads in this window; cookies and logins live in the one shared, persistent data store;
    /// Safari's Web Inspector can attach where macOS allows it (13.3 and later).
    pub fn open(title: &str, _cx: &mut App) -> Browser {
        let mtm = MainThreadMarker::new().expect("GPUI's App lives on the main thread");
        let window = new_window(
            mtm,
            NSWindowStyleMask::Titled
                | NSWindowStyleMask::Closable
                | NSWindowStyleMask::Resizable
                | NSWindowStyleMask::Miniaturizable,
        );
        let app = NSApplication::sharedApplication(mtm);
        match app.keyWindow().or_else(|| app.mainWindow()) {
            Some(beside) => {
                let frame = beside.frame();
                let top_left = NSPoint::new(frame.origin.x, frame.origin.y + frame.size.height);
                // The first call puts the window on that corner and answers the next cascade
                // step (down and to the right), where the second puts it.
                let next = window.cascadeTopLeftFromPoint(top_left);
                window.cascadeTopLeftFromPoint(next);
            }
            None => window.center(),
        }
        let browser = Browser::in_window(mtm, window);
        browser.set_title(title);
        browser
    }

    /// A browser in a borderless window that is never shown (the probe's).
    fn offscreen(mtm: MainThreadMarker) -> Browser {
        Browser::in_window(mtm, new_window(mtm, NSWindowStyleMask::Borderless))
    }

    /// Fill `window` with a new web view and take both on.
    fn in_window(mtm: MainThreadMarker, window: Retained<NSWindow>) -> Browser {
        let delegate = BrowserDelegate::new(mtm);
        // SAFETY: main thread; plain WebKit set-up with valid arguments. The delegate is kept
        // alive by the `Browser`, which clears both delegate links before it goes.
        let view = unsafe {
            let config = WKWebViewConfiguration::new(mtm);
            config.setWebsiteDataStore(&WKWebsiteDataStore::defaultDataStore(mtm));
            let shim = WKUserScript::initWithSource_injectionTime_forMainFrameOnly(
                mtm.alloc(),
                &NSString::from_str(browser_js::window_open_shim()),
                WKUserScriptInjectionTime::AtDocumentStart,
                true,
            );
            config.userContentController().addUserScript(&shim);
            let frame = NSRect::new(NSPoint::new(0.0, 0.0), WINDOW_SIZE);
            let view = WKWebView::initWithFrame_configuration(mtm.alloc(), frame, &config);
            if view.respondsToSelector(sel!(setInspectable:)) {
                view.setInspectable(true);
            }
            view.setUIDelegate(Some(ProtocolObject::from_ref(&*delegate)));
            view.setNavigationDelegate(Some(ProtocolObject::from_ref(&*delegate)));
            view
        };
        window.setContentView(Some(&view));
        window.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        Browser {
            window,
            view,
            delegate,
            requested: RefCell::new(None),
            pending: RefCell::new(Vec::new()),
        }
    }

    /// Go to `url` (already normalised to http or https). Returns at once; `state` tells when
    /// the page has loaded, or that it failed to.
    pub fn load(&self, url: &str) {
        self.requested.replace(Some(url.to_string()));
        self.delegate.ivars().failure.take();
        match NSURL::URLWithString(&NSString::from_str(url)) {
            // SAFETY: main thread; the view copies the request.
            Some(address) => unsafe {
                self.view
                    .loadRequest(&NSURLRequest::requestWithURL(&address));
            },
            // Nothing loads, and whatever page was showing stays: say so.
            None => {
                let failure = Some("not a valid address".to_string());
                self.delegate.ivars().failure.replace(failure);
            }
        }
    }

    /// Run `js` in the page and answer with its value as JSON (`undefined` as `null`; what JSON
    /// can't hold, such as functions and DOM nodes, as `null` or `{}`), or what it threw as
    /// `the script failed: <error>`. It runs as one expression (on any page); when that doesn't
    /// parse, as statements through `eval`, which some pages' Content-Security-Policy forbids.
    /// The answer never times out by itself: the caller limits the wait (see [`within`]).
    pub fn eval(&self, js: &str) -> oneshot::Receiver<Result<Value, String>> {
        let (tx, rx) = oneshot::channel();
        let slot = self.pend(tx);
        let statements = browser_js::eval_statements_js(js);
        // Weak: a block WebKit never calls must not keep the view alive.
        let view = objc2::rc::Weak::from_retained(&self.view);
        let block = RcBlock::new(move |result: *mut AnyObject, error: *mut NSError| {
            // SAFETY: WebKit passes a valid object or nil for each.
            match unsafe { (result.as_ref(), error.as_ref()) } {
                // The expression form didn't parse, so nothing ran: try it as statements. Any
                // other failure may come after the code ran, and is never run again.
                (_, Some(error)) if is_syntax_error(error) => match view.load() {
                    Some(view) => evaluate(&view, &statements, slot.clone()),
                    None => answer(&slot, Err(CLOSED.to_string())),
                },
                (result, error) => answer(&slot, script_answer(result, error)),
            }
        });
        let script = NSString::from_str(&browser_js::eval_expression_js(js));
        // SAFETY: main thread; WebKit copies the block and calls it once, on the main thread.
        unsafe {
            self.view
                .evaluateJavaScript_completionHandler(&script, Some(&block))
        };
        rx
    }

    /// Run one of fuxx's own page scripts (`browser_js`: snapshot, click, fill, press, read,
    /// wait), answered like `eval`. It is one expression and runs exactly once: never retried as
    /// statements, so a click can't happen twice.
    pub fn run_script(&self, script: &str) -> oneshot::Receiver<Result<Value, String>> {
        let (tx, rx) = oneshot::channel();
        let script = browser_js::eval_expression_js(script);
        evaluate(&self.view, &script, self.pend(tx));
        rx
    }

    /// Where the browser is and whether it is still loading. Polled: once loading stops, `error`
    /// tells whether the last navigation failed (connection refused, unknown host and so on).
    pub fn state(&self) -> PageState {
        // SAFETY: main thread; plain property reads.
        let (url, title, loading) = unsafe {
            (
                self.view.URL().and_then(|u| u.absoluteString()),
                self.view.title(),
                self.view.isLoading(),
            )
        };
        let url = url.map(|u| u.to_string()).unwrap_or_default();
        let error = load_error(
            self.requested.borrow().as_deref(),
            &url,
            loading,
            self.delegate.ivars().failure.borrow().as_deref(),
        );
        PageState {
            url,
            title: title.map(|t| t.to_string()).unwrap_or_default(),
            loading,
            error,
        }
    }

    /// A PNG of the visible part of the page, at the window's backing scale (2× on Retina).
    pub fn screenshot(&self) -> oneshot::Receiver<Result<Vec<u8>, String>> {
        let (tx, rx) = oneshot::channel();
        let slot = self.pend(tx);
        let block = RcBlock::new(move |image: *mut NSImage, error: *mut NSError| {
            // SAFETY: WebKit passes a valid object or nil for each.
            let shot = match unsafe { (image.as_ref(), error.as_ref()) } {
                (_, Some(error)) => Err(format!(
                    "the screenshot failed: {}",
                    error.localizedDescription()
                )),
                (Some(image), None) => png_bytes(image),
                (None, None) => Err("the screenshot came back empty".to_string()),
            };
            answer(&slot, shot);
        });
        // SAFETY: main thread; no configuration means the whole visible view.
        unsafe {
            self.view
                .takeSnapshotWithConfiguration_completionHandler(None, &block)
        };
        rx
    }

    /// Go back a page (nothing happens on the first page).
    pub fn back(&self) {
        self.delegate.ivars().failure.take();
        // SAFETY: main thread.
        unsafe { self.view.goBack() };
    }

    /// Load the page again.
    pub fn reload(&self) {
        self.delegate.ivars().failure.take();
        // SAFETY: main thread.
        unsafe { self.view.reload() };
    }

    /// The window's title.
    pub fn set_title(&self, title: &str) {
        self.window.setTitle(&NSString::from_str(title));
    }

    /// Show the window (again, if the user closed or minimised it) in front of the others.
    /// `take_keyboard` makes it the key window (the person asked for it); without, it only comes
    /// forward, so what they are typing in a terminal keeps going there. Not
    /// `orderFrontRegardless`: an agent's browser may sit behind other apps rather than jump over
    /// whatever the person is using.
    pub fn bring_to_front(&self, take_keyboard: bool) {
        match front_order(take_keyboard) {
            FrontOrder::TakeKeyboard => self.window.makeKeyAndOrderFront(None),
            FrontOrder::KeepKeyboard => self.window.orderFront(None),
        }
        self.delegate.ivars().closed.set(false);
    }

    /// Whether the user has closed the window (the browser itself still works until dropped).
    pub fn is_closed(&self) -> bool {
        self.delegate.ivars().closed.get()
    }

    /// Close the window and stop the page; what is still waiting answers `browser closed`.
    pub fn close(self) {
        drop(self);
    }

    /// Keep `tx` where both its completion block and `close` can answer it, first come first
    /// served.
    fn pend<T: 'static>(&self, tx: oneshot::Sender<Result<T, String>>) -> Slot<T> {
        let slot: Slot<T> = Rc::new(Cell::new(Some(tx)));
        let mut pending = self.pending.borrow_mut();
        // Answered requests are gone (WebKit released their blocks): forget them.
        pending.retain(|request| request.strong_count() > 0);
        let request: Weak<dyn Unanswered> = Rc::downgrade(&slot) as Weak<_>;
        pending.push(request);
        slot
    }
}

impl Drop for Browser {
    fn drop(&mut self) {
        for request in self.pending.take() {
            if let Some(request) = request.upgrade() {
                request.give_up();
            }
        }
        // SAFETY: main thread (a `Browser` never leaves it).
        unsafe {
            self.view.stopLoading();
            self.view.setUIDelegate(None);
            self.view.setNavigationDelegate(None);
        }
        self.window.setDelegate(None);
        self.window.close();
    }
}

/// Where a script's or screenshot's answer goes: taken by whichever comes first, WebKit's
/// completion block or the browser closing.
type Slot<T> = Rc<Cell<Option<oneshot::Sender<Result<T, String>>>>>;

/// A request still waiting for WebKit, whatever its answer's type.
trait Unanswered {
    /// Answer `browser closed` (unless WebKit answered first).
    fn give_up(&self);
}

impl<T> Unanswered for Cell<Option<oneshot::Sender<Result<T, String>>>> {
    fn give_up(&self) {
        if let Some(tx) = self.take() {
            let _ = tx.send(Err(CLOSED.to_string()));
        }
    }
}

/// An answer from [`Browser::eval`] or [`Browser::screenshot`], or `the page didn't answer in
/// <N> s` once `limit` has passed. WebKit can't take a script back, so a late answer is dropped.
pub async fn within<T>(
    answer: oneshot::Receiver<Result<T, String>>,
    limit: Duration,
    executor: &BackgroundExecutor,
) -> Result<T, String> {
    answer_before(answer, executor.timer(limit), limit).await
}

/// `within`, with the timer that ends the wait passed in (`limit` is what it stands for).
async fn answer_before<T>(
    answer: oneshot::Receiver<Result<T, String>>,
    timer: impl Future<Output = ()> + Unpin,
    limit: Duration,
) -> Result<T, String> {
    match future::select(answer, timer).await {
        Either::Left((Ok(answer), _)) => answer,
        // WebKit dropped the request without answering: its web view is gone.
        Either::Left((Err(_), _)) => Err(CLOSED.to_string()),
        Either::Right(_) => Err(didnt_answer(limit)),
    }
}

/// The error for a page that took longer than `limit`.
pub fn didnt_answer(limit: Duration) -> String {
    format!("the page didn't answer in {} s", limit.as_secs())
}

/// Run `script` (an eval script) in `view`; its answer goes to `slot`.
fn evaluate(view: &WKWebView, script: &str, slot: Slot<Value>) {
    let block = RcBlock::new(move |result: *mut AnyObject, error: *mut NSError| {
        // SAFETY: WebKit passes a valid object or nil for each.
        let (result, error) = unsafe { (result.as_ref(), error.as_ref()) };
        answer(&slot, script_answer(result, error));
    });
    // SAFETY: main thread; WebKit copies the block and calls it once, on the main thread.
    unsafe { view.evaluateJavaScript_completionHandler(&NSString::from_str(script), Some(&block)) };
}

/// What WebKit's answer to an eval script means.
fn script_answer(result: Option<&AnyObject>, error: Option<&NSError>) -> Result<Value, String> {
    match (result, error) {
        (_, Some(error)) => Err(format!(
            "the script failed: {}",
            exception_message(error).unwrap_or_else(|| error.localizedDescription().to_string())
        )),
        (Some(text), None) => match text.downcast_ref::<NSString>() {
            Some(text) => parse_answer(&text.to_string()).map_err(|e| {
                if e.starts_with("the script failed: EvalError") {
                    format!("{} ({EVAL_REFUSED_HINT})", e.trim_end())
                } else {
                    e
                }
            }),
            None => Err("the page's answer isn't text".to_string()),
        },
        (None, None) => Err("the page's answer is empty".to_string()),
    }
}

/// The JavaScript exception an evaluation error carries (`SyntaxError: …`), if it carries one.
fn exception_message(error: &NSError) -> Option<String> {
    let key = NSString::from_str("WKJavaScriptExceptionMessage");
    let message = error.userInfo().objectForKey(&key)?;
    message.downcast::<NSString>().ok().map(|m| m.to_string())
}

/// Whether WebKit refused a script because it didn't parse (so none of it ran).
fn is_syntax_error(error: &NSError) -> bool {
    did_not_parse(
        &error.domain().to_string(),
        error.code(),
        exception_message(error).as_deref(),
    )
}

/// `is_syntax_error` on the error's parts: a JavaScript exception in WebKit's domain whose
/// message is a `SyntaxError`.
fn did_not_parse(domain: &str, code: isize, message: Option<&str>) -> bool {
    domain == "WKErrorDomain"
        && code == WKErrorCode::JavaScriptExceptionOccurred.0
        && message.is_some_and(|m| m.starts_with("SyntaxError"))
}

/// Give `slot` its answer, unless it has had one (or the browser closed first).
fn answer<T>(slot: &Slot<T>, answer: Result<T, String>) {
    if let Some(tx) = slot.take() {
        let _ = tx.send(answer);
    }
}

/// How `bring_to_front` orders the window.
#[derive(Debug, PartialEq)]
enum FrontOrder {
    TakeKeyboard,
    KeepKeyboard,
}

fn front_order(take_keyboard: bool) -> FrontOrder {
    if take_keyboard {
        FrontOrder::TakeKeyboard
    } else {
        FrontOrder::KeepKeyboard
    }
}

/// A shortcut the browser window answers itself.
#[derive(Debug, PartialEq)]
enum WindowKey {
    Close,
    Undo,
    Redo,
}

/// The browser window's own shortcuts: ⌘W, ⌘Z and ⇧⌘Z. `key` is the key pressed without
/// modifiers other than shift (⇧⌘Z gives `Z`).
fn window_key(key: &str, cmd: bool, shift: bool, alt: bool, ctrl: bool) -> Option<WindowKey> {
    if !cmd || alt || ctrl {
        return None;
    }
    match (key.to_ascii_lowercase().as_str(), shift) {
        ("w", false) => Some(WindowKey::Close),
        ("z", false) => Some(WindowKey::Undo),
        ("z", true) => Some(WindowKey::Redo),
        _ => None,
    }
}

/// The key codes (U.S. keyboard places) of W and Z (`kVK_ANSI_W`, `kVK_ANSI_Z`).
const KEY_W: u16 = 0x0D;
const KEY_Z: u16 = 0x06;

/// The key a shortcut is read as, the way AppKit's menus read it: the key's own character
/// (`chars`, without modifiers) when the layout types Latin letters, else (another script, or
/// none) the letter at that place on a U.S. keyboard. So ⌘W is the same key for the browser
/// window as for the menus on every layout, and never falls through to Close Session.
fn shortcut_letter(chars: Option<&str>, key_code: u16) -> Option<char> {
    let mut typed = chars.unwrap_or_default().chars();
    match (typed.next(), typed.next()) {
        (Some(c), None) if c.is_ascii() => Some(c.to_ascii_lowercase()),
        _ => match key_code {
            KEY_W => Some('w'),
            KEY_Z => Some('z'),
            _ => None,
        },
    }
}

/// `window_key` for a key event.
fn key_of(event: &NSEvent) -> Option<WindowKey> {
    let flags = event.modifierFlags();
    let chars = event.charactersIgnoringModifiers().map(|c| c.to_string());
    let key = shortcut_letter(chars.as_deref(), event.keyCode())?;
    window_key(
        &key.to_string(),
        flags.contains(NSEventModifierFlags::Command),
        flags.contains(NSEventModifierFlags::Shift),
        flags.contains(NSEventModifierFlags::Option),
        flags.contains(NSEventModifierFlags::Control),
    )
}

/// A titled-or-not window of the browser's size that AppKit doesn't release on close.
/// It answers its own shortcuts (see `ShortcutWindow`).
fn new_window(mtm: MainThreadMarker, style: NSWindowStyleMask) -> Retained<NSWindow> {
    let frame = NSRect::new(NSPoint::new(0.0, 0.0), WINDOW_SIZE);
    let this = ShortcutWindow::alloc(mtm).set_ivars(());
    // SAFETY: main thread; NSWindow's own initialiser, with valid arguments. The window is
    // owned by `Retained`, not by AppKit's close-releases-it rule.
    let window: Retained<ShortcutWindow> = unsafe {
        msg_send![
            super(this),
            initWithContentRect: frame,
            styleMask: style,
            backing: NSBackingStoreType::Buffered,
            defer: false,
        ]
    };
    // SAFETY: as above; AppKit keeps the window until `Retained` lets it go.
    unsafe { window.setReleasedWhenClosed(false) };
    Retained::into_super(window)
}

define_class!(
    // SAFETY: NSWindow may be subclassed; `ShortcutWindow` has no ivars and no `Drop`.
    #[unsafe(super(NSWindow, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    struct ShortcutWindow;

    impl ShortcutWindow {
        #[unsafe(method(performKeyEquivalent:))]
        fn perform_key_equivalent(&self, event: &NSEvent) -> bool {
            self.key_equivalent(event)
        }
    }
);

impl ShortcutWindow {
    /// fuxx's menu items answer only fuxx's own window, so this window takes ⌘W itself (before
    /// the page sees it: closing is the window's, as in Safari), and ⌘Z / ⇧⌘Z once the page has
    /// passed on them (WebKit sends a key the page didn't handle round again). Undo goes to the
    /// window's undo manager. Cut, copy, paste and select all come from fuxx's Edit menu, as the
    /// standard selectors.
    fn key_equivalent(&self, event: &NSEvent) -> bool {
        let key = key_of(event);
        if key == Some(WindowKey::Close) {
            self.close();
            return true;
        }
        // SAFETY: NSWindow's own `performKeyEquivalent:`, with the event it was given.
        let handled: bool = unsafe { msg_send![super(self), performKeyEquivalent: event] };
        if handled {
            return true;
        }
        let action = match key {
            Some(WindowKey::Undo) => sel!(undo:),
            Some(WindowKey::Redo) => sel!(redo:),
            _ => return false,
        };
        // Up the window's own responder chain only (the page's view, the window, which keeps
        // the undo manager): never to the app, whose delegate is GPUI's.
        let sender: &AnyObject = self.as_ref();
        // SAFETY: main thread; standard actions that take any sender.
        unsafe {
            match self.firstResponder() {
                Some(first) => first.tryToPerform_with(action, Some(sender)),
                None => self.tryToPerform_with(action, Some(sender)),
            }
        }
    }
}

/// What the browser's delegate keeps.
#[derive(Default)]
struct DelegateState {
    /// The window has closed.
    closed: Cell<bool>,
    /// Why the last navigation failed (cleared when the browser starts a new one, and when a
    /// page arrives, however it was asked for).
    failure: RefCell<Option<String>>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and `BrowserDelegate` has no `Drop`.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = DelegateState]
    struct BrowserDelegate;

    // SAFETY: NSObjectProtocol has no safety requirements.
    unsafe impl NSObjectProtocol for BrowserDelegate {}

    // SAFETY: the signature matches `windowWillClose:`.
    unsafe impl NSWindowDelegate for BrowserDelegate {
        #[unsafe(method(windowWillClose:))]
        fn window_will_close(&self, _notification: &NSNotification) {
            self.ivars().closed.set(true);
        }
    }

    // SAFETY: the signatures match `webView:didCommitNavigation:`,
    // `webView:didFailProvisionalNavigation:withError:` and `webView:didFailNavigation:withError:`.
    unsafe impl WKNavigationDelegate for BrowserDelegate {
        // A page arrived (a load, a click, a form, a redirect): any earlier failure is over.
        #[unsafe(method(webView:didCommitNavigation:))]
        fn did_commit_navigation(&self, _web_view: &WKWebView, _navigation: Option<&WKNavigation>) {
            self.ivars().failure.take();
        }

        #[unsafe(method(webView:didFailProvisionalNavigation:withError:))]
        fn did_fail_provisional_navigation(
            &self,
            _web_view: &WKWebView,
            _navigation: Option<&WKNavigation>,
            error: &NSError,
        ) {
            self.navigation_failed(error);
        }

        #[unsafe(method(webView:didFailNavigation:withError:))]
        fn did_fail_navigation(
            &self,
            _web_view: &WKWebView,
            _navigation: Option<&WKNavigation>,
            error: &NSError,
        ) {
            self.navigation_failed(error);
        }
    }

    // SAFETY: the signature matches
    // `webView:createWebViewWithConfiguration:forNavigationAction:windowFeatures:`.
    unsafe impl WKUIDelegate for BrowserDelegate {
        // A page asks for a new window (a `target="_blank"` link or form, a `window.open` the
        // shim didn't see): load its http(s) address in this window instead, and make none.
        #[unsafe(method_id(webView:createWebViewWithConfiguration:forNavigationAction:windowFeatures:))]
        fn create_web_view(
            &self,
            web_view: &WKWebView,
            _configuration: &WKWebViewConfiguration,
            action: &WKNavigationAction,
            _features: &WKWindowFeatures,
        ) -> Option<Retained<WKWebView>> {
            // SAFETY: main thread (WebKit calls its delegates there); plain property reads.
            let request = unsafe { action.request() };
            let web = request
                .URL()
                .and_then(|url| url.scheme())
                .is_some_and(|scheme| {
                    let scheme = scheme.to_string().to_ascii_lowercase();
                    scheme == "http" || scheme == "https"
                });
            if web {
                // SAFETY: main thread; the view copies the request.
                unsafe { web_view.loadRequest(&request) };
            }
            None
        }
    }
);

impl BrowserDelegate {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(DelegateState::default());
        // SAFETY: NSObject's `init` has this signature.
        unsafe { msg_send![super(this), init] }
    }

    /// Keep WebKit's reason, unless the navigation was only cancelled (another one replaced it).
    /// WebKit's own error codes are positive, so only NSURLErrorDomain's -999 is a cancel.
    fn navigation_failed(&self, error: &NSError) {
        if error.code() != NSURLErrorCancelled {
            let reason = error.localizedDescription().to_string();
            self.ivars().failure.replace(Some(reason));
        }
    }
}

/// An eval script's answer as a value: `{"ok": value}` (a missing `ok` is `null`) or
/// `{"error": text}`, which becomes `the script failed: <text>`.
fn parse_answer(text: &str) -> Result<Value, String> {
    let answer: serde_json::Map<String, Value> = serde_json::from_str(text)
        .map_err(|e| format!("the page's answer isn't a JSON object ({e})"))?;
    match answer.get("error") {
        Some(error) => Err(format!(
            "the script failed: {}",
            error
                .as_str()
                .map_or_else(|| error.to_string(), str::to_string)
        )),
        None => Ok(answer.get("ok").cloned().unwrap_or(Value::Null)),
    }
}

/// Once nothing is loading: `the page failed to load: <reason>` with WebKit's reason for the
/// last failed navigation, or (with no reason, as for an address NSURL can't read) with the
/// requested address when the browser still shows no page.
fn load_error(
    requested: Option<&str>,
    url: &str,
    loading: bool,
    failure: Option<&str>,
) -> Option<String> {
    if loading {
        return None;
    }
    if let Some(reason) = failure {
        return Some(format!("the page failed to load: {reason}"));
    }
    let requested = requested?;
    let blank = url.is_empty() || url == "about:blank";
    blank.then(|| format!("the page failed to load: {requested}"))
}

/// The width and height a PNG's header gives, in pixels.
fn png_size(png: &[u8]) -> Option<(u32, u32)> {
    if png.len() < 24 || !png.starts_with(b"\x89PNG\r\n\x1a\n") || &png[12..16] != b"IHDR" {
        return None;
    }
    let number = |at: usize| u32::from_be_bytes(png[at..at + 4].try_into().unwrap());
    Some((number(16), number(20)))
}

/// Encode an image as PNG (through its TIFF form, which every `NSImage` has).
fn png_bytes(image: &NSImage) -> Result<Vec<u8>, String> {
    let tiff = image
        .TIFFRepresentation()
        .ok_or("the screenshot has no bitmap form")?;
    let bitmap =
        NSBitmapImageRep::imageRepWithData(&tiff).ok_or("the screenshot isn't a bitmap")?;
    // SAFETY: an empty properties dictionary is valid for every file type.
    let png = unsafe {
        bitmap.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new())
    }
    .ok_or("the screenshot couldn't be encoded as PNG")?;
    Ok(png.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    #[test]
    fn only_the_person_s_open_takes_the_keyboard() {
        assert_eq!(front_order(true), FrontOrder::TakeKeyboard);
        assert_eq!(front_order(false), FrontOrder::KeepKeyboard);
    }

    #[test]
    fn answers_become_values() {
        assert_eq!(parse_answer(r#"{"ok":"fuxx"}"#), Ok(json!("fuxx")));
        assert_eq!(
            parse_answer(r#"{"ok":{"a":1,"b":[true,null]}}"#),
            Ok(json!({"a": 1, "b": [true, null]}))
        );
        assert_eq!(parse_answer(r#"{"ok":null}"#), Ok(Value::Null));
        // A function at the top level: `JSON.stringify` leaves `ok` out.
        assert_eq!(parse_answer("{}"), Ok(Value::Null));
        assert_eq!(
            parse_answer(r#"{"error":"ReferenceError: Can't find variable: nope"}"#),
            Err("the script failed: ReferenceError: Can't find variable: nope".to_string())
        );
        assert!(
            parse_answer("not json")
                .unwrap_err()
                .starts_with("the page's answer")
        );
        assert!(
            parse_answer("[1]")
                .unwrap_err()
                .starts_with("the page's answer")
        );
    }

    #[test]
    fn a_load_that_never_left_the_blank_page_failed() {
        let url = "http://localhost:3000/";
        let failed = Some(format!("the page failed to load: {url}"));
        assert_eq!(load_error(Some(url), "", false, None), failed);
        assert_eq!(load_error(Some(url), "about:blank", false, None), failed);
        // Still loading, or nothing asked for yet: no verdict.
        assert_eq!(load_error(Some(url), "", true, None), None);
        assert_eq!(load_error(None, "", false, None), None);
        // A page arrived (even after a redirect elsewhere).
        assert_eq!(
            load_error(Some(url), "http://localhost:3000/login", false, None),
            None
        );
    }

    #[test]
    fn a_failed_navigation_gives_its_reason() {
        let url = "http://localhost:3000/";
        let refused = Some("Could not connect to the server.");
        let failed = Some("the page failed to load: Could not connect to the server.".to_string());
        // The old page is still showing: only WebKit's reason tells.
        assert_eq!(
            load_error(Some(url), "https://example.com/", false, refused),
            failed
        );
        assert_eq!(load_error(Some(url), "", false, refused), failed);
        // Not while something is still loading (a redirect or the next try).
        assert_eq!(
            load_error(Some(url), "https://example.com/", true, refused),
            None
        );
    }

    #[test]
    fn only_a_syntax_error_is_retried_as_statements() {
        let exception = 4; // WKErrorJavaScriptExceptionOccurred
        let syntax = Some("SyntaxError: Unexpected keyword 'const'");
        assert!(did_not_parse("WKErrorDomain", exception, syntax));
        // The script ran and threw outside fuxx's try (a page patched JSON.stringify): never again.
        assert!(!did_not_parse(
            "WKErrorDomain",
            exception,
            Some("Error: patched")
        ));
        assert!(!did_not_parse("WKErrorDomain", exception, None));
        // Other WebKit errors (the web view went away, an unsupported result type).
        assert!(!did_not_parse("WKErrorDomain", 1, syntax));
        assert!(!did_not_parse("WKErrorDomain", 5, syntax));
        assert!(!did_not_parse("NSURLErrorDomain", exception, syntax));
    }

    #[test]
    fn the_browser_window_s_own_shortcuts() {
        assert_eq!(
            window_key("w", true, false, false, false),
            Some(WindowKey::Close)
        );
        assert_eq!(
            window_key("W", true, false, false, false),
            Some(WindowKey::Close)
        );
        assert_eq!(
            window_key("z", true, false, false, false),
            Some(WindowKey::Undo)
        );
        assert_eq!(
            window_key("Z", true, true, false, false),
            Some(WindowKey::Redo)
        );
        assert_eq!(
            window_key("z", true, true, false, false),
            Some(WindowKey::Redo)
        );
        // Not with another modifier, nor without ⌘; other keys go on to the page and menus.
        assert_eq!(window_key("w", true, true, false, false), None);
        assert_eq!(window_key("w", true, false, true, false), None);
        assert_eq!(window_key("z", true, false, false, true), None);
        assert_eq!(window_key("w", false, false, false, false), None);
        assert_eq!(window_key("c", true, false, false, false), None);
        assert_eq!(window_key("q", true, false, false, false), None);
    }

    #[test]
    fn shortcuts_read_the_key_as_the_menus_do() {
        // A layout that types Latin letters: the key's own letter, wherever it sits (AZERTY
        // has W where a U.S. keyboard has Z, and Z where it has W).
        assert_eq!(shortcut_letter(Some("w"), KEY_W), Some('w'));
        assert_eq!(shortcut_letter(Some("w"), KEY_Z), Some('w'));
        assert_eq!(shortcut_letter(Some("z"), KEY_W), Some('z'));
        assert_eq!(shortcut_letter(Some("Z"), KEY_Z), Some('z'));
        assert_eq!(shortcut_letter(Some(","), KEY_W), Some(','));
        // Another script (Russian, Greek…): the letter at that place on a U.S. keyboard, as
        // the menus fall back to, so ⌘W never reaches fuxx's Close Session.
        assert_eq!(shortcut_letter(Some("ц"), KEY_W), Some('w'));
        assert_eq!(shortcut_letter(Some("я"), KEY_Z), Some('z'));
        assert_eq!(shortcut_letter(Some("ж"), 0x29), None);
        assert_eq!(shortcut_letter(None, KEY_W), Some('w'));
        assert_eq!(shortcut_letter(Some(""), KEY_Z), Some('z'));
    }

    #[test]
    fn timeouts_answer_with_the_exact_text() {
        use futures::executor::block_on;
        // A script WebKit never answers (its sender is kept, never used) and a limit that has
        // passed.
        let (_never, answer) = oneshot::channel::<Result<Value, String>>();
        let late = block_on(answer_before(
            answer,
            future::ready(()),
            Duration::from_secs(10),
        ));
        assert_eq!(late, Err("the page didn't answer in 10 s".to_string()));
        assert_eq!(
            didnt_answer(Duration::from_secs(30)),
            "the page didn't answer in 30 s"
        );
        // An answer that came first wins, even when the limit has passed too.
        let (tx, answer) = oneshot::channel();
        tx.send(Ok(json!(1))).unwrap();
        let first = block_on(answer_before(
            answer,
            future::ready(()),
            Duration::from_secs(10),
        ));
        assert_eq!(first, Ok(json!(1)));
        // The request dropped unanswered: the browser is gone.
        let (tx, answer) = oneshot::channel::<Result<Value, String>>();
        drop(tx);
        let gone = block_on(answer_before(
            answer,
            future::pending(),
            Duration::from_secs(10),
        ));
        assert_eq!(gone, Err(CLOSED.to_string()));
    }

    #[test]
    fn png_size_reads_the_header() {
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        png.extend_from_slice(&1100u32.to_be_bytes());
        png.extend_from_slice(&800u32.to_be_bytes());
        assert_eq!(png_size(&png), Some((1100, 800)));
        assert_eq!(png_size(&png[..20]), None);
        assert_eq!(png_size(b"GIF89a and then some more bytes"), None);
    }
}
