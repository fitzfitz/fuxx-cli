//! The agent box (M11ak): the chips under an agent tab's conversation that choose its model,
//! effort, mode and folder. This file holds the pure part: what a choice is (`Change`), when it
//! reaches the agent (`step_for`), the line a restart leaves (`switched_notice`) and what the
//! chips say (`chip_model_label`, `model_chip_text`, `mode_chip_text`, `usage_line`); and the
//! box's drawing and its menus (the folder picker: Task 6). The tab (`agent_tab.rs`) carries a
//! choice out.

use std::path::{Path, PathBuf};

use gpui::{
    Anchor, AnyElement, App, ClickEvent, Context, Div, Entity, Focusable, MouseButton,
    MouseDownEvent, SharedString, Stateful, Subscription, Window, anchored, deferred, div, point,
    prelude::*, px,
};

use crate::agent_catalog::{Catalogs, Lookup};
use crate::agent_tab::{AgentTab, AgentTabEvent, Colors, button, chip};
use crate::appearance::Appearance;
use crate::folder_picker::{
    KnownFolders, NEW_PROJECT, NO_PROJECT, PickerRow, THIS_COMPUTER, computer_folders, folder_row,
    picker_rows,
};
use crate::name_input::{NameInput, NameInputEvent};
use crate::providers::{Applies, Catalog, ChangeKind, ModeChoice, ModelChoice, Provider, Start};
use crate::terminal_view::hsla;
use crate::theme::UiColors;
use crate::ui_tokens::text;
use alacritty_terminal::vte::ansi::Rgb;

/// The box's sizes, named here so the UI uplift (M11al) restyles them in one place.
/// The room around the box: at its sides, above the folder row, under the box.
const AREA_X: f32 = 14.0;
const AREA_TOP: f32 = 6.0;
const AREA_BOTTOM: f32 = 12.0;
/// Between the folder row and the box.
const PLACE_GAP: f32 = 6.0;
/// The box: its corners, its inner room, the gap between its rows (attachments, text, chips).
const BOX_RADIUS: f32 = 12.0;
const BOX_X: f32 = 12.0;
const BOX_Y: f32 = 10.0;
const BOX_GAP: f32 = 8.0;
/// A chip: its height, inner room and corners; the gap between chips.
const CHIP_H: f32 = 24.0;
const CHIP_X: f32 = 8.0;
const CHIP_RADIUS: f32 = 6.0;
const CHIP_GAP: f32 = 4.0;
/// The `+` and send buttons' side.
const BUTTON: f32 = 24.0;
/// A menu: its width, and the gap between it and the chip it opens up from.
const MENU_WIDTH: f32 = 240.0;
const MENU_GAP: f32 = 4.0;
/// How far from the window's edges a menu keeps.
const MENU_EDGE: f32 = 8.0;
/// A menu row's height and corners; the most a menu's list grows before it scrolls.
const ROW_H: f32 = 26.0;
const ROW_RADIUS: f32 = 6.0;
const MENU_LIST_MAX: f32 = 320.0;
/// The id of This Computer's rows, beside the picker.
const COMPUTER_ROWS: &str = "box-computer";

/// The folder picker's words (§2), and the choice it asks after the first message.
const SEARCH: &str = "Search";
pub const MOVE_HERE: &str = "Move this conversation";
pub const NEW_CONVERSATION: &str = "New conversation";

/// The model menu's words (§4).
const MODEL_HEADING: &str = "Model";
pub const LOADING: &str = "Loading\u{2026}";
pub const MODEL_LIST_FAILED: &str = "Couldn't read the model list";
pub const DEFAULT_LIST: &str = "(default list)";
pub const NEXT_CONVERSATION_NOTE: &str = "applies to a new conversation";
pub const VIEW_USAGE: &str = "View Usage \u{25b8}";
/// The id of the effort list's rows, beside the model menu, and the width of its panel.
const EFFORT_ROWS: &str = "box-effort";
const EFFORT_WIDTH: f32 = 140.0;

/// The send button's glyph.
const SEND: &str = "\u{27a4}";

/// Where each part of the box was laid out, by name (scenes read it: `shot_part`).
#[cfg(feature = "ui-shots")]
pub type Parts = std::rc::Rc<
    std::cell::RefCell<std::collections::HashMap<&'static str, gpui::Bounds<gpui::Pixels>>>,
>;

/// Where each row of the open menu was laid out, by its words (scenes click them).
#[cfg(feature = "ui-shots")]
pub type MenuRows =
    std::rc::Rc<std::cell::RefCell<std::collections::HashMap<String, gpui::Bounds<gpui::Pixels>>>>;

/// The folder picker while it is open: its search box, the rows for what is typed in it, and
/// This Computer's folders beside it while that is open (with whether the agent's own list is
/// still being read).
pub(crate) struct FolderPicker {
    search: Entity<NameInput>,
    rows: Vec<PickerRow>,
    computer: Option<(Vec<PickerRow>, bool)>,
    _subscriptions: Vec<Subscription>,
}

/// The box's text while nothing is being asked (§1).
pub const PLACEHOLDER: &str = "Ask anything, @ to mention, / for actions";

/// A menu of the box, open over its chip: the folder picker (and, after it, Move this
/// conversation or a new one), the model menu (with one model's efforts, or View Usage, open
/// beside it), the mode menu, and `+`'s attach menu. One is open at a time.
#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)] // each opened by its chip's task (7–11)
pub enum BoxMenu {
    Folder,
    Model {
        efforts_of: Option<usize>,
        usage: bool,
    },
    Mode,
    Attach,
    Move(PathBuf),
}

/// What the box's chips and the top line say, as drawn (scenes read it).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BoxTexts {
    pub folder: String,
    /// The branch chip, when the folder is in a repo.
    pub branch: Option<String>,
    pub model: String,
    pub mode: Option<String>,
    /// `➤`, or the Stop button's label in its place.
    pub send: String,
    /// The top line's cost and context.
    pub usage: Option<String>,
}

/// The catalog model (its place) and effort level that `start`'s flags name: a model whose id,
/// or one of whose efforts' `model_id`, is `model_flag` (agy's ids carry the effort).
pub fn choice_of(
    catalog: &Catalog,
    model_flag: &str,
    effort_flag: Option<&str>,
) -> Option<(usize, Option<String>)> {
    if let Some(at) = catalog.models.iter().position(|m| m.id == model_flag) {
        return Some((at, effort_flag.map(String::from)));
    }
    catalog.models.iter().enumerate().find_map(|(at, m)| {
        m.efforts
            .iter()
            .find(|e| e.model_id.as_deref() == Some(model_flag))
            .map(|e| (at, Some(e.level.clone())))
    })
}

/// The model chip's name: the chosen model's label (`choice_of`); else the catalog model the
/// agent reports running (its `resolves_to`, or its id), one that names a model rather than
/// the agent's default; else the provider's name for the reported model; else `Model`.
pub fn chip_model_label(
    catalog: Option<&Catalog>,
    start: &Start,
    reported: Option<&str>,
    provider: &dyn Provider,
) -> String {
    let chosen = catalog
        .zip(start.model.as_deref())
        .and_then(|(catalog, flag)| {
            choice_of(catalog, flag, start.effort.as_deref()).map(|(at, _)| &catalog.models[at])
        });
    if let Some(model) = chosen {
        return model.label.clone();
    }
    let Some(reported) = reported else {
        return "Model".into();
    };
    let running = catalog.and_then(|catalog| {
        let names = |m: &&crate::providers::ModelChoice| {
            m.resolves_to.as_deref().unwrap_or(&m.id) == reported || m.id == reported
        };
        let mut matches = catalog.models.iter().filter(names);
        let first = matches.clone().next();
        matches
            .find(|m| !provider.stands_for_default(&m.id))
            .or(first)
    });
    match running {
        Some(model) => model.label.clone(),
        None => provider.model_label(reported),
    }
}

/// An effort level as the chip says it: `high` → `High`, `xhigh` → `XHigh`.
pub fn effort_word(level: &str) -> String {
    match level.strip_prefix('x') {
        Some(rest) if !rest.is_empty() => format!("X{}", effort_word(rest)),
        _ => {
            let mut chars = level.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().chain(chars).collect(),
                None => String::new(),
            }
        }
    }
}

/// The model chip: `<label>  <Effort> ▾`, or `<label> ▾` with no effort known.
pub fn model_chip_text(label: &str, effort: Option<&str>) -> String {
    match effort {
        Some(level) => format!("{label}  {} \u{25be}", effort_word(level)),
        None => format!("{label} \u{25be}"),
    }
}

