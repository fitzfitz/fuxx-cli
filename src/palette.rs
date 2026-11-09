//! M11j: the ⌘K command palette, a panel near the top of the window: one box over every action,
//! workspace, file and setting. Chips under the box narrow it to one kind (Tab and ⇧Tab move
//! between them; on All, a query starting with `>` searches Actions); the list under them is
//! `palette_items::compose`'s, rebuilt on each keystroke and each time the Workspace draws (so
//! the workspaces' marks move and the order follows their statuses while it's open), the
//! selection kept on its item. ⌘P opens it on Workspaces: the old ⌘P list.
//!
//! The box is the sidebar's rename box (`NameInput`), as in the Help panel: its Enter does the
//! chosen row, its Escape (or losing focus, or a click outside) closes the palette. A pick
//! closes the palette, gives the keyboard back to the focused tab, and only then runs (an
//! action is dispatched from there, so it lands where its key would).

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use alacritty_terminal::vte::ansi::Rgb;
use gpui::{
    AnyElement, App, Bounds, ClickEvent, Context, Div, Entity, EventEmitter, FocusHandle,
    Focusable, FontWeight, HighlightStyle, MouseDownEvent, Pixels, ScrollHandle, StyledText,
    Subscription, Window, actions, canvas, div, prelude::*, px,
};

use crate::appearance::Appearance;
use crate::help_panel::render_group;
use crate::help_shortcuts::binding_pairs;
use crate::host_picker::move_selection;
use crate::layout_model::{FileMode, TabKind, WorkspaceId};
use crate::name_input::{NameInput, NameInputEvent};
use crate::overlay_panel;
use crate::palette_items::{
    ActionRow, COMMANDS, Files, Focused, HINT, Key, Line, Recent, Run, Scope, Sources, Step,
    action_rows, compose, more_label, palette_step, parse_query, selectable,
};
use crate::palette_match::byte_ranges;
use crate::preferences::PaletteSetting;
use crate::right_sidebar::{FileKind, kind_ink};
use crate::sidebar::{MARK_SLOT, tabular};
use crate::status::Status;
use crate::switcher::{Entry, ROW, ROW_GAP, workspace_row};
use crate::terminal_view::hsla;
use crate::theme::{Theme, UiColors, readable};
use crate::ui_tokens::{MARK_MIN, text};
use crate::workspace::{Overlay, Workspace};

// Bound in main.rs with the "Palette" key context; Enter and Escape are the box's own.
actions!(palette, [SelectPrev, SelectNext, NextScope, PrevScope]);

/// Wider than the Help panel: a row holds a name, a folder or keys.
const PANEL_WIDTH: f32 = 600.0;
/// A scope chip's height.
const CHIP: f32 = 22.0;
/// The list's tallest; it also keeps `LIST_BOTTOM` from the window's bottom, but is never
/// shorter than `LIST_MIN`.
const LIST_MAX: f32 = 420.0;
const LIST_MIN: f32 = 120.0;
const LIST_BOTTOM: f32 = 200.0;
/// The room between a row's icon slot and its name.
const SLOT_GAP: f32 = 8.0;

/// How tall the list may grow in a window `window_h` tall.
fn list_max_height(window_h: f32) -> f32 {
    (window_h - LIST_BOTTOM).clamp(LIST_MIN, LIST_MAX)
}

/// A file row's icon tint: the Files panel's (`right_sidebar::kind_ink`), pushed (hue kept) to
/// `MARK_MIN` on the palette's surfaces too.
#[allow(dead_code)] // drawn by the file rows (M11j task 5)
pub(crate) fn palette_file_ink(kind: FileKind, theme: &Theme) -> Rgb {
    let ui = theme.ui();
    readable(
        kind_ink(kind, theme, &ui),
        &[ui.panel, ui.panel_hover, ui.row_active],
        MARK_MIN,
    )
}

