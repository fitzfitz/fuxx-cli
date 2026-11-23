//! The pane bar's + menu (chore 2026-11-23) and what ⌘T makes.

use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{
    Anchor, AnyElement, AnyView, App, Bounds, ClickEvent, Context, Entity, EventEmitter,
    FocusHandle, Focusable, MouseButton, MouseDownEvent, MouseMoveEvent, Pixels, Point, Render,
    ScrollHandle, SharedString, Subscription, Window, actions, anchored, canvas, div, point,
    prelude::*, px,
};

use crate::agent_detect::Found;
use crate::appearance::Appearance;
use crate::browser_history::{self, BrowserHistory};
use crate::icons::Icon;
use crate::layout_model::{PaneId, TabId, TabKind};
use crate::name_input::{NameInput, NameInputEvent};
use crate::overlay_panel;
use crate::palette_items::Scope;
use crate::palette_match::fuzzy;
use crate::presets::{MAX_PRESETS, Preset};
use crate::restore::{Launch, SavedSession};
use crate::scrollbar::{self, Axis as ScrollAxis};
use crate::sidebar_toggle::Tip;
use crate::tab_bar::TabIcon;
use crate::terminal_view::hsla;
use crate::theme::UiColors;
use crate::ui_tokens::text;
use crate::workspace::{At, Overlay, Workspace};

/// What ⌘T makes, from the focused pane's active tab (`plan_like`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NewTabPlan {
    /// A terminal, as the bar's new-terminal button makes one (SSH: another session to the host).
    Terminal,
    /// A blank browser tab, its address field ready.
    Browser,
    /// An agent tab of this provider in `cwd`, with this model and effort (its mode the
    /// provider's default).
    Agent {
        provider: &'static str,
        cwd: PathBuf,
        model: Option<String>,
        effort: Option<String>,
    },
}

/// ⌘T "based on the last tab" (your words, 2026-11-23): after a terminal, a terminal; after a
/// browser tab, a browser tab; after an agent tab of a provider fuxx has, in a workspace on this
/// Mac (`local`), the same agent in that tab's folder with its model and effort; after anything
/// else (a file, an agent fuxx doesn't know, no tab), a terminal.
pub fn plan_like(kind: Option<&TabKind>, local: bool) -> NewTabPlan {
    match kind {
        Some(TabKind::Browser { .. }) => NewTabPlan::Browser,
        Some(TabKind::Agent {
            provider,
            cwd,
            model,
            effort,
            ..
        }) if local => match crate::providers::by_name(provider) {
            Some(p) => NewTabPlan::Agent {
                provider: p.info().name,
                cwd: cwd.clone(),
                model: model.clone(),
                effort: effort.clone(),
            },
            None => NewTabPlan::Terminal,
        },
        _ => NewTabPlan::Terminal,
    }
}

/// At most this many tab rows and history rows while typing.
const TAB_RESULTS: usize = 5;
const HISTORY_RESULTS: usize = 5;
/// The slot's words on a preset whose command isn't on your PATH.
pub const NOT_INSTALLED: &str = "Not installed";
/// The agents' PATH answers are asked again on open once they're this old.
pub const RECHECK: Duration = Duration::from_secs(30);

/// The icon row's buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NewKind {
    Terminal,
    Browser,
    File,
}

impl NewKind {
    pub const ALL: [NewKind; 3] = [NewKind::Terminal, NewKind::Browser, NewKind::File];

    /// Its name (the icon row's words, a "New" result's label).
    pub fn name(self) -> &'static str {
        match self {
            NewKind::Terminal => "Terminal",
            NewKind::Browser => "Browser",
            NewKind::File => "File",
        }
    }

    /// Its tooltip (without the ⌘T mark: `MenuModel::tooltip`).
    pub fn tooltip(self) -> &'static str {
        match self {
            NewKind::Terminal => "New terminal tab",
            NewKind::Browser => "New browser tab",
            NewKind::File => "Open a file in a tab…",
        }
    }

    pub fn icon(self) -> Icon {
        match self {
            NewKind::Terminal => Icon::Terminal,
            NewKind::Browser => Icon::Globe,
            NewKind::File => Icon::File,
        }
    }
}

/// One agent preset's row: its place in the presets, its name, the provider that runs it as an
/// agent tab (none: it runs in a terminal), and whether its command is on your PATH.
#[derive(Clone, Debug, PartialEq)]
pub struct AgentRow {
    pub preset: usize,
    pub name: String,
    pub provider: Option<&'static str>,
    pub found: Found,
}

impl AgentRow {
    /// It can be chosen: only "not found" can't (checking and unknown read as installed, so
    /// nothing flickers).
    pub fn enabled(&self) -> bool {
        self.found != Found::No
    }
}

/// A tab as the typed results list it (a snapshot taken at open).
#[derive(Clone, Debug, PartialEq)]
pub struct TabEntry {
    pub tab: TabId,
    pub title: String,
    pub workspace: String,
    pub icon: TabIcon,
}

/// What a typed result row does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Item {
    New(NewKind),
    /// A preset, by its place.
    Agent(usize),
    /// Open this address (a typed one, a web search, a history page) in a new browser tab.
    Url(String),
    Tab(TabId),
    /// ⌘K on Files with the query.
    Files,
}

/// A typed row's icon: one of the set, a preset's logo or badge, or a tab's icon.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RowIcon {
    Icon(Icon),
    Agent(usize),
    Tab(TabIcon),
}

/// A typed result row.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub item: Item,
    pub icon: RowIcon,
    pub label: String,
    /// The matched chars of `label`.
    pub positions: Vec<usize>,
    pub meta: Option<String>,
    pub enabled: bool,
}

/// A line of the typed results: a heading or a row.
#[derive(Clone, Debug, PartialEq)]
pub enum Line {
    Heading(&'static str),
    Row(Row),
}

/// What the typed results are made from.
#[derive(Clone, Copy)]
pub struct Sources<'a> {
    pub agents: &'a [AgentRow],
    pub tabs: &'a [TabEntry],
    /// The history's suggestions for the query (address, title), best first.
    pub history: &'a [(String, String)],
    /// The workspace is on this Mac: its files can be searched.
    pub files: bool,
}

/// The rows for `query` (not blank), under their headings, in this order: Open URL (the address,
/// or a web search for the words), Agents, New, Tabs (at most 5), History (at most 5), Files.
pub fn results(query: &str, s: &Sources) -> Vec<Line> {
    let q = query.trim();
    let mut lines = Vec::new();
    let mut section = |heading: &'static str, rows: Vec<Row>| {
        if !rows.is_empty() {
            lines.push(Line::Heading(heading));
            lines.extend(rows.into_iter().map(Line::Row));
        }
    };
    let url = browser_history::enter_target(q).ok().map(|url| {
        let label = if browser_history::looks_like_address(q) {
            format!("Open {q}")
        } else {
            browser_history::search_label(q)
        };
        plain(Item::Url(url), Icon::Globe, label)
    });
    section("Open URL", url.into_iter().collect());
    let agents = best(q, s.agents, |a| &a.name, usize::MAX)
        .into_iter()
        .map(|(a, positions)| Row {
            item: Item::Agent(a.preset),
            icon: RowIcon::Agent(a.preset),
            label: a.name.clone(),
            positions,
            meta: (!a.enabled()).then(|| NOT_INSTALLED.to_string()),
            enabled: a.enabled(),
        })
        .collect();
    section("Agents", agents);
    let new = [NewKind::Terminal, NewKind::Browser]
        .into_iter()
        .filter_map(|k| {
            let m = fuzzy(q, k.name())?;
            Some(Row {
                positions: m.positions,
                ..plain(Item::New(k), k.icon(), k.name().into())
            })
        })
        .collect();
    section("New", new);
    let tabs = best(q, s.tabs, |t| &t.title, TAB_RESULTS)
        .into_iter()
        .map(|(t, positions)| Row {
            item: Item::Tab(t.tab),
            icon: RowIcon::Tab(t.icon),
            label: t.title.clone(),
            positions,
            meta: Some(t.workspace.clone()),
            enabled: true,
        })
        .collect();
    section("Tabs", tabs);
    let history = s
        .history
        .iter()
        .take(HISTORY_RESULTS)
        .map(|(url, title)| {
            let label = if title.trim().is_empty() { url } else { title };
            Row {
                positions: fuzzy(q, label).map(|m| m.positions).unwrap_or_default(),
                meta: Some(url.clone()),
                ..plain(Item::Url(url.clone()), Icon::Globe, label.clone())
            }
        })
        .collect();
    section("History", history);
    let files = s.files.then(|| {
        plain(
            Item::Files,
            Icon::Search,
            format!("Search files for \u{201c}{q}\u{201d}"),
        )
    });
    section("Files", files.into_iter().collect());
    lines
}

