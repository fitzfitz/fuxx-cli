//! A browser tab (M11b): a GPUI bar over its page — back, forward, reload / stop and a rounded
//! address field (with "Show address bar" off, the page's title in its place) — and the page
//! itself, a native web view placed over the rest of the tab every frame (`pane_browser`).
//!
//! Typing in the field opens suggestions under it: visited pages (`browser_history`, kept only
//! with "Keep history and data" on), then "Search Google for “…”". They are one of fuxx's
//! overlays: the page hides while they are open. Enter opens the chosen row, else what was typed
//! (an address by the http/https rule, or a search), and the page takes the keyboard.
//!
//! `browser_tab_title` and `BarState` are the pure part; the rest draws and acts.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

use futures::StreamExt;
use futures::channel::mpsc::{self, UnboundedSender};
use gpui::{
    AnyElement, AnyView, App, Bounds, ClickEvent, Context, Entity, EventEmitter, FocusHandle,
    Focusable, MouseButton, MouseDownEvent, Pixels, SharedString, Subscription, Task, Window,
    actions, anchored, canvas, deferred, div, point, prelude::*, px,
};
use objc2::MainThreadMarker;

use crate::appearance::Appearance;
use crate::browser::{Page, PageEvent, follow_system_appearance};
use crate::browser_history::{self, BrowserHistory, Visit, enter_target, search_label};
use crate::layout_model::TabId;
use crate::name_input::{NameInput, NameInputEvent};
use crate::pane_browser::{NativeWindow, PaneWebView, Showing, page_hidden};
use crate::sidebar_toggle::Tip;
use crate::tab_bar::{BROWSER_TITLE, Glyph, glyph};
use crate::terminal_view::hsla;
use crate::theme::UiColors;
use crate::ui_tokens::text;

// Bound in main.rs: ⌘L in a browser tab; ↑ / ↓ in its address field.
actions!(browser_tab, [FocusAddress, SuggestPrev, SuggestNext]);

/// The bar's height, its buttons, the field's height and its rounding.
pub const BAR: f32 = 34.0;
const BUTTON: f32 = 24.0;
const BUTTON_GAP: f32 = 2.0;
const FIELD: f32 = 24.0;
const FIELD_RADIUS: f32 = 7.0;
const BAR_PAD: f32 = 6.0;
/// A suggestion row, the widest its title gets (its address takes the rest), and the
/// popover's padding.
const ROW: f32 = 30.0;
const TITLE_MAX: f32 = 320.0;
const POPOVER_PAD: f32 = 4.0;
const PLACEHOLDER: &str = "Search or enter an address";

/// A browser tab's title: the page's, else its address's host, else "New tab".
pub fn browser_tab_title(url: &str, title: &str) -> String {
    let title = title.trim();
    if !title.is_empty() {
        return title.to_string();
    }
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if host.is_empty() || url.starts_with("about:") {
        BROWSER_TITLE.to_string()
    } else {
        host.to_string()
    }
}

/// What the bar shows of the page.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BarState {
    /// The page's address (or, before a page comes, the one asked for).
    pub url: String,
    pub title: String,
    pub loading: bool,
    pub can_back: bool,
    pub can_forward: bool,
}

pub enum BrowserTabEvent {
    /// The person clicked in the page: its tab takes the focus, and the page the keyboard.
    Clicked,
    /// The page's address or title changed (the tab's title, its saved address, the visited
    /// list), or it started or stopped loading.
    Changed,
}

/// A row under the address field.
#[derive(Clone, Debug, PartialEq)]
pub enum Suggestion {
    Visited(Visit),
    /// The last row: search for what was typed.
    Search(String),
}

/// The address field while it is being typed in.
struct Editing {
    input: Entity<NameInput>,
    /// Changed since the field opened: suggestions show only then.
    typed: bool,
    /// The row chosen with ↑ / ↓; none: Enter opens what is typed.
    chosen: Option<usize>,
    /// Why the last Enter couldn't open what was typed.
    error: Option<String>,
    _events: Subscription,
    _changes: Subscription,
}

