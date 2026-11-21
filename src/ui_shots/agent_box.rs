//! M11ak: the agent box's scenes. As the agent tab's (`agent.rs`), no process runs: the tab is
//! made with `AgentTab::for_shots`, fed decoded events through `shot_feed`, and what it would
//! write to its agent is read back with `shot_written`.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use gpui::{
    AnyWindowHandle, AppContext, Bounds, Context, Entity, Focusable, IntoElement, Modifiers,
    Pixels, VisualTestAppContext, div, prelude::*, px, size,
};

use super::{
    Kind, Scene, Themes, frame_fresh, render_with, shoot_with, shot_appearance, shot_theme,
    shot_themes,
};
use crate::agent_box::{
    AtRow, BoxMenu, CHECKING, Change, ModeRow, ModelRow, PLACEHOLDER, SlashRow, UsageRow,
};
use crate::agent_catalog::{Catalogs, Lookup};
use crate::agent_conversation::Item;
use crate::agent_tab::{AgentTab, AgentTabEvent, Attachment};
use crate::code_ask::{CodeLines, CodeRef};
use crate::folder_picker::{KnownFolders, PickerRow};
use crate::providers::agy::AGY;
use crate::providers::claude::CLAUDE;
use crate::providers::{
    Catalog, ChangeKind, EffortChoice, Event, LimitState, ModelChoice, Provider, RateLimit, Reset,
    Start, UsageGroup, UsageLimit,
};
use crate::status::Status;

/// This file, as `file!()` gives it: `registry::AREAS` pairs it with `SCENES`.
pub(super) const FILE: &str = file!();

/// The files a box change goes through: the tab, the box's rules, the conversation, the
/// providers that say how a change applies and write it.
const BOX: &[&str] = &[
    "src/agent_tab.rs",
    "src/agent_box.rs",
    "src/agent_box_menus.rs",
    "src/agent_conversation.rs",
    "src/providers/mod.rs",
    "src/providers/claude.rs",
    "src/providers/agy.rs",
    "src/providers/fixtures/agy-text.jsonl",
];

/// The files the box is drawn from: the change files, the composer inside it, the catalog its
/// chips read, and the menu panel's look.
const LAYOUT: &[&str] = &[
    "src/agent_tab.rs",
    "src/agent_box.rs",
    "src/agent_box_menus.rs",
    "src/agent_conversation.rs",
    "src/agent_catalog.rs",
    "src/composer.rs",
    "src/overlay_panel.rs",
    "src/providers/mod.rs",
    "src/providers/claude.rs",
];

/// The files the attach menu and its chips are made of: the box's, the composer (a paste), the
/// menu panel's look, and the providers that say how an image is sent.
const ATTACH: &[&str] = &[
    "src/agent_tab.rs",
    "src/agent_box.rs",
    "src/agent_box_menus.rs",
    "src/agent_conversation.rs",
    "src/agent_catalog.rs",
    "src/composer.rs",
    "src/overlay_panel.rs",
    "src/providers/mod.rs",
    "src/providers/claude.rs",
    "src/providers/agy.rs",
];

/// The files the folder picker is made of: the box's, the picker's lists, its search box, and
/// the version and folder lists the providers read.
const FOLDER: &[&str] = &[
    "src/agent_tab.rs",
    "src/agent_box.rs",
    "src/agent_box_menus.rs",
    "src/agent_conversation.rs",
    "src/agent_catalog.rs",
    "src/folder_picker.rs",
    "src/name_input.rs",
    "src/composer.rs",
    "src/overlay_panel.rs",
    "src/providers/mod.rs",
    "src/providers/claude.rs",
];

/// The files the model menu is made of: the box's, the catalog it lists, and the providers that
/// say each model's efforts and how a change applies.
const MODEL: &[&str] = &[
    "src/agent_tab.rs",
    "src/agent_box.rs",
    "src/agent_box_menus.rs",
    "src/agent_conversation.rs",
    "src/agent_catalog.rs",
    "src/composer.rs",
    "src/overlay_panel.rs",
    "src/providers/mod.rs",
    "src/providers/claude.rs",
    "src/providers/agy.rs",
];

/// The files the mode menu is made of: the model menu's (the same menu code and catalog).
const MODE: &[&str] = MODEL;

/// The files View Usage is made of: the model menu's, and the usage the providers read.
const USAGE: &[&str] = MODEL;

/// The files the branch chip is made of: the box's, and the branch look-up and switcher it
/// opens.
const BRANCH: &[&str] = &[
    "src/agent_tab.rs",
    "src/agent_box.rs",
    "src/agent_box_menus.rs",
    "src/agent_catalog.rs",
    "src/composer.rs",
    "src/git_branch.rs",
    "src/branch_switcher.rs",
    "src/workspace_agent.rs",
    "src/providers/mod.rs",
    "src/providers/claude.rs",
];

/// This area's scenes (`registry.rs` runs them in its `ORDER`).
pub(super) const SCENES: &[Scene] = &[
    Scene {
        name: "agent-box-empty",
        run: agent_box_empty,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: LAYOUT,
    },
    Scene {
        name: "agent-box-working",
        run: agent_box_working,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: LAYOUT,
    },
    Scene {
        name: "agent-box-attachment",
        run: agent_box_attachment,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: LAYOUT,
    },
    Scene {
        name: "agent-box-attach-menu",
        run: agent_box_attach_menu,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: ATTACH,
    },
    Scene {
        name: "agent-box-file-chip",
        run: agent_box_file_chip,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: ATTACH,
    },
    Scene {
        name: "agent-box-code-chip",
        run: agent_box_code_chip,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: ATTACH,
    },
    Scene {
        name: "agent-box-image-chip",
        run: agent_box_image_chip,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: ATTACH,
    },
    Scene {
        name: "agent-box-image-agy",
        run: agent_box_image_agy,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: ATTACH,
    },
    Scene {
        name: "agent-box-image-too-big",
        run: agent_box_image_too_big,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: ATTACH,
    },
    Scene {
        name: "agent-box-long-text",
        run: agent_box_long_text,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: LAYOUT,
    },
    Scene {
        name: "agent-box-folder-picker",
        run: agent_box_folder_picker,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: FOLDER,
    },
    Scene {
        name: "agent-box-folder-search",
        run: agent_box_folder_search,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: FOLDER,
    },
    Scene {
        name: "agent-box-folder-locked",
        run: agent_box_folder_locked,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: FOLDER,
    },
    Scene {
        name: "agent-box-folder-before-first",
        run: agent_box_folder_before_first,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: FOLDER,
    },
    Scene {
        name: "agent-box-change-waits",
        run: agent_box_change_waits,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: BOX,
    },
    Scene {
        name: "agent-box-mode-waits",
        run: agent_box_mode_waits,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: BOX,
    },
    Scene {
        name: "agent-box-send-notch",
        run: agent_box_send_notch,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: BOX,
    },
    Scene {
        name: "agent-box-terminal-text",
        run: agent_box_terminal_text,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: BOX,
    },
    Scene {
        name: "agent-box-shell-keys",
        run: agent_box_shell_keys,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: BOX,
    },
    Scene {
        name: "agent-box-change-restart",
        run: agent_box_change_restart,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: BOX,
    },
    Scene {
        name: "agent-box-restart-keeps-queued-text",
        run: agent_box_restart_keeps_queued_text,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: BOX,
    },
    Scene {
        name: "agent-box-model-menu",
        run: agent_box_model_menu,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: MODEL,
    },
    Scene {
        name: "agent-box-effort-list",
        run: agent_box_effort_list,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: MODEL,
    },
    Scene {
        name: "agent-box-model-agy",
        run: agent_box_model_agy,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: MODEL,
    },
    Scene {
        name: "agent-box-model-loading",
        run: agent_box_model_loading,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: MODEL,
    },
    Scene {
        name: "agent-box-model-failed",
        run: agent_box_model_failed,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: MODEL,
    },
    Scene {
        name: "agent-box-model-fallback",
        run: agent_box_model_fallback,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: MODEL,
    },
    Scene {
        name: "agent-box-model-choose-sends-nothing",
        run: agent_box_model_choose_sends_nothing,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: MODEL,
    },
    Scene {
        name: "agent-box-model-effort-composes",
        run: agent_box_model_effort_composes,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: MODEL,
    },
    Scene {
        name: "agent-box-usage-claude",
        run: agent_box_usage_claude,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: USAGE,
    },
    Scene {
        name: "agent-box-usage-agy",
        run: agent_box_usage_agy,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: USAGE,
    },
    Scene {
        name: "agent-box-usage-checking",
        run: agent_box_usage_checking,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: USAGE,
    },
    Scene {
        name: "agent-box-usage-text-line",
        run: agent_box_usage_text_line,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: USAGE,
    },
    Scene {
        name: "agent-box-usage-limit-close",
        run: agent_box_usage_limit_close,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: USAGE,
    },
    Scene {
        name: "agent-box-usage-cached",
        run: agent_box_usage_cached,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: USAGE,
    },
    Scene {
        name: "agent-box-slash",
        run: agent_box_slash,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: SLASH,
    },
    Scene {
        name: "agent-box-slash-failed-return-sends",
        run: agent_box_slash_failed_return_sends,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: SLASH,
    },
    Scene {
        name: "agent-box-slash-filtered",
        run: agent_box_slash_filtered,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: SLASH,
    },
    Scene {
        name: "agent-box-slash-tab-completes",
        run: agent_box_slash_tab_completes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: SLASH,
    },
    Scene {
        name: "agent-box-slash-usage-opens-menu",
        run: agent_box_slash_usage_opens_menu,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: SLASH,
    },
    Scene {
        name: "agent-box-at-sections",
        run: agent_box_at_sections,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: SLASH,
    },
    Scene {
        name: "agent-box-card",
        run: agent_box_card,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: LAYOUT,
    },
    Scene {
        name: "agent-box-model-waiting-no-efforts",
        run: agent_box_model_waiting_no_efforts,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: MODEL,
    },
    Scene {
        name: "agent-box-mode-menu",
        run: agent_box_mode_menu,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: MODE,
    },
    Scene {
        name: "agent-box-mode-agy-after-message",
        run: agent_box_mode_agy_after_message,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: MODE,
    },
    Scene {
        name: "agent-box-mode-reported",
        run: agent_box_mode_reported,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: MODE,
    },
    Scene {
        name: "agent-box-mode-follows-claude",
        run: agent_box_mode_follows_claude,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: MODE,
    },
    Scene {
        name: "agent-box-branch",
        run: agent_box_branch,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: BRANCH,
    },
    Scene {
        name: "agent-box-branch-opens-switcher",
        run: agent_box_branch_opens_switcher,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: BRANCH,
    },
];

type Slot = Rc<RefCell<Option<Entity<AgentTab>>>>;
/// The settings the tab told (`AgentTabEvent::Settings`): model, effort, mode.
type Told = Rc<RefCell<Vec<(Option<String>, Option<String>, Option<String>)>>>;

/// A folder under your home (`~/code/my-app`); nothing is read from it.
fn cwd() -> PathBuf {
    crate::workspace::home_dir()
        .unwrap_or_else(|| PathBuf::from("/home/me"))
        .join("code/my-app")
}

fn start(mode: Option<&str>, model: Option<&str>) -> Start {
    Start {
        cwd: cwd(),
        resume: None,
        new_id: None,
        mode: mode.map(String::from),
        model: model.map(String::from),
        effort: None,
        extra_args: Vec::new(),
    }
}

fn feed(
    cx: &mut VisualTestAppContext,
    tab: &Entity<AgentTab>,
    f: impl FnOnce(&mut AgentTab, &mut Context<AgentTab>),
) {
    cx.update(|cx| tab.update(cx, f));
    cx.run_until_parked();
}

/// Checks only: a `provider` tab with no process, `start` given, its history in (its process
/// "started": `shot_last_argv` is what it would run); the settings it tells are kept.
fn run_scene(
    name: &str,
    provider: &'static dyn Provider,
    start: Start,
    act: impl FnOnce(&str, &mut VisualTestAppContext, &Entity<AgentTab>, &Told) + 'static,
) {
    let theme = shot_theme("Catppuccin Mocha");
    let slot: Slot = Rc::default();
    let keep = slot.clone();
    let told: Told = Rc::default();
    let heard = told.clone();
    let n = name.to_string();
    render_with(
        &theme,
        size(px(720.), px(640.)),
        move |window, cx| {
            cx.set_global(shot_appearance(theme, cx));
            let tab = cx.new(|cx| AgentTab::for_shots(provider, start, window, cx));
            cx.subscribe(&tab, move |_, e: &AgentTabEvent, _| {
                if let AgentTabEvent::Settings {
                    model,
                    effort,
                    mode,
                } = e
                {
                    heard
                        .borrow_mut()
                        .push((model.clone(), effort.clone(), mode.clone()));
                }
            })
            .detach();
            *keep.borrow_mut() = Some(tab.clone());
            tab.into()
        },
        move |cx, _| {
            let tab = slot.borrow().clone().expect("the agent tab");
            feed(cx, &tab, |t, cx| t.shot_history(Vec::new(), cx));
            act(&n, cx, &tab, &told);
            slot.borrow_mut().take();
        },
    );
}

fn written(cx: &VisualTestAppContext, tab: &Entity<AgentTab>) -> Vec<serde_json::Value> {
    cx.read(|cx| tab.read(cx).shot_written())
        .iter()
        .map(|l| serde_json::from_str(l).expect("a JSON line"))
        .collect()
}

fn argv(cx: &VisualTestAppContext, tab: &Entity<AgentTab>) -> Vec<String> {
    cx.read(|cx| tab.read(cx).shot_last_argv())
}

/// The word after `flag` in the last command line.
fn flag(argv: &[String], flag: &str) -> Option<String> {
    argv.windows(2).find(|w| w[0] == flag).map(|w| w[1].clone())
}

fn notices(cx: &VisualTestAppContext, tab: &Entity<AgentTab>) -> Vec<String> {
    cx.read(|cx| {
        tab.read(cx)
            .shot_items()
            .iter()
            .filter_map(|i| match i {
                Item::Notice(t) => Some(t.clone()),
                _ => None,
            })
            .collect()
    })
}

