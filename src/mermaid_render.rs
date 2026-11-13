//! Rendering Mermaid diagrams (M11ac Part 2): one hidden `WKWebView`, made on first use, loads
//! `mermaid::page_html` (the bundled mermaid.js, no base URL, a policy that lets nothing be
//! fetched), draws each diagram with `mermaid.render` and snapshots it to a PNG (at 2× unless
//! huge) that the Preview shows as an image.
//!
//! Everything here runs on the main thread, never blocking it: each WebKit answer comes back
//! through a completion block into a channel a GPUI task awaits. One diagram renders at a time
//! (the page is shared); the rest wait in a queue. A diagram gets 5 s (the page's first load
//! 10 s more); past that, or when WebKit itself fails, the web view is thrown away and the next
//! diagram gets a fresh one.
//!
//! No network, ever: the page has no base URL and a Content-Security-Policy of
//! `default-src 'none'` (inline script and styles, `data:` images and fonts only); the
//! navigation delegate allows the one load of the page and cancels every other navigation;
//! the data store is a fresh non-persistent one; no UI delegate, so no new windows.
//!
//! `Mermaids` (a GPUI global) holds the cache (`mermaid::Lru`, 64 entries: an image or an
//! error per key) and the queue; `Mermaids::shown` is what a Preview asks each frame.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use block2::RcBlock;
use futures::channel::oneshot;
use gpui::{App, AsyncApp, BackgroundExecutor, Global, Image, ImageFormat};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{NSBackingStoreType, NSImage, NSWindow, NSWindowStyleMask};
use objc2_foundation::{
    NSError, NSNumber, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString,
};
use objc2_web_kit::{
    WKContentWorld, WKNavigation, WKNavigationAction, WKNavigationActionPolicy,
    WKNavigationDelegate, WKSnapshotConfiguration, WKWebView, WKWebViewConfiguration,
    WKWebsiteDataStore,
};

use crate::browser::{exception_message, png_bytes, within};
use crate::mermaid::{self, CACHE_SIZE, Key, Lru};

/// How long one diagram may take (render and snapshot), and the page's first load.
pub const RENDER_LIMIT: Duration = Duration::from_secs(5);
const LOAD_LIMIT: Duration = Duration::from_secs(10);
/// The web view's size before a diagram resizes it (points).
const VIEW_SIZE: NSSize = NSSize::new(1200.0, 900.0);

/// A diagram as the cache keeps it: its PNG and its natural size in points, or why it failed.
#[derive(Clone)]
pub enum Entry {
    Ready { image: Arc<Image>, w: f32, h: f32 },
    Failed(String),
}

/// What to do once a diagram is ready (or failed): the Preview that asked draws again. Kept
/// once per asker (a Preview asks each frame while it waits).
type Waiter = (u64, Box<dyn FnOnce(&mut App)>);

/// The diagrams: the cache, the ones waiting and their waiters, and the renderer.
pub struct Mermaids {
    cache: Lru<Key, Entry>,
    waiting: HashMap<Key, Vec<Waiter>>,
    queue: VecDeque<(Key, String)>,
    running: bool,
    renderer: Option<Rc<Renderer>>,
}

impl Default for Mermaids {
    fn default() -> Self {
        Mermaids {
            cache: Lru::new(CACHE_SIZE),
            waiting: HashMap::new(),
            queue: VecDeque::new(),
            running: false,
            renderer: None,
        }
    }
}

impl Global for Mermaids {}

impl Mermaids {
    /// The diagram for `key` if it is ready (or failed). Otherwise it is queued (once) with
    /// `source`, and `then` runs when it is (once per `who`, the asker's id): `None` for now
    /// (the Preview shows the code). Only the cache is read when it is there: scrolling never
    /// renders again.
    pub fn shown(
        key: Key,
        source: &str,
        who: u64,
        then: impl FnOnce(&mut App) + 'static,
        cx: &mut App,
    ) -> Option<Entry> {
        let this = cx.default_global::<Mermaids>();
        if let Some(entry) = this.cache.get(&key) {
            return Some(entry.clone());
        }
        match this.waiting.get_mut(&key) {
            Some(waiters) if waiters.iter().any(|(w, _)| *w == who) => {}
            Some(waiters) => waiters.push((who, Box::new(then))),
            None => {
                this.waiting.insert(key, vec![(who, Box::new(then))]);
                this.queue.push_back((key, source.to_string()));
            }
        }
        if !this.running {
            this.running = true;
            cx.spawn(async move |cx: &mut AsyncApp| pump(cx).await)
                .detach();
        }
        None
    }

    /// Keep `entry` for `key` and tell its waiters. A diagram dropped from the cache lets go of
    /// its decoded image too (GPUI keeps it until told).
    fn finish(key: Key, entry: Entry, cx: &mut App) {
        let this = cx.default_global::<Mermaids>();
        let dropped = this.cache.put(key, entry);
        let waiters = this.waiting.remove(&key).unwrap_or_default();
        for (_, old) in dropped {
            if let Entry::Ready { image, .. } = old {
                image.remove_asset(cx);
            }
        }
        for (_, then) in waiters {
            then(cx);
        }
    }
}

#[cfg(feature = "ui-shots")]
impl Mermaids {
    /// How many diagrams (and errors) are cached, and how many wait to render.
    pub fn counts(cx: &mut App) -> (usize, usize) {
        let this = cx.default_global::<Mermaids>();
        (this.cache.len(), this.waiting.len())
    }
}

