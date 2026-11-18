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
use crate::palette::name_cell;
use crate::palette_items::FileHit;
use crate::palette_match::fuzzy;
use crate::providers::{
    AgentCommand, Applies, Catalog, ChangeKind, CommandKind, LimitState, ModeChoice, ModelChoice,
    Provider, RateLimit, Reset, Start, UsageGroup, UsageLimit,
};
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
/// The `/` list's tallest height (it scrolls) and the width its names have before their lines.
const SLASH_MAX_H: f32 = 340.0;
const SLASH_NAME_W: f32 = 180.0;
/// The id of This Computer's rows, beside the picker.
const COMPUTER_ROWS: &str = "box-computer";

/// The folder picker's words (§2), and the choice it asks after the first message.
const SEARCH: &str = "Search";
pub const MOVE_HERE: &str = "Move this conversation";
pub const NEW_CONVERSATION: &str = "New conversation";

/// The model menu's words (§4).
const MODEL_HEADING: &str = "Model";
/// The mode menu's heading (§6).
const MODE_HEADING: &str = "Mode";
pub const LOADING: &str = "Loading\u{2026}";
/// The `/` list's group headings, and its notes.
pub const SLASH_HEADINGS: [&str; 3] = ["Commands", "Skills", "Agents"];
pub const SLASH_LIST_FAILED: &str = "Couldn't read the list";
/// The `@` list's group headings, and the most agents or skills it shows of each.
pub const AT_HEADINGS: [&str; 3] = ["Files", "Agents", "Skills"];
pub const AT_NAMED_LIMIT: usize = 5;
pub const SLASH_NONE: &str = "No command matches";
pub const MODEL_LIST_FAILED: &str = "Couldn't read the model list";
pub const DEFAULT_LIST: &str = "(default list)";
pub const NEXT_CONVERSATION_NOTE: &str = "applies to a new conversation";
pub const VIEW_USAGE: &str = "View Usage \u{25b8}";
/// The id of the effort list's rows, beside the model menu, and the width of its panel.
const EFFORT_ROWS: &str = "box-effort";
const EFFORT_WIDTH: f32 = 140.0;
/// The View Usage panel's width, and its ring's size and stroke.
const USAGE_WIDTH: f32 = 270.0;
const RING_SIZE: f32 = 14.0;
const RING_STROKE: f32 = 2.0;

/// The send button's glyph.
/// The warning dot on the model chip.
const LIMIT_DOT: f32 = 6.0;
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

/// The attach menu's rows.
pub const FILE_ROW: &str = "File\u{2026}";
pub const IMAGE_ROW: &str = "Image\u{2026}";

/// The box's text while nothing is being asked (§1).
pub const PLACEHOLDER: &str = "Ask anything, @ to mention, / for actions";

/// A menu of the box, open over its chip: the folder picker (and, after it, Move this
/// conversation or a new one), the model menu (with one model's efforts, or View Usage, open
/// beside it), the mode menu, and `+`'s attach menu. One is open at a time.
#[derive(Clone, Debug, PartialEq)]
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
    /// The warning dot on the model chip: a limit is close or reached.
    pub limit_warning: bool,
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

/// The mode in force: the chosen one, else the one the agent reports, else the provider's default.
pub fn current_mode<'a>(
    chosen: Option<&'a str>,
    reported: Option<&'a str>,
    default: Option<&'a str>,
) -> Option<&'a str> {
    chosen.or(reported).or(default)
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
    let id = current_mode(chosen, reported, default)?;
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

/// What the View Usage panel says while a reading is made, when one failed, and when the agent
/// has none.
pub const CHECKING: &str = "Checking\u{2026}";
pub const USAGE_FAILED: &str = "Couldn't read usage";

/// `Usage isn't available for Claude`.
pub fn usage_unavailable(label: &str) -> String {
    format!("Usage isn't available for {label}")
}

/// Now, in unix seconds.
pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// `Resets in 3d 16h`, `Resets in 2h 5m`, `Resets in 59m`, `Resets now`: the time left, to the
/// minute (cut, never rounded up, but at least `1m` while any is left), with the day and hour
/// parts only when they are not nil (`1d`, `1h`).
pub fn resets_in(secs: i64) -> String {
    if secs <= 0 {
        return "Resets now".into();
    }
    let (days, hours, mins) = (secs / 86_400, secs % 86_400 / 3600, secs % 3600 / 60);
    let words = if days > 0 {
        match hours {
            0 => format!("{days}d"),
            h => format!("{days}d {h}h"),
        }
    } else if hours > 0 {
        match mins {
            0 => format!("{hours}h"),
            m => format!("{hours}h {m}m"),
        }
    } else {
        format!("{}m", mins.max(1))
    };
    format!("Resets in {words}")
}

/// A limit's reset line at `now_unix`: a moment counts down (`resets_in`); the agent's own words
/// (Claude's `Nov 17 at 8:10pm`) stay its words.
pub fn reset_text(reset: &Reset, now_unix: i64) -> String {
    match reset {
        Reset::At(at) => resets_in(at - now_unix),
        Reset::Words(words) => format!("Resets {words}"),
    }
}

/// What the agent said of a limit that is close or reached (`Fine`: nothing), the time of day
/// (24-hour) at `utc_offset` seconds east of UTC.
pub fn limit_line(limit: &RateLimit, utc_offset: i64) -> Option<String> {
    match limit.state {
        LimitState::Fine => None,
        LimitState::Close => Some("Close to limit".into()),
        LimitState::Reached => Some(match limit.resets_at {
            Some(at) => format!(
                "Limit reached \u{b7} resets {}",
                crate::activity::clock_hm(at, utc_offset)
            ),
            None => "Limit reached".into(),
        }),
    }
}