pub struct BrowserTab {
    pub tab: TabId,
    focus: FocusHandle,
    /// The page; none until fuxx's window is known (and never in the UI screenshots).
    web: Option<Rc<PaneWebView>>,
    /// To load once the page exists.
    pending: Option<String>,
    /// Made with "Keep history and data" on: the saved data store, and visits are recorded.
    keep_data: bool,
    shown: BarState,
    editing: Option<Editing>,
    /// Whether the page may show this frame: the workspace sets it each frame.
    pub(crate) showing: Rc<Cell<Showing>>,
    /// Give the page the keyboard as soon as it shows (a navigation the person started).
    wants_keyboard: Rc<Cell<bool>>,
    /// The field's bounds as last drawn: the suggestions open under it.
    field_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// When it was last the focused tab, or an agent's: an agent's open reuses the workspace's
    /// most recent one.
    pub(crate) last_used: Cell<u64>,
    /// When it opened (the ⌘P list's age for a workspace of browser tabs only).
    pub(crate) since: Instant,
    /// The page's address last recorded as a visit.
    recorded: Option<String>,
    events: UnboundedSender<PageEvent>,
    _events: Task<()>,
}

impl EventEmitter<BrowserTabEvent> for BrowserTab {}

impl Focusable for BrowserTab {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl BrowserTab {
    /// A browser tab for `tab` that opens `url` (none: a blank page), its page on the data store
    /// `keep_data` picks.
    pub fn new(tab: TabId, url: Option<String>, keep_data: bool, cx: &mut Context<Self>) -> Self {
        let (events, mut rx) = mpsc::unbounded::<PageEvent>();
        let task = cx.spawn(async move |this, cx| {
            while let Some(event) = rx.next().await {
                if this
                    .update(cx, |this, cx| this.on_page_event(event, cx))
                    .is_err()
                {
                    break;
                }
            }
        });
        let mut this = Self {
            tab,
            focus: cx.focus_handle(),
            web: None,
            shown: BarState {
                url: url.clone().unwrap_or_default(),
                ..BarState::default()
            },
            pending: url,
            keep_data,
            editing: None,
            showing: Rc::new(Cell::new(Showing::default())),
            wants_keyboard: Rc::new(Cell::new(false)),
            field_bounds: Rc::new(Cell::new(None)),
            last_used: Cell::new(0),
            since: Instant::now(),
            recorded: None,
            events,
            _events: task,
        };
        this.ensure_page(cx);
        this
    }

    /// Make the page once fuxx's window is known (at once, or on a later frame), and load what
    /// was asked for meanwhile.
    pub fn ensure_page(&mut self, cx: &mut App) {
        if self.web.is_some() {
            return;
        }
        let Some(native) = cx.try_global::<NativeWindow>() else {
            return;
        };
        let mtm = MainThreadMarker::new().expect("GPUI's App lives on the main thread");
        let web = PaneWebView::new(self.keep_data, mtm);
        web.attach(&native.content, &native.gpui);
        web.route_events_to(&native.gpui);
        web.listen(self.events.clone());
        follow_system_appearance(web.view(), mtm);
        if let Some(url) = self.pending.take() {
            web.page().load(&url);
        }
        self.web = Some(Rc::new(web));
    }

    /// The page, once made.
    pub fn page(&self) -> Option<&Page> {
        self.web.as_deref().map(PaneWebView::page)
    }

    /// Whether it was made with "Keep history and data" on.
    pub fn keeps_data(&self) -> bool {
        self.keep_data
    }

    /// What the bar shows.
    pub fn bar_state(&self) -> &BarState {
        &self.shown
    }

    /// Its title for the tab bar.
    pub fn title(&self) -> String {
        browser_tab_title(&self.shown.url, &self.shown.title)
    }

    /// Show `state` in the bar without a page (the UI screenshots).
    #[cfg(feature = "ui-shots")]
    pub fn show_state(&mut self, state: BarState) {
        self.shown = state;
    }

    /// Where the address field was last drawn (the UI screenshots click it).
    #[cfg(feature = "ui-shots")]
    pub fn field_bounds(&self) -> Option<Bounds<Pixels>> {
        self.field_bounds.get()
    }

    /// Go to `url` (already normalised). `by_person`: the person asked (the page then takes the
    /// keyboard once it shows); an agent's leaves the keyboard where it is.
    pub fn load(&mut self, url: &str, by_person: bool, cx: &mut Context<Self>) {
        match self.page() {
            Some(page) => page.load(url),
            None => self.pending = Some(url.to_string()),
        }
        if by_person {
            self.wants_keyboard.set(true);
        }
        self.refresh(cx);
    }

