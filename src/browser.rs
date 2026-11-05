//! The agent browser (M10c): a `WKWebView` in its own `NSWindow`, driven through objc2.
//!
//! First cut: the hidden `fuxx __browser-probe` self-test. It runs GPUI's run loop (WebKit needs
//! a main run loop) with no fuxx window, sessions, socket or background service, drives an
//! off-screen web view end to end, prints one JSON line and quits.
//!
//! Every WebKit and AppKit call runs on the main thread. WebKit answers through completion
//! blocks, also on the main thread; each answer goes through a channel to a GPUI task, so
//! nothing ever blocks the run loop waiting for it.

use std::cell::Cell;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use block2::RcBlock;
use futures::channel::oneshot;
use gpui::{App, AsyncApp};
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSBitmapImageFileType,
    NSBitmapImageRep, NSImage, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{NSDictionary, NSError, NSPoint, NSRect, NSSize, NSString};
use objc2_web_kit::{WKWebView, WKWebViewConfiguration};
use serde_json::json;

/// The probe gives up and exits with code 2 after this long, whatever is still pending.
const PROBE_DEADLINE: Duration = Duration::from_secs(15);
/// How often the probe checks whether its test page has loaded.
const LOAD_POLL: Duration = Duration::from_millis(50);

/// The probe's test page: a heading, a link, a text field and a button that changes the heading.
const PROBE_PAGE: &str = r#"<!doctype html>
<html><head><meta charset="utf-8"><title>fuxx probe</title></head>
<body>
<h1 id="heading">Hello from fuxx</h1>
<p><a id="link" href="https://example.com/">a link</a></p>
<input id="name" type="text" placeholder="your name">
<button id="go" onclick="document.getElementById('heading').textContent = 'Hi, ' + document.getElementById('name').value">Go</button>
</body></html>"#;

/// `fuxx __browser-probe`: never returns. Exits 0 after printing
/// `{"eval":…, "clicked":…, "png_bytes":N, "ms":…}` (`ms`: from making the web view to the
/// report), 1 with a message on stderr if a step fails, 2 if it takes longer than 15 s.
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

/// Load the test page, script it and capture it; the JSON report line.
async fn probe(mtm: MainThreadMarker, cx: &mut AsyncApp) -> Result<String, String> {
    let started = Instant::now();
    let (_window, view) = offscreen_web_view(mtm, 800.0, 600.0);
    // SAFETY: main thread; the view outlives the navigation (both live until the probe ends).
    unsafe { view.loadHTMLString_baseURL(&NSString::from_str(PROBE_PAGE), None) };

    // Until the page has loaded, scripts run in the blank page that was there before it.
    let ready = "document.getElementById('heading') ? document.readyState : 'loading'";
    while eval(&view, ready).await? != "complete" {
        cx.background_executor().timer(LOAD_POLL).await;
    }
    let title = eval(&view, "document.title").await?;
    eval(
        &view,
        "(() => { const f = document.getElementById('name'); f.value = 'probe'; \
         f.dispatchEvent(new Event('input', { bubbles: true })); \
         document.getElementById('go').click(); return 'ok'; })()",
    )
    .await?;
    let heading = eval(&view, "document.getElementById('heading').textContent").await?;
    let png = snapshot_png(&view).await?;

    let dir = std::env::temp_dir().join("fuxx-browser-probe");
    let path: PathBuf = dir.join("snapshot.png");
    std::fs::create_dir_all(&dir)
        .and_then(|_| std::fs::write(&path, &png))
        .map_err(|e| format!("couldn't save the snapshot to {}: {e}", path.display()))?;
    eprintln!("[fuxx] browser probe: snapshot saved to {}", path.display());

    Ok(json!({
        "eval": title,
        "clicked": heading,
        "png_bytes": png.len(),
        "ms": started.elapsed().as_millis() as u64,
    })
    .to_string())
}

/// A borderless window of `width` × `height` points holding a web view, never ordered on
/// screen. The window must be kept alive as long as the view is used.
fn offscreen_web_view(
    mtm: MainThreadMarker,
    width: f64,
    height: f64,
) -> (Retained<NSWindow>, Retained<WKWebView>) {
    let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(width, height));
    // SAFETY: main thread; plain AppKit/WebKit initialisers with valid arguments.
    unsafe {
        let window = NSWindow::initWithContentRect_styleMask_backing_defer(
            mtm.alloc(),
            frame,
            NSWindowStyleMask::Borderless,
            NSBackingStoreType::Buffered,
            false,
        );
        // Owned by `Retained`, not by AppKit's close-releases-it rule.
        window.setReleasedWhenClosed(false);
        let config = WKWebViewConfiguration::new(mtm);
        let view = WKWebView::initWithFrame_configuration(mtm.alloc(), frame, &config);
        window.setContentView(Some(&view));
        (window, view)
    }
}

/// Run `script` in the page; its result, which must be a string.
async fn eval(view: &WKWebView, script: &str) -> Result<String, String> {
    let (tx, rx) = oneshot::channel();
    let tx = Cell::new(Some(tx));
    let block = RcBlock::new(move |result: *mut AnyObject, error: *mut NSError| {
        // SAFETY: WebKit passes a valid object or nil for each.
        let answer = match unsafe { (result.as_ref(), error.as_ref()) } {
            (_, Some(error)) => Err(format!(
                "the script failed: {}",
                error.localizedDescription()
            )),
            (Some(value), None) => value
                .downcast_ref::<NSString>()
                .map(|s| s.to_string())
                .ok_or_else(|| "the script answered with something other than text".to_string()),
            (None, None) => Err("the script answered with nothing".to_string()),
        };
        if let Some(tx) = tx.take() {
            let _ = tx.send(answer);
        }
    });
    // SAFETY: main thread; WebKit copies the block and calls it once, on the main thread.
    unsafe { view.evaluateJavaScript_completionHandler(&NSString::from_str(script), Some(&block)) };
    rx.await
        .map_err(|_| "WebKit dropped the script without answering".to_string())?
}

/// The visible page as PNG bytes.
async fn snapshot_png(view: &WKWebView) -> Result<Vec<u8>, String> {
    let (tx, rx) = oneshot::channel();
    let tx = Cell::new(Some(tx));
    let block = RcBlock::new(move |image: *mut NSImage, error: *mut NSError| {
        // SAFETY: WebKit passes a valid object or nil for each.
        let answer = match unsafe { (image.as_ref(), error.as_ref()) } {
            (_, Some(error)) => Err(format!(
                "the snapshot failed: {}",
                error.localizedDescription()
            )),
            (Some(image), None) => png_bytes(image),
            (None, None) => Err("the snapshot came back empty".to_string()),
        };
        if let Some(tx) = tx.take() {
            let _ = tx.send(answer);
        }
    });
    // SAFETY: main thread; no configuration means the whole visible view.
    unsafe { view.takeSnapshotWithConfiguration_completionHandler(None, &block) };
    rx.await
        .map_err(|_| "WebKit dropped the snapshot without answering".to_string())?
}

/// Encode an image as PNG (through its TIFF form, which every `NSImage` has).
fn png_bytes(image: &NSImage) -> Result<Vec<u8>, String> {
    let tiff = image
        .TIFFRepresentation()
        .ok_or("the snapshot has no bitmap form")?;
    let bitmap = NSBitmapImageRep::imageRepWithData(&tiff).ok_or("the snapshot isn't a bitmap")?;
    // SAFETY: an empty properties dictionary is valid for every file type.
    let png = unsafe {
        bitmap.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new())
    }
    .ok_or("the snapshot couldn't be encoded as PNG")?;
    Ok(png.to_vec())
}
