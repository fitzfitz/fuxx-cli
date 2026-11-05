//! M10c: the workspace's browsers, one per session at most (`Browsers`), and File → Open
//! Browser… (⇧⌘B): a box over the window for an address, opened for the active session.
//!
//! A browser window is titled `<session name> — <page title>`. One 500 ms timer runs while any
//! browser is open: it keeps the titles up to date (a renamed session, a new page) and forgets
//! the windows the user closed. Closing a session closes its browser.

use std::collections::HashMap;
use std::time::Duration;

use gpui::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, MouseDownEvent, Subscription,
    Window, div, prelude::*, px,
};

use crate::appearance::Appearance;
use crate::browser::Browser;
use crate::browser_url::normalize_url;
use crate::name_input::{NameInput, NameInputEvent};
use crate::terminal_view::hsla;
use crate::workspace::Workspace;

/// How often the open browsers' titles are brought up to date and closed windows noticed.
const BROWSER_TICK: Duration = Duration::from_millis(500);

const PANEL_WIDTH: f32 = 480.0;
const PLACEHOLDER: &str = "Address \u{2014} localhost:3000 or a URL";
const HINT: &str = "Enter open \u{b7} Esc close";

/// A browser window's title: the session's name, then the page's title once there is one.
pub fn browser_title(session: &str, page: Option<&str>) -> String {
    match page.map(str::trim) {
        Some(page) if !page.is_empty() => format!("{session} \u{2014} {page}"),
        _ => session.to_string(),
    }
}

/// What `Browsers` needs of a browser: whether the user closed its window, and its title.
pub(crate) trait BrowserWindow {
    fn is_closed(&self) -> bool;
    fn set_title(&self, title: &str);
}

impl BrowserWindow for Browser {
    fn is_closed(&self) -> bool {
        Browser::is_closed(self)
    }

    fn set_title(&self, title: &str) {
        Browser::set_title(self, title)
    }
}

/// The sessions' browsers, by session id, each with the title its window shows. Dropping a
/// browser closes its window.
pub(crate) struct Browsers<B = Browser> {
    open: HashMap<u64, (B, String)>,
}

impl<B: BrowserWindow> Browsers<B> {
    pub fn new() -> Self {
        Self {
            open: HashMap::new(),
        }
    }

    pub fn get(&self, session: u64) -> Option<&B> {
        self.open.get(&session).map(|(b, _)| b)
    }

    /// Keep `browser` as `session`'s, its window titled `title`.
    pub fn insert(&mut self, session: u64, browser: B, title: String) {
        self.open.insert(session, (browser, title));
    }

    /// Session `session` closed: its browser goes, and its window with it.
    pub fn session_closed(&mut self, session: u64) {
        self.open.remove(&session);
    }

    /// Drop the browsers whose window the user closed, and those of sessions gone.
    pub fn forget_closed(&mut self, has_session: impl Fn(u64) -> bool) {
        self.open
            .retain(|session, (browser, _)| has_session(*session) && !browser.is_closed());
    }

    /// Give each window the title `title_for` its session and browser, when that changed.
    pub fn retitle(&mut self, title_for: impl Fn(u64, &B) -> String) {
        for (session, (browser, shown)) in &mut self.open {
            let title = title_for(*session, browser);
            if *shown != title {
                browser.set_title(&title);
                *shown = title;
            }
        }
    }

    /// Session `old` goes on as `new` (an SSH reconnect): its browser goes with it.
    pub fn rekey(&mut self, old: u64, new: u64) {
        if let Some(open) = self.open.remove(&old) {
            self.open.insert(new, open);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.open.is_empty()
    }
}

pub enum AddressBoxEvent {
    /// Enter: the address as typed (trimmed).
    Open(String),
    /// Escape, a click outside, or the box lost focus.
    Cancel,
}

/// The address box: a `NameInput` (so typing, accents and IME work as in the rename box) with
/// a line under it for why the last address couldn't be opened.
pub struct AddressBox {
    input: Entity<NameInput>,
    /// Why the address last entered was refused; cleared by the next edit.
    error: Option<String>,
    _input_events: Subscription,
    _input_changes: Subscription,
}

impl EventEmitter<AddressBoxEvent> for AddressBox {}

impl Focusable for AddressBox {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.focus_handle(cx)
    }
}

impl AddressBox {
    /// `address`: the browser's current one (all of it selected, so typing replaces it), or
    /// empty.
    pub fn new(address: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| {
            let mut input = NameInput::new(address, window, cx);
            input.select_all();
            input
        });
        let input_events = cx.subscribe_in(&input, window, |_, _, event, _, cx| match event {
            NameInputEvent::Confirm(text) => cx.emit(AddressBoxEvent::Open(text.clone())),
            NameInputEvent::Cancel => cx.emit(AddressBoxEvent::Cancel),
        });
        let input_changes = cx.observe(&input, |this, _, cx| {
            if this.error.take().is_some() {
                cx.notify();
            }
        });
        Self {
            input,
            error: None,
            _input_events: input_events,
            _input_changes: input_changes,
        }
    }

    /// Say why the address can't be opened; the box stays for another try.
    pub fn show_error(&mut self, why: String, cx: &mut Context<Self>) {
        self.error = Some(why);
        cx.notify();
    }
}

