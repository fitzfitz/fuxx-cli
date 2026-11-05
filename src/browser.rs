//! The agent browser (M10c): a `WKWebView` in its own `NSWindow`, driven through objc2.
//!
//! A [`Browser`] is one session's browser window. Every WebKit and AppKit call runs on the main
//! thread (a `Browser` can't leave it). WebKit answers scripts and screenshots through
//! completion blocks, also on the main thread; each answer goes into a channel the caller
//! awaits from a GPUI task, so nothing ever blocks the run loop waiting for a page.
//!
//! The hidden `fuxx __browser-probe` self-test drives an off-screen `Browser` through every
//! operation, against pages it serves itself on 127.0.0.1, prints one JSON report and quits.

use std::cell::{Cell, RefCell};
use std::fmt::Debug;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::rc::{Rc, Weak};
use std::time::{Duration, Instant};

use block2::RcBlock;
use futures::channel::oneshot;
use futures::future::{self, Either};
use gpui::{App, AsyncApp, BackgroundExecutor};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSBitmapImageFileType,
    NSBitmapImageRep, NSImage, NSWindow, NSWindowDelegate, NSWindowStyleMask,
};
use objc2_foundation::{
    NSDictionary, NSError, NSNotification, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize,
    NSString, NSURL, NSURLErrorCancelled, NSURLRequest,
};
use objc2_web_kit::{
    WKNavigation, WKNavigationAction, WKNavigationDelegate, WKUIDelegate, WKUserScript,
    WKUserScriptInjectionTime, WKWebView, WKWebViewConfiguration, WKWebsiteDataStore,
    WKWindowFeatures,
};
use serde_json::{Value, json};

use crate::browser_js::{self, Snapshot};

