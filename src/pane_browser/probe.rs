//! The hidden `fuxx __pane-browser-probe` spike (M11b Task 0): a `PaneWebView` over a GPUI
//! placeholder in a GPUI window that is never shown, driven through placing, a bounds change, a
//! window resize, hiding under an overlay, a page load and scripts, shortcuts while the page has
//! the keyboard, and mouse moves; one JSON report line, then it quits.
//!
//! Task 4 added what browser tabs rely on: a click in the page is told (`PageEvent::Clicked`),
//! navigations are told (`Changed`), ⌃Tab in the page reaches GPUI, the keyboard is handed over
//! and back (AppKit's answers), a page with the keyboard gives it back before it hides, a closed
//! tab's page leaves the window, and Pop Out moves a live page into a browser window.
//!
//! Task 4e binds fuxx's own keymap (`key_bindings`) in place of the probe's stand-ins, and adds
//! the browser keys: ⌘[ and ⌘R in a page with the keyboard go back and reload it, not to fuxx
//! (no previous pane, no rename), ⌘= zooms it, and fuxx's ⌘W / ⌘T / ⌘N / ⌘1 still reach fuxx.
//!
//! The window is never on screen, so GPUI's display link never runs: the probe draws each frame
//! itself (`Window::draw`, what the display link calls). Key and mouse events are made up and
//! handed to the window the way AppKit hands it real ones.

use std::cell::{Cell, RefCell};
use std::fmt::Debug;
use std::rc::Rc;
use std::time::{Duration, Instant};

use alacritty_terminal::vte::ansi::Rgb;
use gpui::{
    App, AsyncApp, Bounds, Context, DispatchPhase, FocusHandle, IntoElement, KeyDownEvent, Menu,
    MenuItem, MouseMoveEvent, Pixels, Point, Render, TitlebarOptions, Window, WindowBounds,
    WindowHandle, WindowOptions, canvas, div, point, prelude::*, px, rgb, size,
};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, msg_send};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSEvent, NSEventModifierFlags, NSEventType,
    NSResponder, NSView, NSWindow,
};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
use serde_json::{Value, json};

use super::{PaneWebView, native_frame};
use crate::browser::{self, BarLook, Browser, PageEvent, within};
use crate::browser_tab::{GoBack, Reload, ResetZoom, Zoom, ZoomIn, zoomed};
use crate::reduce_motion;
use crate::terminal_view::Copy;
use crate::workspace::{
    CloseSession, NewTab, NewWorkspace, NextTab, PrevPane, RenameSession, SelectSession1,
};

/// The probe gives up and exits with code 2 after this long, whatever is still pending.
const PROBE_DEADLINE: Duration = Duration::from_secs(15);
/// How long the probe waits for a page to load, and how often it looks.
const SETTLE_LIMIT: Duration = Duration::from_secs(5);
const LOAD_POLL: Duration = Duration::from_millis(50);
/// How long a script may take.
const ANSWER_LIMIT: Duration = Duration::from_secs(2);
/// How long the probe waits for WebKit to send back a shortcut the page didn't use.
const RESEND_WAIT: Duration = Duration::from_millis(300);

/// The window's content size, in points, and the size it is resized to.
const CONTENT: (f32, f32) = (800.0, 600.0);
const TALLER: f64 = 700.0;

/// The key codes (U.S. keyboard places) of W, T, N, 1, C, X, R, [, = and 0.
const KEY_W: u16 = 0x0D;
const KEY_T: u16 = 0x11;
const KEY_N: u16 = 0x2D;
const KEY_1: u16 = 0x12;
const KEY_C: u16 = 0x08;
const KEY_X: u16 = 0x07;
const KEY_R: u16 = 0x0F;
const KEY_LEFT_BRACKET: u16 = 0x21;
const KEY_EQUAL: u16 = 0x18;
const KEY_0: u16 = 0x1D;
const KEY_TAB: u16 = 0x30;

/// The placeholder before and after its bounds change.
fn first_place() -> Bounds<Pixels> {
    Bounds::new(point(px(100.0), px(80.0)), size(px(400.0), px(300.0)))
}

fn second_place() -> Bounds<Pixels> {
    Bounds::new(point(px(60.0), px(120.0)), size(px(520.0), px(260.0)))
}

/// A GPUI overlay drawn over part of the placeholder (as a menu or the rename box would be).
fn overlay_place() -> Bounds<Pixels> {
    Bounds::new(point(px(100.0), px(150.0)), size(px(200.0), px(100.0)))
}

/// The probe window's root: a placeholder whose `canvas` places the web view, and maybe an
/// overlay over it.
struct ProbeView {
    web: Rc<PaneWebView>,
    placeholder: Bounds<Pixels>,
    overlay: bool,
    /// The hide rule: the web view hides while the overlay shows.
    hide_under_overlay: bool,
    /// The bounds the placeholder's prepaint last placed the web view at.
    placed: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// Where GPUI saw the mouse move, in GPUI's coordinates.
    moves: Rc<RefCell<Vec<Point<Pixels>>>>,
    /// The keys GPUI saw go down.
    keys: Rc<RefCell<Vec<String>>>,
    /// GPUI's keyboard focus, and the key context of what has it (`Terminal`: fuxx binds ⌘C
    /// there; `BrowserTab`: ⌃Tab and the browser keys, nothing of the page's editing).
    focus: FocusHandle,
    context: &'static str,
}