/// What the palette knows of the files (`palette_files`): a note in their place, a first
/// listing running, a failed one, or the listed paths (from `root`; `more`: how many past the
/// cap).
#[derive(Clone, Debug, PartialEq)]
pub enum FilesData {
    Note(&'static str),
    #[allow(dead_code)] // listed from M11j task 5
    Listing,
    #[allow(dead_code)] // listed from M11j task 5
    Failed,
    #[allow(dead_code)] // listed from M11j task 5
    Ready {
        root: PathBuf,
        paths: Arc<Vec<String>>,
        more: usize,
    },
}

impl FilesData {
    /// As `compose` takes it. The hits are the ranking's (M11j task 5): none yet.
    fn as_files(&self) -> Files<'_> {
        match self {
            FilesData::Note(note) => Files::Note(note),
            FilesData::Listing => Files::Listing,
            FilesData::Failed => Files::Failed,
            FilesData::Ready { root, more, .. } => Files::Ready {
                root,
                root_name: root.file_name().and_then(|n| n.to_str()).unwrap_or(""),
                hits: &[],
                total: 0,
                capped: *more > 0,
            },
        }
    }
}

/// What the palette lists from, as the Workspace has it now: the workspaces in display order,
/// the actions that apply where the focus was, the settings, the files, and the marks' clock.
pub struct PaletteData {
    pub workspaces: Vec<Entry>,
    pub actions: Vec<ActionRow>,
    pub settings: Vec<PaletteSetting>,
    pub files: FilesData,
    pub phase: f32,
}

pub enum PaletteEvent {
    /// Do this row (never a `… more` row: the palette handles those itself).
    Pick(Key),
    /// Escape, a click outside, or the box lost focus.
    Cancel,
}

/// The ⌘K panel.
pub struct Palette {
    input: Entity<NameInput>,
    /// The box's text as last listed.
    query: String,
    /// The chosen chip (a `>` query on All shows Actions: `scope`).
    base: Scope,
    data: PaletteData,
    /// The Workspace's, as it was when the palette opened (a pick closes it).
    recent: Recent,
    lines: Vec<Line>,
    /// The places in `lines` of the rows that can be chosen.
    rows: Vec<usize>,
    /// The chosen one, a place in `rows`.
    selected: usize,
    scroll: ScrollHandle,
    /// Where the panel and its chips were last drawn (the UI screenshots read them).
    panel_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    chip_bounds: Rc<RefCell<[Option<Bounds<Pixels>>; 5]>>,
    _input_events: Subscription,
    _input_changes: Subscription,
}

impl EventEmitter<PaletteEvent> for Palette {}

impl Focusable for Palette {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.focus_handle(cx)
    }
}

/// The key of a row that can be chosen.
fn line_key(line: &Line) -> Option<Key> {
    match line {
        Line::Item { key, .. } => Some(key.clone()),
        Line::More { kind, .. } => Some(Key::More(*kind)),
        Line::Header(_) | Line::Note(_) => None,
    }
}

