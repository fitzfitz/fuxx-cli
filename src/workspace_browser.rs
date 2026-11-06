//! M10c: the workspace's browsers, one per session at most (`Browsers`), and File → Open
//! Browser… (⇧⌘B): a box over the window for an address, opened for the active session.
//!
//! A browser window is titled `<session name> — <page title>`. One timer runs while any browser
//! is open (every 500 ms, 100 ms while a page loads): it keeps the titles and the top bars up to
//! date (a renamed session, a new page, the loading line) and forgets the windows the user
//! closed. Closing a session closes its browser. The bars follow the theme and the
//! `browser-address-bar` setting as soon as they change.
//!
//! Also the `browser.*` socket methods (`fuxx browser …`, the MCP `browser_*` tools): checked
//! at once, then answered from a GPUI task once the page has loaded or answered, each wait
//! with its time limit, so the UI thread never waits for a page.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gpui::{
    App, AsyncApp, BackgroundExecutor, Context, Entity, EventEmitter, FocusHandle, Focusable,
    MouseDownEvent, Subscription, WeakEntity, Window, div, prelude::*, px,
};
use objc2::MainThreadMarker;
use serde_json::{Value, json};

use crate::api::{self, ApiError, Request};
use crate::api_server::Conn;
use crate::appearance::Appearance;
use crate::browser::{
    BarLook, Browser, CLOSED, PageState, didnt_answer, forget_saved_data, within,
};
use crate::browser_js::{self, Snapshot};
use crate::browser_url::normalize_url;
use crate::name_input::{NameInput, NameInputEvent};
use crate::overlay_panel;
use crate::terminal_view::hsla;
use crate::theme::Theme;
use crate::ui_tokens::text;
use crate::workspace::{Overlay, Workspace};
use crate::workspace_api::{now_ms, opt_str, required_str};

/// How often the open browsers' titles and top bars are brought up to date and closed windows
/// noticed; more often while a page loads, so the loading line moves smoothly.
const BROWSER_TICK: Duration = Duration::from_millis(500);
const BROWSER_TICK_LOADING: Duration = Duration::from_millis(100);

/// How often `open`, `back`, `reload` and `wait` look at the page.
const BROWSER_POLL: Duration = Duration::from_millis(200);
/// How long `open`, `back` and `reload` wait for the page to load.
const LOAD_LIMIT: Duration = Duration::from_secs(30);
/// How long a script (`eval`, and fuxx's own) or a screenshot may take.
const SCRIPT_LIMIT: Duration = Duration::from_secs(10);
/// `wait`'s limit when none is given, and the longest it takes.
const WAIT_DEFAULT: Duration = Duration::from_secs(30);
const WAIT_MAX: Duration = Duration::from_secs(300);

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

/// The browsers' top bar for the fuxx theme and the `browser-address-bar` setting: dark with a
/// dark theme, its loading line in the theme's accent.
pub fn bar_look(theme: &Theme, address_bar: bool) -> BarLook {
    BarLook {
        dark: theme.is_dark(),
        accent: theme.ui().accent,
        address_bar,
    }
}

/// The wait before the next browser check, from each browser's (loading, window can be seen):
/// shorter only while a page loads in a window someone can see (a hidden one's loading line
/// needn't move, and the check costs battery).
fn tick_interval(browsers: impl IntoIterator<Item = (bool, bool)>) -> Duration {
    let mut browsers = browsers.into_iter();
    if browsers.any(|(loading, seen)| loading && seen) {
        BROWSER_TICK_LOADING
    } else {
        BROWSER_TICK
    }
}