/// A row with no matched letters and no meta.
fn plain(item: Item, icon: Icon, label: String) -> Row {
    Row {
        item,
        icon: RowIcon::Icon(icon),
        label,
        positions: Vec::new(),
        meta: None,
        enabled: true,
    }
}

/// The items whose `text` matches `query`, best first (ties keep their order), at most `max`,
/// each with its matched chars.
fn best<'a, T>(
    query: &str,
    items: &'a [T],
    text: impl Fn(&T) -> &String,
    max: usize,
) -> Vec<(&'a T, Vec<usize>)> {
    let mut hits: Vec<(i32, &'a T, Vec<usize>)> = items
        .iter()
        .filter_map(|item| {
            let m = fuzzy(query, text(item))?;
            Some((m.score, item, m.positions))
        })
        .collect();
    hits.sort_by_key(|(score, _, _)| std::cmp::Reverse(*score));
    hits.into_iter().take(max).map(|(_, i, p)| (i, p)).collect()
}

/// What is lit: an icon row button, an agent row (by its preset), or a typed line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Lit {
    Icon(NewKind),
    Agent(usize),
    Line(usize),
}

/// What a pick does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Choice {
    /// What ⌘T does (`Workspace::new_tab_like`).
    Like,
    New(NewKind),
    /// A preset, by its place.
    Agent(usize),
    Url(String),
    Tab(TabId),
    /// ⌘K on Files with this query.
    Files(String),
}

/// The menu as a model: its agent rows, what ⌘T would make among them (`like`), the query and
/// its lines, and the one thing lit (the pointer and the keys light the same; one at a time).
#[derive(Clone, Debug)]
pub struct MenuModel {
    pub agents: Vec<AgentRow>,
    pub like: Option<Lit>,
    pub query: String,
    pub lines: Vec<Line>,
    pub lit: Option<Lit>,
    /// The icon row's button ↑↓ come back to.
    icon_last: NewKind,
}

impl MenuModel {
    /// Opened for `plan` (what ⌘T would make): lit on its button or its agent row.
    pub fn new(agents: Vec<AgentRow>, plan: &NewTabPlan) -> Self {
        let mut m = Self {
            agents,
            like: None,
            query: String::new(),
            lines: Vec::new(),
            lit: None,
            icon_last: NewKind::Terminal,
        };
        m.like = m.like_for(plan);
        m.lit = m.like.clone();
        if let Some(Lit::Icon(k)) = m.lit {
            m.icon_last = k;
        }
        m
    }

    /// ⌘T's button or agent row for `plan` (none: an agent with no row that can be chosen).
    fn like_for(&self, plan: &NewTabPlan) -> Option<Lit> {
        match plan {
            NewTabPlan::Terminal => Some(Lit::Icon(NewKind::Terminal)),
            NewTabPlan::Browser => Some(Lit::Icon(NewKind::Browser)),
            NewTabPlan::Agent { provider, .. } => self
                .agents
                .iter()
                .find(|a| a.provider == Some(*provider) && a.enabled())
                .map(|a| Lit::Agent(a.preset)),
        }
    }

    fn agent(&self, preset: usize) -> Option<&AgentRow> {
        self.agents.iter().find(|a| a.preset == preset)
    }

    /// Whether `lit` can be lit and chosen.
    fn can_light(&self, lit: &Lit) -> bool {
        match lit {
            Lit::Icon(_) => self.query.is_empty(),
            Lit::Agent(p) => self.query.is_empty() && self.agent(*p).is_some_and(AgentRow::enabled),
            Lit::Line(i) => matches!(self.lines.get(*i), Some(Line::Row(r)) if r.enabled),
        }
    }

    /// The PATH answers came (one per agent row, in order; answers for another list are
    /// dropped): a row lit that can't be chosen now gives the light back to ⌘T's.
    pub fn set_found(&mut self, found: &[Found]) {
        if found.len() != self.agents.len() {
            return;
        }
        for (a, f) in self.agents.iter_mut().zip(found) {
            a.found = *f;
        }
        if let Some(Lit::Agent(p)) = &self.like
            && !self.agent(*p).is_some_and(AgentRow::enabled)
        {
            self.like = None;
        }
        if self.lit.as_ref().is_some_and(|l| !self.can_light(l)) {
            self.lit = if self.query.is_empty() {
                self.like.clone()
            } else {
                self.first_row()
            };
        }
    }

    /// The query changed: its `lines` (none for a blank query, which shows the icon row and
    /// the agents again, lit on ⌘T's); typed, the first row that can be chosen is lit.
    pub fn set_results(&mut self, query: &str, lines: Vec<Line>) {
        self.query = if query.trim().is_empty() {
            String::new()
        } else {
            query.to_string()
        };
        self.lines = lines;
        self.lit = if self.query.is_empty() {
            self.like.clone()
        } else {
            self.first_row()
        };
    }

    fn first_row(&self) -> Option<Lit> {
        self.stops().into_iter().next()
    }

    /// What ↑↓ step over, in order: blank, the icon row (one stop, on its last button) and the
    /// agents that can be chosen; typed, the rows that can be chosen.
    fn stops(&self) -> Vec<Lit> {
        if self.query.is_empty() {
            let icon = match self.lit {
                Some(Lit::Icon(k)) => k,
                _ => self.icon_last,
            };
            std::iter::once(Lit::Icon(icon))
                .chain(
                    self.agents
                        .iter()
                        .filter(|a| a.enabled())
                        .map(|a| Lit::Agent(a.preset)),
                )
                .collect()
        } else {
            (0..self.lines.len())
                .map(Lit::Line)
                .filter(|l| self.can_light(l))
                .collect()
        }
    }

    /// ↑ (`delta` -1) or ↓ (1): the previous or next stop, wrapping.
    pub fn step(&mut self, delta: i32) {
        let stops = self.stops();
        if stops.is_empty() {
            return;
        }
        let at = match self
            .lit
            .as_ref()
            .and_then(|l| stops.iter().position(|s| s == l))
        {
            Some(i) => crate::host_picker::move_selection(i, stops.len(), delta),
            None if delta > 0 => 0,
            None => stops.len() - 1,
        };
        if let Some(Lit::Icon(k)) = self.lit {
            self.icon_last = k;
        }
        self.lit = Some(stops[at].clone());
    }

    /// ← (-1) or → (1) on the icon row: the button beside (not past the row's ends). Whether
    /// it moved; off the icon row they're the caret's.
    pub fn side(&mut self, delta: i32) -> bool {
        let Some(Lit::Icon(k)) = self.lit else {
            return false;
        };
        let at = NewKind::ALL.iter().position(|x| *x == k).unwrap_or(0) as i64 + i64::from(delta);
        let Some(&next) = usize::try_from(at).ok().and_then(|i| NewKind::ALL.get(i)) else {
            return false;
        };
        self.lit = Some(Lit::Icon(next));
        self.icon_last = next;
        true
    }