impl Render for ProbeView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let web = self.web.clone();
        let placed = self.placed.clone();
        let moves = self.moves.clone();
        let keys = self.keys.clone();
        let hidden = self.overlay && self.hide_under_overlay;
        let p = self.placeholder;
        let o = overlay_place();
        div()
            .size_full()
            .relative()
            .track_focus(&self.focus)
            .key_context(self.context)
            .bg(rgb(0x1e1e2e))
            .child(
                div()
                    .absolute()
                    .left(p.origin.x)
                    .top(p.origin.y)
                    .w(p.size.width)
                    .h(p.size.height)
                    .child(
                        canvas(
                            move |bounds, window, _| {
                                web.place(bounds, window.viewport_size().height, hidden);
                                placed.set(Some(bounds));
                            },
                            move |_, _, window, _| {
                                window.on_mouse_event(move |e: &MouseMoveEvent, phase, _, _| {
                                    if phase == DispatchPhase::Bubble {
                                        moves.borrow_mut().push(e.position);
                                    }
                                });
                                window.on_key_event(move |e: &KeyDownEvent, phase, _, _| {
                                    if phase == DispatchPhase::Bubble {
                                        keys.borrow_mut().push(e.keystroke.to_string());
                                    }
                                });
                            },
                        )
                        .size_full(),
                    ),
            )
            .when(self.overlay, |d| {
                d.child(
                    div()
                        .absolute()
                        .left(o.origin.x)
                        .top(o.origin.y)
                        .w(o.size.width)
                        .h(o.size.height)
                        .bg(rgb(0xf38ba8)),
                )
            })
    }
}

/// `fuxx __pane-browser-probe`: never returns. Exits 0 after printing a one-line JSON report, 1
/// with a message on stderr if something that must hold doesn't, 2 if it takes longer than 15 s.
pub fn run_probe() -> ! {
    std::thread::spawn(|| {
        std::thread::sleep(PROBE_DEADLINE);
        eprintln!("fuxx __pane-browser-probe: no answer in 15 s");
        std::process::exit(2);
    });
    gpui_platform::application().run(|cx: &mut App| {
        let mtm = MainThreadMarker::new().expect("GPUI runs its callback on the main thread");
        // GPUI made fuxx a regular app (a Dock icon); the probe shows nothing, so it isn't one.
        NSApplication::sharedApplication(mtm)
            .setActivationPolicy(NSApplicationActivationPolicy::Accessory);
        // fuxx's own key bindings, bound as main.rs binds them, and a menu that shows two. The
        // actions the probe watches are logged (nothing else handles them here); a browser
        // tab's page keys also act on the page, as a browser tab does.
        let actions: Rc<RefCell<Vec<&'static str>>> = Rc::default();
        cx.bind_keys(crate::new_bindings());
        cx.bind_keys(crate::key_bindings());
        let log = actions.clone();
        cx.on_action(move |_: &CloseSession, _| log.borrow_mut().push("cmd-w"));
        let log = actions.clone();
        cx.on_action(move |_: &NewTab, _| log.borrow_mut().push("cmd-t"));
        let log = actions.clone();
        cx.on_action(move |_: &NewWorkspace, _| log.borrow_mut().push("cmd-n"));
        let log = actions.clone();
        cx.on_action(move |_: &SelectSession1, _| log.borrow_mut().push("cmd-1"));
        let log = actions.clone();
        cx.on_action(move |_: &Copy, _| log.borrow_mut().push("terminal cmd-c"));
        let log = actions.clone();
        cx.on_action(move |_: &NextTab, _| log.borrow_mut().push("ctrl-tab"));
        let log = actions.clone();
        cx.on_action(move |_: &RenameSession, _| log.borrow_mut().push("rename"));
        let log = actions.clone();
        cx.on_action(move |_: &PrevPane, _| log.borrow_mut().push("previous pane"));
        cx.set_menus([
            Menu::new("fuxx"),
            Menu::new("File").items([
                MenuItem::action("New Tab", NewTab),
                MenuItem::action("Close", CloseSession),
            ]),
        ]);

        let web = Rc::new(PaneWebView::new(false, mtm));
        let page_keys = (actions.clone(), web.clone());
        let (log, page) = page_keys.clone();
        cx.on_action(move |_: &Reload, _| {
            log.borrow_mut().push("page reload");
            page.page().reload();
        });
        let (log, page) = page_keys.clone();
        cx.on_action(move |_: &GoBack, _| {
            log.borrow_mut().push("page back");
            page.page().back();
        });
        let (log, page) = page_keys.clone();
        cx.on_action(move |_: &ZoomIn, _| {
            log.borrow_mut().push("page zoom in");
            page.page().set_zoom(zoomed(page.page().zoom(), Zoom::In));
        });
        let (log, page) = page_keys;
        cx.on_action(move |_: &ResetZoom, _| {
            log.borrow_mut().push("page actual size");
            page.page()
                .set_zoom(zoomed(page.page().zoom(), Zoom::Reset));
        });
        let placed: Rc<Cell<Option<Bounds<Pixels>>>> = Rc::default();
        let moves: Rc<RefCell<Vec<Point<Pixels>>>> = Rc::default();
        let keys: Rc<RefCell<Vec<String>>> = Rc::default();
        let before = reduce_motion::window_numbers();
        let root = (web.clone(), placed.clone(), moves.clone(), keys.clone());
        let window = cx
            .open_window(
                WindowOptions {
                    // Far off every screen, and never shown or focused.
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        point(px(-20000.0), px(-20000.0)),
                        size(px(CONTENT.0), px(CONTENT.1)),
                    ))),
                    // As fuxx's window: the content runs under a transparent title bar.
                    titlebar: Some(TitlebarOptions {
                        title: None,
                        appears_transparent: true,
                        traffic_light_position: None,
                    }),
                    show: false,
                    focus: false,
                    ..Default::default()
                },
                move |_, cx| {
                    cx.new(|cx| ProbeView {
                        web: root.0,
                        placeholder: first_place(),
                        overlay: false,
                        hide_under_overlay: false,
                        placed: root.1,
                        moves: root.2,
                        keys: root.3,
                        focus: cx.focus_handle(),
                        context: "Probe",
                    })
                },
            )
            .expect("open the probe window");
        let probe = Probe {
            mtm,
            window,
            web,
            placed,
            moves,
            keys,
            actions,
        };
        cx.spawn(
            async move |cx: &mut AsyncApp| match probe.run(&before, cx).await {
                Ok(report) => {
                    println!("{report}");
                    cx.update(|cx| cx.quit());
                }
                Err(message) => {
                    eprintln!("fuxx __pane-browser-probe: {message}");
                    std::process::exit(1);
                }
            },
        )
        .detach();
    });
    // The run loop only ends by exiting the process.
    std::process::exit(0)
}