/// The effort the chip shows: the one `start`'s model flag carries (agy), else its effort flag.
pub fn chip_effort(catalog: Option<&Catalog>, start: &Start) -> Option<String> {
    let carried = catalog
        .zip(start.model.as_deref())
        .and_then(|(catalog, flag)| choice_of(catalog, flag, None))
        .and_then(|(_, effort)| effort);
    carried.or_else(|| start.effort.clone())
}

/// The mode chip: `🛡 <label> ▾` for the chosen mode, else the one the agent reports, else the
/// provider's default (an id not in `modes` as it is); None when the provider has no modes or
/// none is known.
pub fn mode_chip_text(
    modes: &[ModeChoice],
    chosen: Option<&str>,
    reported: Option<&str>,
    default: Option<&str>,
) -> Option<String> {
    if modes.is_empty() {
        return None;
    }
    let id = chosen.or(reported).or(default)?;
    let label = modes.iter().find(|m| m.id == id).map_or(id, |m| m.label);
    Some(format!("\u{1f6e1} {label} \u{25be}"))
}

/// The folder chip: `📁 <the folder's name> ▾` (it opens the picker).
pub fn folder_chip_text(folder: &Path) -> String {
    let name = folder
        .file_name()
        .map_or_else(|| folder.to_string_lossy(), |n| n.to_string_lossy());
    format!("\u{1f4c1} {name} \u{25be}")
}

/// The branch chip: `⑂ <branch> ▾` (it opens the branch switcher on the folder).
pub fn branch_chip_text(branch: &str) -> String {
    format!("\u{2442} {branch} \u{25be}")
}

/// What the pane's top line says after the status: `$0.21 · context 18%`, either part alone,
/// or nothing.
pub fn usage_line(cost_usd: Option<f64>, context_pct: Option<u8>) -> Option<String> {
    let parts: Vec<String> = [
        cost_usd.map(|usd| format!("${usd:.2}")),
        context_pct.map(|p| format!("context {p}%")),
    ]
    .into_iter()
    .flatten()
    .collect();
    (!parts.is_empty()).then(|| parts.join(" \u{b7} "))
}

/// One row of the model menu, top to bottom.
#[derive(Clone, Debug, PartialEq)]
pub enum ModelRow {
    Heading,
    Model {
        ix: usize,
        label: String,
        effort: Option<String>,
        current: bool,
        has_efforts: bool,
    },
    Note(&'static str),
    NewConversation,
    Divider,
    Usage,
}

/// The model menu's rows (§4): the heading, then the models (or `Loading…`, or that the list
/// couldn't be read, or the `(default list)` note), a New conversation row when the provider
/// takes a model only in a new conversation, a divider and View Usage. The chosen model (by
/// `choice_of`, from `start`'s flags) is current and shows its effort.
pub fn model_rows(catalog: &Lookup<Catalog>, start: &Start, applies: Applies) -> Vec<ModelRow> {
    let next = applies == Applies::NextConversation;
    let mut rows = vec![ModelRow::Heading];
    if next {
        rows.push(ModelRow::Note(NEXT_CONVERSATION_NOTE));
    }
    match catalog {
        Lookup::Loading => rows.push(ModelRow::Note(LOADING)),
        Lookup::Failed(_) => rows.push(ModelRow::Note(MODEL_LIST_FAILED)),
        Lookup::Ready(catalog) => {
            if catalog.fallback_models {
                rows.push(ModelRow::Note(DEFAULT_LIST));
            }
            let chosen = start
                .model
                .as_deref()
                .and_then(|flag| choice_of(catalog, flag, start.effort.as_deref()));
            for (ix, model) in catalog.models.iter().enumerate() {
                let current = chosen.as_ref().is_some_and(|(at, _)| *at == ix);
                rows.push(ModelRow::Model {
                    ix,
                    label: model.label.clone(),
                    effort: chosen.clone().filter(|_| current).and_then(|(_, e)| e),
                    current,
                    has_efforts: !model.efforts.is_empty(),
                });
            }
        }
    }
    if next {
        rows.push(ModelRow::NewConversation);
    }
    rows.push(ModelRow::Divider);
    rows.push(ModelRow::Usage);
    rows
}

/// The effort to ask for when `model` is picked while the agent runs at `current`: the same
/// level when the model has it. A model whose ids carry the effort (agy's) needs one named: its
/// `medium`, else its first. Otherwise none (the agent keeps or clamps its own).
pub fn effort_on_switch(model: &ModelChoice, current: Option<&str>) -> Option<String> {
    if let Some(level) = current.filter(|c| model.efforts.iter().any(|e| e.level == *c)) {
        return Some(level.to_string());
    }
    if model.efforts.iter().any(|e| e.model_id.is_some()) {
        let pick = model
            .efforts
            .iter()
            .find(|e| e.level == "medium")
            .or(model.efforts.first());
        return pick.map(|e| e.level.clone());
    }
    None
}

/// The effort list of the model at `ix`: only the levels it has, the running one ticked when
/// the model is the one `start` names.
pub fn effort_rows(catalog: &Catalog, ix: usize, start: &Start) -> Vec<(String, bool)> {
    let running = start
        .model
        .as_deref()
        .and_then(|flag| choice_of(catalog, flag, start.effort.as_deref()))
        .filter(|(at, _)| *at == ix)
        .and_then(|(_, effort)| effort);
    catalog.models.get(ix).map_or_else(Vec::new, |model| {
        model
            .efforts
            .iter()
            .map(|e| {
                (
                    e.level.clone(),
                    running.as_deref() == Some(e.level.as_str()),
                )
            })
            .collect()
    })
}

/// A choice made in the box. An effort alone is a `Model` change with the current model's id.
#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    Model { id: String, effort: Option<String> },
    Mode(String),
    Folder(PathBuf),
}

/// When a change reaches the agent: now on its running process (`Live`), now by starting it
/// again on the same conversation (`RestartNow`), when the current turn ends (`AfterTurn`), or
/// only when it next starts (`NextStart`: the tab keeps it in its start flags).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Live,
    RestartNow,
    AfterTurn,
    NextStart,
}

/// The step for a change the provider takes as `applies`, the agent's process `running`, a
/// message `spoken` in the tab, a turn `working`.
pub fn step_for(applies: Applies, running: bool, spoken: bool, working: bool) -> Step {
    match (applies, spoken) {
        // Nothing said yet: live on a running agent, else it starts again with the change (no
        // process: only its start flags change).
        (Applies::Live, false) if running => Step::Live,
        (_, false) => Step::RestartNow,
        // This conversation never takes it.
        (Applies::NextConversation, true) => Step::NextStart,
        _ if working => Step::AfterTurn,
        (Applies::Live, true) if running => Step::Live,
        (Applies::Restart, true) if running => Step::RestartNow,
        _ => Step::NextStart,
    }
}

/// The quiet line a restart leaves in the conversation: `Switched to <model label> · <effort>`
/// (no effort part when none), `Switched to <mode label>`, `Moved to <~/path>`. A label not
/// given falls back to the id.
pub fn switched_notice(
    change: &Change,
    model_label: Option<&str>,
    mode_label: Option<&str>,
    home: Option<&Path>,
) -> String {
    match change {
        Change::Model { id, effort } => {
            let label = model_label.unwrap_or(id);
            match effort {
                Some(effort) => format!("Switched to {label} \u{b7} {effort}"),
                None => format!("Switched to {label}"),
            }
        }
        Change::Mode(id) => format!("Switched to {}", mode_label.unwrap_or(id)),
        Change::Folder(path) => {
            format!("Moved to {}", crate::sidebar::home_relative(path, home))
        }
    }
}

