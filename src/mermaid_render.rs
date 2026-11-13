//! Rendering Mermaid diagrams (M11ac Part 2): one hidden `WKWebView`, made on first use, loads
//! `mermaid::page_html` (no base URL, a policy that lets nothing be fetched and no page script
//! run) with mermaid.js injected as a user script, draws each diagram with `mermaid.render`
//! and snapshots it to a PNG (at 2× unless huge), decoded here into the image the Preview shows.
//!
//! Everything here runs on the main thread, never blocking it: each WebKit answer comes back
//! through a completion block into a channel a GPUI task awaits, and the PNG is decoded off the
//! UI thread. One diagram renders at a time (the page is shared); the rest wait in a queue, and
//! a job nobody wants any more (its tab closed, its file or theme changed) is dropped before it
//! renders. A diagram gets one 5 s deadline for its render and snapshot (the page's first load
//! 10 s more); past that, or when WebKit itself fails, the web view is thrown away, the next
//! diagram gets a fresh one, and the failed one renders once more the next time it's shown.
//! After 60 s with nothing to render the web view (and its WebKit process) goes.
//!
//! No network, ever: the page has no base URL and a Content-Security-Policy of
//! `default-src 'none'; script-src 'none'` (inline styles, `data:` images and fonts only); the
//! navigation delegate allows only the main frame's `about:blank` (the page from a string) and
//! cancels everything else; the data store is a fresh non-persistent one; no UI delegate, so no
//! new windows. The window it lives in is never shown, sits far off screen, can't be captured
//! and is left out of window cycling and Mission Control.
//!
//! `Mermaids` (a GPUI global) holds the `mermaid::Store` (cache of 64 and 256 MB of decoded
//! images, queue, waiters); an image dropped from the cache is taken out of every window's
//! sprite atlas (`App::drop_image`). `Mermaids::shown` is what a Preview asks each frame.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use block2::RcBlock;
use futures::channel::oneshot;
use futures::future::{self, Either};
use gpui::{
    App, AsyncApp, BackgroundExecutor, BorrowAppContext, Global, Image, ImageFormat, RenderImage,
};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSBackingStoreType, NSImage, NSWindow, NSWindowCollectionBehavior, NSWindowSharingType,
    NSWindowStyleMask,
};
use objc2_foundation::{
    NSError, NSNumber, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString,
};
use objc2_web_kit::{
    WKContentWorld, WKNavigation, WKNavigationAction, WKNavigationActionPolicy,
    WKNavigationDelegate, WKSnapshotConfiguration, WKUserScript, WKUserScriptInjectionTime,
    WKWebView, WKWebViewConfiguration, WKWebsiteDataStore,
};

use crate::browser::{didnt_answer, exception_message, png_bytes, within};
use crate::mermaid::{self, Asked, CACHE_BYTES, CACHE_SIZE, Key, Outcome, Store};

/// How long one diagram may take (render and snapshot together), and the page's first load.
pub const RENDER_LIMIT: Duration = Duration::from_secs(5);
const LOAD_LIMIT: Duration = Duration::from_secs(10);
/// The web view (and its WebKit process) goes after this long with nothing to render.
const IDLE_LIMIT: Duration = Duration::from_secs(60);
/// The web view's size before a diagram resizes it (points), and where its window sits.
const VIEW_SIZE: NSSize = NSSize::new(1200.0, 900.0);
const OFF_SCREEN: NSPoint = NSPoint::new(-20000.0, -20000.0);

/// A drawn diagram: its decoded image and its natural size in points.
#[derive(Clone, Debug)]
pub struct Drawn {
    pub image: Arc<RenderImage>,
    pub w: f32,
    pub h: f32,
}

/// Whether a waiter still wants a diagram (asked before it renders).
pub type Wanted = Box<dyn Fn(&App, &Key) -> bool>;

/// Someone waiting for a diagram: what to do when it's ready (draw again), and whether they
/// still want it (asked before it renders).
pub struct Waiter {
    pub tell: Box<dyn FnOnce(&mut App)>,
    pub wanted: Wanted,
}

/// The diagrams: the store, the renderer, and whether the pump runs (and its turn count, for
/// the idle teardown).
pub struct Mermaids {
    store: Store<Drawn, Waiter>,
    running: bool,
    turns: u64,
    renderer: Option<Rc<Renderer>>,
}