/// What the probe drives and watches.
struct Probe {
    mtm: MainThreadMarker,
    window: WindowHandle<ProbeView>,
    web: Rc<PaneWebView>,
    placed: Rc<Cell<Option<Bounds<Pixels>>>>,
    moves: Rc<RefCell<Vec<Point<Pixels>>>>,
    keys: Rc<RefCell<Vec<String>>>,
    /// The GPUI actions that ran, in order.
    actions: Rc<RefCell<Vec<&'static str>>>,
}

impl Probe {
    async fn run(&self, before: &[isize], cx: &mut AsyncApp) -> Result<Value, String> {
        let started = Instant::now();
        let ex = cx.background_executor().clone();
        let home = format!("http://127.0.0.1:{}/", browser::serve_probe_pages()?);

        // GPUI's window and view, found the way fuxx finds its main window.
        let ns_window =
            reduce_motion::window_opened_since(before).ok_or("GPUI's window wasn't found")?;
        let content = ns_window
            .contentView()
            .ok_or("GPUI's window has no content view")?;
        let gpui_view = content
            .subviews()
            .iter()
            .find(|v| class_name(v) == "GPUIView")
            .ok_or("GPUI's view isn't in the content view")?;
        self.web.attach(&content, &gpui_view);
        let order: Vec<String> = content.subviews().iter().map(|v| class_name(&v)).collect();
        check(
            order.len() == 2 && order[0] == "GPUIView",
            "the web view sits above GPUI's view",
            &order,
        )?;
        let web: &NSView = self.web.view();

        // 1. Placing: one frame puts the web view exactly over the placeholder.
        self.draw(cx)?;
        let (viewport, gpui_scale) = self
            .window
            .update(cx, |_, w, _| (w.viewport_size(), w.scale_factor()))
            .map_err(|e| e.to_string())?;
        let content_size = content.bounds().size;
        check(
            content_size.height == f64::from(f32::from(viewport.height))
                && content_size.width == f64::from(f32::from(viewport.width)),
            "GPUI's viewport is the content view's size",
            &(content_size, viewport),
        )?;
        let placed = self
            .placed
            .get()
            .ok_or("the placeholder was never placed")?;
        check(placed == first_place(), "the placeholder's bounds", &placed)?;
        let frame = web.frame();
        check(
            frame == native_frame(placed, viewport.height)
                && frame == rect(100.0, 220.0, 400.0, 300.0),
            "the web view covers the placeholder",
            &frame,
        )?;
        let backing_scale = ns_window.backingScaleFactor();
        let backing = web.convertRectToBacking(web.bounds()).size;
        check(
            backing.width == frame.size.width * backing_scale
                && backing.height == frame.size.height * backing_scale,
            "AppKit applies the backing scale itself",
            &(backing, backing_scale),
        )?;

        // 2. The placeholder moves and grows: the next frame follows, not before.
        self.window
            .update(cx, |v, _, cx| {
                v.placeholder = second_place();
                cx.notify();
            })
            .map_err(|e| e.to_string())?;
        let before_frame = web.frame();
        self.draw(cx)?;
        let after_frame = web.frame();
        check(
            before_frame == frame && after_frame == native_frame(second_place(), viewport.height),
            "the web view follows a bounds change on the next frame",
            &(before_frame, after_frame),
        )?;

        // 3. The window grows taller: the top stays put between frames (the autoresizing mask),
        // and the next frame agrees.
        ns_window.setContentSize(NSSize::new(f64::from(CONTENT.0), TALLER));
        let between = web.frame();
        let tall = px(TALLER as f32);
        self.draw(cx)?;
        let after_resize = web.frame();
        let viewport_after = self
            .window
            .update(cx, |_, w, _| w.viewport_size())
            .map_err(|e| e.to_string())?;
        check(
            viewport_after.height == tall
                && between == native_frame(second_place(), tall)
                && after_resize == between,
            "the web view keeps its place through a window resize",
            &(between, after_resize, viewport_after),
        )?;

        // 4. An overlay over the placeholder: GPUI draws it beneath the web view, which covers
        // it (and takes its clicks), until the hide rule hides the web view.
        let o = overlay_place();
        let overlay_middle = to_content(o.center(), TALLER);
        self.window
            .update(cx, |v, _, cx| {
                v.overlay = true;
                cx.notify();
            })
            .map_err(|e| e.to_string())?;
        self.draw(cx)?;
        let covered = hit(&content, overlay_middle, web);
        self.window
            .update(cx, |v, _, cx| {
                v.hide_under_overlay = true;
                cx.notify();
            })
            .map_err(|e| e.to_string())?;
        self.draw(cx)?;
        let hidden = web.isHidden();
        let uncovered = hit(&content, overlay_middle, web);
        self.window
            .update(cx, |v, _, cx| {
                v.overlay = false;
                cx.notify();
            })
            .map_err(|e| e.to_string())?;
        self.draw(cx)?;
        let shown_again = !web.isHidden();
        check(
            covered == "web view" && hidden && uncovered == "GPUIView" && shown_again,
            "the hide rule",
            &(&covered, hidden, &uncovered, shown_again),
        )?;

        // 5. A page, and scripts: the page is laid out at the placeholder's size. The page's
        // events go to a channel, as a browser tab's do.
        let (events_tx, mut events_rx) = futures::channel::mpsc::unbounded::<PageEvent>();
        self.web.listen(events_tx);
        self.web.page().load(&home);
        let deadline = Instant::now() + SETTLE_LIMIT;
        loop {
            // SAFETY: main thread; plain property reads.
            let (loading, url) = unsafe {
                (
                    self.web.view().isLoading(),
                    self.web.view().URL().and_then(|u| u.absoluteString()),
                )
            };
            if !loading && url.is_some_and(|u| u.to_string() == home) {
                break;
            }
            check(Instant::now() < deadline, "the page never loaded", &home)?;
            ex.timer(LOAD_POLL).await;
        }
        let title = within(self.web.page().eval("document.title"), ANSWER_LIMIT, &ex).await?;
        check(title == json!("fuxx probe"), "eval title", &title)?;
        let page_size = within(
            self.web.page().eval("[innerWidth, innerHeight]"),
            ANSWER_LIMIT,
            &ex,
        )
        .await?;
        check(
            page_size == json!([520, 260]),
            "the page is the placeholder's size",
            &page_size,
        )?;

        // 6. Shortcuts. AppKit offers a ⌘ key to the key window's views first
        // (`performKeyEquivalent:` down the view tree), then to the main menu; the probe does
        // the same, as its window can't be the key window.
        ns_window.makeFirstResponder(Some(&gpui_view));
        let gpui_has_keys = self.key(&ns_window, "w", KEY_W, &ex).await?;
        ns_window.makeFirstResponder(Some(web));
        let page_has_keyboard = first_responder_is(&ns_window, web);
        check(
            page_has_keyboard,
            "the page takes the keyboard",
            &page_has_keyboard,
        )?;
        let mut page_has_keys = serde_json::Map::new();
        for (name, chars, code) in [
            ("cmd-w", "w", KEY_W),
            ("cmd-t", "t", KEY_T),
            ("cmd-n", "n", KEY_N),
            ("cmd-1", "1", KEY_1),
        ] {
            page_has_keys.insert(name.into(), self.key(&ns_window, chars, code, &ex).await?);
        }
        let menu = {
            let before = self.actions.borrow().len();
            let event = key_event(&ns_window, "w", KEY_W, NSEventModifierFlags::Command)?;
            let app = NSApplication::sharedApplication(self.mtm);
            // SAFETY: main thread; NSApplication's main menu (GPUI's), asked as AppKit asks it.
            let handled: bool = unsafe {
                let menu: Option<Retained<AnyObject>> = msg_send![&app, mainMenu];
                match menu {
                    Some(menu) => msg_send![&menu, performKeyEquivalent: &*event],
                    None => false,
                }
            };
            json!({ "handled": handled, "actions": self.actions.borrow()[before..] })
        };
        // GPUI answers a key equivalent from what has GPUI's focus, whichever view has AppKit's:
        // a terminal's ⌘C would win over the page's.
        let terminal = self.focused_context("Terminal", cx)?;
        let terminal_cmd_c = self.key(&ns_window, "c", KEY_C, &ex).await?;
        let browser_tab = self.focused_context("BrowserTab", cx)?;
        let browser_cmd_c = self.key(&ns_window, "c", KEY_C, &ex).await?;
        let browser_cmd_w = self.key(&ns_window, "w", KEY_W, &ex).await?;
        let gpui_focus = json!({
            "terminal_context": { "focused": terminal, "cmd-c": terminal_cmd_c },
            "browser_tab_context": {
                "focused": browser_tab,
                "cmd-c": browser_cmd_c,
                "cmd-w": browser_cmd_w,
            },
        });
        // Typing: a plain key goes to the page (the first responder), never past GPUI.
        within(
            self.web
                .page()
                .eval("document.getElementById('name').focus(), document.activeElement.id"),
            ANSWER_LIMIT,
            &ex,
        )
        .await?;
        let keys_before = self.keys.borrow().len();
        for kind in [NSEventType::KeyDown, NSEventType::KeyUp] {
            ns_window.sendEvent(&*plain_key_event(&ns_window, kind, "x", KEY_X)?);
        }
        ex.timer(RESEND_WAIT).await;
        let field = within(
            self.web
                .page()
                .eval("document.getElementById('name').value"),
            ANSWER_LIMIT,
            &ex,
        )
        .await?;
        let typing = json!({
            "page_field": field,
            "gpui_keys": self.keys.borrow()[keys_before..],
            "page_still_has_keyboard": first_responder_is(&ns_window, web),
        });
        // 7. The mouse. AppKit sends mouse moves through the window; does GPUI see them over
        // the web view, and beside it, while the page has the keyboard?
        let over_page = to_content(second_place().center(), TALLER);
        let beside_page = to_content(point(px(20.0), px(20.0)), TALLER);
        let page_keyboard_moves = json!({
            "over_page": self.mouse_moved(&ns_window, &content, over_page)?,
            "beside_page": self.mouse_moved(&ns_window, &content, beside_page)?,
        });

        // The fix to try: the web view offers each key equivalent to GPUI's view first, and
        // passes it the mouse moves.
        self.web.route_events_to(&gpui_view);
        let mut routed = serde_json::Map::new();
        for (name, chars, code) in [
            ("cmd-w", "w", KEY_W),
            ("cmd-t", "t", KEY_T),
            ("cmd-n", "n", KEY_N),
            ("cmd-1", "1", KEY_1),
            ("cmd-c", "c", KEY_C),
        ] {
            routed.insert(name.into(), self.key(&ns_window, chars, code, &ex).await?);
        }
        let still_page = first_responder_is(&ns_window, web);

        // 7. The mouse, continued: with the web view passing GPUI its mouse moves (routing on
        // since the keys above), then with GPUI holding the keyboard.
        let page_keyboard_moves_routed = json!({
            "over_page": self.mouse_moved(&ns_window, &content, over_page)?,
            "beside_page": self.mouse_moved(&ns_window, &content, beside_page)?,
        });
        ns_window.makeFirstResponder(Some(&gpui_view));
        let gpui_keyboard_moves = json!({
            "over_page": self.mouse_moved(&ns_window, &content, over_page)?,
            "beside_page": self.mouse_moved(&ns_window, &content, beside_page)?,
        });
        // A click beside the page while the page has the keyboard: who has it afterwards?
        ns_window.makeFirstResponder(Some(web));
        self.click(&ns_window, &content, beside_page)?;
        let after_click_beside = responder_name(&ns_window, web, &gpui_view);
        ns_window.makeFirstResponder(Some(&gpui_view));
        self.click(&ns_window, &content, over_page)?;
        let after_click_on_page = responder_name(&ns_window, web, &gpui_view);

        // 8. What browser tabs rely on (Task 4).
        let mut events = Vec::new();
        while let Ok(event) = events_rx.try_recv() {
            events.push(format!("{event:?}"));
        }
        let navigation_told = events.iter().any(|e| e == "Changed");
        check(navigation_told, "the page's navigations are told", &events)?;
        // Same-document changes: `pushState` and a title set after load are no navigations, but
        // the page still tells (it is watched, not polled).
        while events_rx.try_recv().is_ok() {}
        within(
            self.web
                .page()
                .eval("history.pushState({}, '', '/pushed'), location.href"),
            ANSWER_LIMIT,
            &ex,
        )
        .await?;
        ex.timer(LOAD_POLL).await;
        let mut push_told = false;
        while let Ok(event) = events_rx.try_recv() {
            push_told |= event == PageEvent::Changed;
        }
        let pushed_url = self.web.page().state().url;
        let (push_back, _) = self.web.page().can_go();
        within(
            self.web.page().eval("document.title = 'retitled'"),
            ANSWER_LIMIT,
            &ex,
        )
        .await?;
        ex.timer(LOAD_POLL).await;
        let mut title_told = false;
        while let Ok(event) = events_rx.try_recv() {
            title_told |= event == PageEvent::Changed;
        }
        let retitled = self.web.page().state().title;
        check(
            push_told
                && pushed_url.ends_with("/pushed")
                && push_back
                && title_told
                && retitled == "retitled",
            "pushState and a late title are told",
            &(push_told, &pushed_url, push_back, title_told, &retitled),
        )?;
        // In a key window AppKit gives a click to the view its hit test finds; this window is
        // never key, so its first click only activates it (as the spike found). Which view gets
        // a click on the page, and that view's `mouseDown:` as AppKit would send it.
        let clicked_by_window = events.iter().any(|e| e == "Clicked");
        let superview = unsafe { content.superview() };
        let in_frame = content.convertPoint_toView(over_page, superview.as_deref());
        let hit_view = content.hitTest(in_frame);
        let hit_is_page_view = hit_view.as_deref().is_some_and(|v| std::ptr::eq(v, web));
        let hit_class = hit_view.as_deref().map(class_name);
        let down = mouse_event(&ns_window, &content, NSEventType::LeftMouseDown, over_page)?;
        let up = mouse_event(&ns_window, &content, NSEventType::LeftMouseUp, over_page)?;
        let view: &NSResponder = web;
        view.mouseDown(&down);
        view.mouseUp(&up);
        let clicked_flag = self.web.clicked();
        ex.timer(LOAD_POLL).await;
        let mut clicked_told = false;
        while let Ok(event) = events_rx.try_recv() {
            clicked_told |= event == PageEvent::Clicked;
        }
        check(
            hit_is_page_view && clicked_flag && clicked_told,
            "a click on the page is the page view's, and is told",
            &(&hit_class, clicked_flag, clicked_told),
        )?;
        self.web.settle_click();
        // The keyboard over to the page and back, as a browser tab hands it.
        let took = self.web.take_keyboard();
        let page_has_it = self.web.has_keyboard();
        let gave = self.web.give_keyboard_back();
        let gpui_has_it = first_responder_is(&ns_window, &gpui_view);
        check(
            took && page_has_it && gave && gpui_has_it,
            "the keyboard goes to the page and back",
            &(took, page_has_it, gave, gpui_has_it),
        )?;
        // ⌃Tab with the page holding the keyboard: AppKit's own route (the window's
        // `sendEvent:`), then the page view's `keyDown:` on its own. Neither types into the page.
        let field_before = within(
            self.web.page().eval(
                "document.getElementById('name').focus(), document.getElementById('name').value",
            ),
            ANSWER_LIMIT,
            &ex,
        )
        .await?;
        self.web.take_keyboard();
        let ctrl_tab = |kind| {
            key_event_of(
                &ns_window,
                kind,
                "\t",
                KEY_TAB,
                NSEventModifierFlags::Control,
            )
        };
        let before = self.actions.borrow().len();
        ns_window.sendEvent(&*ctrl_tab(NSEventType::KeyDown)?);
        ns_window.sendEvent(&*ctrl_tab(NSEventType::KeyUp)?);
        let by_window: Vec<&str> = self.actions.borrow()[before..].to_vec();
        let before = self.actions.borrow().len();
        let view: &NSResponder = web;
        view.keyDown(&*ctrl_tab(NSEventType::KeyDown)?);
        let by_page_view: Vec<&str> = self.actions.borrow()[before..].to_vec();
        ex.timer(RESEND_WAIT).await;
        let field_after = within(
            self.web
                .page()
                .eval("document.getElementById('name').value"),
            ANSWER_LIMIT,
            &ex,
        )
        .await?;
        check(
            by_page_view == ["ctrl-tab"] && field_after == field_before,
            "⌃Tab in the page reaches GPUI, not the page",
            &(&by_window, &by_page_view, &field_before, &field_after),
        )?;
        // 9. The browser keys (Task 4e): the page has the keyboard and its tab GPUI's focus.
        // ⌘[ goes back (from the pushState above) and ⌘R reloads, in the page, and neither
        // reaches fuxx (no previous pane, no rename); ⌘= zooms the page and ⌘0 sets it back;
        // fuxx's keys still reach fuxx.
        self.focused_context("BrowserTab", cx)?;
        self.web.take_keyboard();
        let back = self.key(&ns_window, "[", KEY_LEFT_BRACKET, &ex).await?;
        let deadline = Instant::now() + SETTLE_LIMIT;
        while self.web.page().state().url != home {
            check(
                Instant::now() < deadline,
                "⌘[ never went back",
                &(&back, self.web.page().state().url),
            )?;
            ex.timer(LOAD_POLL).await;
        }
        within(
            self.web.page().eval("window.fuxxMark = 7"),
            ANSWER_LIMIT,
            &ex,
        )
        .await?;
        let reload = self.key(&ns_window, "r", KEY_R, &ex).await?;
        let deadline = Instant::now() + SETTLE_LIMIT;
        let reloaded = loop {
            let mark = within(self.web.page().eval("window.fuxxMark"), ANSWER_LIMIT, &ex).await;
            if !self.web.page().state().loading && mark == Ok(Value::Null) {
                break true;
            }
            if Instant::now() >= deadline {
                break false;
            }
            ex.timer(LOAD_POLL).await;
        };
        let zoom_before = self.web.page().zoom();
        let zoom_in = self.key(&ns_window, "=", KEY_EQUAL, &ex).await?;
        let zoom_in_to = self.web.page().zoom();
        let actual_size = self.key(&ns_window, "0", KEY_0, &ex).await?;
        let actual_size_to = self.web.page().zoom();
        let mut fuxx_keys = serde_json::Map::new();
        for (name, chars, code) in [
            ("cmd-w", "w", KEY_W),
            ("cmd-t", "t", KEY_T),
            ("cmd-n", "n", KEY_N),
            ("cmd-1", "1", KEY_1),
        ] {
            fuxx_keys.insert(name.into(), self.key(&ns_window, chars, code, &ex).await?);
        }
        let only = |key: &Value, action: &str| {
            key["handled"] == json!(true)
                && key["gpui_actions_at_once"] == json!([action])
                && key["gpui_actions_later"] == json!([])
                && key["web_view_handled"] == json!(0)
        };
        let fuxx_reached = fuxx_keys.iter().all(|(name, key)| only(key, name));
        let page_keyboard_kept = self.web.has_keyboard();
        check(
            only(&back, "page back")
                && only(&reload, "page reload")
                && reloaded
                && only(&zoom_in, "page zoom in")
                && (zoom_before - 1.0).abs() < 1e-3
                && (zoom_in_to - 1.1).abs() < 1e-3
                && only(&actual_size, "page actual size")
                && (actual_size_to - 1.0).abs() < 1e-3
                && fuxx_reached
                && page_keyboard_kept,
            "browser keys act on the page; fuxx's keys reach fuxx",
            &(
                &back,
                &reload,
                reloaded,
                (zoom_before, zoom_in_to, actual_size_to),
                &fuxx_keys,
                page_keyboard_kept,
            ),
        )?;
        let browser_keys = json!({
            "cmd-[": back,
            "cmd-r": { "key": reload, "page_reloaded": reloaded },
            "cmd-=": { "key": zoom_in, "zoom": [zoom_before, zoom_in_to] },
            "cmd-0": { "key": actual_size, "zoom": actual_size_to },
            "fuxx_keys": fuxx_keys,
            "page_still_has_keyboard": page_keyboard_kept,
        });
        // A page holding the keyboard gives it back to GPUI's view before it hides.
        self.web.take_keyboard();
        self.web.hide();
        let hidden_gave_back = first_responder_is(&ns_window, &gpui_view) && web.isHidden();
        check(
            hidden_gave_back,
            "hiding gives the keyboard back first",
            &responder_name(&ns_window, web, &gpui_view),
        )?;
        // Pop Out: a second page, loaded, moved whole into a browser window.
        let other = PaneWebView::new(false, self.mtm);
        other.attach(&content, &gpui_view);
        other.route_events_to(&gpui_view);
        let second = format!("{home}two");
        other.page().load(&second);
        let deadline = Instant::now() + SETTLE_LIMIT;
        while other.page().state().loading || other.page().state().url != second {
            check(
                Instant::now() < deadline,
                "the second page never loaded",
                &second,
            )?;
            ex.timer(LOAD_POLL).await;
        }
        let marked = within(other.page().eval("window.fuxxMark = 42"), ANSWER_LIMIT, &ex).await?;
        let other_view: Retained<NSView> = Retained::from(other.view() as &NSView);
        let page = other.into_page();
        let left_window = other_view.window().is_none();
        let look = BarLook {
            dark: true,
            accent: Rgb {
                r: 137,
                g: 180,
                b: 250,
            },
            address_bar: true,
        };
        let popped = Browser::adopt(page, "probe", &look, self.mtm);
        let popped_url = popped.state().url;
        // The same page, not loaded again: what its script set is still there.
        let still_marked = within(popped.eval("window.fuxxMark"), ANSWER_LIMIT, &ex).await?;
        let in_browser_window = other_view
            .window()
            .is_some_and(|w| !std::ptr::eq(&*w, &*ns_window));
        check(
            marked == json!(42)
                && left_window
                && popped_url == second
                && still_marked == json!(42)
                && in_browser_window,
            "Pop Out moves the live page into a browser window",
            &(left_window, &popped_url, &still_marked, in_browser_window),
        )?;
        drop(popped);
        // A closed tab's page leaves the window at once (and stops), whoever still holds it.
        self.web.remove();
        let removed =
            web.window().is_none() && !content.subviews().iter().any(|v| std::ptr::eq(&*v, web));
        check(removed, "a closed tab's page leaves the window", &removed)?;

        Ok(json!({
            "view_order": order,
            "content_view_flipped": content.isFlipped(),
            "place": {
                "placeholder": bounds_json(placed),
                "content_height": f32::from(viewport.height),
                "web_frame": rect_json(frame),
                "gpui_scale": gpui_scale,
                "backing_scale": backing_scale,
                "web_backing_size": [backing.width, backing.height],
            },
            "bounds_change": {
                "placeholder": bounds_json(second_place()),
                "before_next_frame": rect_json(before_frame),
                "after_one_frame": rect_json(after_frame),
                "frames_to_follow": 1,
            },
            "window_resize": {
                "content_height": TALLER,
                "before_next_frame": rect_json(between),
                "after_one_frame": rect_json(after_resize),
            },
            "hide": {
                "overlay_point_hits_while_shown": covered,
                "hidden_under_overlay": hidden,
                "overlay_point_hits_while_hidden": uncovered,
                "shown_again": shown_again,
            },
            "page": { "url": home, "title": title, "inner_size": page_size },
            "keys": {
                "gpui_view_first_responder": { "cmd-w": gpui_has_keys },
                "web_view_first_responder": page_has_keys,
                "main_menu_cmd_w": menu,
                "by_gpui_focus": gpui_focus,
                "typing_x": typing,
                "routed_through_gpui_first": routed,
                "page_still_has_keyboard": still_page,
                "gpui_view_accepts_first_responder": gpui_view.acceptsFirstResponder(),
                "web_view_accepts_first_responder": web.acceptsFirstResponder(),
            },
            "mouse": {
                "web_view_first_responder": page_keyboard_moves,
                "web_view_first_responder_routed": page_keyboard_moves_routed,
                "gpui_view_first_responder": gpui_keyboard_moves,
                "keyboard_after_click_beside_page": after_click_beside,
                "keyboard_after_click_on_page": after_click_on_page,
                "window_visible": ns_window.isVisible(),
                "window_key": ns_window.isKeyWindow(),
            },
            "browser_tab": {
                "navigation_told": navigation_told,
                "same_document": { "push_state_told": push_told, "url": pushed_url, "can_go_back": push_back, "late_title_told": title_told, "title": retitled },
                "click": {
                    "by_window_sendevent": clicked_by_window,
                    "hit_view": hit_class,
                    "hit_is_page_view": hit_is_page_view,
                    "page_view_mouse_down_told": clicked_told,
                    "clicked_flag": clicked_flag,
                },
                "keyboard": { "took": took, "page_had_it": page_has_it, "gave_back": gave, "gpui_has_it": gpui_has_it },
                "ctrl_tab": { "by_window": by_window, "by_page_view": by_page_view, "page_field_unchanged": field_after == field_before },
                "hide_gives_keyboard_back": hidden_gave_back,
                "pop_out": { "left_fuxx_window": left_window, "url_kept": popped_url == second, "page_state_kept": still_marked == json!(42), "in_browser_window": in_browser_window },
                "closed_page_removed": removed,
                "browser_keys": browser_keys,
            },
            "ms": started.elapsed().as_millis() as u64,
        }))
    }

