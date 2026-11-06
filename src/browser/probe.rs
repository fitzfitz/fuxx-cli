//! The hidden `fuxx __browser-probe` self-test: an off-screen `Browser` driven through every
//! operation, against pages it serves itself on 127.0.0.1; one JSON report line, then it quits.

use std::fmt::Debug;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use alacritty_terminal::vte::ansi::Rgb;
use gpui::{App, AsyncApp, BackgroundExecutor};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObjectProtocol, Sel};
use objc2::{AnyThread, MainThreadMarker, msg_send, sel};
use objc2_app_kit::{
    NSAppearanceCustomization, NSApplication, NSApplicationActivationPolicy, NSBitmapImageFileType,
    NSBitmapImageRep, NSDeviceRGBColorSpace, NSEvent, NSEventModifierFlags, NSEventType,
    NSGraphicsContext, NSImage, NSImageView, NSResponder, NSToolbarItem, NSView, NSWindow,
    NSWindowTitleVisibility, NSWindowToolbarStyle,
};
use objc2_foundation::{NSDictionary, NSPoint, NSRect, NSString, NSUserDefaults};
use objc2_web_kit::WKWebsiteDataStore;
use serde_json::{Value, json};

use super::{
    BarLook, Browser, CLOSED, EVAL_REFUSED_HINT, PageState, WINDOW_SIZE, png_size, within,
};
use crate::browser_js::{self, Snapshot};
use crate::settings;
use crate::theme::{self, Theme};
use crate::workspace_browser::bar_look;

/// The bar the probe's other browsers have (the bar's own checks make theirs from themes).
const PROBE_LOOK: BarLook = BarLook {
    dark: true,
    accent: Rgb {
        r: 0x89,
        g: 0xb4,
        b: 0xfa,
    },
    address_bar: true,
};

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