impl Default for Mermaids {
    fn default() -> Self {
        Mermaids {
            store: Store::new(CACHE_SIZE, CACHE_BYTES),
            running: false,
            turns: 0,
            renderer: None,
        }
    }
}

impl Global for Mermaids {}

impl Mermaids {
    /// The diagram for `key`, if it is drawn (or failed, or dropped just now for room).
    /// Otherwise it is queued (once) with `source`, and `waiter` (kept once per `who`, the
    /// asker's id) is told when it's ready: `Asked::Waiting` for now (the Preview shows the
    /// code). Only the cache is read when it is there: scrolling never renders again.
    pub fn shown(key: Key, source: &str, who: u64, waiter: Waiter, cx: &mut App) -> Asked<Drawn> {
        let this = cx.default_global::<Mermaids>();
        let asked = this.store.ask(key, source, who, waiter, Instant::now());
        if asked == Asked::Waiting && !this.running {
            this.running = true;
            cx.spawn(async move |cx: &mut AsyncApp| pump(cx).await)
                .detach();
        }
        asked
    }
}

impl PartialEq for Drawn {
    fn eq(&self, other: &Self) -> bool {
        self.image.id == other.image.id
    }
}

#[cfg(feature = "ui-shots")]
impl Mermaids {
    /// In the live page: does a script of the page's own run? An inline event handler (on an
    /// image that loads) and an inserted `<script>`; answers `{"handler", "script"}` (both
    /// should be false under `script-src 'none'`). `None` with no web view.
    pub fn probe_page_script(cx: &mut App) -> Option<oneshot::Receiver<Result<String, String>>> {
        let renderer = cx.default_global::<Mermaids>().renderer.clone()?;
        Some(renderer.call(
            r#"const d = document.createElement('div');
d.innerHTML = '<img src="data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7" onload="window.__fuxxHandler = 1" onerror="window.__fuxxHandler = 1">';
document.body.appendChild(d);
const s = document.createElement('script');
s.textContent = 'window.__fuxxScript = 1';
document.body.appendChild(s);
await new Promise(r => setTimeout(r, 300));
d.remove(); s.remove();
return JSON.stringify({ handler: !!window.__fuxxHandler, script: !!window.__fuxxScript });"#,
        ))
    }

    /// Diagrams (and errors) cached, their images' bytes, jobs queued, and whether the pump
    /// runs.
    pub fn counts(cx: &mut App) -> (usize, usize, usize, bool) {
        let this = cx.default_global::<Mermaids>();
        let (cached, bytes, queued) = this.store.counts();
        (cached, bytes, queued, this.running)
    }
}

/// Render the queued diagrams one at a time until none is wanted; then, after `IDLE_LIMIT`
/// with nothing new, let the web view go.
async fn pump(cx: &mut AsyncApp) {
    loop {
        let next = cx.update(|cx| {
            cx.default_global::<Mermaids>();
            let job = cx.update_global::<Mermaids, _>(|this, cx| {
                this.store.next_job(|key, waiter| (waiter.wanted)(cx, key))
            });
            let this = cx.default_global::<Mermaids>();
            this.turns += 1;
            if job.is_none() {
                this.running = false;
            }
            job.map(|job| (job, this.renderer.clone(), this.turns))
        });
        let Some(((key, source), renderer, _)) = next else {
            let turns = cx.update(|cx| cx.default_global::<Mermaids>().turns);
            cx.background_executor().timer(IDLE_LIMIT).await;
            cx.update(|cx| {
                let this = cx.default_global::<Mermaids>();
                if !this.running && this.turns == turns {
                    this.renderer = None;
                }
            });
            return;
        };
        let ex = cx.background_executor().clone();
        let (outcome, keep) = match render(renderer, &source, key.dark, &ex).await {
            Ok((renderer, png, w, h)) => {
                let svg = cx.update(|cx| cx.svg_renderer());
                let decoded = ex
                    .spawn(
                        async move { Image::from_bytes(ImageFormat::Png, png).to_image_data(svg) },
                    )
                    .await;
                match decoded {
                    Ok(image) => {
                        let size = image.size(0);
                        let bytes =
                            size.width.0.max(0) as usize * size.height.0.max(0) as usize * 4;
                        (Outcome::Drawn(Drawn { image, w, h }, bytes), Some(renderer))
                    }
                    Err(e) => (
                        Outcome::Mermaid(format!("the snapshot couldn't be read: {e}")),
                        Some(renderer),
                    ),
                }
            }
            Err(Failure::Mermaid(e, renderer)) => (Outcome::Mermaid(e), Some(renderer)),
            Err(Failure::WebKit(e)) => (Outcome::WebKit(e), None),
        };
        let outcome = match outcome {
            Outcome::Mermaid(e) => {
                eprintln!("[fuxx] mermaid: {e}");
                Outcome::Mermaid(mermaid::short_error(&e))
            }
            Outcome::WebKit(e) => {
                eprintln!("[fuxx] mermaid (WebKit): {e}");
                Outcome::WebKit(mermaid::short_error(&e))
            }
            drawn => drawn,
        };
        cx.update(|cx| {
            let this = cx.default_global::<Mermaids>();
            this.renderer = keep;
            let (tell, freed) = this.store.finish(key, outcome, Instant::now());
            // Out of every window's sprite atlas, not only out of the cache.
            for drawn in freed {
                cx.drop_image(drawn.image, None);
            }
            for waiter in tell {
                (waiter.tell)(cx);
            }
        });
    }
}

