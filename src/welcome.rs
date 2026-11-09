//! M11k: the Welcome card, a centred card over a scrim on a first run (and from Help → Welcome
//! to fuxx): the folder to start in, one row per agent preset (marked by whether its program
//! is on your `PATH`) and Plain terminal, and three shortcuts. Its rules and words are
//! `welcome_items`'; the `PATH` check is `agent_detect`'s, run off the UI thread by
//! `Workspace::open_welcome` and handed to the card when it answers (it only changes labels,
//! and the choice while you haven't moved it).
//!
//! The card reads ↑, ↓, Return and Esc itself (`on_key_down` on its focused root), so nothing
//! joins the key bindings; every other key works as anywhere (⌘K opens the palette and closes
//! the card). It closes on Esc, Skip, a click on the scrim, a pick or another overlay, never
//! on losing the keyboard (the folder panel takes it while it's up). Nothing in it animates.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

#[cfg(feature = "ui-shots")]
use futures::channel::oneshot;
use gpui::{
    AnyElement, App, Bounds, ClickEvent, Context, Div, Entity, EventEmitter, FocusHandle,
    Focusable, FontWeight, Hsla, KeyDownEvent, MouseButton, MouseDownEvent, Pixels, ScrollHandle,
    SharedString, Window, canvas, div, prelude::*, px,
};

use crate::agent_detect::{self, Found};
use crate::appearance::Appearance;
use crate::help_shortcuts::keys_for;
use crate::overlay_panel;
use crate::palette::binding_table;
use crate::presets::{MAX_PRESETS, Preset};
use crate::sidebar::{home_relative, tabular};
use crate::terminal_view::hsla;
use crate::theme::UiColors;
use crate::ui_tokens::size::TOP_BAND;
use crate::ui_tokens::text;
use crate::welcome_items::{
    CHOOSE, Choice, Effect, HEADING_AGENT, HEADING_FOLDER, HEADING_SHORTCUTS, HINT, NO_PRESETS,
    Origin, PLAIN_TERMINAL, Row, SHORTCUTS, SKIP, SUBTITLE, TITLE, default_row, pick_effect, rows,
    start_label, start_launch, status_word, step,
};
use crate::workspace::{At, Opened, Overlay, Workspace, folder_panel};

/// The card's width (less in a narrow window) and its inner padding.
const CARD_WIDTH: f32 = 520.0;
const CARD_PADDING: f32 = 20.0;
/// How far the card keeps from the window's sides and bottom (and, centred, the top band).
const EDGE: f32 = 16.0;
/// The scrim: the theme's `shadow` at this alpha, below the top band.
const SCRIM_ALPHA: f32 = 0.35;
/// The title's size and line.
const TITLE_SIZE: f32 = 18.0;
const TITLE_LINE: f32 = 22.0;
/// The subtitle's line.
const SUBTITLE_LINE: f32 = 18.0;
/// A heading's line, and the room above and below it.
const HEADING_LINE: f32 = 14.0;
const HEADING_ABOVE: f32 = 14.0;
const HEADING_BELOW: f32 = 6.0;
/// A choosable row (the folder field too), and the room between agent rows.
const ROW: f32 = 28.0;
const ROW_GAP: f32 = 2.0;
/// The Choose… button's height.
const CHOOSE_HEIGHT: f32 = 24.0;
/// A keycap: its height, least width, side padding, corner and text size; the room between the
/// widest cap and the lines, and between the shortcut lines.
const KEYCAP: f32 = 22.0;
const KEYCAP_MIN: f32 = 28.0;
const KEYCAP_PADDING: f32 = 6.0;
const KEYCAP_RADIUS: f32 = 5.0;
const KEYCAP_TEXT: f32 = 12.0;
const CAP_GAP: f32 = 10.0;
const SHORTCUT_GAP: f32 = 2.0;
/// The footer: the room above its line and between the line and the buttons.
const FOOTER_ABOVE: f32 = 12.0;
const FOOTER_BELOW: f32 = 10.0;
/// The footer's buttons.
const BUTTON: f32 = 28.0;
const BUTTON_PADDING: f32 = 12.0;
const RADIUS: f32 = 6.0;