impl Render for AddressBox {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = cx.global::<Appearance>().theme.ui();
        // The box, with the placeholder laid over it while it is empty (as the host picker's).
        let empty = self.input.read(cx).text().is_empty();
        let field = div()
            .relative()
            .flex()
            .child(self.input.clone())
            .when(empty, |field| {
                field.child(
                    div()
                        .absolute()
                        // Inside the box's 1 px border and 4 px padding.
                        .top(px(1.0))
                        .left(px(5.0))
                        .text_color(hsla(ui.text_dim))
                        .child(PLACEHOLDER),
                )
            });
        let line = match &self.error {
            // The waiting ring's colour: the theme's red.
            Some(why) => div().text_color(hsla(ui.ring_waiting)).child(why.clone()),
            None => div().text_color(hsla(ui.text_dim)).child(HINT),
        };
        div()
            .key_context("AddressBox")
            // The terminal under the panel gets neither its clicks nor its hover.
            .occlude()
            .on_mouse_down_out(
                cx.listener(|_, _: &MouseDownEvent, _, cx| cx.emit(AddressBoxEvent::Cancel)),
            )
            .flex()
            .flex_col()
            .w(px(PANEL_WIDTH))
            .max_w_full()
            .p_2()
            .gap_2()
            .rounded_lg()
            .border_1()
            .border_color(hsla(ui.sidebar_border))
            .bg(hsla(ui.sidebar_background))
            .shadow_lg()
            .text_sm()
            .text_color(hsla(ui.text))
            .child(field)
            .child(line.px_1().text_xs())
    }
}

impl Workspace {
    /// Open `url` in session `index`'s browser, making the browser first if it has none, and
    /// bring it to the front. A refused address (see `normalize_url`) is the error; a page that
    /// fails to load shows in the browser (and in its `state`).
    pub(crate) fn browser_open(
        &mut self,
        index: usize,
        url: &str,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let url = normalize_url(url)?;
        let Some(session) = self.sessions.items().get(index) else {
            return Err("no such session".to_string());
        };
        let id = session.id;
        if self.browsers.get(id).is_none() {
            let title = browser_title(&session.label(), None);
            let browser = Browser::open(&title, cx);
            self.browsers.insert(id, browser, title);
            self.tick_browsers(cx);
        }
        if let Some(browser) = self.browsers.get(id) {
            browser.load(&url);
            browser.bring_to_front();
        }
        Ok(())
    }

    /// Bring the browsers' titles up to date and drop those the user closed (or whose session
    /// has gone).
    fn check_browsers(&mut self) {
        let sessions = self.sessions.items();
        self.browsers
            .forget_closed(|id| sessions.iter().any(|s| s.id == id));
        self.browsers.retitle(|id, browser| {
            let label = sessions
                .iter()
                .find(|s| s.id == id)
                .map(|s| s.label().to_string())
                .unwrap_or_default();
            browser_title(&label, Some(&browser.state().title))
        });
    }