/// A page that replaces `JSON.stringify` with one that throws; its button counts clicks in the
/// title (and carries a ref already, as no snapshot can be answered there).
const PATCHED_PAGE: &str = r#"<!doctype html>
<html><head><meta charset="utf-8"><title>clicks 0</title>
<script>window.clicks = 0; JSON.stringify = () => { throw new Error('patched'); };</script></head>
<body><button data-fuxx-ref="e1" onclick="document.title = 'clicks ' + (++window.clicks)">Count</button></body></html>"#;

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
    // Keep-data off: the probe's pages leave nothing in the real, saved data store.
    let b = Browser::offscreen(mtm, false, &PROBE_LOOK);

    // The data stores. Off: every window shares the run's one in-memory store. On: the default
    // store, which keeps data on disk. Only looked at: the probe never deletes from it.
    let other = Browser::offscreen(mtm, false, &PROBE_LOOK);
    let kept = Browser::offscreen(mtm, true, &PROBE_LOOK);
    let (off_store, other_store, kept_store) = (store_of(&b), store_of(&other), store_of(&kept));
    // SAFETY: main thread (the marker); plain WebKit class method and property reads.
    let (default_store, off_persistent, on_persistent) = unsafe {
        (
            WKWebsiteDataStore::defaultDataStore(mtm),
            off_store.isPersistent(),
            kept_store.isPersistent(),
        )
    };
    let stores = json!({
        "off_persistent": off_persistent,
        "off_shared": std::ptr::eq(&*off_store, &*other_store),
        "off_is_default": std::ptr::eq(&*off_store, &*default_store),
        "on_persistent": on_persistent,
        "on_is_default": std::ptr::eq(&*kept_store, &*default_store),
    });
    check(
        stores["off_persistent"] == json!(false) && stores["off_is_default"] == json!(false),
        "keep-data off keeps nothing on disk",
        &stores,
    )?;
    check(
        stores["off_shared"] == json!(true),
        "keep-data off windows share one store",
        &stores,
    )?;
    check(
        stores["on_persistent"] == json!(true) && stores["on_is_default"] == json!(true),
        "keep-data on uses the default store",
        &stores,
    )?;
    other.close();
    kept.close();

    // The top bar.
    let bar = probe_bar(mtm, &ex, &base).await?;

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
    // A page the page itself asks for (a link click) clears that failure.
    let link = find_ref(&take_snapshot(&b, &ex).await?, "link", "page two")?;
    run(&b, &ex, &browser_js::click_js(&link)).await?;
    let followed_link = settle(&b, &ex, |s| !s.loading && s.url == two).await?;
    check(
        followed_link.error.is_none(),
        "a page-initiated load clears the failure",
        &followed_link,
    )?;
    // An address NSURL can't read: nothing loads, the page stays, and that is the error.
    b.load("http://[");
    let invalid = settle(&b, &ex, |s| !s.loading).await?;
    check(
        invalid.url == two
            && invalid.error.as_deref() == Some("the page failed to load: not a valid address"),
        "an invalid address",
        &invalid,
    )?;
    b.load(&home);
    settle(&b, &ex, |s| !s.loading && s.url == home).await?;
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
    let filled = run(&b, &ex, &browser_js::fill_js(&field, "probe")).await?;
    check(filled == json!({"ok": true}), "fill", &filled)?;
    let clicked = run(&b, &ex, &browser_js::click_js(&go)).await?;
    check(clicked == json!({"ok": true}), "click", &clicked)?;
    let heading = ask(&b, &ex, "document.getElementById('heading').textContent").await?;
    check(
        heading == json!("Hi, probe"),
        "heading after click",
        &heading,
    )?;
    let unknown = run(&b, &ex, &browser_js::click_js("e999")).await?;
    check(
        unknown == json!({"error": "unknown ref e999"}),
        "unknown ref",
        &unknown,
    )?;
    let read = run(&b, &ex, browser_js::read_js()).await?;
    check(
        read.as_str().is_some_and(|t| t.contains("Hi, probe")),
        "read",
        &read,
    )?;
    let wait_hit = run(
        &b,
        &ex,
        &browser_js::wait_check_js(Some("Hi, probe"), Some("#go")),
    )
    .await?;
    let wait_miss = run(&b, &ex, &browser_js::wait_check_js(Some("nowhere"), None)).await?;
    check(
        wait_hit == json!(true) && wait_miss == json!(false),
        "wait",
        &(&wait_hit, &wait_miss),
    )?;
    let not_a_key = run(&b, &ex, &browser_js::press_js("constructor")).await?;
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
    let followed = run(&b, &ex, &browser_js::click_js(&blank_link)).await?;
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

    // A page that breaks `JSON.stringify`: the answer fails after the click ran, and the click
    // never runs again (through `run_script` or `eval`).
    b.load(&format!("{base}/patched"));
    settle(&b, &ex, |s| !s.loading && s.title == "clicks 0").await?;
    let patched_run = within(
        b.run_script(&browser_js::click_js("e1")),
        PROBE_ANSWER_LIMIT,
        &ex,
    )
    .await;
    let after_run = settled_title(&b, &ex, "clicks 0").await?;
    let patched_eval = within(b.eval(&browser_js::click_js("e1")), PROBE_ANSWER_LIMIT, &ex).await;
    let after_eval = settled_title(&b, &ex, &after_run).await?;
    check(
        [&patched_run, &patched_eval]
            .iter()
            .all(|r| r.as_ref().is_err_and(|e| e.contains("patched")))
            && after_run == "clicks 1"
            && after_eval == "clicks 2",
        "each click runs once on a patched page",
        &(&patched_run, &after_run, &patched_eval, &after_eval),
    )?;

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
    let locked_click = run(&b, &ex, &browser_js::click_js(&press)).await?;
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

    // The Edit menu's selectors reach the web view, and select all selects the page.
    let selectors = [sel!(cut:), sel!(copy:), sel!(paste:), sel!(selectAll:)];
    let missing: Vec<&str> = selectors
        .iter()
        .filter(|s| !b.view.respondsToSelector(**s))
        .map(|s| s.name().to_str().unwrap_or("?"))
        .collect();
    check(missing.is_empty(), "edit selectors", &missing)?;
    // SAFETY: main thread; `selectAll:` takes any sender.
    let _: () = unsafe { msg_send![&*b.view, selectAll: None::<&AnyObject>] };
    let selected = settle_value(&b, &ex, "String(getSelection())", |v| {
        v.as_str().is_some_and(|t| t.contains("Locked page"))
    })
    .await?;
    // ⌘Z and ⇧⌘Z end at the window, which takes `undo:` and `redo:` (not at the app).
    let undo = b.window.respondsToSelector(sel!(undo:)) && b.window.respondsToSelector(sel!(redo:));
    check(undo, "an undo responder", &undo)?;
    // The window's own ⌘W closes it; other shortcuts aren't the window's.
    let keyed = Browser::offscreen(mtm, false, &PROBE_LOOK);
    // With the keyboard in the page, WebKit takes every ⌘ key for the page first (and sends
    // back what the page leaves); the window alone is asked here.
    keyed.window.makeFirstResponder(None);
    let quit = keyed
        .window
        .performKeyEquivalent(&*command_key(&keyed.window, "q")?);
    check(!quit && !keyed.is_closed(), "⌘Q is not the window's", &quit)?;
    // ⌘= ⌘- ⌘0 are swallowed (the terminals' font size is not this window's), not passed on.
    let mut font_keys = vec![];
    for key in ["=", "-", "0"] {
        let swallowed = keyed
            .window
            .performKeyEquivalent(&*command_key(&keyed.window, key)?);
        check(
            swallowed && !keyed.is_closed(),
            "⌘= ⌘- ⌘0 are swallowed",
            &(key, swallowed),
        )?;
        font_keys.push(swallowed);
    }
    let closed_by_key = keyed
        .window
        .performKeyEquivalent(&*command_key(&keyed.window, "w")?);
    check(
        closed_by_key && keyed.is_closed(),
        "⌘W closes the window",
        &closed_by_key,
    )?;
    keyed.close();

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
        "data_store": stores,
        "bar": bar,
        "failed_load": failed.error,
        "failed_load_over_a_page": { "url": failed_over.url, "error": failed_over.error },
        "error_after_a_link_click": followed_link.error,
        "invalid_address": invalid.error,
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
        "patched_json": {
            "run_script": patched_run.unwrap_err(),
            "eval": patched_eval.unwrap_err(),
            "titles": [after_run, after_eval],
        },
        "csp_locked": {
            "eval": locked_title,
            "snapshot_nodes": locked_snapshot.nodes.len(),
            "click": locked_click,
            "iife": locked_iife,
            "statements": locked_statements.unwrap_err(),
        },
        "edit": { "selected": selected, "undo_responder": undo, "cmd_w_closes": closed_by_key, "cmd_q_passes": !quit, "font_keys_swallowed": font_keys },
        "window_title": window_title,
        "user_closed": { "is_closed": true, "still_answers": after_close },
        "timeout": hang.unwrap_err(),
        "closed_mid_eval": [late_eval.unwrap_err(), late_screenshot.unwrap_err()],
        "ms": started.elapsed().as_millis() as u64,
    })
    .to_string())
}