/// What the card asks of the Workspace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WelcomeEvent {
    /// Start `choice` in `folder` (Return on an agent row, or the Start button).
    Start { folder: PathBuf, choice: Choice },
    /// Esc or the Skip button.
    Skip,
}

/// The keys the card shows, read from the key-binding table: each preset's (⌃⌘n, none past
/// what is bound) and the three shortcuts' (empty if unbound).
pub struct WelcomeKeys {
    pub presets: Vec<Option<String>>,
    pub shortcuts: [String; 3],
}

impl WelcomeKeys {
    /// The keys for `presets` presets and `SHORTCUTS` from `table` (`palette::binding_table`,
    /// the one Help and the palette read).
    pub fn from_table(table: &[(String, String)], presets: usize) -> Self {
        Self {
            presets: (1..=presets)
                .map(|n| keys_for(table, &format!("workspace::LaunchPreset{n}")))
                .collect(),
            shortcuts: SHORTCUTS.map(|(action, _)| keys_for(table, action).unwrap_or_default()),
        }
    }
}

/// The folder the card starts on: home on a first run; from Help, the active terminal's folder
/// (but home over an SSH terminal: the welcome starts local work), else home; `/` with no home.
fn default_folder(
    origin: Origin,
    active: Option<PathBuf>,
    ssh: bool,
    home: Option<&Path>,
) -> PathBuf {
    let home = || home.map_or_else(|| PathBuf::from("/"), Path::to_path_buf);
    match origin {
        Origin::FirstRun { .. } => home(),
        Origin::Help if ssh => home(),
        Origin::Help => active.unwrap_or_else(home),
    }
}

/// Each string the card draws, in order, as it draws them (kept only for the UI screenshots,
/// which read them).
#[derive(Default)]
struct Said {
    #[cfg(feature = "ui-shots")]
    drawn: Vec<String>,
}

impl Said {
    fn say(&mut self, s: impl Into<String>) -> SharedString {
        let s = s.into();
        #[cfg(feature = "ui-shots")]
        self.drawn.push(s.clone());
        s.into()
    }
}

/// The Welcome card.
pub struct Welcome {
    origin: Origin,
    folder: PathBuf,
    /// Your home folder, for writing the folder as the sidebar does (`~/…`).
    home: Option<PathBuf>,
    /// The presets as they were when the card opened.
    presets: Vec<Preset>,
    keys: WelcomeKeys,
    /// Detection's answer for each preset (`Checking` until it lands).
    found: Vec<Found>,
    rows: Vec<Row>,
    /// The row ↑/↓ and a click move.
    chosen: Row,
    /// What Start starts: the chosen agent row, or the one chosen last while the folder row is.
    agent: Row,
    /// The choice was moved (↑/↓ or a click): detection's answer then leaves it.
    moved: bool,
    /// The folder panel is up (asking again meanwhile does nothing).
    panel_open: bool,
    focus: FocusHandle,
    /// The agent list's (only it scrolls).
    scroll: ScrollHandle,
    /// Where the card, the folder field, the Start button and Choose… were last drawn.
    card_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    folder_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    start_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    choose_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    #[cfg(feature = "ui-shots")]
    drawn: Vec<String>,
    /// The UI screenshots' stand-in for the folder panel: its answer, once sent.
    #[cfg(feature = "ui-shots")]
    panel_answer: Option<oneshot::Receiver<Option<PathBuf>>>,
}

impl EventEmitter<WelcomeEvent> for Welcome {}