    /// Give GPUI's focus to the root, now in key context `context`, and draw; whether it has it.
    fn focused_context(&self, context: &'static str, cx: &mut AsyncApp) -> Result<bool, String> {
        let focused = self
            .window
            .update(cx, |v, window, cx| {
                v.context = context;
                window.focus(&v.focus, cx);
                cx.notify();
                v.focus.is_focused(window)
            })
            .map_err(|e| e.to_string())?;
        self.draw(cx)?;
        Ok(focused)
    }

    /// One frame, as GPUI's display link would draw it.
    fn draw(&self, cx: &mut AsyncApp) -> Result<(), String> {
        // The window, not its root view: drawing renders the root, which an update of it holds.
        cx.update_window(self.window.into(), |_, window, cx| window.draw(cx).clear())
            .map_err(|e| e.to_string())
    }

    /// ⌘-`chars` offered to the window's views as AppKit offers a key equivalent: whether a
    /// view took it, the GPUI actions that ran at once and once WebKit had time to send back
    /// what the page didn't use, and how many times the web view handled it itself.
    async fn key(
        &self,
        window: &NSWindow,
        chars: &str,
        code: u16,
        ex: &gpui::BackgroundExecutor,
    ) -> Result<Value, String> {
        let event = key_event(window, chars, code, NSEventModifierFlags::Command)?;
        let (actions_before, seen_before) =
            (self.actions.borrow().len(), self.web.key_equivalents_seen());
        let handled = window.performKeyEquivalent(&event);
        let at_once: Vec<&str> = self.actions.borrow()[actions_before..].to_vec();
        ex.timer(RESEND_WAIT).await;
        let later: Vec<&str> = self.actions.borrow()[actions_before + at_once.len()..].to_vec();
        Ok(json!({
            "handled": handled,
            "gpui_actions_at_once": at_once,
            "gpui_actions_later": later,
            "web_view_handled": self.web.key_equivalents_seen() - seen_before,
        }))
    }