/// Render the queued diagrams one at a time until none is left.
async fn pump(cx: &mut AsyncApp) {
    loop {
        let next = cx.update(|cx| {
            let this = cx.default_global::<Mermaids>();
            let job = this.queue.pop_front();
            if job.is_none() {
                this.running = false;
            }
            job.map(|job| (job, this.renderer.clone()))
        });
        let Some(((key, source), renderer)) = next else {
            return;
        };
        let ex = cx.background_executor().clone();
        let (entry, keep) = match render(renderer, &source, key.dark, &ex).await {
            Ok((renderer, image, w, h)) => (Entry::Ready { image, w, h }, Some(renderer)),
            Err(Failure::Mermaid(e, renderer)) => (Entry::Failed(e), Some(renderer)),
            Err(Failure::WebKit(e)) => (Entry::Failed(e), None),
        };
        if let Entry::Failed(e) = &entry {
            eprintln!("[fuxx] mermaid: {e}");
        }
        let entry = match entry {
            Entry::Failed(e) => Entry::Failed(mermaid::short_error(&e)),
            ready => ready,
        };
        cx.update(|cx| {
            cx.default_global::<Mermaids>().renderer = keep;
            Mermaids::finish(key, entry, cx);
        });
    }
}

/// Why a diagram didn't render: Mermaid's own error (the page is fine, kept), or WebKit's (a
/// timeout, a crash: the web view is thrown away).
enum Failure {
    Mermaid(String, Rc<Renderer>),
    WebKit(String),
}

/// Draw `source` in `renderer` (made, and its page loaded, if there is none): the PNG and
/// the diagram's size in points.
async fn render(
    renderer: Option<Rc<Renderer>>,
    source: &str,
    dark: bool,
    ex: &BackgroundExecutor,
) -> Result<(Rc<Renderer>, Arc<Image>, f32, f32), Failure> {
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
    let work = async {
        let answer = within(
            renderer.call(&mermaid::render_script(source, dark)),
            RENDER_LIMIT,
            ex,
        )
        .await
        .map_err(Failure::WebKit)?;
        let (w, h) = match mermaid::read_answer(&answer) {
            Ok(size) => size,
            Err(e) => return Err(Failure::Mermaid(e, renderer.clone())),
        };
        let png = within(renderer.snapshot(w, h), RENDER_LIMIT, ex)
            .await
            .map_err(Failure::WebKit)?;
        Ok((w, h, png))
    };
    let (w, h, png) = work.await?;
    let image = Arc::new(Image::from_bytes(ImageFormat::Png, png));
    Ok((renderer, image, w as f32, h as f32))
}

/// The hidden web view, its never-shown window, and its navigation delegate.
pub struct Renderer {
    window: Retained<NSWindow>,
    view: Retained<WKWebView>,
    delegate: Retained<MermaidDelegate>,
}

impl Renderer {
    /// A web view in a window that is never shown, loading the page; the receiver answers when
    /// it has loaded (or failed to).
    fn new(dark: bool, mtm: MainThreadMarker) -> (Renderer, oneshot::Receiver<Result<(), String>>) {
        let frame = NSRect::new(NSPoint::new(0.0, 0.0), VIEW_SIZE);
        // SAFETY: main thread (the marker); AppKit's and WebKit's own initialisers and setters,
        // with valid arguments. The window is owned by `Retained`, not by close-releases-it.
        let (window, view) = unsafe {
            let window = NSWindow::initWithContentRect_styleMask_backing_defer(
                mtm.alloc(),
                frame,
                NSWindowStyleMask::Borderless,
                NSBackingStoreType::Buffered,
                false,
            );
            window.setReleasedWhenClosed(false);
            window.setExcludedFromWindowsMenu(true);
            let config = WKWebViewConfiguration::new(mtm);
            config.setWebsiteDataStore(&WKWebsiteDataStore::nonPersistentDataStore(mtm));
            let view = WKWebView::initWithFrame_configuration(mtm.alloc(), frame, &config);
            window.setContentView(Some(&view));
            (window, view)
        };
        let delegate = MermaidDelegate::new(mtm);
        let (tx, rx) = oneshot::channel();
        delegate.ivars().loaded.replace(Some(tx));
        delegate.ivars().allow_next.set(true);
        // SAFETY: main thread; the delegate lives as long as the `Renderer`, which clears the
        // link before it goes. No base URL: the page is `about:blank`'s origin, with nothing
        // relative to resolve.
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

/// The delegate's state: whether the next navigation (the page's own load) may happen, and
/// who waits for the page to load.
#[derive(Default)]
pub struct DelegateState {
    allow_next: Cell<bool>,
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
        // Only the page's own load; every other navigation (a link, a redirect, a frame, a
        // script setting `location`) is cancelled.
        #[unsafe(method(webView:decidePolicyForNavigationAction:decisionHandler:))]
        fn decide_policy(
            &self,
            _web_view: &WKWebView,
            _action: &WKNavigationAction,
            decision: &block2::DynBlock<dyn Fn(WKNavigationActionPolicy)>,
        ) {
            let allow = self.ivars().allow_next.replace(false);
            decision.call((if allow {
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
        // timer; the next diagram's failure throws the view away.
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