/// The top bar of a fresh browser: its items, the address field (Enter, a refused address,
/// Escape, ⌘L), the buttons' states over two navigations and back, the loading line, the
/// light and dark looks, and the address field turned off and on again. Saves a picture of the
/// window in each look next to the screenshot.
async fn probe_bar(
    mtm: MainThreadMarker,
    ex: &BackgroundExecutor,
    base: &str,
) -> Result<Value, String> {
    let (home, x) = (format!("{base}/"), format!("{base}/x"));
    let dark = bar_look(&theme_named("Catppuccin Mocha"), true);
    let light = bar_look(&theme_named("Catppuccin Latte"), true);
    let b = Browser::offscreen(mtm, false, &dark);
    b.set_title("probe \u{2014} page two");
    let window = &b.window;

    let items = bar_items(&b);
    check(
        items
            == [
                "fuxx.back",
                "fuxx.forward",
                "fuxx.reload",
                "fuxx.address",
                "fuxx.mark",
            ],
        "the bar's items",
        &items,
    )?;
    // fuxx's mark: a picture, not a button; saved to look at (the window's own picture can't
    // show the toolbar's glass).
    let mark = mark_of(&b)?;
    check(
        !mark["clickable"].as_bool().unwrap_or(true)
            && !mark["bordered"].as_bool().unwrap_or(true)
            && mark["size"] == json!([18.0, 18.0]),
        "the mark",
        &mark,
    )?;
    let compact = window.toolbarStyle() == NSWindowToolbarStyle::UnifiedCompact;
    let title_hidden = window.titleVisibility() == NSWindowTitleVisibility::Hidden;
    check(
        compact && title_hidden,
        "a compact bar, no title",
        &(compact, title_hidden),
    )?;
    let content = window.contentView().map(|v| v.frame().size);
    check(content == Some(WINDOW_SIZE), "the content size", &content)?;
    check(
        page_has_keyboard(&b),
        "the page has the keyboard at first",
        &(),
    )?;
    // A window never shown can't be seen: its loading doesn't speed up the browser check.
    let seen = b.can_be_seen();
    check(!seen, "a hidden window can't be seen", &seen)?;
    // A copy the toolbar asks for but won't show (as for a customisation palette) is left
    // alone: the bar's buttons and its field stay the ones in the bar.
    palette_copies(&b)?;
    let field_in_bar = bar_item(&b, "fuxx.address")?.view().is_some_and(|v| {
        let field: &NSView = b.bar.field();
        std::ptr::eq(&*v, field)
    });
    check(field_in_bar, "the field stays in the bar", &())?;
    let fresh = buttons_now(&b);
    check(
        fresh == (false, false, "Reload".into()),
        "fresh buttons",
        &fresh,
    )?;

    // Enter in the field opens what was typed, as agents' addresses are checked.
    let host = base.trim_start_matches("http://");
    type_in_field(&b, &format!("{host}/"), sel!(insertNewline:))?;
    let after_enter = (
        b.state().loading,
        buttons_now(&b).2,
        !b.bar.line().isHidden(),
    );
    let entered = settle(&b, ex, |s| !s.loading && s.url == home).await?;
    check(
        page_has_keyboard(&b),
        "Enter gives the page the keyboard",
        &(),
    )?;
    b.refresh_bar();
    let field_after = b.bar.field().stringValue().to_string();
    check(
        field_after == home,
        "the field shows the page",
        &field_after,
    )?;
    let line_idle = b.bar.line().isHidden();
    check(line_idle, "no loading line once loaded", &())?;
    type_in_field(&b, &format!("{host}/x"), sel!(insertNewline:))?;
    settle(&b, ex, |s| !s.loading && s.url == x).await?;
    b.refresh_bar();
    let two_pages = buttons_now(&b);
    check(
        two_pages == (true, false, "Reload".into()),
        "after two pages",
        &two_pages,
    )?;
    press_item(&b, "fuxx.back")?;
    settle(&b, ex, |s| !s.loading && s.url == home).await?;
    b.refresh_bar();
    let went_back = buttons_now(&b);
    check(
        went_back == (false, true, "Reload".into()),
        "after back",
        &went_back,
    )?;
    press_item(&b, "fuxx.forward")?;
    settle(&b, ex, |s| !s.loading && s.url == x).await?;

    // A refused address loads nothing: it stays in the field, red, with why as its tooltip.
    type_in_field(&b, "javascript:alert(1)", sel!(insertNewline:))?;
    let field = b.bar.field();
    let refused = json!({
        "url": b.state().url,
        "text": field.stringValue().to_string(),
        "tooltip": field.toolTip().map(|t| t.to_string()),
        "still_typing": field.currentEditor().is_some(),
    });
    check(
        refused["url"] == json!(x)
            && refused["still_typing"] == json!(true)
            && refused["tooltip"]
                .as_str()
                .is_some_and(|t| t.contains("http")),
        "a javascript: address is refused",
        &refused,
    )?;
    // Escape puts the page's address back and gives the page the keyboard.
    type_in_field(&b, "half-typed", sel!(cancelOperation:))?;
    let escaped = (
        field.stringValue().to_string(),
        field.toolTip().is_none(),
        page_has_keyboard(&b),
    );
    check(escaped == (x.clone(), true, true), "Escape", &escaped)?;

    // ⌘L: the field takes the keyboard with all of it selected.
    let cmd_l = window.performKeyEquivalent(&*command_key(window, "l")?);
    let selected = field.currentEditor().map(|e| e.selectedRange());
    check(
        cmd_l && selected.is_some_and(|r| r.location == 0 && r.length == x.chars().count()),
        "⌘L focuses the field",
        &(cmd_l, selected),
    )?;
    // The font-size keys are still swallowed with the field focused.
    let font_in_field = window.performKeyEquivalent(&*command_key(window, "=")?);
    check(font_in_field, "⌘= in the field is swallowed", &())?;
    type_in_field(&b, "", sel!(cancelOperation:))?;

    // The page's light or dark follows macOS, not the fuxx theme (only the bar does): pages
    // and agents see what they saw before the bar.
    let system_dark = system_is_dark(mtm);
    let mut page_dark = vec![];
    for look in [&light, &dark] {
        b.set_look(look);
        b.refresh_bar();
        let dark_page = settle_value(
            &b,
            ex,
            "matchMedia('(prefers-color-scheme: dark)').matches",
            |v| *v == json!(system_dark),
        )
        .await?;
        page_dark.push(dark_page);
    }

    // Light and dark follow the fuxx theme.
    let looks = [&dark, &light].map(|look| {
        b.set_look(look);
        window
            .appearance()
            .map(|a| a.name().to_string())
            .unwrap_or_default()
    });
    check(
        looks == ["NSAppearanceNameDarkAqua", "NSAppearanceNameAqua"],
        "light and dark",
        &looks,
    )?;
    let dir = std::env::temp_dir().join("fuxx-browser-probe");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut pictures = vec![];
    for (name, look) in [("bar-dark.png", &dark), ("bar-light.png", &light)] {
        b.set_look(look);
        let path = dir.join(name);
        std::fs::write(&path, window_png(window)?)
            .map_err(|e| format!("couldn't save {}: {e}", path.display()))?;
        pictures.push(path.display().to_string());
    }

    // The address field off: the buttons stay, the title shows, ⌘L isn't the window's.
    let off = BarLook {
        address_bar: false,
        ..dark
    };
    b.set_look(&off);
    let items_off = bar_items(&b);
    let title_shown = window.titleVisibility() == NSWindowTitleVisibility::Visible;
    // ⌘L goes on to the page (WebKit takes ⌘ keys for the page first), not to the field.
    window.performKeyEquivalent(&*command_key(window, "l")?);
    let cmd_l_off = field.currentEditor().is_some();
    check(
        items_off == ["fuxx.back", "fuxx.forward", "fuxx.reload", "fuxx.mark"]
            && title_shown
            && !cmd_l_off
            && page_has_keyboard(&b),
        "the address field off",
        &(&items_off, title_shown, cmd_l_off),
    )?;
    let path = dir.join("bar-off.png");
    std::fs::write(&path, window_png(window)?)
        .map_err(|e| format!("couldn't save {}: {e}", path.display()))?;
    pictures.push(path.display().to_string());
    b.set_look(&dark);
    let items_on = bar_items(&b);
    check(items_on == items, "the address field back", &items_on)?;
    // ⌘W still closes the window with the bar there.
    let closed = window.performKeyEquivalent(&*command_key(window, "w")?);
    check(closed && b.is_closed(), "⌘W with the bar", &closed)?;
    b.close();

    Ok(json!({
        "items": items,
        "mark": mark,
        "compact": compact,
        "title_hidden": title_hidden,
        "fresh_buttons": fresh,
        "enter": { "url": entered.url, "right_after": after_enter, "field": field_after },
        "after_two_pages": two_pages,
        "after_back": went_back,
        "refused": refused,
        "escape": escaped,
        "cmd_l": { "handled": cmd_l, "selected": selected.map(|r| [r.location, r.length]) },
        "font_key_in_field": font_in_field,
        "appearance": looks,
        "page_follows_macos": { "system_dark": system_dark, "page_dark_with_light_then_dark_bar": page_dark },
        "field_in_bar_after_palette_copies": field_in_bar,
        "hidden_window_can_be_seen": seen,
        "address_bar_off": { "items": items_off, "title_shown": title_shown, "cmd_l_focuses": cmd_l_off },
        "address_bar_on_again": items_on,
        "cmd_w": closed,
        "pictures": pictures,
    }))
}