    /// Back, forward, reload or stop (the bar's buttons); the first three are navigations the
    /// person started.
    fn go(&mut self, action: fn(&Page), navigates: bool, cx: &mut Context<Self>) {
        if let Some(page) = self.page() {
            action(page);
            if navigates {
                self.wants_keyboard.set(true);
            }
        }
        self.refresh(cx);
    }

    /// The bar follows the page. Changed: the workspace hears of it.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        let Some(page) = self.page() else {
            let url = self.pending.clone().unwrap_or_default();
            if self.shown.url != url {
                self.shown.url = url;
                cx.emit(BrowserTabEvent::Changed);
                cx.notify();
            }
            return;
        };
        let state = page.state();
        let (can_back, can_forward) = page.can_go();
        let blank = state.url.is_empty() || state.url == "about:blank";
        let url = match (blank, page.requested()) {
            (true, Some(requested)) => requested,
            _ => state.url.clone(),
        };
        let next = BarState {
            url,
            title: state.title.clone(),
            loading: state.loading,
            can_back,
            can_forward,
        };
        if next != self.shown {
            // A page that has finished: a visit (its title may come later).
            if !next.loading && state.error.is_none() && !blank {
                let keep = self.keep_data && cx.global::<Appearance>().browser_keep_data;
                if self.recorded.as_deref() != Some(state.url.as_str()) {
                    self.recorded = Some(state.url.clone());
                    BrowserHistory::record(&state.url, &state.title, keep, cx);
                } else if next.title != self.shown.title {
                    BrowserHistory::retitle(&state.url, &state.title, keep, cx);
                }
            }
            self.shown = next;
            cx.emit(BrowserTabEvent::Changed);
            cx.notify();
        }
    }

    fn on_page_event(&mut self, event: PageEvent, cx: &mut Context<Self>) {
        match event {
            PageEvent::Clicked => cx.emit(BrowserTabEvent::Clicked),
            PageEvent::Changed => self.refresh(cx),
        }
    }

    /// The person clicked in the page and its tab has the focus now: the page keeps the
    /// keyboard AppKit gave it (or takes it).
    pub fn page_takes_keyboard(&self) {
        if let Some(web) = &self.web {
            web.settle_click();
            if !web.has_keyboard() {
                web.take_keyboard();
            }
        }
    }

    /// Each frame: the page gives the keyboard back to fuxx once GPUI's focus has left the tab
    /// (a click on a terminal, ⌘1, a box opening), unless a click in it hasn't been caught up
    /// with yet.
    pub fn sync_keyboard(&self, window: &Window) {
        if let Some(web) = &self.web
            && web.has_keyboard()
            && !web.clicked()
            && !self.focus.is_focused(window)
        {
            web.give_keyboard_back();
        }
    }

    /// Hide the page now (its tab isn't on screen this frame).
    pub fn hide_page(&self) {
        if let Some(web) = &self.web {
            web.hide();
        }
    }

    /// Its tab closed: the page leaves the window now and stops.
    pub fn close_page(&mut self) {
        if let Some(web) = self.web.take() {
            web.remove();
        }
    }

    /// The page, out of fuxx's window (Pop Out); none without one (or if a frame still holds
    /// it, which can't happen between frames).
    pub fn take_page(&mut self) -> Option<Page> {
        let web = self.web.take()?;
        match Rc::try_unwrap(web) {
            Ok(web) => Some(web.into_page()),
            Err(web) => {
                self.web = Some(web);
                None
            }
        }
    }

    /// Whether the suggestions are open: the field is being typed in, and has text.
    pub fn suggestions_open(&self, cx: &App) -> bool {
        self.editing
            .as_ref()
            .is_some_and(|e| e.typed && !e.input.read(cx).text().trim().is_empty())
    }

    /// ⌘L, or a click on the field: type an address, the current one selected. Not with the
    /// address bar off.
    pub fn start_editing(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !cx.global::<Appearance>().browser_address_bar {
            return;
        }
        if let Some(editing) = &self.editing {
            editing.input.focus_handle(cx).focus(window, cx);
            return;
        }
        let url = if self.shown.url == "about:blank" {
            String::new()
        } else {
            self.shown.url.clone()
        };
        let input = cx.new(|cx| {
            let mut input = NameInput::new(url, window, cx);
            input.bare();
            input.select_all();
            input
        });
        let events = cx.subscribe_in(&input, window, |this, input, event, window, cx| {
            // A field already closed can still report losing focus.
            if this.editing.as_ref().map(|e| &e.input) != Some(input) {
                return;
            }
            match event {
                NameInputEvent::Confirm(text) => this.enter(text, window, cx),
                // Escape (the field still has the keyboard) returns it to the tab; a click
                // elsewhere leaves it there.
                NameInputEvent::Cancel => {
                    let escape = input.focus_handle(cx).is_focused(window);
                    this.stop_editing(cx);
                    if escape {
                        this.focus.focus(window, cx);
                    }
                }
            }
        });
        let changes = cx.observe(&input, |this, _, cx| {
            if let Some(editing) = &mut this.editing {
                editing.typed = true;
                editing.chosen = None;
                editing.error = None;
                cx.notify();
            }
        });
        input.focus_handle(cx).focus(window, cx);
        self.editing = Some(Editing {
            input,
            typed: false,
            chosen: None,
            error: None,
            _events: events,
            _changes: changes,
        });
        cx.notify();
    }

    fn stop_editing(&mut self, cx: &mut Context<Self>) {
        if self.editing.take().is_some() {
            cx.notify();
        }
    }

    /// Enter in the field: the chosen row, else what was typed; the page takes the keyboard.
    /// What can't be opened stays in the field, with why.
    fn enter(&mut self, typed: &str, window: &mut Window, cx: &mut Context<Self>) {
        let chosen = self
            .editing
            .as_ref()
            .and_then(|e| e.chosen)
            .and_then(|i| self.suggestions(cx).into_iter().nth(i));
        let target = match chosen {
            Some(Suggestion::Visited(v)) => Ok(v.url),
            Some(Suggestion::Search(text)) => Ok(browser_history::search_url(&text)),
            None => enter_target(typed),
        };
        match target {
            Ok(url) => self.open_from_field(&url, window, cx),
            Err(why) => {
                if let Some(editing) = &mut self.editing {
                    editing.error = Some(why);
                    cx.notify();
                }
            }
        }
    }

    /// Open `url` (from the field or a row): the field closes, the tab keeps GPUI's focus and
    /// the page takes the keyboard once it shows again.
    fn open_from_field(&mut self, url: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.stop_editing(cx);
        self.focus.focus(window, cx);
        self.load(url, true, cx);
    }

    /// The rows under the field for what is typed: visited pages, then the search.
    fn suggestions(&self, cx: &mut App) -> Vec<Suggestion> {
        let Some(editing) = &self.editing else {
            return Vec::new();
        };
        let typed = editing.input.read(cx).text().trim().to_string();
        if typed.is_empty() {
            return Vec::new();
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        // Only what "Keep history and data" kept: none while it is off.
        let keep = cx.global::<Appearance>().browser_keep_data;
        let mut rows: Vec<Suggestion> = if keep {
            browser_history::suggest(BrowserHistory::list(cx), &typed, now)
                .into_iter()
                .cloned()
                .map(Suggestion::Visited)
                .collect()
        } else {
            Vec::new()
        };
        rows.push(Suggestion::Search(typed));
        rows
    }

    /// ↑ / ↓ in the field: the previous or next row (from none, ↓ is the first; ↑ above the
    /// first is none again).
    fn choose(&mut self, delta: i32, cx: &mut Context<Self>) {
        let n = self.suggestions(cx).len();
        let Some(editing) = &mut self.editing else {
            return;
        };
        if n == 0 {
            return;
        }
        editing.chosen = match (editing.chosen, delta > 0) {
            (None, true) => Some(0),
            (None, false) => Some(n - 1),
            (Some(i), true) => (i + 1 < n).then_some(i + 1),
            (Some(0), false) => None,
            (Some(i), false) => Some(i - 1),
        };
        cx.notify();
    }

    /// A button of the bar: its icon, dim until hovered, faint when it can't be used.
    fn button(
        &self,
        id: &'static str,
        kind: Glyph,
        tip: &'static str,
        enabled: bool,
        ui: UiColors,
        on_click: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let color = if enabled { ui.text_dim } else { ui.text_faint };
        div()
            .id(id)
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(px(BUTTON))
            .rounded(px(5.0))
            .child(glyph(kind, color))
            .when(enabled, |b| {
                b.cursor_pointer()
                    .hover(move |s| s.bg(hsla(ui.panel_hover)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        on_click(this, window, cx)
                    }))
            })
            .tooltip(move |_, cx| -> AnyView { cx.new(|_| Tip(tip.into())).into() })
            .into_any_element()
    }

    /// The address field: the page's address (or the placeholder) until clicked, then the box
    /// being typed in.
    fn field(&self, ui: UiColors, cx: &mut Context<Self>) -> AnyElement {
        let bounds = self.field_bounds.clone();
        let editing = self.editing.as_ref();
        let error = editing.and_then(|e| e.error.clone());
        let edge = match (editing.is_some(), error.is_some()) {
            (_, true) => ui.ring_waiting,
            (true, false) => ui.focus_ring,
            (false, false) => ui.panel_border,
        };
        let content: AnyElement = match editing {
            Some(editing) => {
                let empty = editing.input.read(cx).text().is_empty();
                div()
                    .relative()
                    .flex()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(editing.input.clone())
                    .when(empty, |d| {
                        d.child(
                            div()
                                .absolute()
                                .left(px(3.0))
                                .text_color(hsla(ui.text_dim))
                                .child(PLACEHOLDER),
                        )
                    })
                    .into_any_element()
            }
            None => {
                let (text, color) = match self.shown.url.as_str() {
                    "" | "about:blank" => (PLACEHOLDER.to_string(), ui.text_dim),
                    url => (url.to_string(), ui.text),
                };
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .text_color(hsla(color))
                    .child(text)
                    .into_any_element()
            }
        };
        div()
            .id("address-field")
            .relative()
            .flex()
            .flex_1()
            .min_w(px(0.0))
            .items_center()
            .h(px(FIELD))
            .px(px(10.0))
            .gap(px(8.0))
            .rounded(px(FIELD_RADIUS))
            .border_1()
            .border_color(hsla(edge))
            .bg(hsla(ui.sidebar_background))
            .text_size(px(text::NAME))
            .key_context("AddressField")
            .when(editing.is_none(), |f| f.cursor_text())
            .on_click(
                cx.listener(|this, _: &ClickEvent, window, cx| this.start_editing(window, cx)),
            )
            .child(content)
            .when_some(error, |f, why| {
                f.child(
                    div()
                        .flex_none()
                        .max_w(px(260.0))
                        .truncate()
                        .text_size(px(text::META))
                        .text_color(hsla(ui.ring_waiting))
                        .child(why),
                )
            })
            .child(
                canvas(move |b, _, _| bounds.set(Some(b)), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            )
            .on_action(cx.listener(|this, _: &SuggestPrev, _, cx| this.choose(-1, cx)))
            .on_action(cx.listener(|this, _: &SuggestNext, _, cx| this.choose(1, cx)))
            .into_any_element()
    }

    /// The bar: the buttons, then the field (or the page's title with the address bar off).
    fn bar(&self, ui: UiColors, cx: &mut Context<Self>) -> AnyElement {
        let address_bar = cx.global::<Appearance>().browser_address_bar;
        let s = self.shown.clone();
        let has_page = self.web.is_some();
        let back = self.button(
            "browser-back",
            Glyph::Back,
            "Back",
            s.can_back,
            ui,
            |this, _, cx| this.go(Page::back, true, cx),
            cx,
        );
        let forward = self.button(
            "browser-forward",
            Glyph::Forward,
            "Forward",
            s.can_forward,
            ui,
            |this, _, cx| this.go(Page::forward, true, cx),
            cx,
        );
        let reload = if s.loading {
            self.button(
                "browser-stop",
                Glyph::Stop,
                "Stop loading this page",
                true,
                ui,
                |this, _, cx| this.go(Page::stop, false, cx),
                cx,
            )
        } else {
            self.button(
                "browser-reload",
                Glyph::Reload,
                "Reload this page",
                has_page && !s.url.is_empty(),
                ui,
                |this, _, cx| this.go(Page::reload, true, cx),
                cx,
            )
        };
        let middle: AnyElement = if address_bar {
            self.field(ui, cx)
        } else {
            div()
                .flex_1()
                .min_w(px(0.0))
                .px(px(6.0))
                .truncate()
                .text_size(px(text::NAME))
                .text_color(hsla(ui.text_dim))
                .child(SharedString::from(self.title()))
                .into_any_element()
        };
        div()
            .flex()
            .flex_none()
            .flex_row()
            .items_center()
            .w_full()
            .h(px(BAR))
            .px(px(BAR_PAD))
            .gap(px(BUTTON_GAP))
            .bg(hsla(ui.panel))
            .border_b_1()
            .border_color(hsla(ui.panel_border))
            .child(back)
            .child(forward)
            .child(reload)
            .child(div().w(px(4.0)).flex_none())
            .child(middle)
            .into_any_element()
    }

    /// The suggestions under the field, over the page (which hides while they show).
    fn popover(&self, ui: UiColors, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.suggestions_open(cx) {
            return None;
        }
        let field = self.field_bounds.get()?;
        let chosen = self.editing.as_ref().and_then(|e| e.chosen);
        let rows = self.suggestions(cx);
        let hover = ui.panel_hover;
        let rows = rows.into_iter().enumerate().map(|(i, row)| {
            let (icon, line, detail, url) = match row {
                Suggestion::Visited(v) => {
                    let title = browser_tab_title(&v.url, &v.title);
                    (Glyph::Globe, title, Some(v.url.clone()), v.url)
                }
                Suggestion::Search(typed) => (
                    Glyph::Search,
                    search_label(&typed),
                    None,
                    browser_history::search_url(&typed),
                ),
            };
            let picked = chosen == Some(i);
            div()
                .id(("browser-suggestion", i))
                .flex()
                .flex_row()
                .items_center()
                .gap(px(8.0))
                .h(px(ROW))
                .px(px(8.0))
                .rounded(px(5.0))
                .cursor_pointer()
                .when(picked, |r| r.bg(hsla(ui.row_active)))
                .when(!picked, |r| r.hover(move |s| s.bg(hsla(hover))))
                .overflow_hidden()
                .child(glyph(icon, ui.text_dim))
                .child(
                    div()
                        .flex_shrink_1()
                        .min_w(px(0.0))
                        .max_w(px(TITLE_MAX))
                        .truncate()
                        .text_color(hsla(ui.text))
                        .child(line),
                )
                .when_some(detail, |r, detail| {
                    r.child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .truncate()
                            .text_size(px(text::META))
                            .text_color(hsla(ui.text_faint))
                            .child(detail),
                    )
                })
                // Its press is its own: the field keeps the keyboard until the row opens.
                .on_mouse_down(MouseButton::Left, |_: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation()
                })
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.open_from_field(&url, window, cx)
                }))
        });
        let panel = div()
            .flex()
            .flex_col()
            .w(field.size.width)
            .p(px(POPOVER_PAD))
            .rounded(px(8.0))
            .bg(hsla(ui.panel))
            .border_1()
            .border_color(hsla(ui.panel_border))
            .shadow(vec![gpui::BoxShadow {
                color: gpui::Hsla {
                    a: 0.3,
                    ..hsla(ui.shadow)
                },
                offset: point(px(0.0), px(6.0)),
                blur_radius: px(18.0),
                spread_radius: px(0.0),
                inset: false,
            }])
            .text_size(px(text::NAME))
            .children(rows);
        let at = point(field.left(), field.bottom() + px(4.0));
        Some(deferred(anchored().position(at).snap_to_window().child(panel)).into_any_element())
    }
}