    /// The pointer moved onto `lit`: it's lit if it can be chosen. Whether that changed.
    pub fn hover(&mut self, lit: Lit) -> bool {
        if self.lit.as_ref() == Some(&lit) || !self.can_light(&lit) {
            return false;
        }
        if let Lit::Icon(k) = lit {
            self.icon_last = k;
        }
        self.lit = Some(lit);
        true
    }

    /// What Enter (or a click, once lit) does: blank, the lit button or agent row, the one ⌘T
    /// would make (or nothing lit) being ⌘T itself; typed, the lit row's.
    pub fn chosen(&self) -> Option<Choice> {
        if self.query.is_empty() {
            if self.lit.is_none() || self.lit == self.like {
                return Some(Choice::Like);
            }
            return match self.lit.as_ref()? {
                Lit::Icon(k) => Some(Choice::New(*k)),
                Lit::Agent(p) => self
                    .agent(*p)
                    .filter(|a| a.enabled())
                    .map(|_| Choice::Agent(*p)),
                Lit::Line(_) => None,
            };
        }
        let Some(Lit::Line(i)) = self.lit else {
            return None;
        };
        let Some(Line::Row(row)) = self.lines.get(i).filter(|_| self.can_light(&Lit::Line(i)))
        else {
            return None;
        };
        Some(match &row.item {
            Item::New(k) => Choice::New(*k),
            Item::Agent(p) => Choice::Agent(*p),
            Item::Url(url) => Choice::Url(url.clone()),
            Item::Tab(t) => Choice::Tab(*t),
            Item::Files => Choice::Files(self.query.trim().to_string()),
        })
    }

    /// Button `k`'s tooltip, " (⌘T)" on the one ⌘T would make.
    pub fn tooltip(&self, k: NewKind) -> String {
        if self.like == Some(Lit::Icon(k)) {
            format!("{} (⌘T)", k.tooltip())
        } else {
            k.tooltip().to_string()
        }
    }

    /// The words in agent row `a`'s slot: "⌘T" on the row ⌘T would open, "Not installed" on
    /// one whose command isn't on your PATH; else none.
    pub fn agent_slot(&self, a: &AgentRow) -> Option<&'static str> {
        if !a.enabled() {
            Some(NOT_INSTALLED)
        } else if self.like == Some(Lit::Agent(a.preset)) {
            Some("⌘T")
        } else {
            None
        }
    }

    /// The icon row's words at its right: the lit button's name and (on ⌘T's) its key.
    pub fn icon_meta(&self) -> Option<(&'static str, Option<&'static str>)> {
        match self.lit {
            Some(Lit::Icon(k)) => {
                Some((k.name(), (self.like == Some(Lit::Icon(k))).then_some("⌘T")))
            }
            _ => None,
        }
    }
}

/// The Workspace's copy of the agents' PATH answers (the + menu asks on open; the first ask can
/// take seconds, so it runs off the UI thread): for which commands, the answers, when they came,
/// and whether an ask is running.
#[derive(Clone, Debug, PartialEq)]
pub struct AgentsFound {
    pub commands: Vec<String>,
    pub found: Vec<Found>,
    pub checked: Option<Instant>,
    pub running: bool,
}

/// Whether the menu opening at `now` should ask again: never asked, other commands (the presets
/// changed), or answers older than `RECHECK`; not while an ask for these commands runs.
pub fn needs_check(cache: Option<&AgentsFound>, commands: &[String], now: Instant) -> bool {
    let Some(c) = cache else {
        return true;
    };
    if c.commands != commands {
        return true;
    }
    if c.running {
        return false;
    }
    c.checked
        .is_none_or(|at| now.saturating_duration_since(at) >= RECHECK)
}

/// The answers the rows show for `commands`: the cached ones for these commands, else
/// "checking" (read as installed).
pub fn found_for(cache: Option<&AgentsFound>, commands: &[String]) -> Vec<Found> {
    match cache {
        Some(c) if c.commands == commands && c.found.len() == commands.len() => c.found.clone(),
        _ => vec![Found::Checking; commands.len()],
    }
}

// ---- The menu over the window (GPUI) ----

// Bound in main.rs under the "NewTabMenu" key context, as "Palette" is: ↑↓ always; ←→ only while
// the icon row is lit (its context then says `icons`), else they stay the caret's. Enter and
// Escape are the box's own.
actions!(new_tab_menu, [SelectPrev, SelectNext, IconPrev, IconNext]);

/// The panel's width, its search box's placeholder, and its parts' sizes (the designer's spec).
pub const PANEL_WIDTH: f32 = 290.0;
pub const PLACEHOLDER: &str = "Search tabs, files, URLs, agents\u{2026}";
const BUTTON: f32 = 28.0;
const BUTTON_ICON: f32 = 14.0;
const AGENT_ROW: f32 = 28.0;
const LOGO: f32 = 16.0;
const RESULT_ROW: f32 = 26.0;
const RESULT_ICON: f32 = 14.0;
const HEADING: f32 = 22.0;
/// At most this many agent rows show; more scroll.
pub const MAX_AGENT_ROWS: usize = 8;
/// A preset row not installed: its logo this opaque.
const DISABLED_LOGO: f32 = 0.4;
/// Its tooltip.
pub const NOT_FOUND_TIP: &str = "Not found on your PATH";

pub enum NewTabMenuEvent {
    /// Do this (the menu closes first).
    Pick(Choice),
    /// Escape with nothing typed, or the box lost focus.
    Close,
}

/// The + menu: its search box (a `NameInput`, as the palette's) and its model.
pub struct NewTabMenu {
    input: Entity<NameInput>,
    model: MenuModel,
    /// The presets' commands its agent rows were made for (PATH answers for others are
    /// dropped).
    commands: Vec<String>,
    /// The tabs, as they were at open.
    tabs: Vec<TabEntry>,
    /// The workspace is on this Mac: a files row.
    files: bool,
    icons: ScrollHandle,
    agent_scroll: ScrollHandle,
    results_scroll: ScrollHandle,
    /// Where the panel was last drawn (the UI screenshots read it).
    panel_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    _input_events: Subscription,
    _input_changes: Subscription,
}

impl EventEmitter<NewTabMenuEvent> for NewTabMenu {}

impl Focusable for NewTabMenu {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.focus_handle(cx)
    }
}