impl Palette {
    pub fn new(
        scope: Scope,
        data: PaletteData,
        recent: Recent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| NameInput::new(String::new(), window, cx));
        let input_events = cx.subscribe_in(&input, window, |this, _, event, _, cx| match event {
            NameInputEvent::Confirm(_) => this.pick(this.selected, cx),
            NameInputEvent::Cancel => cx.emit(PaletteEvent::Cancel),
        });
        let input_changes = cx.observe(&input, |this, _, cx| this.query_changed(cx));
        let mut this = Self {
            input,
            query: String::new(),
            base: scope,
            data,
            recent,
            lines: Vec::new(),
            rows: Vec::new(),
            selected: 0,
            scroll: ScrollHandle::new(),
            panel_bounds: Rc::default(),
            chip_bounds: Rc::default(),
            _input_events: input_events,
            _input_changes: input_changes,
        };
        this.relist(false);
        this
    }

    /// The scope shown: the chosen chip's, or Actions for a `>` query on All.
    pub fn scope(&self) -> Scope {
        parse_query(self.base, &self.query).0
    }

    /// Show `scope` (a chip, Tab, ⌘K or ⌘P, a `… more` row), the text kept, the first row
    /// chosen.
    pub fn set_scope(&mut self, scope: Scope, cx: &mut Context<Self>) {
        self.base = scope;
        self.relist(false);
        cx.notify();
    }

    /// The sources as they are now (each time the Workspace draws). The chosen row stays on its
    /// item while it is still listed; else the first is chosen.
    pub fn refresh(&mut self, data: PaletteData) {
        let changed = data.workspaces != self.data.workspaces
            || data.actions != self.data.actions
            || data.settings != self.data.settings
            || data.files != self.data.files;
        self.data = data;
        if changed {
            self.relist(true);
        }
    }

    /// The workspaces listed, in order (for the marks' ticker and the age refresh).
    pub fn shown_workspaces(&self) -> impl Iterator<Item = &Entry> {
        self.lines.iter().filter_map(|line| match line {
            Line::Item {
                key: Key::Workspace(id),
                ..
            } => self.data.workspaces.iter().find(|e| e.id == *id),
            _ => None,
        })
    }

    /// The chosen row's key; none when nothing can be chosen.
    pub fn selected_key(&self) -> Option<&Key> {
        let line = self.lines.get(*self.rows.get(self.selected)?)?;
        match line {
            Line::Item { key, .. } => Some(key),
            // `line_key` makes a key of a `… more` row; here it has to be borrowed.
            Line::More { kind, .. } => MORE_KEYS.iter().find(|k| k.kind() == *kind),
            Line::Header(_) | Line::Note(_) => None,
        }
    }

    /// The box changed (typing, or the IME's composition): list again if its text did, from
    /// the first row.
    fn query_changed(&mut self, cx: &mut Context<Self>) {
        let query = self.input.read(cx).text().to_string();
        if query != self.query {
            self.query = query;
            self.relist(false);
            cx.notify();
        }
    }

    /// Rebuild the lines. `keep`: the chosen item stays chosen if it is still listed; else (or
    /// not `keep`) the first row is chosen and the list goes back to its top.
    fn relist(&mut self, keep: bool) {
        let before = self.selected_key().cloned().filter(|_| keep);
        let (scope, query) = parse_query(self.base, &self.query);
        // Every Recent key compose is given still stands for something it lists (M11j task 5
        // checks a file's against its listing).
        let valid = |_: &Key| true;
        self.lines = compose(&Sources {
            scope,
            query,
            workspaces: &self.data.workspaces,
            actions: &self.data.actions,
            settings: &self.data.settings,
            files: self.data.files.as_files(),
            recent: &self.recent,
            valid: &valid,
        });
        self.rows = selectable(&self.lines);
        let kept = before.and_then(|key| {
            self.rows
                .iter()
                .position(|&at| line_key(&self.lines[at]).as_ref() == Some(&key))
        });
        match kept {
            Some(at) => {
                if at != self.selected {
                    self.selected = at;
                    self.scroll.scroll_to_item(self.rows[at]);
                }
            }
            None => {
                self.selected = 0;
                self.scroll.scroll_to_item(0);
            }
        }
    }

    fn select_by(&mut self, delta: i32, cx: &mut Context<Self>) {
        self.selected = move_selection(self.selected, self.rows.len(), delta);
        if let Some(&at) = self.rows.get(self.selected) {
            self.scroll.scroll_to_item(at);
        }
        cx.notify();
    }

    /// Do row `at` (a place in `rows`; nothing if there is none, e.g. nothing matches): a `…
    /// more` row shows its kind's scope; anything else goes to the Workspace.
    fn pick(&mut self, at: usize, cx: &mut Context<Self>) {
        let Some(key) = self
            .rows
            .get(at)
            .and_then(|&line| line_key(&self.lines[line]))
        else {
            return;
        };
        match key {
            Key::More(kind) => self.set_scope(Scope::of(kind), cx),
            key => cx.emit(PaletteEvent::Pick(key)),
        }
    }

    /// The name of an item's row and where its matched letters are.
    fn item_name(&self, key: &Key) -> String {
        match key {
            Key::Workspace(id) => self
                .data
                .workspaces
                .iter()
                .find(|e| e.id == *id)
                .map(|e| e.label.clone())
                .unwrap_or_default(),
            Key::Action(_) | Key::Preset(_) => self
                .data
                .actions
                .iter()
                .find(|r| r.key == *key)
                .map(|r| r.name.clone())
                .unwrap_or_default(),
            Key::Setting(name) => name.to_string(),
            Key::File(path) => path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            Key::More(_) => String::new(),
        }
    }

    /// A row of the list: its line (`at`), chosen or not.
    fn render_line(
        &self,
        at: usize,
        line: &Line,
        chosen: bool,
        ui: &UiColors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let row = match line {
            Line::Header(title) => return render_group(title.clone(), at == 0, ui),
            Line::Note(note) => {
                return div()
                    .flex_none()
                    .px(px(8.0))
                    .py(px(4.0))
                    .text_size(px(text::NAME))
                    .text_color(hsla(ui.text_dim))
                    .child(note.clone())
                    .into_any_element();
            }
            Line::More { kind, count } => item_row(chosen, ui).child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .text_color(hsla(ui.text_dim))
                    .child(more_label(*kind, *count)),
            ),
            Line::Item {
                key: Key::Workspace(id),
                positions,
            } => {
                let Some((place, entry)) = self
                    .data
                    .workspaces
                    .iter()
                    .enumerate()
                    .find(|(_, e)| e.id == *id)
                else {
                    return div().into_any_element();
                };
                let highlights = byte_ranges(&entry.label, positions);
                workspace_row(entry, place + 1, chosen, &highlights, self.data.phase, ui)
            }
            Line::Item { key, positions } => {
                let name = self.item_name(key);
                let right = match key {
                    Key::Action(_) | Key::Preset(_) => self
                        .data
                        .actions
                        .iter()
                        .find(|r| r.key == *key)
                        .and_then(|r| r.keys.clone())
                        .map(|keys| (keys, ui.text_dim)),
                    Key::Setting(name) => self
                        .data
                        .settings
                        .iter()
                        .find(|s| s.name == *name)
                        .map(|s| (s.section.title().to_string(), ui.text_faint)),
                    _ => None,
                };
                item_row(chosen, ui)
                    .child(name_cell(&name, positions, chosen))
                    .when_some(right, |row, (right, color)| {
                        row.child(
                            div()
                                .flex_none()
                                .ml(px(8.0))
                                .text_size(px(text::META))
                                .font_features(tabular())
                                .text_color(hsla(color))
                                .child(right),
                        )
                    })
            }
        };
        let Some(place) = self.rows.iter().position(|&l| l == at) else {
            return row.into_any_element();
        };
        row.id(("palette-row", at))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.selected = place;
                this.pick(place, cx);
            }))
            .into_any_element()
    }

    /// A scope's chip: filled `row_active` while it's the scope shown; a click shows it.
    fn render_chip(&self, scope: Scope, cx: &mut Context<Self>, ui: &UiColors) -> AnyElement {
        let current = scope == self.scope();
        let hover = ui.panel_hover;
        let at = Scope::ALL.iter().position(|s| *s == scope).unwrap_or(0);
        let bounds = self.chip_bounds.clone();
        div()
            .id(("palette-chip", at))
            .relative()
            .flex_none()
            .flex()
            .items_center()
            .h(px(CHIP))
            .px(px(10.0))
            .rounded(px(5.0))
            .text_size(px(text::META))
            .when(current, |chip| {
                chip.bg(hsla(ui.row_active)).text_color(hsla(ui.text))
            })
            .when(!current, |chip| {
                chip.text_color(hsla(ui.text_dim))
                    .hover(move |s| s.bg(hsla(hover)))
            })
            .cursor_pointer()
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.set_scope(scope, cx)))
            .child(scope.label())
            .child(
                canvas(
                    move |b, _, _| bounds.borrow_mut()[at] = Some(b),
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .into_any_element()
    }
}