    /// A mouse move to `at` (content coordinates) through the window; where GPUI saw it, if it
    /// did.
    fn mouse_moved(
        &self,
        window: &NSWindow,
        content: &NSView,
        at: NSPoint,
    ) -> Result<Value, String> {
        let before = self.moves.borrow().len();
        window.sendEvent(&*mouse_event(window, content, NSEventType::MouseMoved, at)?);
        let seen: Vec<[f32; 2]> = self.moves.borrow()[before..]
            .iter()
            .map(|p| [f32::from(p.x), f32::from(p.y)])
            .collect();
        Ok(json!(seen))
    }

    /// A left click at `at` (content coordinates) through the window.
    fn click(&self, window: &NSWindow, content: &NSView, at: NSPoint) -> Result<(), String> {
        window.sendEvent(&*mouse_event(
            window,
            content,
            NSEventType::LeftMouseDown,
            at,
        )?);
        window.sendEvent(&*mouse_event(
            window,
            content,
            NSEventType::LeftMouseUp,
            at,
        )?);
        Ok(())
    }
}

/// A view's Objective-C class name.
fn class_name(view: &NSView) -> String {
    view.class().name().to_string_lossy().into_owned()
}

/// What the content view's hit test finds at `at` (content coordinates): `web view` (or a view
/// inside it), or the class of what else.
fn hit(content: &NSView, at: NSPoint, web: &NSView) -> String {
    // SAFETY: main thread; the content view's superview is the window's frame view.
    let superview = unsafe { content.superview() };
    let at = content.convertPoint_toView(at, superview.as_deref());
    match content.hitTest(at) {
        Some(view) if view.isDescendantOf(web) => "web view".to_string(),
        Some(view) => class_name(&view),
        None => "nothing".to_string(),
    }
}