impl NewTabMenu {
    pub fn new(
        agents: Vec<AgentRow>,
        commands: Vec<String>,
        plan: &NewTabPlan,
        tabs: Vec<TabEntry>,
        files: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| NameInput::new(String::new(), window, cx));
        let input_events = cx.subscribe_in(&input, window, |this, input, event, window, cx| {
            match event {
                NameInputEvent::Confirm(_) => this.confirm(cx),
                // Escape with something typed clears it; with nothing (or the box losing the
                // keyboard) the menu closes.
                NameInputEvent::Cancel => {
                    let typed = !input.read(cx).text().is_empty();
                    if typed && input.focus_handle(cx).is_focused(window) {
                        input.update(cx, |input, cx| {
                            input.set_text(String::new());
                            cx.notify();
                        });
                    } else {
                        cx.emit(NewTabMenuEvent::Close);
                    }
                }
            }
        });
        let input_changes = cx.observe(&input, |this, _, cx| this.query_changed(cx));
        Self {
            input,
            model: MenuModel::new(agents, plan),
            commands,
            tabs,
            files,
            icons: ScrollHandle::new(),
            agent_scroll: ScrollHandle::new(),
            results_scroll: ScrollHandle::new(),
            panel_bounds: Rc::default(),
            _input_events: input_events,
            _input_changes: input_changes,
        }
    }

    /// The box changed: its rows again, only when the query did (a caret move notifies too).
    fn query_changed(&mut self, cx: &mut Context<Self>) {
        let text = self.input.read(cx).text().to_string();
        let query = if text.trim().is_empty() { "" } else { &text };
        if query != self.model.query {
            self.relist_for(&text, cx);
        }
    }

    /// The rows again for what is typed (the browser history came).
    pub fn relist(&mut self, cx: &mut Context<Self>) {
        let text = self.input.read(cx).text().to_string();
        self.relist_for(&text, cx);
    }

    fn relist_for(&mut self, text: &str, cx: &mut Context<Self>) {
        let lines = if text.trim().is_empty() {
            Vec::new()
        } else {
            let history = history_for(text, cx);
            results(
                text,
                &Sources {
                    agents: &self.model.agents,
                    tabs: &self.tabs,
                    history: &history,
                    files: self.files,
                },
            )
        };
        self.model.set_results(text, lines);
        self.scroll_to_lit();
        cx.notify();
    }

    /// The PATH answers for `commands` came.
    pub fn set_found(&mut self, commands: &[String], found: &[Found], cx: &mut Context<Self>) {
        if commands == self.commands {
            self.model.set_found(found);
            cx.notify();
        }
    }

    /// Enter: the lit item (with nothing typed and nothing lit, ⌘T's).
    fn confirm(&mut self, cx: &mut Context<Self>) {
        if let Some(choice) = self.model.chosen() {
            cx.emit(NewTabMenuEvent::Pick(choice));
        }
    }

    /// A click: `lit` lights (if it can be chosen), then it's done.
    fn pick(&mut self, lit: Lit, cx: &mut Context<Self>) {
        self.model.hover(lit.clone());
        if self.model.lit == Some(lit) {
            self.confirm(cx);
        }
    }

    /// The pointer moved onto `lit`: it lights (a redraw only if that changed). Only a move
    /// does it, so rows drawn again under a still pointer while you type never take the light.
    fn hover(&mut self, lit: Lit, cx: &mut Context<Self>) {
        if self.model.hover(lit) {
            cx.notify();
        }
    }

    fn select_by(&mut self, delta: i32, cx: &mut Context<Self>) {
        self.model.step(delta);
        self.scroll_to_lit();
        cx.notify();
    }

    fn side(&mut self, delta: i32, cx: &mut Context<Self>) {
        if self.model.side(delta) {
            cx.notify();
        }
    }

    fn scroll_to_lit(&self) {
        match self.model.lit {
            Some(Lit::Agent(p)) => {
                if let Some(at) = self.model.agents.iter().position(|a| a.preset == p) {
                    self.agent_scroll.scroll_to_item(at);
                }
            }
            Some(Lit::Line(at)) => self.results_scroll.scroll_to_item(at),
            _ => {}
        }
    }

    /// The icon row: Terminal, Browser, File; the lit one's name and key at its right.
    fn icon_row(&self, ui: &UiColors, cx: &mut Context<Self>) -> AnyElement {
        let buttons: Vec<AnyElement> = NewKind::ALL
            .iter()
            .map(|&k| {
                let lit = self.model.lit == Some(Lit::Icon(k));
                let tip: SharedString = self.model.tooltip(k).into();
                let ink = if lit { ui.text } else { ui.text_dim };
                div()
                    .id(("new-tab-menu-icon", k as usize))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(BUTTON))
                    .rounded(px(6.0))
                    .when(lit, |b| b.bg(hsla(ui.panel_hover)))
                    .cursor_pointer()
                    .child(crate::icons::icon(k.icon(), px(BUTTON_ICON), hsla(ink)))
                    .on_mouse_move(cx.listener(move |this, _: &MouseMoveEvent, _, cx| {
                        this.hover(Lit::Icon(k), cx)
                    }))
                    .on_click(
                        cx.listener(move |this, _: &ClickEvent, _, cx| this.pick(Lit::Icon(k), cx)),
                    )
                    .tooltip(move |_, cx| -> AnyView { cx.new(|_| Tip(tip.clone())).into() })
                    .into_any_element()
            })
            .collect();
        let meta = self.model.icon_meta().map(|(name, key)| {
            div()
                .flex_none()
                .flex()
                .gap(px(8.0))
                .pr(px(6.0))
                .text_size(px(text::MENU_META))
                .child(div().text_color(hsla(ui.text_dim)).child(name))
                .children(key.map(|key| div().text_color(hsla(ui.text_faint)).child(key)))
        });
        div()
            .id("new-tab-menu-icons")
            .flex()
            .items_center()
            .h(px(BUTTON))
            .gap(px(2.0))
            .track_scroll(&self.icons)
            .children(buttons)
            .child(div().flex_1())
            .children(meta)
            .into_any_element()
    }

    /// The agent rows (one per preset; at most `MAX_AGENT_ROWS` show, the rest scroll under an
    /// overlay thumb).
    fn agent_list(&self, ui: &UiColors, cx: &mut Context<Self>) -> AnyElement {
        let rows: Vec<AnyElement> = self
            .model
            .agents
            .iter()
            .map(|a| self.agent_row(a, ui, cx))
            .collect();
        let n = rows.len();
        let viewport = n.min(MAX_AGENT_ROWS) as f32 * AGENT_ROW;
        let content = n as f32 * AGENT_ROW;
        let handle = self.agent_scroll.clone();
        let ink = ui.text;
        div()
            .relative()
            .child(
                div()
                    .id("new-tab-menu-agents")
                    .flex()
                    .flex_col()
                    .max_h(px(viewport))
                    .overflow_y_scroll()
                    .track_scroll(&self.agent_scroll)
                    .children(rows),
            )
            .when(n > MAX_AGENT_ROWS, |list| {
                list.child(
                    canvas(
                        |_, _, _| {},
                        move |area, _, window, _| {
                            let offset = -f32::from(handle.offset().y);
                            if let Some(t) = scrollbar::thumb(viewport, content, offset) {
                                let alpha = scrollbar::alpha(Duration::ZERO, false);
                                scrollbar::paint(
                                    window,
                                    area,
                                    ScrollAxis::Vertical,
                                    &t,
                                    alpha,
                                    ink,
                                );
                            }
                        },
                    )
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
                )
            })
            .into_any_element()
    }

    /// A preset's row: its logo, its name, and its slot (⌘T on the row ⌘T would open; "Not
    /// installed", the row greyed and not choosable, when its command isn't on your PATH).
    fn agent_row(&self, a: &AgentRow, ui: &UiColors, cx: &mut Context<Self>) -> AnyElement {
        let enabled = a.enabled();
        let preset = a.preset;
        let lit = enabled && self.model.lit == Some(Lit::Agent(preset));
        let ink = if enabled { ui.text } else { ui.text_faint };
        div()
            .id(("new-tab-menu-agent", preset))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(8.0))
            .h(px(AGENT_ROW))
            .pl(px(8.0))
            .pr(px(8.0))
            .rounded(px(6.0))
            .text_size(px(text::MENU))
            .when(lit, |r| r.bg(hsla(ui.panel_hover)))
            .child(agent_logo(a, LOGO, cx))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .text_color(hsla(ink))
                    .child(a.name.clone()),
            )
            .children(self.model.agent_slot(a).map(|slot| {
                div()
                    .flex_none()
                    .text_size(px(text::KEYS))
                    .text_color(hsla(ui.text_faint))
                    .child(slot)
            }))
            .when(enabled, |r| {
                r.cursor_pointer()
                    .on_mouse_move(cx.listener(move |this, _: &MouseMoveEvent, _, cx| {
                        this.hover(Lit::Agent(preset), cx)
                    }))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.pick(Lit::Agent(preset), cx)
                    }))
            })
            .when(!enabled, |r| {
                r.tooltip(|_, cx| -> AnyView { cx.new(|_| Tip(NOT_FOUND_TIP.into())).into() })
            })
            .into_any_element()
    }

    /// The typed results under their headings, or one line when nothing matches.
    fn results_list(&self, ui: &UiColors, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        if self.model.lines.is_empty() {
            return div()
                .px(px(8.0))
                .py(px(4.0))
                .text_size(px(text::MENU))
                .text_color(hsla(ui.text_dim))
                .child(format!(
                    "No matches for \u{2018}{}\u{2019}",
                    self.model.query.trim()
                ))
                .into_any_element();
        }
        let lines: Vec<AnyElement> = self
            .model
            .lines
            .iter()
            .enumerate()
            .map(|(at, line)| match line {
                Line::Heading(h) => div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .h(px(HEADING))
                    .px(px(8.0))
                    .text_size(px(text::MENU_META))
                    .text_color(hsla(ui.text_dim))
                    .child(*h)
                    .into_any_element(),
                Line::Row(row) => self.result_row(at, row, ui, cx),
            })
            .collect();
        let max_h = crate::palette::list_max_height(f32::from(window.viewport_size().height));
        div()
            .id("new-tab-menu-results")
            .flex()
            .flex_col()
            .max_h(px(max_h))
            .overflow_y_scroll()
            .track_scroll(&self.results_scroll)
            .children(lines)
            .into_any_element()
    }

    fn result_row(
        &self,
        at: usize,
        row: &Row,
        ui: &UiColors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let lit = row.enabled && self.model.lit == Some(Lit::Line(at));
        let icon = match row.icon {
            RowIcon::Icon(icon) => {
                crate::icons::icon(icon, px(RESULT_ICON), hsla(ui.text_dim)).into_any_element()
            }
            RowIcon::Agent(p) => match self.model.agents.iter().find(|a| a.preset == p) {
                Some(a) => agent_logo(a, RESULT_ICON, cx),
                None => div().size(px(RESULT_ICON)).into_any_element(),
            },
            RowIcon::Tab(icon) => crate::tab_bar::tile(icon, ui, cx).into_any_element(),
        };
        let ink = if row.enabled { ui.text } else { ui.text_faint };
        div()
            .id(("new-tab-menu-row", at))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(8.0))
            .h(px(RESULT_ROW))
            .px(px(8.0))
            .rounded(px(5.0))
            .text_size(px(text::MENU))
            .when(lit, |r| r.bg(hsla(ui.panel_hover)))
            .child(icon)
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .text_color(hsla(ink))
                    .child(crate::palette::highlighted(&row.label, &row.positions)),
            )
            .children(row.meta.clone().map(|meta| {
                div()
                    .flex_none()
                    .max_w(px(120.0))
                    .truncate()
                    .text_size(px(text::MENU_META))
                    .text_color(hsla(ui.text_faint))
                    .child(meta)
            }))
            .when(row.enabled, |r| {
                r.cursor_pointer()
                    .on_mouse_move(cx.listener(move |this, _: &MouseMoveEvent, _, cx| {
                        this.hover(Lit::Line(at), cx)
                    }))
                    .on_click(
                        cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.pick(Lit::Line(at), cx)
                        }),
                    )
            })
            .into_any_element()
    }
}