    /// Start the 500 ms browser check unless it's running; it stops once no browser is open.
    fn tick_browsers(&mut self, cx: &mut Context<Self>) {
        if self.browsers_ticking || self.browsers.is_empty() {
            return;
        }
        self.browsers_ticking = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(BROWSER_TICK).await;
                let open = this
                    .update(cx, |this, _| {
                        this.check_browsers();
                        this.browsers_ticking = !this.browsers.is_empty();
                        this.browsers_ticking
                    })
                    .unwrap_or(false);
                if !open {
                    break;
                }
            }
        })
        .detach();
    }

    /// File → Open Browser… (⇧⌘B): the address box, for the active session (none: nothing
    /// happens). With a browser already open it holds that page's address, selected. Already
    /// open: back to its box.
    pub(crate) fn open_address_box(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(address) = &self.address_box {
            address.focus_handle(cx).focus(window, cx);
            return;
        }
        let Some(session) = self.sessions.active() else {
            return;
        };
        let current = self
            .browsers
            .get(session.id)
            .map(|b| b.state().url)
            .filter(|url| url != "about:blank")
            .unwrap_or_default();
        let address = cx.new(|cx| AddressBox::new(current, window, cx));
        cx.subscribe_in(&address, window, Self::on_address_event)
            .detach();
        address.focus_handle(cx).focus(window, cx);
        self.address_box = Some(address);
        cx.notify();
    }

    /// Enter in the address box opens the address for the active session, and the box goes;
    /// a refused address stays, with why. Closed: the typing returns to the terminal.
    fn on_address_event(
        &mut self,
        address: &Entity<AddressBox>,
        event: &AddressBoxEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A box already closed can still report losing focus: ignore it.
        if self.address_box.as_ref() != Some(address) {
            return;
        }
        if let AddressBoxEvent::Open(text) = event
            && let Some(index) = self.sessions.active_index()
            && let Err(why) = self.browser_open(index, text, cx)
        {
            address.update(cx, |address, cx| address.show_error(why, cx));
            return;
        }
        self.address_box = None;
        self.focus_active(window, cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    use super::*;

    /// A browser that says when it has been dropped (its window closed).
    struct Fake {
        closed: Cell<bool>,
        title: RefCell<String>,
        dropped: Rc<Cell<bool>>,
    }

    impl Fake {
        fn new() -> (Fake, Rc<Cell<bool>>) {
            let dropped = Rc::new(Cell::new(false));
            let fake = Fake {
                closed: Cell::new(false),
                title: RefCell::new(String::new()),
                dropped: dropped.clone(),
            };
            (fake, dropped)
        }
    }

    impl Drop for Fake {
        fn drop(&mut self) {
            self.dropped.set(true);
        }
    }

    impl BrowserWindow for Fake {
        fn is_closed(&self) -> bool {
            self.closed.get()
        }

        fn set_title(&self, title: &str) {
            self.title.replace(title.to_string());
        }
    }

    #[test]
    fn browser_titles_name_the_session_then_the_page() {
        assert_eq!(browser_title("api", None), "api");
        assert_eq!(browser_title("api", Some("")), "api");
        assert_eq!(browser_title("api", Some("  ")), "api");
        assert_eq!(browser_title("api", Some("Docs")), "api \u{2014} Docs");
    }

    #[test]
    fn closing_a_session_drops_its_browser() {
        let mut browsers = Browsers::new();
        let (one, one_dropped) = Fake::new();
        let (two, two_dropped) = Fake::new();
        browsers.insert(1, one, "one".into());
        browsers.insert(2, two, "two".into());
        browsers.session_closed(1);
        assert!(one_dropped.get(), "its window closes");
        assert!(browsers.get(1).is_none());
        assert!(!two_dropped.get());
        assert!(browsers.get(2).is_some());
        // A session without a browser: nothing happens.
        browsers.session_closed(7);
        assert!(browsers.get(2).is_some());
    }

    #[test]
    fn a_user_closed_window_is_forgotten() {
        let mut browsers = Browsers::new();
        let (one, one_dropped) = Fake::new();
        let (two, two_dropped) = Fake::new();
        let (three, three_dropped) = Fake::new();
        browsers.insert(1, one, "one".into());
        browsers.insert(2, two, "two".into());
        browsers.insert(3, three, "three".into());
        browsers.get(1).unwrap().closed.set(true);
        // Session 3 is gone too (its browser outlived it).
        browsers.forget_closed(|session| session != 3);
        assert!(one_dropped.get() && browsers.get(1).is_none());
        assert!(three_dropped.get() && browsers.get(3).is_none());
        assert!(!two_dropped.get() && browsers.get(2).is_some());
    }

    #[test]
    fn a_window_is_retitled_only_when_its_title_changes() {
        let mut browsers = Browsers::new();
        let (one, _) = Fake::new();
        browsers.insert(1, one, "api".into());
        browsers.retitle(|_, _| "api".into());
        assert_eq!(
            *browsers.get(1).unwrap().title.borrow(),
            "",
            "unchanged: not set"
        );
        browsers.retitle(|session, _| format!("s{session} \u{2014} Docs"));
        assert_eq!(*browsers.get(1).unwrap().title.borrow(), "s1 \u{2014} Docs");
    }

    #[test]
    fn a_reconnected_session_keeps_its_browser() {
        let mut browsers = Browsers::new();
        let (one, one_dropped) = Fake::new();
        browsers.insert(1, one, "box".into());
        browsers.rekey(1, 5);
        assert!(!one_dropped.get(), "the window stays open");
        assert!(browsers.get(1).is_none());
        assert!(browsers.get(5).is_some());
        // Still known by its new id to the sweep.
        browsers.forget_closed(|session| session == 5);
        assert!(browsers.get(5).is_some());
        // A session without a browser: nothing happens.
        browsers.rekey(2, 6);
        assert!(browsers.get(6).is_none());
    }

    #[test]
    fn a_new_browser_for_a_session_replaces_the_old_one() {
        let mut browsers = Browsers::new();
        let (old, old_dropped) = Fake::new();
        let (new, _) = Fake::new();
        browsers.insert(1, old, "old".into());
        browsers.insert(1, new, "new".into());
        assert!(old_dropped.get());
        assert!(browsers.get(1).is_some());
    }
}