/// Ask `b`'s toolbar delegate for copies of a button and the field that won't be put in the
/// bar (`willBeInsertedIntoToolbar:` NO).
fn palette_copies(b: &Browser) -> Result<(), String> {
    let toolbar = b.window.toolbar().ok_or("no toolbar")?;
    let delegate = toolbar.delegate().ok_or("no toolbar delegate")?;
    for id in ["fuxx.back", "fuxx.address", "fuxx.mark"] {
        // SAFETY: main thread; the toolbar delegate's own method, with its argument types.
        let _: Option<Retained<NSToolbarItem>> = unsafe {
            msg_send![
                &*delegate,
                toolbar: &*toolbar,
                itemForItemIdentifier: &*NSString::from_str(id),
                willBeInsertedIntoToolbar: false
            ]
        };
    }
    Ok(())
}

/// The bar's mark: whether it can be clicked (an action) or has a bezel, its picture's size in
/// points, and where a PNG of that picture (at 2×) was saved.
fn mark_of(b: &Browser) -> Result<Value, String> {
    let item = bar_item(b, "fuxx.mark")?;
    let view = item.view().ok_or("the mark has no view")?;
    let image_view = view
        .downcast::<NSImageView>()
        .map_err(|_| "the mark isn't a picture")?;
    let image = image_view.image().ok_or("the mark has no picture")?;
    let size = image.size();
    let path = std::env::temp_dir()
        .join("fuxx-browser-probe")
        .join("mark@2x.png");
    std::fs::create_dir_all(path.parent().unwrap_or(&path)).map_err(|e| e.to_string())?;
    std::fs::write(&path, image_png_2x(&image)?)
        .map_err(|e| format!("couldn't save {}: {e}", path.display()))?;
    Ok(json!({
        "clickable": item.action().is_some() || image_view.isEnabled() && image_view.action().is_some(),
        "bordered": item.isBordered(),
        "size": [size.width, size.height],
        "png": path.display().to_string(),
    }))
}