impl Render for BrowserTab {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.global::<Appearance>().theme;
        let ui = theme.ui();
        let bar = self.bar(ui, cx);
        let popover = self.popover(ui, cx);
        let web = self.web.clone();
        let showing = self.showing.clone();
        let wants = self.wants_keyboard.clone();
        // The page's place: the web view covers it each frame, or hides (`page_hidden`).
        let page = div()
            .relative()
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .bg(hsla(theme.background))
            .child(
                canvas(
                    move |bounds, window, _| {
                        let Some(web) = &web else { return };
                        let hidden = page_hidden(showing.get());
                        web.place(bounds, window.viewport_size().height, hidden);
                        if !hidden && wants.take() {
                            web.take_keyboard();
                        }
                    },
                    |_, _, _, _| {},
                )
                .size_full(),
            );
        div()
            .flex()
            .flex_col()
            .size_full()
            .track_focus(&self.focus)
            .key_context("BrowserTab")
            .on_action(
                cx.listener(|this, _: &FocusAddress, window, cx| this.start_editing(window, cx)),
            )
            .child(bar)
            .child(page)
            .children(popover)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_browser_tab_is_titled_by_its_page_then_its_host() {
        assert_eq!(
            browser_tab_title("https://docs.rs/gpui", "gpui - Rust"),
            "gpui - Rust"
        );
        assert_eq!(
            browser_tab_title("http://localhost:3000/a?b", " "),
            "localhost:3000"
        );
        assert_eq!(browser_tab_title("", ""), BROWSER_TITLE);
        assert_eq!(browser_tab_title("about:blank", ""), BROWSER_TITLE);
    }
}
