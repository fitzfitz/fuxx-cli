//! The agent box (M11ak): the chips under an agent tab's conversation that choose its model,
//! effort, mode and folder. This file holds the pure part: what a choice is (`Change`), when it
//! reaches the agent (`step_for`), the line a restart leaves (`switched_notice`) and what the
//! chips say (`chip_model_label`, `model_chip_text`, `mode_chip_text`, `usage_line`). The tab
//! (`agent_tab.rs`) carries it out.

use std::path::{Path, PathBuf};

use gpui::{
    Anchor, AnyElement, App, Context, Div, Focusable, MouseButton, MouseDownEvent, SharedString,
    Stateful, Window, anchored, deferred, div, point, prelude::*, px,
};

use crate::agent_catalog::{Catalogs, Lookup};
use crate::agent_tab::{AgentTab, Colors, button, chip};
use crate::appearance::Appearance;
use crate::providers::{Applies, Catalog, ModeChoice, Provider, Start};
use crate::terminal_view::hsla;
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

/// The send button's glyph.
const SEND: &str = "\u{27a4}";

/// Where each part of the box was laid out, by name (scenes read it: `shot_part`).
#[cfg(feature = "ui-shots")]
pub type Parts = std::rc::Rc<
    std::cell::RefCell<std::collections::HashMap<&'static str, gpui::Bounds<gpui::Pixels>>>,
>;

/// The box's text while nothing is being asked (§1).
pub const PLACEHOLDER: &str = "Ask anything, @ to mention, / for actions";

/// A menu of the box, open over its chip: the folder picker (and, after it, Move this
/// conversation or a new one), the model menu (with one model's efforts, or View Usage, open
/// beside it), the mode menu, and `+`'s attach menu. One is open at a time.
#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)] // each opened by its chip's task (6–11)
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

/// The folder chip: `📁 <the folder's name>` (shown as text until the picker, Task 6).
pub fn folder_chip_text(folder: &Path) -> String {
    let name = folder
        .file_name()
        .map_or_else(|| folder.to_string_lossy(), |n| n.to_string_lossy());
    format!("\u{1f4c1} {name}")
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

/// A choice made in the box. An effort alone is a `Model` change with the current model's id.
#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    #[allow(dead_code)] // chosen by the model menu (a later M11ak task)
    Model {
        id: String,
        effort: Option<String>,
    },
    Mode(String),
    #[allow(dead_code)] // chosen by the folder chip (a later M11ak task)
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
                    .text_size(px(text::META))
                    .text_color(hsla(c.dim))
                    .child(SharedString::from(texts.folder.clone())),
            )
            .when(
                menu_on(|m| matches!(m, BoxMenu::Folder | BoxMenu::Move(_))),
                |d| d.child(self.menu_frame(cx)),
            );
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
                d.child(self.menu_frame(cx))
            });
        let model = self
            .marked("model", box_chip("agent-model", texts.model.clone(), c))
            .when(menu_on(|m| matches!(m, BoxMenu::Model { .. })), |d| {
                d.child(self.menu_frame(cx))
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
                    d.child(self.menu_frame(cx))
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
                    .child(folder),
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

    /// The open menu's panel, hung above its chip (a child of it) and opening upward, in the
    /// tab menu's look. A click outside it closes it. What each menu lists comes with its own
    /// task (the folder picker, the model, mode and attach menus).
    fn menu_frame(&self, cx: &mut Context<Self>) -> AnyElement {
        let ui = cx.global::<Appearance>().theme.ui();
        let panel = crate::overlay_panel::shell(&ui, MENU_WIDTH)
            .id("agent-box-menu")
            .p(px(4.))
            .gap(px(0.))
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| this.close_menu(cx)));
        div()
            .absolute()
            .top_0()
            .left_0()
            .child(deferred(
                anchored()
                    .anchor(Anchor::BottomLeft)
                    .offset(point(px(0.), px(-MENU_GAP)))
                    .snap_to_window_with_margin(px(MENU_EDGE))
                    .child(self.marked("menu", div().child(panel))),
            ))
            .into_any_element()
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
    use crate::providers::agy::AGY;
    use crate::providers::claude::CLAUDE;
    use crate::providers::{EffortChoice, ModelChoice};
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
    fn the_folder_chip_names_the_folder() {
        assert_eq!(
            folder_chip_text(Path::new("/home/me/code/my-app")),
            "\u{1f4c1} my-app"
        );
        assert_eq!(folder_chip_text(Path::new("/")), "\u{1f4c1} /");
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
