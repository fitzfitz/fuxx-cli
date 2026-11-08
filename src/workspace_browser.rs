//! The workspace's browsers (M11b): browser tabs in its panes (`BrowserTab`, each with its page
//! over its pane), the windows browser tabs are popped out into (M10c's `Browser`), File → Open
//! Browser… (⇧⌘B, a box over the window for an address), and agents' `browser.*`.
//!
//! An agent's `fuxx browser open` uses the agent's workspace's browser tab (the most recently
//! used, if it has several), else splits the agent's pane to the right with a new one; it never
//! moves the person's focus. Every other `browser.*` command acts on that tab, and fails as before
//! when there is none. Each is checked at once, then answered from a GPUI task once the page has
//! loaded or answered, each wait with its time limit, so the UI thread never waits for a page.
//!
//! A popped-out window is titled with its page's title. One timer runs while any is open (every
//! 500 ms, 100 ms while a page loads): it keeps the titles and top bars up to date and forgets
//! the windows the user closed. Agents don't reach popped-out windows (they use browser tabs).

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
    BarLook, Browser, CLOSED, Page, PageState, didnt_answer, forget_saved_data, within,
};
use crate::browser_history::{BrowserHistory, enter_target};
use crate::browser_js::{self, Snapshot};
use crate::browser_tab::{BrowserTab, BrowserTabEvent};
use crate::browser_url::normalize_url;
use crate::layout_model::{Model, PaneId, TabId, TabKind, WorkspaceId};
use crate::name_input::{NameInput, NameInputEvent};
use crate::overlay_panel;
use crate::pane_browser::{Overlays, Showing};
use crate::split_tree::Axis;
use crate::terminal_view::hsla;
use crate::theme::Theme;
use crate::ui_tokens::text;
use crate::workspace::{Overlay, Workspace};
use crate::workspace_api::{now_ms, opt_str, required_str};

/// How often the popped-out windows' titles and top bars are brought up to date and closed
/// windows noticed; more often while a page loads, so the loading line moves smoothly.
const BROWSER_TICK: Duration = Duration::from_millis(500);
const BROWSER_TICK_LOADING: Duration = Duration::from_millis(100);

/// How often `open`, `back` and `reload` look at the page.
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

