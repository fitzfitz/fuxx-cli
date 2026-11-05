//! The hidden `fuxx __browser-probe` self-test: an off-screen `Browser` driven through every
//! operation, against pages it serves itself on 127.0.0.1; one JSON report line, then it quits.

use std::fmt::Debug;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use gpui::{App, AsyncApp, BackgroundExecutor};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObjectProtocol};
use objc2::{MainThreadMarker, msg_send, sel};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSEvent, NSEventModifierFlags, NSEventType,
    NSWindow,
};
use objc2_foundation::{NSPoint, NSString};
use serde_json::{Value, json};

use super::{Browser, CLOSED, EVAL_REFUSED_HINT, PageState, WINDOW_SIZE, png_size, within};
use crate::browser_js::{self, Snapshot};

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
    let keyed = Browser::offscreen(mtm);
    let quit = keyed
        .window
        .performKeyEquivalent(&*command_key(&keyed.window, "q")?);
    check(!quit && !keyed.is_closed(), "⌘Q is not the window's", &quit)?;
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
        "edit": { "selected": selected, "undo_responder": undo, "cmd_w_closes": closed_by_key, "cmd_q_passes": !quit },
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