fn said(cx: &VisualTestAppContext, tab: &Entity<AgentTab>) -> Vec<String> {
    cx.read(|cx| {
        tab.read(cx)
            .shot_items()
            .iter()
            .filter_map(|i| match i {
                Item::You { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    })
}

fn turn_done() -> Event {
    Event::TurnDone {
        ok: true,
        message: None,
    }
}

fn agy_text() -> Vec<Event> {
    let mut t = AGY.translator();
    include_str!("../providers/fixtures/agy-text.jsonl")
        .lines()
        .flat_map(|l| t.decode(l))
        .collect()
}

/// The conversation agy's text recording started.
fn agy_conversation() -> String {
    agy_text()
        .iter()
        .find_map(|e| match e {
            Event::Started { conversation, .. } => Some(conversation.clone()),
            _ => None,
        })
        .expect("agy-text starts a conversation")
}

/// Your words: "can you make the button on the bottom right to be like this? its outside but we
/// play radius on the inner box on the bottom right". The designer's details: send a 34 circle
/// 3 in from the outer box's inside right and bottom, its centre level with the controls'; the
/// mode pill ends 14 before it (the cut 6 round the circle: "add gap between chatbox with button"); the notch stays at the bottom-right as the box grows.
pub(super) fn agent_box_send_notch() {
    layout_scene("agent-box-send-notch", |n, cx, window, tab| {
        let near = |a: Pixels, b: Pixels| (a - b).abs() <= px(0.5);
        let check = |cx: &mut VisualTestAppContext, when: &str| {
            let (outer, send) = (part(cx, tab, n, "outer"), part(cx, tab, n, "send"));
            let (attach, mode) = (part(cx, tab, n, "attach"), part(cx, tab, n, "mode"));
            assert_eq!(send.size, size(px(34.), px(34.)), "{n} {when}: 34 across");
            // (Inside the outer box's 1 px edge, the box's gap in: `OUTER_INSET`.)
            let inset = px(crate::card_view::EDGE + crate::card_view::OUTER_INSET);
            assert!(
                near(outer.right() - send.right(), inset),
                "{n} {when}: right {outer:?} {send:?}"
            );
            assert!(
                near(outer.bottom() - send.bottom(), inset),
                "{n} {when}: bottom"
            );
            // (Within a pixel: layout lands on whole pixels.)
            assert!(
                (send.center().y - attach.center().y).abs() <= px(1.),
                "{n} {when}: level with the controls ({:?} {:?})",
                send.center().y,
                attach.center().y
            );
            assert!(
                (send.left() - mode.right() - px(14.)).abs() <= px(1.),
                "{n} {when}: the mode pill 14 before it ({:?} {:?})",
                send.left(),
                mode.right()
            );
        };
        check(cx, "empty");
        let line = "Please read src/rsvp.ts and the failing test, then explain why past dates \
                    are accepted. ";
        cx.simulate_input(window, &line.repeat(3));
        let _ = frame_fresh(cx, window);
        let (rows, _, _) = cx.read(|cx| tab.read(cx).shot_composer().read(cx).shot_rows());
        assert!(rows > 1, "{n}: the box grew ({rows} rows)");
        check(cx, "grown");
        assert!(written(cx, tab).is_empty(), "{n}: nothing sent");
    });
}

/// Your words: "Because right now, you see the differences? also the cursor is very different"
/// (the agent box beside the shell box). The designer's details: the typing in the terminal's
/// font, size and cell height; the caret the terminal's cursor (the setting's shape, one cell,
/// hollow without focus); a `❯` 14 in from the box's left, the typing two cells after it, the
/// first line 12 down; the placeholder one cell after the caret's cell.
pub(super) fn agent_box_terminal_text() {
    use crate::appearance::{Appearance, CursorShapeSetting};
    use crate::cursor_look::PaintShape;
    layout_scene("agent-box-terminal-text", |n, cx, window, tab| {
        // Positions first, without motion (the glide would catch the caret mid-way).
        cx.update(|cx| {
            cx.update_global::<Appearance, _>(|a, _| {
                a.cursor_style = CursorShapeSetting::Block;
                a.animations = false;
            })
        });
        let composer = cx.read(|cx| tab.read(cx).shot_composer());
        cx.update(|cx| composer.update(cx, |c, _| c.shot_be_key()));
        let text = composer_bounds(cx, tab);
        cx.simulate_click(window, text.center(), Modifiers::none());
        cx.run_until_parked();
        let _ = frame_fresh(cx, window);
        let font_size = cx.read(|cx| cx.global::<Appearance>().font_size);
        let (cell_w, line_h) = cx.read(|cx| composer.read(cx).shot_cell());
        assert_eq!(
            f32::from(line_h),
            (font_size * 1.3).round(),
            "{n}: the terminal's cell height"
        );
        let text = composer_bounds(cx, tab);
        assert_eq!(text.size.height, line_h, "{n}: one row, a cell tall");
        let caret = |cx: &VisualTestAppContext| cx.read(|cx| composer.read(cx).shot_caret());
        let (shape, at) = caret(cx).unwrap_or_else(|| panic!("{n}: no caret"));
        assert_eq!(shape, PaintShape::Block, "{n}: the setting's shape");
        assert_eq!(
            (at.size.width, at.size.height),
            (cell_w, line_h),
            "{n}: one cell"
        );
        assert_eq!(at.left(), text.left(), "{n}: at the start");
        let (words, spot) = cx
            .read(|cx| composer.read(cx).shot_placeholder())
            .unwrap_or_else(|| panic!("{n}: no placeholder"));
        assert!(
            PLACEHOLDER.starts_with(words.trim_end_matches('\u{2026}')),
            "{n}: {words}"
        );
        assert_eq!(
            spot.left(),
            text.left() + cell_w,
            "{n}: one cell after the caret"
        );
        let (mark, the_box) = (part(cx, tab, n, "mark"), part(cx, tab, n, "box"));
        assert_eq!(mark.left() - the_box.left(), px(14.), "{n}: the mark 14 in");
        // (Layout lands on whole pixels; the cells are fractional.)
        let near = |a: Pixels, b: Pixels| (a - b).abs() <= px(0.5);
        let gap = text.left() - mark.left();
        assert!(
            near(gap, cell_w * 2.),
            "{n}: the typing two cells after ({gap:?})"
        );
        assert_eq!(
            text.top() - the_box.top(),
            px(12.),
            "{n}: the first line 12 down"
        );
        cx.simulate_input(window, "hi");
        let _ = frame_fresh(cx, window);
        let (_, at) = caret(cx).unwrap_or_else(|| panic!("{n}: no caret"));
        assert_eq!(
            at.left(),
            text.left() + cell_w * 2.,
            "{n}: after two letters"
        );
        // The cursor setting changed: the box redraws with it (your words: "it doesn't respect
        // the cursor setting").
        cx.update(|cx| {
            cx.update_global::<Appearance, _>(|a, _| a.cursor_style = CursorShapeSetting::Underline)
        });
        let _ = frame_fresh(cx, window);
        assert_eq!(
            caret(cx).map(|c| c.0),
            Some(PaintShape::Underline),
            "{n}: the new setting's shape"
        );
        // With motion on, a typed letter glides the caret over, frame by frame, as in a pane
        // (your words: "it doesnt glow and smooth like terminal tab").
        cx.update(|cx| {
            cx.update_global::<Appearance, _>(|a, _| {
                a.animations = true;
                a.cursor_glide = true;
            })
        });
        let _ = frame_fresh(cx, window);
        cx.simulate_input(window, "!");
        let _ = frame_fresh(cx, window);
        assert_eq!(
            cx.read(|cx| composer.read(cx).shot_frames()),
            crate::cursor_look::Frames::EveryFrame,
            "{n}: gliding"
        );
        cx.update(|cx| composer.update(cx, |c, _| c.shot_not_key()));
        cx.update_window(window, |_, w, _| w.refresh()).ok();
        let _ = frame_fresh(cx, window);
        assert_eq!(
            caret(cx).map(|c| c.0),
            Some(PaintShape::Hollow),
            "{n}: hollow, not key"
        );
        assert!(written(cx, tab).is_empty(), "{n}: nothing sent");
    });
}

/// Your words: "make sure the font the textbox nature like cursor move with keyboard, delete
/// text/sentences,etc same as terminal textbox": real keystrokes edit the box as they edit the
/// shell's line (⌥⌫, ⌃W, ⌃A, ⌃F, ⌃K, ⌃B, ⌃H, ⌃D, ⌃E, ⌃U).
pub(super) fn agent_box_shell_keys() {
    layout_scene("agent-box-shell-keys", |n, cx, window, tab| {
        cx.simulate_input(window, "git commit -m fix it");
        let _ = frame_fresh(cx, window);
        press(cx, window, "alt-backspace");
        assert_eq!(
            box_text(cx, tab),
            "git commit -m fix ",
            "{n}: \u{2325}\u{232b}"
        );
        press(cx, window, "ctrl-w");
        assert_eq!(box_text(cx, tab), "git commit -m ", "{n}: \u{2303}W");
        press(cx, window, "ctrl-a ctrl-f ctrl-f ctrl-f ctrl-f ctrl-k");
        assert_eq!(
            box_text(cx, tab),
            "git ",
            "{n}: \u{2303}A, \u{2303}F, \u{2303}K"
        );
        press(cx, window, "ctrl-b ctrl-h");
        assert_eq!(box_text(cx, tab), "gi ", "{n}: \u{2303}B, \u{2303}H");
        press(cx, window, "ctrl-d");
        assert_eq!(box_text(cx, tab), "gi", "{n}: \u{2303}D");
        press(cx, window, "ctrl-a ctrl-e");
        cx.simulate_input(window, "t");
        assert_eq!(box_text(cx, tab), "git", "{n}: \u{2303}E to the end");
        press(cx, window, "ctrl-u");
        assert_eq!(box_text(cx, tab), "", "{n}: \u{2303}U");
        assert!(written(cx, tab).is_empty(), "{n}: nothing sent");
    });
}

/// Your words: "Make sure I can choose mode mid run and the next prompt will pick up my chosen
/// mode. same goes model": a mode chosen while Claude works shows at once on the chip and in the
/// menu (it waits for the turn, as a model does), and when the turn ends the mode is asked for
/// before the next message is written.
pub(super) fn agent_box_mode_waits() {
    run_scene(
        "agent-box-mode-waits",
        &CLAUDE,
        start(None, Some("opus")),
        |n, cx, tab, _told| {
            feed(cx, tab, |t, cx| t.send("Fix notes.txt".into(), cx));
            assert_eq!(cx.read(|cx| tab.read(cx).status()), Status::Working, "{n}");
            feed(cx, tab, |t, cx| t.choose(Change::Mode("plan".into()), cx));
            assert_eq!(
                cx.read(|cx| tab.read(cx).shot_box(cx)).mode.as_deref(),
                Some("Plan"),
                "{n}: the chip shows the mode chosen, at once"
            );
            let before = written(cx, tab).len();
            feed(cx, tab, |t, cx| t.shot_feed(vec![turn_done()], cx));
            feed(cx, tab, |t, cx| t.send("Now plan the rest".into(), cx));
            let lines = written(cx, tab);
            let after: Vec<_> = lines[before..].iter().collect();
            let mode_at = after
                .iter()
                .position(|l| l["request"]["subtype"] == "set_permission_mode")
                .unwrap_or_else(|| panic!("{n}: the mode asked for: {after:?}"));
            assert_eq!(after[mode_at]["request"]["mode"], "plan", "{n}");
            let message_at = after
                .iter()
                .position(|l| l["type"] == "user")
                .unwrap_or_else(|| panic!("{n}: the next message: {after:?}"));
            assert!(
                mode_at < message_at,
                "{n}: the mode before the next message"
            );
        },
    );
}

/// Review Focus #1: a change chosen while Claude works waits for the turn — nothing is written
/// until `TurnDone`, then the set request once (never a message); the chip's start flags change
/// only when Claude confirms; a refusal is a notice and changes nothing. Idle, an effort alone
/// is asked as an effort, a model with an effort as both.
pub(super) fn agent_box_change_waits() {
    run_scene(
        "agent-box-change-waits",
        &CLAUDE,
        start(None, Some("opus")),
        |n, cx, tab, told| {
            feed(cx, tab, |t, cx| t.send("Fix notes.txt".into(), cx));
            assert_eq!(cx.read(|cx| tab.read(cx).status()), Status::Working, "{n}");
            let before = written(cx, tab).len();
            let sonnet = Change::Model {
                id: "sonnet".into(),
                effort: None,
            };
            let chosen = sonnet.clone();
            feed(cx, tab, |t, cx| t.choose(chosen, cx));
            assert_eq!(
                written(cx, tab).len(),
                before,
                "{n}: nothing written mid-turn"
            );
            assert_eq!(
                cx.read(|cx| tab.read(cx).waiting_change().cloned()),
                Some(sonnet),
                "{n}: it waits"
            );
            let argv_before = argv(cx, tab);
            // A live change holds nothing back (R5): a note goes at once, and the turn it
            // starts is waited for too.
            feed(cx, tab, |t, cx| t.send("Also check todo.txt".into(), cx));
            let lines = written(cx, tab);
            assert_eq!(lines.len(), before + 1, "{n}: the note is written at once");
            assert_eq!(lines[before]["type"], "user", "{n}");
            assert_eq!(
                cx.read(|cx| tab.read(cx).shot_pending()),
                0,
                "{n}: not held"
            );
            let before = lines.len();
            feed(cx, tab, |t, cx| t.shot_feed(vec![turn_done()], cx));
            assert_eq!(cx.read(|cx| tab.read(cx).status()), Status::Working, "{n}");
            assert_eq!(
                written(cx, tab).len(),
                before,
                "{n}: the note's turn is still to run"
            );
            feed(cx, tab, |t, cx| t.shot_feed(vec![turn_done()], cx));
            let lines = written(cx, tab);
            assert_eq!(lines.len(), before + 1, "{n}: one line after the turn");
            let line = &lines[before];
            assert_eq!(line["type"], "control_request", "{n}: {line}");
            assert_eq!(line["request"]["subtype"], "set_model", "{n}");
            assert_eq!(line["request"]["model"], "sonnet", "{n}");
            assert!(
                lines.iter().filter(|l| l["type"] == "user").count() == 2,
                "{n}: choosing never sends a message (yours: two)"
            );
            assert_eq!(argv(cx, tab), argv_before, "{n}: live, not a restart");
            assert_eq!(cx.read(|cx| tab.read(cx).waiting_change().cloned()), None);
            assert!(told.borrow().is_empty(), "{n}: not until Claude confirms");
            feed(cx, tab, |t, cx| {
                t.shot_feed(
                    vec![Event::Applied {
                        kind: ChangeKind::Model,
                        value: "claude-sonnet-5-5".into(),
                        ok: true,
                    }],
                    cx,
                )
            });
            assert_eq!(
                told.borrow().last().cloned(),
                Some((Some("sonnet".into()), None, None)),
                "{n}: confirmed, the tab starts with it from now on"
            );
            assert!(
                notices(cx, tab).is_empty(),
                "{n}: a live switch leaves no row"
            );

            // A refusal: one notice, nothing changes.
            feed(cx, tab, |t, cx| {
                t.choose(
                    Change::Model {
                        id: "haiku".into(),
                        effort: None,
                    },
                    cx,
                )
            });
            let told_before = told.borrow().len();
            feed(cx, tab, |t, cx| {
                t.shot_feed(
                    vec![Event::Applied {
                        kind: ChangeKind::Model,
                        value: "Unknown model".into(),
                        ok: false,
                    }],
                    cx,
                )
            });
            assert_eq!(notices(cx, tab), ["Couldn't switch: Unknown model"], "{n}");
            assert_eq!(told.borrow().len(), told_before, "{n}: nothing saved");

            // Idle: an effort alone is asked as an effort; a new model with an effort as both.
            let before = written(cx, tab).len();
            feed(cx, tab, |t, cx| {
                t.choose(
                    Change::Model {
                        id: "sonnet".into(),
                        effort: Some("low".into()),
                    },
                    cx,
                )
            });
            let lines = written(cx, tab);
            let subtypes: Vec<&str> = lines[before..]
                .iter()
                .map(|l| l["request"]["subtype"].as_str().unwrap_or(""))
                .collect();
            assert_eq!(subtypes, ["apply_flag_settings", "get_settings"], "{n}");
            let before = lines.len();
            feed(cx, tab, |t, cx| {
                t.choose(
                    Change::Model {
                        id: "opus".into(),
                        effort: Some("max".into()),
                    },
                    cx,
                )
            });
            let lines = written(cx, tab);
            let subtypes: Vec<&str> = lines[before..]
                .iter()
                .map(|l| l["request"]["subtype"].as_str().unwrap_or(""))
                .collect();
            assert_eq!(
                subtypes,
                ["set_model", "apply_flag_settings", "get_settings"],
                "{n}"
            );
        },
    );
}

/// agy after a message, idle: choosing a mode restarts it on the same conversation with the new
/// `--mode` and leaves one quiet row; chosen mid-turn, a model waits for the turn's end.
pub(super) fn agent_box_change_restart() {
    run_scene(
        "agent-box-change-restart",
        &AGY,
        start(Some("accept-edits"), None),
        |n, cx, tab, told| {
            let id = agy_conversation();
            feed(cx, tab, |t, cx| t.send("Say hello in one word".into(), cx));
            feed(cx, tab, |t, cx| t.shot_feed(agy_text(), cx));
            assert_eq!(cx.read(|cx| tab.read(cx).status()), Status::Done, "{n}");
            feed(cx, tab, |t, cx| t.choose(Change::Mode("plan".into()), cx));
            let a = argv(cx, tab);
            assert_eq!(flag(&a, "--conversation"), Some(id.clone()), "{n}: {a:?}");
            assert_eq!(flag(&a, "--mode").as_deref(), Some("plan"), "{n}: {a:?}");
            assert_eq!(notices(cx, tab), ["Switched to Plan"], "{n}");
            assert_eq!(
                cx.read(|cx| tab.read(cx).mode().map(str::to_string)),
                Some("plan".into()),
                "{n}"
            );
            assert_eq!(
                told.borrow().last().cloned(),
                Some((None, None, Some("plan".into()))),
                "{n}: saved"
            );
            assert!(
                written(cx, tab)
                    .iter()
                    .filter(|l| l["event"] == "user")
                    .count()
                    == 1,
                "{n}: nothing sent"
            );

            // Mid-turn: the model waits for the turn's end, then restarts with it.
            feed(cx, tab, |t, cx| t.send("Now fix notes.txt".into(), cx));
            let a = argv(cx, tab);
            feed(cx, tab, |t, cx| {
                t.choose(
                    Change::Model {
                        id: "gemini-3.8-flash-low".into(),
                        effort: None,
                    },
                    cx,
                )
            });
            assert_eq!(argv(cx, tab), a, "{n}: no restart mid-turn");
            feed(cx, tab, |t, cx| t.shot_feed(vec![turn_done()], cx));
            let a = argv(cx, tab);
            assert_eq!(flag(&a, "--conversation"), Some(id), "{n}: {a:?}");
            assert_eq!(
                flag(&a, "--model").as_deref(),
                Some("gemini-3.8-flash-low"),
                "{n}: {a:?}"
            );
            assert_eq!(flag(&a, "--mode").as_deref(), Some("plan"), "{n}: kept");
            assert!(
                !a.iter().any(|w| w == "--effort"),
                "{n}: never --effort beside agy's id"
            );
            assert_eq!(
                notices(cx, tab).last().map(String::as_str),
                Some("Switched to gemini-3.8-flash-low"),
                "{n}"
            );
        },
    );
}

/// A restart waiting for the turn, then the mode it runs chosen again: the change is dropped,
/// and what you sent meanwhile still goes when the turn ends (never left waiting).
fn cancelled_change_still_sends() {
    run_scene(
        "agent-box-restart-keeps-queued-text-cancelled",
        &AGY,
        start(Some("accept-edits"), None),
        |n, cx, tab, _| {
            feed(cx, tab, |t, cx| t.send("Say hello in one word".into(), cx));
            let mut events = agy_text();
            let done = events
                .iter()
                .position(|e| matches!(e, Event::TurnDone { .. }))
                .expect("a turn end");
            let rest = events.split_off(done);
            feed(cx, tab, |t, cx| t.shot_feed(events, cx));
            let a = argv(cx, tab);
            feed(cx, tab, |t, cx| t.choose(Change::Mode("plan".into()), cx));
            feed(cx, tab, |t, cx| t.send("hi".into(), cx));
            assert_eq!(cx.read(|cx| tab.read(cx).shot_pending()), 1, "{n}: held");
            feed(cx, tab, |t, cx| {
                t.choose(Change::Mode("accept-edits".into()), cx)
            });
            assert_eq!(cx.read(|cx| tab.read(cx).waiting_change().cloned()), None);
            feed(cx, tab, |t, cx| t.shot_feed(rest, cx));
            assert_eq!(argv(cx, tab), a, "{n}: no restart");
            assert_eq!(
                cx.read(|cx| tab.read(cx).shot_pending()),
                0,
                "{n}: not stuck"
            );
            let lines = written(cx, tab);
            let last = lines.last().expect("a line");
            assert_eq!(last["message"]["content"], "hi", "{n}: {last}");
        },
    );
}

/// Review Focus #3: a message sent while a restart waits for the turn is held (not written to
/// the process about to end) and sent to the restarted one; if that restart can't start, what
/// was sent stays in the conversation (the tab's bad-resume path) — nothing typed is lost.
pub(super) fn agent_box_restart_keeps_queued_text() {
    run_scene(
        "agent-box-restart-keeps-queued-text",
        &AGY,
        start(Some("accept-edits"), None),
        |n, cx, tab, _| {
            feed(cx, tab, |t, cx| t.send("Say hello in one word".into(), cx));
            let mut events = agy_text();
            // The turn's end comes later: it is still working.
            let done = events
                .iter()
                .position(|e| matches!(e, Event::TurnDone { .. }))
                .expect("a turn end");
            let rest = events.split_off(done);
            feed(cx, tab, |t, cx| t.shot_feed(events, cx));
            assert_eq!(cx.read(|cx| tab.read(cx).status()), Status::Working, "{n}");
            feed(cx, tab, |t, cx| t.choose(Change::Mode("plan".into()), cx));
            let sent = |cx: &VisualTestAppContext| {
                written(cx, tab)
                    .iter()
                    .filter(|l| l["event"] == "user")
                    .count()
            };
            assert_eq!(sent(cx), 1, "{n}");
            feed(cx, tab, |t, cx| t.send("And then this".into(), cx));
            assert_eq!(sent(cx), 1, "{n}: held while the restart waits");
            assert_eq!(cx.read(|cx| tab.read(cx).shot_pending()), 1, "{n}");
            feed(cx, tab, |t, cx| t.shot_feed(rest, cx));
            let a = argv(cx, tab);
            assert_eq!(flag(&a, "--mode").as_deref(), Some("plan"), "{n}: {a:?}");
            assert_eq!(cx.read(|cx| tab.read(cx).shot_pending()), 0, "{n}");
            let lines = written(cx, tab);
            let last = lines.last().expect("a line");
            assert_eq!(last["event"], "user", "{n}: {last}");
            assert_eq!(
                last["message"]["content"], "And then this",
                "{n}: sent to the restarted agent"
            );
            assert_eq!(said(cx, tab), ["Say hello in one word", "And then this"]);

            // That restart couldn't start: the tab's bad-resume path; the message stays.
            feed(cx, tab, |t, cx| {
                t.shot_process(
                    crate::agent_process::ProcessEvent::Exited {
                        code: Some(1),
                        stderr_tail: "network down".into(),
                    },
                    cx,
                )
            });
            assert!(
                said(cx, tab).iter().any(|s| s == "And then this"),
                "{n}: nothing typed is lost"
            );
        },
    );
    cancelled_change_still_sends();
}

// ---- The box's layout (Task 5) ----

/// The window the layout scenes draw in.
const DIMS: (f32, f32) = (720.0, 640.0);

/// A made-up catalog: one model, `Claude Example 5`, with three efforts; the agent says it runs
/// in its `default` mode.
fn example_catalog() -> Catalog {
    Catalog {
        models: vec![ModelChoice {
            id: "example-5".into(),
            label: "Claude Example 5".into(),
            resolves_to: Some("claude-example-5".into()),
            efforts: ["low", "medium", "high"]
                .map(|l| EffortChoice {
                    level: l.into(),
                    model_id: None,
                })
                .to_vec(),
        }],
        mode: Some("default".into()),
        ..Catalog::default()
    }
}

/// Claude, `example-5` at `high` chosen, in `~/code/my-app`.
fn example_start() -> Start {
    Start {
        effort: Some("high".into()),
        ..start(None, Some("example-5"))
    }
}

/// Shoot layout scene `name` in both themes: the tab (keys bound, the keyboard in its box) with
/// its history in and the made-up catalog read; `act` sets it up and checks it before the shot.
fn layout_scene(
    name: &str,
    act: impl Fn(&str, &mut VisualTestAppContext, AnyWindowHandle, &Entity<AgentTab>) + Clone + 'static,
) {
    box_scene(
        name,
        &CLAUDE,
        example_start(),
        Lookup::Ready(example_catalog()),
        act,
    );
}

/// `layout_scene` for any agent, its `start` and what its catalog lookup says.
fn box_scene(
    name: &str,
    provider: &'static dyn Provider,
    start: Start,
    lookup: Lookup<Catalog>,
    act: impl Fn(&str, &mut VisualTestAppContext, AnyWindowHandle, &Entity<AgentTab>) + Clone + 'static,
) {
    box_scene_on(name, provider, start, lookup, false, act)
}

/// A tab on a red ground (`box_scene_on`).
struct Ground(Entity<AgentTab>);

impl gpui::Render for Ground {
    fn render(&mut self, _: &mut gpui::Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().bg(gpui::red()).child(self.0.clone())
    }
}

/// `box_scene`; with `backdrop` the tab sits on a red ground, so what lies outside its rounded
/// corners shows (the window's own clear colour is the page's).
fn box_scene_on(
    name: &str,
    provider: &'static dyn Provider,
    start: Start,
    lookup: Lookup<Catalog>,
    backdrop: bool,
    act: impl Fn(&str, &mut VisualTestAppContext, AnyWindowHandle, &Entity<AgentTab>) + Clone + 'static,
) {
    for &(slug, theme_name) in shot_themes() {
        let theme = shot_theme(theme_name);
        let slot: Slot = Rc::default();
        let keep = slot.clone();
        let n = format!("{name}-{slug}");
        let act = act.clone();
        let (start, lookup) = (start.clone(), lookup.clone());
        let path = shoot_with(
            name,
            slug,
            &theme,
            size(px(DIMS.0), px(DIMS.1)),
            move |window, cx| {
                cx.set_global(shot_appearance(theme, cx));
                cx.bind_keys(crate::key_bindings());
                let tab = cx.new(|cx| AgentTab::for_shots(provider, start, window, cx));
                let focus = tab.read(cx).focus_handle(cx);
                focus.focus(window, cx);
                *keep.borrow_mut() = Some(tab.clone());
                if backdrop {
                    cx.new(|_| Ground(tab)).into()
                } else {
                    tab.into()
                }
            },
            move |cx, window| {
                let tab = slot.borrow().clone().expect("the agent tab");
                feed(cx, &tab, |t, cx| t.shot_history(Vec::new(), cx));
                feed(cx, &tab, |t, cx| t.shot_catalog(lookup, cx));
                let _ = frame_fresh(cx, window);
                act(&n, cx, window, &tab);
                let _ = frame_fresh(cx, window);
                slot.borrow_mut().take();
            },
        );
        println!("ui-shots: {}", path.display());
    }
}

/// Where `part` of the box was drawn (it must have been).
fn part(cx: &VisualTestAppContext, tab: &Entity<AgentTab>, n: &str, part: &str) -> Bounds<Pixels> {
    cx.read(|cx| tab.read(cx).shot_part(part))
        .unwrap_or_else(|| panic!("{n}: the {part} was never drawn"))
}

/// `inner` lies inside `outer`.
fn inside(inner: Bounds<Pixels>, outer: Bounds<Pixels>) -> bool {
    inner.left() >= outer.left()
        && inner.right() <= outer.right()
        && inner.top() >= outer.top()
        && inner.bottom() <= outer.bottom()
}

/// The composer's text area, as last drawn.
fn composer_bounds(cx: &VisualTestAppContext, tab: &Entity<AgentTab>) -> Bounds<Pixels> {
    cx.read(|cx| tab.read(cx).shot_composer().read(cx).shot_rows().2)
        .expect("the composer was drawn")
}

/// §1's box, empty: the folder above it; inside it the placeholder and, along the bottom, `+`,
/// the model chip (`Claude Example 5  High ▾`), then on the right the mode chip and send. A
/// menu opens upward from its chip, one at a time, and Esc closes it (stopping nothing).
pub(super) fn agent_box_empty() {
    layout_scene("agent-box-empty", |n, cx, window, tab| {
        let texts = cx.read(|cx| tab.read(cx).shot_box(cx));
        assert_eq!(texts.folder, "my-app", "{n}");
        assert_eq!(texts.model, "Claude Example 5  High", "{n}");
        assert_eq!(
            texts.mode.as_deref(),
            Some("Ask before edits"),
            "{n}: the mode Claude reports"
        );
        assert_eq!(texts.send, "\u{27a4}", "{n}");
        assert_eq!(texts.usage, None, "{n}: nothing spent yet");
        let placeholder = cx.read(|cx| {
            tab.read(cx)
                .shot_composer()
                .read(cx)
                .placeholder()
                .to_string()
        });
        assert_eq!(placeholder, PLACEHOLDER, "{n}");

        let (folder, the_box) = (part(cx, tab, n, "folder"), part(cx, tab, n, "box"));
        let (attach, model) = (part(cx, tab, n, "attach"), part(cx, tab, n, "model"));
        let (mode, send) = (part(cx, tab, n, "mode"), part(cx, tab, n, "send"));
        let text = composer_bounds(cx, tab);
        assert!(
            folder.bottom() <= the_box.top(),
            "{n}: the folder sits above the box"
        );
        assert!(!inside(folder, the_box), "{n}");
        for (name, b) in [
            ("+", attach),
            ("model", model),
            ("mode", mode),
            ("send", send),
        ] {
            assert!(
                inside(b, the_box),
                "{n}: {name} {b:?} is inside the box {the_box:?}"
            );
            assert!(b.top() >= text.bottom(), "{n}: {name} is under the text");
            assert!(
                (b.center().y - send.center().y).abs() < px(2.0),
                "{n}: {name} is on the bottom row"
            );
        }
        assert!(inside(text, the_box), "{n}: the text is in the box");
        assert!(attach.right() <= model.left(), "{n}: + then the model");
        assert!(model.right() < mode.left(), "{n}: the mode on the right");
        assert!(mode.right() <= send.left(), "{n}: then send, last");
        assert!(
            the_box.right() - send.right() < px(16.0),
            "{n}: send at the box's right edge"
        );
        assert!(
            attach.left() - the_box.left() < px(16.0),
            "{n}: + at its left edge"
        );

        // A menu opens upward from its chip; one at a time; Esc closes it.
        feed(cx, tab, |t, cx| {
            t.open_menu(BoxMenu::Model { efforts_of: None }, cx)
        });
        let _ = frame_fresh(cx, window);
        let menu = part(cx, tab, n, "menu");
        assert!(
            menu.bottom() <= model.top(),
            "{n}: above the model chip {menu:?}"
        );
        assert!(
            (menu.left() - model.left()).abs() < px(1.0),
            "{n}: from the chip's left {menu:?} {model:?}"
        );
        feed(cx, tab, |t, cx| t.open_menu(BoxMenu::Folder, cx));
        let _ = frame_fresh(cx, window);
        assert_eq!(
            cx.read(|cx| tab.read(cx).shot_menu()),
            Some(BoxMenu::Folder),
            "{n}: one at a time"
        );
        let menu = part(cx, tab, n, "menu");
        assert!(
            menu.bottom() <= folder.top(),
            "{n}: above the folder {menu:?}"
        );
        cx.simulate_keystrokes(window, "escape");
        cx.run_until_parked();
        cx.read(|cx| {
            let t = tab.read(cx);
            assert_eq!(t.shot_menu(), None, "{n}: Esc closed it");
            assert_eq!(t.status(), Status::Idle, "{n}");
            assert_eq!(t.shot_stop_label(), None, "{n}: and stopped nothing");
        });
        assert!(written(cx, tab).is_empty(), "{n}: nothing sent");

        // The box looks the same with or without the keyboard (the designer's call: no border,
        // no focus ring; the caret shows it); a click on the box's own room (not a chip) gives
        // it the keyboard back.
        let edge_px = |cx: &mut VisualTestAppContext| {
            let shot = frame_fresh(cx, window);
            // Mid-way down the box's left edge (its probe sits just inside it).
            super::pixel_at(
                &shot,
                size(px(DIMS.0), px(DIMS.1)),
                f32::from(the_box.left()) - 0.5,
                f32::from(the_box.center().y),
            )
        };
        let focused_px = edge_px(cx);
        cx.update_window(window, |_, window, _| window.blur())
            .expect("the window");
        cx.run_until_parked();
        assert_eq!(edge_px(cx), focused_px, "{n}: no focus ring");
        let room = gpui::point(the_box.center().x, send.center().y);
        cx.simulate_click(window, room, Modifiers::none());
        cx.run_until_parked();
        cx.update_window(window, |_, window, cx| {
            assert!(
                tab.read(cx).focus_handle(cx).is_focused(window),
                "{n}: a click in the box's room focuses its text"
            );
        })
        .expect("the window");
        assert!(written(cx, tab).is_empty(), "{n}: the click sent nothing");
    });
}

/// M11al §8: the box sits `AREA_X` (28) in from the page's sides, where the terminal's text
/// column starts; the conversation's column starts at that same x; and the page is rounded at
/// its bottom corners as the card is (a pixel just outside the curve is not the page's fill).
pub(super) fn agent_box_card() {
    box_scene_on(
        "agent-box-card",
        &CLAUDE,
        example_start(),
        Lookup::Ready(example_catalog()),
        true,
        |n, cx, window, tab| {
            feed(cx, tab, |t, cx| t.shot_feed(agy_text(), cx));
            let shot = frame_fresh(cx, window);
            let the_box = part(cx, tab, n, "box");
            let column = part(cx, tab, n, "column");
            // The box (no edge of its own since the designer's details) sits in the outer box
            // (its 1 px edge and `OUTER_INSET`), which is 28 in.
            let inset = crate::card_view::EDGE + crate::card_view::OUTER_INSET;
            let (side, edge) = (px(28.0 + inset), px(0.0));
            assert!(
                (the_box.left() - edge - side).abs() < px(0.5),
                "{n}: the box is 28 + {inset} in from the left: {the_box:?}"
            );
            assert!(
                (px(DIMS.0) - the_box.right() - edge - side).abs() < px(0.5),
                "{n}: and from the right: {the_box:?}"
            );
            assert!(
                (column.left() - (the_box.left() - edge - px(inset))).abs() < px(0.5),
                "{n}: the conversation's column starts at the outer box's x: {column:?} {the_box:?}"
            );
            // The page's bottom corners: inside the curve is the page, just outside it is not.
            let page = cx.read(|cx| {
                crate::agent_tab::Colors::of(&cx.global::<crate::appearance::Appearance>().theme)
                    .page
            });
            let page = [page.r, page.g, page.b];
            let dims = size(px(DIMS.0), px(DIMS.1));
            let near = |a: [u8; 3], b: [u8; 3]| a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= 4);
            let r = crate::ui_tokens::size::CARD_RADIUS;
            let (w, h) = (DIMS.0, DIMS.1);
            for (x, y) in [(0.5, h - 0.5), (w - 0.5, h - 0.5)] {
                let p = super::pixel_at(&shot, dims, x, y);
                assert!(
                    near(p, [255, 0, 0]) && !near(p, page),
                    "{n}: ({x}, {y}) is outside the rounded corner (the ground), not the page: {p:?}"
                );
            }
            for (x, y) in [
                (r + 2.0, h - 0.5),
                (w - r - 2.0, h - 0.5),
                (0.5, h - r - 2.0),
                (w / 2.0, h - 0.5),
            ] {
                let p = super::pixel_at(&shot, dims, x, y);
                assert!(near(p, page), "{n}: ({x}, {y}) is the page: {p:?}");
            }
        },
    );
}

/// Working: Stop in send's place; the top line says what the conversation cost and how full
/// its context is (`$0.21 · context 18%`), no longer the chips.
pub(super) fn agent_box_working() {
    layout_scene("agent-box-working", |n, cx, window, tab| {
        let send = part(cx, tab, n, "send");
        feed(cx, tab, |t, cx| t.send("Fix notes.txt".into(), cx));
        feed(cx, tab, |t, cx| {
            t.shot_feed(
                vec![Event::Usage {
                    context_pct: Some(18),
                    cost_usd: Some(0.2116),
                }],
                cx,
            )
        });
        let _ = frame_fresh(cx, window);
        let texts = cx.read(|cx| tab.read(cx).shot_box(cx));
        assert_eq!(cx.read(|cx| tab.read(cx).status()), Status::Working, "{n}");
        assert_eq!(
            texts.send, "\u{25a0} Stop  esc",
            "{n}: Stop in send's place"
        );
        assert_eq!(
            texts.usage.as_deref(),
            Some("$0.21 \u{b7} context 18%"),
            "{n}"
        );
        let stop = part(cx, tab, n, "send");
        assert!(
            (stop.right() - send.right()).abs() < px(1.0),
            "{n}: where send was {stop:?} {send:?}"
        );
        assert!(inside(stop, part(cx, tab, n, "box")), "{n}");
        let usage = part(cx, tab, n, "usage");
        assert!(
            usage.bottom() <= px(30.0),
            "{n}: on the pane's top line {usage:?}"
        );
        assert!(usage.right() > px(DIMS.0 / 2.0), "{n}: at its right");
    });
}

/// A command block attached: its chip inside the box, above the text; its × takes it off
/// (nothing is sent).
pub(super) fn agent_box_attachment() {
    layout_scene("agent-box-attachment", |n, cx, window, tab| {
        let npm = "$ npm test\n(exit 1)\n> app@1.0.0 test\nFAIL src/rsvp.test.ts";
        let done = "$ echo done\n(exit 0)\ndone";
        feed(cx, tab, |t, cx| t.attach(Attachment::Text(done.into()), cx));
        feed(cx, tab, |t, cx| t.attach(Attachment::Text(npm.into()), cx));
        let _ = frame_fresh(cx, window);
        let chips = part(cx, tab, n, "attachments");
        let the_box = part(cx, tab, n, "box");
        assert!(inside(chips, the_box), "{n}: the chips are in the box");
        assert!(
            chips.bottom() <= composer_bounds(cx, tab).top(),
            "{n}: above the text"
        );
        let x = cx
            .read(|cx| tab.read(cx).shot_remove_bounds(0))
            .expect("the first chip's \u{d7}");
        assert!(inside(x, chips), "{n}");
        cx.simulate_click(window, x.center(), Modifiers::none());
        cx.run_until_parked();
        cx.read(|cx| {
            let t = tab.read(cx);
            assert_eq!(
                t.shot_attachments(),
                [Attachment::Text(npm.to_string())],
                "{n}: \u{d7} took it off"
            );
            assert!(t.shot_items().is_empty(), "{n}: nothing sent");
        });
        assert!(written(cx, tab).is_empty(), "{n}");
    });
}

/// A folder of made-up files for the attach scenes, removed when dropped.
struct Files(PathBuf);

impl Files {
    fn new(scene: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("fuxx-shots-{scene}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the scene's folder");
        Files(dir)
    }

    /// File `name` with `bytes`; its path.
    fn make(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, bytes).expect("a made-up file");
        path
    }

    /// File `name` of `len` zero bytes (sparse: it costs nothing to make); its path.
    fn make_sized(&self, name: &str, len: usize) -> PathBuf {
        let path = self.0.join(name);
        let file = std::fs::File::create(&path).expect("a made-up file");
        file.set_len(len as u64).expect("its size");
        path
    }
}

impl Drop for Files {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A made-up image file's bytes (a PNG's signature, then nothing: no scene decodes it).
const PNG: &[u8] = b"\x89PNG\r\n\x1a\nmade-up";

/// Run until `done` holds: image files are read off the UI thread, so a chip (or a notice)
/// comes a little later. At most five seconds.
fn settle_until(
    n: &str,
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    what: &str,
    done: impl Fn(&VisualTestAppContext) -> bool,
) {
    let started = std::time::Instant::now();
    loop {
        cx.run_until_parked();
        if done(cx) {
            let _ = frame_fresh(cx, window);
            return;
        }
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "{n}: {what}"
        );
        super::prof::sleep(std::time::Duration::from_millis(10));
    }
}