/// A point in GPUI's coordinates as AppKit's, in a content view `height` high.
fn to_content(p: Point<Pixels>, height: f64) -> NSPoint {
    NSPoint::new(
        f64::from(f32::from(p.x)),
        height - f64::from(f32::from(p.y)),
    )
}

fn first_responder_is(window: &NSWindow, view: &NSView) -> bool {
    let view: &NSResponder = view;
    window
        .firstResponder()
        .is_some_and(|r| std::ptr::eq(&*r, view))
}

/// Who has the keyboard: `web view`, `GPUIView`, or the responder's class.
fn responder_name(window: &NSWindow, web: &NSView, gpui: &NSView) -> String {
    if first_responder_is(window, web) {
        "web view".to_string()
    } else if first_responder_is(window, gpui) {
        "GPUIView".to_string()
    } else {
        window.firstResponder().map_or("none".to_string(), |r| {
            r.class().name().to_string_lossy().into_owned()
        })
    }
}

/// A key-down event for `window` with `flags` held, on the key at `code`.
fn key_event(
    window: &NSWindow,
    chars: &str,
    code: u16,
    flags: NSEventModifierFlags,
) -> Result<Retained<NSEvent>, String> {
    key_event_of(window, NSEventType::KeyDown, chars, code, flags)
}