/// A PNG of `image` drawn at twice its size in points, as on a Retina screen.
fn image_png_2x(image: &NSImage) -> Result<Vec<u8>, String> {
    let size = image.size();
    let (width, height) = ((size.width * 2.0) as isize, (size.height * 2.0) as isize);
    // SAFETY: a null planes pointer makes the bitmap allocate its own pixels; 8-bit RGBA with
    // the row and pixel sizes worked out by AppKit (0).
    let bitmap = unsafe {
        NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(),
            std::ptr::null_mut(),
            width,
            height,
            8,
            4,
            true,
            false,
            NSDeviceRGBColorSpace,
            0,
            0,
        )
    }
    .ok_or("no bitmap for the mark")?;
    // Points over the bitmap's pixels: 2 pixels a point.
    bitmap.setSize(size);
    let context = NSGraphicsContext::graphicsContextWithBitmapImageRep(&bitmap)
        .ok_or("no drawing context for the mark")?;
    let previous = NSGraphicsContext::currentContext();
    NSGraphicsContext::setCurrentContext(Some(&context));
    image.drawInRect(NSRect::new(NSPoint::new(0.0, 0.0), size));
    context.flushGraphics();
    NSGraphicsContext::setCurrentContext(previous.as_deref());
    // SAFETY: an empty properties dictionary is valid for every file type.
    let png = unsafe {
        bitmap.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new())
    }
    .ok_or("the mark couldn't be encoded as PNG")?;
    Ok(png.to_vec())
}