/// A `… more` row's key, to lend out (`Palette::selected_key`).
static MORE_KEYS: [Key; 4] = [
    Key::More(crate::palette_items::Kind::Workspace),
    Key::More(crate::palette_items::Kind::Action),
    Key::More(crate::palette_items::Kind::File),
    Key::More(crate::palette_items::Kind::Setting),
];

/// An item's row (not a workspace's): `ROW` tall, an empty icon slot so the names line up,
/// filled `row_active` while chosen, `panel_hover` under the pointer.
fn item_row(chosen: bool, ui: &UiColors) -> Div {
    let hover = ui.panel_hover;
    div()
        .flex_none()
        .flex()
        .items_center()
        .h(px(ROW))
        .px(px(8.0))
        .rounded(px(6.0))
        .text_size(px(text::NAME))
        .when(chosen, |row| row.bg(hsla(ui.row_active)))
        .when(!chosen, |row| row.hover(move |s| s.bg(hsla(hover))))
        .child(div().flex_none().w(px(MARK_SLOT)).mr(px(SLOT_GAP)))
}

/// A row's name, cut at its end, the matched letters (char `positions`) semibold; medium while
/// chosen.
fn name_cell(name: &str, positions: &[usize], chosen: bool) -> Div {
    let matched = HighlightStyle {
        font_weight: Some(FontWeight::SEMIBOLD),
        ..Default::default()
    };
    let ranges = byte_ranges(name, positions);
    div()
        .flex_1()
        .min_w(px(0.0))
        .truncate()
        .when(chosen, |cell| cell.font_weight(FontWeight::MEDIUM))
        .child(
            StyledText::new(name.to_string())
                .with_highlights(ranges.into_iter().map(|r| (r, matched))),
        )
}