/// A key event of `kind` with no modifiers.
fn plain_key_event(
    window: &NSWindow,
    kind: NSEventType,
    chars: &str,
    code: u16,
) -> Result<Retained<NSEvent>, String> {
    key_event_of(window, kind, chars, code, NSEventModifierFlags::empty())
}

fn key_event_of(
    window: &NSWindow,
    kind: NSEventType,
    chars: &str,
    code: u16,
    flags: NSEventModifierFlags,
) -> Result<Retained<NSEvent>, String> {
    let chars = NSString::from_str(chars);
    NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
        kind,
        NSPoint::new(0.0, 0.0),
        flags,
        0.0,
        window.windowNumber(),
        None,
        &chars,
        &chars,
        false,
        code,
    )
    .ok_or_else(|| "no key event".to_string())
}

/// A mouse event of `kind` at `at` (content coordinates) for `window`.
fn mouse_event(
    window: &NSWindow,
    content: &NSView,
    kind: NSEventType,
    at: NSPoint,
) -> Result<Retained<NSEvent>, String> {
    let at = content.convertPoint_toView(at, None);
    NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
        kind,
        at,
        NSEventModifierFlags::empty(),
        0.0,
        window.windowNumber(),
        None,
        0,
        1,
        1.0,
    )
    .ok_or_else(|| "no mouse event".to_string())
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(w, h))
}

fn rect_json(r: NSRect) -> Value {
    json!([r.origin.x, r.origin.y, r.size.width, r.size.height])
}

fn bounds_json(b: Bounds<Pixels>) -> Value {
    json!([
        f32::from(b.origin.x),
        f32::from(b.origin.y),
        f32::from(b.size.width),
        f32::from(b.size.height)
    ])
}

/// `Err` naming `what` and showing `got` unless `ok`.
fn check(ok: bool, what: &str, got: &impl Debug) -> Result<(), String> {
    if ok {
        Ok(())
    } else {
        Err(format!("{what}: got {got:?}"))
    }
}