impl Focusable for Welcome {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Welcome {
    /// A card for `presets` (at most `MAX_PRESETS` are shown) in `folder`, every preset
    /// `Checking` and the choice `default_row`'s for that.
    pub fn new(
        origin: Origin,
        folder: PathBuf,
        home: Option<PathBuf>,
        mut presets: Vec<Preset>,
        keys: WelcomeKeys,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        presets.truncate(MAX_PRESETS);
        let found = vec![Found::Checking; presets.len()];
        let chosen = default_row(&found);
        Self {
            origin,
            folder,
            home,
            rows: rows(presets.len()),
            presets,
            keys,
            found,
            chosen,
            agent: chosen,
            moved: false,
            panel_open: false,
            focus: cx.focus_handle(),
            scroll: ScrollHandle::new(),
            card_bounds: Rc::default(),
            folder_bounds: Rc::default(),
            start_bounds: Rc::default(),
            choose_bounds: Rc::default(),
            #[cfg(feature = "ui-shots")]
            drawn: Vec::new(),
            #[cfg(feature = "ui-shots")]
            panel_answer: None,
        }
    }

    /// Detection answered (one per preset): the labels change, and the choice goes to
    /// `default_row` unless it was moved. An answer for another number of presets is dropped.
    pub fn set_found(&mut self, found: Vec<Found>, cx: &mut Context<Self>) {
        if found.len() != self.presets.len() {
            return;
        }
        self.found = found;
        if !self.moved {
            self.choose(default_row(&self.found));
        }
        cx.notify();
    }

    /// Start in `folder` from now on.
    pub fn set_folder(&mut self, folder: PathBuf, cx: &mut Context<Self>) {
        self.folder = folder;
        cx.notify();
    }

    /// Why the card is open.
    pub fn origin(&self) -> Origin {
        self.origin
    }