/// A popped-out window's title: its page's title, else "Browser".
pub fn popped_title(page: &str) -> String {
    match page.trim() {
        "" => "Browser".to_string(),
        page => page.to_string(),
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

/// Whether fuxx's work is all gone, so its window closes (and fuxx quits): no terminal, no
/// workspace, and no popped-out browser window still open (Pop Out of the last tab keeps fuxx).
pub(crate) fn nothing_left(terminals: usize, workspaces: usize, popped_open: usize) -> bool {
    terminals == 0 && workspaces == 0 && popped_open == 0
}

/// Whether closing `closed` popped-out windows (the user closed them) leaves nothing (see
/// `nothing_left`), so fuxx quits now: the last tab popped out, then its window closed. With
/// none closed, an empty window stays (the first run's, before any workspace).
pub(crate) fn quits_after_closing(
    closed: usize,
    terminals: usize,
    workspaces: usize,
    popped_open: usize,
) -> bool {
    closed > 0 && nothing_left(terminals, workspaces, popped_open)
}

/// Whether the saved browser data should be deleted now: `browser-keep-data` is off, a page was
/// made with it on at some time (`flag`, kept in the sessions file) and no page made with it on
/// is still open (a browser tab or window; it keeps writing to the saved store until it closes).
fn should_wipe(setting_on: bool, flag: bool, open_on_pages: usize) -> bool {
    !setting_on && flag && open_on_pages == 0
}

/// Where a browser goes for a workspace: its browser tab, or a new one split to the right of a
/// pane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserPlace {
    Reuse(TabId),
    Split(PaneId),
}

/// Where a browser goes in workspace `ws` for something in `pane`: the workspace's most recently
/// used browser tab (`used`: higher is more recent; a tie goes to the first in reading order),
/// else a new one split to the right of `pane`.
pub fn browser_place(
    model: &Model,
    ws: WorkspaceId,
    pane: PaneId,
    used: impl Fn(TabId) -> u64,
) -> BrowserPlace {
    let mut best: Option<(TabId, u64)> = None;
    for tab in model.tabs_of(ws) {
        if !matches!(
            model.tab(tab).map(|t| &t.kind),
            Some(TabKind::Browser { .. })
        ) {
            continue;
        }
        let u = used(tab);
        if best.is_none_or(|(_, b)| u > b) {
            best = Some((tab, u));
        }
    }
    match best {
        Some((tab, _)) => BrowserPlace::Reuse(tab),
        None => BrowserPlace::Split(pane),
    }
}

/// Where an agent's browser goes: `browser_place` in the workspace and pane of its terminal
/// `sN`; none when no tab shows it.
pub fn agent_place(
    model: &Model,
    terminal: u64,
    used: impl Fn(TabId) -> u64,
) -> Option<BrowserPlace> {
    let (ws, pane) = model.locate(model.terminal_tab(terminal)?)?;
    Some(browser_place(model, ws, pane, used))
}

/// The error every browser command but `open` answers when the agent's workspace has no
/// browser tab.
fn no_browser(session: u64) -> ApiError {
    ApiError::not_found(format!(
        "no browser for s{session} (fuxx browser open URL first)"
    ))
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

/// The popped-out browser windows, by a number of their own, each with the title its window
/// shows. Dropping a browser closes its window.
pub(crate) struct Browsers<B = Browser> {
    open: HashMap<u64, (B, String)>,
}

impl<B: BrowserWindow> Browsers<B> {
    pub fn new() -> Self {
        Self {
            open: HashMap::new(),
        }
    }

    #[cfg(test)]
    pub fn get(&self, id: u64) -> Option<&B> {
        self.open.get(&id).map(|(b, _)| b)
    }

    /// Keep `browser` as window `id`, titled `title`.
    pub fn insert(&mut self, id: u64, browser: B, title: String) {
        self.open.insert(id, (browser, title));
    }

    /// Drop the browsers whose window the user closed; how many.
    pub fn forget_closed(&mut self) -> usize {
        let before = self.open.len();
        self.open.retain(|_, (browser, _)| !browser.is_closed());
        before - self.open.len()
    }

    /// Give each window the title `title_for` its browser, when that changed.
    pub fn retitle(&mut self, title_for: impl Fn(&B) -> String) {
        for (browser, shown) in self.open.values_mut() {
            let title = title_for(browser);
            if *shown != title {
                browser.set_title(&title);
                *shown = title;
            }
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
    /// Whether nothing is left (`nothing_left`): the window then closes and fuxx quits.
    pub(crate) fn all_gone(&self) -> bool {
        let popped = self.browsers.iter().filter(|b| !b.is_closed()).count();
        nothing_left(
            self.sessions.items().len(),
            self.model.workspaces().len(),
            popped,
        )
    }

    /// The browser tab the focused pane shows, if it shows one.
    pub(crate) fn focused_browser_tab(&self) -> Option<&Entity<BrowserTab>> {
        self.browser_tabs.get(&self.model.focused_tab()?)
    }

    /// The browser tab pane `pane` shows, if it shows one.
    pub(crate) fn pane_browser_tab(&self, pane: PaneId) -> Option<Entity<BrowserTab>> {
        self.browser_tabs
            .get(&self.model.pane(pane)?.active)
            .cloned()
    }

    /// Which of fuxx's overlays are open: the one signal every page's hide rule reads.
    pub(crate) fn overlays(&self, cx: &App) -> Overlays {
        Overlays {
            rename: self.renaming.is_some(),
            host_picker: self.host_picker.is_some(),
            address_box: self.address_box.is_some(),
            switcher: self.switcher.is_some(),
            row_menu: self.sidebar.tab_menu.is_some(),
            tab_menu: self.pane_tab_menu.is_some(),
            pane_actions: self.pane_actions.is_some(),
            new_menu: self.sidebar.new_menu,
            suggestions: self
                .browser_tabs
                .values()
                .any(|v| v.read(cx).suggestions_open(cx)),
            notifications: self.notifications_popover.is_some(),
            help: self.help.is_some(),
        }
    }

    /// Every browser tab of the model has its view (a restored one, one a script made), and
    /// each view its page once fuxx's window is known. A new view for the focused tab takes the
    /// keyboard.
    pub(crate) fn ensure_browser_tabs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let missing: Vec<(TabId, Option<String>)> = self
            .model
            .workspaces()
            .iter()
            .flat_map(|w| &w.panes)
            .flat_map(|p| &p.tabs)
            .filter(|t| !self.browser_tabs.contains_key(&t.id))
            .filter_map(|t| match &t.kind {
                TabKind::Browser { url } => Some((t.id, url.clone())),
                TabKind::Terminal(_) | TabKind::File { .. } => None,
            })
            .collect();
        let focused = self.model.focused_tab();
        let mut focus_it = false;
        for (tab, url) in missing {
            self.make_browser_tab(tab, url, window, cx);
            focus_it |= Some(tab) == focused;
        }
        // The focused tab's view didn't exist when it was focused (a restore, a script).
        if focus_it {
            self.focus_active(window, cx);
        }
        for view in self.browser_tabs.values() {
            view.update(cx, |view, cx| view.ensure_page(cx));
        }
    }

    /// The view for browser tab `tab`, opening `url`. Its page uses the data store the setting
    /// picks now, for as long as it lives.
    pub(crate) fn make_browser_tab(
        &mut self,
        tab: TabId,
        url: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<BrowserTab> {
        let keep_data = cx.global::<Appearance>().browser_keep_data;
        if keep_data && !self.browser_data_kept {
            // The saved store may now hold data; remembered on disk so that turning the
            // setting off later deletes it even after a restart.
            self.browser_data_kept = true;
            self.save();
        }
        let view = cx.new(|cx| BrowserTab::new(tab, url, keep_data, cx));
        cx.subscribe_in(&view, window, Self::on_browser_tab_event)
            .detach();
        self.browser_titles.insert(tab, view.read(cx).title());
        self.browser_tabs.insert(tab, view.clone());
        view
    }

    /// A click in a page: its tab (pane, workspace) takes the focus and the page the keyboard.
    /// A page changed: the tab's title, and its saved address.
    fn on_browser_tab_event(
        &mut self,
        view: &Entity<BrowserTab>,
        event: &BrowserTabEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tab = view.read(cx).tab;
        match event {
            BrowserTabEvent::Clicked => {
                self.select_tab(tab, window, cx);
                self.focus_active(window, cx);
                view.read(cx).page_takes_keyboard();
            }
            BrowserTabEvent::Changed => {
                let (url, title) = {
                    let view = view.read(cx);
                    (view.bar_state().url.clone(), view.title())
                };
                self.browser_titles.insert(tab, title);
                let Some(TabKind::Browser { url: saved }) = self.model.tab(tab).map(|t| &t.kind)
                else {
                    return;
                };
                let now = Some(url).filter(|u| !u.is_empty() && u != "about:blank");
                if now.is_some() && now != *saved {
                    self.model.set_kind(tab, TabKind::Browser { url: now });
                    self.save();
                }
                cx.notify();
            }
        }
    }

    /// Each frame, once its overlays are known: which pages may show (`page_hidden`), and where
    /// the keyboard is. A page whose tab isn't on screen hides now (no frame places it); the
    /// rest are placed, or hidden, by their tab's own drawing.
    pub(crate) fn update_pages(&mut self, window: &Window, cx: &mut Context<Self>) {
        self.close_browser_pages(cx);
        let overlay_open = self.overlays(cx).any();
        let dragging = self.dragging.is_some()
            || self.sidebar.resizing
            || self.slide.is_some()
            || cx.has_active_drag();
        let on_screen: Vec<TabId> = self
            .shown_layout()
            .map(|tree| {
                tree.leaves()
                    .into_iter()
                    .filter_map(|p| self.model.pane(p).map(|p| p.active))
                    .collect()
            })
            .unwrap_or_default();
        for (tab, view) in &self.browser_tabs {
            let view = view.read(cx);
            let tab_shown = on_screen.contains(tab);
            view.showing.set(Showing {
                tab_shown,
                overlay_open,
                dragging,
            });
            if !tab_shown {
                view.hide_page();
            }
            view.sync_keyboard(window);
        }
        // The browser tab now focused is the most recently used.
        let focused = self
            .model
            .focused_tab()
            .filter(|t| self.browser_tabs.contains_key(t));
        if focused != self.focused_browser {
            self.focused_browser = focused;
            if let Some(tab) = focused {
                self.use_browser_tab(tab, cx);
            }
        }
    }

    /// The pages of browser tabs just closed leave the window (whoever still holds their views),
    /// and saved data is deleted if the last page that kept it was one of them.
    pub(crate) fn close_browser_pages(&mut self, cx: &mut Context<Self>) {
        if self.closed_browser_tabs.is_empty() {
            return;
        }
        for view in std::mem::take(&mut self.closed_browser_tabs) {
            view.update(cx, |view, _| view.close_page());
        }
        self.wipe_saved_data_if_due(cx);
    }

    /// Tab `tab` was just used (shown, or an agent's command): an agent's open picks it first.
    fn use_browser_tab(&mut self, tab: TabId, cx: &App) {
        if let Some(view) = self.browser_tabs.get(&tab) {
            self.browser_clock += 1;
            view.read(cx).last_used.set(self.browser_clock);
        }
    }

    /// When tab `tab` was last used (0: never).
    fn browser_used(&self, tab: TabId, cx: &App) -> u64 {
        self.browser_tabs
            .get(&tab)
            .map_or(0, |v| v.read(cx).last_used.get())
    }

    /// A bar's "new browser tab": a blank browser tab at the end of `pane`'s, its address field
    /// ready for typing.
    pub(crate) fn new_browser_tab(
        &mut self,
        pane: PaneId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(opened) = self.model.add_tab(pane, TabKind::Browser { url: None }) else {
            return;
        };
        self.sync();
        let view = self.make_browser_tab(opened.tab, None, window, cx);
        self.save();
        self.activate(window, cx);
        view.update(cx, |view, cx| view.start_editing(window, cx));
    }

    /// Put a browser in workspace `ws` for something in `pane` (`browser_place`), going to
    /// `url`: its browser tab, or a new one split right of `pane`. `by_person`: the tab is shown
    /// and focused, and its page takes the keyboard; otherwise (an agent's) the person's focus
    /// stays exactly where it was. Its tab.
    fn browser_in(
        &mut self,
        ws: WorkspaceId,
        pane: PaneId,
        url: &str,
        by_person: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<TabId> {
        let place = browser_place(&self.model, ws, pane, |t| self.browser_used(t, cx));
        let tab = match place {
            BrowserPlace::Reuse(tab) => {
                let view = self.browser_tabs.get(&tab)?.clone();
                view.update(cx, |view, cx| view.load(url, by_person, cx));
                tab
            }
            BrowserPlace::Split(pane) => {
                let before = self.model.focused_tab();
                let ws_focus = self.model.workspace(ws).map(|w| w.focused);
                let kind = TabKind::Browser {
                    url: Some(url.to_string()),
                };
                let opened = self.model.split(pane, Axis::Row, kind)?;
                if !by_person {
                    // The agent's workspace keeps its focused pane, and the person theirs.
                    if let Some(p) = ws_focus {
                        self.model.focus_pane(p);
                    }
                    if let Some(before) = before {
                        self.model.select_tab(before);
                    }
                }
                self.sync();
                let view = self.make_browser_tab(opened.tab, None, window, cx);
                view.update(cx, |view, cx| view.load(url, by_person, cx));
                self.save();
                opened.tab
            }
        };
        self.use_browser_tab(tab, cx);
        if by_person {
            self.select_tab(tab, window, cx);
        }
        cx.notify();
        Some(tab)
    }

    /// The settings changed: the popped-out windows' top bars follow the theme and
    /// `browser-address-bar` at once, and saved data is deleted if `browser-keep-data` just went
    /// off (`wipe_saved_data_if_due`); pages already open keep the store they were made with.
    pub(crate) fn browser_settings_changed(&mut self, cx: &mut Context<Self>) {
        let appearance = cx.global::<Appearance>();
        let look = bar_look(&appearance.theme, appearance.browser_address_bar);
        for browser in self.browsers.iter() {
            browser.set_look(&look);
        }
        self.wipe_saved_data_if_due(cx);
    }

    /// How many open pages (browser tabs and popped-out windows) were made with
    /// `browser-keep-data` on.
    fn pages_keeping_data(&self, cx: &App) -> usize {
        let windows = self.browsers.iter().filter(|b| b.keeps_data()).count();
        let tabs = self
            .browser_tabs
            .values()
            .filter(|v| v.read(cx).keeps_data())
            .count();
        windows + tabs
    }

    /// Delete the saved browser data, and the visited pages, when `should_wipe` says so (at
    /// launch, when the setting goes off, when the last page made with it on closes). The flag
    /// in the sessions file is cleared only once WebKit says the deletion is done, so a quit
    /// meanwhile retries it at the next launch.
    pub(crate) fn wipe_saved_data_if_due(&mut self, cx: &mut Context<Self>) {
        let keep_data = cx.global::<Appearance>().browser_keep_data;
        let on_pages = self.pages_keeping_data(cx);
        if self.browser_wiping || !should_wipe(keep_data, self.browser_data_kept, on_pages) {
            return;
        }
        self.browser_wiping = true;
        BrowserHistory::forget(cx);
        let done = forget_saved_data(
            MainThreadMarker::new().expect("GPUI's App lives on the main thread"),
        );
        cx.spawn(async move |this, cx| {
            let finished = done.await.is_ok();
            this.update(cx, |this, cx| {
                this.browser_wiping = false;
                // A page made with it on since then writes there again: the flag stays.
                if finished && this.pages_keeping_data(cx) == 0 {
                    this.browser_data_kept = false;
                    this.save();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Bring the popped-out windows' titles and top bars up to date and drop those the user
    /// closed; if that leaves nothing (`quits_after_closing`), fuxx's window closes and fuxx
    /// quits. The wait before the next check (see `tick_interval`).
    fn check_browsers(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Duration {
        let closed = self.browsers.forget_closed();
        if quits_after_closing(
            closed,
            self.sessions.items().len(),
            self.model.workspaces().len(),
            self.browsers.iter().count(),
        ) {
            let conns = self.subscribers.clone();
            self.quit_after_flush(conns, None, window, cx);
        }
        self.browsers
            .retitle(|browser| popped_title(&browser.state().title));
        let mut seen = vec![];
        for browser in self.browsers.iter() {
            browser.refresh_bar();
            seen.push((browser.state().loading, browser.can_be_seen()));
        }
        // The last window made with the setting on may just have closed.
        self.wipe_saved_data_if_due(cx);
        tick_interval(seen)
    }

    /// Start the windows' check (every 500 ms, 100 ms while a page loads) unless it's running;
    /// it stops once no window is open.
    fn tick_browsers(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.browsers_ticking || self.browsers.is_empty() {
            return;
        }
        self.browsers_ticking = true;
        cx.spawn_in(window, async move |this, cx| {
            let mut wait = BROWSER_TICK_LOADING;
            loop {
                cx.background_executor().timer(wait).await;
                let next = this
                    .update_in(cx, |this, window, cx| {
                        let next = this.check_browsers(window, cx);
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

    /// A browser tab's menu → Pop Out: its page (with its history, and whatever it is doing)
    /// moves into a browser window of its own, in front, and the tab closes. A tab with no page
    /// yet just closes.
    pub(crate) fn pop_out(&mut self, tab: TabId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(view) = self.browser_tabs.get(&tab).cloned() else {
            return;
        };
        let had_page = view.read(cx).page().is_some();
        let (page, title) = view.update(cx, |view, _| (view.take_page(), view.title()));
        if had_page && page.is_none() {
            // Something still holds the page (a frame being drawn): the tab stays, page and all.
            eprintln!("[fuxx] browser: the page couldn't be popped out; the tab stays");
            return;
        }
        if let Some(page) = page {
            let appearance = cx.global::<Appearance>();
            let look = bar_look(&appearance.theme, appearance.browser_address_bar);
            let mtm = MainThreadMarker::new().expect("GPUI's App lives on the main thread");
            let title = popped_title(&title);
            let browser = Browser::adopt(page, &title, &look, mtm);
            browser.bring_to_front(true);
            let id = self.next_popped;
            self.next_popped += 1;
            self.browsers.insert(id, browser, title);
            self.tick_browsers(window, cx);
        }
        self.close_tab(tab, window, cx);
    }

    /// File → Open Browser… (⇧⌘B): the address box, holding the focused workspace's browser
    /// tab's address (selected), or empty. Already open: back to its box.
    pub(crate) fn open_address_box(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(address) = &self.address_box {
            address.focus_handle(cx).focus(window, cx);
            return;
        }
        let (Some(ws), Some(pane)) = (self.model.focused_workspace(), self.model.focused_pane())
        else {
            return;
        };
        let current = match browser_place(&self.model, ws, pane, |t| self.browser_used(t, cx)) {
            BrowserPlace::Reuse(tab) => self
                .browser_tabs
                .get(&tab)
                .map(|v| v.read(cx).bar_state().url.clone())
                .filter(|url| url != "about:blank")
                .unwrap_or_default(),
            BrowserPlace::Split(_) => String::new(),
        };
        self.close_overlays(Overlay::AddressBox);
        let address = cx.new(|cx| AddressBox::new(current, window, cx));
        cx.subscribe_in(&address, window, Self::on_address_event)
            .detach();
        address.focus_handle(cx).focus(window, cx);
        self.address_box = Some(address);
        cx.notify();
    }

    /// Enter in the address box opens the address (or searches) in the focused workspace's
    /// browser tab, or a new one split right of the focused pane, and the box goes; a refused
    /// address stays, with why. Closed: the typing returns to the focused tab.
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
        self.address_box = None;
        cx.notify();
        let AddressBoxEvent::Open(text) = event else {
            self.focus_active(window, cx);
            return;
        };
        let url = match enter_target(text) {
            Ok(url) => url,
            Err(why) => {
                self.address_box = Some(address.clone());
                address.update(cx, |address, cx| address.show_error(why, cx));
                return;
            }
        };
        let place = self
            .model
            .focused_workspace()
            .zip(self.model.focused_pane());
        let opened = place.and_then(|(ws, pane)| self.browser_in(ws, pane, &url, true, window, cx));
        if opened.is_none() {
            eprintln!("[fuxx] browser: no browser tab could be opened for {url}");
            self.focus_active(window, cx);
        }
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
    /// A `browser.*` request for the target session (by default the caller's), on its
    /// workspace's browser tab (`agent_place`). Bad parameters and a workspace with no browser
    /// tab are answered now, as are `url` and `close`; the rest (`None`) reply to `conn` from a
    /// GPUI task once the page has loaded or answered.
    pub(crate) fn browser_request(
        &mut self,
        request: &Request,
        conn: &Conn,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Option<Value>, ApiError> {
        let p = &request.params;
        let command = BrowserCommand::parse(&request.method, p)?;
        let index = self.target_index(p, "target")?;
        let session = self.sessions.items()[index].id;
        let place = agent_place(&self.model, session, |t| self.browser_used(t, cx));
        let tab = match (&command, place) {
            (BrowserCommand::Open { url }, Some(_)) => {
                let url = normalize_url(url).map_err(ApiError::bad_request)?;
                let (ws, pane) = self
                    .model
                    .terminal_tab(session)
                    .and_then(|t| self.model.locate(t))
                    .ok_or_else(|| no_browser(session))?;
                self.browser_in(ws, pane, &url, false, window, cx)
                    .ok_or_else(|| ApiError::refused("the browser tab couldn't be opened"))?
            }
            (_, Some(BrowserPlace::Reuse(tab))) => {
                self.use_browser_tab(tab, cx);
                tab
            }
            _ => return Err(no_browser(session)),
        };
        match &command {
            BrowserCommand::Url => {
                let state = self
                    .tab_page(tab, cx)
                    .map(Page::state)
                    .ok_or_else(|| ApiError::refused(CLOSED))?;
                return Ok(Some(json!({
                    "url": state.url,
                    "title": state.title,
                    "loading": state.loading,
                })));
            }
            BrowserCommand::Close => {
                // An agent's: the person's focus stays where it is.
                self.close_browser_tab(tab, false, window, cx);
                return Ok(Some(json!({})));
            }
            _ => {}
        }
        let (conn, id) = (conn.clone(), request.id);
        cx.spawn(async move |this, cx| {
            let page = TabPage {
                workspace: this,
                tab,
                session,
                executor: cx.background_executor().clone(),
            };
            let result = page.run(command, cx).await;
            conn.send_line(&api::reply(id, result));
        })
        .detach();
        Ok(None)
    }

    /// Browser tab `tab`'s page, if it has one.
    fn tab_page<'a>(&'a self, tab: TabId, cx: &'a App) -> Option<&'a Page> {
        self.browser_tabs.get(&tab)?.read(cx).page()
    }
}

/// An agent's browser tab, as a command running in a GPUI task reaches it: one gone meanwhile
/// (its tab closed or popped out) answers `browser closed`.
struct TabPage {
    workspace: WeakEntity<Workspace>,
    tab: TabId,
    /// The agent's terminal (screenshots are named after it).
    session: u64,
    executor: BackgroundExecutor,
}

impl TabPage {
    /// `command`'s reply (`url` and `close` are answered before any task).
    async fn run(&self, command: BrowserCommand, cx: &mut AsyncApp) -> Result<Value, ApiError> {
        match command {
            // `browser_request` has started the load.
            BrowserCommand::Open { .. } => self.loaded(cx).await,
            BrowserCommand::Back => {
                self.with(cx, Page::back)?;
                self.loaded(cx).await
            }
            BrowserCommand::Reload => {
                self.with(cx, Page::reload)?;
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
                let answer = self.with(cx, |p| p.eval(&js))?;
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

    /// `f` of the tab's page, if it is still there.
    fn with<R>(&self, cx: &AsyncApp, f: impl FnOnce(&Page) -> R) -> Result<R, ApiError> {
        self.workspace
            .read_with(cx, |this, cx| this.tab_page(self.tab, cx).map(f))
            .ok()
            .flatten()
            .ok_or_else(|| ApiError::refused(CLOSED))
    }

    /// One of fuxx's own page scripts' value (run once, never retried), within the script limit.
    async fn script(&self, cx: &AsyncApp, script: &str) -> Result<Value, ApiError> {
        let answer = self.with(cx, |p| p.run_script(script))?;
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
            if let Some(reply) = settled(&self.with(cx, Page::state)?) {
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
        let answer = self.with(cx, Page::screenshot)?;
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
    fn a_popped_out_window_is_titled_by_its_page() {
        assert_eq!(popped_title("Docs"), "Docs");
        assert_eq!(popped_title("  "), "Browser");
        assert_eq!(popped_title(""), "Browser");
    }

    #[test]
    fn a_user_closed_window_is_forgotten() {
        let mut browsers = Browsers::new();
        let (one, one_dropped) = Fake::new();
        let (two, two_dropped) = Fake::new();
        browsers.insert(1, one, "one".into());
        browsers.insert(2, two, "two".into());
        assert_eq!(browsers.forget_closed(), 0);
        browsers.get(1).unwrap().closed.set(true);
        assert_eq!(browsers.forget_closed(), 1);
        assert!(one_dropped.get() && browsers.get(1).is_none());
        assert!(!two_dropped.get() && browsers.get(2).is_some());
    }

    #[test]
    fn a_window_is_retitled_only_when_its_title_changes() {
        let mut browsers = Browsers::new();
        let (one, _) = Fake::new();
        browsers.insert(1, one, "Docs".into());
        browsers.retitle(|_| "Docs".into());
        assert_eq!(
            *browsers.get(1).unwrap().title.borrow(),
            "",
            "unchanged: not set"
        );
        browsers.retitle(|_| "Docs \u{2014} API".into());
        assert_eq!(
            *browsers.get(1).unwrap().title.borrow(),
            "Docs \u{2014} API"
        );
    }

    #[test]
    fn a_new_browser_under_a_number_replaces_the_old_one() {
        let mut browsers = Browsers::new();
        let (old, old_dropped) = Fake::new();
        let (new, _) = Fake::new();
        browsers.insert(1, old, "old".into());
        browsers.insert(1, new, "new".into());
        assert!(old_dropped.get());
        assert!(browsers.get(1).is_some());
    }

    /// Workspace one: terminal s1 alone in its pane; workspace two: s2.
    fn two_workspaces() -> (Model, PaneId, PaneId) {
        let mut m = Model::new();
        let one = m.new_workspace(0, TabKind::Terminal(1)).unwrap();
        let two = m.new_workspace(1, TabKind::Terminal(2)).unwrap();
        (m, one.pane, two.pane)
    }

    fn browser() -> TabKind {
        TabKind::Browser { url: None }
    }

    #[test]
    fn an_agent_s_browser_is_its_workspace_s_browser_tab_else_a_split_of_its_pane() {
        let (mut m, pane, other) = two_workspaces();
        let never = |_| 0;
        // No browser tab yet: split the agent's pane to the right.
        assert_eq!(agent_place(&m, 1, never), Some(BrowserPlace::Split(pane)));
        // Another workspace's browser tab isn't the agent's.
        let theirs = m.add_tab(other, browser()).unwrap().tab;
        assert_eq!(agent_place(&m, 1, never), Some(BrowserPlace::Split(pane)));
        assert_eq!(agent_place(&m, 2, never), Some(BrowserPlace::Reuse(theirs)));
        // A browser tab anywhere in the agent's workspace (here in a pane split beside it).
        let first = m.split(pane, Axis::Row, browser()).unwrap();
        assert_eq!(
            agent_place(&m, 1, never),
            Some(BrowserPlace::Reuse(first.tab))
        );
        // Two of them: the most recently used; a tie goes to the first in reading order.
        let second = m.add_tab(pane, browser()).unwrap().tab;
        assert_eq!(
            agent_place(&m, 1, never),
            Some(BrowserPlace::Reuse(second)),
            "the agent's own pane reads first"
        );
        let used = |t: TabId| if t == first.tab { 5 } else { 2 };
        assert_eq!(
            agent_place(&m, 1, used),
            Some(BrowserPlace::Reuse(first.tab))
        );
        // A terminal no tab shows: nowhere.
        assert_eq!(agent_place(&m, 9, never), None);
        // The person's (⇧⌘B): the focused workspace's, else a split of the focused pane.
        let (m, pane, _) = two_workspaces();
        let ws = m.workspaces()[0].id;
        assert_eq!(
            browser_place(&m, ws, pane, never),
            BrowserPlace::Split(pane)
        );
    }

    #[test]
    fn a_command_without_a_browser_tab_errors_as_before() {
        let e = no_browser(3);
        assert_eq!(
            (e.code, e.message.as_str()),
            (
                "not_found",
                "no browser for s3 (fuxx browser open URL first)"
            )
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
    fn fuxx_stays_while_a_terminal_a_workspace_or_a_popped_out_window_is_left() {
        assert!(nothing_left(0, 0, 0));
        assert!(!nothing_left(1, 1, 0));
        // Browser tabs only.
        assert!(!nothing_left(0, 1, 0));
        // The last tab popped out: its window keeps fuxx running.
        assert!(!nothing_left(0, 0, 1));
        assert!(!nothing_left(0, 0, 2));
    }

    #[test]
    fn closing_the_last_popped_out_window_with_nothing_else_left_quits() {
        // The last tab popped out, then its window closed.
        assert!(quits_after_closing(1, 0, 0, 0));
        assert!(quits_after_closing(2, 0, 0, 0));
        // Another popped-out window is still open.
        assert!(!quits_after_closing(1, 0, 0, 1));
        // A workspace (browser tabs only) or a terminal is left.
        assert!(!quits_after_closing(1, 0, 1, 0));
        assert!(!quits_after_closing(1, 1, 1, 0));
        // No window closed: the empty first-run window stays.
        assert!(!quits_after_closing(0, 0, 0, 0));
    }

    #[test]
    fn saved_data_is_deleted_only_when_off_flagged_and_no_on_page_is_open() {
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