/// The waiting attachments, as the chips say them (`glyph label`).
fn chips(cx: &VisualTestAppContext, tab: &Entity<AgentTab>) -> Vec<String> {
    cx.read(|cx| {
        tab.read(cx)
            .shot_attachments()
            .iter()
            .map(|a| format!("{} {}", a.glyph(), a.label()))
            .collect()
    })
}

/// The last user message written to the agent.
fn last_user(cx: &VisualTestAppContext, tab: &Entity<AgentTab>) -> serde_json::Value {
    written(cx, tab)
        .into_iter()
        .rfind(|l| l["type"] == "user" || l["event"] == "user")
        .expect("a message was written")
}

/// §8: `+` opens a menu over it with `File\u{2026}` and `Image\u{2026}`, one menu at a time (a
/// press on another chip closes it). Choosing a row closes it and opens the file panel (a shot
/// has none: it is cancelled), so nothing is attached or sent. Left open for the shot.
pub(super) fn agent_box_attach_menu() {
    layout_scene("agent-box-attach-menu", |n, cx, window, tab| {
        let plus = part(cx, tab, n, "attach");
        cx.simulate_click(window, plus.center(), Modifiers::none());
        cx.run_until_parked();
        let _ = frame_fresh(cx, window);
        assert_eq!(menu(cx, tab), Some(BoxMenu::Attach), "{n}");
        assert_eq!(labels(cx, tab), ["File\u{2026}", "Image\u{2026}"], "{n}");
        let menu_box = part(cx, tab, n, "menu");
        assert!(plus.top() >= menu_box.bottom(), "{n}: upward from +");
        let at = |l: &str| {
            cx.read(|cx| tab.read(cx).shot_menu_row(l))
                .unwrap_or_else(|| panic!("{n}: no row {l}"))
        };
        let (file, image) = (at("File\u{2026}"), at("Image\u{2026}"));
        assert!(file.bottom() <= image.top(), "{n}: top to bottom");
        assert!(inside(file, menu_box) && inside(image, menu_box), "{n}");
        // + again closes it; a press on another chip closes it and opens nothing, and + does the
        // same to another menu.
        cx.simulate_click(window, plus.center(), Modifiers::none());
        cx.run_until_parked();
        assert_eq!(menu(cx, tab), None, "{n}: + closes it");
        cx.simulate_click(window, plus.center(), Modifiers::none());
        click_model_chip(n, cx, window, tab);
        assert_eq!(menu(cx, tab), None, "{n}: the model chip's press closes it");
        click_model_chip(n, cx, window, tab);
        assert!(matches!(menu(cx, tab), Some(BoxMenu::Model { .. })), "{n}");
        cx.simulate_click(window, plus.center(), Modifiers::none());
        cx.run_until_parked();
        assert_eq!(menu(cx, tab), None, "{n}: +'s press closes the model menu");
        cx.simulate_click(window, plus.center(), Modifiers::none());
        cx.run_until_parked();
        let _ = frame_fresh(cx, window);
        assert_eq!(menu(cx, tab), Some(BoxMenu::Attach), "{n}");
        // Each row closes it; the panel is cancelled here: no chip, nothing sent.
        for row in ["File\u{2026}", "Image\u{2026}"] {
            click_row(n, cx, window, tab, row);
            assert_eq!(menu(cx, tab), None, "{n}: {row} closes it");
            cx.simulate_click(window, plus.center(), Modifiers::none());
            cx.run_until_parked();
            let _ = frame_fresh(cx, window);
            assert_eq!(menu(cx, tab), Some(BoxMenu::Attach), "{n}");
        }
        assert!(
            chips(cx, tab).is_empty(),
            "{n}: a cancelled panel attaches nothing"
        );
        assert!(written(cx, tab).is_empty(), "{n}: nothing sent");
        assert!(cx.read(|cx| tab.read(cx).shot_items().is_empty()), "{n}");
    });
}

/// §8: files (from `File\u{2026}`, or dropped on the box) are chips, `\u{1f4ce} name`, in the box above the
/// text; a dropped image file is an image chip instead. A message sends each file as its path
/// after the text, and the conversation lists their names; the \u{d7} takes one off.
pub(super) fn agent_box_file_chip() {
    layout_scene("agent-box-file-chip", |n, cx, window, tab| {
        let files = Files::new("file-chip");
        let notes = files.make("notes.txt", b"made-up");
        let todo = files.make("todo.md", b"made-up");
        let log = files.make("build.log", b"made-up");
        feed(cx, tab, |t, cx| t.attach_files(vec![notes.clone()], cx));
        feed(cx, tab, |t, cx| {
            t.attach_dropped(vec![todo.clone(), log.clone()], cx)
        });
        let _ = frame_fresh(cx, window);
        assert_eq!(
            chips(cx, tab),
            [
                "\u{1f4ce} notes.txt",
                "\u{1f4ce} todo.md",
                "\u{1f4ce} build.log"
            ],
            "{n}"
        );
        let chip_row = part(cx, tab, n, "attachments");
        assert!(inside(chip_row, part(cx, tab, n, "box")), "{n}: in the box");
        assert!(
            chip_row.bottom() <= composer_bounds(cx, tab).top(),
            "{n}: above the text"
        );
        assert!(written(cx, tab).is_empty(), "{n}: attaching sends nothing");
        // The \u{d7} of the last takes it off.
        let x = cx
            .read(|cx| tab.read(cx).shot_remove_bounds(2))
            .expect("the last chip's \u{d7}");
        cx.simulate_click(window, x.center(), Modifiers::none());
        cx.run_until_parked();
        assert_eq!(chips(cx, tab).len(), 2, "{n}: \u{d7} took one off");
        feed(cx, tab, |t, cx| t.attach_files(vec![log.clone()], cx));
        let _ = frame_fresh(cx, window);
        // Sent: the paths go after the text, as text (not an image block).
        feed(cx, tab, |t, cx| t.send("Read these".into(), cx));
        let sent = last_user(cx, tab);
        assert_eq!(
            sent["message"]["content"],
            format!(
                "Read these\n\n{}\n\n{}\n\n{}",
                notes.display(),
                todo.display(),
                log.display()
            ),
            "{n}"
        );
        assert!(chips(cx, tab).is_empty(), "{n}: the box has none left");
        let said = cx.read(|cx| {
            tab.read(cx).shot_items().iter().find_map(|i| match i {
                Item::You { attachments, .. } => Some(attachments.clone()),
                _ => None,
            })
        });
        assert_eq!(
            said.as_deref(),
            Some(
                &[
                    "notes.txt".to_string(),
                    "todo.md".into(),
                    "build.log".into()
                ][..]
            ),
            "{n}"
        );
        // One more waits for the shot.
        feed(cx, tab, |t, cx| t.attach_files(vec![notes.clone()], cx));
    });
}