    /// A click on the folder field or Choose…, or Return on the folder row: the folder panel
    /// (⌘O's), which has the keyboard while it's up; one at a time. A folder picked becomes the
    /// card's; picked or not, the card (if it is still open) takes the keyboard back with the
    /// agent row chosen again.
    pub fn choose_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.panel_open {
            return;
        }
        self.panel_open = true;
        #[cfg(feature = "ui-shots")]
        if let Some(answer) = self.panel_answer.take() {
            self.when_answered(async move { answer.await.ok().flatten() }, window, cx);
            return;
        }
        let picked = cx.prompt_for_paths(folder_panel());
        let answer = async move {
            match picked.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                Ok(Ok(None)) | Err(_) => None,
                Ok(Err(err)) => {
                    eprintln!("[fuxx] welcome: folder picker failed: {err}");
                    None
                }
            }
        };
        self.when_answered(answer, window, cx);
    }

    /// Once the folder panel answers: the folder, the agent row and the keyboard back. A card
    /// closed meanwhile is gone (only the Workspace holds it) and the answer is dropped.
    fn when_answered(
        &mut self,
        answer: impl Future<Output = Option<PathBuf>> + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.spawn_in(window, async move |this, cx| {
            let folder = answer.await;
            this.update_in(cx, |card, window, cx| {
                card.panel_open = false;
                if let Some(folder) = folder {
                    card.set_folder(folder, cx);
                }
                card.choose(card.agent);
                card.focus.focus(window, cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Choose `row`; an agent row is also what Start starts. The list scrolls it into view.
    fn choose(&mut self, row: Row) {
        self.chosen = row;
        if row != Row::Folder {
            self.agent = row;
        }
        if let Some(at) = self.list_index(row) {
            self.scroll.scroll_to_item(at);
        }
    }

    /// Choose `row` by a key or a click: detection leaves it from now on.
    fn move_to(&mut self, row: Row, cx: &mut Context<Self>) {
        self.moved = true;
        self.choose(row);
        cx.notify();
    }

    /// Where `row` is in the agent list (the folder row isn't in it).
    fn list_index(&self, row: Row) -> Option<usize> {
        match row {
            Row::Folder => None,
            Row::Preset(i) => Some(i),
            Row::Plain => Some(self.presets.len()),
        }
    }

    /// What Start starts.
    fn choice(&self) -> Choice {
        match self.agent {
            Row::Preset(i) => self
                .presets
                .get(i)
                .cloned()
                .map_or(Choice::Plain, Choice::Preset),
            Row::Folder | Row::Plain => Choice::Plain,
        }
    }

    fn start(&mut self, cx: &mut Context<Self>) {
        cx.emit(WelcomeEvent::Start {
            folder: self.folder.clone(),
            choice: self.choice(),
        });
    }

    /// ↑ and ↓ move round the rows; Return starts the chosen agent row (on the folder row it
    /// opens the folder panel); Esc skips. Only without modifiers; every other key goes on.
    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.modifiers.modified() {
            return;
        }
        match event.keystroke.key.as_str() {
            "down" | "up" => {
                let down = event.keystroke.key == "down";
                let next = step(&self.rows, self.chosen, down);
                self.move_to(next, cx);
            }
            "enter" => {
                if self.chosen == Row::Folder {
                    self.choose_folder(window, cx);
                } else {
                    self.start(cx);
                }
            }
            "escape" => cx.emit(WelcomeEvent::Skip),
            _ => return,
        }
        cx.stop_propagation();
    }

    /// An agent row (a preset's or Plain terminal's): its name, its status word, its key at the
    /// right; filled `row_active` while chosen, `panel_hover` under the pointer; a click
    /// chooses it.
    fn agent_row(
        &self,
        row: Row,
        name: SharedString,
        status: Option<SharedString>,
        keys: Option<SharedString>,
        ui: &UiColors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let chosen = self.chosen == row;
        let hover = ui.panel_hover;
        let at = self.list_index(row).unwrap_or(0);
        div()
            .id(("welcome-row", at))
            .flex_none()
            .flex()
            .items_center()
            .h(px(ROW))
            .px(px(8.0))
            .rounded(px(RADIUS))
            .text_size(px(text::NAME))
            .text_color(hsla(ui.text))
            .when(chosen, |r| r.bg(hsla(ui.row_active)))
            .when(!chosen, |r| r.hover(move |s| s.bg(hsla(hover))))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.move_to(row, cx)))
            .child(
                div()
                    .flex_none()
                    .whitespace_nowrap()
                    .when(chosen, |n| n.font_weight(FontWeight::MEDIUM))
                    .child(name),
            )
            .when_some(status, |r, status| {
                r.child(
                    div()
                        .min_w(px(0.0))
                        .ml(px(8.0))
                        .truncate()
                        .text_size(px(text::META))
                        .text_color(hsla(ui.text_faint))
                        .child(status),
                )
            })
            .child(div().flex_1())
            .when_some(keys, |r, keys| {
                r.child(
                    div()
                        .flex_none()
                        .ml(px(8.0))
                        .text_size(px(text::META))
                        .font_features(tabular())
                        .text_color(hsla(ui.text_dim))
                        .child(keys),
                )
            })
            .into_any_element()
    }
}

/// A section's heading: small, faint, upper case.
fn heading(title: SharedString, ui: &UiColors) -> Div {
    div()
        .flex_none()
        .mt(px(HEADING_ABOVE))
        .mb(px(HEADING_BELOW))
        .text_size(px(text::META))
        .line_height(px(HEADING_LINE))
        .text_color(hsla(ui.text_faint))
        .child(title)
}

/// A keycap: the keys in a small raised box.
fn keycap(keys: SharedString, ui: &UiColors) -> Div {
    div()
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .h(px(KEYCAP))
        .min_w(px(KEYCAP_MIN))
        .px(px(KEYCAP_PADDING))
        .rounded(px(KEYCAP_RADIUS))
        .bg(hsla(ui.panel_hover))
        .border_1()
        .border_color(hsla(ui.panel_border))
        .text_size(px(KEYCAP_TEXT))
        .font_features(tabular())
        .text_color(hsla(ui.text))
        .child(keys)
}

/// Fills its parent and notes where it was laid out in `cell`.
fn bounds_into(cell: &Rc<Cell<Option<Bounds<Pixels>>>>) -> impl IntoElement {
    let cell = cell.clone();
    canvas(move |b, _, _| cell.set(Some(b)), |_, _, _, _| {})
        .absolute()
        .top_0()
        .left_0()
        .size_full()
}

/// The UI screenshots' look inside.
#[cfg(feature = "ui-shots")]
impl Welcome {
    pub fn chosen(&self) -> Row {
        self.chosen
    }

    pub fn folder(&self) -> &Path {
        &self.folder
    }

    pub fn found(&self) -> &[Found] {
        &self.found
    }

    /// The next folder panel is answered by `answer` instead of the real one.
    pub fn answer_panel_with(&mut self, answer: oneshot::Receiver<Option<PathBuf>>) {
        self.panel_answer = Some(answer);
    }

    /// Each string drawn last, in order.
    pub fn texts(&self) -> Vec<String> {
        self.drawn.clone()
    }

    pub fn card_bounds(&self) -> Option<Bounds<Pixels>> {
        self.card_bounds.get()
    }

    /// Where `row` was last drawn, the list's scroll applied (the folder row: its field).
    pub fn row_bounds(&self, row: Row) -> Option<Bounds<Pixels>> {
        match self.list_index(row) {
            Some(at) => {
                let b = self.scroll.bounds_for_item(at)?;
                Some(Bounds::new(b.origin + self.scroll.offset(), b.size))
            }
            None => self.folder_bounds.get(),
        }
    }

    /// The agent list's visible part.
    pub fn list_bounds(&self) -> Option<Bounds<Pixels>> {
        Some(self.scroll.bounds())
    }

    pub fn start_bounds(&self) -> Option<Bounds<Pixels>> {
        self.start_bounds.get()
    }

    pub fn choose_bounds(&self) -> Option<Bounds<Pixels>> {
        self.choose_bounds.get()
    }

    pub fn folder_bounds(&self) -> Option<Bounds<Pixels>> {
        self.folder_bounds.get()
    }
}

impl Render for Welcome {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = cx.global::<Appearance>().theme.ui();
        let mut said = Said::default();
        // Never taller than the window less the top band and the bottom edge.
        let max_h = (f32::from(window.viewport_size().height) - TOP_BAND - EDGE).max(0.0);

        let title = said.say(TITLE);
        let subtitle = said.say(SUBTITLE);

        let folder_heading = said.say(HEADING_FOLDER.to_uppercase());
        let folder_text = said.say(home_relative(&self.folder, self.home.as_deref()));
        let choose = said.say(CHOOSE);
        let folder_chosen = self.chosen == Row::Folder;
        let field_edge = if folder_chosen {
            ui.focus_ring
        } else {
            ui.panel_hover
        };
        let folder_row = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(
                div()
                    .id("welcome-folder-field")
                    .relative()
                    .flex_1()
                    .min_w(px(0.0))
                    .h(px(ROW))
                    .flex()
                    .items_center()
                    .px(px(8.0))
                    .rounded(px(RADIUS))
                    .bg(hsla(ui.panel_hover))
                    .border_1()
                    .border_color(hsla(field_edge))
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.choose_folder(window, cx)
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis_start()
                            .child(folder_text),
                    )
                    .child(bounds_into(&self.folder_bounds)),
            )
            .child({
                let hover = ui.panel_hover;
                div()
                    .id("welcome-choose")
                    .relative()
                    .flex_none()
                    .flex()
                    .items_center()
                    .h(px(CHOOSE_HEIGHT))
                    .px(px(10.0))
                    .rounded(px(RADIUS))
                    .border_1()
                    .border_color(hsla(ui.panel_border))
                    .hover(move |s| s.bg(hsla(hover)))
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.choose_folder(window, cx)
                    }))
                    .child(choose)
                    .child(bounds_into(&self.choose_bounds))
            });

        let agent_heading = said.say(HEADING_AGENT.to_uppercase());
        let note = self.presets.is_empty().then(|| said.say(NO_PRESETS));
        let mut agents: Vec<AnyElement> = Vec::new();
        for (i, preset) in self.presets.iter().enumerate() {
            let name = said.say(preset.name.clone());
            let status = self
                .found
                .get(i)
                .and_then(|f| status_word(*f))
                .map(|w| said.say(w));
            let keys = self
                .keys
                .presets
                .get(i)
                .cloned()
                .flatten()
                .map(|k| said.say(k));
            agents.push(self.agent_row(Row::Preset(i), name, status, keys, &ui, cx));
        }
        let plain = said.say(PLAIN_TERMINAL);
        agents.push(self.agent_row(Row::Plain, plain, None, None, &ui, cx));

        let shortcuts_heading = said.say(HEADING_SHORTCUTS.to_uppercase());
        let mut caps = Vec::new();
        let mut lines = Vec::new();
        for (keys, (_, line)) in self.keys.shortcuts.iter().zip(SHORTCUTS) {
            caps.push(keycap(said.say(keys.clone()), &ui));
            lines.push(
                div()
                    .flex_none()
                    .h(px(KEYCAP))
                    .flex()
                    .items_center()
                    .child(div().min_w(px(0.0)).truncate().child(said.say(line))),
            );
        }

        let hint = said.say(HINT);
        let skip = said.say(SKIP);
        let label = said.say(start_label(&self.choice()));
        #[cfg(feature = "ui-shots")]
        {
            self.drawn = said.drawn;
        }

        let hover = ui.panel_hover;
        let footer = div()
            .flex_none()
            .flex()
            .items_center()
            .mt(px(FOOTER_ABOVE))
            .pt(px(FOOTER_BELOW))
            .border_t_1()
            .border_color(hsla(ui.panel_border))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .text_size(px(text::META))
                    .text_color(hsla(ui.text_faint))
                    .child(hint),
            )
            .child(
                div()
                    .id("welcome-skip")
                    .flex_none()
                    .flex()
                    .items_center()
                    .ml(px(8.0))
                    .h(px(BUTTON))
                    .px(px(BUTTON_PADDING))
                    .rounded(px(RADIUS))
                    .text_color(hsla(ui.text_dim))
                    .hover(move |s| s.bg(hsla(hover)))
                    .cursor_pointer()
                    .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(WelcomeEvent::Skip)))
                    .child(skip),
            )
            .child(
                div()
                    .id("welcome-start")
                    .relative()
                    .flex_none()
                    .flex()
                    .items_center()
                    .ml(px(6.0))
                    .h(px(BUTTON))
                    .px(px(BUTTON_PADDING))
                    .rounded(px(RADIUS))
                    .bg(hsla(ui.accent))
                    .text_color(hsla(ui.on_accent))
                    .font_weight(FontWeight::MEDIUM)
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.start(cx)))
                    .child(label)
                    .child(bounds_into(&self.start_bounds)),
            );

        overlay_panel::shell(&ui, CARD_WIDTH)
            .relative()
            .flex_none()
            .p(px(CARD_PADDING))
            .gap(px(0.0))
            .max_h(px(max_h))
            .track_focus(&self.focus)
            .key_context("Welcome")
            .on_key_down(cx.listener(Self::on_key_down))
            // Neither the scrim (a click there skips) nor anything under it gets its clicks.
            .occlude()
            .child(
                div()
                    .flex_none()
                    .text_size(px(TITLE_SIZE))
                    .line_height(px(TITLE_LINE))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title),
            )
            .child(
                div()
                    .flex_none()
                    .line_height(px(SUBTITLE_LINE))
                    .text_color(hsla(ui.text_dim))
                    .child(subtitle),
            )
            .child(heading(folder_heading, &ui))
            .child(folder_row)
            .child(heading(agent_heading, &ui))
            .when_some(note, |card, note| {
                card.child(
                    div()
                        .flex_none()
                        .mb(px(ROW_GAP))
                        .text_color(hsla(ui.text_dim))
                        .child(note),
                )
            })
            .child(
                div()
                    .id("welcome-agents")
                    .flex()
                    .flex_col()
                    .gap(px(ROW_GAP))
                    .flex_shrink(1.0)
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .children(agents),
            )
            .child(heading(shortcuts_heading, &ui))
            .child(
                div()
                    .flex_none()
                    .flex()
                    .child(
                        div()
                            .flex_none()
                            .flex()
                            .flex_col()
                            .items_start()
                            .gap(px(SHORTCUT_GAP))
                            .children(caps),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .ml(px(CAP_GAP))
                            .flex()
                            .flex_col()
                            .gap(px(SHORTCUT_GAP))
                            .text_color(hsla(ui.text_dim))
                            .children(lines),
                    ),
            )
            .child(footer)
            .child(bounds_into(&self.card_bounds))
    }
}