/// The UI screenshots' look inside.
#[cfg(feature = "ui-shots")]
impl Palette {
    pub fn lines(&self) -> &[Line] {
        &self.lines
    }

    /// Where line `at` was last drawn.
    pub fn row_bounds(&self, at: usize) -> Option<Bounds<Pixels>> {
        self.scroll.bounds_for_item(at)
    }

    pub fn panel_bounds(&self) -> Option<Bounds<Pixels>> {
        self.panel_bounds.get()
    }

    pub fn chip_bounds(&self, scope: Scope) -> Option<Bounds<Pixels>> {
        let at = Scope::ALL.iter().position(|s| *s == scope)?;
        self.chip_bounds.borrow()[at]
    }

    /// The keys an action's row shows (by its name).
    pub fn action_keys(&self, name: &str) -> Option<String> {
        self.data
            .actions
            .iter()
            .find(|r| r.name == name)
            .and_then(|r| r.keys.clone())
    }
}

impl Render for Palette {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = cx.global::<Appearance>().theme.ui();
        let chosen_line = self.rows.get(self.selected).copied();
        let rows: Vec<AnyElement> = self
            .lines
            .iter()
            .enumerate()
            .map(|(at, line)| self.render_line(at, line, chosen_line == Some(at), &ui, cx))
            .collect();
        let list = if rows.is_empty() {
            div()
                .px(px(8.0))
                .py(px(4.0))
                .text_size(px(text::NAME))
                .text_color(hsla(ui.text_dim))
                .child(crate::palette_items::NOTHING)
                .into_any_element()
        } else {
            let max_h = list_max_height(f32::from(window.viewport_size().height));
            div()
                .id("palette-rows")
                .flex()
                .flex_col()
                .gap(px(ROW_GAP))
                .max_h(px(max_h))
                .overflow_y_scroll()
                .track_scroll(&self.scroll)
                .children(rows)
                .into_any_element()
        };
        let chips: Vec<AnyElement> = Scope::ALL
            .iter()
            .map(|&scope| self.render_chip(scope, cx, &ui))
            .collect();

        let empty = self.input.read(cx).text().is_empty();
        let field = overlay_panel::filter_box(&ui, &self.input, empty, self.base.placeholder());
        let bounds = self.panel_bounds.clone();