/// Whether the saved browser data should be deleted now: `browser-keep-data` is off, a browser
/// was opened with it on at some time (`flag`, kept in the sessions file) and no window opened
/// with it on is still open (it keeps writing to the saved store until it closes).
fn should_wipe(setting_on: bool, flag: bool, open_on_windows: usize) -> bool {
    !setting_on && flag && open_on_windows == 0
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

    /// `session`'s browser while its window is open; else the error every browser command
    /// but `open` answers.
    pub fn live(&self, session: u64) -> Result<&B, ApiError> {
        self.get(session).filter(|b| !b.is_closed()).ok_or_else(|| {
            ApiError::not_found(format!(
                "no browser for s{session} (fuxx browser open URL first)"
            ))
        })
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

    /// Every open browser.
    pub fn iter(&self) -> impl Iterator<Item = &B> {
        self.open.values().map(|(b, _)| b)
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
        let empty = self.input.read(cx).text().is_empty();
        let field = overlay_panel::filter_box(&ui, &self.input, empty, PLACEHOLDER);
        let line = match &self.error {
            // The waiting ring's colour: the theme's red.
            Some(why) => div().text_color(hsla(ui.ring_waiting)).child(why.clone()),
            None => div().text_color(hsla(ui.text_dim)).child(HINT),
        };
        overlay_panel::shell(&ui, PANEL_WIDTH)
            .key_context("AddressBox")
            // The terminal under the panel gets neither its clicks nor its hover.
            .occlude()
            .on_mouse_down_out(
                cx.listener(|_, _: &MouseDownEvent, _, cx| cx.emit(AddressBoxEvent::Cancel)),
            )
            .child(field)
            .child(line.px_1().text_size(px(text::META)))
    }
}

impl Workspace {
    /// Open `url` in session `index`'s browser, making the browser first if it has none, and
    /// bring it to the front: `take_keyboard` (the person's own open) also makes it the key
    /// window, an agent's open leaves the typing where it is. A refused address (see `normalize_url`) is the error; a page that
    /// fails to load shows in the browser (and in its `state`).
    pub(crate) fn browser_open(
        &mut self,
        index: usize,
        url: &str,
        take_keyboard: bool,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let url = normalize_url(url)?;
        let Some(session) = self.sessions.items().get(index) else {
            return Err("no such session".to_string());
        };
        let id = session.id;
        if self.browsers.get(id).is_none() {
            let title = browser_title(&session.label(), None);
            // Windows opened from now on follow the setting; open ones keep their store.
            let appearance = cx.global::<Appearance>();
            let keep_data = appearance.browser_keep_data;
            let look = bar_look(&appearance.theme, appearance.browser_address_bar);
            let browser = Browser::open(&title, keep_data, &look, cx);
            if keep_data && !self.browser_data_kept {
                // The saved store may now hold data; remembered on disk so that turning the
                // setting off later deletes it even after a restart.
                self.browser_data_kept = true;
                self.save();
            }
            self.browsers.insert(id, browser, title);
            self.tick_browsers(cx);
        }
        if let Some(browser) = self.browsers.get(id) {
            browser.load(&url);
            browser.bring_to_front(take_keyboard);
        }
        Ok(())
    }

    /// The settings changed: the open browsers' top bars follow the theme and
    /// `browser-address-bar` at once, and saved data is deleted if `browser-keep-data` just went
    /// off (`wipe_saved_data_if_due`); windows already open keep the store they were opened with.
    pub(crate) fn browser_settings_changed(&mut self, cx: &mut Context<Self>) {
        let appearance = cx.global::<Appearance>();
        let look = bar_look(&appearance.theme, appearance.browser_address_bar);
        for browser in self.browsers.iter() {
            browser.set_look(&look);
        }
        self.wipe_saved_data_if_due(cx);
    }

    /// Delete the saved browser data when `should_wipe` says so (at launch, when the setting
    /// goes off, when the last window opened with it on closes). The flag in the sessions file
    /// is cleared only once WebKit says the deletion is done, so a quit meanwhile retries it at
    /// the next launch.
    pub(crate) fn wipe_saved_data_if_due(&mut self, cx: &mut Context<Self>) {
        let keep_data = cx.global::<Appearance>().browser_keep_data;
        let on_windows = self.browsers.iter().filter(|b| b.keeps_data()).count();
        if self.browser_wiping || !should_wipe(keep_data, self.browser_data_kept, on_windows) {
            return;
        }
        self.browser_wiping = true;
        let done = forget_saved_data(
            MainThreadMarker::new().expect("GPUI's App lives on the main thread"),
        );
        cx.spawn(async move |this, cx| {
            let finished = done.await.is_ok();
            this.update(cx, |this, _| {
                this.browser_wiping = false;
                // A window opened with it on since then writes there again: the flag stays.
                if finished && this.browsers.iter().all(|b| !b.keeps_data()) {
                    this.browser_data_kept = false;
                    this.save();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Bring the browsers' titles and top bars up to date and drop those the user closed (or
    /// whose session has gone). The wait before the next check (see `tick_interval`).
    fn check_browsers(&mut self, cx: &mut Context<Self>) -> Duration {
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
        let mut seen = vec![];
        for browser in self.browsers.iter() {
            browser.refresh_bar();
            seen.push((browser.state().loading, browser.can_be_seen()));
        }
        // The last window opened with the setting on may just have closed.
        self.wipe_saved_data_if_due(cx);
        tick_interval(seen)
    }

    /// Start the browser check (every 500 ms, 100 ms while a page loads) unless it's running;
    /// it stops once no browser is open.
    fn tick_browsers(&mut self, cx: &mut Context<Self>) {
        if self.browsers_ticking || self.browsers.is_empty() {
            return;
        }
        self.browsers_ticking = true;
        cx.spawn(async move |this, cx| {
            let mut wait = BROWSER_TICK_LOADING;
            loop {
                cx.background_executor().timer(wait).await;
                let next = this
                    .update(cx, |this, cx| {
                        let next = this.check_browsers(cx);
                        this.browsers_ticking = !this.browsers.is_empty();
                        this.browsers_ticking.then_some(next)
                    })
                    .ok()
                    .flatten();
                match next {
                    Some(next) => wait = next,
                    None => break,
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
        self.close_overlays(Overlay::AddressBox);
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
            && let Err(why) = self.browser_open(index, text, true, cx)
        {
            address.update(cx, |address, cx| address.show_error(why, cx));
            return;
        }
        self.address_box = None;
        self.focus_active(window, cx);
        cx.notify();
    }
}

/// A `browser.*` request, its parameters checked.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum BrowserCommand {
    Open {
        url: String,
    },
    Snapshot,
    Click {
        r#ref: String,
    },
    Fill {
        r#ref: String,
        text: String,
    },
    Press {
        key: String,
    },
    Read,
    Eval {
        js: String,
    },
    /// Until the page shows `text` and has an element matching `selector` (at least one is
    /// given), for at most `limit`.
    Wait {
        text: Option<String>,
        selector: Option<String>,
        limit: Duration,
    },
    /// To `path`, or a new file in the temp folder.
    Screenshot {
        path: Option<PathBuf>,
    },
    Url,
    Back,
    Reload,
    Close,
}

impl BrowserCommand {
    /// The command `method` asks for, with its parameters from `p`.
    pub fn parse(method: &str, p: &Value) -> Result<Self, ApiError> {
        let text = |key: &str| required_str(p, key).map(str::to_string);
        Ok(match method {
            "browser.open" => Self::Open { url: text("url")? },
            "browser.snapshot" => Self::Snapshot,
            "browser.click" => Self::Click {
                r#ref: text("ref")?,
            },
            "browser.fill" => Self::Fill {
                r#ref: text("ref")?,
                text: text("text")?,
            },
            "browser.press" => Self::Press { key: text("key")? },
            "browser.read" => Self::Read,
            "browser.eval" => Self::Eval { js: text("js")? },
            "browser.wait" => wait_params(p)?,
            "browser.screenshot" => Self::Screenshot {
                path: screenshot_path(p)?,
            },
            "browser.url" => Self::Url,
            "browser.back" => Self::Back,
            "browser.reload" => Self::Reload,
            "browser.close" => Self::Close,
            other => {
                return Err(ApiError::unknown_method(format!(
                    "unknown method \"{other}\" (see system.version)"
                )));
            }
        })
    }
}

/// `browser.wait`'s `text` and `selector` (empty counts as absent; one is needed) and its
/// `timeout_ms` (30 s when absent, at most 300 s).
fn wait_params(p: &Value) -> Result<BrowserCommand, ApiError> {
    let given = |key: &str| -> Result<Option<String>, ApiError> {
        Ok(opt_str(p, key)?
            .filter(|s| !s.is_empty())
            .map(str::to_string))
    };
    let (text, selector) = (given("text")?, given("selector")?);
    if text.is_none() && selector.is_none() {
        return Err(ApiError::bad_request("wait needs text or selector"));
    }
    let limit = match p.get("timeout_ms") {
        None | Some(Value::Null) => WAIT_DEFAULT,
        Some(ms) => ms
            .as_u64()
            .map(Duration::from_millis)
            .ok_or_else(|| {
                ApiError::bad_request("timeout_ms must be a whole number of milliseconds")
            })?
            .min(WAIT_MAX),
    };
    Ok(BrowserCommand::Wait {
        text,
        selector,
        limit,
    })
}

/// `browser.screenshot`'s `path`: absent or empty is a temp file; else a full path in a
/// folder that exists (the PNG is written as is, replacing any file there).
fn screenshot_path(p: &Value) -> Result<Option<PathBuf>, ApiError> {
    let Some(path) = opt_str(p, "path")?.filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let path = PathBuf::from(path);
    if path.is_relative() {
        return Err(ApiError::bad_request(format!(
            "path must be a full path: {}",
            path.display()
        )));
    }
    match path.parent() {
        Some(folder) if folder.is_dir() => Ok(Some(path)),
        Some(folder) => Err(ApiError::not_found(format!(
            "no folder {}",
            folder.display()
        ))),
        None => Err(ApiError::bad_request("path must name a file")),
    }
}

/// What a click, fill or press script answered: `{ok: true}`, or its error as it is, but an
/// unknown ref says to take a new snapshot.
fn action_answer(answer: &Value) -> Result<(), String> {
    match answer.get("error") {
        Some(error) => {
            let error = error
                .as_str()
                .map_or_else(|| error.to_string(), str::to_string);
            if error.starts_with("unknown ref ") {
                Err(format!("{error} (take a new snapshot)"))
            } else {
                Err(error)
            }
        }
        None if answer.get("ok") == Some(&Value::Bool(true)) => Ok(()),
        None => Err(format!("the page's answer isn't ok or an error: {answer}")),
    }
}

/// Once the page has stopped loading: its address and title, or why it failed to load.
fn settled(state: &PageState) -> Option<Result<Value, ApiError>> {
    if state.loading {
        return None;
    }
    Some(match &state.error {
        Some(error) => Err(ApiError::refused(error.clone())),
        None => Ok(json!({ "url": state.url, "title": state.title })),
    })
}

/// A screenshot's file when none is named: `fuxx-browser-s<N>-<unix ms>.png` in `dir`.
fn screenshot_file(dir: &Path, session: u64, ms: u64) -> PathBuf {
    dir.join(format!("fuxx-browser-s{session}-{ms}.png"))
}

/// A page's own error as the reply's: a time limit passed is `timeout`, the rest `refused`.
fn page_error(message: String) -> ApiError {
    if message.starts_with("the page didn't answer") {
        ApiError::timeout(message)
    } else {
        ApiError::refused(message)
    }
}

impl Workspace {
    /// A `browser.*` request for the target session (by default the caller's). Bad parameters
    /// and a session with no browser are answered now, as are `url` and `close`; the rest
    /// (`None`) reply to `conn` from a GPUI task once the page has loaded or answered.
    pub(crate) fn browser_request(
        &mut self,
        request: &Request,
        conn: &Conn,
        cx: &mut Context<Self>,
    ) -> Result<Option<Value>, ApiError> {
        let p = &request.params;
        let command = BrowserCommand::parse(&request.method, p)?;
        let index = self.target_index(p, "target")?;
        let session = self.sessions.items()[index].id;
        match &command {
            BrowserCommand::Open { url } => {
                self.browser_open(index, url, false, cx)
                    .map_err(ApiError::bad_request)?;
            }
            BrowserCommand::Url => {
                let state = self.browsers.live(session)?.state();
                return Ok(Some(json!({
                    "url": state.url,
                    "title": state.title,
                    "loading": state.loading,
                })));
            }
            BrowserCommand::Close => {
                self.browsers.live(session)?;
                self.browsers.session_closed(session);
                return Ok(Some(json!({})));
            }
            _ => {
                self.browsers.live(session)?;
            }
        }
        let (conn, id) = (conn.clone(), request.id);
        cx.spawn(async move |this, cx| {
            let page = SessionPage {
                workspace: this,
                session,
                executor: cx.background_executor().clone(),
            };
            let result = page.run(command, cx).await;
            conn.send_line(&api::reply(id, result));
        })
        .detach();
        Ok(None)
    }
}

/// A session's browser, as a command running in a GPUI task reaches it: one gone meanwhile
/// (its window or session closed) answers `browser closed`.
struct SessionPage {
    workspace: WeakEntity<Workspace>,
    session: u64,
    executor: BackgroundExecutor,
}

impl SessionPage {
    /// `command`'s reply (`url` and `close` are answered before any task).
    async fn run(&self, command: BrowserCommand, cx: &mut AsyncApp) -> Result<Value, ApiError> {
        match command {
            // `browser_request` has started the load.
            BrowserCommand::Open { .. } => self.loaded(cx).await,
            BrowserCommand::Back => {
                self.with(cx, Browser::back)?;
                self.loaded(cx).await
            }
            BrowserCommand::Reload => {
                self.with(cx, Browser::reload)?;
                self.loaded(cx).await
            }
            BrowserCommand::Snapshot => {
                let value = self.script(cx, browser_js::snapshot_js()).await?;
                let snapshot: Snapshot = serde_json::from_value(value).map_err(|e| {
                    ApiError::refused(format!("the page's snapshot didn't parse ({e})"))
                })?;
                Ok(json!(snapshot))
            }
            BrowserCommand::Click { r#ref } => self.act(cx, &browser_js::click_js(&r#ref)).await,
            BrowserCommand::Fill { r#ref, text } => {
                self.act(cx, &browser_js::fill_js(&r#ref, &text)).await
            }
            BrowserCommand::Press { key } => self.act(cx, &browser_js::press_js(&key)).await,
            BrowserCommand::Read => {
                let text = self.script(cx, browser_js::read_js()).await?;
                Ok(json!({ "text": text }))
            }
            BrowserCommand::Eval { js } => {
                let answer = self.with(cx, |b| b.eval(&js))?;
                let value = within(answer, SCRIPT_LIMIT, &self.executor)
                    .await
                    .map_err(page_error)?;
                Ok(json!({ "value": value }))
            }
            BrowserCommand::Wait {
                text,
                selector,
                limit,
            } => {
                let check = browser_js::wait_check_js(text.as_deref(), selector.as_deref());
                self.shows(cx, &check, limit).await
            }
            BrowserCommand::Screenshot { path } => self.screenshot(cx, path).await,
            BrowserCommand::Url | BrowserCommand::Close => {
                unreachable!("answered in browser_request")
            }
        }
    }

    /// `f` of the browser, if it is still there.
    fn with<R>(&self, cx: &AsyncApp, f: impl FnOnce(&Browser) -> R) -> Result<R, ApiError> {
        self.workspace
            .read_with(cx, |this, _| this.browsers.get(self.session).map(f))
            .ok()
            .flatten()
            .ok_or_else(|| ApiError::refused(CLOSED))
    }

    /// One of fuxx's own page scripts' value (run once, never retried), within the script limit.
    async fn script(&self, cx: &AsyncApp, script: &str) -> Result<Value, ApiError> {
        let answer = self.with(cx, |b| b.run_script(script))?;
        within(answer, SCRIPT_LIMIT, &self.executor)
            .await
            .map_err(page_error)
    }

    /// A click, fill or press: `{}` once the page has done it.
    async fn act(&self, cx: &AsyncApp, script: &str) -> Result<Value, ApiError> {
        let answer = self.script(cx, script).await?;
        action_answer(&answer).map_err(ApiError::refused)?;
        Ok(json!({}))
    }

    /// The address and title once the page has loaded (looked at every 200 ms, for at most
    /// 30 s), or why it didn't.
    async fn loaded(&self, cx: &AsyncApp) -> Result<Value, ApiError> {
        let deadline = Instant::now() + LOAD_LIMIT;
        loop {
            self.executor.timer(BROWSER_POLL).await;
            if let Some(reply) = settled(&self.with(cx, Browser::state)?) {
                return reply;
            }
            if Instant::now() >= deadline {
                return Err(ApiError::timeout(didnt_answer(LOAD_LIMIT)));
            }
        }
    }

    /// `{}` once the wait script `check` answers true (asked every 200 ms, for at most
    /// `limit`); its error (a bad selector) at once.
    async fn shows(&self, cx: &AsyncApp, check: &str, limit: Duration) -> Result<Value, ApiError> {
        let deadline = Instant::now() + limit;
        loop {
            if self.script(cx, check).await? == json!(true) {
                return Ok(json!({}));
            }
            if Instant::now() >= deadline {
                return Err(ApiError::timeout("timed out waiting"));
            }
            self.executor.timer(BROWSER_POLL).await;
        }
    }

    /// A PNG of the page, written to `path` (or a new temp file) off the UI thread; its path.
    async fn screenshot(&self, cx: &AsyncApp, path: Option<PathBuf>) -> Result<Value, ApiError> {
        let answer = self.with(cx, Browser::screenshot)?;
        let png = within(answer, SCRIPT_LIMIT, &self.executor)
            .await
            .map_err(page_error)?;
        let path =
            path.unwrap_or_else(|| screenshot_file(&std::env::temp_dir(), self.session, now_ms()));
        let file = path.clone();
        cx.background_spawn(async move { std::fs::write(file, png) })
            .await
            .map_err(|e| ApiError::refused(format!("couldn't write {}: {e}", path.display())))?;
        Ok(json!({ "path": path.display().to_string() }))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    use super::*;
    use alacritty_terminal::vte::ansi::Rgb;

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

    #[test]
    fn a_command_without_a_browser_errors() {
        let mut browsers: Browsers<Fake> = Browsers::new();
        let none = browsers.live(3).err().expect("no browser yet");
        assert_eq!(
            (none.code, none.message.as_str()),
            (
                "not_found",
                "no browser for s3 (fuxx browser open URL first)"
            )
        );
        let (one, _) = Fake::new();
        browsers.insert(3, one, "api".into());
        assert!(browsers.live(3).is_ok());
        // A window the user closed (not forgotten yet) is no browser either.
        browsers.get(3).unwrap().closed.set(true);
        let closed = browsers.live(3).err().expect("its window closed");
        assert_eq!(
            closed.message,
            "no browser for s3 (fuxx browser open URL first)"
        );
    }

    fn parse(method: &str, p: Value) -> Result<BrowserCommand, ApiError> {
        BrowserCommand::parse(method, &p)
    }

    #[test]
    fn wait_needs_text_or_selector() {
        for neither in [
            json!({}),
            json!({"text": ""}),
            json!({"text": null, "selector": ""}),
            json!({"timeout_ms": 1000}),
        ] {
            let e = parse("browser.wait", neither.clone()).unwrap_err();
            assert_eq!(
                (e.code, e.message.as_str()),
                ("bad_request", "wait needs text or selector"),
                "{neither}"
            );
        }
        let wait = |text: Option<&str>, selector: Option<&str>, secs: u64| BrowserCommand::Wait {
            text: text.map(str::to_string),
            selector: selector.map(str::to_string),
            limit: Duration::from_secs(secs),
        };
        assert_eq!(
            parse("browser.wait", json!({"text": "Hi"})),
            Ok(wait(Some("Hi"), None, 30))
        );
        assert_eq!(
            parse(
                "browser.wait",
                json!({"selector": "#go", "timeout_ms": 5000})
            ),
            Ok(wait(None, Some("#go"), 5))
        );
        assert_eq!(
            parse("browser.wait", json!({"text": "a", "selector": "b"})),
            Ok(wait(Some("a"), Some("b"), 30))
        );
        // At most 300 s.
        assert_eq!(
            parse("browser.wait", json!({"text": "a", "timeout_ms": 900_000})),
            Ok(wait(Some("a"), None, 300))
        );
        for bad in [json!("5"), json!(-1), json!(1.5)] {
            let p = json!({"text": "a", "timeout_ms": bad});
            assert_eq!(parse("browser.wait", p).unwrap_err().code, "bad_request");
        }
        assert_eq!(
            parse("browser.wait", json!({"text": 3})).unwrap_err().code,
            "bad_request"
        );
    }

    #[test]
    fn browser_methods_take_their_params() {
        let s = str::to_string;
        assert_eq!(
            parse(
                "browser.open",
                json!({"url": "localhost:3000", "target": "s2"})
            ),
            Ok(BrowserCommand::Open {
                url: s("localhost:3000")
            })
        );
        assert_eq!(
            parse("browser.click", json!({"ref": "e3"})),
            Ok(BrowserCommand::Click { r#ref: s("e3") })
        );
        // Empty text clears a field.
        assert_eq!(
            parse("browser.fill", json!({"ref": "e5", "text": ""})),
            Ok(BrowserCommand::Fill {
                r#ref: s("e5"),
                text: s("")
            })
        );
        assert_eq!(
            parse("browser.press", json!({"key": "Enter"})),
            Ok(BrowserCommand::Press { key: s("Enter") })
        );
        assert_eq!(
            parse("browser.eval", json!({"js": "document.title"})),
            Ok(BrowserCommand::Eval {
                js: s("document.title")
            })
        );
        for (method, command) in [
            ("browser.snapshot", BrowserCommand::Snapshot),
            ("browser.read", BrowserCommand::Read),
            ("browser.url", BrowserCommand::Url),
            ("browser.back", BrowserCommand::Back),
            ("browser.reload", BrowserCommand::Reload),
            ("browser.close", BrowserCommand::Close),
            (
                "browser.screenshot",
                BrowserCommand::Screenshot { path: None },
            ),
        ] {
            assert_eq!(parse(method, json!({"caller": 3})), Ok(command), "{method}");
        }
        for (method, missing) in [
            ("browser.open", json!({})),
            ("browser.click", json!({})),
            ("browser.fill", json!({"ref": "e5"})),
            ("browser.fill", json!({"text": "x"})),
            ("browser.press", json!({})),
            ("browser.eval", json!({"js": 1})),
        ] {
            let e = parse(method, missing.clone()).unwrap_err();
            assert_eq!(e.code, "bad_request", "{method} {missing}");
        }
        let e = parse("browser.forward", json!({})).unwrap_err();
        assert_eq!(e.code, "unknown_method");
        assert!(e.message.contains("browser.forward"), "{}", e.message);
    }

    #[test]
    fn a_screenshot_goes_where_asked_or_to_a_temp_file() {
        let tmp = std::env::temp_dir();
        let shot = tmp.join("shot.png");
        assert_eq!(
            parse(
                "browser.screenshot",
                json!({"path": shot.display().to_string()})
            ),
            Ok(BrowserCommand::Screenshot { path: Some(shot) })
        );
        assert_eq!(
            parse("browser.screenshot", json!({"path": ""})),
            Ok(BrowserCommand::Screenshot { path: None })
        );
        let relative = parse("browser.screenshot", json!({"path": "shot.png"})).unwrap_err();
        assert_eq!(relative.code, "bad_request");
        let nowhere = tmp.join("fuxx-no-such-folder-5b1e").join("shot.png");
        let e = parse(
            "browser.screenshot",
            json!({"path": nowhere.display().to_string()}),
        )
        .unwrap_err();
        assert_eq!(e.code, "not_found");
        assert!(
            e.message.contains("fuxx-no-such-folder-5b1e"),
            "{}",
            e.message
        );
        assert_eq!(
            screenshot_file(Path::new("/tmp/x"), 4, 1_700_000_000_123),
            PathBuf::from("/tmp/x/fuxx-browser-s4-1700000000123.png")
        );
    }

    #[test]
    fn page_actions_answer_ok_or_their_error() {
        assert_eq!(action_answer(&json!({"ok": true})), Ok(()));
        assert_eq!(
            action_answer(&json!({"error": "unknown ref e9"})),
            Err("unknown ref e9 (take a new snapshot)".to_string())
        );
        // Other errors (fill, press) pass through as they are.
        assert_eq!(
            action_answer(&json!({"error": "e3 is not a text field"})),
            Err("e3 is not a text field".to_string())
        );
        assert_eq!(
            action_answer(&json!({"error": "unknown key \"F13\" (use Enter, …)"})),
            Err("unknown key \"F13\" (use Enter, …)".to_string())
        );
        assert!(action_answer(&json!(null)).is_err());
        assert!(action_answer(&json!({"ok": false})).is_err());
    }

    #[test]
    fn a_page_settles_once_it_stops_loading() {
        let state = |loading: bool, error: Option<&str>| PageState {
            url: "http://localhost:3000/".into(),
            title: "App".into(),
            loading,
            error: error.map(str::to_string),
        };
        assert_eq!(settled(&state(true, None)), None);
        assert_eq!(
            settled(&state(false, None)),
            Some(Ok(json!({"url": "http://localhost:3000/", "title": "App"})))
        );
        let failed = "the page failed to load: Could not connect to the server.";
        let e = settled(&state(false, Some(failed))).unwrap().unwrap_err();
        assert_eq!((e.code, e.message.as_str()), ("refused", failed));
        // Timeouts are `timeout` (exit 124); other page errors are refusals.
        assert_eq!(page_error(didnt_answer(SCRIPT_LIMIT)).code, "timeout");
        assert_eq!(page_error("the script failed: x".into()).code, "refused");
    }

    #[test]
    fn saved_data_is_deleted_only_when_off_flagged_and_no_on_window_is_open() {
        // Off, the saved store may hold data, no window opened with it on is left.
        assert!(should_wipe(false, true, 0));
        // Nothing saved (the flag is clear): nothing to delete.
        assert!(!should_wipe(false, false, 0));
        // Still on: keep it.
        assert!(!should_wipe(true, true, 0));
        assert!(!should_wipe(true, false, 0));
        // A window opened with it on still writes there: wait until it closes.
        assert!(!should_wipe(false, true, 1));
        assert!(!should_wipe(false, true, 3));
    }

    #[test]
    fn the_bar_follows_the_theme_and_the_setting() {
        let dark = Theme::default();
        assert_eq!(
            bar_look(&dark, true),
            BarLook {
                dark: true,
                accent: dark.ui().accent,
                address_bar: true,
            }
        );
        let light = Theme {
            background: Rgb {
                r: 0xff,
                g: 0xff,
                b: 0xff,
            },
            foreground: Rgb { r: 0, g: 0, b: 0 },
            ..Theme::default()
        };
        assert_eq!(
            bar_look(&light, false),
            BarLook {
                dark: false,
                accent: light.ui().accent,
                address_bar: false,
            }
        );
    }

    #[test]
    fn the_check_runs_faster_while_a_page_loads() {
        assert_eq!(tick_interval([]), BROWSER_TICK);
        assert_eq!(tick_interval([(false, true)]), BROWSER_TICK);
        assert_eq!(tick_interval([(true, true)]), BROWSER_TICK_LOADING);
        // Loading in a window no one can see (hidden, minimised, covered): the slow check.
        assert_eq!(tick_interval([(true, false)]), BROWSER_TICK);
        assert_eq!(tick_interval([(true, false), (false, true)]), BROWSER_TICK);
        assert_eq!(
            tick_interval([(true, false), (true, true)]),
            BROWSER_TICK_LOADING
        );
        assert!(BROWSER_TICK_LOADING < BROWSER_TICK);
    }
}