impl Workspace {
    /// Help → Welcome to fuxx, or a first run: open the Welcome card over the window. Any other
    /// box closes first; the card counts as shown from now on (saved before it shows). Its
    /// folder is `default_folder`'s; the presets' `PATH` check runs on the background executor
    /// (`agent_detect::check` blocks on the login shell's environment) and goes to the card if
    /// it is still open. The card takes the keyboard.
    pub(crate) fn open_welcome(
        &mut self,
        origin: Origin,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_overlays(Overlay::Welcome);
        self.welcomed = true;
        self.save();
        let home = self.places.home.clone();
        let folder = default_folder(
            origin,
            self.active_project(),
            self.active_ssh().is_some(),
            home.as_deref(),
        );
        let presets: Vec<Preset> = cx
            .global::<Appearance>()
            .presets
            .iter()
            .take(MAX_PRESETS)
            .cloned()
            .collect();
        let keys = WelcomeKeys::from_table(&binding_table(self.persistent), presets.len());
        let commands: Vec<String> = presets.iter().map(|p| p.command.clone()).collect();
        let card = cx.new(|cx| Welcome::new(origin, folder, home, presets, keys, window, cx));
        cx.subscribe_in(&card, window, Self::on_welcome_event)
            .detach();
        if !self.agents_frozen {
            let answer = card.downgrade();
            let check = cx.background_spawn(async move { agent_detect::check(commands) });
            cx.spawn(async move |_, cx| {
                let found = check.await;
                // A card closed meanwhile drops it.
                let _ = answer.update(cx, |card, cx| card.set_found(found, cx));
            })
            .detach();
        }
        card.focus_handle(cx).focus(window, cx);
        self.welcome = Some(card);
        cx.notify();
    }