        overlay_panel::shell(&ui, PANEL_WIDTH)
            .relative()
            .key_context("Palette")
            .on_action(cx.listener(|this, _: &SelectPrev, _, cx| this.select_by(-1, cx)))
            .on_action(cx.listener(|this, _: &SelectNext, _, cx| this.select_by(1, cx)))
            .on_action(cx.listener(|this, _: &NextScope, _, cx| {
                let next = this.scope().next();
                this.set_scope(next, cx)
            }))
            .on_action(cx.listener(|this, _: &PrevScope, _, cx| {
                let prev = this.scope().prev();
                this.set_scope(prev, cx)
            }))
            // The terminal under the panel gets neither its clicks nor its hover.
            .occlude()
            .on_mouse_down_out(
                cx.listener(|_, _: &MouseDownEvent, _, cx| cx.emit(PaletteEvent::Cancel)),
            )
            .child(field)
            .child(div().flex().gap(px(4.0)).m(px(2.0)).children(chips))
            .child(list)
            .child(
                div()
                    .px(px(8.0))
                    .pt(px(5.0))
                    .border_t_1()
                    .border_color(hsla(ui.panel_border))
                    .text_size(px(text::META))
                    .text_color(hsla(ui.text_faint))
                    .child(HINT),
            )
            .child(
                canvas(move |b, _, _| bounds.set(Some(b)), |_, _, _, _| {})
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
            )
    }
}

/// The keys' table as `main` binds it (persistent sessions' ⌥⌘Q only while they're on): where
/// an action row's keys come from, as the Help list's.
fn binding_table(persistent: bool) -> Vec<(String, String)> {
    let mut bindings = crate::new_bindings().to_vec();
    bindings.extend(crate::key_bindings());
    if persistent {
        bindings.extend(crate::persistent_bindings());
    }
    binding_pairs(&bindings)
}

impl Workspace {
    /// The kind of tab that has the focus (what the palette's actions apply to).
    fn focused_kind(&self) -> Focused {
        let tab = self.model.focused_tab().and_then(|t| self.model.tab(t));
        match tab.map(|t| &t.kind) {
            Some(TabKind::Terminal(_)) => Focused::Terminal,
            Some(TabKind::Browser { .. }) => Focused::Browser,
            Some(TabKind::File { .. }) => Focused::File,
            None => Focused::Nothing,
        }
    }

    /// The palette's sources now; `actions` as they were worked out when it opened (they follow
    /// the focus then, and the palette has it since).
    fn palette_data(&self, actions: Vec<ActionRow>, cx: &App) -> PaletteData {
        PaletteData {
            workspaces: self.switcher_entries(cx),
            actions,
            // The settings and the files are listed from M11j task 5.
            settings: Vec::new(),
            files: FilesData::Note(crate::palette_items::NO_FOLDER),
            phase: self.phase,
        }
    }

    /// ⌘K (`All`), ⌘P (`Workspaces`) or the row menu's Show All Workspaces: open the palette on
    /// `scope`; open on it already, close it; open on another, show `scope`, the text kept
    /// (`palette_step`). Any other box over the window closes first.
    pub(crate) fn open_palette(
        &mut self,
        scope: Scope,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let open = self.palette.as_ref().map(|p| p.read(cx).base);
        match palette_step(open, scope) {
            Step::Close => {
                self.palette = None;
                self.focus_active(window, cx);
            }
            Step::Rescope => {
                if let Some(palette) = self.palette.clone() {
                    palette.update(cx, |p, cx| p.set_scope(scope, cx));
                    palette.focus_handle(cx).focus(window, cx);
                }
            }
            Step::Open => {
                self.close_overlays(Overlay::Palette);
                let presets: Vec<String> = cx
                    .global::<Appearance>()
                    .presets
                    .iter()
                    .map(|p| p.name.clone())
                    .collect();
                let actions = action_rows(
                    self.focused_kind(),
                    self.persistent,
                    &presets,
                    &binding_table(self.persistent),
                );
                let data = self.palette_data(actions, cx);
                let recent = self.palette_recent.clone();
                let palette = cx.new(|cx| Palette::new(scope, data, recent, window, cx));
                cx.subscribe_in(&palette, window, Self::on_palette_event)
                    .detach();
                palette.focus_handle(cx).focus(window, cx);
                self.palette = Some(palette);
            }
        }
        cx.notify();
    }

    /// The palette picked a row or was closed.
    fn on_palette_event(
        &mut self,
        palette: &Entity<Palette>,
        event: &PaletteEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A palette already closed or replaced can still report losing focus: ignore it.
        if self.palette.as_ref() != Some(palette) {
            return;
        }
        match event {
            PaletteEvent::Pick(key) => self.run_pick(key.clone(), window, cx),
            PaletteEvent::Cancel => {
                self.palette = None;
                self.focus_active(window, cx);
                cx.notify();
            }
        }
    }