/// Why a diagram didn't render: Mermaid's own error (the page is fine, kept), or WebKit's (a
/// timeout, a crash, the page not loading: the web view is thrown away).
enum Failure {
    Mermaid(String, Rc<Renderer>),
    WebKit(String),
}

/// Draw `source` in `renderer` (made, and its page loaded, if there is none): the PNG and
/// the diagram's size in points. The render and the snapshot share one `RENDER_LIMIT`.
async fn render(
    renderer: Option<Rc<Renderer>>,
    source: &str,
    dark: bool,
    ex: &BackgroundExecutor,
) -> Result<(Rc<Renderer>, Vec<u8>, f32, f32), Failure> {
    let renderer = match renderer {
        Some(r) => r,
        None => {
            let mtm = MainThreadMarker::new()
                .ok_or_else(|| Failure::WebKit("not on the main thread".into()))?;
            let (r, loaded) = Renderer::new(dark, mtm);
            within(loaded, LOAD_LIMIT, ex)
                .await
                .map_err(Failure::WebKit)?;
            Rc::new(r)
        }
    };
    let gone = |_| Failure::WebKit("the renderer went away".to_string());
    let work = {
        let renderer = renderer.clone();
        let script = mermaid::render_script(source, dark);
        async move {
            let answer = renderer
                .call(&script)
                .await
                .map_err(gone)?
                .map_err(Failure::WebKit)?;
            let (w, h) = match mermaid::read_answer(&answer) {
                Ok(size) => size,
                Err(e) => return Err(Failure::Mermaid(e, renderer.clone())),
            };
            let png = renderer
                .snapshot(w, h)
                .await
                .map_err(gone)?
                .map_err(Failure::WebKit)?;
            Ok((png, w, h))
        }
    };
    match future::select(Box::pin(work), ex.timer(RENDER_LIMIT)).await {
        Either::Left((done, _)) => {
            let (png, w, h) = done?;
            Ok((renderer, png, w as f32, h as f32))
        }
        Either::Right(_) => Err(Failure::WebKit(didnt_answer(RENDER_LIMIT))),
    }
}

/// The hidden web view, its never-shown window, and its navigation delegate.
pub struct Renderer {
    window: Retained<NSWindow>,
    view: Retained<WKWebView>,
    delegate: Retained<MermaidDelegate>,
}