    /// The card started something or was skipped.
    fn on_welcome_event(
        &mut self,
        card: &Entity<Welcome>,
        event: &WelcomeEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A card already closed or replaced is ignored.
        if self.welcome.as_ref() != Some(card) {
            return;
        }
        match event {
            WelcomeEvent::Skip => self.skip_welcome(window, cx),
            WelcomeEvent::Start { folder, choice } => {
                self.run_welcome_pick(folder.clone(), choice.clone(), window, cx)
            }
        }
    }

    /// Esc, Skip or a click on the scrim: the card closes and the keyboard goes back to the
    /// focused tab.
    fn skip_welcome(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.welcome = None;
        self.focus_active(window, cx);
        cx.notify();
    }

    /// A pick: the card closes and the keyboard goes back to the focused tab, then what was
    /// picked opens (`pick_effect`): nothing for Plain terminal in home on a first run (that is
    /// the terminal already there); otherwise `start_launch`'s session in a workspace of its
    /// own. On a first run the terminal the card was over goes once the new one exists, if it
    /// is still alone in its workspace: never before (fuxx quits with its last terminal) and
    /// never if the new one didn't start.
    pub(crate) fn run_welcome_pick(
        &mut self,
        folder: PathBuf,
        choice: Choice,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let origin = self
            .welcome
            .as_ref()
            .map_or(Origin::Help, |card| card.read(cx).origin());
        self.welcome = None;
        self.focus_active(window, cx);
        cx.notify();
        let over_alone = match origin {
            Origin::FirstRun { over } => self.over_alone(over),
            Origin::Help => false,
        };
        // The home the card wrote the folder against: exactly it is no project.
        let home = self.places.home.clone();
        let folder_is_home = home.as_deref() == Some(folder.as_path());
        let Effect::Open { close_after } = pick_effect(origin, over_alone, &choice, folder_is_home)
        else {
            return;
        };
        let launch = start_launch(&folder, home.as_deref(), &choice);
        let then: Opened = Box::new(move |this, opened, window, cx| {
            // Still alone now: with the background service the open waited for its answer.
            let Some(old) = close_after.filter(|old| opened.is_some() && this.over_alone(*old))
            else {
                return;
            };
            if let Some(index) = this.index_of(old) {
                this.close_at(index, window, cx);
            }
        });
        self.open_session_then(launch, At::New, Some(then), window, cx);
    }