/// A preset's logo `size` square: its provider's logo, else its badge (an unknown provider's:
/// "?"); faded while it isn't installed.
fn agent_logo(a: &AgentRow, size: f32, cx: &App) -> AnyElement {
    let logo = a
        .provider
        .and_then(crate::providers::by_name)
        .and_then(|p| p.info().logo)
        .and_then(|logo| crate::logos::image(logo, size, cx));
    let mark = match logo {
        Some(image) => gpui::img(image).size(px(size)).into_any_element(),
        None => {
            let (_, badge, rgb) = crate::workspace_agent::provider_look(&a.name);
            crate::workspace_agent::agent_badge(badge, rgb, size).into_any_element()
        }
    };
    div()
        .flex_none()
        .size(px(size))
        .when(!a.enabled(), |d| d.opacity(DISABLED_LOGO))
        .child(mark)
        .into_any_element()
}

/// The history's suggestions for `query` (address, title), only while history is kept and
/// already read (its first read is the Workspace's, off the UI thread).
fn history_for(query: &str, cx: &mut App) -> Vec<(String, String)> {
    if !cx.global::<Appearance>().browser_keep_data || !BrowserHistory::is_loaded(cx) {
        return Vec::new();
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    browser_history::suggest(BrowserHistory::list(cx), query, now)
        .into_iter()
        .take(HISTORY_RESULTS)
        .map(|v| (v.url.clone(), v.title.clone()))
        .collect()
}

impl Render for NewTabMenu {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = cx.global::<Appearance>().theme.ui();
        let empty = self.input.read(cx).text().is_empty();
        let field = overlay_panel::filter_box(&ui, &self.input, empty, PLACEHOLDER);
        let context = if matches!(self.model.lit, Some(Lit::Icon(_))) {
            "NewTabMenu icons"
        } else {
            "NewTabMenu"
        };
        let blank = self.model.query.is_empty();
        let hairline = || {
            div()
                .flex_none()
                .h(px(1.0))
                .mx(px(8.0))
                .bg(hsla(ui.panel_border))
        };
        let body: Vec<AnyElement> = if blank {
            let mut body = vec![self.icon_row(&ui, cx)];
            if !self.model.agents.is_empty() {
                body.push(hairline().into_any_element());
                body.push(self.agent_list(&ui, cx));
            }
            body
        } else {
            vec![self.results_list(&ui, window, cx)]
        };
        let bounds = self.panel_bounds.clone();
        overlay_panel::shell(&ui, PANEL_WIDTH)
            .relative()
            .key_context(context)
            .on_action(cx.listener(|this, _: &SelectPrev, _, cx| this.select_by(-1, cx)))
            .on_action(cx.listener(|this, _: &SelectNext, _, cx| this.select_by(1, cx)))
            .on_action(cx.listener(|this, _: &IconPrev, _, cx| this.side(-1, cx)))
            .on_action(cx.listener(|this, _: &IconNext, _, cx| this.side(1, cx)))
            .child(field)
            .children(body)
            .child(
                canvas(move |b, _, _| bounds.set(Some(b)), |_, _, _, _| {})
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
            )
    }
}

/// The UI screenshots' look inside.
#[cfg(feature = "ui-shots")]
impl NewTabMenu {
    pub fn model(&self) -> &MenuModel {
        &self.model
    }

    pub fn panel_bounds(&self) -> Option<Bounds<Pixels>> {
        self.panel_bounds.get()
    }

    /// Where icon button `k`, agent row `n` (in order) or typed line `at` was last drawn.
    pub fn icon_bounds(&self, k: NewKind) -> Option<Bounds<Pixels>> {
        self.icons.bounds_for_item(k as usize)
    }

    pub fn agent_bounds(&self, n: usize) -> Option<Bounds<Pixels>> {
        self.agent_scroll.bounds_for_item(n)
    }

    pub fn line_bounds(&self, at: usize) -> Option<Bounds<Pixels>> {
        self.results_scroll.bounds_for_item(at)
    }

    pub fn box_text(&self, cx: &App) -> String {
        self.input.read(cx).text().to_string()
    }

    pub fn input_focused(&self, window: &Window, cx: &App) -> bool {
        self.input.focus_handle(cx).is_focused(window)
    }
}

// ---- The Workspace's side: open, act, close ----

/// An open + menu: whose pane, the point it hangs from (the +'s bottom left), and the menu.
pub(crate) struct OpenNewTabMenu {
    pub pane: PaneId,
    pub at: Point<Pixels>,
    pub menu: Entity<NewTabMenu>,
}

/// How far the panel sits from the +'s bottom left, and from the window's edges.
const OFFSET: (f32, f32) = (-8.0, 4.0);
const EDGE_MARGIN: f32 = 8.0;