/// Whether the model chip carries the warning dot: any limit the agent said is close or reached
/// (and has not reset since).
/// A limit the agent said is close or reached counts until its reset moment has passed.
pub fn limits_warn(limits: &[RateLimit], now_unix: i64) -> bool {
    limits.iter().any(|l| is_live(l, now_unix))
}

/// Whether the agent's word on `limit` still stands: close or reached, and its reset (if it
/// said one) not yet come.
fn is_live(limit: &RateLimit, now_unix: i64) -> bool {
    limit.state != LimitState::Fine && limit.resets_at.is_none_or(|at| at > now_unix)
}

/// One line of the View Usage panel, top to bottom.
#[derive(Clone, Debug, PartialEq)]
pub enum UsageRow {
    /// A group's name (agy names its groups; Claude's lines have none).
    Heading(String),
    /// A limit: how much is left (none: the agent only said it is close or reached), when it
    /// resets, and what the agent said of its state.
    Limit {
        name: String,
        left_pct: Option<u8>,
        resets: Option<String>,
        note: Option<String>,
    },
    /// A line fuxx couldn't read, as the agent wrote it.
    Text(String),
    /// Checking, failed, or not available.
    Quiet(String),
}

/// The View Usage panel's lines (§5). `found` is the reading (none: not asked yet, as loading);
/// `limits` what the agent said of its limits while it ran, shown under the limit of that name,
/// or on their own when the reading has none of that name (or none was read).
pub fn usage_rows(
    found: Option<&Lookup<Vec<UsageGroup>>>,
    limits: &[RateLimit],
    available: Option<&str>,
    now_unix: i64,
    utc_offset: i64,
) -> Vec<UsageRow> {
    let said = |name: &str| limits.iter().find(|l| l.limit == name);
    // The agent's moment, while it is still ahead (a Fine limit's old one is not a reset).
    let ahead = |l: &RateLimit| l.resets_at.filter(|at| *at > now_unix);
    let mut rows = Vec::new();
    let mut shown: Vec<&str> = Vec::new();
    match (available, found) {
        (Some(label), _) => rows.push(UsageRow::Quiet(usage_unavailable(label))),
        (None, None | Some(Lookup::Loading)) => rows.push(UsageRow::Quiet(CHECKING.into())),
        (None, Some(Lookup::Failed(_))) => rows.push(UsageRow::Quiet(USAGE_FAILED.into())),
        (None, Some(Lookup::Ready(groups))) => {
            for group in groups {
                if !group.name.is_empty() {
                    rows.push(UsageRow::Heading(group.name.clone()));
                }
                for limit in &group.limits {
                    rows.push(match limit {
                        UsageLimit::Text(text) => UsageRow::Text(text.clone()),
                        UsageLimit::Read {
                            name,
                            left_pct,
                            resets,
                        } => {
                            let told = said(name).filter(|l| is_live(l, now_unix));
                            shown.push(name);
                            let resets = match resets {
                                Some(reset) => Some(reset_text(reset, now_unix)),
                                None => told.and_then(ahead).map(|at| resets_in(at - now_unix)),
                            };
                            UsageRow::Limit {
                                name: name.clone(),
                                left_pct: Some(*left_pct),
                                resets,
                                note: told.and_then(|l| limit_line(l, utc_offset)),
                            }
                        }
                    });
                }
            }
            if rows.is_empty() {
                rows.push(UsageRow::Quiet(USAGE_FAILED.into()));
            }
        }
    }
    for limit in limits
        .iter()
        .filter(|l| !shown.contains(&l.limit.as_str()) && is_live(l, now_unix))
    {
        if let Some(note) = limit_line(limit, utc_offset) {
            rows.push(UsageRow::Limit {
                name: limit.limit.clone(),
                left_pct: None,
                resets: ahead(limit).map(|at| resets_in(at - now_unix)),
                note: Some(note),
            });
        }
    }
    rows
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

/// One row of the mode menu, top to bottom.
#[derive(Clone, Debug, PartialEq)]
pub enum ModeRow {
    Heading,
    Note(&'static str),
    Mode {
        id: &'static str,
        label: &'static str,
        about: &'static str,
        current: bool,
    },
    NewConversation,
}

/// The mode menu's rows (§6): the heading, `applies to a new conversation` when a mode takes
/// effect only then, each of the provider's modes (the one in force ticked), and a New
/// conversation row in that case.
pub fn mode_rows(modes: &[ModeChoice], current: Option<&str>, applies: Applies) -> Vec<ModeRow> {
    let next = applies == Applies::NextConversation;
    let mut rows = vec![ModeRow::Heading];
    if next {
        rows.push(ModeRow::Note(NEXT_CONVERSATION_NOTE));
    }
    rows.extend(modes.iter().map(|m| ModeRow::Mode {
        id: m.id,
        label: m.label,
        about: m.about,
        current: current == Some(m.id),
    }));
    if next {
        rows.push(ModeRow::NewConversation);
    }
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

/// The most rows one group of the `/` list shows.
pub const SLASH_LIMIT: usize = 8;

/// What follows the `/` the text starts with, up to the caret, while the caret is still in that
/// first word; else None (no list).
pub fn slash_query(text: &str, cursor: usize) -> Option<&str> {
    let rest = text.strip_prefix('/')?;
    let word_end = 1 + rest.find(char::is_whitespace).unwrap_or(rest.len());
    if cursor < 1 || cursor > word_end {
        return None;
    }
    text.get(1..cursor)
}

/// A row of the `/` list: a group's heading, or a command (its place in the catalog's list, and
/// the chars of its name the query matched).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SlashRow {
    Heading(CommandKind),
    Item { ix: usize, positions: Vec<usize> },
}

/// The `/` list for `query`: the commands, skills and sub-agents that match it (fuzzy, on the
/// name and the aliases), best first, at most `limit` of each, each group under its heading.
pub fn slash_rows(commands: &[AgentCommand], query: &str, limit: usize) -> Vec<SlashRow> {
    let mut rows = Vec::new();
    for kind in [CommandKind::Command, CommandKind::Skill, CommandKind::Agent] {
        // (score, place, the name's matched chars) of each command of this kind that matches.
        let mut found: Vec<(i32, usize, Vec<usize>)> = commands
            .iter()
            .enumerate()
            .filter(|(_, c)| c.kind == kind)
            .filter_map(|(ix, c)| {
                if let Some(m) = fuzzy(query, &c.name) {
                    return Some((m.score, ix, m.positions));
                }
                let by_alias = c
                    .aliases
                    .iter()
                    .filter_map(|a| fuzzy(query, a))
                    .max_by_key(|m| m.score);
                by_alias.map(|m| (m.score, ix, Vec::new()))
            })
            .collect();
        // Best first; equal scores keep the agent's order.
        found.sort_by_key(|(score, ..)| std::cmp::Reverse(*score));
        found.truncate(limit);
        if found.is_empty() {
            continue;
        }
        rows.push(SlashRow::Heading(kind));
        rows.extend(
            found
                .into_iter()
                .map(|(_, ix, positions)| SlashRow::Item { ix, positions }),
        );
    }
    rows
}

/// A row of the `@` list: a section's heading, a file (its place among the hits), or a sub-agent
/// or skill (its place in the catalog's list, and the chars of its name the query matched).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AtRow {
    Heading(&'static str),
    File(usize),
    Named { ix: usize, positions: Vec<usize> },
}

/// The `@` list for `query`: the ranked file `files` as they are, then the sub-agents, then the
/// skills that match it (fuzzy, best first, at most `AT_NAMED_LIMIT` of each), each section under
/// its heading; a section with nothing in it is left out.
pub fn at_rows(files: &[FileHit], commands: &[AgentCommand], query: &str) -> Vec<AtRow> {
    let mut rows = Vec::new();
    if !files.is_empty() {
        rows.push(AtRow::Heading(AT_HEADINGS[0]));
        rows.extend((0..files.len()).map(AtRow::File));
    }
    for (kind, heading) in [
        (CommandKind::Agent, AT_HEADINGS[1]),
        (CommandKind::Skill, AT_HEADINGS[2]),
    ] {
        let mut found: Vec<(i32, usize, Vec<usize>)> = commands
            .iter()
            .enumerate()
            .filter(|(_, c)| c.kind == kind)
            .filter_map(|(ix, c)| fuzzy(query, &c.name).map(|m| (m.score, ix, m.positions)))
            .collect();
        found.sort_by_key(|(score, ..)| std::cmp::Reverse(*score));
        found.truncate(AT_NAMED_LIMIT);
        if found.is_empty() {
            continue;
        }
        rows.push(AtRow::Heading(heading));
        rows.extend(
            found
                .into_iter()
                .map(|(_, ix, positions)| AtRow::Named { ix, positions }),
        );
    }
    rows
}

/// What the `@` list says of its Agents and Skills while the agent's list is unknown (the files
/// are listed regardless).
pub fn at_list_note(lookup: &Lookup<Catalog>) -> Option<&'static str> {
    match lookup {
        Lookup::Loading => Some(LOADING),
        Lookup::Failed(_) => Some(SLASH_LIST_FAILED),
        Lookup::Ready(_) => None,
    }
}

/// The line the `/` list says in place of rows: while the commands load or could not be read,
/// or when nothing matches.
pub fn slash_note(lookup: &Lookup<Catalog>, rows: &[SlashRow]) -> Option<&'static str> {
    match lookup {
        Lookup::Loading => Some(LOADING),
        Lookup::Failed(_) => Some(SLASH_LIST_FAILED),
        Lookup::Ready(_) => rows.is_empty().then_some(SLASH_NONE),
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
            limit_warning: self.limit_warning(cx),
        }
    }

    /// Whether the agent said a limit is close or reached (`Event::RateLimit`, kept in
    /// `Catalogs`); the model chip's warning dot.
    fn limit_warning(&self, cx: &App) -> bool {
        self.provider
            .zip(cx.try_global::<Catalogs>())
            .is_some_and(|(p, catalogs)| limits_warn(catalogs.limits(p.info().name), now_unix()))
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
            .when(self.provider.is_some(), |d| {
                let hover = c.box_hover;
                d.cursor_pointer()
                    .hover(move |s| s.bg(hsla(hover)))
                    .on_click(cx.listener(|this, event: &ClickEvent, _, cx| {
                        let pressed = match event {
                            ClickEvent::Mouse(click) => Some(click.down.position),
                            _ => None,
                        };
                        // This press already closed the open menu (from outside it).
                        let closed_at = this.chip_closed_at.take();
                        if pressed.is_none() || closed_at != pressed {
                            this.attach_clicked(cx)
                        }
                    }))
            })
            .when(menu_on(|m| matches!(m, BoxMenu::Attach)), |d| {
                d.child(self.menu_frame(Some(self.attach_menu(cx)), None, cx))
            });
        let model_hover = c.box_hover;
        let warn = c.ask_edge;
        let model = self
            .marked("model", box_chip("agent-model", texts.model.clone(), c))
            .relative()
            .when(texts.limit_warning, |d| {
                d.child(
                    self.marked(
                        "limit-warning",
                        div()
                            .absolute()
                            .top(px(4.))
                            .right(px(2.))
                            .size(px(LIMIT_DOT))
                            .rounded_full()
                            .bg(hsla(warn)),
                    ),
                )
            })
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
            let hover = c.box_hover;
            self.marked("mode", box_chip("agent-mode", text, c))
                .cursor_pointer()
                .hover(move |s| s.bg(hsla(hover)))
                .on_click(cx.listener(|this, event: &ClickEvent, _, cx| {
                    let pressed = match event {
                        ClickEvent::Mouse(click) => Some(click.down.position),
                        _ => None,
                    };
                    // This press already closed the open menu (from outside it).
                    let closed_at = this.chip_closed_at.take();
                    if pressed.is_none() || closed_at != pressed {
                        this.mode_clicked(cx)
                    }
                }))
                .when(menu_on(|m| matches!(m, BoxMenu::Mode)), |d| {
                    d.child(self.menu_frame(Some(self.mode_menu(cx)), None, cx))
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
        let drop_fill = cx.global::<Appearance>().theme.ui().drop_card;
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
                // Files dropped on it become chips (tinted while one is over it).
                .drag_over::<gpui::ExternalPaths>(move |s, _, _, _| s.bg(hsla(drop_fill)))
                .on_drop(cx.listener(|this, paths: &gpui::ExternalPaths, _, cx| {
                    this.attach_dropped(paths.paths().to_vec(), cx)
                }))
                .children(self.attachment_chips(c, cx))
                .child(
                    div()
                        .when(self.at_open() || self.slash_has_picks(cx), |d| {
                            d.key_context("AtPopup")
                        })
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
            .children(self.at_list(c, mono.clone(), cx))
            .children(self.slash_list(c, mono, cx))
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

    /// The `/` list, over the box while it is open: the agent's commands, skills and sub-agents
    /// that match what is typed, under their headings, each with its one-line description.
    fn slash_list(&self, c: &Colors, mono: SharedString, cx: &mut Context<Self>) -> Option<Div> {
        let list = self.slash.as_ref()?;
        let lookup = self.catalog_lookup(cx);
        let (rows, note) = match &lookup {
            Lookup::Ready(catalog) => {
                let rows = slash_rows(&catalog.commands, &list.query, SLASH_LIMIT);
                let els = self.slash_row_els(&catalog.commands, &rows, list.selected, c, cx);
                (els, slash_note(&lookup, &rows))
            }
            _ => (Vec::new(), slash_note(&lookup, &[])),
        };
        let panel = div()
            .w_full()
            .mb(px(8.))
            .p(px(4.))
            .rounded(px(7.))
            .bg(hsla(c.panel))
            .border_1()
            .border_color(hsla(c.panel_border))
            .font_family(mono)
            .text_size(px(text::META + 0.5))
            .flex()
            .flex_col()
            .child(
                div()
                    .id("agent-slash")
                    .flex()
                    .flex_col()
                    .max_h(px(SLASH_MAX_H))
                    .overflow_y_scroll()
                    .track_scroll(&self.slash_scroll)
                    .children(rows),
            )
            .when_some(note, |d, n| {
                d.child(
                    div()
                        .px(px(8.))
                        .py(px(3.))
                        .text_color(hsla(c.panel_dim))
                        .child(n),
                )
            });
        Some(self.marked("slash", div().w_full().child(panel)))
    }

    /// The `@` list, over the box while it is open: the files ranked for what follows the `@`,
    /// then the agent's sub-agents and skills that match, each section under its heading.
    pub(crate) fn at_list(
        &self,
        c: &Colors,
        mono: SharedString,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let at = self.at.as_ref()?;
        let lookup = self.catalog_lookup(cx);
        let commands: &[AgentCommand] = match &lookup {
            Lookup::Ready(catalog) => &catalog.commands,
            _ => &[],
        };
        let mut pick = 0;
        let mut rows: Vec<AnyElement> = Vec::new();
        for row in at_rows(&at.hits, commands, &at.query) {
            let (this, label, positions, about) = match &row {
                AtRow::Heading(heading) => {
                    rows.push(
                        div()
                            .px(px(8.))
                            .pt(px(4.))
                            .pb(px(1.))
                            .text_size(px(text::META))
                            .text_color(hsla(c.panel_dim))
                            .child(*heading)
                            .into_any_element(),
                    );
                    continue;
                }
                AtRow::File(i) => {
                    let Some(hit) = at.hits.get(*i) else {
                        continue;
                    };
                    (pick, hit.rel.clone(), Vec::new(), None)
                }
                AtRow::Named { ix, positions } => {
                    let Some(command) = commands.get(*ix) else {
                        continue;
                    };
                    // The matched letters are the name's; the `@` shifts them by one.
                    let shifted = positions.iter().map(|p| p + 1).collect();
                    let about = command.about.lines().next().unwrap_or("").to_string();
                    (pick, format!("@{}", command.name), shifted, Some(about))
                }
            };
            pick += 1;
            let selected = this == at.selected;
            let el = div()
                .id(("agent-at", this))
                .px(px(8.))
                .py(px(3.))
                .gap(px(10.))
                .flex()
                .rounded(px(4.))
                .when(selected, |d| d.bg(hsla(c.panel_hover)))
                .text_color(hsla(c.panel_text))
                .cursor_pointer()
                .on_click(cx.listener(move |t, _, _, cx| t.pick_at(Some(this), cx)));
            rows.push(match about {
                None => el.child(SharedString::from(label)).into_any_element(),
                Some(about) => el
                    .child(
                        div()
                            .flex()
                            .flex_none()
                            .w(px(SLASH_NAME_W))
                            .child(name_cell(&label, &positions, selected)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(hsla(c.panel_dim))
                            .child(SharedString::from(about)),
                    )
                    .into_any_element(),
            });
        }
        let file_note = if self.listing {
            Some("Listing files\u{2026}")
        } else if at.query.is_empty() {
            Some("Type to find a file")
        } else if at.hits.is_empty() {
            Some("No file matches")
        } else {
            None
        };
        // Agents and skills come from the agent: its list is not made up while it is unknown.
        let list_note = at_list_note(&lookup);
        let note = |n: &'static str| {
            div()
                .px(px(8.))
                .py(px(3.))
                .text_color(hsla(c.panel_dim))
                .child(n)
        };
        let panel = div()
            .w_full()
            .mb(px(8.))
            .p(px(4.))
            .rounded(px(7.))
            .bg(hsla(c.panel))
            .border_1()
            .border_color(hsla(c.panel_border))
            .font_family(mono)
            .text_size(px(text::META + 0.5))
            .flex()
            .flex_col()
            .child(
                div()
                    .id("agent-at")
                    .flex()
                    .flex_col()
                    .max_h(px(SLASH_MAX_H))
                    .overflow_y_scroll()
                    .track_scroll(&self.slash_scroll)
                    .children(rows),
            )
            .children(file_note.map(note))
            .children(list_note.map(note));
        Some(self.marked("at", div().w_full().child(panel)))
    }

    /// The `/` list's rows as elements: a heading, or a command (its pick number is how many
    /// commands come before it).
    fn slash_row_els(
        &self,
        commands: &[AgentCommand],
        rows: &[SlashRow],
        selected: usize,
        c: &Colors,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut pick = 0;
        let mut els = Vec::new();
        for row in rows {
            match row {
                SlashRow::Heading(kind) => {
                    let at = match kind {
                        CommandKind::Command => 0,
                        CommandKind::Skill => 1,
                        CommandKind::Agent => 2,
                    };
                    els.push(
                        div()
                            .px(px(8.))
                            .pt(px(4.))
                            .pb(px(1.))
                            .text_size(px(text::META))
                            .text_color(hsla(c.panel_dim))
                            .child(SLASH_HEADINGS[at])
                            .into_any_element(),
                    );
                }
                SlashRow::Item { ix, positions } => {
                    let Some(command) = commands.get(*ix) else {
                        continue;
                    };
                    let this = pick;
                    pick += 1;
                    // The matched letters are the name's; the `/` shifts them by one.
                    let shifted: Vec<usize> = positions.iter().map(|p| p + 1).collect();
                    let about = command.about.lines().next().unwrap_or("").to_string();
                    els.push(
                        div()
                            .id(("agent-slash-row", this))
                            .px(px(8.))
                            .py(px(3.))
                            .gap(px(10.))
                            .flex()
                            .rounded(px(4.))
                            .when(this == selected, |d| d.bg(hsla(c.panel_hover)))
                            .text_color(hsla(c.panel_text))
                            .cursor_pointer()
                            .on_click(cx.listener(move |this_tab, _, _, cx| {
                                this_tab.pick_slash(Some(this), cx);
                            }))
                            .child(
                                div()
                                    .flex()
                                    .flex_none()
                                    .w(px(SLASH_NAME_W))
                                    .child(name_cell(
                                        &format!("/{}", command.name),
                                        &shifted,
                                        this == selected,
                                    )),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_color(hsla(c.panel_dim))
                                    .child(SharedString::from(about)),
                            )
                            .into_any_element(),
                    );
                }
            }
        }
        els
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
            let label = a.label();
            chip(
                format!("{} {}", a.glyph(), label.lines().next().unwrap_or("")),
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
}

#[path = "agent_box_menus.rs"]
mod menus;

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

    /// The `@` list's rows as drawn now (headings, files, agents and skills).
    pub fn shot_at_rows(&self, cx: &App) -> Vec<AtRow> {
        self.at_rows_now(cx)
    }

    /// The paths of the `@` list's ranked files, in order.
    pub fn shot_at_paths(&self) -> Vec<String> {
        self.at
            .as_ref()
            .map_or_else(Vec::new, |a| a.hits.iter().map(|h| h.rel.clone()).collect())
    }

    /// Whether the `@` or `/` list is scrolled off its top.
    pub fn shot_list_scrolled(&self) -> bool {
        self.slash_scroll.offset().y != gpui::px(0.)
    }

    /// The `@` list's selected pick (none: it is closed).
    pub fn shot_at_selected(&self) -> Option<usize> {
        self.at.as_ref().map(|a| a.selected)
    }

    /// What the `@` list says of its agents and skills now.
    pub fn shot_at_note(&self, cx: &App) -> Option<&'static str> {
        self.at.as_ref()?;
        at_list_note(&self.catalog_lookup(cx))
    }

    /// The `/` list's text and its selected pick (none: it is closed).
    pub fn shot_slash(&self) -> Option<(String, usize)> {
        self.slash.as_ref().map(|l| (l.query.clone(), l.selected))
    }

    /// What the `/` list says in place of rows now.
    pub fn shot_slash_note(&self, cx: &App) -> Option<&'static str> {
        self.slash.as_ref()?;
        Some(slash_note(
            &self.catalog_lookup(cx),
            &self.slash_rows_now(cx),
        ))
        .flatten()
    }

    /// The `/` list's rows as drawn now (headings and commands).
    pub fn shot_slash_rows(&self, cx: &App) -> Vec<SlashRow> {
        self.slash_rows_now(cx)
    }

    /// Where the `/` list's row number `row` (headings count) was last laid out.
    pub fn shot_slash_row(&self, row: usize) -> Option<gpui::Bounds<gpui::Pixels>> {
        self.slash_scroll.bounds_for_item(row)
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

    /// The mode menu's rows as drawn now (what `mode_rows` makes of the mode in force).
    pub fn shot_mode_rows(&self, cx: &App) -> Vec<ModeRow> {
        self.mode_rows_now(cx)
    }

    /// The usage the agent's View Usage shows, as if it had been read now, and what the agent
    /// said of its limits.
    pub fn shot_usage(
        &mut self,
        found: Lookup<Vec<UsageGroup>>,
        limits: Vec<RateLimit>,
        cx: &mut Context<Self>,
    ) {
        if let Some(provider) = self.provider {
            let agent = provider.info().name;
            cx.update_default_global(|c: &mut Catalogs, _| {
                c.set_usage(agent, found, std::time::Instant::now());
                for limit in limits {
                    c.note_limit(agent, limit);
                }
            });
        }
        cx.notify();
    }

    /// Make the reading `secs` old (as if it had been read that long ago).
    pub fn shot_age_usage(&mut self, secs: u64, cx: &mut Context<Self>) {
        if let Some(provider) = self.provider {
            let agent = provider.info().name;
            cx.update_default_global(|c: &mut Catalogs, _| {
                if let Some(found) = c.usage_last(agent).cloned() {
                    let then = std::time::Instant::now()
                        .checked_sub(std::time::Duration::from_secs(secs))
                        .expect("an instant that far back");
                    c.set_usage(agent, found, then);
                }
            });
        }
        cx.notify();
    }

    /// The usage lines as drawn now.
    pub fn shot_usage_rows(&self, cx: &App) -> Vec<UsageRow> {
        self.usage_rows_now(cx)
    }

    /// The usage the agent's catalog holds now (none: not asked, or stale).
    pub fn shot_usage_lookup(&self, cx: &App) -> Option<Lookup<Vec<UsageGroup>>> {
        let agent = self.provider?.info().name;
        cx.try_global::<Catalogs>()?
            .usage(agent, std::time::Instant::now())
            .cloned()
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

    fn command(name: &str, kind: CommandKind, aliases: &[&str]) -> AgentCommand {
        AgentCommand {
            name: name.into(),
            about: format!("About {name}"),
            kind,
            aliases: aliases.iter().map(|a| a.to_string()).collect(),
        }
    }

    #[test]
    fn slash_query_only_at_the_start() {
        assert_eq!(slash_query("/mo", 3), Some("mo"));
        assert_eq!(slash_query("/", 1), Some(""));
        assert_eq!(
            slash_query("/model", 3),
            Some("mo"),
            "the caret inside the word"
        );
        assert_eq!(slash_query("/model", 6), Some("model"));
        assert_eq!(slash_query("a /mo", 5), None, "not at the start");
        assert_eq!(slash_query(" /mo", 4), None, "not at the start");
        assert_eq!(
            slash_query("/model x", 8),
            None,
            "caret past the first word"
        );
        assert_eq!(slash_query("/model x", 9), None);
        assert_eq!(slash_query("/model ", 7), None, "the space ends the word");
        assert_eq!(
            slash_query("/model x", 3),
            Some("mo"),
            "caret still in the word"
        );
        assert_eq!(slash_query("/mo", 0), None, "caret before the slash");
        assert_eq!(slash_query("", 0), None);
        assert_eq!(slash_query("/a\nb", 4), None);
    }

    #[test]
    fn at_rows_have_sections_in_order() {
        use CommandKind::{Agent, Command, Skill};
        let file = |rel: &str| FileHit {
            rel: rel.into(),
            positions: Vec::new(),
            score: 0,
        };
        let files = vec![file("src/a.rs"), file("src/b.rs")];
        let list = vec![
            command("model", Command, &[]),
            command("example-skill", Skill, &[]),
            command("example-agent", Agent, &[]),
            command("example-review", Skill, &[]),
        ];
        let named = |ix: usize| AtRow::Named {
            ix,
            positions: Vec::new(),
        };
        // Files, then Agents, then Skills; commands are not in it.
        assert_eq!(
            at_rows(&files, &list, ""),
            vec![
                AtRow::Heading("Files"),
                AtRow::File(0),
                AtRow::File(1),
                AtRow::Heading("Agents"),
                named(2),
                AtRow::Heading("Skills"),
                named(1),
                named(3),
            ]
        );
        // A section with nothing is left out.
        assert_eq!(
            at_rows(&[], &list, "agent"),
            vec![
                AtRow::Heading("Agents"),
                AtRow::Named {
                    ix: 2,
                    positions: vec![8, 9, 10, 11, 12]
                }
            ]
        );
        assert_eq!(
            at_rows(&files, &[], ""),
            vec![AtRow::Heading("Files"), AtRow::File(0), AtRow::File(1)]
        );
        assert!(at_rows(&[], &list, "zzz").is_empty());
        // At most five of each.
        let many: Vec<AgentCommand> = (0..7)
            .map(|i| command(&format!("skill-{i}"), Skill, &[]))
            .collect();
        assert_eq!(at_rows(&[], &many, "").len(), 1 + AT_NAMED_LIMIT);
    }

    #[test]
    fn slash_rows_group_and_filter() {
        use CommandKind::{Agent, Command, Skill};
        let list = vec![
            command("model", Command, &[]),
            command("example-skill", Skill, &[]),
            command("compact", Command, &["squeeze"]),
            command("example-agent", Agent, &[]),
            command("clear", Command, &["reset"]),
            command("example-review", Skill, &[]),
        ];
        let item = |ix: usize| SlashRow::Item {
            ix,
            positions: Vec::new(),
        };
        let ix_of = |rows: &[SlashRow]| -> Vec<usize> {
            rows.iter()
                .filter_map(|r| match r {
                    SlashRow::Item { ix, .. } => Some(*ix),
                    SlashRow::Heading(_) => None,
                })
                .collect()
        };
        // No query: every group in the order Commands, Skills, Agents, each in the list's order.
        let all = slash_rows(&list, "", 8);
        assert_eq!(
            all,
            vec![
                SlashRow::Heading(Command),
                item(0),
                item(2),
                item(4),
                SlashRow::Heading(Skill),
                item(1),
                item(5),
                SlashRow::Heading(Agent),
                item(3),
            ]
        );
        // A query keeps what matches (the name's letters are marked); a group with none has
        // no heading.
        let c = slash_rows(&list, "cl", 8);
        assert_eq!(
            c,
            vec![
                SlashRow::Heading(Command),
                SlashRow::Item {
                    ix: 4,
                    positions: vec![0, 1]
                },
            ]
        );
        // An alias matches too (nothing of the name is marked).
        let by_alias = slash_rows(&list, "reset", 8);
        assert_eq!(ix_of(&by_alias), [4]);
        assert_eq!(by_alias[1], item(4));
        // Best first: the exact name leads.
        let best = slash_rows(&list, "exa", 8);
        assert_eq!(ix_of(&best), [1, 5, 3]);
        // At most `limit` of a group.
        assert_eq!(ix_of(&slash_rows(&list, "", 2)), [0, 2, 1, 5, 3]);
        // Nothing matches: no rows at all.
        assert!(slash_rows(&list, "zzz", 8).is_empty());
        assert!(slash_rows(&[], "", 8).is_empty());
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
    fn mode_rows_list_the_modes_with_the_current_ticked() {
        let words = |rows: &[ModeRow]| -> Vec<String> {
            rows.iter()
                .map(|r| match r {
                    ModeRow::Heading => "Mode".into(),
                    ModeRow::Mode { label, current, .. } => {
                        format!("{}{label}", if *current { "+" } else { "" })
                    }
                    ModeRow::Note(n) => n.to_string(),
                    ModeRow::NewConversation => "New conversation".into(),
                })
                .collect()
        };
        let modes = CLAUDE.info().modes;
        let live = mode_rows(modes, Some("plan"), Applies::Live);
        assert_eq!(
            words(&live),
            ["Mode", "Ask before edits", "Accept edits", "+Plan", "Auto"]
        );
        assert!(
            live.iter().all(|r| match r {
                ModeRow::Mode { about, .. } => !about.is_empty(),
                _ => true,
            }),
            "each mode says what it does"
        );
        assert_eq!(
            words(&mode_rows(modes, None, Applies::Restart))[1..],
            ["Ask before edits", "Accept edits", "Plan", "Auto"],
            "none known: none ticked; a restart is no note"
        );
        assert_eq!(
            words(&mode_rows(modes, Some("plan"), Applies::NextConversation)),
            [
                "Mode",
                "applies to a new conversation",
                "Ask before edits",
                "Accept edits",
                "+Plan",
                "Auto",
                "New conversation"
            ]
        );
    }

    #[test]
    fn the_mode_in_force_is_chosen_then_reported_then_default() {
        assert_eq!(
            current_mode(Some("plan"), Some("auto"), Some("x")),
            Some("plan")
        );
        assert_eq!(current_mode(None, Some("auto"), Some("x")), Some("auto"));
        assert_eq!(current_mode(None, None, Some("x")), Some("x"));
        assert_eq!(current_mode(None, None, None), None);
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

    fn rate(name: &str, state: LimitState, resets_at: Option<i64>) -> RateLimit {
        RateLimit {
            limit: name.into(),
            state,
            resets_at,
        }
    }

    #[test]
    fn resets_in_words() {
        assert_eq!(resets_in(316_800), "Resets in 3d 16h");
        assert_eq!(resets_in(7_500), "Resets in 2h 5m");
        assert_eq!(resets_in(3_540), "Resets in 59m");
        assert_eq!(resets_in(0), "Resets now");
        assert_eq!(resets_in(-5), "Resets now");
        // The boundaries: a minute short of an hour, the hour, a day, a day and an hour.
        assert_eq!(resets_in(3_599), "Resets in 59m");
        assert_eq!(resets_in(3_600), "Resets in 1h");
        assert_eq!(resets_in(86_399), "Resets in 23h 59m");
        assert_eq!(resets_in(86_400), "Resets in 1d");
        assert_eq!(resets_in(90_000), "Resets in 1d 1h");
        assert_eq!(
            resets_in(90_059),
            "Resets in 1d 1h",
            "the minutes drop from a day"
        );
        // Under a minute is still a minute left, not "now".
        assert_eq!(resets_in(1), "Resets in 1m");
        assert_eq!(resets_in(59), "Resets in 1m");
        assert_eq!(resets_in(60), "Resets in 1m");
    }

    #[test]
    fn reset_text_counts_a_moment_down_and_keeps_the_agents_words() {
        assert_eq!(
            reset_text(&Reset::At(1_000 + 7_500), 1_000),
            "Resets in 2h 5m"
        );
        assert_eq!(reset_text(&Reset::At(1_000), 1_000), "Resets now");
        assert_eq!(
            reset_text(&Reset::At(500), 1_000),
            "Resets now",
            "a past time"
        );
        assert_eq!(
            reset_text(&Reset::Words("Nov 17 at 8:10pm".into()), 0),
            "Resets Nov 17 at 8:10pm"
        );
    }

    #[test]
    fn limit_lines() {
        assert_eq!(limit_line(&rate("s", LimitState::Fine, None), 0), None);
        assert_eq!(
            limit_line(&rate("s", LimitState::Close, Some(1)), 0).as_deref(),
            Some("Close to limit")
        );
        // 14:00 local at +7 h.
        let at = 7 * 3600;
        assert_eq!(
            limit_line(&rate("s", LimitState::Reached, Some(at)), 7 * 3600).as_deref(),
            Some("Limit reached \u{b7} resets 14:00")
        );
        assert_eq!(
            limit_line(&rate("s", LimitState::Reached, None), 0).as_deref(),
            Some("Limit reached")
        );
    }

    #[test]
    fn the_chip_warns_while_a_limit_is_close_or_reached() {
        assert!(!limits_warn(&[], 0));
        assert!(!limits_warn(&[rate("a", LimitState::Fine, None)], 0));
        assert!(limits_warn(
            &[
                rate("a", LimitState::Fine, None),
                rate("b", LimitState::Close, None)
            ],
            0
        ));
        assert!(limits_warn(&[rate("a", LimitState::Reached, None)], 0));
        // A limit whose reset has come is fine again (no dot), the moment itself included.
        let gone = [rate("a", LimitState::Reached, Some(100))];
        assert!(limits_warn(&gone, 99));
        assert!(!limits_warn(&gone, 100));
        assert!(!limits_warn(&gone, 500));
    }

    #[test]
    fn a_limit_past_its_reset_says_nothing() {
        let found = Lookup::Ready(vec![group("", vec![read("Session", 50, None)])]);
        let said = [
            rate("Session", LimitState::Reached, Some(100)),
            rate("Opus", LimitState::Close, Some(100)),
        ];
        assert_eq!(
            usage_rows(Some(&found), &said, None, 500, 0),
            vec![limit_row("Session", Some(50), None, None)],
            "no note, no reset, no line of its own"
        );
    }

    fn read(name: &str, left: u8, resets: Option<Reset>) -> UsageLimit {
        UsageLimit::Read {
            name: name.into(),
            left_pct: left,
            resets,
        }
    }

    fn group(name: &str, limits: Vec<UsageLimit>) -> UsageGroup {
        UsageGroup {
            name: name.into(),
            about: None,
            limits,
        }
    }

    fn limit_row(
        name: &str,
        left: Option<u8>,
        resets: Option<&str>,
        note: Option<&str>,
    ) -> UsageRow {
        UsageRow::Limit {
            name: name.into(),
            left_pct: left,
            resets: resets.map(String::from),
            note: note.map(String::from),
        }
    }

    #[test]
    fn usage_rows_say_checking_failed_and_unavailable() {
        let quiet = |t: &str| vec![UsageRow::Quiet(t.into())];
        assert_eq!(usage_rows(None, &[], None, 0, 0), quiet(CHECKING));
        assert_eq!(
            usage_rows(Some(&Lookup::Loading), &[], None, 0, 0),
            quiet(CHECKING)
        );
        assert_eq!(
            usage_rows(Some(&Lookup::Failed("why".into())), &[], None, 0, 0),
            quiet(USAGE_FAILED)
        );
        assert_eq!(
            usage_rows(Some(&Lookup::Ready(Vec::new())), &[], None, 0, 0),
            quiet(USAGE_FAILED),
            "a reading with nothing in it"
        );
        assert_eq!(
            usage_rows(None, &[], Some("Claude"), 0, 0),
            quiet("Usage isn't available for Claude")
        );
    }

    #[test]
    fn usage_rows_list_each_group_and_limit() {
        let found = Lookup::Ready(vec![
            group(
                "Pro models",
                vec![
                    read("Daily", 63, Some(Reset::At(10_000 + 316_800))),
                    UsageLimit::Text("Mystery".into()),
                ],
            ),
            group(
                "",
                vec![read(
                    "Session",
                    100,
                    Some(Reset::Words("Nov 17 at 8:10pm".into())),
                )],
            ),
        ]);
        assert_eq!(
            usage_rows(Some(&found), &[], None, 10_000, 0),
            vec![
                UsageRow::Heading("Pro models".into()),
                limit_row("Daily", Some(63), Some("Resets in 3d 16h"), None),
                UsageRow::Text("Mystery".into()),
                limit_row("Session", Some(100), Some("Resets Nov 17 at 8:10pm"), None),
            ]
        );
    }

    #[test]
    fn usage_rows_put_what_the_agent_said_under_its_limit() {
        let found = Lookup::Ready(vec![group(
            "",
            vec![read("Session", 4, None), read("Weekly", 80, None)],
        )]);
        let said = [
            rate("Session", LimitState::Reached, Some(7 * 3600)),
            rate("Weekly", LimitState::Fine, Some(1)),
            rate("Opus", LimitState::Close, Some(1_000 + 3_540)),
        ];
        assert_eq!(
            usage_rows(Some(&found), &said, None, 1_000, 7 * 3600),
            vec![
                // No reset in the reading: the agent's moment fills it in.
                limit_row(
                    "Session",
                    Some(4),
                    Some("Resets in 6h 43m"),
                    Some("Limit reached \u{b7} resets 14:00")
                ),
                // Fine says nothing, and a Fine limit's old moment isn't shown as a reset.
                limit_row("Weekly", Some(80), None, None),
                // A limit the reading doesn't have, on its own.
                limit_row("Opus", None, Some("Resets in 59m"), Some("Close to limit")),
            ]
        );
        // Without a reading, the limits that are close or reached still show.
        assert_eq!(
            usage_rows(Some(&Lookup::Failed("x".into())), &said, None, 1_000, 0)
                .iter()
                .filter(|r| matches!(r, UsageRow::Limit { .. }))
                .count(),
            2
        );
    }
}