    /// Whether terminal `over` is open and alone in its workspace (no other tab of any kind).
    fn over_alone(&self, over: u64) -> bool {
        self.model
            .terminal_tab(over)
            .and_then(|tab| self.model.locate(tab))
            .is_some_and(|(ws, _)| self.model.tabs_of(ws).len() == 1)
    }

    /// The scrim under the card: the window below the top band (which stays live: the window
    /// drags by it and the traffic lights work), darkened, taking every click (one skips). The
    /// card is centred in it both ways, `EDGE` from the sides and the bottom at least: the two
    /// spacers share the room above and below it equally, the lower never less than `EDGE`.
    pub(crate) fn welcome_layer(
        &self,
        card: Entity<Welcome>,
        ui: UiColors,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        div()
            .absolute()
            .top(px(TOP_BAND))
            .left_0()
            .right_0()
            .bottom_0()
            .bg(Hsla {
                a: SCRIM_ALPHA,
                ..hsla(ui.shadow)
            })
            .occlude()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, window, cx| this.skip_welcome(window, cx)),
            )
            .flex()
            .flex_col()
            .items_center()
            .px(px(EDGE))
            .child(div().flex_grow(1.0).flex_basis(px(0.0)).min_h(px(0.0)))
            .child(card)
            .child(div().flex_grow(1.0).flex_basis(px(0.0)).min_h(px(EDGE)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::every_theme;
    use crate::ui_tokens::{TEXT_MIN, contrast};

    /// Every ink the card draws, on every surface it draws it on.
    #[test]
    fn welcome_inks_meet_the_floors() {
        let mut failures = Vec::new();
        for (name, theme) in every_theme() {
            let ui = theme.ui();
            let mut pairs = Vec::new();
            for (bg_name, bg) in [
                ("panel", ui.panel),
                ("panel_hover", ui.panel_hover),
                ("row_active", ui.row_active),
            ] {
                for (fg_name, fg) in [
                    ("text", ui.text),
                    ("text_dim", ui.text_dim),
                    ("text_faint", ui.text_faint),
                ] {
                    pairs.push((fg_name, fg, bg_name, bg));
                }
            }
            pairs.push(("on_accent", ui.on_accent, "accent", ui.accent));
            for (fg_name, fg, bg_name, bg) in pairs {
                let c = contrast(fg, bg);
                if c < TEXT_MIN {
                    failures.push(format!("{name}: {fg_name} on {bg_name} {c:.2}"));
                }
            }
        }
        assert!(failures.is_empty(), "{failures:#?}");
    }

    /// The keys the card shows are the bound ones, read from the same table as Help's.
    #[test]
    fn the_three_shortcuts_are_bound() {
        let keys = WelcomeKeys::from_table(&binding_table(false), 3);
        assert_eq!(keys.shortcuts, ["⌘K", "⌘N", "⇧⌘U"]);
        assert_eq!(keys.presets.len(), 3);
        assert_eq!(keys.presets[0].as_deref(), Some("⌃⌘1"));
        assert_eq!(keys.presets[2].as_deref(), Some("⌃⌘3"));
    }

    /// Home on a first run; from Help the active folder (not an SSH terminal's), else home.
    #[test]
    fn the_folder_starts_at_home_or_the_active_folder() {
        let home = Path::new("/Users/me");
        let app = || Some(PathBuf::from("/w/app"));
        let first = Origin::FirstRun { over: 1 };
        assert_eq!(default_folder(first, app(), false, Some(home)), home);
        assert_eq!(
            default_folder(Origin::Help, app(), false, Some(home)),
            Path::new("/w/app")
        );
        assert_eq!(default_folder(Origin::Help, app(), true, Some(home)), home);
        assert_eq!(default_folder(Origin::Help, None, false, Some(home)), home);
        assert_eq!(
            default_folder(Origin::Help, None, false, None),
            Path::new("/")
        );
    }
}