impl Workspace {
    /// A click on `pane`'s +: its menu opens hanging from `at` (the +'s bottom left), its search
    /// box takes the keyboard; open on this pane already, it closes.
    pub(crate) fn toggle_new_tab_menu(
        &mut self,
        pane: PaneId,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.new_tab_menu.as_ref().is_some_and(|m| m.pane == pane) {
            self.dismiss_new_tab_menu(window, cx);
            return;
        }
        if self.model.pane(pane).is_none() {
            return;
        }
        self.close_overlays(Overlay::NewTabMenu);
        let local = self.pane_is_local(pane);
        let presets: Vec<Preset> = cx
            .global::<Appearance>()
            .presets
            .iter()
            .take(MAX_PRESETS)
            .cloned()
            .collect();
        let commands: Vec<String> = presets.iter().map(|p| p.command.clone()).collect();
        // Agents run only on this Mac: none in a workspace on another host.
        let agents: Vec<AgentRow> = if local {
            let found = found_for(self.agents_found.as_ref(), &commands);
            presets
                .iter()
                .zip(found)
                .enumerate()
                .map(|(preset, (p, found))| AgentRow {
                    preset,
                    name: p.name.clone(),
                    provider: crate::presets::provider_for(p).map(|v| v.info().name),
                    found,
                })
                .collect()
        } else {
            Vec::new()
        };
        let kind = self
            .model
            .pane(pane)
            .and_then(|p| self.model.tab(p.active))
            .map(|t| t.kind.clone());
        let plan = plan_like(kind.as_ref(), local);
        let tabs = self.tab_entries();
        let menu =
            cx.new(|cx| NewTabMenu::new(agents, commands.clone(), &plan, tabs, local, window, cx));
        cx.subscribe_in(&menu, window, Self::on_new_tab_menu_event)
            .detach();
        menu.focus_handle(cx).focus(window, cx);
        self.new_tab_menu = Some(OpenNewTabMenu { pane, at, menu });
        if local
            && !self.agents_frozen
            && needs_check(self.agents_found.as_ref(), &commands, Instant::now())
        {
            self.check_agents(commands, cx);
        }
        self.load_history(cx);
        cx.notify();
    }

    /// Every tab of every workspace as the typed results list them (taken once, at open).
    fn tab_entries(&self) -> Vec<TabEntry> {
        let now = Instant::now();
        let mut entries = Vec::new();
        for w in self.model.workspaces() {
            let workspace = self.workspace_label(w.id).to_string();
            for p in &w.panes {
                for t in self.pane_bar_tabs(p, now) {
                    entries.push(TabEntry {
                        tab: t.tab,
                        title: t.title.to_string(),
                        workspace: workspace.clone(),
                        icon: t.icon,
                    });
                }
            }
        }
        entries
    }

    /// Ask whether the presets' `commands` are on your PATH, off the UI thread (the first ask
    /// can take seconds, as Welcome's): the answers kept meanwhile still show; the new ones go
    /// to the cache and an open menu.
    fn check_agents(&mut self, commands: Vec<String>, cx: &mut Context<Self>) {
        self.agents_found = Some(AgentsFound {
            found: found_for(self.agents_found.as_ref(), &commands),
            checked: self.agents_found.as_ref().and_then(|c| c.checked),
            commands: commands.clone(),
            running: true,
        });
        let asked = commands.clone();
        let check = cx.background_spawn(async move { crate::agent_detect::check(asked) });
        cx.spawn(async move |this, cx| {
            let found = check.await;
            let _ = this.update(cx, |ws, cx| ws.agents_checked(commands, found, cx));
        })
        .detach();
    }

    /// The PATH answers for `commands` came (or the UI screenshots set them).
    pub(crate) fn agents_checked(
        &mut self,
        commands: Vec<String>,
        found: Vec<Found>,
        cx: &mut Context<Self>,
    ) {
        if let Some(open) = &self.new_tab_menu {
            open.menu
                .update(cx, |m, cx| m.set_found(&commands, &found, cx));
        }
        self.agents_found = Some(AgentsFound {
            commands,
            found,
            checked: Some(Instant::now()),
            running: false,
        });
    }

    /// Kept browser history not read yet: read it off the UI thread, then the menu lists it.
    fn load_history(&mut self, cx: &mut Context<Self>) {
        let keep = cx.global::<Appearance>().browser_keep_data;
        if !keep || self.history_loading || BrowserHistory::is_loaded(cx) {
            return;
        }
        self.history_loading = true;
        let read = cx.background_spawn(async { browser_history::read_file() });
        cx.spawn(async move |this, cx| {
            let list = read.await;
            let _ = this.update(cx, |ws, cx| {
                ws.history_loading = false;
                BrowserHistory::set_loaded(list, cx);
                if let Some(open) = &ws.new_tab_menu {
                    open.menu.update(cx, |m, cx| m.relist(cx));
                }
            });
        })
        .detach();
    }

    /// The menu picked or closed; one already closed or replaced is ignored (its box's late
    /// "lost focus").
    fn on_new_tab_menu_event(
        &mut self,
        menu: &Entity<NewTabMenu>,
        event: &NewTabMenuEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = self.new_tab_menu.as_ref().filter(|m| m.menu == *menu) else {
            return;
        };
        let pane = open.pane;
        match event {
            NewTabMenuEvent::Close => self.dismiss_new_tab_menu(window, cx),
            NewTabMenuEvent::Pick(choice) => {
                self.run_new_tab_choice(pane, choice.clone(), window, cx)
            }
        }
    }