    /// Do a pick: the palette closes and the keyboard goes back to the focused tab, then the
    /// pick runs from there (an action is dispatched as its key would be, so it lands where the
    /// key would); it goes to the front of Recent. A `… more` row shows its scope instead, the
    /// palette left open.
    pub(crate) fn run_pick(&mut self, key: Key, window: &mut Window, cx: &mut Context<Self>) {
        if let Key::More(kind) = key {
            if let Some(palette) = &self.palette {
                palette.update(cx, |p, cx| p.set_scope(Scope::of(kind), cx));
            }
            return;
        }
        self.palette = None;
        self.focus_active(window, cx);
        self.palette_recent.push(key.clone());
        match key {
            Key::Action(name) => match COMMANDS.iter().find(|c| c.name == name).map(|c| c.run) {
                Some(Run::Action(make)) => window.dispatch_action(make(), cx),
                Some(Run::NewBrowserTab) => {
                    if let Some(pane) = self.model.focused_pane() {
                        self.new_browser_tab(pane, window, cx);
                    }
                }
                None => {}
            },
            Key::Preset(n) => self.launch_preset(n, window, cx),
            // As ⌘P always did: its most urgent terminal (its pane and tab shown), else the
            // workspace as it was (one of browser or file tabs only).
            Key::Workspace(id) => {
                let ws = WorkspaceId(id);
                match self.rollup_session(ws) {
                    Some(index) => self.select(index, window, cx),
                    None => self.select_workspace(ws, window, cx),
                }
                self.focus_active(window, cx);
            }
            Key::File(path) => self.open_file_tab(path, FileMode::Plain, window, cx),
            // Preferences on that setting's section: M11j task 5.
            Key::Setting(_) => {}
            Key::More(_) => {}
        }
        cx.notify();
    }

    /// Give the open palette the sources as they are now (each time the Workspace draws).
    pub(crate) fn refresh_palette(&self, cx: &mut Context<Self>) {
        if let Some(palette) = &self.palette {
            let actions = palette.read(cx).data.actions.clone();
            let data = self.palette_data(actions, cx);
            palette.update(cx, |p, _| p.refresh(data));
        }
    }

    /// The open palette's workspace rows (each one's status and since when); none while it's
    /// closed.
    pub(crate) fn palette_shown(&self, cx: &App) -> Vec<(Status, Instant)> {
        self.palette
            .as_ref()
            .map(|p| {
                p.read(cx)
                    .shown_workspaces()
                    .map(|e| (e.status, e.since))
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::every_theme;
    use crate::ui_tokens::{TEXT_MIN, contrast};

    #[test]
    fn palette_inks_meet_the_floors() {
        let mut failures = Vec::new();
        for (name, theme) in every_theme() {
            let ui = theme.ui();
            let surfaces = [
                ("panel", ui.panel),
                ("panel_hover", ui.panel_hover),
                ("row_active", ui.row_active),
            ];
            for (bg_name, bg) in surfaces {
                for (fg_name, fg) in [
                    ("text", ui.text),
                    ("text_dim", ui.text_dim),
                    ("text_faint", ui.text_faint),
                ] {
                    let c = contrast(fg, bg);
                    if c < TEXT_MIN {
                        failures.push(format!("{name}: {fg_name} on {bg_name} {c:.2}"));
                    }
                }
                for kind in FileKind::ALL {
                    let c = contrast(palette_file_ink(kind, &theme), bg);
                    if c < MARK_MIN {
                        failures.push(format!("{name}: {kind:?} ink on {bg_name} {c:.2}"));
                    }
                }
            }
        }
        assert!(failures.is_empty(), "{failures:#?}");
    }

    #[test]
    fn the_list_is_at_most_420_and_keeps_200_from_the_bottom() {
        assert_eq!(list_max_height(1000.0), 420.0);
        assert_eq!(list_max_height(600.0), 400.0);
        assert_eq!(list_max_height(300.0), 120.0);
        assert_eq!(list_max_height(100.0), 120.0);
    }
}