impl AgentTab {
    /// The catalog the chips read (the tab's agent and folder), once it is read.
    fn catalog<'a>(&self, cx: &'a App) -> Option<&'a Catalog> {
        let provider = self.provider?;
        match cx
            .try_global::<Catalogs>()?
            .catalog(provider.info().name, &self.start.cwd)?
        {
            Lookup::Ready(catalog) => Some(catalog),
            _ => None,
        }
    }

    /// What the box's chips and the top line say.
    pub(crate) fn box_texts(&self, cx: &App) -> BoxTexts {
        let catalog = self.catalog(cx);
        let model = self.provider.map(|provider| {
            let label = chip_model_label(catalog, &self.start, self.conversation.model(), provider);
            model_chip_text(&label, chip_effort(catalog, &self.start).as_deref())
        });
        let mode = self.provider.and_then(|provider| {
            let info = provider.info();
            mode_chip_text(
                info.modes,
                self.start.mode.as_deref(),
                catalog.and_then(|c| c.mode.as_deref()),
                info.default_mode,
            )
        });
        BoxTexts {
            folder: folder_chip_text(&self.start.cwd),
            branch: self.branch.as_deref().map(branch_chip_text),
            model: model.unwrap_or_default(),
            mode,
            send: self.stop_label().unwrap_or(SEND).to_string(),
            usage: usage_line(
                self.conversation.cost_usd(),
                self.conversation.context_pct(),
            ),
        }
    }

    /// The box's edge: the focus ring while its text has the keyboard, else the quiet edge
    /// (as the framed composer's own edge was). Read while drawing: a focus change refreshes
    /// the window, which draws cached views again.
    pub(crate) fn box_edge(&self, c: &Colors, window: &Window, cx: &App) -> Rgb {
        if self.composer.read(cx).focus_handle(cx).is_focused(window) {
            c.box_focus
        } else {
            c.box_edge
        }
    }

    /// `el`, and under `ui-shots` a probe that keeps where it was laid out as `part`.
    pub(crate) fn marked<E: ParentElement>(&self, part: &'static str, el: E) -> E {
        #[cfg(feature = "ui-shots")]
        {
            let parts = self.parts.clone();
            el.child(
                gpui::canvas(
                    move |bounds, _, _| {
                        parts.borrow_mut().insert(part, bounds);
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
        }
        #[cfg(not(feature = "ui-shots"))]
        {
            let _ = part;
            el
        }
    }

    /// The box (§1): the folder row above it; in it the attachments, the text and, along the
    /// bottom, `+`, the model chip, then the mode chip and send (Stop while the turn runs). The
    /// open menu hangs above its chip.
    pub(crate) fn box_area(
        &self,
        c: &Colors,
        mono: SharedString,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let texts = self.box_texts(cx);
        let edge = self.box_edge(c, window, cx);
        let menu = self.menu.clone();
        let menu_on = |chip: fn(&BoxMenu) -> bool| menu.as_ref().is_some_and(chip);
        let folder_hover = c.box_hover;
        let folder = self
            .marked(
                "folder",
                div()
                    .id("agent-folder")
                    .flex_none()
                    .h(px(CHIP_H))
                    .px(px(CHIP_X))
                    .flex()
                    .items_center()
                    .rounded(px(CHIP_RADIUS))
                    .text_size(px(text::META))
                    .text_color(hsla(c.dim))
                    .child(SharedString::from(texts.folder.clone())),
            )
            .when(self.provider.is_some(), |d| {
                d.cursor_pointer()
                    .hover(move |s| s.bg(hsla(folder_hover)))
                    .on_click(cx.listener(|this, event: &ClickEvent, window, cx| {
                        let pressed = match event {
                            ClickEvent::Mouse(click) => Some(click.down.position),
                            _ => None,
                        };
                        // This press already closed the open picker (from outside it).
                        let closed_at = this.chip_closed_at.take();
                        if pressed.is_none() || closed_at != pressed {
                            this.folder_clicked(window, cx)
                        }
                    }))
            })
            .when(
                menu_on(|m| matches!(m, BoxMenu::Folder | BoxMenu::Move(_))),
                |d| {
                    let (menu, side) = self.folder_menu(cx);
                    d.child(self.menu_frame(Some(menu), side, cx))
                },
            );
        let branch = texts.branch.clone().map(|text| {
            let hover = c.box_hover;
            self.marked(
                "branch",
                div()
                    .id("agent-branch")
                    .flex_none()
                    .h(px(CHIP_H))
                    .px(px(CHIP_X))
                    .flex()
                    .items_center()
                    .rounded(px(CHIP_RADIUS))
                    .text_size(px(text::META))
                    .text_color(hsla(c.dim))
                    .child(SharedString::from(text)),
            )
            .cursor_pointer()
            .hover(move |s| s.bg(hsla(hover)))
            .on_click(cx.listener(|this, _, _, cx| {
                this.close_menu(cx);
                cx.emit(AgentTabEvent::OpenBranchSwitcher(this.start.cwd.clone()))
            }))
        });
        let attach = self
            .marked(
                "attach",
                div()
                    .id("agent-attach")
                    .flex_none()
                    .size(px(BUTTON))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(CHIP_RADIUS))
                    .text_color(hsla(c.box_dim))
                    .child("+"),
            )
            .when(menu_on(|m| matches!(m, BoxMenu::Attach)), |d| {
                d.child(self.menu_frame(None, None, cx))
            });
        let model_hover = c.box_hover;
        let model = self
            .marked("model", box_chip("agent-model", texts.model.clone(), c))
            .when(self.provider.is_some(), |d| {
                d.cursor_pointer()
                    .hover(move |s| s.bg(hsla(model_hover)))
                    .on_click(cx.listener(|this, event: &ClickEvent, _, cx| {
                        let pressed = match event {
                            ClickEvent::Mouse(click) => Some(click.down.position),
                            _ => None,
                        };
                        // This press already closed the open menu (from outside it).
                        let closed_at = this.chip_closed_at.take();
                        if pressed.is_none() || closed_at != pressed {
                            this.model_clicked(cx)
                        }
                    }))
            })
            .when(menu_on(|m| matches!(m, BoxMenu::Model { .. })), |d| {
                let (menu, side) = self.model_menu(cx);
                d.child(self.menu_frame(Some(menu), side, cx))
            });
        let mode = texts.mode.clone().map(|text| {
            // Before the first message it steps to the agent's next mode (the mode menu, a
            // later task, replaces it).
            let steps = !self.spoken();
            let hover = c.box_hover;
            self.marked("mode", box_chip("agent-mode", text, c))
                .when(steps, |d| {
                    d.cursor_pointer()
                        .hover(move |s| s.bg(hsla(hover)))
                        .on_click(cx.listener(|this, _, _, cx| this.next_mode(cx)))
                })
                .when(menu_on(|m| matches!(m, BoxMenu::Mode)), |d| {
                    d.child(self.menu_frame(None, None, cx))
                })
        });
        let send = match self.stop_label() {
            // (A wrapper is probed: a probe inside an edge would sit 1 px in.)
            Some(label) => self
                .marked(
                    "send",
                    div().flex_none().child(
                        button("agent-stop", label, false, c)
                            .text_size(px(text::META))
                            .on_click(cx.listener(|this, _, _, cx| this.stop_pressed(cx))),
                    ),
                )
                .into_any_element(),
            None => self
                .marked(
                    "send",
                    div()
                        .id("agent-send")
                        .flex_none()
                        .size(px(BUTTON))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .bg(hsla(c.accent))
                        .text_color(hsla(c.on_accent))
                        .text_size(px(text::META))
                        .cursor_pointer()
                        .child(SEND),
                )
                .on_click(cx.listener(|this, _, _, cx| this.send_pressed(cx)))
                .into_any_element(),
        };
        let the_box = self.marked(
            "box",
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(BOX_GAP))
                .px(px(BOX_X))
                .py(px(BOX_Y))
                .rounded(px(BOX_RADIUS))
                .border_1()
                .border_color(hsla(edge))
                // A press on the box's own room (not a chip or button: theirs act as well)
                // puts the keyboard in its text, as the framed text box did.
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _: &MouseDownEvent, window, cx| {
                        this.composer.read(cx).focus_handle(cx).focus(window, cx)
                    }),
                )
                .bg(hsla(c.box_fill))
                .children(self.attachment_chips(c, cx))
                .child(
                    div()
                        .when(self.at_open(), |d| d.key_context("AtPopup"))
                        .w_full()
                        .font_family(mono.clone())
                        .child(self.composer.clone()),
                )
                .child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .gap(px(CHIP_GAP))
                        .child(attach)
                        .child(model)
                        .child(div().flex_1())
                        .children(mode)
                        .child(send),
                ),
        );
        div()
            .flex_none()
            .w_full()
            .px(px(AREA_X))
            .pt(px(AREA_TOP))
            .pb(px(AREA_BOTTOM))
            .flex()
            .flex_col()
            .children(self.at_list(c, mono, cx))
            .child(
                // The folder's name lines up with the text in the box.
                div()
                    .pl(px(BOX_X - CHIP_X))
                    .mb(px(PLACE_GAP))
                    .flex()
                    .items_center()
                    .gap(px(CHIP_GAP))
                    .child(folder)
                    .children(branch),
            )
            .child(the_box)
            .into_any_element()
    }

    /// What goes with your next message (a dropped command block), on top inside the box until
    /// it is sent; each chip's × takes it off.
    fn attachment_chips(&self, c: &Colors, cx: &mut Context<Self>) -> Option<Div> {
        if self.attachments.is_empty() {
            return None;
        }
        let chips = self.attachments.iter().enumerate().map(|(i, a)| {
            let (dim, text, hover) = (c.panel_dim, c.panel_text, c.panel_hover);
            let remove = div()
                .id(("attachment-remove", i))
                .size(px(14.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(3.))
                .text_color(hsla(dim))
                .cursor_pointer()
                .hover(move |s| s.bg(hsla(hover)).text_color(hsla(text)))
                .child("\u{d7}")
                .tooltip(|_, cx| -> gpui::AnyView {
                    cx.new(|_| crate::sidebar_toggle::Tip("Remove".into()))
                        .into()
                })
                .on_click(cx.listener(move |this, _, _, cx| this.remove_attachment_at(i, cx)));
            #[cfg(feature = "ui-shots")]
            let remove = {
                let removes = self.removes.clone();
                removes.borrow_mut().resize(self.attachments.len(), None);
                remove.child(
                    gpui::canvas(
                        move |b, _, _| {
                            if let Some(slot) = removes.borrow_mut().get_mut(i) {
                                *slot = Some(b);
                            }
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
                )
            };
            chip(
                format!("\u{1f4ce} {}", a.lines().next().unwrap_or("")),
                true,
                c,
            )
            .pr(px(4.))
            .flex()
            .items_center()
            .gap(px(4.))
            .child(remove)
        });
        Some(self.marked(
            "attachments",
            div().flex().flex_wrap().gap(px(6.)).children(chips),
        ))
    }

    /// The open menu's panel (`content` in it), hung above its chip (a child of it) and opening
    /// upward, in the tab menu's look; `side` (This Computer's folders) beside it. A click
    /// outside them closes it. What the model, mode and attach menus list comes with their own
    /// tasks.
    fn menu_frame(
        &self,
        content: Option<AnyElement>,
        side: Option<AnyElement>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ui = cx.global::<Appearance>().theme.ui();
        let panel = crate::overlay_panel::shell(&ui, MENU_WIDTH)
            .id("agent-box-menu")
            .p(px(4.))
            .gap(px(0.))
            .occlude()
            .children(content);
        div()
            .absolute()
            .top_0()
            .left_0()
            .child(deferred(
                anchored()
                    .anchor(Anchor::BottomLeft)
                    .offset(point(px(0.), px(-MENU_GAP)))
                    .snap_to_window_with_margin(px(MENU_EDGE))
                    .child(
                        // A press outside both panels closes it (one in the panel beside the
                        // menu is a pick, not "outside").
                        div()
                            .flex()
                            .items_end()
                            .gap(px(MENU_GAP))
                            .on_mouse_down_out(cx.listener(
                                |this, event: &MouseDownEvent, _, cx| {
                                    if matches!(
                                        this.menu,
                                        Some(
                                            BoxMenu::Folder
                                                | BoxMenu::Move(_)
                                                | BoxMenu::Model { .. }
                                        )
                                    ) {
                                        this.chip_closed_at = Some(event.position);
                                    }
                                    this.close_menu(cx)
                                },
                            ))
                            .child(self.marked("menu", div().child(panel)))
                            .children(side),
                    ),
            ))
            .into_any_element()
    }

    /// The folder chip's click: the picker opens over it, or (open) closes.
    fn folder_clicked(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.menu, Some(BoxMenu::Folder | BoxMenu::Move(_))) {
            self.close_menu(cx);
        } else {
            self.open_folder_picker(window, cx);
        }
    }

    /// Open the folder picker (§2), its search taking the keyboard. The first time in an app
    /// run it reads the folders the agent knows and the agent's version (whether it can move a
    /// conversation); a scene tab reads neither.
    pub fn open_folder_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(provider) = self.provider else {
            return;
        };
        if !self.no_process {
            crate::agent_catalog::load_known_folders(provider, cx);
            crate::agent_catalog::load_version(provider, cx);
        }
        let search = cx.new(|cx| NameInput::new(String::new(), window, cx));
        let subscriptions = vec![
            cx.observe(&search, |this, _, cx| this.refresh_picker(cx)),
            cx.subscribe_in(&search, window, Self::on_search),
        ];
        search.read(cx).focus_handle(cx).focus(window, cx);
        self.open_menu(BoxMenu::Folder, cx);
        self.picker = Some(FolderPicker {
            search,
            rows: Vec::new(),
            computer: None,
            _subscriptions: subscriptions,
        });
        self.refresh_picker(cx);
    }

    /// The search box: Return picks the first folder listed; Esc (while it has the keyboard:
    /// losing it to a click on a row is no cancel) closes the picker.
    fn on_search(
        &mut self,
        search: &Entity<NameInput>,
        event: &NameInputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            NameInputEvent::Confirm(_) => {
                let first = self.picker.as_ref().and_then(|p| {
                    p.rows
                        .iter()
                        .find(|r| matches!(r, PickerRow::Folder { .. }))
                        .cloned()
                });
                if let Some(row) = first {
                    self.pick_row(row, cx);
                    self.keyboard_back(window, cx);
                }
            }
            NameInputEvent::Cancel => {
                if search.read(cx).focus_handle(cx).is_focused(window) {
                    self.close_menu(cx);
                    self.keyboard_back(window, cx);
                }
            }
        }
    }

    /// The keyboard goes back to the box's text once the picker has closed.
    fn keyboard_back(&self, window: &mut Window, cx: &mut Context<Self>) {
        if self.menu.is_none() {
            self.composer.read(cx).focus_handle(cx).focus(window, cx);
        }
    }

    /// Whether `folder` is there to start in. A scene tab reads no disk: its made-up folders are.
    fn folder_exists(&self, folder: &Path) -> bool {
        self.no_process || folder.is_dir()
    }

    /// Make the open picker's rows again: for what is typed, from the folders fuxx knows
    /// (`KnownFolders`) and the agent's own (once read); This Computer's too while it is open.
    pub(crate) fn refresh_picker(&mut self, cx: &mut Context<Self>) {
        let Some(picker) = &self.picker else {
            return;
        };
        let query = picker.search.read(cx).text().to_string();
        let open_beside = picker.computer.is_some();
        let known = cx.try_global::<KnownFolders>().cloned().unwrap_or_default();
        let agents = self.provider.and_then(|p| {
            cx.try_global::<Catalogs>()?
                .known_folders(p.info().name)
                .cloned()
        });
        let loading = matches!(agents, Some(Lookup::Loading));
        let agents = match agents {
            Some(Lookup::Ready(list)) => list,
            _ => Vec::new(),
        };
        let computer = computer_folders(&known.projects, &agents);
        let home = crate::workspace::home_dir();
        let cwd = &self.start.cwd;
        let rows = picker_rows(
            &query,
            &known.recent,
            &computer,
            cwd,
            home.as_deref(),
            |f| self.folder_exists(f),
        );
        let beside = open_beside.then(|| {
            let rows = computer
                .iter()
                .filter(|f| self.folder_exists(f))
                .map(|f| folder_row(f, cwd, home.as_deref()))
                .collect();
            (rows, loading)
        });
        if let Some(picker) = self.picker.as_mut() {
            picker.rows = rows;
            picker.computer = beside;
        }
        // Rows no longer listed leave no probe behind.
        #[cfg(feature = "ui-shots")]
        self.menu_rows.borrow_mut().clear();
        cx.notify();
    }

    /// A row of the picker chosen (its click; Return in the search for the first folder).
    pub fn pick_row(&mut self, row: PickerRow, cx: &mut Context<Self>) {
        match row {
            PickerRow::Folder { path, .. } => self.choose_folder(path, cx),
            PickerRow::ThisComputer => {
                if let Some(picker) = self.picker.as_mut() {
                    picker.computer = match picker.computer {
                        Some(_) => None,
                        None => Some((Vec::new(), false)),
                    };
                }
                self.refresh_picker(cx);
            }
            PickerRow::NewProject => {
                self.close_menu(cx);
                // The folder panel, "New Folder" in it: the folder chosen or made is used.
                let picked = cx.prompt_for_paths(crate::workspace::folder_panel());
                cx.spawn(async move |this, cx| {
                    let folder = match picked.await {
                        Ok(Ok(Some(paths))) => paths.into_iter().next(),
                        Ok(Err(err)) => {
                            eprintln!("[fuxx] agent box: folder panel failed: {err}");
                            None
                        }
                        _ => None,
                    };
                    if let Some(folder) = folder {
                        this.update(cx, |this, cx| this.choose_folder(folder, cx))
                            .ok();
                    }
                })
                .detach();
            }
            PickerRow::NoProject => match crate::workspace::home_dir() {
                Some(home) => self.choose_folder(home, cx),
                None => self.close_menu(cx),
            },
            PickerRow::Heading(_) => {}
        }
    }

    /// `folder` chosen: the folder it is in already does nothing; before the first message the
    /// agent starts there (`choose`); after it, the picker asks: Move this conversation, or a
    /// New conversation there.
    fn choose_folder(&mut self, folder: PathBuf, cx: &mut Context<Self>) {
        if folder == self.start.cwd {
            self.close_menu(cx);
        } else if self.spoken() {
            self.open_menu(BoxMenu::Move(folder), cx);
        } else {
            self.close_menu(cx);
            self.choose(Change::Folder(folder), cx);
        }
    }

    /// The answer to Move this conversation (`move_it`) or New conversation: the agent
    /// restarts on this conversation in the folder (after the turn, if one runs), or a new
    /// agent tab opens there.
    pub fn move_choice(&mut self, move_it: bool, cx: &mut Context<Self>) {
        let Some(BoxMenu::Move(folder)) = self.menu.clone() else {
            return;
        };
        self.close_menu(cx);
        if !move_it {
            cx.emit(AgentTabEvent::NewConversationIn(folder));
        } else if self.can_move(cx) {
            self.choose(Change::Folder(folder), cx);
        }
    }

    /// Whether the agent can carry this conversation to another folder: by a restart (never
    /// live), and its provider says so for the version it read.
    fn can_move(&self, cx: &App) -> bool {
        let Some(provider) = self.provider else {
            return false;
        };
        let version =
            cx.try_global::<Catalogs>()
                .and_then(|c| match c.version(provider.info().name) {
                    Some(Lookup::Ready(version)) => version.clone(),
                    _ => None,
                });
        provider.applies(ChangeKind::Folder) == Applies::Restart
            && provider.can_move(version.as_deref())
    }

    /// The folder menu's panel content, and This Computer's list beside it while open: the
    /// picker (search, rows), or after the first message the Move / New conversation choice.
    fn folder_menu(&self, cx: &mut Context<Self>) -> (AnyElement, Option<AnyElement>) {
        let ui = cx.global::<Appearance>().theme.ui();
        if let Some(BoxMenu::Move(folder)) = &self.menu {
            let home = crate::workspace::home_dir();
            let mut rows = vec![heading(
                crate::sidebar::home_relative(folder, home.as_deref()),
                &ui,
            )];
            if self.can_move(cx) {
                rows.push(self.menu_row(
                    ("box-move", 0),
                    MOVE_HERE.into(),
                    false,
                    false,
                    &ui,
                    cx,
                    |this, window, cx| {
                        this.move_choice(true, cx);
                        this.keyboard_back(window, cx);
                    },
                ));
            }
            rows.push(self.menu_row(
                ("box-move", 1),
                NEW_CONVERSATION.into(),
                false,
                false,
                &ui,
                cx,
                |this, window, cx| {
                    this.move_choice(false, cx);
                    this.keyboard_back(window, cx);
                },
            ));
            return (
                div().flex().flex_col().children(rows).into_any_element(),
                None,
            );
        }
        let Some(picker) = &self.picker else {
            return (div().into_any_element(), None);
        };
        let empty = picker.search.read(cx).text().is_empty();
        let field = crate::overlay_panel::filter_box(&ui, &picker.search, empty, SEARCH);
        let rows: Vec<AnyElement> = picker
            .rows
            .iter()
            .enumerate()
            .map(|(i, row)| self.picker_row(("box-folder", i), row, &ui, cx))
            .collect();
        let list = div()
            .id("box-folder-rows")
            .flex()
            .flex_col()
            .max_h(px(MENU_LIST_MAX))
            .overflow_y_scroll()
            .children(rows);
        let menu = div()
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(field)
            .child(list)
            .into_any_element();
        let side = picker.computer.as_ref().map(|(rows, loading)| {
            let mut items: Vec<AnyElement> = rows
                .iter()
                .enumerate()
                .map(|(i, row)| self.picker_row((COMPUTER_ROWS, i), row, &ui, cx))
                .collect();
            if *loading {
                items.push(quiet("Loading\u{2026}", &ui));
            } else if items.is_empty() {
                items.push(quiet("No folders", &ui));
            }
            let panel = crate::overlay_panel::shell(&ui, MENU_WIDTH)
                .p(px(4.))
                .gap(px(0.))
                .occlude()
                .child(heading(THIS_COMPUTER.into(), &ui))
                .child(
                    div()
                        .id("box-computer-rows")
                        .flex()
                        .flex_col()
                        .max_h(px(MENU_LIST_MAX))
                        .overflow_y_scroll()
                        .children(items),
                );
            self.marked("computer", div().child(panel))
                .into_any_element()
        });
        (menu, side)
    }

    /// One of the picker's rows, drawn; its click picks it.
    fn picker_row(
        &self,
        id: (&'static str, usize),
        row: &PickerRow,
        ui: &UiColors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (label, check, more) = match row {
            PickerRow::Heading(h) => return heading((*h).into(), ui),
            PickerRow::Folder { shown, current, .. } => (shown.clone(), *current, false),
            PickerRow::ThisComputer => (THIS_COMPUTER.to_string(), false, true),
            PickerRow::NewProject => (NEW_PROJECT.to_string(), false, false),
            PickerRow::NoProject => (NO_PROJECT.to_string(), false, false),
        };
        let row = row.clone();
        self.menu_row(id, label, check, more, ui, cx, move |this, window, cx| {
            this.pick_row(row.clone(), cx);
            this.keyboard_back(window, cx);
        })
    }

    /// The catalog lookup the model menu reads: what is known for the agent and folder (not yet
    /// asked: still loading).
    fn catalog_lookup(&self, cx: &App) -> Lookup<Catalog> {
        let found = self
            .provider
            .zip(cx.try_global::<Catalogs>())
            .and_then(|(p, catalogs)| catalogs.catalog(p.info().name, &self.start.cwd));
        found.cloned().unwrap_or(Lookup::Loading)
    }

    /// The start flags the model menu shows as current: the model the agent runs (named by the
    /// chip when none was chosen), with a model change waiting for the turn laid over it (so an
    /// effort picked meanwhile belongs to the model you picked, and composes with it).
    pub(crate) fn shown_start(&self, cx: &App) -> Start {
        let mut shown = self.start.clone();
        let Some(provider) = self.provider else {
            return shown;
        };
        if let Some(Change::Model { id, effort }) = self.waiting_model() {
            let (model, effort) = self.model_flags(provider, id, effort.as_deref(), cx);
            shown.model = Some(model);
            if effort.is_some() {
                shown.effort = effort;
            }
        } else if shown.model.is_none()
            && let Some(catalog) = self.catalog(cx)
        {
            let label = chip_model_label(
                Some(catalog),
                &self.start,
                self.conversation.model(),
                provider,
            );
            shown.model = catalog
                .models
                .iter()
                .find(|m| m.label == label)
                .map(|m| m.id.clone());
        }
        shown
    }

    /// The model chip's click: the model menu opens over it, or (open) closes. Opening asks for
    /// the model list again when it could not be read (or was never asked).
    fn model_clicked(&mut self, cx: &mut Context<Self>) {
        if matches!(self.menu, Some(BoxMenu::Model { .. })) {
            self.close_menu(cx);
            return;
        }
        if let Some(provider) = self.provider
            && !self.no_process
        {
            crate::agent_catalog::load_catalog(provider, self.start.cwd.clone(), cx);
        }
        self.open_menu(
            BoxMenu::Model {
                efforts_of: None,
                usage: false,
            },
            cx,
        );
    }

    /// The model at `ix` of the open list chosen, with the effort it keeps or needs
    /// (`effort_on_switch`): the menu closes, the change is asked for (`choose`), nothing is sent.
    fn pick_model(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Lookup::Ready(catalog) = self.catalog_lookup(cx) else {
            return;
        };
        let Some(model) = catalog.models.get(ix) else {
            return;
        };
        let current = chip_effort(Some(&catalog), &self.shown_start(cx));
        let change = Change::Model {
            id: model.id.clone(),
            effort: effort_on_switch(model, current.as_deref()),
        };
        self.close_menu(cx);
        self.choose(change, cx);
        self.keyboard_back(window, cx);
    }

    /// `level` of the model at `ix` chosen (as for that model's row, with this effort).
    fn pick_effort(
        &mut self,
        ix: usize,
        level: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Lookup::Ready(catalog) = self.catalog_lookup(cx) else {
            return;
        };
        let Some(model) = catalog.models.get(ix) else {
            return;
        };
        let change = Change::Model {
            id: model.id.clone(),
            effort: Some(level),
        };
        self.close_menu(cx);
        self.choose(change, cx);
        self.keyboard_back(window, cx);
    }

    /// The model menu's panel content, and the open model's effort list beside it.
    fn model_menu(&self, cx: &mut Context<Self>) -> (AnyElement, Option<AnyElement>) {
        let ui = cx.global::<Appearance>().theme.ui();
        let lookup = self.catalog_lookup(cx);
        let shown = self.shown_start(cx);
        let applies = self
            .provider
            .map_or(Applies::Live, |p| p.applies(ChangeKind::Model));
        let efforts_of = match &self.menu {
            Some(BoxMenu::Model { efforts_of, .. }) => *efforts_of,
            _ => None,
        };
        let mut top = Vec::new();
        let mut list = Vec::new();
        let mut bottom = Vec::new();
        for (i, row) in model_rows(&lookup, &shown, applies).iter().enumerate() {
            match row {
                ModelRow::Heading => top.push(heading(MODEL_HEADING.into(), &ui)),
                ModelRow::Note(note) => list.push(quiet(note, &ui)),
                ModelRow::Model { .. } => {
                    list.push(self.model_row(i, row, efforts_of, &ui, cx));
                }
                ModelRow::NewConversation => list.push(self.menu_row(
                    ("box-model", i),
                    NEW_CONVERSATION.into(),
                    false,
                    false,
                    &ui,
                    cx,
                    |this, window, cx| {
                        this.close_menu(cx);
                        cx.emit(AgentTabEvent::NewConversationIn(this.start.cwd.clone()));
                        this.keyboard_back(window, cx);
                    },
                )),
                ModelRow::Divider => bottom.push(
                    div()
                        .h(px(1.))
                        .mx(px(4.))
                        .my(px(4.))
                        .bg(hsla(ui.panel_border))
                        .into_any_element(),
                ),
                // Its submenu is a later task's (View Usage).
                ModelRow::Usage => bottom.push(self.menu_row(
                    ("box-model", i),
                    VIEW_USAGE.into(),
                    false,
                    false,
                    &ui,
                    cx,
                    |_, _, _| {},
                )),
            }
        }
        let menu = div()
            .flex()
            .flex_col()
            .children(top)
            .child(
                div()
                    .id("box-model-rows")
                    .flex()
                    .flex_col()
                    .max_h(px(MENU_LIST_MAX))
                    .overflow_y_scroll()
                    .children(list),
            )
            .children(bottom)
            .into_any_element();
        let side = match (efforts_of, &lookup) {
            (Some(ix), Lookup::Ready(catalog)) => {
                let items: Vec<AnyElement> = effort_rows(catalog, ix, &shown)
                    .into_iter()
                    .enumerate()
                    .map(|(i, (level, current))| {
                        self.menu_row(
                            (EFFORT_ROWS, i),
                            effort_word(&level),
                            current,
                            false,
                            &ui,
                            cx,
                            move |this, window, cx| this.pick_effort(ix, level.clone(), window, cx),
                        )
                    })
                    .collect();
                let panel = crate::overlay_panel::shell(&ui, EFFORT_WIDTH)
                    .p(px(4.))
                    .gap(px(0.))
                    .occlude()
                    .children(items);
                Some(self.marked("effort", div().child(panel)).into_any_element())
            }
            _ => None,
        };
        (menu, side)
    }

    /// One model of the menu: ✓ when current, its name, its effort in grey (the current one's),
    /// and `▸` (when it has efforts), which opens its effort list beside the menu. A click on
    /// the name picks the model.
    fn model_row(
        &self,
        i: usize,
        row: &ModelRow,
        efforts_of: Option<usize>,
        ui: &UiColors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ModelRow::Model {
            ix,
            label,
            effort,
            current,
            has_efforts,
        } = row
        else {
            return div().into_any_element();
        };
        let (ix, hover) = (*ix, ui.panel_hover);
        let name = div()
            .id(("box-model-pick", i))
            .flex_1()
            .min_w_0()
            .h_full()
            .pl(px(8.))
            .flex()
            .items_center()
            .gap(px(6.))
            .cursor_pointer()
            .child(
                div()
                    .flex_none()
                    .w(px(12.))
                    .text_color(hsla(ui.accent))
                    .child(if *current { "\u{2713}" } else { "" }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(SharedString::from(label.clone())),
            )
            .children(effort.as_deref().map(|level| {
                div()
                    .flex_none()
                    .text_color(hsla(ui.text_dim))
                    .child(SharedString::from(effort_word(level)))
            }))
            .on_click(cx.listener(move |this, _, window, cx| this.pick_model(ix, window, cx)));
        let more = div()
            .id(("box-model-more", i))
            .flex_none()
            .h_full()
            .px(px(8.))
            .flex()
            .items_center()
            .text_color(hsla(ui.text_dim))
            .cursor_pointer()
            .child("\u{25b8}")
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open_menu(
                    BoxMenu::Model {
                        efforts_of: Some(ix),
                        usage: false,
                    },
                    cx,
                )
            }));
        let row = div()
            .id(("box-model", i))
            .h(px(ROW_H))
            .flex()
            .items_center()
            .rounded(px(ROW_RADIUS))
            .hover(move |s| s.bg(hsla(hover)))
            .when(efforts_of == Some(ix), move |d| d.bg(hsla(hover)))
            .child(name)
            .when(*has_efforts, |d| d.child(more));
        self.marked_row(label.clone(), row).into_any_element()
    }

    /// A menu row: a ✓ when `check`, its words (cut at the start when long: a path keeps its
    /// folder's name), `▸` when `more` opens beside it; `pick` on its click.
    #[allow(clippy::too_many_arguments)]
    fn menu_row(
        &self,
        id: (&'static str, usize),
        label: String,
        check: bool,
        more: bool,
        ui: &UiColors,
        cx: &mut Context<Self>,
        pick: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> AnyElement {
        let hover = ui.panel_hover;
        let row = div()
            .id(id)
            .h(px(ROW_H))
            .px(px(8.))
            .flex()
            .items_center()
            .gap(px(6.))
            .rounded(px(ROW_RADIUS))
            .cursor_pointer()
            .hover(move |s| s.bg(hsla(hover)))
            .child(
                div()
                    .flex_none()
                    .w(px(12.))
                    .text_color(hsla(ui.accent))
                    .child(if check { "\u{2713}" } else { "" }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis_start()
                    .child(SharedString::from(label.clone())),
            )
            .when(more, |d| {
                d.child(
                    div()
                        .flex_none()
                        .text_color(hsla(ui.text_dim))
                        .child("\u{25b8}"),
                )
            })
            .on_click(cx.listener(move |this, _, window, cx| pick(this, window, cx)));
        // A row beside the menu (This Computer's, an effort) is probed apart from the menu's own.
        let probe = match id.0 {
            COMPUTER_ROWS => format!("{THIS_COMPUTER} \u{25b8} {label}"),
            EFFORT_ROWS => format!("Effort \u{25b8} {label}"),
            _ => label,
        };
        self.marked_row(probe, row).into_any_element()
    }

    /// `el`, and under `ui-shots` a probe that keeps where it was laid out by its `label`.
    fn marked_row<E: ParentElement>(&self, label: String, el: E) -> E {
        #[cfg(feature = "ui-shots")]
        {
            let rows = self.menu_rows.clone();
            el.child(
                gpui::canvas(
                    move |bounds, _, _| {
                        rows.borrow_mut().insert(label.clone(), bounds);
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
        }
        #[cfg(not(feature = "ui-shots"))]
        {
            let _ = label;
            el
        }
    }
}

/// The scenes' hooks for the folder picker.
#[cfg(feature = "ui-shots")]
impl AgentTab {
    /// The picker's rows as drawn (none while it is closed).
    pub fn shot_picker_rows(&self) -> Vec<PickerRow> {
        self.picker
            .as_ref()
            .map_or_else(Vec::new, |p| p.rows.clone())
    }

    /// This Computer's rows beside the picker, while open.
    pub fn shot_computer_rows(&self) -> Option<Vec<PickerRow>> {
        self.picker
            .as_ref()
            .and_then(|p| p.computer.as_ref().map(|(rows, _)| rows.clone()))
    }

    /// The picker's search box.
    pub fn shot_search(&self) -> Option<Entity<NameInput>> {
        self.picker.as_ref().map(|p| p.search.clone())
    }

    /// Where the open menu's row saying `label` was last laid out.
    pub fn shot_menu_row(&self, label: &str) -> Option<gpui::Bounds<gpui::Pixels>> {
        self.menu_rows.borrow().get(label).copied()
    }

    /// The words of the open menu's rows, top to bottom (as last laid out).
    pub fn shot_menu_labels(&self) -> Vec<String> {
        let rows = self.menu_rows.borrow();
        let mut labels: Vec<(&String, &gpui::Bounds<gpui::Pixels>)> = rows.iter().collect();
        labels.sort_by(|a, b| a.1.top().partial_cmp(&b.1.top()).expect("a number"));
        labels.into_iter().map(|(l, _)| l.clone()).collect()
    }

    /// The model menu's rows as drawn now (what `model_rows` makes of the lookup and the start
    /// the menu shows as current).
    pub fn shot_model_rows(&self, cx: &App) -> Vec<ModelRow> {
        let applies = self
            .provider
            .map_or(Applies::Live, |p| p.applies(ChangeKind::Model));
        model_rows(&self.catalog_lookup(cx), &self.shown_start(cx), applies)
    }

    /// The folder the agent starts (or runs) in.
    pub fn shot_cwd(&self) -> PathBuf {
        self.start.cwd.clone()
    }

    /// Put the tab in `folder` on `branch` (the branch chip), as a scene sets the place without
    /// reading a disk.
    pub fn shot_place(&mut self, folder: PathBuf, branch: Option<String>, cx: &mut Context<Self>) {
        self.branch = branch;
        self.start.cwd = folder;
        self.refresh_picker(cx);
        cx.notify();
    }
}

/// A menu's section heading.
fn heading(text: String, ui: &UiColors) -> AnyElement {
    div()
        .px(px(8.))
        .pt(px(6.))
        .pb(px(2.))
        .text_size(px(text::META))
        .text_color(hsla(ui.text_faint))
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis_start()
        .child(SharedString::from(text))
        .into_any_element()
}

/// A quiet line in a menu's list (no row to pick).
fn quiet(text: &'static str, ui: &UiColors) -> AnyElement {
    div()
        .h(px(ROW_H))
        .px(px(26.))
        .flex()
        .items_center()
        .text_color(hsla(ui.text_dim))
        .child(text)
        .into_any_element()
}

/// A chip on the box's bottom row: quiet text, no edge.
fn box_chip(id: &'static str, text: String, c: &Colors) -> Stateful<Div> {
    div()
        .id(id)
        .flex_none()
        .h(px(CHIP_H))
        .px(px(CHIP_X))
        .flex()
        .items_center()
        .rounded(px(CHIP_RADIUS))
        .text_size(px(text::META))
        .text_color(hsla(c.box_dim))
        .child(SharedString::from(text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::EffortChoice;
    use crate::providers::agy::AGY;
    use crate::providers::claude::CLAUDE;
    use Applies::{Live, NextConversation, Restart};
    use Step::{AfterTurn, NextStart, RestartNow};

    fn model(id: &str, label: &str, resolves_to: Option<&str>, efforts: &[&str]) -> ModelChoice {
        ModelChoice {
            id: id.into(),
            label: label.into(),
            resolves_to: resolves_to.map(String::from),
            efforts: efforts
                .iter()
                .map(|l| EffortChoice {
                    level: l.to_string(),
                    model_id: None,
                })
                .collect(),
        }
    }

    fn catalog(models: Vec<ModelChoice>) -> Catalog {
        Catalog {
            models,
            ..Catalog::default()
        }
    }

    fn start(model: Option<&str>, effort: Option<&str>) -> Start {
        Start {
            cwd: "/tmp/my-app".into(),
            resume: None,
            new_id: None,
            mode: None,
            model: model.map(String::from),
            effort: effort.map(String::from),
            extra_args: Vec::new(),
        }
    }

    /// Claude's list as its `initialize` gives it: the default entry first, standing for the
    /// same model as `opus`.
    fn claude_list() -> Catalog {
        catalog(vec![
            model(
                "default",
                "Default (recommended)",
                Some("claude-opus-5-5"),
                &["low", "high"],
            ),
            model(
                "opus",
                "Opus 5.5",
                Some("claude-opus-5-5"),
                &["low", "high"],
            ),
            model(
                "sonnet",
                "Sonnet 5.5",
                Some("claude-sonnet-5-5"),
                &["low", "high"],
            ),
        ])
    }

    /// agy's grouped list: the effort is in each level's id.
    fn agy_list() -> Catalog {
        let with_ids = |id: &str, label: &str, levels: &[&str]| ModelChoice {
            id: id.into(),
            label: label.into(),
            resolves_to: None,
            efforts: levels
                .iter()
                .map(|l| EffortChoice {
                    level: l.to_string(),
                    model_id: Some(format!("{id}-{l}")),
                })
                .collect(),
        };
        catalog(vec![
            with_ids("gemini-3.8-flash", "Gemini 3.8 Flash", &["low", "high"]),
            with_ids(
                "claude-opus-5-5",
                "Claude Opus 5.5",
                &["low", "medium", "high"],
            ),
        ])
    }

    #[test]
    fn chip_model_label_prefers_the_named_model() {
        let list = claude_list();
        // Nothing chosen, Claude reports the model the default stands for: its own name.
        assert_eq!(
            chip_model_label(
                Some(&list),
                &start(None, None),
                Some("claude-opus-5-5"),
                &CLAUDE
            ),
            "Opus 5.5"
        );
        // Chosen: that model's label, whatever is reported.
        assert_eq!(
            chip_model_label(
                Some(&list),
                &start(Some("sonnet"), Some("low")),
                Some("claude-opus-5-5"),
                &CLAUDE
            ),
            "Sonnet 5.5"
        );
        // Chosen by a full id the list doesn't name, reported as such: the provider's name.
        assert_eq!(
            chip_model_label(
                Some(&list),
                &start(Some("claude-haiku-9"), None),
                Some("claude-haiku-9"),
                &CLAUDE
            ),
            "haiku"
        );
        // No list yet: the provider's name for what is reported; nothing known: `Model`.
        assert_eq!(
            chip_model_label(None, &start(None, None), Some("claude-opus-5-5"), &CLAUDE),
            "opus"
        );
        assert_eq!(
            chip_model_label(None, &start(None, None), None, &CLAUDE),
            "Model"
        );
    }

    #[test]
    fn choice_of_finds_agy_effort_ids() {
        let list = agy_list();
        assert_eq!(
            choice_of(&list, "claude-opus-5-5-high", None),
            Some((1, Some("high".into())))
        );
        // The model's own id, with an effort flag (Claude's way): that model, that effort.
        assert_eq!(
            choice_of(&list, "gemini-3.8-flash", Some("low")),
            Some((0, Some("low".into())))
        );
        assert_eq!(choice_of(&list, "gemini-3.8-flash", None), Some((0, None)));
        assert_eq!(choice_of(&list, "gpt-oss-120b-medium", None), None);
        // As the chip reads it: agy's name and the effort its id carries.
        let s = start(Some("claude-opus-5-5-high"), None);
        assert_eq!(
            chip_model_label(Some(&list), &s, None, &AGY),
            "Claude Opus 5.5"
        );
        assert_eq!(chip_effort(Some(&list), &s).as_deref(), Some("high"));
        // Claude's: the effort flag.
        let s = start(Some("opus"), Some("xhigh"));
        assert_eq!(
            chip_effort(Some(&claude_list()), &s).as_deref(),
            Some("xhigh")
        );
        assert_eq!(chip_effort(None, &s).as_deref(), Some("xhigh"));
        assert_eq!(chip_effort(None, &start(None, None)), None);
    }

    #[test]
    fn model_rows_loading_failed_and_fallback() {
        let s = start(None, None);
        let words = |rows: &[ModelRow]| -> Vec<String> {
            rows.iter()
                .map(|r| match r {
                    ModelRow::Heading => "Model".into(),
                    ModelRow::Model { label, .. } => label.clone(),
                    ModelRow::Note(n) => n.to_string(),
                    ModelRow::NewConversation => "New conversation".into(),
                    ModelRow::Divider => "-".into(),
                    ModelRow::Usage => "View Usage".into(),
                })
                .collect()
        };
        let loading = model_rows(&Lookup::Loading, &s, Live);
        assert_eq!(
            words(&loading),
            ["Model", "Loading\u{2026}", "-", "View Usage"]
        );
        let failed = model_rows(&Lookup::Failed("boom".into()), &s, Live);
        assert_eq!(
            words(&failed),
            ["Model", "Couldn't read the model list", "-", "View Usage"],
            "the reason is for logs, not made-up words"
        );
        let fallback = Catalog {
            fallback_models: true,
            ..claude_list()
        };
        let rows = model_rows(&Lookup::Ready(fallback), &s, Live);
        assert_eq!(
            words(&rows)[..3],
            ["Model", "(default list)", "Default (recommended)"],
            "{rows:?}"
        );
        let read = model_rows(&Lookup::Ready(claude_list()), &s, Live);
        assert!(!words(&read).contains(&"(default list)".to_string()));
        // A provider that takes a change only in a new conversation says so, and offers one.
        let next = model_rows(&Lookup::Ready(claude_list()), &s, NextConversation);
        assert_eq!(
            words(&next)[..2],
            ["Model", "applies to a new conversation"]
        );
        assert!(next.contains(&ModelRow::NewConversation));
        assert!(!read.contains(&ModelRow::NewConversation));
        assert_eq!(next.last(), Some(&ModelRow::Usage));
    }

    #[test]
    fn model_rows_mark_the_chosen() {
        let list = Lookup::Ready(claude_list());
        let models = |s: &Start| -> Vec<(usize, bool, Option<String>, bool)> {
            model_rows(&list, s, Live)
                .into_iter()
                .filter_map(|r| match r {
                    ModelRow::Model {
                        ix,
                        current,
                        effort,
                        has_efforts,
                        ..
                    } => Some((ix, current, effort, has_efforts)),
                    _ => None,
                })
                .collect()
        };
        let rows = models(&start(Some("sonnet"), Some("low")));
        let current: Vec<_> = rows.iter().filter(|r| r.1).collect();
        assert_eq!(current.len(), 1, "one is chosen: {rows:?}");
        assert_eq!(current[0].2.as_deref(), Some("low"), "its effort, in grey");
        assert!(
            rows.iter().filter(|r| !r.1).all(|r| r.2.is_none()),
            "the others show no effort"
        );
        // Nothing chosen: nothing is ticked.
        assert!(models(&start(None, None)).iter().all(|r| !r.1));
        // agy's effort rides in the id.
        let agy = Lookup::Ready(agy_list());
        let rows = model_rows(&agy, &start(Some("claude-opus-5-5-high"), None), Restart);
        assert!(rows.iter().any(|r| matches!(
            r,
            ModelRow::Model { ix: 1, current: true, effort: Some(e), has_efforts: true, .. } if e == "high"
        )));
    }

    #[test]
    fn a_model_switch_keeps_the_effort_it_has() {
        let list = claude_list();
        let sonnet = &list.models[2];
        assert_eq!(
            effort_on_switch(sonnet, Some("low")).as_deref(),
            Some("low")
        );
        // Claude clamps an effort the model lacks; nothing is made up for it.
        let no_effort = model("haiku", "Haiku", None, &[]);
        assert_eq!(effort_on_switch(&no_effort, Some("low")), None);
        assert_eq!(effort_on_switch(sonnet, Some("max-ish")), None);
        // agy's model ids carry the effort: one must be named (the current, else medium).
        let agy = agy_list();
        let opus = &agy.models[1];
        assert_eq!(
            effort_on_switch(opus, Some("high")).as_deref(),
            Some("high")
        );
        assert_eq!(effort_on_switch(opus, None).as_deref(), Some("medium"));
        assert_eq!(
            effort_on_switch(opus, Some("xhigh")).as_deref(),
            Some("medium")
        );
        let (id, effort) = AGY.model_flags(opus, effort_on_switch(opus, None).as_deref());
        assert_eq!((id.as_str(), effort), ("claude-opus-5-5-medium", None));
    }

    #[test]
    fn effort_rows_list_only_the_models_levels() {
        let list = claude_list();
        let s = start(Some("sonnet"), Some("low"));
        let levels = |ix| -> Vec<(String, bool)> { effort_rows(&list, ix, &s) };
        let on_sonnet = levels(2);
        assert!(on_sonnet.iter().any(|(l, c)| l == "low" && *c));
        assert!(on_sonnet.iter().filter(|(_, c)| *c).count() == 1);
        let on_opus = levels(1);
        assert!(on_opus.iter().all(|(_, c)| !c), "another model: no tick");
        assert_eq!(
            on_opus.iter().map(|(l, _)| l.as_str()).collect::<Vec<_>>(),
            list.models[1]
                .efforts
                .iter()
                .map(|e| e.level.as_str())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn effort_words() {
        assert_eq!(effort_word("high"), "High");
        assert_eq!(effort_word("xhigh"), "XHigh");
        assert_eq!(effort_word("low"), "Low");
        assert_eq!(effort_word("max"), "Max");
        assert_eq!(effort_word(""), "");
        assert_eq!(
            model_chip_text("Claude Example 5", Some("high")),
            "Claude Example 5  High \u{25be}"
        );
        assert_eq!(model_chip_text("Opus 5.5", None), "Opus 5.5 \u{25be}");
    }

    #[test]
    fn mode_chip_says_the_mode_in_force() {
        let modes = CLAUDE.info().modes;
        let chip = |chosen, reported, default| mode_chip_text(modes, chosen, reported, default);
        assert_eq!(
            chip(Some("plan"), Some("default"), None).as_deref(),
            Some("\u{1f6e1} Plan \u{25be}"),
            "chosen first"
        );
        assert_eq!(
            chip(None, Some("default"), None).as_deref(),
            Some("\u{1f6e1} Ask before edits \u{25be}"),
            "then what the agent reports"
        );
        assert_eq!(
            mode_chip_text(AGY.info().modes, None, None, AGY.info().default_mode).as_deref(),
            Some("\u{1f6e1} Accept edits \u{25be}"),
            "then the provider's default"
        );
        assert_eq!(
            chip(None, Some("bypassPermissions"), None).as_deref(),
            Some("\u{1f6e1} bypassPermissions \u{25be}"),
            "a mode not listed: its id"
        );
        assert_eq!(chip(None, None, None), None, "none known");
        assert_eq!(
            mode_chip_text(&[], Some("plan"), None, None),
            None,
            "no modes"
        );
    }

    #[test]
    fn the_branch_chip_names_the_branch() {
        assert_eq!(branch_chip_text("master"), "\u{2442} master \u{25be}");
        assert_eq!(
            branch_chip_text("feature/rows"),
            "\u{2442} feature/rows \u{25be}"
        );
    }

    #[test]
    fn the_folder_chip_names_the_folder() {
        assert_eq!(
            folder_chip_text(Path::new("/home/me/code/my-app")),
            "\u{1f4c1} my-app \u{25be}"
        );
        assert_eq!(folder_chip_text(Path::new("/")), "\u{1f4c1} / \u{25be}");
    }

    #[test]
    fn the_top_line_carries_cost_and_context() {
        assert_eq!(
            usage_line(Some(0.2116182), Some(18)).as_deref(),
            Some("$0.21 \u{b7} context 18%")
        );
        assert_eq!(usage_line(Some(1.5), None).as_deref(), Some("$1.50"));
        assert_eq!(usage_line(None, Some(3)).as_deref(), Some("context 3%"));
        assert_eq!(usage_line(None, None), None);
    }

    #[test]
    fn step_for_covers_every_case() {
        // (applies, running, spoken, working) → step.
        let table = [
            // Before the first message: live on a running process, else a restart (which, with
            // no process, only changes how it starts).
            (Live, true, false, false, Step::Live),
            (Live, false, false, false, RestartNow),
            (Restart, true, false, false, RestartNow),
            (Restart, false, false, false, RestartNow),
            (NextConversation, true, false, false, RestartNow),
            (NextConversation, false, false, false, RestartNow),
            // While a turn runs: after it.
            (Live, true, true, true, AfterTurn),
            (Live, false, true, true, AfterTurn),
            (Restart, true, true, true, AfterTurn),
            (Restart, false, true, true, AfterTurn),
            // Idle after the first message.
            (Live, true, true, false, Step::Live),
            (Live, false, true, false, NextStart),
            (Restart, true, true, false, RestartNow),
            (Restart, false, true, false, NextStart),
            // Only a new conversation takes it: never this one.
            (NextConversation, true, true, false, NextStart),
            (NextConversation, false, true, false, NextStart),
            (NextConversation, true, true, true, NextStart),
            (NextConversation, false, true, true, NextStart),
        ];
        for (applies, running, spoken, working, want) in table {
            assert_eq!(
                step_for(applies, running, spoken, working),
                want,
                "{applies:?} running {running} spoken {spoken} working {working}"
            );
        }
    }

    #[test]
    fn switched_notices() {
        let model = |effort: Option<&str>| Change::Model {
            id: "sonnet".into(),
            effort: effort.map(String::from),
        };
        assert_eq!(
            switched_notice(
                &model(Some("medium")),
                Some("Claude Sonnet 5.5"),
                None,
                None
            ),
            "Switched to Claude Sonnet 5.5 \u{b7} medium"
        );
        assert_eq!(
            switched_notice(&model(None), Some("Claude Sonnet 5.5"), None, None),
            "Switched to Claude Sonnet 5.5"
        );
        assert_eq!(
            switched_notice(&model(None), None, None, None),
            "Switched to sonnet",
            "no label: the id"
        );
        assert_eq!(
            switched_notice(&Change::Mode("plan".into()), None, Some("Plan"), None),
            "Switched to Plan"
        );
        let home = Path::new("/home/me");
        assert_eq!(
            switched_notice(
                &Change::Folder(home.join("code/scratch")),
                None,
                None,
                Some(home)
            ),
            "Moved to ~/code/scratch"
        );
        assert_eq!(
            switched_notice(&Change::Folder("/tmp/x".into()), None, None, Some(home)),
            "Moved to /tmp/x",
            "outside home: as it is"
        );
    }
}