/// M11ag: code asked about from a file tab is a chip, `</> workspace.rs:43\u{2013}45`, with the full
/// path on hover; the same code twice is one chip; the \u{d7} takes it off. Attaching writes nothing;
/// the send puts the path (from the agent's folder), lines and a fence after the text.
pub(super) fn agent_box_code_chip() {
    layout_scene("agent-box-code-chip", |n, cx, window, tab| {
        let cwd = cwd();
        let code = || CodeRef {
            path: cwd.join("src/workspace.rs"),
            real: None,
            lines: CodeLines::Lines {
                first: 43,
                last: 45,
            },
            fence: Some("rust".into()),
            body: "if let Some(tab) = pane.find(&path) {\n    return self.show(tab);\n}".into(),
        };
        feed(cx, tab, |t, cx| t.attach(Attachment::Code(code()), cx));
        feed(cx, tab, |t, cx| t.attach(Attachment::Code(code()), cx));
        let _ = frame_fresh(cx, window);
        assert_eq!(
            chips(cx, tab),
            ["</> workspace.rs:43\u{2013}45"],
            "{n}: one chip"
        );
        let tip = cx.read(|cx| tab.read(cx).shot_attachments()[0].tip());
        assert_eq!(
            tip.as_deref(),
            Some(cwd.join("src/workspace.rs").to_str().unwrap()),
            "{n}: hover says the full path"
        );
        let chip_row = part(cx, tab, n, "attachments");
        assert!(inside(chip_row, part(cx, tab, n, "box")), "{n}: in the box");
        assert!(written(cx, tab).is_empty(), "{n}: attaching sends nothing");
        // Hovering it is harmless (the tip is a window overlay).
        let x = cx
            .read(|cx| tab.read(cx).shot_remove_bounds(0))
            .expect("the chip's \u{d7}");
        cx.simulate_mouse_move(window, chip_row.center(), None, Modifiers::none());
        cx.run_until_parked();
        assert!(written(cx, tab).is_empty(), "{n}: hovering sends nothing");
        cx.simulate_click(window, x.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(chips(cx, tab).is_empty(), "{n}: \u{d7} took it off");
        assert!(written(cx, tab).is_empty(), "{n}: removing sends nothing");
        feed(cx, tab, |t, cx| t.attach(Attachment::Code(code()), cx));
        let _ = frame_fresh(cx, window);
        assert!(written(cx, tab).is_empty(), "{n}: nothing before the send");
        feed(cx, tab, |t, cx| t.send("why?".into(), cx));
        assert_eq!(
            last_user(cx, tab)["message"]["content"],
            "why?\n\nsrc/workspace.rs:43-45\n```rust\nif let Some(tab) = pane.find(&path) {\n    return self.show(tab);\n}\n```",
            "{n}"
        );
        assert!(chips(cx, tab).is_empty(), "{n}: sent");
        // One more waits for the shot.
        feed(cx, tab, |t, cx| t.attach(Attachment::Code(code()), cx));
    });
}

/// §8: an image from `Image\u{2026}`, pasted, or dropped is a chip, `\u{1f5bc} name`. Claude takes
/// them in the message: image blocks (base64) before the text, no path in the text; the
/// conversation lists their names.
pub(super) fn agent_box_image_chip() {
    layout_scene("agent-box-image-chip", |n, cx, window, tab| {
        let files = Files::new("image-chip");
        let diagram = files.make("diagram.png", PNG);
        let photo = files.make("photo.JPG", b"made-up jpeg");
        feed(cx, tab, |t, cx| t.attach_images(vec![diagram.clone()], cx));
        feed(cx, tab, |t, cx| t.attach_dropped(vec![photo.clone()], cx));
        let image = gpui::Image::from_bytes(gpui::ImageFormat::Png, b"pasted bytes".to_vec());
        feed(cx, tab, |t, cx| t.attach_pasted_image(image, cx));
        settle_until(n, cx, window, "three image chips", |cx| {
            chips(cx, tab).len() == 3
        });
        let mut got = chips(cx, tab);
        got.sort();
        assert_eq!(
            got,
            [
                "\u{1f5bc} diagram.png",
                "\u{1f5bc} pasted-image.png",
                "\u{1f5bc} photo.JPG"
            ],
            "{n}"
        );
        let chip_row = part(cx, tab, n, "attachments");
        assert!(inside(chip_row, part(cx, tab, n, "box")), "{n}: in the box");
        assert!(
            chip_row.bottom() <= composer_bounds(cx, tab).top(),
            "{n}: above the text"
        );
        assert!(written(cx, tab).is_empty(), "{n}: attaching sends nothing");
        // Claude is not told paths: each image is kept as it is, the pasted one has no file.
        cx.read(|cx| {
            for a in tab.read(cx).shot_attachments() {
                let Attachment::Image {
                    name,
                    picture,
                    path,
                } = a
                else {
                    panic!("{n}: {a:?}");
                };
                match name.as_str() {
                    "diagram.png" => {
                        assert_eq!(picture.media_type, "image/png", "{n}");
                        assert_eq!(path.as_deref(), Some(diagram.as_path()), "{n}");
                    }
                    "photo.JPG" => assert_eq!(picture.media_type, "image/jpeg", "{n}"),
                    _ => assert_eq!(path, &None, "{n}: nothing written for Claude"),
                }
            }
        });
        feed(cx, tab, |t, cx| t.send("What are these?".into(), cx));
        let sent = last_user(cx, tab);
        let blocks = sent["message"]["content"].as_array().expect("blocks");
        assert_eq!(blocks.len(), 4, "{n}: three images, then the text");
        for b in &blocks[..3] {
            assert_eq!(b["type"], "image", "{n}");
            assert_eq!(b["source"]["type"], "base64", "{n}");
        }
        assert_eq!(blocks[0]["source"]["media_type"], "image/png", "{n}");
        assert_eq!(
            blocks[0]["source"]["data"],
            crate::providers::base64(PNG),
            "{n}"
        );
        assert_eq!(
            blocks[3],
            serde_json::json!({"type":"text","text":"What are these?"}),
            "{n}"
        );
        assert!(chips(cx, tab).is_empty(), "{n}: the box has none left");
        // Another waits for the shot.
        let again = files.make("sketch.webp", b"made-up webp");
        feed(cx, tab, |t, cx| t.attach_images(vec![again], cx));
        settle_until(n, cx, window, "the chip", |cx| chips(cx, tab).len() == 1);
        assert_eq!(chips(cx, tab), ["\u{1f5bc} sketch.webp"], "{n}");
    });
}

/// §8, agy: it takes no image in the message, so an image goes as its absolute path after the
/// text: a file's own path, and a pasted image's, written first to
/// `$TMPDIR/fuxx-images/<uuid>.png` (off the UI thread).
pub(super) fn agent_box_image_agy() {
    box_scene(
        "agent-box-image-agy",
        &AGY,
        start(Some("accept-edits"), None),
        Lookup::Ready(agy_example_catalog()),
        |n, cx, window, tab| {
            let files = Files::new("image-agy");
            let diagram = files.make("diagram.png", PNG);
            feed(cx, tab, |t, cx| t.attach_images(vec![diagram.clone()], cx));
            let image = gpui::Image::from_bytes(gpui::ImageFormat::Png, b"pasted bytes".to_vec());
            feed(cx, tab, |t, cx| t.attach_pasted_image(image, cx));
            settle_until(n, cx, window, "two chips", |cx| chips(cx, tab).len() == 2);
            let pasted = cx.read(|cx| {
                tab.read(cx)
                    .shot_attachments()
                    .iter()
                    .find_map(|a| match a {
                        Attachment::Image { name, path, .. } if name == "pasted-image.png" => {
                            path.clone()
                        }
                        _ => None,
                    })
            });
            let pasted = pasted.expect("a pasted image has a file for agy");
            let folder = std::env::temp_dir().join("fuxx-images");
            assert_eq!(pasted.parent(), Some(folder.as_path()), "{n}");
            assert_eq!(
                pasted.extension().and_then(|e| e.to_str()),
                Some("png"),
                "{n}"
            );
            assert_eq!(
                std::fs::read(&pasted).expect("written"),
                b"pasted bytes",
                "{n}"
            );
            feed(cx, tab, |t, cx| t.send("Look".into(), cx));
            let sent = last_user(cx, tab);
            assert_eq!(
                sent["message"]["content"],
                format!("Look\n\n{}\n\n{}", diagram.display(), pasted.display()),
                "{n}"
            );
            let _ = std::fs::remove_file(&pasted);
        },
    );
}

/// §8: an image too big for Claude (over 5 MB once in base64: 3.75 MB of file, R8) is refused:
/// no chip, one notice, `Image too big for Claude (3.75 MB max): <name>`, in the conversation; a
/// pasted one likewise. One of exactly 3.75 MB is taken.
pub(super) fn agent_box_image_too_big() {
    layout_scene("agent-box-image-too-big", |n, cx, window, tab| {
        let max = CLAUDE.image_limit().raw_max();
        let files = Files::new("too-big");
        let exact = files.make_sized("exact.png", max);
        let big = files.make_sized("big-scan.png", max + 1);
        feed(cx, tab, |t, cx| t.attach_images(vec![exact.clone()], cx));
        settle_until(n, cx, window, "exactly 3.75 MB is taken", |cx| {
            chips(cx, tab).len() == 1
        });
        assert!(notices(cx, tab).is_empty(), "{n}");
        feed(cx, tab, |t, cx| t.attach_images(vec![big.clone()], cx));
        let pasted = gpui::Image::from_bytes(gpui::ImageFormat::Png, vec![0; max + 1]);
        feed(cx, tab, |t, cx| t.attach_pasted_image(pasted, cx));
        settle_until(n, cx, window, "two notices", |cx| {
            notices(cx, tab).len() == 2
        });
        let mut got = notices(cx, tab);
        got.sort();
        assert_eq!(
            got,
            [
                "Image too big for Claude (3.75 MB max): big-scan.png",
                "Image too big for Claude (3.75 MB max): pasted-image.png"
            ],
            "{n}"
        );
        assert_eq!(
            chips(cx, tab),
            ["\u{1f5bc} exact.png"],
            "{n}: no chip for the others"
        );
        assert!(written(cx, tab).is_empty(), "{n}: nothing sent");
    });
}

/// A long message: the box grows to 8 rows, then scrolls; the chips above and below the text
/// stay put as more is typed.
pub(super) fn agent_box_long_text() {
    layout_scene("agent-box-long-text", |n, cx, window, tab| {
        let line = "Please read src/rsvp.ts and the failing test, then explain why past dates \
                    are accepted and fix the comparison without changing today() at all. ";
        cx.simulate_input(window, "Fix it");
        let _ = frame_fresh(cx, window);
        let one = composer_bounds(cx, tab);
        cx.simulate_input(window, &line.repeat(6));
        let _ = frame_fresh(cx, window);
        let composer = cx.read(|cx| tab.read(cx).shot_composer());
        let (rows, first, _) = cx.read(|cx| composer.read(cx).shot_rows());
        assert!(rows > 8, "{n}: more rows than shown ({rows})");
        assert!(first > 0, "{n}: scrolled to the caret");
        let eight = composer_bounds(cx, tab);
        let ratio = eight.size.height / one.size.height;
        assert!(
            (7.5..8.5).contains(&ratio),
            "{n}: eight rows tall ({ratio})"
        );
        let parts = |cx: &VisualTestAppContext| {
            ["folder", "box", "attach", "model", "mode", "send"].map(|p| part(cx, tab, n, p))
        };
        let before = parts(cx);
        assert!(inside(eight, before[1]), "{n}: the text stays in the box");
        assert!(
            before[0].bottom() <= before[1].top(),
            "{n}: the folder above"
        );
        assert!(before[0].top() >= px(0.0), "{n}: still in the window");
        cx.simulate_input(window, &line.repeat(3));
        let _ = frame_fresh(cx, window);
        assert_eq!(parts(cx), before, "{n}: the chips stay put");
        assert_eq!(
            composer_bounds(cx, tab),
            eight,
            "{n}: no taller than 8 rows"
        );
    });
}

// ---- The folder chip and picker (Task 6) ----

/// A made-up folder under your home (`~/code/<name>`); nothing is read from it.
fn folder(name: &str) -> PathBuf {
    cwd().with_file_name(name)
}

/// What the tab told of its folder: `Folder`, and `NewConversationIn` (as `true`).
type Moves = Rc<RefCell<Vec<(PathBuf, bool)>>>;

/// Shoot folder scene `name` in both themes: the layout scenes' Claude tab in `~/code/my-app`,
/// fuxx knowing (made-up) `~/code/my-app` and `~/code/scratch` as Recent and as projects, Claude
/// knowing `~/code/scratch`, and Claude's version `version` read; what it tells of its folder is
/// kept. The picker is left as `act` leaves it for the shot.
fn folder_scene(
    name: &str,
    act: impl Fn(&str, &mut VisualTestAppContext, AnyWindowHandle, &Entity<AgentTab>, &Moves)
    + Clone
    + 'static,
) {
    for &(slug, theme_name) in shot_themes() {
        let theme = shot_theme(theme_name);
        let slot: Slot = Rc::default();
        let keep = slot.clone();
        let moves: Moves = Rc::default();
        let heard = moves.clone();
        let n = format!("{name}-{slug}");
        let act = act.clone();
        let path = shoot_with(
            name,
            slug,
            &theme,
            size(px(DIMS.0), px(DIMS.1)),
            move |window, cx| {
                cx.set_global(shot_appearance(theme, cx));
                cx.bind_keys(crate::key_bindings());
                let both = vec![folder("my-app"), folder("scratch")];
                cx.set_global(KnownFolders {
                    recent: both.clone(),
                    projects: both,
                });
                let catalogs = cx.default_global::<Catalogs>();
                catalogs.set_known_folders("claude", Lookup::Ready(vec![folder("scratch")]));
                let tab = cx.new(|cx| AgentTab::for_shots(&CLAUDE, example_start(), window, cx));
                cx.subscribe(&tab, move |_, e: &AgentTabEvent, _| match e {
                    AgentTabEvent::Folder(f) => heard.borrow_mut().push((f.clone(), false)),
                    AgentTabEvent::NewConversationIn(f) => {
                        heard.borrow_mut().push((f.clone(), true))
                    }
                    _ => {}
                })
                .detach();
                let focus = tab.read(cx).focus_handle(cx);
                focus.focus(window, cx);
                *keep.borrow_mut() = Some(tab.clone());
                tab.into()
            },
            move |cx, window| {
                let tab = slot.borrow().clone().expect("the agent tab");
                feed(cx, &tab, |t, cx| t.shot_history(Vec::new(), cx));
                feed(cx, &tab, |t, cx| {
                    t.shot_catalog(Lookup::Ready(example_catalog()), cx)
                });
                let _ = frame_fresh(cx, window);
                act(&n, cx, window, &tab, &moves);
                let _ = frame_fresh(cx, window);
                slot.borrow_mut().take();
            },
        );
        println!("ui-shots: {}", path.display());
    }
}

/// Click the folder chip (a real click), and draw what opened.
fn click_folder_chip(
    n: &str,
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    tab: &Entity<AgentTab>,
) {
    let chip = part(cx, tab, n, "folder");
    cx.simulate_click(window, chip.center(), Modifiers::none());
    cx.run_until_parked();
    let _ = frame_fresh(cx, window);
}

/// Click the open menu's row saying `label` (a real click), and draw what follows.
fn click_row(
    n: &str,
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    tab: &Entity<AgentTab>,
    label: &str,
) {
    let row = cx
        .read(|cx| tab.read(cx).shot_menu_row(label))
        .unwrap_or_else(|| panic!("{n}: no row {label} was drawn"));
    cx.simulate_click(window, row.center(), Modifiers::none());
    cx.run_until_parked();
    let _ = frame_fresh(cx, window);
}

fn rows(cx: &VisualTestAppContext, tab: &Entity<AgentTab>) -> Vec<PickerRow> {
    cx.read(|cx| tab.read(cx).shot_picker_rows())
}

fn folder_row_of(name: &str, current: bool) -> PickerRow {
    PickerRow::Folder {
        path: folder(name),
        shown: format!("~/code/{name}"),
        current,
    }
}

fn menu(cx: &VisualTestAppContext, tab: &Entity<AgentTab>) -> Option<BoxMenu> {
    cx.read(|cx| tab.read(cx).shot_menu())
}

fn labels(cx: &VisualTestAppContext, tab: &Entity<AgentTab>) -> Vec<String> {
    cx.read(|cx| tab.read(cx).shot_menu_labels())
}

fn tab_cwd(cx: &VisualTestAppContext, tab: &Entity<AgentTab>) -> PathBuf {
    cx.read(|cx| tab.read(cx).shot_cwd())
}

/// Whether the keyboard is in `focus`'s view.
fn has_keys(
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    focus: impl Fn(&gpui::App) -> gpui::FocusHandle,
) -> bool {
    cx.update_window(window, |_, window, cx| focus(cx).is_focused(window))
        .expect("the window")
}

/// The search box has the keyboard.
fn search_has_keys(
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    tab: &Entity<AgentTab>,
) -> bool {
    let tab = tab.clone();
    has_keys(cx, window, move |cx| {
        let search = tab.read(cx).shot_search().expect("the search box");
        search.read(cx).focus_handle(cx)
    })
}

/// The conversation's first message has gone and Claude said it started (under the id fuxx
/// chose): after this, the picker asks before it moves anything.
fn first_message(cx: &mut VisualTestAppContext, tab: &Entity<AgentTab>) -> String {
    let id = flag(&argv(cx, tab), "--session-id").expect("a new conversation's id");
    feed(cx, tab, |t, cx| t.send("Fix notes.txt".into(), cx));
    let started = Event::Started {
        conversation: id.clone(),
        model: None,
    };
    feed(cx, tab, |t, cx| t.shot_feed(vec![started, turn_done()], cx));
    id
}

/// §2's picker from the chip (a click): its search on top, with the keyboard; Recent (✓ on the
/// folder the agent is in); This Computer ▸, which opens fuxx's and Claude's folders beside it;
/// New Project; No Project. Opening it chooses nothing.
pub(super) fn agent_box_folder_picker() {
    folder_scene("agent-box-folder-picker", |n, cx, window, tab, moves| {
        let before = argv(cx, tab);
        click_folder_chip(n, cx, window, tab);
        assert_eq!(
            menu(cx, tab),
            Some(BoxMenu::Folder),
            "{n}: the chip opens it"
        );
        assert!(
            search_has_keys(cx, window, tab),
            "{n}: the search has the keyboard"
        );
        assert_eq!(
            rows(cx, tab),
            [
                PickerRow::Heading("Recent"),
                folder_row_of("my-app", true),
                folder_row_of("scratch", false),
                PickerRow::ThisComputer,
                PickerRow::NewProject,
                PickerRow::NoProject,
            ],
            "{n}"
        );
        assert_eq!(
            labels(cx, tab),
            [
                "~/code/my-app",
                "~/code/scratch",
                "This Computer",
                "New Project",
                "No Project"
            ],
            "{n}: drawn top to bottom"
        );
        let (the_menu, chip) = (part(cx, tab, n, "menu"), part(cx, tab, n, "folder"));
        assert!(
            the_menu.bottom() <= chip.top(),
            "{n}: it opens up from the chip"
        );
        click_row(n, cx, window, tab, "This Computer");
        assert_eq!(
            cx.read(|cx| tab.read(cx).shot_computer_rows()),
            Some(vec![
                folder_row_of("my-app", true),
                folder_row_of("scratch", false)
            ]),
            "{n}: fuxx's and Claude's folders, each once"
        );
        let beside = part(cx, tab, n, "computer");
        assert!(
            beside.left() >= the_menu.right(),
            "{n}: beside the picker {beside:?} {the_menu:?}"
        );
        assert_eq!(menu(cx, tab), Some(BoxMenu::Folder), "{n}: still open");
        // The chip pressed while it is open closes it (not closed and opened again); the
        // next press opens it.
        click_folder_chip(n, cx, window, tab);
        assert_eq!(menu(cx, tab), None, "{n}: the chip closes it");
        click_folder_chip(n, cx, window, tab);
        assert_eq!(
            menu(cx, tab),
            Some(BoxMenu::Folder),
            "{n}: and opens it again"
        );
        click_row(n, cx, window, tab, "This Computer");
        assert!(
            cx.read(|cx| tab.read(cx).shot_computer_rows()).is_some(),
            "{n}"
        );
        assert_eq!(argv(cx, tab), before, "{n}: nothing restarted");
        assert!(moves.borrow().is_empty(), "{n}: nothing chosen");
        assert!(written(cx, tab).is_empty(), "{n}: nothing sent");
    });
}

/// The search filters every section as you type (`scr` leaves `~/code/scratch`); Esc closes the
/// picker and gives the keyboard back to the box (stopping nothing).
pub(super) fn agent_box_folder_search() {
    folder_scene("agent-box-folder-search", |n, cx, window, tab, moves| {
        click_folder_chip(n, cx, window, tab);
        cx.simulate_keystrokes(window, "escape");
        cx.run_until_parked();
        assert_eq!(menu(cx, tab), None, "{n}: Esc closed it");
        let composer = cx.read(|cx| tab.read(cx).shot_composer());
        assert!(
            has_keys(cx, window, move |cx| composer.read(cx).focus_handle(cx)),
            "{n}: the keyboard is back in the box"
        );
        assert_eq!(cx.read(|cx| tab.read(cx).status()), Status::Idle, "{n}");

        click_folder_chip(n, cx, window, tab);
        cx.simulate_input(window, "scr");
        cx.run_until_parked();
        let _ = frame_fresh(cx, window);
        assert_eq!(
            rows(cx, tab),
            [
                PickerRow::Heading("Recent"),
                folder_row_of("scratch", false)
            ],
            "{n}"
        );
        assert_eq!(labels(cx, tab), ["~/code/scratch"], "{n}: drawn so");
        assert!(moves.borrow().is_empty(), "{n}: typing chooses nothing");
        assert!(written(cx, tab).is_empty(), "{n}: nothing sent");
    });
}

/// After the first message the folder is fixed (chores, your words: "when we already make a
/// prompt, disabled the change project/folder"): a click on its pill opens nothing, asking for
/// the picker another way is refused, the conversation stays where it is and nothing is sent.
pub(super) fn agent_box_folder_locked() {
    folder_scene("agent-box-folder-locked", |n, cx, window, tab, moves| {
        first_message(cx, tab);
        let before = argv(cx, tab);
        let chip = part(cx, tab, n, "folder");
        cx.simulate_click(window, chip.center(), gpui::Modifiers::none());
        cx.run_until_parked();
        assert_eq!(menu(cx, tab), None, "{n}: the pill opens nothing");
        cx.update_window(window, |_, window, cx| {
            tab.update(cx, |t, cx| t.open_folder_picker(window, cx))
        })
        .expect("window");
        assert_eq!(menu(cx, tab), None, "{n}: nor the picker asked for");
        assert_eq!(tab_cwd(cx, tab), folder("my-app"), "{n}: it stays");
        assert_eq!(argv(cx, tab), before, "{n}: not restarted");
        assert!(moves.borrow().is_empty(), "{n}: nothing saved");
    });
}

/// Before the first message a folder chosen just changes where the agent starts: the idle
/// process starts again there on a new conversation (no `--resume`, no row), the folder saved;
/// Return in the search picks its first folder.
pub(super) fn agent_box_folder_before_first() {
    folder_scene(
        "agent-box-folder-before-first",
        |n, cx, window, tab, moves| {
            let first = flag(&argv(cx, tab), "--session-id").expect("a new conversation");
            // Chosen from This Computer's list, beside the picker.
            click_folder_chip(n, cx, window, tab);
            click_row(n, cx, window, tab, "This Computer");
            click_row(n, cx, window, tab, "This Computer \u{25b8} ~/code/scratch");
            assert_eq!(menu(cx, tab), None, "{n}: chosen, it closes");
            assert_eq!(tab_cwd(cx, tab), folder("scratch"), "{n}: it starts there");
            let a = argv(cx, tab);
            assert_eq!(flag(&a, "--resume"), None, "{n}: nothing to resume {a:?}");
            let fresh = flag(&a, "--session-id").expect("a new conversation");
            assert_ne!(fresh, first, "{n}: started again");
            assert!(
                notices(cx, tab).is_empty(),
                "{n}: no row before a conversation"
            );
            assert_eq!(*moves.borrow(), [(folder("scratch"), false)], "{n}: saved");
            assert!(written(cx, tab).is_empty(), "{n}: nothing sent");
            assert_eq!(
                cx.read(|cx| cx
                    .global::<Catalogs>()
                    .catalog("claude", &folder("scratch"))
                    .cloned()),
                Some(Lookup::Loading),
                "{n}: the lists are read for it"
            );
            let composer = cx.read(|cx| tab.read(cx).shot_composer());
            assert!(
                has_keys(cx, window, move |cx| composer.read(cx).focus_handle(cx)),
                "{n}: the keyboard is back in the box"
            );

            // Return in the search picks the first folder listed.
            click_folder_chip(n, cx, window, tab);
            cx.simulate_input(window, "my-app");
            cx.simulate_keystrokes(window, "enter");
            cx.run_until_parked();
            assert_eq!(tab_cwd(cx, tab), folder("my-app"), "{n}: Return chose it");
            assert_eq!(menu(cx, tab), None, "{n}");
            assert!(written(cx, tab).is_empty(), "{n}: and sent nothing");
            assert_eq!(
                cx.read(|cx| tab.read(cx).shot_box(cx)).folder,
                "my-app",
                "{n}"
            );
        },
    );
}

// ---- The model menu (Task 8) ----

/// A made-up Claude list: `Claude Example 5` (five efforts, `high` running in the scenes),
/// `Claude Example 4` (three) and `Claude Tiny` (none).
fn model_catalog() -> Catalog {
    let levels = |ls: &[&str]| {
        ls.iter()
            .map(|l| EffortChoice {
                level: l.to_string(),
                model_id: None,
            })
            .collect::<Vec<_>>()
    };
    let model = |id: &str, label: &str, ls: &[&str]| ModelChoice {
        id: id.into(),
        label: label.into(),
        resolves_to: None,
        efforts: levels(ls),
    };
    Catalog {
        models: vec![
            model(
                "example-5",
                "Claude Example 5",
                &["low", "medium", "high", "xhigh", "max"],
            ),
            model("example-4", "Claude Example 4", &["low", "medium", "high"]),
            model("tiny", "Claude Tiny", &[]),
        ],
        mode: Some("default".into()),
        ..Catalog::default()
    }
}

/// A made-up agy list: the effort is in each level's id (`example-pro-high`).
fn agy_example_catalog() -> Catalog {
    let model = |id: &str, label: &str, ls: &[&str]| ModelChoice {
        id: id.into(),
        label: label.into(),
        resolves_to: None,
        efforts: ls
            .iter()
            .map(|l| EffortChoice {
                level: l.to_string(),
                model_id: Some(format!("{id}-{l}")),
            })
            .collect(),
    };
    Catalog {
        models: vec![
            model("example-pro", "Example Pro", &["low", "medium", "high"]),
            model("example-flash", "Example Flash", &["low", "high"]),
        ],
        ..Catalog::default()
    }
}

/// Claude on `example-5` at `high`, with the made-up list.
fn model_scene(
    name: &str,
    lookup: Lookup<Catalog>,
    act: impl Fn(&str, &mut VisualTestAppContext, AnyWindowHandle, &Entity<AgentTab>) + Clone + 'static,
) {
    box_scene(name, &CLAUDE, example_start(), lookup, act);
}

/// Click the model chip (a real click), and draw what opened.
fn click_model_chip(
    n: &str,
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    tab: &Entity<AgentTab>,
) {
    let chip = part(cx, tab, n, "model");
    cx.simulate_click(window, chip.center(), Modifiers::none());
    cx.run_until_parked();
    let _ = frame_fresh(cx, window);
}

/// Click the `▸` of the model row `label` (the row's right end), and draw what opened.
fn click_more(
    n: &str,
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    tab: &Entity<AgentTab>,
    label: &str,
) {
    let row = cx
        .read(|cx| tab.read(cx).shot_menu_row(label))
        .unwrap_or_else(|| panic!("{n}: no row {label} was drawn"));
    let at = gpui::point(row.right() - px(12.), row.center().y);
    cx.simulate_click(window, at, Modifiers::none());
    cx.run_until_parked();
    let _ = frame_fresh(cx, window);
}

/// Where the effort list's row `level` was drawn (none: not listed).
fn effort_row(
    cx: &VisualTestAppContext,
    tab: &Entity<AgentTab>,
    level: &str,
) -> Option<Bounds<Pixels>> {
    cx.read(|cx| {
        tab.read(cx)
            .shot_menu_row(&format!("Effort \u{25b8} {level}"))
    })
}

fn model_rows_of(cx: &VisualTestAppContext, tab: &Entity<AgentTab>) -> Vec<ModelRow> {
    cx.read(|cx| tab.read(cx).shot_model_rows(cx))
}

/// The words of the model rows that are models, in order.
fn model_names(rows: &[ModelRow]) -> Vec<String> {
    rows.iter()
        .filter_map(|r| match r {
            ModelRow::Model { label, .. } => Some(label.clone()),
            _ => None,
        })
        .collect()
}

/// §4's menu from the chip (a click): the `Model` heading, the models (✓ and the effort, in
/// grey, on the running one; `▸` on those with efforts), a divider and `View Usage ▸`. Opening
/// it chooses nothing; a press on the chip again closes it, and a press on another chip
/// (folder, branch) closes it too.
pub(super) fn agent_box_model_menu() {
    model_scene(
        "agent-box-model-menu",
        Lookup::Ready(model_catalog()),
        |n, cx, window, tab| {
            click_model_chip(n, cx, window, tab);
            assert_eq!(
                menu(cx, tab),
                Some(BoxMenu::Model { efforts_of: None }),
                "{n}"
            );
            let rows = model_rows_of(cx, tab);
            assert_eq!(
                rows[1],
                ModelRow::Model {
                    ix: 0,
                    label: "Claude Example 5".into(),
                    effort: Some("high".into()),
                    current: true,
                    has_efforts: true
                },
                "{n}: ✓ and the effort on the running one"
            );
            assert_eq!(
                model_names(&rows),
                ["Claude Example 5", "Claude Example 4", "Claude Tiny"],
                "{n}"
            );
            assert!(
                matches!(
                    rows[3],
                    ModelRow::Model {
                        has_efforts: false,
                        ..
                    }
                ),
                "{n}"
            );
            // Rows, top to bottom; no Usage row (it is its own button beside the pill).
            let at = |l: &str| {
                cx.read(|cx| tab.read(cx).shot_menu_row(l))
                    .unwrap_or_else(|| panic!("{n}: no row {l}"))
            };
            let (a, b, t) = (
                at("Claude Example 5"),
                at("Claude Example 4"),
                at("Claude Tiny"),
            );
            assert!(a.bottom() <= b.top() && b.bottom() <= t.top(), "{n}");
            assert!(
                cx.read(|cx| tab.read(cx).shot_menu_row("Usage")).is_none(),
                "{n}: no Usage row in the menu"
            );
            let menu_box = part(cx, tab, n, "menu");
            assert!(inside(t, menu_box), "{n}: inside the menu");
            assert!(
                part(cx, tab, n, "model").top() >= menu_box.bottom(),
                "{n}: upward"
            );
            assert!(written(cx, tab).is_empty(), "{n}: opening chooses nothing");

            // The chip again closes it (the press is no reopening).
            click_model_chip(n, cx, window, tab);
            assert_eq!(menu(cx, tab), None, "{n}: the chip closes it");
            // A press on another chip while a menu is open closes it (and opens nothing).
            click_folder_chip(n, cx, window, tab);
            assert_eq!(menu(cx, tab), Some(BoxMenu::Folder), "{n}");
            click_model_chip(n, cx, window, tab);
            assert_eq!(
                menu(cx, tab),
                None,
                "{n}: the model chip's press closes the picker"
            );
            click_model_chip(n, cx, window, tab);
            assert!(matches!(menu(cx, tab), Some(BoxMenu::Model { .. })), "{n}");
            // The folder chip where the open menu leaves it showing (M11al: `text::META` 12.5
            // widened the chip, its middle now under the menu's left edge).
            let (chip, open) = (part(cx, tab, n, "folder"), part(cx, tab, n, "menu"));
            assert!(
                chip.left() + px(8.) < open.left(),
                "{n}: the chip's left shows"
            );
            let at = gpui::point(chip.left() + px(4.), chip.center().y);
            cx.simulate_click(window, at, Modifiers::none());
            cx.run_until_parked();
            let _ = frame_fresh(cx, window);
            assert_eq!(
                menu(cx, tab),
                None,
                "{n}: and the folder chip's, the model menu"
            );
            click_model_chip(n, cx, window, tab);
            assert!(matches!(menu(cx, tab), Some(BoxMenu::Model { .. })), "{n}");
            // And the branch chip's press closes an open menu too.
            feed(cx, tab, |t, cx| {
                t.shot_place(folder("my-app"), Some("master".into()), cx)
            });
            let _ = frame_fresh(cx, window);
            let branch = part(cx, tab, n, "branch");
            cx.simulate_click(window, branch.center(), Modifiers::none());
            cx.run_until_parked();
            assert_eq!(menu(cx, tab), None, "{n}: the branch chip closes the menu");
            // Leave it open for the shot.
            click_model_chip(n, cx, window, tab);
            assert!(menu(cx, tab).is_some(), "{n}");
        },
    );
}

/// Click the mode chip (a real click), and draw what opened.
fn click_mode_chip(
    n: &str,
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    tab: &Entity<AgentTab>,
) {
    let chip = part(cx, tab, n, "mode");
    cx.simulate_click(window, chip.center(), Modifiers::none());
    cx.run_until_parked();
    let _ = frame_fresh(cx, window);
}

fn mode_rows_of(cx: &VisualTestAppContext, tab: &Entity<AgentTab>) -> Vec<ModeRow> {
    cx.read(|cx| tab.read(cx).shot_mode_rows(cx))
}

/// §6's menu from the chip (a click): the `Mode` heading and Claude's four modes, each with its
/// one-line description under it, the one in force ticked. Opening chooses nothing; the chip
/// again closes it, and a press on another chip closes it (and opens nothing). Choosing asks
/// Claude live (`set_permission_mode`), sends no message, and closes the menu.
pub(super) fn agent_box_mode_menu() {
    box_scene(
        "agent-box-mode-menu",
        &CLAUDE,
        start(Some("acceptEdits"), Some("example-5")),
        Lookup::Ready(model_catalog()),
        |n, cx, window, tab| {
            click_mode_chip(n, cx, window, tab);
            assert_eq!(menu(cx, tab), Some(BoxMenu::Mode), "{n}");
            let rows = mode_rows_of(cx, tab);
            let ticked: Vec<&str> = rows
                .iter()
                .filter_map(|r| match r {
                    ModeRow::Mode {
                        label,
                        current: true,
                        ..
                    } => Some(*label),
                    _ => None,
                })
                .collect();
            assert_eq!(ticked, ["Accept edits"], "{n}");
            assert_eq!(rows.len(), 5, "{n}: the heading and four modes");
            let at = |l: &str| {
                cx.read(|cx| tab.read(cx).shot_menu_row(l))
                    .unwrap_or_else(|| panic!("{n}: no row {l}"))
            };
            let (a, b, c, d) = (
                at("Ask before edits"),
                at("Accept edits"),
                at("Plan"),
                at("Auto"),
            );
            assert!(
                a.bottom() <= b.top() && b.bottom() <= c.top() && c.bottom() <= d.top(),
                "{n}: top to bottom"
            );
            let menu_box = part(cx, tab, n, "menu");
            assert!(inside(d, menu_box), "{n}: inside the menu");
            assert!(
                part(cx, tab, n, "mode").top() >= menu_box.bottom(),
                "{n}: upward"
            );
            assert!(written(cx, tab).is_empty(), "{n}: opening chooses nothing");
            // The chip again closes it; a press on another chip closes it and opens nothing.
            click_mode_chip(n, cx, window, tab);
            assert_eq!(menu(cx, tab), None, "{n}: the chip closes it");
            click_model_chip(n, cx, window, tab);
            assert!(matches!(menu(cx, tab), Some(BoxMenu::Model { .. })), "{n}");
            click_mode_chip(n, cx, window, tab);
            assert_eq!(menu(cx, tab), None, "{n}: the mode chip's press closes it");
            click_mode_chip(n, cx, window, tab);
            click_folder_chip(n, cx, window, tab);
            assert_eq!(menu(cx, tab), None, "{n}: and the folder chip's, this one");
            // Choosing: live, nothing sent.
            click_mode_chip(n, cx, window, tab);
            click_row(n, cx, window, tab, "Plan");
            assert_eq!(menu(cx, tab), None, "{n}: chosen, it closes");
            let sent = written(cx, tab);
            let asks: Vec<&serde_json::Value> = sent
                .iter()
                .filter(|l| l["type"] == "control_request")
                .collect();
            assert_eq!(asks.len(), 1, "{n}: {sent:?}");
            assert_eq!(asks[0]["request"]["subtype"], "set_permission_mode", "{n}");
            assert_eq!(asks[0]["request"]["mode"], "plan", "{n}");
            assert!(sent.iter().all(|l| l["type"] != "user"), "{n}: no message");
            // Left open for the shot.
            click_mode_chip(n, cx, window, tab);
            assert_eq!(menu(cx, tab), Some(BoxMenu::Mode), "{n}");
        },
    );
}

/// agy after a message is no longer locked (M11aa's lock goes): the chip opens the menu, and
/// choosing `plan` restarts agy on the same conversation with `--mode plan`, with one quiet row
/// and no message. agy's own `init` says `request-review` whatever it runs, so the chip shows
/// what was chosen.
pub(super) fn agent_box_mode_agy_after_message() {
    box_scene(
        "agent-box-mode-agy-after-message",
        &AGY,
        start(Some("accept-edits"), None),
        Lookup::Ready(agy_example_catalog()),
        |n, cx, window, tab| {
            let id = agy_conversation();
            feed(cx, tab, |t, cx| t.send("Say hello in one word".into(), cx));
            feed(cx, tab, |t, cx| t.shot_feed(agy_text(), cx));
            assert_eq!(cx.read(|cx| tab.read(cx).status()), Status::Done, "{n}");
            let sent = written(cx, tab).len();
            click_mode_chip(n, cx, window, tab);
            assert_eq!(menu(cx, tab), Some(BoxMenu::Mode), "{n}: not locked");
            assert_eq!(
                labels(cx, tab),
                ["Accept edits", "Auto", "Plan"],
                "{n}: agy's three"
            );
            click_row(n, cx, window, tab, "Plan");
            assert_eq!(menu(cx, tab), None, "{n}: chosen, it closes");
            let a = argv(cx, tab);
            assert_eq!(flag(&a, "--conversation"), Some(id), "{n}: {a:?}");
            assert_eq!(flag(&a, "--mode").as_deref(), Some("plan"), "{n}: {a:?}");
            assert_eq!(notices(cx, tab), ["Switched to Plan"], "{n}");
            assert_eq!(written(cx, tab).len(), sent, "{n}: nothing sent");
            let texts = cx.read(|cx| tab.read(cx).shot_box(cx));
            assert_eq!(texts.mode.as_deref(), Some("Plan"), "{n}");
            // Left open, on the new mode, for the shot.
            click_mode_chip(n, cx, window, tab);
            let rows = mode_rows_of(cx, tab);
            assert!(
                rows.iter().any(|r| matches!(
                    r,
                    ModeRow::Mode {
                        id: "plan",
                        current: true,
                        ..
                    }
                )),
                "{n}: {rows:?}"
            );
        },
    );
}

/// Claude with no mode chosen shows the one its catalog reports: `default` as `Ask before
/// edits`, ticked in the menu.
pub(super) fn agent_box_mode_reported() {
    let list = Catalog {
        mode: Some("default".into()),
        ..model_catalog()
    };
    box_scene(
        "agent-box-mode-reported",
        &CLAUDE,
        start(None, Some("example-5")),
        Lookup::Ready(list),
        |n, cx, window, tab| {
            let texts = cx.read(|cx| tab.read(cx).shot_box(cx));
            assert_eq!(texts.mode.as_deref(), Some("Ask before edits"), "{n}");
            click_mode_chip(n, cx, window, tab);
            let rows = mode_rows_of(cx, tab);
            assert!(
                rows.iter().any(|r| matches!(
                    r,
                    ModeRow::Mode {
                        id: "default",
                        current: true,
                        ..
                    }
                )),
                "{n}: {rows:?}"
            );
        },
    );
}

/// R9: Claude leaving plan mode by itself (a plan approved) is followed: the chip, the menu's ✓
/// and the saved mode say the mode it reports, so choosing Plan again asks for it (and a
/// restart doesn't resume a stale plan mode). While a change of yours is pending (asked, or
/// waiting for the turn), what Claude reports is not taken: yours is what it runs next.
pub(super) fn agent_box_mode_follows_claude() {
    run_scene(
        "agent-box-mode-follows-claude",
        &CLAUDE,
        start(Some("plan"), Some("opus")),
        |n, cx, tab, told| {
            let chip = |cx: &VisualTestAppContext| cx.read(|cx| tab.read(cx).shot_box(cx).mode);
            let mode =
                |cx: &VisualTestAppContext| cx.read(|cx| tab.read(cx).mode().map(str::to_string));
            feed(cx, tab, |t, cx| t.send("Plan the fix".into(), cx));
            // The plan is approved: Claude says it runs in acceptEdits now.
            feed(cx, tab, |t, cx| {
                t.shot_feed(vec![Event::Mode("acceptEdits".into())], cx)
            });
            assert_eq!(mode(cx).as_deref(), Some("acceptEdits"), "{n}");
            assert_eq!(
                chip(cx).as_deref(),
                Some("Accept edits"),
                "{n}: the chip follows"
            );
            assert_eq!(
                told.borrow().last().cloned(),
                Some((Some("opus".into()), None, Some("acceptEdits".into()))),
                "{n}: saved"
            );
            let saved = told.borrow().len();
            // The same mode again changes nothing (nothing told).
            feed(cx, tab, |t, cx| {
                t.shot_feed(vec![Event::Mode("acceptEdits".into())], cx)
            });
            assert_eq!(told.borrow().len(), saved, "{n}: nothing new to save");
            feed(cx, tab, |t, cx| t.shot_feed(vec![turn_done()], cx));
            assert_eq!(cx.read(|cx| tab.read(cx).status()), Status::Done, "{n}");

            // Plan chosen again is asked for (it isn't "already plan").
            let before = written(cx, tab).len();
            feed(cx, tab, |t, cx| t.choose(Change::Mode("plan".into()), cx));
            let asks = written(cx, tab).split_off(before);
            assert_eq!(asks.len(), 1, "{n}: {asks:?}");
            assert_eq!(asks[0]["request"]["subtype"], "set_permission_mode", "{n}");
            assert_eq!(asks[0]["request"]["mode"], "plan", "{n}");
            assert!(asks.iter().all(|l| l["type"] != "user"), "{n}: no message");
            // Asked and not yet answered: a mode Claude reports meanwhile is not taken.
            feed(cx, tab, |t, cx| {
                t.shot_feed(vec![Event::Mode("default".into())], cx)
            });
            assert_eq!(mode(cx).as_deref(), Some("acceptEdits"), "{n}: ignored");
            assert_eq!(told.borrow().len(), saved, "{n}: nothing saved");
            feed(cx, tab, |t, cx| {
                t.shot_feed(
                    vec![Event::Applied {
                        kind: ChangeKind::Mode,
                        value: "plan".into(),
                        ok: true,
                    }],
                    cx,
                )
            });
            assert_eq!(mode(cx).as_deref(), Some("plan"), "{n}: yours, confirmed");
            let saved = told.borrow().len();

            // Waiting for the turn: what Claude reports meanwhile is not taken either.
            feed(cx, tab, |t, cx| t.send("Go on".into(), cx));
            feed(cx, tab, |t, cx| {
                t.choose(Change::Mode("acceptEdits".into()), cx)
            });
            assert_eq!(
                cx.read(|cx| tab.read(cx).waiting_change().cloned()),
                Some(Change::Mode("acceptEdits".into())),
                "{n}: it waits"
            );
            feed(cx, tab, |t, cx| {
                t.shot_feed(vec![Event::Mode("default".into())], cx)
            });
            assert_eq!(
                mode(cx).as_deref(),
                Some("plan"),
                "{n}: ignored while waiting"
            );
            assert_eq!(told.borrow().len(), saved, "{n}: nothing saved");
        },
    );
}

/// `▸` on a model opens its effort list beside the menu: only that model's levels, the running
/// one ticked on the running model; the list follows the `▸` pressed; a model without efforts has
/// no `▸`. A press outside both closes them; one on the effort list is a pick, not outside.
pub(super) fn agent_box_effort_list() {
    model_scene(
        "agent-box-effort-list",
        Lookup::Ready(model_catalog()),
        |n, cx, window, tab| {
            click_model_chip(n, cx, window, tab);
            click_more(n, cx, window, tab, "Claude Example 4");
            assert_eq!(
                menu(cx, tab),
                Some(BoxMenu::Model {
                    efforts_of: Some(1)
                }),
                "{n}"
            );
            assert!(
                effort_row(cx, tab, "Low").is_some() && effort_row(cx, tab, "High").is_some(),
                "{n}"
            );
            assert!(
                effort_row(cx, tab, "XHigh").is_none(),
                "{n}: Example 4 has three levels only"
            );
            click_more(n, cx, window, tab, "Claude Example 5");
            let levels = ["Low", "Medium", "High", "XHigh", "Max"];
            let spots: Vec<_> = levels.iter().map(|l| effort_row(cx, tab, l)).collect();
            assert!(
                spots.iter().all(Option::is_some),
                "{n}: Low … Max {spots:?}"
            );
            let tops: Vec<_> = spots.iter().map(|b| b.unwrap().top()).collect();
            assert!(
                tops.windows(2).all(|w| w[0] < w[1]),
                "{n}: in order {tops:?}"
            );
            let side = part(cx, tab, n, "effort");
            let menu_box = part(cx, tab, n, "menu");
            assert!(side.left() >= menu_box.right(), "{n}: beside the menu");
            assert!(inside(spots[0].unwrap(), side), "{n}");
            assert!(written(cx, tab).is_empty(), "{n}: opening chooses nothing");
            // A model with no efforts has no ▸: its right end picks it.
            assert!(
                !matches!(
                    model_rows_of(cx, tab)[3],
                    ModelRow::Model {
                        has_efforts: true,
                        ..
                    }
                ),
                "{n}"
            );
            // A press outside closes both.
            cx.simulate_click(window, gpui::point(px(30.), px(120.)), Modifiers::none());
            cx.run_until_parked();
            assert_eq!(menu(cx, tab), None, "{n}: outside closes");
            // Reopened, the effort list is open on the first model for the shot.
            click_model_chip(n, cx, window, tab);
            click_more(n, cx, window, tab, "Claude Example 5");
            assert!(effort_row(cx, tab, "Max").is_some(), "{n}");
        },
    );
}

/// agy's models grouped by name, the effort in each id: `▸` lists that model's levels. Choosing
/// before the first message restarts agy with `--model <id with the effort>` and no `--effort`.
pub(super) fn agent_box_model_agy() {
    box_scene(
        "agent-box-model-agy",
        &AGY,
        start(None, Some("example-pro-high")),
        Lookup::Ready(agy_example_catalog()),
        |n, cx, window, tab| {
            click_model_chip(n, cx, window, tab);
            let rows = model_rows_of(cx, tab);
            assert_eq!(model_names(&rows), ["Example Pro", "Example Flash"], "{n}");
            assert!(
                matches!(&rows[1], ModelRow::Model { current: true, effort: Some(e), .. } if e == "high"),
                "{n}: the effort rides in the id: {rows:?}"
            );
            click_more(n, cx, window, tab, "Example Flash");
            assert!(
                cx.read(|cx| tab.read(cx).shot_menu_row("Effort \u{25b8} Medium"))
                    .is_none(),
                "{n}: Example Flash has no medium"
            );
            click_more(n, cx, window, tab, "Example Pro");
            click_row(n, cx, window, tab, "Effort \u{25b8} Low");
            assert_eq!(menu(cx, tab), None, "{n}: chosen, it closes");
            let a = argv(cx, tab);
            assert_eq!(
                flag(&a, "--model").as_deref(),
                Some("example-pro-low"),
                "{n}: {a:?}"
            );
            assert_eq!(flag(&a, "--effort"), None, "{n}: never --effort {a:?}");
            // Another model: its id with the effort it keeps (low: Flash has it).
            click_model_chip(n, cx, window, tab);
            click_row(n, cx, window, tab, "Example Flash");
            let a = argv(cx, tab);
            assert_eq!(
                flag(&a, "--model").as_deref(),
                Some("example-flash-low"),
                "{n}: {a:?}"
            );
            // Left open, on the effort list, for the shot.
            click_model_chip(n, cx, window, tab);
            click_more(n, cx, window, tab, "Example Pro");
            assert!(
                cx.read(|cx| tab.read(cx).shot_menu_row("Effort \u{25b8} High"))
                    .is_some(),
                "{n}"
            );
        },
    );
}

/// While the list is read the menu says `Loading…` (the divider and View Usage stay).
pub(super) fn agent_box_model_loading() {
    model_scene(
        "agent-box-model-loading",
        Lookup::Loading,
        |n, cx, window, tab| {
            click_model_chip(n, cx, window, tab);
            assert_eq!(
                model_rows_of(cx, tab),
                [ModelRow::Heading, ModelRow::Note("Loading\u{2026}"),],
                "{n}"
            );
        },
    );
}

/// A list that could not be read says so; no models are made up.
pub(super) fn agent_box_model_failed() {
    model_scene(
        "agent-box-model-failed",
        Lookup::Failed("claude: not found".into()),
        |n, cx, window, tab| {
            click_model_chip(n, cx, window, tab);
            assert_eq!(
                model_rows_of(cx, tab),
                [
                    ModelRow::Heading,
                    ModelRow::Note("Couldn't read the model list"),
                ],
                "{n}"
            );
        },
    );
}

/// The agent's own aliases stand in when it lists none: marked `(default list)`; Haiku (which
/// takes no effort) has no `▸`.
pub(super) fn agent_box_model_fallback() {
    let list = Catalog {
        models: CLAUDE.fallback_models(),
        fallback_models: true,
        ..Catalog::default()
    };
    box_scene(
        "agent-box-model-fallback",
        &CLAUDE,
        start(None, None),
        Lookup::Ready(list),
        |n, cx, window, tab| {
            click_model_chip(n, cx, window, tab);
            let rows = model_rows_of(cx, tab);
            assert_eq!(rows[1], ModelRow::Note("(default list)"), "{n}");
            assert_eq!(
                model_names(&rows),
                ["Opus", "Sonnet", "Haiku", "Fable"],
                "{n}"
            );
            let efforts = |label: &str| {
                rows.iter().find_map(|r| match r {
                    ModelRow::Model {
                        label: l,
                        has_efforts,
                        ..
                    } if l == label => Some(*has_efforts),
                    _ => None,
                })
            };
            assert_eq!(efforts("Haiku"), Some(false), "{n}: Haiku takes no effort");
            assert_eq!(efforts("Opus"), Some(true), "{n}");
            assert!(written(cx, tab).is_empty(), "{n}");
        },
    );
}

/// Choosing never sends a message: idle Claude writes the set request (a model, then an effort
/// of the running model: an effort alone), and no `user` line; the menu closes and the keyboard
/// is back in the box.
pub(super) fn agent_box_model_choose_sends_nothing() {
    model_scene(
        "agent-box-model-choose-sends-nothing",
        Lookup::Ready(model_catalog()),
        |n, cx, window, tab| {
            let subtypes = |cx: &VisualTestAppContext, from: usize| -> Vec<String> {
                written(cx, tab)[from..]
                    .iter()
                    .map(|l| l["request"]["subtype"].as_str().unwrap_or("").to_string())
                    .collect()
            };
            let lines = |cx: &VisualTestAppContext| written(cx, tab).len();
            // A model: asked as a model change (its effort is kept where it has it).
            let before = lines(cx);
            click_model_chip(n, cx, window, tab);
            click_row(n, cx, window, tab, "Claude Example 4");
            assert_eq!(menu(cx, tab), None, "{n}: chosen, it closes");
            let sent = written(cx, tab);
            assert_eq!(subtypes(cx, before), ["set_model"], "{n}: {sent:?}");
            assert_eq!(sent[before]["request"]["model"], "example-4", "{n}");
            assert!(sent.iter().all(|l| l["type"] != "user"), "{n}: no message");
            assert_eq!(said(cx, tab), Vec::<String>::new(), "{n}: nothing said");
            // Claude confirms: the chip follows.
            feed(cx, tab, |t, cx| {
                t.shot_feed(
                    vec![Event::Applied {
                        kind: ChangeKind::Model,
                        value: "example-4".into(),
                        ok: true,
                    }],
                    cx,
                )
            });
            let texts = cx.read(|cx| tab.read(cx).shot_box(cx));
            assert_eq!(texts.model, "Claude Example 4  High", "{n}: {texts:?}");
            // An effort of the running model alone: an effort request.
            let before = lines(cx);
            click_model_chip(n, cx, window, tab);
            click_more(n, cx, window, tab, "Claude Example 4");
            click_row(n, cx, window, tab, "Effort \u{25b8} Low");
            assert_eq!(
                subtypes(cx, before),
                ["apply_flag_settings", "get_settings"],
                "{n}"
            );
            assert!(written(cx, tab).iter().all(|l| l["type"] != "user"), "{n}");
            // A model with no efforts: the model alone.
            let before = lines(cx);
            click_model_chip(n, cx, window, tab);
            click_row(n, cx, window, tab, "Claude Tiny");
            assert_eq!(subtypes(cx, before), ["set_model"], "{n}");
            let composer = cx.read(|cx| tab.read(cx).shot_composer());
            assert!(
                has_keys(cx, window, move |cx| composer.read(cx).focus_handle(cx)),
                "{n}: the keyboard is back in the box"
            );
        },
    );
}

/// While Claude works, a model picked waits for the turn; an effort picked meanwhile belongs to
/// that picked model (the menu shows it as current): they compose into one change, never one
/// replacing the other. An effort of the model that runs, picked while a model waits, is a
/// choice of that model again: the waiting change is dropped.
pub(super) fn agent_box_model_effort_composes() {
    model_scene(
        "agent-box-model-effort-composes",
        Lookup::Ready(model_catalog()),
        |n, cx, window, tab| {
            feed(cx, tab, |t, cx| t.send("Fix notes.txt".into(), cx));
            assert_eq!(cx.read(|cx| tab.read(cx).status()), Status::Working, "{n}");
            let before = written(cx, tab).len();
            let waiting =
                |cx: &VisualTestAppContext| cx.read(|cx| tab.read(cx).waiting_change().cloned());
            click_model_chip(n, cx, window, tab);
            click_row(n, cx, window, tab, "Claude Example 4");
            assert_eq!(
                waiting(cx),
                Some(Change::Model {
                    id: "example-4".into(),
                    effort: Some("high".into())
                }),
                "{n}: waits, keeping the effort it has"
            );
            // Example 4 now shows as current (✓), with its effort.
            click_model_chip(n, cx, window, tab);
            let rows = model_rows_of(cx, tab);
            assert!(
                matches!(&rows[2], ModelRow::Model { current: true, effort: Some(e), .. } if e == "high"),
                "{n}: the picked model is the current one: {rows:?}"
            );
            assert!(
                matches!(&rows[1], ModelRow::Model { current: false, .. }),
                "{n}"
            );
            // An effort of it: the same model, the new effort, in one waiting change.
            click_more(n, cx, window, tab, "Claude Example 4");
            click_row(n, cx, window, tab, "Effort \u{25b8} Low");
            assert_eq!(
                waiting(cx),
                Some(Change::Model {
                    id: "example-4".into(),
                    effort: Some("low".into())
                }),
                "{n}: model and effort together"
            );
            assert_eq!(
                written(cx, tab).len(),
                before,
                "{n}: nothing written mid-turn"
            );
            // After the turn: the model, then the effort, once.
            feed(cx, tab, |t, cx| t.shot_feed(vec![turn_done()], cx));
            let lines = written(cx, tab);
            let subtypes: Vec<&str> = lines[before..]
                .iter()
                .map(|l| l["request"]["subtype"].as_str().unwrap_or(""))
                .collect();
            assert_eq!(
                subtypes,
                ["set_model", "apply_flag_settings", "get_settings"],
                "{n}"
            );
            assert_eq!(lines[before]["request"]["model"], "example-4", "{n}");
            assert!(
                lines.iter().filter(|l| l["type"] == "user").count() == 1,
                "{n}: only yours"
            );
        },
    );
}

// ---- View Usage (Task 9) ----

use crate::agent_box::now_unix;

/// A limit read, Claude's way: the agent's own words for when it resets.
fn claude_limit(name: &str, left_pct: u8, resets: &str) -> UsageLimit {
    UsageLimit::Read {
        name: name.into(),
        left_pct,
        resets: Some(Reset::Words(resets.into())),
    }
}

fn claude_usage() -> Lookup<Vec<UsageGroup>> {
    Lookup::Ready(vec![UsageGroup {
        name: String::new(),
        about: None,
        limits: vec![
            claude_limit("Current session", 100, "Nov 17 at 8:10pm"),
            claude_limit("Current week (all models)", 63, "Nov 22 at 10pm"),
        ],
    }])
}

/// Press the usage button beside the model pill (a real click): its panel opens over it.
fn open_usage(
    n: &str,
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    tab: &Entity<AgentTab>,
) {
    let button = part(cx, tab, n, "usage-button");
    cx.simulate_click(window, button.center(), Modifiers::none());
    cx.run_until_parked();
    let _ = frame_fresh(cx, window);
    assert_eq!(menu(cx, tab), Some(BoxMenu::Usage), "{n}");
}

fn usage_rows_of(cx: &VisualTestAppContext, tab: &Entity<AgentTab>) -> Vec<UsageRow> {
    cx.read(|cx| tab.read(cx).shot_usage_rows(cx))
}

fn limit_row(name: &str, left: Option<u8>, resets: Option<&str>, note: Option<&str>) -> UsageRow {
    UsageRow::Limit {
        name: name.into(),
        used_pct: left.map(|l| 100 - l),
        resets: resets.map(String::from),
        note: note.map(String::from),
    }
}

/// The usage panel is drawn beside the model menu, each of `names` has its row in it, top to
/// bottom.
fn assert_usage_panel(n: &str, cx: &VisualTestAppContext, tab: &Entity<AgentTab>, names: &[&str]) {
    // Over its button, beside the model pill (your word, 2026-11-21).
    let side = part(cx, tab, n, "usage");
    let button = part(cx, tab, n, "usage-button");
    assert!(side.bottom() <= button.top(), "{n}: over its button");
    let spots: Vec<Bounds<Pixels>> = names
        .iter()
        .map(|name| {
            cx.read(|cx| {
                tab.read(cx)
                    .shot_menu_row(&format!("Usage \u{25b8} {name}"))
            })
            .unwrap_or_else(|| panic!("{n}: no usage row {name}"))
        })
        .collect();
    assert!(spots.iter().all(|r| inside(*r, side)), "{n}: {spots:?}");
    assert!(
        spots.windows(2).all(|w| w[0].bottom() <= w[1].top()),
        "{n}: in order {spots:?}"
    );
}

/// §5, Claude: `View Usage ▸` opens the panel beside the menu: each limit's name, `N% left` with
/// its ring, and `Resets …` in Claude's own words. Opening reads nothing from the screen.
pub(super) fn agent_box_usage_claude() {
    model_scene(
        "agent-box-usage-claude",
        Lookup::Ready(model_catalog()),
        |n, cx, window, tab| {
            feed(cx, tab, |t, cx| {
                t.shot_usage(claude_usage(), Vec::new(), cx)
            });
            open_usage(n, cx, window, tab);
            assert_eq!(
                usage_rows_of(cx, tab),
                [
                    limit_row(
                        "Current session",
                        Some(100),
                        Some("Resets Nov 17 at 8:10pm"),
                        None
                    ),
                    limit_row(
                        "Current week (all models)",
                        Some(63),
                        Some("Resets Nov 22 at 10pm"),
                        None
                    ),
                ],
                "{n}"
            );
            assert_usage_panel(
                n,
                cx,
                tab,
                &["Current session", "Current week (all models)"],
            );
            assert!(
                written(cx, tab).is_empty(),
                "{n}: nothing sent to the agent"
            );
            // The button again closes it.
            let button = part(cx, tab, n, "usage-button");
            cx.simulate_click(window, button.center(), Modifiers::none());
            cx.run_until_parked();
            assert_eq!(menu(cx, tab), None, "{n}: the button closes it");
            // And the panel is back for the shot.
            open_usage(n, cx, window, tab);
            assert_usage_panel(n, cx, tab, &["Current session"]);
        },
    );
}

/// §5, agy: two groups, each named, each limit with its ring; a reset moment counts down.
pub(super) fn agent_box_usage_agy() {
    box_scene(
        "agent-box-usage-agy",
        &AGY,
        start(None, Some("example-pro-high")),
        Lookup::Ready(agy_example_catalog()),
        |n, cx, window, tab| {
            let at = |secs: i64| Some(Reset::At(now_unix() + secs + 30));
            let read = |name: &str, left: u8, resets| UsageLimit::Read {
                name: name.into(),
                left_pct: left,
                resets,
            };
            let found = Lookup::Ready(vec![
                UsageGroup {
                    name: "Example Pro".into(),
                    about: None,
                    limits: vec![
                        read("5 hour", 23, at(2 * 3600 + 5 * 60)),
                        read("Weekly", 78, at(3 * 86_400 + 16 * 3600)),
                    ],
                },
                UsageGroup {
                    name: "Example Flash".into(),
                    about: None,
                    limits: vec![read("Daily", 100, at(59 * 60)), read("Weekly", 0, None)],
                },
            ]);
            feed(cx, tab, |t, cx| t.shot_usage(found, Vec::new(), cx));
            open_usage(n, cx, window, tab);
            assert_eq!(
                usage_rows_of(cx, tab),
                [
                    UsageRow::Heading("Example Pro".into()),
                    limit_row("5 hour", Some(23), Some("Resets in 2h 5m"), None),
                    limit_row("Weekly", Some(78), Some("Resets in 3d 16h"), None),
                    UsageRow::Heading("Example Flash".into()),
                    limit_row("Daily", Some(100), Some("Resets in 59m"), None),
                    limit_row("Weekly", Some(0), None, None),
                ],
                "{n}"
            );
            let side = part(cx, tab, n, "usage");
            assert!(
                side.right() <= px(DIMS.0),
                "{n}: the panel is on screen {side:?}"
            );
        },
    );
}

/// While the reading is made the panel says `Checking…`; opening asks for it (the scene tab runs
/// no process: it marks the reading loading).
pub(super) fn agent_box_usage_checking() {
    model_scene(
        "agent-box-usage-checking",
        Lookup::Ready(model_catalog()),
        |n, cx, window, tab| {
            assert_eq!(
                cx.read(|cx| tab.read(cx).shot_usage_lookup(cx)),
                None,
                "{n}"
            );
            open_usage(n, cx, window, tab);
            assert_eq!(
                cx.read(|cx| tab.read(cx).shot_usage_lookup(cx)),
                Some(Lookup::Loading),
                "{n}: opening asked"
            );
            assert_eq!(
                usage_rows_of(cx, tab),
                [UsageRow::Quiet(CHECKING.into())],
                "{n}"
            );
            // The answer fills the panel in (no reopening).
            feed(cx, tab, |t, cx| {
                t.shot_usage(claude_usage(), Vec::new(), cx)
            });
            let _ = frame_fresh(cx, window);
            assert_eq!(usage_rows_of(cx, tab).len(), 2, "{n}");
            // A later reading that failed keeps the numbers it had (M11ap: they never blank out;
            // the status bar's popover says "Couldn't refresh").
            feed(cx, tab, |t, cx| {
                t.shot_usage(Lookup::Failed("exit 1".into()), Vec::new(), cx)
            });
            let _ = frame_fresh(cx, window);
            let rows = usage_rows_of(cx, tab);
            assert_eq!(rows.len(), 3, "{n}: kept, and said to be old");
            assert_eq!(
                rows[2],
                UsageRow::Quiet("Couldn't refresh \u{b7} updated just now".into()),
                "{n}"
            );
            // And the shot is of it reading again: the numbers stay (M11ap), not "Checking…".
            feed(cx, tab, |t, cx| {
                t.shot_usage(Lookup::Loading, Vec::new(), cx)
            });
            let _ = frame_fresh(cx, window);
            assert_eq!(usage_rows_of(cx, tab).len(), 2, "{n}: kept while it reads");
        },
    );
}

/// A line fuxx couldn't read is shown as the agent wrote it, beside the limits it could.
pub(super) fn agent_box_usage_text_line() {
    model_scene(
        "agent-box-usage-text-line",
        Lookup::Ready(model_catalog()),
        |n, cx, window, tab| {
            let found = Lookup::Ready(vec![UsageGroup {
                name: String::new(),
                about: None,
                limits: vec![
                    claude_limit("Current session", 87, "Nov 17 at 8:10pm"),
                    UsageLimit::Text("Extra usage: off".into()),
                ],
            }]);
            feed(cx, tab, |t, cx| t.shot_usage(found, Vec::new(), cx));
            open_usage(n, cx, window, tab);
            assert_eq!(
                usage_rows_of(cx, tab),
                [
                    limit_row(
                        "Current session",
                        Some(87),
                        Some("Resets Nov 17 at 8:10pm"),
                        None
                    ),
                    UsageRow::Text("Extra usage: off".into()),
                ],
                "{n}"
            );
        },
    );
}

/// A limit the agent says is close: the model chip carries a warning dot, the limit says
/// `Close to limit`; when it is back to fine the dot goes (the state comes in as the agent's
/// own `rate_limit_event`).
pub(super) fn agent_box_usage_limit_close() {
    model_scene(
        "agent-box-usage-limit-close",
        Lookup::Ready(model_catalog()),
        |n, cx, window, tab| {
            let said = |state| {
                vec![Event::RateLimit(RateLimit {
                    limit: "Current session".into(),
                    state,
                    resets_at: None,
                    used_pct: None,
                })]
            };
            let warning =
                |cx: &VisualTestAppContext| cx.read(|cx| tab.read(cx).shot_box(cx).limit_warning);
            feed(cx, tab, |t, cx| {
                t.shot_usage(claude_usage(), Vec::new(), cx)
            });
            let _ = frame_fresh(cx, window);
            assert!(!warning(cx), "{n}: fine: no dot");
            assert!(
                cx.read(|cx| tab.read(cx).shot_part("limit-warning"))
                    .is_none(),
                "{n}"
            );
            feed(cx, tab, |t, cx| t.shot_feed(said(LimitState::Close), cx));
            let _ = frame_fresh(cx, window);
            assert!(warning(cx), "{n}: close: the dot");
            // On the usage button, where usage is now (beside the model pill).
            let chip = part(cx, tab, n, "usage-button");
            let dot = part(cx, tab, n, "limit-warning");
            assert!(inside(dot, chip), "{n}: on the button {dot:?} {chip:?}");
            assert!(
                chip.left() >= part(cx, tab, n, "model").right(),
                "{n}: the button after the model pill"
            );
            // Back to fine.
            feed(cx, tab, |t, cx| t.shot_feed(said(LimitState::Fine), cx));
            assert!(!warning(cx), "{n}: fine again");
            feed(cx, tab, |t, cx| t.shot_feed(said(LimitState::Close), cx));
            open_usage(n, cx, window, tab);
            assert_eq!(
                usage_rows_of(cx, tab)[0],
                limit_row(
                    "Current session",
                    Some(100),
                    Some("Resets Nov 17 at 8:10pm"),
                    Some("Close to limit")
                ),
                "{n}"
            );
            // Reached, with its moment: the line says when.
            feed(cx, tab, |t, cx| {
                t.shot_feed(
                    vec![Event::RateLimit(RateLimit {
                        limit: "Current session".into(),
                        state: LimitState::Reached,
                        resets_at: Some(now_unix() + 3600),
                        used_pct: None,
                    })],
                    cx,
                )
            });
            let _ = frame_fresh(cx, window);
            let UsageRow::Limit {
                note: Some(note), ..
            } = &usage_rows_of(cx, tab)[0]
            else {
                panic!("{n}: a note under the limit");
            };
            assert!(
                note.starts_with("Limit reached \u{b7} resets "),
                "{n}: {note}"
            );
            // Leave it close for the shot.
            feed(cx, tab, |t, cx| t.shot_feed(said(LimitState::Close), cx));
            let _ = frame_fresh(cx, window);
        },
    );
}

/// Opening the panel again within a minute starts no new reading (and shows the one it has).
pub(super) fn agent_box_usage_cached() {
    model_scene(
        "agent-box-usage-cached",
        Lookup::Ready(model_catalog()),
        |n, cx, window, tab| {
            feed(cx, tab, |t, cx| {
                t.shot_usage(claude_usage(), Vec::new(), cx)
            });
            let ready = Some(claude_usage());
            for round in 0..2 {
                open_usage(n, cx, window, tab);
                assert_eq!(
                    cx.read(|cx| tab.read(cx).shot_usage_lookup(cx)),
                    ready,
                    "{n}: round {round}: no new reading was begun"
                );
                assert_eq!(usage_rows_of(cx, tab).len(), 2, "{n}");
                feed(cx, tab, |t, cx| t.close_menu(cx));
            }
            // A minute on, the reading is stale (a reopening would read again) but the open
            // panel keeps its numbers, not `Checking…`.
            open_usage(n, cx, window, tab);
            feed(cx, tab, |t, cx| t.shot_age_usage(61, cx));
            let _ = frame_fresh(cx, window);
            assert_eq!(
                cx.read(|cx| tab.read(cx).shot_usage_lookup(cx)),
                None,
                "{n}: stale"
            );
            assert_eq!(usage_rows_of(cx, tab).len(), 2, "{n}: the numbers stay");
        },
    );
}

/// A model with no efforts, picked mid-turn, shows none in the menu (not the last model's).
pub(super) fn agent_box_model_waiting_no_efforts() {
    model_scene(
        "agent-box-model-waiting-no-efforts",
        Lookup::Ready(model_catalog()),
        |n, cx, window, tab| {
            feed(cx, tab, |t, cx| t.send("Fix notes.txt".into(), cx));
            click_model_chip(n, cx, window, tab);
            click_row(n, cx, window, tab, "Claude Tiny");
            assert_eq!(
                cx.read(|cx| tab.read(cx).waiting_change().cloned()),
                Some(Change::Model {
                    id: "tiny".into(),
                    effort: None
                }),
                "{n}"
            );
            click_model_chip(n, cx, window, tab);
            let rows = model_rows_of(cx, tab);
            assert!(
                matches!(
                    &rows[3],
                    ModelRow::Model {
                        current: true,
                        effort: None,
                        ..
                    }
                ),
                "{n}: Tiny is current with no effort: {rows:?}"
            );
        },
    );
}

// ---- The branch chip (Task 7) ----

/// §3: `⑂ master ▾` beside `📁 my-app ▾` when the folder is in a repo; no chip when it isn't.
pub(super) fn agent_box_branch() {
    layout_scene("agent-box-branch", |n, cx, window, tab| {
        // A folder that isn't in a repo: no chip.
        let texts = cx.read(|cx| tab.read(cx).shot_box(cx));
        assert_eq!(texts.branch, None, "{n}: no branch, no chip");
        feed(cx, tab, |t, cx| t.shot_place(folder("my-app"), None, cx));
        let _ = frame_fresh(cx, window);
        assert_eq!(
            cx.read(|cx| tab.read(cx).shot_part("branch")),
            None,
            "{n}: no chip is drawn"
        );

        feed(cx, tab, |t, cx| {
            t.shot_place(folder("my-app"), Some("master".into()), cx)
        });
        let _ = frame_fresh(cx, window);
        let texts = cx.read(|cx| tab.read(cx).shot_box(cx));
        assert_eq!(texts.branch.as_deref(), Some("master"), "{n}");
        assert_eq!(texts.folder, "my-app", "{n}");
        let (folder_chip, chip) = (part(cx, tab, n, "folder"), part(cx, tab, n, "branch"));
        assert!(
            chip.left() >= folder_chip.right(),
            "{n}: beside the folder chip, after it"
        );
        assert!(
            (chip.center().y - folder_chip.center().y).abs() < px(1.),
            "{n}: on the folder chip's row"
        );
    });
}

/// A click on the chip asks the Workspace to open its branch switcher on the box's folder; the
/// agent's process is not started again.
pub(super) fn agent_box_branch_opens_switcher() {
    use crate::layout_model::TabKind;
    use crate::workspace::Workspace;

    let n = "agent-box-branch-opens-switcher";
    let Some(git) = crate::git_branch::find_git() else {
        println!("ui-shots: {n} skipped (no git on this Mac)");
        return;
    };
    let base =
        std::env::temp_dir().join(format!("fuxx-ui-shots-box-branch-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let repo = base.join("my-app");
    std::fs::create_dir_all(&repo).expect("make the repo");
    let run = |args: &[&str]| super::branch::scratch_git(&git, &repo, args);
    run(&["init", "-q", "-b", "master"]);
    std::fs::write(repo.join("a.txt"), "a\n").unwrap();
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "one"]);

    let theme = shot_theme("Catppuccin Mocha");
    type WsSlot = Rc<RefCell<Option<Entity<Workspace>>>>;
    let slot: WsSlot = Rc::default();
    let keep = slot.clone();
    let at = repo.clone();
    render_with(
        &theme,
        size(px(900.), px(560.)),
        move |window, cx| {
            cx.set_global(shot_appearance(theme, cx));
            cx.bind_keys(crate::key_bindings());
            let ws = cx.new(|cx| {
                let mut ws = Workspace::for_shots(window, cx);
                let project = std::path::Path::new("/Users/me/code/api");
                ws.open_shot_session("api", project, super::SHELL_TEXT, None, window, cx);
                let pane = ws.model.focused_pane().expect("a pane");
                ws.model
                    .add_tab(
                        pane,
                        TabKind::Agent {
                            provider: "claude".into(),
                            cwd: at.clone(),
                            conversation: None,
                            mode: None,
                            title: None,
                            model: None,
                            effort: None,
                        },
                    )
                    .expect("an agent tab");
                ws.sync();
                ws
            });
            *keep.borrow_mut() = Some(ws.clone());
            ws.into()
        },
        move |cx, window| {
            let ws = slot.borrow().clone().expect("the workspace");
            cx.run_until_parked();
            let _ = frame_fresh(cx, window);
            let tab = cx.read(|cx| {
                let ws = ws.read(cx);
                let id = ws.model.focused_tab().expect("a tab");
                ws.agents.get(&id).expect("its view").view.clone()
            });
            feed(cx, &tab, |t, cx| t.shot_history(Vec::new(), cx));
            feed(cx, &tab, |t, cx| {
                t.shot_place(repo.clone(), Some("master".into()), cx)
            });
            let _ = frame_fresh(cx, window);
            let argv_before = argv(cx, &tab);
            assert!(!argv_before.is_empty(), "{n}: it would start the agent");
            assert!(
                cx.read(|cx| ws.read(cx).branch_switcher.is_none()),
                "{n}: closed to begin with"
            );

            let chip = part(cx, &tab, n, "branch");
            cx.simulate_click(window, chip.center(), Modifiers::none());
            cx.run_until_parked();
            let _ = frame_fresh(cx, window);
            let switcher = cx
                .read(|cx| ws.read(cx).branch_switcher.clone())
                .unwrap_or_else(|| panic!("{n}: the chip opens the switcher"));
            assert!(
                super::branch::settle_until(cx, window, 10, |cx| cx
                    .read(|cx| switcher.read(cx).title() == "Switch branch \u{b7} my-app")),
                "{n}: it is on the box's folder: {}",
                cx.read(|cx| switcher.read(cx).title())
            );
            assert_eq!(
                argv(cx, &tab),
                argv_before,
                "{n}: the agent did not restart"
            );
            assert_eq!(tab_cwd(cx, &tab), repo, "{n}: and is where it was");
            slot.borrow_mut().take();
        },
    );
    let _ = std::fs::remove_dir_all(&base);
    println!("ui-shots: {n} ok");
}

// ---- The `/` menu (Task 12) ----

/// The files the `/` list is made of: the box's, the composer it follows, the catalog it lists,
/// the list's name cells and fuzzy match, and the providers that say which commands are the
/// box's own.
const SLASH: &[&str] = &[
    "src/agent_tab.rs",
    "src/agent_box.rs",
    "src/agent_box_menus.rs",
    "src/agent_conversation.rs",
    "src/agent_catalog.rs",
    "src/composer.rs",
    "src/overlay_panel.rs",
    "src/palette.rs",
    "src/palette_match.rs",
    "src/providers/mod.rs",
    "src/providers/claude.rs",
    "src/providers/agy.rs",
];

/// A made-up list of commands, skills and sub-agents, with the model that has efforts.
fn slash_catalog() -> Catalog {
    use crate::providers::{AgentCommand, CommandKind};
    let item = |name: &str, about: &str, kind: CommandKind| AgentCommand {
        name: name.into(),
        about: about.into(),
        kind,
        aliases: Vec::new(),
    };
    let command = |name: &str, about: &str| item(name, about, CommandKind::Command);
    let commands = vec![
        command("help", "Show what can be done here"),
        command("clear", "Start over with an empty conversation"),
        command("compact", "Shrink the conversation to a summary"),
        command("export", "Write the conversation out to a file"),
        command("model", "Choose the model"),
        command("effort", "Choose how hard the model thinks"),
        command("usage", "Show how much of the limits is left"),
        item(
            "example-skill",
            "A made-up skill that tidies the notes",
            CommandKind::Skill,
        ),
        item(
            "example-review",
            "A made-up skill that reviews a change",
            CommandKind::Skill,
        ),
        item(
            "example-agent",
            "A made-up sub-agent that looks things up",
            CommandKind::Agent,
        ),
    ];
    Catalog {
        commands,
        ..example_catalog()
    }
}

/// A scene of the `/` list: Claude on `example-5`, the made-up list read.
fn slash_scene(
    name: &str,
    act: impl Fn(&str, &mut VisualTestAppContext, AnyWindowHandle, &Entity<AgentTab>) + Clone + 'static,
) {
    box_scene(
        name,
        &CLAUDE,
        example_start(),
        Lookup::Ready(slash_catalog()),
        act,
    );
}

/// Type `text` into the box (real input), and draw what it opened.
fn type_in_box(cx: &mut VisualTestAppContext, window: AnyWindowHandle, text: &str) {
    cx.simulate_input(window, text);
    cx.run_until_parked();
    let _ = frame_fresh(cx, window);
}

/// Press `keys` (real keystrokes), and draw what changed.
fn press(cx: &mut VisualTestAppContext, window: AnyWindowHandle, keys: &str) {
    cx.simulate_keystrokes(window, keys);
    cx.run_until_parked();
    let _ = frame_fresh(cx, window);
}

fn box_text(cx: &VisualTestAppContext, tab: &Entity<AgentTab>) -> String {
    cx.read(|cx| tab.read(cx).shot_composer().read(cx).text().to_string())
}

fn slash_rows_of(cx: &VisualTestAppContext, tab: &Entity<AgentTab>) -> Vec<SlashRow> {
    cx.read(|cx| tab.read(cx).shot_slash_rows(cx))
}

fn slash_state(cx: &VisualTestAppContext, tab: &Entity<AgentTab>) -> Option<(String, usize)> {
    cx.read(|cx| tab.read(cx).shot_slash())
}

/// The names of the `/` list's rows as `Heading` or `/name`, in order.
fn slash_names(cx: &VisualTestAppContext, tab: &Entity<AgentTab>) -> Vec<String> {
    let commands = slash_catalog().commands;
    slash_rows_of(cx, tab)
        .iter()
        .map(|row| match row {
            SlashRow::Heading(kind) => format!("{kind:?}s"),
            SlashRow::Item { ix, .. } => format!("/{}", commands[*ix].name),
        })
        .collect()
}

fn nothing_sent(n: &str, cx: &VisualTestAppContext, tab: &Entity<AgentTab>) {
    assert!(
        written(cx, tab).iter().all(|l| l["type"] != "user"),
        "{n}: no message was written: {:?}",
        written(cx, tab)
    );
    assert_eq!(said(cx, tab), Vec::<String>::new(), "{n}: nothing said");
}

/// §9: a `/` at the start of the box opens a list over it, grouped (Commands, Skills, Agents),
/// each row a name and its one line; the first is selected; ↑ ↓ move (and stop at the ends);
/// nothing is sent. While the list loads it says `Loading…`, and `Couldn't read the list` when
/// it could not be read: no list is made up.
pub(super) fn agent_box_slash() {
    box_scene(
        "agent-box-slash",
        &CLAUDE,
        example_start(),
        Lookup::Loading,
        |n, cx, window, tab| {
            type_in_box(cx, window, "/");
            assert_eq!(slash_state(cx, tab), Some((String::new(), 0)), "{n}");
            assert_eq!(slash_rows_of(cx, tab), [], "{n}: no rows while it loads");
            assert_eq!(
                cx.read(|cx| tab.read(cx).shot_slash_note(cx)),
                Some("Loading\u{2026}"),
                "{n}"
            );
            // Return does not send the half-typed `/` while the list loads.
            press(cx, window, "enter");
            assert_eq!(box_text(cx, tab), "/", "{n}: kept");
            nothing_sent(n, cx, tab);
            feed(cx, tab, |t, cx| {
                t.shot_catalog(Lookup::Failed("it ended".into()), cx)
            });
            let _ = frame_fresh(cx, window);
            assert_eq!(
                cx.read(|cx| tab.read(cx).shot_slash_note(cx)),
                Some("Couldn't read the list"),
                "{n}"
            );
            assert_eq!(slash_rows_of(cx, tab), [], "{n}: no made-up list");
            // (Return on a list that couldn't be read sends: agent-box-slash-failed-return-sends.)
            feed(cx, tab, |t, cx| {
                t.shot_catalog(Lookup::Ready(slash_catalog()), cx)
            });
            let _ = frame_fresh(cx, window);
            assert_eq!(
                slash_names(cx, tab),
                [
                    "Commands",
                    "/help",
                    "/clear",
                    "/compact",
                    "/export",
                    "/model",
                    "/effort",
                    "/usage",
                    "Skills",
                    "/example-skill",
                    "/example-review",
                    "Agents",
                    "/example-agent"
                ],
                "{n}"
            );
            assert_eq!(cx.read(|cx| tab.read(cx).shot_slash_note(cx)), None, "{n}");
            // Drawn over the box, inside the window, rows top to bottom, in the list's panel.
            let list = part(cx, tab, n, "slash");
            let folder = part(cx, tab, n, "folder");
            assert!(list.bottom() <= folder.top(), "{n}: above the box {list:?}");
            let row = |cx: &VisualTestAppContext, i: usize| {
                cx.read(|cx| tab.read(cx).shot_slash_row(i))
                    .unwrap_or_else(|| panic!("{n}: row {i} was never drawn"))
            };
            let spots: Vec<_> = (0..13).map(|i| row(cx, i)).collect();
            assert!(
                spots.windows(2).all(|w| w[0].bottom() <= w[1].top()),
                "{n}: top to bottom"
            );
            assert!(inside(spots[0], list), "{n}: in the panel");
            // ↓ and ↑ move the selection and stop at the ends.
            press(cx, window, "down down");
            assert_eq!(slash_state(cx, tab), Some((String::new(), 2)), "{n}");
            press(cx, window, "up");
            assert_eq!(slash_state(cx, tab), Some((String::new(), 1)), "{n}");
            press(cx, window, "up up up");
            assert_eq!(slash_state(cx, tab), Some((String::new(), 0)), "{n}");
            press(
                cx,
                window,
                "down down down down down down down down down down down down",
            );
            assert_eq!(
                slash_state(cx, tab),
                Some((String::new(), 9)),
                "{n}: the last"
            );
            press(cx, window, "up up up up up up up up up");
            assert_eq!(slash_state(cx, tab), Some((String::new(), 0)), "{n}");
            press(cx, window, "down down");
            assert_eq!(box_text(cx, tab), "/", "{n}: the keys moved no caret");
            nothing_sent(n, cx, tab);
        },
    );
}

/// R10: Return blocks only while the `/` list loads; one that couldn't be read is no list to
/// pick from, so Return sends the line as typed (a `/command` Claude may know).
pub(super) fn agent_box_slash_failed_return_sends() {
    box_scene(
        "agent-box-slash-failed-return-sends",
        &CLAUDE,
        example_start(),
        Lookup::Loading,
        |n, cx, window, tab| {
            type_in_box(cx, window, "/compact");
            press(cx, window, "enter");
            assert_eq!(box_text(cx, tab), "/compact", "{n}: kept while it loads");
            nothing_sent(n, cx, tab);
            feed(cx, tab, |t, cx| {
                t.shot_catalog(Lookup::Failed("it ended".into()), cx)
            });
            let _ = frame_fresh(cx, window);
            assert_eq!(
                cx.read(|cx| tab.read(cx).shot_slash_note(cx)),
                Some("Couldn't read the list"),
                "{n}"
            );
            press(cx, window, "enter");
            assert_eq!(said(cx, tab), ["/compact"], "{n}: sent");
            assert_eq!(box_text(cx, tab), "", "{n}: the box is empty");
            assert_eq!(last_user(cx, tab)["message"]["content"], "/compact", "{n}");
        },
    );
}

/// §9: the list filters as you type (fuzzy; a group with no match has no heading; the matched
/// letters are marked); Esc closes it and it stays closed until the `/` word is left.
pub(super) fn agent_box_slash_filtered() {
    slash_scene("agent-box-slash-filtered", |n, cx, window, tab| {
        type_in_box(cx, window, "/ex");
        assert_eq!(
            slash_names(cx, tab),
            [
                "Commands",
                "/export",
                "Skills",
                "/example-skill",
                "/example-review",
                "Agents",
                "/example-agent"
            ],
            "{n}"
        );
        assert_eq!(slash_state(cx, tab), Some(("ex".into(), 0)), "{n}");
        // Narrower: the group with nothing left has no heading.
        type_in_box(cx, window, "a");
        assert_eq!(
            slash_names(cx, tab),
            [
                "Skills",
                "/example-skill",
                "/example-review",
                "Agents",
                "/example-agent"
            ],
            "{n}"
        );
        type_in_box(cx, window, "zz");
        assert_eq!(slash_names(cx, tab), Vec::<String>::new(), "{n}");
        assert_eq!(
            cx.read(|cx| tab.read(cx).shot_slash_note(cx)),
            Some("No command matches"),
            "{n}"
        );
        // Esc closes it (stopping nothing) and keeps it closed while typing on.
        press(cx, window, "escape");
        assert_eq!(slash_state(cx, tab), None, "{n}: Esc closed it");
        assert_eq!(box_text(cx, tab), "/exazz", "{n}: the text stays");
        cx.read(|cx| assert_eq!(tab.read(cx).shot_stop_label(), None, "{n}: nothing stopped"));
        press(cx, window, "backspace");
        assert_eq!(slash_state(cx, tab), None, "{n}: still closed");
        // Out of the word and back: it opens again.
        press(
            cx,
            window,
            "backspace backspace backspace backspace backspace backspace",
        );
        assert_eq!(box_text(cx, tab), "", "{n}");
        type_in_box(cx, window, "/ex");
        assert_eq!(
            slash_state(cx, tab),
            Some(("ex".into(), 0)),
            "{n}: opens again"
        );
        press(cx, window, "down");
        assert_eq!(slash_state(cx, tab), Some(("ex".into(), 1)), "{n}");
        // A `/` that isn't at the start opens nothing.
        feed(cx, tab, |t, cx| {
            t.shot_composer()
                .update(cx, |c, cx| c.set_text("fix /ex".into(), cx))
        });
        let _ = frame_fresh(cx, window);
        assert_eq!(slash_state(cx, tab), None, "{n}: not at the start");
        feed(cx, tab, |t, cx| {
            t.shot_composer()
                .update(cx, |c, cx| c.set_text(String::new(), cx))
        });
        type_in_box(cx, window, "/ex");
        press(cx, window, "down");
        assert_eq!(slash_state(cx, tab), Some(("ex".into(), 1)), "{n}");
        nothing_sent(n, cx, tab);
    });
}

/// §9: Return picks the selected command (`/name ` goes in the box and nothing is sent), and
/// Return again on the complete line sends it as text. Tab completes the selected row as text.
pub(super) fn agent_box_slash_tab_completes() {
    slash_scene("agent-box-slash-tab-completes", |n, cx, window, tab| {
        // Return picks; the next Return sends the line as text, as it always did.
        type_in_box(cx, window, "/clear");
        assert_eq!(slash_names(cx, tab), ["Commands", "/clear"], "{n}");
        press(cx, window, "enter");
        assert_eq!(box_text(cx, tab), "/clear ", "{n}: picked");
        assert_eq!(slash_state(cx, tab), None, "{n}: closed");
        nothing_sent(n, cx, tab);
        press(cx, window, "enter");
        let sent = last_user(cx, tab);
        assert_eq!(
            sent["message"]["content"].as_str().map(str::trim),
            Some("/clear"),
            "{n}: sent as text {sent}"
        );
        assert_eq!(box_text(cx, tab), "", "{n}: the box is clear");
        let sent_lines = written(cx, tab).len();
        // A caret past the first word is out of the list.
        feed(cx, tab, |t, cx| {
            t.shot_composer()
                .update(cx, |c, cx| c.set_text("/co now".into(), cx))
        });
        let _ = frame_fresh(cx, window);
        assert_eq!(
            slash_state(cx, tab),
            None,
            "{n}: the caret is past the word"
        );
        feed(cx, tab, |t, cx| {
            t.shot_composer()
                .update(cx, |c, cx| c.set_text(String::new(), cx))
        });
        // Tab completes the selected row, not the first.
        type_in_box(cx, window, "/c");
        assert_eq!(
            slash_names(cx, tab),
            ["Commands", "/clear", "/compact"],
            "{n}"
        );
        press(cx, window, "down");
        press(cx, window, "tab");
        assert_eq!(box_text(cx, tab), "/compact ", "{n}: completed");
        assert_eq!(slash_state(cx, tab), None, "{n}: closed");
        assert_eq!(written(cx, tab).len(), sent_lines, "{n}: Tab sends nothing");
    });
}

/// §9: `/usage`, `/model` and `/effort` open the box's own menus (View Usage, the model list,
/// the running model's efforts) and clear the box: picked from the list, or typed whole and sent.
/// None writes a message.
pub(super) fn agent_box_slash_usage_opens_menu() {
    slash_scene("agent-box-slash-usage-opens-menu", |n, cx, window, tab| {
        feed(cx, tab, |t, cx| {
            t.shot_usage(claude_usage(), Vec::new(), cx)
        });
        let model = |efforts_of| Some(BoxMenu::Model { efforts_of });
        // Picked from the list.
        type_in_box(cx, window, "/model");
        press(cx, window, "enter");
        assert_eq!(menu(cx, tab), model(None), "{n}: the model menu");
        assert_eq!(box_text(cx, tab), "", "{n}: the box is clear");
        assert_eq!(slash_state(cx, tab), None, "{n}");
        press(cx, window, "escape");
        assert_eq!(menu(cx, tab), None, "{n}");
        type_in_box(cx, window, "/effort");
        press(cx, window, "enter");
        assert_eq!(menu(cx, tab), model(Some(0)), "{n}: the efforts");
        assert!(
            effort_row(cx, tab, "High").is_some(),
            "{n}: its efforts are drawn"
        );
        press(cx, window, "escape");
        // Typed whole and sent (the list is dismissed).
        type_in_box(cx, window, "/effort");
        press(cx, window, "escape");
        press(cx, window, "enter");
        assert_eq!(
            menu(cx, tab),
            model(Some(0)),
            "{n}: sent, it opens the menu"
        );
        assert_eq!(box_text(cx, tab), "", "{n}");
        press(cx, window, "escape");
        // `/usage` with a space (completed by Tab), then Return.
        type_in_box(cx, window, "/us");
        press(cx, window, "tab");
        assert_eq!(box_text(cx, tab), "/usage ", "{n}");
        press(cx, window, "enter");
        assert_eq!(menu(cx, tab), Some(BoxMenu::Usage), "{n}: the usage panel");
        assert_eq!(box_text(cx, tab), "", "{n}");
        nothing_sent(n, cx, tab);
        // A command with something after it is not one of these: it goes as text.
        press(cx, window, "escape");
        type_in_box(cx, window, "/usage now");
        press(cx, window, "enter");
        assert_eq!(menu(cx, tab), None, "{n}: not the menu");
        assert_eq!(
            last_user(cx, tab)["message"]["content"].as_str(),
            Some("/usage now"),
            "{n}"
        );
        // Picked with the caret inside the first word of a longer line: the name goes in as
        // text, the rest stays, and no menu opens.
        press(cx, window, "escape");
        type_in_box(cx, window, "/model x");
        press(cx, window, "left left left left left");
        assert_eq!(
            slash_state(cx, tab).map(|(q, _)| q),
            Some("mo".into()),
            "{n}: the list follows the caret"
        );
        press(cx, window, "enter");
        assert_eq!(menu(cx, tab), None, "{n}: no menu for a longer line");
        assert_eq!(box_text(cx, tab), "/model x", "{n}: the rest stays");
        press(cx, window, "cmd-a backspace");
        // Left for the shot: View Usage, from the list.
        type_in_box(cx, window, "/usage");
        press(cx, window, "enter");
        assert_eq!(menu(cx, tab), Some(BoxMenu::Usage), "{n}");
        assert_eq!(box_text(cx, tab), "", "{n}");
    });
}

/// A made-up list for the `@` menu: the commands, two sub-agents and three skills.
fn at_catalog() -> Catalog {
    use crate::providers::{AgentCommand, CommandKind};
    let item = |name: &str, about: &str, kind: CommandKind| AgentCommand {
        name: name.into(),
        about: about.into(),
        kind,
        aliases: Vec::new(),
    };
    Catalog {
        commands: vec![
            item("clear", "Start over", CommandKind::Command),
            item(
                "example-researcher",
                "A made-up sub-agent that looks things up",
                CommandKind::Agent,
            ),
            item(
                "example-reviewer",
                "A made-up sub-agent that reads a change",
                CommandKind::Agent,
            ),
            item(
                "example-release",
                "A made-up skill that cuts a release",
                CommandKind::Skill,
            ),
            item(
                "example-refactor",
                "A made-up skill that tidies a module",
                CommandKind::Skill,
            ),
            item(
                "example-rebase",
                "A made-up skill that replays commits",
                CommandKind::Skill,
            ),
            item(
                "example-notes",
                "A made-up skill that files the notes",
                CommandKind::Skill,
            ),
        ],
        ..example_catalog()
    }
}

fn at_rows_of(cx: &VisualTestAppContext, tab: &Entity<AgentTab>) -> Vec<AtRow> {
    cx.read(|cx| tab.read(cx).shot_at_rows(cx))
}

/// The `@` list's rows as `Heading`, a file's path or `@name`, in order.
fn at_names(cx: &VisualTestAppContext, tab: &Entity<AgentTab>) -> Vec<String> {
    let commands = at_catalog().commands;
    let hits = cx.read(|cx| tab.read(cx).shot_at_paths());
    at_rows_of(cx, tab)
        .iter()
        .map(|row| match row {
            AtRow::Heading(h) => h.to_string(),
            AtRow::File(i) => hits[*i].clone(),
            AtRow::Named { ix, .. } => format!("@{}", commands[*ix].name),
        })
        .collect()
}

/// Open the `@` list over `paths` for `query`, and wait for the files to be ranked.
fn open_at_list(
    n: &str,
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    tab: &Entity<AgentTab>,
    paths: &[&str],
    query: &str,
) {
    let paths: Vec<String> = paths.iter().map(|p| p.to_string()).collect();
    let query = query.to_string();
    feed(cx, tab, |t, cx| {
        t.shot_at(example_start().cwd, paths, &query, cx)
    });
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while cx.read(|cx| tab.read(cx).at_hits()) == 0 {
        assert!(
            std::time::Instant::now() < until,
            "{n}: the files were never ranked"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
        cx.run_until_parked();
    }
    let _ = frame_fresh(cx, window);
}

/// §10: `@` opens the file picker with sections: Files (as before), Agents (the sub-agents) and
/// Skills; a section with nothing is left out. A file inserts its path; an agent or skill inserts
/// `@name `; nothing is sent. While the agent's list loads (or could not be read) the files work
/// and the agents and skills say so: none is made up.
pub(super) fn agent_box_at_sections() {
    const FILES: [&str; 5] = [
        "README.md",
        "src/rsvp.ts",
        "src/rsvp_dates.ts",
        "test/rsvp.test.ts",
        "package.json",
    ];
    box_scene(
        "agent-box-at-sections",
        &CLAUDE,
        example_start(),
        Lookup::Loading,
        |n, cx, window, tab| {
            // Loading: the files are there, the agents and skills say Loading.
            open_at_list(n, cx, window, tab, &FILES, "rsvp");
            assert_eq!(
                at_names(cx, tab),
                [
                    "Files",
                    "src/rsvp.ts",
                    "src/rsvp_dates.ts",
                    "test/rsvp.test.ts"
                ],
                "{n}: files only, no made-up list"
            );
            assert_eq!(
                cx.read(|cx| tab.read(cx).shot_at_note(cx)),
                Some("Loading\u{2026}"),
                "{n}"
            );
            feed(cx, tab, |t, cx| {
                t.shot_catalog(Lookup::Failed("it ended".into()), cx)
            });
            let _ = frame_fresh(cx, window);
            assert_eq!(
                cx.read(|cx| tab.read(cx).shot_at_note(cx)),
                Some("Couldn't read the list"),
                "{n}"
            );
            assert_eq!(at_names(cx, tab).len(), 4, "{n}: files still work");
            // Ready: the sections, in order.
            feed(cx, tab, |t, cx| {
                t.shot_catalog(Lookup::Ready(at_catalog()), cx)
            });
            open_at_list(n, cx, window, tab, &FILES, "re");
            assert_eq!(cx.read(|cx| tab.read(cx).shot_at_note(cx)), None, "{n}");
            let names = at_names(cx, tab);
            assert_eq!(names.first().map(String::as_str), Some("Files"), "{n}");
            let at_agents = names.iter().position(|r| r == "Agents").expect("Agents");
            let at_skills = names.iter().position(|r| r == "Skills").expect("Skills");
            assert!(at_agents > 1 && at_skills > at_agents, "{n}: {names:?}");
            assert!(
                names[at_agents + 1..at_skills]
                    .iter()
                    .all(|r| r.starts_with("@example-re")),
                "{n}: {names:?}"
            );
            // Drawn: the panel is above the box, its rows top to bottom.
            let list = part(cx, tab, n, "at");
            let folder = part(cx, tab, n, "folder");
            assert!(list.bottom() <= folder.top(), "{n}: above the box {list:?}");
            // Picking a file inserts its path.
            assert_eq!(
                cx.read(|cx| tab.read(cx).shot_at_selected()),
                Some(0),
                "{n}"
            );
            let second = format!("{} ", names[2]);
            press(cx, window, "down enter");
            assert_eq!(box_text(cx, tab), second, "{n}: the second file's path");
            nothing_sent(n, cx, tab);
            // Picking an agent inserts `@name `: down past the files, to the first agent.
            open_at_list(n, cx, window, tab, &FILES, "re");
            let files = names[1..at_agents].len();
            let keys = vec!["down"; files].join(" ");
            press(cx, window, &keys);
            press(cx, window, "enter");
            assert!(
                box_text(cx, tab).starts_with("@example-re") && box_text(cx, tab).ends_with(' '),
                "{n}: {:?}",
                box_text(cx, tab)
            );
            assert_eq!(cx.read(|cx| tab.read(cx).at_hits()), 0, "{n}: closed");
            nothing_sent(n, cx, tab);
            // A skill: ↓ stops at the last row, which is a skill.
            open_at_list(n, cx, window, tab, &FILES, "re");
            press(cx, window, &vec!["down"; names.len()].join(" "));
            press(cx, window, "enter");
            let last = names.last().cloned().unwrap_or_default();
            assert_eq!(box_text(cx, tab), format!("{last} "), "{n}: the last skill");
            nothing_sent(n, cx, tab);
            // A long list scrolls with ↓; a new query, or a list opened again, starts at the top
            // with the first row selected.
            let many: Vec<String> = (0..8).map(|i| format!("src/re{i}.ts")).collect();
            let many: Vec<&str> = many.iter().map(String::as_str).collect();
            open_at_list(n, cx, window, tab, &many, "re");
            let rows = at_rows_of(cx, tab).len();
            press(cx, window, &vec!["down"; rows].join(" "));
            assert!(
                cx.read(|cx| tab.read(cx).shot_list_scrolled()),
                "{n}: the long list scrolled"
            );
            type_in_box(cx, window, "e");
            assert_eq!(box_text(cx, tab), "@ree", "{n}");
            assert_eq!(
                cx.read(|cx| tab.read(cx).shot_at_selected()),
                Some(0),
                "{n}"
            );
            assert!(
                !cx.read(|cx| tab.read(cx).shot_list_scrolled()),
                "{n}: a new query scrolls back to the top"
            );
            // Scrolled again, closed, then `@` typed anew while the files are still being listed:
            // the top, the first row selected, and Return inserts it.
            feed(cx, tab, |t, cx| {
                t.shot_composer()
                    .update(cx, |c, cx| c.set_text("@re".into(), cx))
            });
            let _ = frame_fresh(cx, window);
            press(cx, window, "down down down down down down");
            press(cx, window, "escape");
            feed(cx, tab, |t, cx| {
                t.shot_composer()
                    .update(cx, |c, cx| c.set_text(String::new(), cx))
            });
            type_in_box(cx, window, "@");
            assert_eq!(
                cx.read(|cx| tab.read(cx).shot_at_selected()),
                Some(0),
                "{n}"
            );
            assert!(
                !cx.read(|cx| tab.read(cx).shot_list_scrolled()),
                "{n}: opened at the top"
            );
            press(cx, window, "enter");
            assert!(
                box_text(cx, tab).starts_with("@example-researcher "),
                "{n}: Return inserted the first row: {:?}",
                box_text(cx, tab)
            );
            nothing_sent(n, cx, tab);
            // Left open for the shot.
            open_at_list(n, cx, window, tab, &FILES, "re");
        },
    );
}