/// Whether macOS is in dark mode, from its own setting (not from fuxx's code).
fn system_is_dark(_mtm: MainThreadMarker) -> bool {
    let style = NSUserDefaults::standardUserDefaults()
        .stringForKey(&NSString::from_str("AppleInterfaceStyle"));
    style.is_some_and(|s| s.to_string() == "Dark")
}

/// A built-in theme by name, as `theme = <name>` gives it.
fn theme_named(name: &str) -> Theme {
    theme::resolve(&settings::parse(&format!("theme = {name}")), true, &|n| {
        theme::builtin_theme(n).map(str::to_string)
    })
}

/// The identifiers of the items in `b`'s bar, in order.
fn bar_items(b: &Browser) -> Vec<String> {
    b.window
        .toolbar()
        .map(|t| {
            t.items()
                .iter()
                .map(|i| i.itemIdentifier().to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// The bar's item `identifier`.
fn bar_item(b: &Browser, identifier: &str) -> Result<Retained<NSToolbarItem>, String> {
    b.window
        .toolbar()
        .and_then(|t| {
            t.items()
                .iter()
                .find(|i| i.itemIdentifier().to_string() == identifier)
        })
        .ok_or_else(|| format!("no {identifier} in the bar"))
}

/// Back enabled, forward enabled, the reload button's label.
fn buttons_now(b: &Browser) -> (bool, bool, String) {
    let enabled = |id| bar_item(b, id).is_ok_and(|i| i.isEnabled());
    let reload = bar_item(b, "fuxx.reload").map_or_else(|e| e, |i| i.label().to_string());
    (enabled("fuxx.back"), enabled("fuxx.forward"), reload)
}

/// Click the bar's button `identifier`, as AppKit does: its action to its target.
fn press_item(b: &Browser, identifier: &str) -> Result<(), String> {
    let item = bar_item(b, identifier)?;
    let app = NSApplication::sharedApplication(MainThreadMarker::from(&*b.window));
    // SAFETY: main thread; the item's own action and target, with the item as the sender.
    let sent = unsafe {
        app.sendAction_to_from(
            item.action().ok_or("no action")?,
            item.target().as_deref(),
            Some(&item),
        )
    };
    check(sent, "the button's action was sent", &identifier)
}

/// Type `text` into the address field (it takes the keyboard first), then the key whose
/// command is `command` (`insertNewline:` for Enter, `cancelOperation:` for Escape).
fn type_in_field(b: &Browser, text: &str, command: Sel) -> Result<(), String> {
    let field = b.bar.field();
    if field.currentEditor().is_none() {
        b.window.makeFirstResponder(Some(field));
    }
    let editor = field
        .currentEditor()
        .ok_or("the field didn't take the keyboard")?;
    editor.setString(&NSString::from_str(text));
    // SAFETY: main thread; the field editor runs the command as a key would.
    let _: () = unsafe { msg_send![&*editor, doCommandBySelector: command] };
    Ok(())
}

/// Whether the page (not the address field) has the keyboard.
fn page_has_keyboard(b: &Browser) -> bool {
    let page: &NSResponder = &b.view;
    b.window
        .firstResponder()
        .is_some_and(|r| std::ptr::eq(&*r, page))
}

/// A PNG of the whole window, title bar and all, as it would draw on screen.
fn window_png(window: &NSWindow) -> Result<Vec<u8>, String> {
    // SAFETY: main thread; the content view's parent is the window's frame view, which draws
    // the title bar and toolbar.
    let frame_view = unsafe { window.contentView().and_then(|v| v.superview()) }
        .ok_or("the window has no frame view")?;
    let bounds = frame_view.bounds();
    frame_view.layoutSubtreeIfNeeded();
    let bitmap = frame_view
        .bitmapImageRepForCachingDisplayInRect(bounds)
        .ok_or("no bitmap for the window")?;
    frame_view.cacheDisplayInRect_toBitmapImageRep(bounds, &bitmap);
    // SAFETY: an empty properties dictionary is valid for every file type.
    let png = unsafe {
        bitmap.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new())
    }
    .ok_or("the window couldn't be encoded as PNG")?;
    Ok(png.to_vec())
}

/// `Err` naming `what` and showing `got` unless `ok`.
fn check(ok: bool, what: &str, got: &impl Debug) -> Result<(), String> {
    if ok {
        Ok(())
    } else {
        Err(format!("{what}: got {got:?}"))
    }
}

/// The data store `b`'s web view was made with.
fn store_of(b: &Browser) -> Retained<WKWebsiteDataStore> {
    // SAFETY: main thread (a `Browser` never leaves it); plain property reads.
    unsafe { b.view.configuration().websiteDataStore() }
}

/// A ⌘-`key` key-down event for `window`.
fn command_key(window: &NSWindow, key: &str) -> Result<Retained<NSEvent>, String> {
    let key = NSString::from_str(key);
    NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
        NSEventType::KeyDown,
        NSPoint::new(0.0, 0.0),
        NSEventModifierFlags::Command,
        0.0,
        window.windowNumber(),
        None,
        &key,
        &key,
        false,
        0,
    )
    .ok_or_else(|| "no key event".to_string())
}

/// `js`'s value once `done` says so (polled), within the settle limit.
async fn settle_value(
    b: &Browser,
    ex: &BackgroundExecutor,
    js: &str,
    done: impl Fn(&Value) -> bool,
) -> Result<Value, String> {
    let deadline = Instant::now() + SETTLE_LIMIT;
    loop {
        let value = ask(b, ex, js).await?;
        if done(&value) {
            return Ok(value);
        }
        if Instant::now() > deadline {
            return Err(format!("{js}: still {value}"));
        }
        ex.timer(LOAD_POLL).await;
    }
}

/// `js`'s value within the probe's limit.
async fn ask(b: &Browser, ex: &BackgroundExecutor, js: &str) -> Result<Value, String> {
    within(b.eval(js), PROBE_ANSWER_LIMIT, ex).await
}

/// One of fuxx's own page scripts' value within the probe's limit.
async fn run(b: &Browser, ex: &BackgroundExecutor, script: &str) -> Result<Value, String> {
    within(b.run_script(script), PROBE_ANSWER_LIMIT, ex).await
}

/// The page's snapshot.
async fn take_snapshot(b: &Browser, ex: &BackgroundExecutor) -> Result<Snapshot, String> {
    let value = run(b, ex, browser_js::snapshot_js()).await?;
    serde_json::from_value(value).map_err(|e| format!("the snapshot didn't parse: {e}"))
}

/// The window title once it is no longer `before` and has stayed put for a moment (so a second
/// change would show).
async fn settled_title(
    b: &Browser,
    ex: &BackgroundExecutor,
    before: &str,
) -> Result<String, String> {
    settle(b, ex, |s| s.title != before).await?;
    ex.timer(Duration::from_millis(300)).await;
    Ok(b.state().title)
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
        "/patched" => ("200 OK", "", PATCHED_PAGE),
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