impl Renderer {
    /// A web view in a window that is never shown, loading the page with mermaid.js injected;
    /// the receiver answers when it has loaded (or failed to).
    fn new(dark: bool, mtm: MainThreadMarker) -> (Renderer, oneshot::Receiver<Result<(), String>>) {
        let frame = NSRect::new(NSPoint::new(0.0, 0.0), VIEW_SIZE);
        // SAFETY: main thread (the marker); AppKit's and WebKit's own initialisers and setters,
        // with valid arguments. The window is owned by `Retained`, not by close-releases-it.
        let (window, view) = unsafe {
            let window = NSWindow::initWithContentRect_styleMask_backing_defer(
                mtm.alloc(),
                NSRect::new(OFF_SCREEN, VIEW_SIZE),
                NSWindowStyleMask::Borderless,
                NSBackingStoreType::Buffered,
                false,
            );
            window.setReleasedWhenClosed(false);
            window.setExcludedFromWindowsMenu(true);
            window.setSharingType(NSWindowSharingType::None);
            window.setCollectionBehavior(
                NSWindowCollectionBehavior::IgnoresCycle | NSWindowCollectionBehavior::Transient,
            );
            let config = WKWebViewConfiguration::new(mtm);
            config.setWebsiteDataStore(&WKWebsiteDataStore::nonPersistentDataStore(mtm));
            // mermaid.js as a user script: the page's `script-src 'none'` doesn't govern it.
            let script = WKUserScript::initWithSource_injectionTime_forMainFrameOnly(
                mtm.alloc(),
                &NSString::from_str(&mermaid::user_script(dark)),
                WKUserScriptInjectionTime::AtDocumentEnd,
                true,
            );
            config.userContentController().addUserScript(&script);
            let view = WKWebView::initWithFrame_configuration(mtm.alloc(), frame, &config);
            window.setContentView(Some(&view));
            (window, view)
        };
        let delegate = MermaidDelegate::new(mtm);
        let (tx, rx) = oneshot::channel();
        delegate.ivars().loaded.replace(Some(tx));
        // SAFETY: main thread; the delegate lives as long as the `Renderer`, which clears the
        // link before it goes. No base URL: the page is `about:blank`, with nothing relative to
        // resolve.
        unsafe {
            view.setNavigationDelegate(Some(ProtocolObject::from_ref(&*delegate)));
            view.loadHTMLString_baseURL(&NSString::from_str(&mermaid::page_html(dark)), None);
        }
        (
            Renderer {
                window,
                view,
                delegate,
            },
            rx,
        )
    }

    /// Run `body` as an async function in the page; its string answer.
    fn call(&self, body: &str) -> oneshot::Receiver<Result<String, String>> {
        let (tx, rx) = oneshot::channel();
        let tx = Cell::new(Some(tx));
        let block = RcBlock::new(move |result: *mut AnyObject, error: *mut NSError| {
            // SAFETY: WebKit passes a valid object or nil for each.
            let answer = match unsafe { (result.as_ref(), error.as_ref()) } {
                (_, Some(error)) => Err(exception_message(error)
                    .unwrap_or_else(|| error.localizedDescription().to_string())),
                (Some(text), None) => text
                    .downcast_ref::<NSString>()
                    .map(|t| t.to_string())
                    .ok_or_else(|| "the renderer answered no text".to_string()),
                (None, None) => Err("the renderer answered nothing".to_string()),
            };
            if let Some(tx) = tx.take() {
                let _ = tx.send(answer);
            }
        });
        // SAFETY: main thread; WebKit copies the block and calls it once, on the main thread.
        unsafe {
            let world = WKContentWorld::pageWorld(MainThreadMarker::from(&*self.view));
            self.view
                .callAsyncJavaScript_arguments_inFrame_inContentWorld_completionHandler(
                    &NSString::from_str(body),
                    None,
                    None,
                    &world,
                    Some(&block),
                );
        }
        rx
    }

    /// A PNG of the diagram drawn at the page's top left, `w` × `h` points, at
    /// `mermaid::density` pixels per point. The view (and its window) grows to hold it first.
    fn snapshot(&self, w: f64, h: f64) -> oneshot::Receiver<Result<Vec<u8>, String>> {
        let (tx, rx) = oneshot::channel();
        let size = NSSize::new(w.max(VIEW_SIZE.width), h.max(VIEW_SIZE.height));
        self.window.setContentSize(size);
        self.window.setFrameOrigin(OFF_SCREEN);
        self.view
            .setFrame(NSRect::new(NSPoint::new(0.0, 0.0), size));
        let backing = self.window.backingScaleFactor().max(1.0);
        let tx = Cell::new(Some(tx));
        let block = RcBlock::new(move |image: *mut NSImage, error: *mut NSError| {
            // SAFETY: WebKit passes a valid object or nil for each.
            let shot = match unsafe { (image.as_ref(), error.as_ref()) } {
                (_, Some(error)) => Err(format!(
                    "the snapshot failed: {}",
                    error.localizedDescription()
                )),
                (Some(image), None) => png_bytes(image),
                (None, None) => Err("the snapshot came back empty".to_string()),
            };
            if let Some(tx) = tx.take() {
                let _ = tx.send(shot);
            }
        });
        // SAFETY: main thread; a configuration with a rect in the view (flipped: from its top
        // left) and a width in points; WebKit copies the block and calls it once.
        unsafe {
            let config = WKSnapshotConfiguration::new(MainThreadMarker::from(&*self.view));
            config.setRect(NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(w, h)));
            let width = w * mermaid::density(w, h) / backing;
            config.setSnapshotWidth(Some(&NSNumber::new_f64(width)));
            config.setAfterScreenUpdates(true);
            self.view
                .takeSnapshotWithConfiguration_completionHandler(Some(&config), &block);
        }
        rx
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        // SAFETY: main thread (a `Renderer` never leaves it); the delegate link goes first.
        unsafe {
            self.view.setNavigationDelegate(None);
            self.view.stopLoading();
        }
        self.delegate.ivars().loaded.take();
        self.window.close();
    }
}