    /// Escape, a click outside, the box losing the keyboard: the menu goes and the typing
    /// returns to the pane.
    pub(crate) fn dismiss_new_tab_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.new_tab_menu.take().is_some() {
            self.focus_active(window, cx);
            cx.notify();
        }
    }

    /// Do `choice` for `pane` (the menu closes): the pane takes the focus first, as the bar's
    /// split buttons do.
    pub(crate) fn run_new_tab_choice(
        &mut self,
        pane: PaneId,
        choice: Choice,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.new_tab_menu = None;
        cx.notify();
        if self.model.pane(pane).is_none() {
            self.focus_active(window, cx);
            return;
        }
        if self.model.focus_pane(pane) {
            self.sync();
        }
        self.focus_active(window, cx);
        match choice {
            Choice::Like => self.new_tab_like(pane, window, cx),
            Choice::New(NewKind::Terminal) => self.new_terminal_tab(pane, true, window, cx),
            Choice::New(NewKind::Browser) => self.new_browser_tab(pane, window, cx),
            Choice::New(NewKind::File) => {
                self.open_palette_with(Scope::Files, String::new(), window, cx)
            }
            Choice::Agent(n) => self.new_preset_tab(pane, n, window, cx),
            Choice::Url(url) => self.new_browser_tab_at(pane, Some(url), window, cx),
            Choice::Tab(tab) => self.select_tab(tab, window, cx),
            Choice::Files(query) => self.open_palette_with(Scope::Files, query, window, cx),
        }
    }

    /// Preset `n` as a new tab at the end of `pane`'s bar: a provider's as its agent tab;
    /// another's (opencode, codex…) as a terminal that runs it, where the pane's terminal is.
    fn new_preset_tab(
        &mut self,
        pane: PaneId,
        n: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(preset) = cx.global::<Appearance>().presets.get(n).cloned() else {
            return;
        };
        if let Some(provider) = crate::presets::provider_for(&preset) {
            self.new_agent_tab(pane, provider.info().name, window, cx);
            return;
        }
        let session = self.pane_session(pane);
        let project = session
            .and_then(|s| s.project.clone())
            .or_else(|| self.active_project());
        let cwd = session
            .and_then(|s| self.folder_now(s))
            .or_else(|| self.active_folder_now());
        let launch = Launch {
            saved: SavedSession {
                project,
                cwd,
                preset: Some(preset),
                ..Default::default()
            },
            run_preset: true,
        };
        self.open_session_then(launch, At::Tab(pane), None, window, cx);
    }

    /// The open + menu, hanging from its +, with its click-away backdrop.
    pub(crate) fn render_new_tab_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let open = self.new_tab_menu.as_ref()?;
        self.model.pane(open.pane)?;
        Some(
            div()
                .id("new-tab-menu-backdrop")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .occlude()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _: &MouseDownEvent, window, cx| {
                        this.dismiss_new_tab_menu(window, cx)
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|this, _: &MouseDownEvent, window, cx| {
                        this.dismiss_new_tab_menu(window, cx)
                    }),
                )
                .child(
                    anchored()
                        .anchor(Anchor::TopLeft)
                        .position(open.at)
                        .offset(point(px(OFFSET.0), px(OFFSET.1)))
                        .snap_to_window_with_margin(px(EDGE_MARGIN))
                        .child(
                            div()
                                // Its own clicks don't reach the backdrop under it.
                                .on_mouse_down(MouseButton::Left, |_: &MouseDownEvent, _, cx| {
                                    cx.stop_propagation()
                                })
                                .on_mouse_down(MouseButton::Right, |_: &MouseDownEvent, _, cx| {
                                    cx.stop_propagation()
                                })
                                .child(open.menu.clone()),
                        ),
                )
                .into_any_element(),
        )
    }

    /// The UI screenshots' PATH answers (the real ask never runs for them: `agents_frozen`).
    #[cfg(feature = "ui-shots")]
    pub(crate) fn shot_agents_found(&mut self, found: Vec<Found>, cx: &mut Context<Self>) {
        let commands: Vec<String> = cx
            .global::<Appearance>()
            .presets
            .iter()
            .take(MAX_PRESETS)
            .map(|p| p.command.clone())
            .collect();
        self.agents_checked(commands, found, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout_model::FileMode;

    fn agent(provider: &str) -> TabKind {
        TabKind::Agent {
            provider: provider.into(),
            cwd: PathBuf::from("/Users/me/code/my-app"),
            conversation: Some("c1".into()),
            mode: Some("plan".into()),
            title: Some("Fix the build".into()),
            model: Some("opus".into()),
            effort: Some("high".into()),
        }
    }

    #[test]
    fn command_t_follows_the_last_tab() {
        assert_eq!(
            plan_like(Some(&TabKind::Terminal(3)), true),
            NewTabPlan::Terminal
        );
        assert_eq!(
            plan_like(Some(&TabKind::Terminal(3)), false),
            NewTabPlan::Terminal
        );
        assert_eq!(
            plan_like(Some(&TabKind::Browser { url: None }), true),
            NewTabPlan::Browser
        );
        let file = TabKind::File {
            path: PathBuf::from("fuxx-cli/src/main.rs"),
            mode: FileMode::Plain,
        };
        assert_eq!(plan_like(Some(&file), true), NewTabPlan::Terminal);
        assert_eq!(plan_like(None, true), NewTabPlan::Terminal);
    }

    #[test]
    fn command_t_in_an_agent_tab_makes_the_same_agent_in_its_folder() {
        // Its folder, model and effort carried; its mode isn't (the provider's default).
        for provider in ["claude", "agy"] {
            assert_eq!(
                plan_like(Some(&agent(provider)), true),
                NewTabPlan::Agent {
                    provider: crate::providers::by_name(provider).unwrap().info().name,
                    cwd: PathBuf::from("/Users/me/code/my-app"),
                    model: Some("opus".into()),
                    effort: Some("high".into()),
                },
                "{provider}"
            );
        }
        // An agent fuxx doesn't know, or one in a workspace not on this Mac: a terminal.
        assert_eq!(plan_like(Some(&agent("aider")), true), NewTabPlan::Terminal);
        assert_eq!(
            plan_like(Some(&agent("claude")), false),
            NewTabPlan::Terminal
        );
    }
    use std::time::Duration;

    fn row(preset: usize, name: &str, provider: Option<&'static str>, found: Found) -> AgentRow {
        AgentRow {
            preset,
            name: name.into(),
            provider,
            found,
        }
    }

    /// The built-in four: claude and agy have providers; opencode isn't installed.
    fn four() -> Vec<AgentRow> {
        vec![
            row(0, "claude", Some("claude"), Found::Yes),
            row(1, "agy", Some("agy"), Found::Checking),
            row(2, "opencode", None, Found::No),
            row(3, "codex", None, Found::Unknown),
        ]
    }

    fn claude_plan() -> NewTabPlan {
        NewTabPlan::Agent {
            provider: "claude",
            cwd: PathBuf::from("/Users/me/code/my-app"),
            model: None,
            effort: None,
        }
    }

    #[test]
    fn the_icon_row_is_terminal_browser_file() {
        assert_eq!(
            NewKind::ALL.map(NewKind::name),
            ["Terminal", "Browser", "File"]
        );
        assert_eq!(
            NewKind::ALL.map(NewKind::tooltip),
            [
                "New terminal tab",
                "New browser tab",
                "Open a file in a tab…"
            ]
        );
    }

    #[test]
    fn it_opens_lit_on_what_command_t_makes() {
        let m = MenuModel::new(four(), &NewTabPlan::Terminal);
        assert_eq!(m.lit, Some(Lit::Icon(NewKind::Terminal)));
        assert_eq!(m.like, Some(Lit::Icon(NewKind::Terminal)));
        let m = MenuModel::new(four(), &NewTabPlan::Browser);
        assert_eq!(m.lit, Some(Lit::Icon(NewKind::Browser)));
        let m = MenuModel::new(four(), &claude_plan());
        assert_eq!(m.lit, Some(Lit::Agent(0)));
        // Enter with nothing typed is ⌘T.
        assert_eq!(m.chosen(), Some(Choice::Like));
        // An agent with no preset row (or one not installed): nothing lit, Enter still ⌘T.
        let m = MenuModel::new(vec![row(2, "opencode", None, Found::No)], &claude_plan());
        assert_eq!((m.like.clone(), m.lit.clone()), (None, None));
        assert_eq!(m.chosen(), Some(Choice::Like));
    }

    #[test]
    fn the_command_t_marks_follow_the_plan() {
        let m = MenuModel::new(four(), &NewTabPlan::Terminal);
        assert_eq!(m.tooltip(NewKind::Terminal), "New terminal tab (⌘T)");
        assert_eq!(m.tooltip(NewKind::Browser), "New browser tab");
        assert_eq!(m.agent_slot(&four()[0]), None);
        let m = MenuModel::new(four(), &claude_plan());
        assert_eq!(m.tooltip(NewKind::Terminal), "New terminal tab");
        assert_eq!(m.agent_slot(&four()[0]), Some("⌘T"));
        assert_eq!(m.agent_slot(&four()[1]), None);
        assert_eq!(m.agent_slot(&four()[2]), Some(NOT_INSTALLED));
        // The icon row's words: the lit button's name and its key.
        let m = MenuModel::new(four(), &NewTabPlan::Terminal);
        assert_eq!(m.icon_meta(), Some(("Terminal", Some("⌘T"))));
        let m = MenuModel::new(four(), &NewTabPlan::Browser);
        assert_eq!(m.icon_meta(), Some(("Browser", Some("⌘T"))));
        let m = MenuModel::new(four(), &claude_plan());
        assert_eq!(m.icon_meta(), None, "an agent row is lit");
    }

    #[test]
    fn up_and_down_step_over_the_icon_row_and_the_installed_agents() {
        let mut m = MenuModel::new(four(), &NewTabPlan::Terminal);
        m.step(1);
        assert_eq!(m.lit, Some(Lit::Agent(0)));
        m.step(1);
        assert_eq!(m.lit, Some(Lit::Agent(1)), "checking reads as installed");
        m.step(1);
        assert_eq!(
            m.lit,
            Some(Lit::Agent(3)),
            "opencode isn't installed: skipped"
        );
        m.step(1);
        assert_eq!(m.lit, Some(Lit::Icon(NewKind::Terminal)), "wraps");
        m.step(-1);
        assert_eq!(m.lit, Some(Lit::Agent(3)), "and back");
        // ← → move along the icon row (not past its ends); back on it, the same button.
        let mut m = MenuModel::new(four(), &NewTabPlan::Terminal);
        assert!(m.side(1));
        assert_eq!(m.lit, Some(Lit::Icon(NewKind::Browser)));
        assert!(m.side(1));
        assert!(!m.side(1), "File is the last");
        assert_eq!(m.lit, Some(Lit::Icon(NewKind::File)));
        m.step(1);
        m.step(-1);
        assert_eq!(m.lit, Some(Lit::Icon(NewKind::File)));
        assert_eq!(m.chosen(), Some(Choice::New(NewKind::File)));
        // Off the icon row, ← → aren't the menu's.
        m.step(1);
        assert!(!m.side(1));
        assert_eq!(m.lit, Some(Lit::Agent(0)));
    }

    #[test]
    fn the_pointer_lights_only_what_can_be_chosen() {
        let mut m = MenuModel::new(four(), &NewTabPlan::Terminal);
        assert!(m.hover(Lit::Agent(1)));
        assert!(!m.hover(Lit::Agent(1)), "no change, no redraw");
        assert!(!m.hover(Lit::Agent(2)), "not installed");
        assert_eq!(m.lit, Some(Lit::Agent(1)));
        assert_eq!(m.chosen(), Some(Choice::Agent(1)));
        // A click on the row ⌘T would open does what ⌘T does.
        let mut m = MenuModel::new(four(), &claude_plan());
        m.hover(Lit::Icon(NewKind::Browser));
        assert_eq!(m.chosen(), Some(Choice::New(NewKind::Browser)));
        m.hover(Lit::Agent(0));
        assert_eq!(m.chosen(), Some(Choice::Like));
    }

    #[test]
    fn found_answers_grey_out_rows_and_move_the_light_off_them() {
        let mut m = MenuModel::new(four(), &NewTabPlan::Terminal);
        m.hover(Lit::Agent(1));
        m.set_found(&[Found::Yes, Found::No, Found::No, Found::Yes]);
        assert_eq!(m.agents[1].found, Found::No);
        assert_eq!(m.lit, Some(Lit::Icon(NewKind::Terminal)), "back on ⌘T's");
        // Answers for other presets (the list changed meanwhile) are dropped.
        m.set_found(&[Found::Yes]);
        assert_eq!(m.agents[1].found, Found::No);
    }

    fn tabs() -> Vec<TabEntry> {
        vec![
            TabEntry {
                tab: TabId(7),
                title: "cargo test".into(),
                workspace: "fuxx-cli".into(),
                icon: TabIcon::Terminal,
            },
            TabEntry {
                tab: TabId(8),
                title: "Claude".into(),
                workspace: "my-app".into(),
                icon: TabIcon::Terminal,
            },
        ]
    }

    fn rows(lines: &[Line]) -> Vec<String> {
        lines
            .iter()
            .map(|l| match l {
                Line::Heading(h) => format!("# {h}"),
                Line::Row(r) => r.label.clone(),
            })
            .collect()
    }

    #[test]
    fn typing_lists_url_agents_new_tabs_history_and_files_in_that_order() {
        let agents = four();
        let tabs = tabs();
        let history = vec![("https://docs.rs/gpui".to_string(), "gpui docs".to_string())];
        let s = Sources {
            agents: &agents,
            tabs: &tabs,
            history: &history,
            files: true,
        };
        // Words: a web search; "c" matches claude, codex, opencode, cargo test and Claude; the
        // history rows are already the history's suggestions.
        let got = results("c", &s);
        assert_eq!(
            rows(&got),
            [
                "# Open URL",
                &crate::browser_history::search_label("c"),
                "# Agents",
                "claude",
                "codex",
                "opencode",
                "# Tabs",
                "cargo test",
                "Claude",
                "# History",
                "gpui docs",
                "# Files",
                "Search files for \u{201c}c\u{201d}",
            ]
        );
        // The new tabs by their names.
        let got = rows(&results("term", &s));
        assert!(
            got.windows(2).any(|w| w == ["# New", "Terminal"]),
            "{got:?}"
        );
        // An address: Open it.
        let got = results("docs.rs", &s);
        let Line::Row(open) = &got[1] else {
            panic!("{got:?}")
        };
        assert_eq!(open.label, "Open docs.rs");
        assert_eq!(open.item, Item::Url("https://docs.rs".into()));
        // The tabs' rows say their workspace; the matched letters are kept.
        let got = results("cargo", &s);
        let tab = got
            .iter()
            .find_map(|l| match l {
                Line::Row(r) if r.item == Item::Tab(TabId(7)) => Some(r),
                _ => None,
            })
            .expect("the tab");
        assert_eq!(tab.meta.as_deref(), Some("fuxx-cli"));
        assert_eq!(tab.positions, [0, 1, 2, 3, 4]);
        // No files row in a workspace on another host; agents not installed can't be chosen.
        let ssh = Sources {
            agents: &[],
            files: false,
            ..s
        };
        assert!(!rows(&results("c", &ssh)).contains(&"# Files".to_string()));
        let got = results("open", &s);
        let open = got
            .iter()
            .find_map(|l| match l {
                Line::Row(r) if r.item == Item::Agent(2) => Some(r),
                _ => None,
            })
            .expect("opencode");
        assert!(!open.enabled);
    }

    #[test]
    fn at_most_five_tabs_and_five_history_rows() {
        let tabs: Vec<TabEntry> = (0..9)
            .map(|n| TabEntry {
                tab: TabId(n),
                title: format!("zsh {n}"),
                workspace: "w".into(),
                icon: TabIcon::Terminal,
            })
            .collect();
        let history: Vec<(String, String)> = (0..9)
            .map(|n| (format!("https://z{n}.dev"), format!("z {n}")))
            .collect();
        let s = Sources {
            agents: &[],
            tabs: &tabs,
            history: &history,
            files: false,
        };
        let got = results("z", &s);
        let count = |f: fn(&Item) -> bool| {
            got.iter()
                .filter(|l| matches!(l, Line::Row(r) if f(&r.item)))
                .count()
        };
        assert_eq!(count(|i| matches!(i, Item::Tab(_))), 5);
        // Five history rows and the web search.
        assert_eq!(count(|i| matches!(i, Item::Url(_))), 6);
    }

    #[test]
    fn typed_the_first_result_is_lit_and_up_down_skip_headings() {
        let agents = four();
        let tabs = tabs();
        let s = Sources {
            agents: &agents,
            tabs: &tabs,
            history: &[],
            files: true,
        };
        let mut m = MenuModel::new(agents.clone(), &NewTabPlan::Terminal);
        m.set_results("open", results("open", &s));
        // The web search, then opencode (not installed: skipped), then the files row.
        assert_eq!(m.lit, Some(Lit::Line(1)));
        assert!(matches!(m.chosen(), Some(Choice::Url(_))));
        m.step(1);
        assert_eq!(
            m.chosen(),
            Some(Choice::Files("open".into())),
            "{:?}",
            rows(&m.lines)
        );
        m.step(1);
        assert!(matches!(m.chosen(), Some(Choice::Url(_))), "wraps");
        // Cleared: back to the icon row on ⌘T's.
        m.set_results("", Vec::new());
        assert_eq!(m.lit, Some(Lit::Icon(NewKind::Terminal)));
    }

    #[test]
    fn found_is_checked_again_only_when_stale_or_changed() {
        let now = Instant::now();
        let cmds = vec!["claude".to_string(), "agy".to_string()];
        assert!(needs_check(None, &cmds, now));
        let fresh = AgentsFound {
            commands: cmds.clone(),
            found: vec![Found::Yes, Found::No],
            checked: Some(now),
            running: false,
        };
        assert!(!needs_check(
            Some(&fresh),
            &cmds,
            now + Duration::from_secs(5)
        ));
        assert!(needs_check(Some(&fresh), &cmds, now + RECHECK));
        assert!(needs_check(Some(&fresh), &["claude".to_string()], now));
        let running = AgentsFound {
            checked: None,
            running: true,
            ..fresh.clone()
        };
        assert!(!needs_check(Some(&running), &cmds, now + RECHECK * 2));
        // What the rows show: the answers for these commands, else checking.
        assert_eq!(found_for(Some(&fresh), &cmds), fresh.found);
        assert_eq!(found_for(None, &cmds), [Found::Checking, Found::Checking]);
        assert_eq!(
            found_for(Some(&fresh), &["codex".to_string()]),
            [Found::Checking]
        );
    }
}