/// A browser window's content size, in points.
const WINDOW_SIZE: NSSize = NSSize::new(1100.0, 800.0);
/// What a pending script or screenshot answers when its browser closes first.
const CLOSED: &str = "browser closed";
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
    /// A new browser window titled `title`, shown in front: 1100 × 800 points, cascaded from the
    /// key (or else main) window, centred when there is none. Pages get a `window.open` that
    /// loads in this window; cookies and logins live in the one shared, persistent data store;
    /// Safari's Web Inspector can attach where macOS allows it (13.3 and later).
    #[allow(dead_code)] // opened by the workspace from M10c Task 4
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
        browser.bring_to_front();
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
        // An address NSURL can't read loads nothing, which `state` reports as a failed load.
        if let Some(address) = NSURL::URLWithString(&NSString::from_str(url)) {
            // SAFETY: main thread; the view copies the request.
            unsafe {
                self.view
                    .loadRequest(&NSURLRequest::requestWithURL(&address))
            };
        }
    }

    /// Run `js` in the page and answer with its value as JSON (`undefined` as `null`; what JSON
    /// can't hold, such as functions and DOM nodes, as `null` or `{}`), or what it threw as
    /// `the script failed: <error>`. It runs as one expression (on any page); when it isn't one,
    /// as statements through `eval`, which some pages' Content-Security-Policy forbids. The
    /// answer never times out by itself: the caller limits the wait (see [`within`]).
    pub fn eval(&self, js: &str) -> oneshot::Receiver<Result<Value, String>> {
        let (tx, rx) = oneshot::channel();
        let slot = self.pend(tx);
        let statements = browser_js::eval_statements_js(js);
        // Weak: a block WebKit never calls must not keep the view alive.
        let view = objc2::rc::Weak::from_retained(&self.view);
        let block = RcBlock::new(move |result: *mut AnyObject, error: *mut NSError| {
            // SAFETY: WebKit passes a valid object or nil for each.
            match unsafe { (result.as_ref(), error.as_ref()) } {
                // The expression form didn't parse (or the code broke out of it): nothing ran.
                (_, Some(_)) => match view.load() {
                    Some(view) => evaluate(&view, &statements, slot.clone()),
                    None => answer(&slot, Err(CLOSED.to_string())),
                },
                (result, None) => answer(&slot, script_answer(result, None)),
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
    pub fn bring_to_front(&self) {
        self.window.makeKeyAndOrderFront(None);
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
    match future::select(answer, executor.timer(limit)).await {
        Either::Left((Ok(answer), _)) => answer,
        // WebKit dropped the request without answering: its web view is gone.
        Either::Left((Err(_), _)) => Err(CLOSED.to_string()),
        Either::Right(_) => Err(format!("the page didn't answer in {} s", limit.as_secs())),
    }
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
            error.localizedDescription()
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

/// Give `slot` its answer, unless it has had one (or the browser closed first).
fn answer<T>(slot: &Slot<T>, answer: Result<T, String>) {
    if let Some(tx) = slot.take() {
        let _ = tx.send(answer);
    }
}

/// A titled-or-not window of the browser's size that AppKit doesn't release on close.
fn new_window(mtm: MainThreadMarker, style: NSWindowStyleMask) -> Retained<NSWindow> {
    let frame = NSRect::new(NSPoint::new(0.0, 0.0), WINDOW_SIZE);
    // SAFETY: main thread; a plain initialiser with valid arguments. The window is owned by
    // `Retained`, not by AppKit's close-releases-it rule.
    unsafe {
        let window = NSWindow::initWithContentRect_styleMask_backing_defer(
            mtm.alloc(),
            frame,
            style,
            NSBackingStoreType::Buffered,
            false,
        );
        window.setReleasedWhenClosed(false);
        window
    }
}

/// What the browser's delegate keeps.
#[derive(Default)]
struct DelegateState {
    /// The window has closed.
    closed: Cell<bool>,
    /// Why the last navigation failed (cleared when the browser starts a new one).
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

    // SAFETY: the signatures match `webView:didFailProvisionalNavigation:withError:` and
    // `webView:didFailNavigation:withError:`.
    unsafe impl WKNavigationDelegate for BrowserDelegate {
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

/// The probe gives up and exits with code 2 after this long, whatever is still pending.
const PROBE_DEADLINE: Duration = Duration::from_secs(15);
/// How often the probe looks at the page while it waits for something.
const LOAD_POLL: Duration = Duration::from_millis(50);
/// How long the probe waits for a page to settle.
const SETTLE_LIMIT: Duration = Duration::from_secs(5);
/// How long the probe lets a script or screenshot take (its own limit, as callers apply one).
const PROBE_ANSWER_LIMIT: Duration = Duration::from_secs(2);
/// The limit for the script that never returns.
const PROBE_HANG_LIMIT: Duration = Duration::from_secs(1);

/// The probe's main page: a heading, a link, a link to a new window, a text field and a
/// button that changes the heading.
const PROBE_PAGE: &str = r#"<!doctype html>
<html><head><meta charset="utf-8"><title>fuxx probe</title></head>
<body>
<h1 id="heading">Hello from fuxx</h1>
<p><a href="/two">page two</a> <a href="/two" target="_blank">page two in a new window</a></p>
<input id="name" type="text" placeholder="your name">
<button id="go" onclick="document.getElementById('heading').textContent = 'Hi, ' + document.getElementById('name').value">Go</button>
</body></html>"#;

/// The page the links lead to.
const PAGE_TWO: &str = r#"<!doctype html>
<html><head><meta charset="utf-8"><title>page two</title></head>
<body><h1>Page two</h1></body></html>"#;

/// A page whose Content-Security-Policy allows no script at all (as strict sites do).
const LOCKED_PAGE: &str = r#"<!doctype html>
<html><head><meta charset="utf-8"><title>locked</title></head>
<body><h1>Locked page</h1><button>Press</button></body></html>"#;

/// `fuxx __browser-probe`: never returns. Exits 0 after printing a one-line JSON report of every
/// browser operation, 1 with a message on stderr if one misbehaves, 2 if it takes longer than
/// 15 s.
pub fn run_probe() -> ! {
    std::thread::spawn(|| {
        std::thread::sleep(PROBE_DEADLINE);
        eprintln!("fuxx __browser-probe: no answer in 15 s");
        std::process::exit(2);
    });
    gpui_platform::application().run(|cx: &mut App| {
        let mtm = MainThreadMarker::new().expect("GPUI runs its callback on the main thread");
        // GPUI made fuxx a regular app (a Dock icon); the probe shows nothing, so it isn't one.
        NSApplication::sharedApplication(mtm)
            .setActivationPolicy(NSApplicationActivationPolicy::Accessory);
        cx.spawn(async move |cx: &mut AsyncApp| match probe(mtm, cx).await {
            Ok(report) => {
                println!("{report}");
                cx.update(|cx| cx.quit());
            }
            Err(message) => {
                eprintln!("fuxx __browser-probe: {message}");
                std::process::exit(1);
            }
        })
        .detach();
    });
    // The run loop only ends by exiting the process.
    std::process::exit(0)
}

/// Every operation through an off-screen `Browser`, each checked; the JSON report line.
async fn probe(mtm: MainThreadMarker, cx: &mut AsyncApp) -> Result<String, String> {
    let started = Instant::now();
    let ex = cx.background_executor().clone();
    let base = format!("http://127.0.0.1:{}", serve_probe_pages()?);
    let home = format!("{base}/");
    let two = format!("{base}/two");
    let b = Browser::offscreen(mtm);

    // A load that fails: nothing listens on that port.
    let dead = format!("http://127.0.0.1:{}/", unused_port()?);
    b.load(&dead);
    let loading_at_once = b.state().loading;
    let failed = settle(&b, &ex, |s| !s.loading).await?;
    check(
        failed.url.is_empty()
            && failed
                .error
                .as_ref()
                .is_some_and(|e| e.starts_with("the page failed to load: ")),
        "failed load",
        &failed,
    )?;

    // The test page.
    b.load(&home);
    let loaded = settle(&b, &ex, |s| !s.loading && s.url == home).await?;
    check(loaded.error.is_none(), "loaded page has no error", &loaded)?;
    check(loaded.title == "fuxx probe", "page title", &loaded)?;

    // A failed load while a page shows: WebKit's reason, the old page stays.
    b.load(&dead);
    let failed_over = settle(&b, &ex, |s| !s.loading).await?;
    check(
        failed_over.url == home
            && failed_over
                .error
                .as_ref()
                .is_some_and(|e| e.starts_with("the page failed to load: ")),
        "failed load over a page",
        &failed_over,
    )?;
    b.load(&home);
    let reloaded = settle(&b, &ex, |s| !s.loading && s.url == home).await?;
    check(
        reloaded.error.is_none(),
        "the same page loaded again",
        &reloaded,
    )?;

    // Any value comes back as JSON; a thrown exception is an error, not a timeout.
    let title = ask(&b, &ex, "document.title").await?;
    check(title == json!("fuxx probe"), "eval title", &title)?;
    let object = ask(
        &b,
        &ex,
        "({ n: 1, list: [true, null, 'x'], gone: undefined })",
    )
    .await?;
    check(
        object == json!({"n": 1, "list": [true, null, "x"]}),
        "eval object",
        &object,
    )?;
    let undefined = ask(&b, &ex, "undefined").await?;
    check(undefined == Value::Null, "eval undefined", &undefined)?;
    let statements = ask(&b, &ex, "const a = [1, 2, 3]; a.length").await?;
    check(statements == json!(3), "eval statements", &statements)?;
    let again = ask(&b, &ex, "const a = 'no clash'; a").await?;
    check(again == json!("no clash"), "eval statements again", &again)?;
    let element = ask(&b, &ex, "document.body").await?;
    let thrown = within(b.eval("throw new Error('boom')"), PROBE_ANSWER_LIMIT, &ex).await;
    check(
        thrown == Err("the script failed: Error: boom".into()),
        "thrown",
        &thrown,
    )?;
    let syntax = within(b.eval("1 +"), PROBE_ANSWER_LIMIT, &ex).await;
    check(
        syntax
            .as_ref()
            .is_err_and(|e| e.starts_with("the script failed: SyntaxError")),
        "syntax error",
        &syntax,
    )?;
    let lone = ask(&b, &ex, r"({ ['\udc00key']: '\ud800x' })").await?;
    check(
        lone == json!({"\u{FFFD}key": "\u{FFFD}x"}),
        "lone surrogates",
        &lone,
    )?;

    // Snapshot, fill, click, read, wait, press.
    let snapshot = take_snapshot(&b, &ex).await?;
    let outline_lines = browser_js::render_outline(&snapshot).lines().count();
    let field = find_ref(&snapshot, "textbox", "your name")?;
    let go = find_ref(&snapshot, "button", "Go")?;
    let filled = ask(&b, &ex, &browser_js::fill_js(&field, "probe")).await?;
    check(filled == json!({"ok": true}), "fill", &filled)?;
    let clicked = ask(&b, &ex, &browser_js::click_js(&go)).await?;
    check(clicked == json!({"ok": true}), "click", &clicked)?;
    let heading = ask(&b, &ex, "document.getElementById('heading').textContent").await?;
    check(
        heading == json!("Hi, probe"),
        "heading after click",
        &heading,
    )?;
    let unknown = ask(&b, &ex, &browser_js::click_js("e999")).await?;
    check(
        unknown == json!({"error": "unknown ref e999"}),
        "unknown ref",
        &unknown,
    )?;
    let read = ask(&b, &ex, browser_js::read_js()).await?;
    check(
        read.as_str().is_some_and(|t| t.contains("Hi, probe")),
        "read",
        &read,
    )?;
    let wait_hit = ask(
        &b,
        &ex,
        &browser_js::wait_check_js(Some("Hi, probe"), Some("#go")),
    )
    .await?;
    let wait_miss = ask(&b, &ex, &browser_js::wait_check_js(Some("nowhere"), None)).await?;
    check(
        wait_hit == json!(true) && wait_miss == json!(false),
        "wait",
        &(&wait_hit, &wait_miss),
    )?;
    let not_a_key = ask(&b, &ex, &browser_js::press_js("constructor")).await?;
    check(
        not_a_key["error"]
            .as_str()
            .is_some_and(|e| e.starts_with("unknown key \"constructor\"")),
        "press constructor",
        &not_a_key,
    )?;

    // `window.open` loads in the same window, and so does a `target="_blank"` link.
    let opened = ask(&b, &ex, "window.open('/two') === window").await?;
    check(
        opened == json!(true),
        "window.open answers the window",
        &opened,
    )?;
    settle(&b, &ex, |s| {
        !s.loading && s.url == two && s.title == "page two"
    })
    .await?;
    b.back();
    settle(&b, &ex, |s| !s.loading && s.url == home).await?;
    let blank_link = find_ref(
        &take_snapshot(&b, &ex).await?,
        "link",
        "page two in a new window",
    )?;
    let followed = ask(&b, &ex, &browser_js::click_js(&blank_link)).await?;
    check(
        followed == json!({"ok": true}),
        "click target=_blank",
        &followed,
    )?;
    let blank_target = settle(&b, &ex, |s| !s.loading && s.url == two).await?;
    b.back();
    let back = settle(&b, &ex, |s| !s.loading && s.url == home).await?;

    // Reload: a fresh page forgets what the old one set.
    ask(&b, &ex, "window.fuxxMark = 1").await?;
    b.reload();
    let deadline = Instant::now() + SETTLE_LIMIT;
    while ask(
        &b,
        &ex,
        "window.fuxxMark === undefined && document.readyState === 'complete'",
    )
    .await?
        != json!(true)
    {
        check(Instant::now() < deadline, "reload", &b.state())?;
        ex.timer(LOAD_POLL).await;
    }

    // The screenshot: the whole view at the backing scale.
    let png = within(b.screenshot(), PROBE_ANSWER_LIMIT, &ex).await?;
    let scale = b.window.backingScaleFactor();
    let size = png_size(&png);
    let expected = (
        (WINDOW_SIZE.width * scale) as u32,
        (WINDOW_SIZE.height * scale) as u32,
    );
    check(size == Some(expected), "screenshot size", &(size, scale))?;
    let dir = std::env::temp_dir().join("fuxx-browser-probe");
    let path: PathBuf = dir.join("screenshot.png");
    std::fs::create_dir_all(&dir)
        .and_then(|_| std::fs::write(&path, &png))
        .map_err(|e| format!("couldn't save the screenshot to {}: {e}", path.display()))?;
    eprintln!(
        "[fuxx] browser probe: screenshot saved to {}",
        path.display()
    );

    // A page whose CSP allows no script: fuxx's scripts still run.
    b.load(&format!("{base}/locked"));
    settle(&b, &ex, |s| !s.loading && s.title == "locked").await?;
    let locked_title = ask(&b, &ex, "document.title;").await?;
    check(
        locked_title == json!("locked"),
        "eval under CSP",
        &locked_title,
    )?;
    let locked_snapshot = take_snapshot(&b, &ex).await?;
    let press = find_ref(&locked_snapshot, "button", "Press")?;
    let locked_click = ask(&b, &ex, &browser_js::click_js(&press)).await?;
    check(
        locked_click == json!({"ok": true}),
        "click under CSP",
        &locked_click,
    )?;
    let locked_iife = ask(&b, &ex, "(() => { const a = 2; return a * 21; })()").await?;
    check(locked_iife == json!(42), "an IIFE under CSP", &locked_iife)?;
    let locked_statements = within(b.eval("const a = 1; a"), PROBE_ANSWER_LIMIT, &ex).await;
    check(
        locked_statements.as_ref().is_err_and(|e| {
            e.starts_with("the script failed: EvalError")
                && e.ends_with(&format!("({EVAL_REFUSED_HINT})"))
        }),
        "statements under CSP",
        &locked_statements,
    )?;

    // The title, and the user closing the window: noticed, and the browser still answers.
    b.set_title("probe — page two");
    let window_title = b.window.title().to_string();
    check(window_title == "probe — page two", "title", &window_title)?;
    check(!b.is_closed(), "open window is not closed", &())?;
    b.window.close();
    check(b.is_closed(), "user-closed window is closed", &())?;
    let after_close = ask(&b, &ex, "1 + 1").await?;

    // A script that never returns hits the caller's limit; closing answers what still waits.
    let hang = within(
        b.eval("(() => { while (true) {} })()"),
        PROBE_HANG_LIMIT,
        &ex,
    )
    .await;
    let hang_text = format!("the page didn't answer in {} s", PROBE_HANG_LIMIT.as_secs());
    check(hang == Err(hang_text), "timeout", &hang)?;
    let late_eval = b.eval("'late'");
    let late_screenshot = b.screenshot();
    b.close();
    let late_eval = late_eval.await.map_err(|_| "dropped".to_string())?;
    let late_screenshot = late_screenshot.await.map_err(|_| "dropped".to_string())?;
    check(
        late_eval == Err(CLOSED.into()) && late_screenshot == Err(CLOSED.into()),
        "closed mid-eval",
        &(&late_eval, &late_screenshot),
    )?;

    Ok(json!({
        "failed_load": failed.error,
        "failed_load_over_a_page": { "url": failed_over.url, "error": failed_over.error },
        "loading_right_after_load": loading_at_once,
        "page": { "url": loaded.url, "title": loaded.title },
        "eval": {
            "title": title,
            "object": object,
            "undefined": undefined,
            "statements": statements,
            "dom_node": element,
            "thrown": thrown.unwrap_err(),
            "syntax": syntax.unwrap_err(),
            "lone_surrogates": lone,
        },
        "snapshot": {
            "nodes": snapshot.nodes.len(),
            "refs": snapshot.nodes.iter().filter(|n| n.r#ref.is_some()).count(),
            "outline_lines": outline_lines,
        },
        "fill": filled,
        "click": clicked,
        "clicked": heading,
        "unknown_ref": unknown,
        "read_chars": read.as_str().map_or(0, |t| t.chars().count()),
        "wait": [wait_hit, wait_miss],
        "press_constructor": not_a_key,
        "window_open": opened,
        "target_blank": blank_target.url,
        "back": back.url,
        "reload": true,
        "screenshot": { "bytes": png.len(), "size": size, "scale": scale },
        "csp_locked": {
            "eval": locked_title,
            "snapshot_nodes": locked_snapshot.nodes.len(),
            "click": locked_click,
            "iife": locked_iife,
            "statements": locked_statements.unwrap_err(),
        },
        "window_title": window_title,
        "user_closed": { "is_closed": true, "still_answers": after_close },
        "timeout": hang.unwrap_err(),
        "closed_mid_eval": [late_eval.unwrap_err(), late_screenshot.unwrap_err()],
        "ms": started.elapsed().as_millis() as u64,
    })
    .to_string())
}

/// `Err` naming `what` and showing `got` unless `ok`.
fn check(ok: bool, what: &str, got: &impl Debug) -> Result<(), String> {
    if ok {
        Ok(())
    } else {
        Err(format!("{what}: got {got:?}"))
    }
}

/// `js`'s value within the probe's limit.
async fn ask(b: &Browser, ex: &BackgroundExecutor, js: &str) -> Result<Value, String> {
    within(b.eval(js), PROBE_ANSWER_LIMIT, ex).await
}

/// The page's snapshot.
async fn take_snapshot(b: &Browser, ex: &BackgroundExecutor) -> Result<Snapshot, String> {
    let value = ask(b, ex, browser_js::snapshot_js()).await?;
    serde_json::from_value(value).map_err(|e| format!("the snapshot didn't parse: {e}"))
}

/// The ref of the control with this role and name.
fn find_ref(snapshot: &Snapshot, role: &str, name: &str) -> Result<String, String> {
    snapshot
        .nodes
        .iter()
        .find(|n| n.role == role && n.name == name)
        .and_then(|n| n.r#ref.clone())
        .ok_or_else(|| format!("no {role} {name:?} in {snapshot:?}"))
}

/// The browser's state once `done` holds (polled), or an error after `SETTLE_LIMIT`.
async fn settle(
    b: &Browser,
    ex: &BackgroundExecutor,
    done: impl Fn(&PageState) -> bool,
) -> Result<PageState, String> {
    let deadline = Instant::now() + SETTLE_LIMIT;
    loop {
        let state = b.state();
        if done(&state) {
            return Ok(state);
        }
        check(Instant::now() < deadline, "the page never settled", &state)?;
        ex.timer(LOAD_POLL).await;
    }
}

/// Serve the probe's pages on 127.0.0.1 (one thread per connection, for as long as the probe
/// runs); the port.
fn serve_probe_pages() -> Result<u16, String> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|e| format!("couldn't start the test server: {e}"))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            std::thread::spawn(move || serve_one(stream));
        }
    });
    Ok(port)
}

/// Answer one request: `/`, `/two`, `/locked` (with a no-script CSP) or 404.
fn serve_one(stream: TcpStream) -> std::io::Result<()> {
    let mut reader = BufReader::new(&stream);
    let mut request = String::new();
    reader.read_line(&mut request)?;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? <= 2 {
            break;
        }
    }
    let (status, extra, body) = match request.split_whitespace().nth(1).unwrap_or("/") {
        "/" => ("200 OK", "", PROBE_PAGE),
        "/two" => ("200 OK", "", PAGE_TWO),
        "/locked" => (
            "200 OK",
            "Content-Security-Policy: default-src 'none'\r\n",
            LOCKED_PAGE,
        ),
        _ => ("404 Not Found", "", ""),
    };
    let mut stream = &stream;
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\nConnection: close\r\n{extra}\r\n{body}",
        body.len()
    )
}

/// A local port nothing listens on (just freed).
fn unused_port() -> Result<u16, String> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    Ok(listener.local_addr().map_err(|e| e.to_string())?.port())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

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
    fn png_size_reads_the_header() {
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        png.extend_from_slice(&1100u32.to_be_bytes());
        png.extend_from_slice(&800u32.to_be_bytes());
        assert_eq!(png_size(&png), Some((1100, 800)));
        assert_eq!(png_size(&png[..20]), None);
        assert_eq!(png_size(b"GIF89a and then some more bytes"), None);
    }
}