/// The delegate's state: who waits for the page to load.
#[derive(Default)]
pub struct DelegateState {
    loaded: RefCell<Option<oneshot::Sender<Result<(), String>>>>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and `MermaidDelegate` has no `Drop`.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = DelegateState]
    pub struct MermaidDelegate;

    // SAFETY: NSObjectProtocol has no safety requirements.
    unsafe impl NSObjectProtocol for MermaidDelegate {}

    // SAFETY: the signatures match `webView:decidePolicyForNavigationAction:decisionHandler:`,
    // `webView:didFinishNavigation:`, `webView:didFailProvisionalNavigation:withError:`,
    // `webView:didFailNavigation:withError:` and `webViewWebContentProcessDidTerminate:`.
    unsafe impl WKNavigationDelegate for MermaidDelegate {
        // Only the page itself (`about:blank` in the main frame); every other navigation (a
        // link, a redirect, a frame, a script setting `location`) is cancelled.
        #[unsafe(method(webView:decidePolicyForNavigationAction:decisionHandler:))]
        fn decide_policy(
            &self,
            _web_view: &WKWebView,
            action: &WKNavigationAction,
            decision: &block2::DynBlock<dyn Fn(WKNavigationActionPolicy)>,
        ) {
            // SAFETY: main thread (WebKit calls its delegates there); plain property reads.
            let (main, url) = unsafe {
                let main = action.targetFrame().is_some_and(|f| f.isMainFrame());
                let url = action
                    .request()
                    .URL()
                    .and_then(|u| u.absoluteString())
                    .map(|u| u.to_string())
                    .unwrap_or_default();
                (main, url)
            };
            decision.call((if mermaid::navigation_allowed(main, &url) {
                WKNavigationActionPolicy::Allow
            } else {
                WKNavigationActionPolicy::Cancel
            },));
        }

        #[unsafe(method(webView:didFinishNavigation:))]
        fn did_finish(&self, _web_view: &WKWebView, _navigation: Option<&WKNavigation>) {
            self.loaded(Ok(()));
        }

        #[unsafe(method(webView:didFailProvisionalNavigation:withError:))]
        fn did_fail_provisional(
            &self,
            _web_view: &WKWebView,
            _navigation: Option<&WKNavigation>,
            error: &NSError,
        ) {
            self.loaded(Err(error.localizedDescription().to_string()));
        }

        #[unsafe(method(webView:didFailNavigation:withError:))]
        fn did_fail(
            &self,
            _web_view: &WKWebView,
            _navigation: Option<&WKNavigation>,
            error: &NSError,
        ) {
            self.loaded(Err(error.localizedDescription().to_string()));
        }

        // The page's process died: whatever waits is answered by its dropped channel or its
        // deadline; that WebKit failure throws the view away.
        #[unsafe(method(webViewWebContentProcessDidTerminate:))]
        fn process_ended(&self, _web_view: &WKWebView) {
            self.loaded(Err("the renderer stopped".to_string()));
        }
    }
);

impl MermaidDelegate {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(DelegateState::default());
        // SAFETY: NSObject's `init` has this signature.
        unsafe { msg_send![super(this), init] }
    }

    /// Tell whoever waits for the page that it loaded (or didn't); later calls do nothing.
    fn loaded(&self, result: Result<(), String>) {
        if let Some(tx) = self.ivars().loaded.take() {
            let _ = tx.send(result);
        }
    }
}
