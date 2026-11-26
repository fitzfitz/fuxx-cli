//! The agent activity chore's scenes: a busy step's file tracks (done, running, a real failure,
//! a planned one, "+N files", "+M earlier", the narrow fallback), small steps as readable rows,
//! and the turn ledger in the box's folder tab (running, done, trimmed) with its popover.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use gpui::{
    AnyWindowHandle, AppContext, Entity, Focusable, Modifiers, VisualTestAppContext, px, size,
};
use serde_json::json;

use super::agent::{Slot, decode, feed, tab_of};
use super::{
    Kind, Scene, Themes, bytes_differ, frame_fresh, keep_shot, shoot_with, shot_appearance,
};
use crate::agent_conversation::Item;
use crate::agent_steps::{Glyph, Tracks};
use crate::agent_tab::{AgentTab, AgentTabEvent};
use crate::providers::claude::CLAUDE;
use crate::providers::{Event, Start};
use crate::status::Status;

pub(super) const FILE: &str = file!();

const SOURCES: &[&str] = &[
    "src/agent_tracks.rs",
    "src/agent_steps.rs",
    "src/agent_step_words.rs",
    "src/agent_activity.rs",
    "src/agent_activity_command.rs",
    "src/agent_activity_checks.rs",
    "src/agent_rows.rs",
    "src/agent_tab.rs",
    "src/agent_chat.rs",
    "src/providers/claude.rs",
    "src/providers/claude/activity.rs",
    "src/providers/fixtures/claude-activity.jsonl",
];

/// The ledger's scenes: the box and its menus too.
const LEDGER: &[&str] = &[
    "src/agent_ledger.rs",
    "src/agent_box.rs",
    "src/agent_box_menus.rs",
    "src/agent_steps.rs",
    "src/agent_step_words.rs",
    "src/agent_activity.rs",
    "src/agent_activity_command.rs",
    "src/agent_activity_checks.rs",
    "src/agent_tab.rs",
    "src/providers/claude.rs",
    "src/providers/claude/activity.rs",
];

const fn scene(name: &'static str, run: fn(), sources: &'static [&'static str]) -> Scene {
    Scene {
        name,
        run,
        themes: Themes::Both,
        serial: false,
        kind: Kind::State,
        sources,
    }
}

pub(super) const SCENES: &[Scene] = &[
    scene("agent-tracks-busy", agent_tracks_busy, SOURCES),
    Scene {
        // Two frames' pixels compared: nothing but the running arc moves (still under reduce
        // motion).
        kind: Kind::Visual,
        ..scene("agent-tracks-running", agent_tracks_running, SOURCES)
    },
    scene("agent-tracks-failure", agent_tracks_failure, SOURCES),
    scene("agent-tracks-planned", agent_tracks_planned, SOURCES),
    scene("agent-tracks-more-files", agent_tracks_more_files, SOURCES),
    scene("agent-tracks-earlier", agent_tracks_earlier, SOURCES),
    scene("agent-tracks-narrow", agent_tracks_narrow, SOURCES),
    Scene {
        // Reads the unknown check's ring's pixels.
        kind: Kind::Visual,
        ..scene("agent-tracks-unknown", agent_tracks_unknown, SOURCES)
    },
    scene("agent-steps-rows", agent_steps_rows, SOURCES),
    scene("agent-ledger-running", agent_ledger_running, LEDGER),
    scene("agent-ledger-done", agent_ledger_done, LEDGER),
    scene("agent-ledger-trimmed", agent_ledger_trimmed, LEDGER),
    scene("agent-ledger-popover", agent_ledger_popover, LEDGER),
    scene("agent-plan-tab", agent_plan_tab, PLAN),
    scene("agent-plan-tab-narrow", agent_plan_tab_narrow, PLAN),
    scene("agent-plan-waiting", agent_plan_waiting, PLAN_CARD),
    scene("agent-plan-reading", agent_plan_reading, PLAN_CARD),
    scene("agent-plan-approved", agent_plan_approved, PLAN_CARD),
];

/// The Plan card's scenes (the landing page's "Plan first, then build"): the card and its rules.
const PLAN_CARD: &[&str] = &[
    "src/agent_plan_card.rs",
    "src/agent_tab.rs",
    "src/agent_rows.rs",
    "src/md_preview.rs",
];

/// The plan tab's scene: the box, its menus and the plan's rules.
const PLAN: &[&str] = &[
    "src/plan_progress.rs",
    "src/agent_box.rs",
    "src/agent_box_menus.rs",
];

const ACTIVITY: &str = include_str!("../providers/fixtures/claude-activity.jsonl");
/// The fixtures' folder: their absolute paths and relative ones name the same files.
const CWD: &str = "/tmp/fx/code/my-app";
pub(super) const DIMS: (f32, f32) = (1100.0, 720.0);

/// The diff tabs (and file tabs: chore 16's shells) a scene's popover asked for.
pub(super) type Told = Rc<RefCell<Vec<PathBuf>>>;

fn start() -> Start {
    Start {
        cwd: PathBuf::from(CWD),
        ..super::agent::start()
    }
}

/// Shoot scene `name` in both themes in a window of `dims`, the tab in the fixtures' folder:
/// `act` sets it up and checks it before the shot.
pub(super) fn shoot(
    name: &str,
    dims: (f32, f32),
    act: impl Fn(&str, &mut VisualTestAppContext, AnyWindowHandle, &Entity<AgentTab>, &Told) + Clone,
) {
    for &(slug, theme_name) in super::shot_themes() {
        let theme = super::shot_theme(theme_name);
        let slot: Slot = Rc::default();
        let keep = slot.clone();
        let told: Told = Rc::default();
        let heard = told.clone();
        let n = format!("{name}-{slug}");
        let act = act.clone();
        let path = shoot_with(
            name,
            slug,
            &theme,
            size(px(dims.0), px(dims.1)),
            move |window, cx| {
                cx.set_global(shot_appearance(theme, cx));
                cx.bind_keys(crate::key_bindings());
                let tab = cx.new(|cx| AgentTab::for_shots(&CLAUDE, start(), window, cx));
                cx.subscribe(&tab, move |_, e: &AgentTabEvent, _| {
                    if let AgentTabEvent::OpenDiff(path) | AgentTabEvent::OpenFile(path) = e {
                        heard.borrow_mut().push(path.clone());
                    }
                })
                .detach();
                tab.read(cx).focus_handle(cx).focus(window, cx);
                *slot.borrow_mut() = Some(tab.clone());
                tab.into()
            },
            move |cx, window| {
                cx.update(|cx| cx.set_reduce_motion(true));
                let tab = tab_of(&keep);
                act(&n, cx, window, &tab, &told);
                keep.borrow_mut().take();
            },
        );
        println!("ui-shots: {}", path.display());
    }
}

/// `events` into the tab as the process would bring them.
pub(super) fn give(cx: &mut VisualTestAppContext, tab: &Entity<AgentTab>, events: Vec<Event>) {
    feed(cx, tab, |t, cx| t.shot_feed(events, cx));
}

fn tracks(cx: &VisualTestAppContext, tab: &Entity<AgentTab>) -> Vec<Rc<Tracks>> {
    cx.read(|cx| tab.read(cx).shot_tracks())
}

/// The chat's status line and its chips (the 2026-11-24 chores: the ledger moved there from the
/// folder tab; chore 22: chips on its right), each as (kind, state, words).
fn status(
    cx: &VisualTestAppContext,
    tab: &Entity<AgentTab>,
) -> (
    String,
    Vec<(
        crate::agent_status_line::ChipKind,
        crate::agent_status_line::ChipState,
        String,
    )>,
) {
    let (line, chips) = cx
        .read(|cx| tab.read(cx).shot_status_line(cx))
        .unwrap_or_else(|| panic!("a status line"));
    let chips = chips
        .into_iter()
        .map(|c| (c.kind, c.state, c.text))
        .collect();
    (line, chips)
}

/// Chore 22: the chips on the status line's row (centres level), pinned to the column's right
/// edge (the box's), the line ending before them.
fn chips_on_the_row(n: &str, cx: &VisualTestAppContext, tab: &Entity<AgentTab>) {
    let line = part(cx, tab, n, "status-line");
    let chips = part(cx, tab, n, "status-chips");
    let outer = part(cx, tab, n, "outer");
    assert!(
        (line.center().y - chips.center().y).abs() < px(1.5),
        "{n}: one row ({line:?} vs {chips:?})"
    );
    assert!(
        line.right() <= chips.left(),
        "{n}: the line ends before the chips"
    );
    assert!(
        (chips.right() - outer.right()).abs() <= px(2.),
        "{n}: pinned to the right edge ({chips:?} vs {outer:?})"
    );
}

pub(super) fn part(
    cx: &VisualTestAppContext,
    tab: &Entity<AgentTab>,
    n: &str,
    name: &str,
) -> gpui::Bounds<gpui::Pixels> {
    cx.read(|cx| tab.read(cx).shot_part(name))
        .unwrap_or_else(|| panic!("{n}: {name} drawn"))
}

pub(super) fn click(
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    at: gpui::Bounds<gpui::Pixels>,
) {
    cx.simulate_click(window, at.center(), Modifiers::none());
    cx.run_until_parked();
}

/// The fixture's events up to (and with) the final activity of call `id`.
fn fixture_until(id: &str) -> Vec<Event> {
    let events = decode(ACTIVITY);
    let finished = events
        .iter()
        .position(|e| matches!(e, Event::ToolFinished { id: i, .. } if i == id))
        .expect("the call's finish");
    events[..=finished + 1].to_vec()
}

/// Claude's stream lines for a made-up turn (the fixture's shapes), decoded by one translator.
struct Turn {
    lines: Vec<String>,
    n: usize,
}

impl Turn {
    fn new() -> Self {
        Turn {
            lines: Vec::new(),
            n: 0,
        }
    }

    fn say(mut self, text: &str) -> Self {
        self.n += 1;
        self.lines.push(
            json!({"type": "assistant", "message": {"id": format!("msg_s_{}", self.n),
                "role": "assistant", "content": [{"type": "text", "text": text}]}})
            .to_string(),
        );
        self
    }

    /// A tool call, and its result unless `result` is None (still running).
    fn call(mut self, name: &str, input: serde_json::Value, result: Option<(&str, bool)>) -> Self {
        self.n += 1;
        let id = format!("toolu_s_{}", self.n);
        self.lines.push(
            json!({"type": "assistant", "message": {"id": format!("msg_s_{}", self.n),
                "role": "assistant",
                "content": [{"type": "tool_use", "id": id, "name": name, "input": input}]}})
            .to_string(),
        );
        if let Some((text, error)) = result {
            self.lines.push(
                json!({"type": "user", "message": {"role": "user", "content": [{
                    "type": "tool_result", "content": text, "is_error": error,
                    "tool_use_id": id}]}})
                .to_string(),
            );
        }
        self
    }

    fn read(self, path: &str) -> Self {
        self.call(
            "Read",
            json!({"file_path": format!("{CWD}/{path}")}),
            Some(("1\tfn main() {}", false)),
        )
    }

    fn edit(self, path: &str, old: &str, new: &str) -> Self {
        self.call(
            "Edit",
            json!({"file_path": format!("{CWD}/{path}"), "old_string": old, "new_string": new}),
            Some(("The file has been updated successfully.", false)),
        )
    }

    fn bash(self, command: &str, result: Option<(&str, bool)>) -> Self {
        self.call("Bash", json!({"command": command}), result)
    }

    fn events(&self) -> Vec<Event> {
        decode(&self.lines.join("\n"))
    }
}

const CARGO_FAILED: &str = include_str!("../fixtures/checks/cargo-test-failed.txt");
const CARGO_E0425: &str = include_str!("../fixtures/checks/cargo-build-e0425.txt");
const PASSED_2: &str = "running 2 tests\n\ntest result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n";

fn failed_run(output: &str) -> String {
    format!("Exit code 101\n{output}")
}

/// The fixture's first step, the turn done: one step model, its counts, a checks track, and
/// its files changed first.
pub(super) fn agent_tracks_busy() {
    shoot("agent-tracks-busy", DIMS, |n, cx, window, tab, _| {
        feed(cx, tab, |t, cx| t.send("Add the CLI side".into(), cx));
        let mut events = fixture_until("toolu_act_7");
        events.push(Event::TurnDone {
            ok: true,
            message: None,
        });
        give(cx, tab, events);
        let _ = frame_fresh(cx, window);
        let all = tracks(cx, tab);
        assert_eq!(all.len(), 1, "{n}: one step model");
        let t = &all[0];
        // The 8th call follows the "red first" narration: a step of its own.
        assert!(t.counts.starts_with("7 actions"), "{n}: {}", t.counts);
        assert_eq!(t.failed, 2, "{n}: the header's failed calls");
        assert!(t.checks.is_some(), "{n}: a checks track");
        let labels: Vec<&str> = t.files.iter().map(|f| f.label.as_str()).collect();
        assert_eq!(
            labels,
            ["api.rs", "api_client.rs", "help_commands.rs", "cli.rs"],
            "{n}"
        );
        let api_client = t.files.iter().find(|f| f.label == "api_client.rs").unwrap();
        assert_eq!(api_client.summary, "read 80–400 · script 1×", "{n}");
        let group = part(cx, tab, n, "tracks-group");
        assert!(
            f32::from(group.size.width) <= crate::ui_tokens::chat::COLUMN_MAX,
            "{n}: {group:?}"
        );
        // ⌥↑ in the box: the step takes the keyboard on its latest call; Esc gives it back.
        cx.simulate_keystrokes(window, "alt-up");
        cx.run_until_parked();
        assert_eq!(
            cx.read(|cx| tab.read(cx).shot_track_state()).2,
            Some(t.columns.len() - 1),
            "{n}: ⌥↑ reaches the tracks"
        );
        cx.simulate_keystrokes(window, "escape");
        cx.run_until_parked();
        assert_eq!(cx.read(|cx| tab.read(cx).shot_track_state()).2, None, "{n}");
        let _ = frame_fresh(cx, window);
    });
}

/// The fixture cut after the 5th call's start: its column has the running arc, and nothing
/// else moves (reduce motion: two frames alike).
pub(super) fn agent_tracks_running() {
    shoot("agent-tracks-running", DIMS, |n, cx, window, tab, _| {
        feed(cx, tab, |t, cx| t.send("Add the CLI side".into(), cx));
        let events = decode(ACTIVITY);
        let started = events
            .iter()
            .position(|e| matches!(e, Event::ToolStarted { id, .. } if id == "toolu_act_5"))
            .unwrap();
        give(cx, tab, events[..=started + 1].to_vec());
        let all = tracks(cx, tab);
        assert_eq!(all.len(), 1, "{n}");
        assert!(all[0].columns[4].running, "{n}: the 5th call runs");
        let checks = all[0].checks.as_ref().expect("a checks track");
        assert_eq!(checks.beads[0].1.glyph, Glyph::Running, "{n}");
        let one = frame_fresh(cx, window);
        let _ = part(cx, tab, n, "tracks-arc");
        let two = frame_fresh(cx, window);
        assert_eq!(bytes_differ(&one, &two), 0, "{n}: nothing moves");
    });
}

/// A step ending in a real compile failure under `cargo test`: a failed bead on the checks
/// track, its first error under the group, and the ledger saying the build fails.
pub(super) fn agent_tracks_failure() {
    shoot("agent-tracks-failure", DIMS, |n, cx, window, tab, _| {
        feed(cx, tab, |t, cx| t.send("Wire the timeout".into(), cx));
        let turn = Turn::new()
            .say("Wire the timeout into the client.")
            .read("src/api_client.rs")
            .edit("src/cli.rs", "run()", "call_within(3)")
            .bash(
                "cargo test --bin fuxx cli::",
                Some((&failed_run(CARGO_E0425), true)),
            );
        give(cx, tab, turn.events());
        let _ = frame_fresh(cx, window);
        let all = tracks(cx, tab);
        assert_eq!(all.len(), 1, "{n}");
        let checks = all[0].checks.as_ref().expect("a checks track");
        assert_eq!(checks.beads[0].1.glyph, Glyph::Failed, "{n}");
        assert!(
            all[0]
                .first_error
                .as_deref()
                .is_some_and(|e| e.starts_with("error[E0425]")),
            "{n}: {:?}",
            all[0].first_error
        );
        let _ = part(cx, tab, n, "tracks-first-error");
        use crate::agent_status_line::{ChipKind, ChipState};
        assert_eq!(
            status(cx, tab).1.last(),
            Some(&(ChipKind::Checks, ChipState::Failed, "build".to_string())),
            "{n}: a compile error under cargo test is the build's"
        );
        // Red with ✕ (chore 22).
        let shot = frame_fresh(cx, window);
        let c = cx.read(|cx| {
            crate::agent_tab::Colors::of(&cx.global::<crate::appearance::Appearance>().theme)
        });
        let chip = part(cx, tab, n, "status-checks");
        let ink = super::ink_in(
            &shot,
            size(px(DIMS.0), px(DIMS.1)),
            chip,
            [c.page.r, c.page.g, c.page.b],
        );
        assert!(
            super::gap(ink, c.ledger_failing) < 40,
            "{n}: the checks chip in red ({ink:?} vs {:?})",
            c.ledger_failing
        );
    });
}

/// "Red first", a failing check then a passing one: a hollow planned bead, "red → green", and
/// no first error.
pub(super) fn agent_tracks_planned() {
    shoot("agent-tracks-planned", DIMS, |n, cx, window, tab, _| {
        feed(cx, tab, |t, cx| t.send("Add the retry test".into(), cx));
        let turn = Turn::new()
            .say("Write the failing test first (red first).")
            .edit(
                "src/cli.rs",
                "mod tests {",
                "mod tests {\n    #[test] fn retries() {}",
            )
            .bash(
                "cargo test --bin fuxx cli::",
                Some((&failed_run(CARGO_FAILED), true)),
            )
            .edit("src/cli.rs", "fn run()", "fn run() -> u8")
            .bash("cargo test --bin fuxx cli::", Some((PASSED_2, false)));
        give(cx, tab, turn.events());
        let _ = frame_fresh(cx, window);
        let all = tracks(cx, tab);
        assert_eq!(all.len(), 1, "{n}");
        let checks = all[0].checks.as_ref().expect("a checks track");
        assert_eq!(checks.beads[0].1.glyph, Glyph::Planned, "{n}");
        assert_eq!(checks.summary, "tests · red → green · 2 passed", "{n}");
        assert_eq!(all[0].first_error, None, "{n}");
    });
}

/// A step reading 8 files: 5 tracks and "+3 files" (shot as `…-collapsed`); a click shows all
/// 8, "fewer" shows 5 again, and another click all 8 for the shot.
pub(super) fn agent_tracks_more_files() {
    shoot("agent-tracks-more-files", DIMS, |n, cx, window, tab, _| {
        feed(cx, tab, |t, cx| t.send("Survey the source".into(), cx));
        let mut turn = Turn::new().say("Look around first.");
        for name in [
            "api.rs",
            "cli.rs",
            "link_route.rs",
            "help_commands.rs",
            "main.rs",
            "lib.rs",
            "config.rs",
            "api_client.rs",
        ] {
            turn = turn.read(&format!("src/{name}"));
        }
        give(cx, tab, turn.events());
        let _ = frame_fresh(cx, window);
        let all = tracks(cx, tab);
        assert_eq!(all[0].shown_files(false), (&all[0].files[..5], 3), "{n}");
        keep_shot(&format!("{n}-collapsed"), &frame_fresh(cx, window));
        let files_all = |cx: &VisualTestAppContext| cx.read(|cx| tab.read(cx).shot_track_state()).1;
        click(cx, window, part(cx, tab, n, "tracks-more-files"));
        assert_eq!(files_all(cx), 1, "{n}: the step shows every file");
        let all = tracks(cx, tab);
        assert_eq!(all[0].shown_files(true).0.len(), 8, "{n}");
        let _ = frame_fresh(cx, window);
        click(cx, window, part(cx, tab, n, "tracks-more-files"));
        assert_eq!(files_all(cx), 0, "{n}: \"fewer\" shows the top files again");
        let _ = frame_fresh(cx, window);
        click(cx, window, part(cx, tab, n, "tracks-more-files"));
        assert_eq!(files_all(cx), 1, "{n}");
        let _ = frame_fresh(cx, window);
    });
}

/// A step of 30 calls: "+6 earlier" in its header (shot as `…-collapsed`); a click shows all 30
/// (sideways) and the header says "fewer", which shows the latest again; ←/→ move the cursor.
pub(super) fn agent_tracks_earlier() {
    shoot("agent-tracks-earlier", DIMS, |n, cx, window, tab, _| {
        feed(cx, tab, |t, cx| t.send("Read it all".into(), cx));
        let mut turn = Turn::new().say("Reading the modules.");
        for i in 0..30 {
            turn = turn.read(["src/api.rs", "src/cli.rs", "src/main.rs"][i % 3]);
        }
        give(cx, tab, turn.events());
        let _ = frame_fresh(cx, window);
        let all = tracks(cx, tab);
        assert_eq!((all[0].columns.len(), all[0].earlier), (30, 6), "{n}");
        let group = part(cx, tab, n, "tracks-group");
        let toggle = part(cx, tab, n, "tracks-earlier");
        assert!(
            toggle.top() < group.top() + px(crate::ui_tokens::chat::TRACK_HEAD),
            "{n}: \"+6 earlier\" sits in the header"
        );
        keep_shot(&format!("{n}-collapsed"), &frame_fresh(cx, window));
        let cols_all = |cx: &VisualTestAppContext| cx.read(|cx| tab.read(cx).shot_track_state()).0;
        click(cx, window, toggle);
        assert_eq!(cols_all(cx), 1, "{n}: every column");
        assert_eq!(all[0].shown_columns(true), 0..30, "{n}");
        let _ = frame_fresh(cx, window);
        click(cx, window, part(cx, tab, n, "tracks-earlier"));
        assert_eq!(cols_all(cx), 0, "{n}: \"fewer\" shows the latest again");
        let _ = frame_fresh(cx, window);
        click(cx, window, part(cx, tab, n, "tracks-earlier"));
        assert_eq!(cols_all(cx), 1, "{n}");
        let _ = frame_fresh(cx, window);
        // The header takes the keyboard: the cursor on the latest call, ← one back.
        let group = part(cx, tab, n, "tracks-group");
        let head = gpui::point(group.origin.x + px(40.), group.origin.y + px(10.));
        cx.simulate_click(window, head, Modifiers::none());
        cx.run_until_parked();
        let _ = frame_fresh(cx, window);
        assert_eq!(
            cx.read(|cx| tab.read(cx).shot_track_state()).2,
            Some(29),
            "{n}"
        );
        cx.simulate_keystrokes(window, "left");
        cx.run_until_parked();
        assert_eq!(
            cx.read(|cx| tab.read(cx).shot_track_state()).2,
            Some(28),
            "{n}"
        );
        cx.simulate_keystrokes(window, "escape");
        cx.run_until_parked();
        assert_eq!(cx.read(|cx| tab.read(cx).shot_track_state()).2, None, "{n}");
        let _ = frame_fresh(cx, window);
        // The 2026-11-25 chores ("the 'active' border is not removed when I clicked
        // anywhere"): the cursor back, then a press above the tracks lets it go.
        cx.simulate_click(window, head, Modifiers::none());
        cx.run_until_parked();
        let _ = frame_fresh(cx, window);
        assert_eq!(
            cx.read(|cx| tab.read(cx).shot_track_state()).2,
            Some(29),
            "{n}"
        );
        let above = gpui::point(group.origin.x + px(40.), group.origin.y - px(30.));
        cx.simulate_click(window, above, Modifiers::none());
        cx.run_until_parked();
        let _ = frame_fresh(cx, window);
        assert_eq!(
            cx.read(|cx| tab.read(cx).shot_track_state()).2,
            None,
            "{n}: a press elsewhere clears the cursor"
        );
    });
}

/// A check whose outcome can't be read (`cargo test | head -5`, its summary cut off): its bead
/// is a drawn hollow ring in `panel_dim` (ink round its cell, empty in the middle), never green;
/// the checks track says it "ran".
pub(super) fn agent_tracks_unknown() {
    shoot("agent-tracks-unknown", DIMS, |n, cx, window, tab, _| {
        feed(cx, tab, |t, cx| t.send("Run the tests".into(), cx));
        let turn = Turn::new()
            .say("Check the client builds and run the tests.")
            .read("src/api_client.rs")
            .edit("src/api_client.rs", "fn get()", "fn get() -> u8")
            .bash(
                "cargo test 2>&1 | head -5",
                Some(("   Compiling my-app v0.1.0 (/tmp/fx/code/my-app)\n", false)),
            );
        give(cx, tab, turn.events());
        let shot = frame_fresh(cx, window);
        let all = tracks(cx, tab);
        assert_eq!(all.len(), 1, "{n}");
        let checks = all[0].checks.as_ref().expect("a checks track");
        assert_eq!(
            checks.beads[0].1.glyph,
            Glyph::Check,
            "{n}: unknown, never green"
        );
        assert_eq!(checks.summary, "tests · ran", "{n}");
        let ring = part(cx, tab, n, "tracks-unknown");
        let c = cx.read(|cx| {
            crate::agent_tab::Colors::of(&cx.global::<crate::appearance::Appearance>().theme)
        });
        let rgb = |c: alacritty_terminal::vte::ansi::Rgb| [c.r, c.g, c.b];
        let dims = size(px(DIMS.0), px(DIMS.1));
        let area = (
            f32::from(ring.left()),
            f32::from(ring.top()),
            f32::from(ring.size.width),
            f32::from(ring.size.height),
        );
        let (x0, x1, y0, y1) = super::ink_box(&shot, dims, area, rgb(c.panel), rgb(c.panel_dim))
            .unwrap_or_else(|| panic!("{n}: the ring draws ink in its cell {ring:?}"));
        assert!(
            x1 - x0 >= 6.0 && y1 - y0 >= 6.0,
            "{n}: a ring, not a dot ({x0}..{x1} × {y0}..{y1})"
        );
        let centre = super::pixel_at(&shot, dims, (x0 + x1) / 2.0, (y0 + y1) / 2.0);
        assert!(
            super::gap(centre, c.panel) < super::gap(centre, c.panel_dim),
            "{n}: hollow ({centre:?})"
        );
    });
}

/// The busy step in a narrow tab: no tracks, readable rows.
pub(super) fn agent_tracks_narrow() {
    shoot(
        "agent-tracks-narrow",
        (520.0, 640.0),
        |n, cx, window, tab, _| {
            feed(cx, tab, |t, cx| t.send("Add the CLI side".into(), cx));
            give(cx, tab, fixture_until("toolu_act_7"));
            let _ = frame_fresh(cx, window);
            let _ = frame_fresh(cx, window);
            assert!(tracks(cx, tab).is_empty(), "{n}: rows, not tracks");
            let tools = cx.read(|cx| {
                tab.read(cx)
                    .shot_items()
                    .iter()
                    .filter(|i| matches!(i, Item::Tool { .. }))
                    .count()
            });
            assert_eq!(tools, 7, "{n}");
            let _ = part(cx, tab, n, "tool-title");
        },
    );
}

/// A 1-call step and a 2-call step: readable rows, the agent's words first, the dim trace and
/// the "+41 −0" chip.
pub(super) fn agent_steps_rows() {
    shoot("agent-steps-rows", DIMS, |n, cx, window, tab, _| {
        feed(cx, tab, |t, cx| {
            t.send("Run the tests, then add the route".into(), cx)
        });
        let turn = Turn::new()
            .say("Run the tests first.")
            .call(
                "Bash",
                json!({"command": "cargo test --bin fuxx cli::",
                    "description": "Run the CLI tests"}),
                Some((PASSED_2, false)),
            )
            .say("Now the link route.")
            .call(
                "Write",
                json!({"file_path": format!("{CWD}/src/link_route.rs"),
                    "content": "fn route() {}\n".repeat(41)}),
                Some(("File created successfully.", false)),
            )
            .read("src/api.rs");
        give(cx, tab, turn.events());
        let _ = frame_fresh(cx, window);
        assert!(tracks(cx, tab).is_empty(), "{n}: small steps are rows");
        let words: Vec<crate::agent_step_words::RowWords> = cx.read(|cx| {
            tab.read(cx)
                .shot_items()
                .iter()
                .filter(|i| matches!(i, Item::Tool { .. }))
                .map(crate::agent_step_words::row_words)
                .collect()
        });
        assert_eq!(words[0].title, "Run the CLI tests", "{n}");
        assert_eq!(
            words[0].trace.as_deref(),
            Some("cargo test --bin fuxx cli::"),
            "{n}"
        );
        assert_eq!(words[0].chips, ["2 passed"], "{n}");
        assert_eq!(words[1].title, "Edited link_route.rs", "{n}");
        assert_eq!(words[1].chips, ["+41 −0"], "{n}");
        let _ = part(cx, tab, n, "tool-title");
    });
}

/// A turn with 4 files changed, two scopes that failed twice then passed (every failure cleared
/// since), and a test run going.
fn ledger_turn(running: bool) -> Turn {
    let mut turn = Turn::new().say("Fix the client, then run the checks.");
    turn = turn
        .edit(
            "src/api.rs",
            "fn get() {}",
            "fn get() {}\nfn list() {}\nfn put() {}",
        )
        .edit("src/cli.rs", "a", "b")
        .edit("src/api_client.rs", "a", "b")
        .edit("src/link_route.rs", "a", "b");
    for scope in ["cli::", "api::"] {
        let command = format!("cargo test --bin fuxx {scope}");
        turn = turn
            .bash(&command, Some((&failed_run(CARGO_FAILED), true)))
            .bash(&command, Some((&failed_run(CARGO_FAILED), true)))
            .bash(&command, Some((PASSED_2, false)));
    }
    let result = (!running).then_some((PASSED_2, false));
    turn.bash("cargo test --bin fuxx", result)
}

/// Plan progress (the 2026-11-24 chores): a turn whose agent keeps a six-item list, four done,
/// shows the burger tab `☰ 4/6` at the folder-tab row's right; a click (and ⌥⌘P) opens its
/// tasks, ✓ done, ◐ being built, ○ next.
pub(super) fn agent_plan_tab() {
    plan_scene("agent-plan-tab", DIMS, false);
}

/// A narrow pane with a long folder and branch: the folder tab gives way (its pills cut with
/// "…") and never runs under the plan tab (the review's finding 12).
pub(super) fn agent_plan_tab_narrow() {
    plan_scene("agent-plan-tab-narrow", (460.0, 640.0), true);
}

fn plan_scene(name: &'static str, dims: (f32, f32), long: bool) {
    use crate::providers::{TodoItem, TodoState};
    shoot(name, dims, move |n, cx, window, tab, _| {
        if long {
            feed(cx, tab, |t, cx| {
                t.shot_place(
                    PathBuf::from("/tmp/fx/code/a-very-long-project-folder-name"),
                    Some("feature/a-really-long-branch-name-for-the-plan".into()),
                    cx,
                )
            });
        }
        feed(cx, tab, |t, cx| t.send("Build the plan".into(), cx));
        let item = |text: &str, state| TodoItem {
            text: text.into(),
            state,
        };
        give(
            cx,
            tab,
            vec![Event::Todo(vec![
                item("Task 1: the activity model", TodoState::Done),
                item("Task 2: Claude and agy fill it", TodoState::Done),
                item("Task 3: the step model", TodoState::Done),
                item("Task 4: file tracks", TodoState::Done),
                item("Task 5: the turn ledger", TodoState::Active),
                item("Task 6: scenes and docs", TodoState::Pending),
            ])],
        );
        let _ = frame_fresh(cx, window);
        cx.run_until_parked();
        let _ = frame_fresh(cx, window);
        let burger = part(cx, tab, n, "plan-tab");
        let folder = part(cx, tab, n, "folder");
        assert!(
            burger.left() > folder.right(),
            "{n}: right of the folder tab"
        );
        if long {
            let branch = part(cx, tab, n, "branch");
            assert!(
                branch.right() < burger.left(),
                "{n}: the branch chip ends before the plan tab ({branch:?} vs {burger:?})"
            );
        }
        assert!(
            (burger.center().y - folder.center().y).abs() < px(3.),
            "{n}: on the folder tab's row"
        );
        // Flush with the box's right edge, as the folder tab is with its left (the 2026-11-25
        // chores: "the edge corner, should we make it like this").
        let shell = part(cx, tab, n, "corner-tabs");
        let outer = part(cx, tab, n, "outer");
        assert!(
            (shell.right() - outer.right()).abs() < px(1.5),
            "{n}: the plan tab ends at the box's right edge ({shell:?} vs {outer:?})"
        );
        corner_step_meets_the_line(n, cx, window, tab, dims);
        click(cx, window, burger);
        let _ = frame_fresh(cx, window);
        assert_eq!(
            cx.read(|cx| tab.read(cx).shot_menu()),
            Some(crate::agent_box::BoxMenu::Plan),
            "{n}: the click opens the plan"
        );
        let plan = cx.read(|cx| tab.read(cx).plan.clone());
        assert_eq!(crate::plan_progress::counts(&plan), (4, 6), "{n}");
        // Over the chat, its right edge on the tab's (the box's), not hanging past it (the
        // 2026-11-25 chores: "should be left aligned").
        let _ = frame_fresh(cx, window);
        let popover = part(cx, tab, n, "plan-popover");
        assert!(
            popover.right() <= outer.right() + px(1.) && popover.right() > outer.right() - px(24.),
            "{n}: the popover ends at the box's right edge ({popover:?} vs {outer:?})"
        );
    });
}

/// Chore 18 ("the line is offside"): the corner tabs' step, as the folder tab's, carries its
/// tab's edge down into the curve alone. Over the 2 pt above the box's top line, the tab's left
/// edge column is the box's fill (no straight line running on beside the curve), and the box's
/// top line goes on under the step's left half. The probes sit inside the 1 pt edges, so the
/// tab's left edge is the column just left of `corner-tabs`, the box's top line the row just
/// above `outer`.
fn corner_step_meets_the_line(
    n: &str,
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    tab: &Entity<AgentTab>,
    dims: (f32, f32),
) {
    let shot = frame_fresh(cx, window);
    let c = cx.read(|cx| {
        crate::agent_tab::Colors::of(&cx.global::<crate::appearance::Appearance>().theme)
    });
    let size = size(px(dims.0), px(dims.1));
    let shell = part(cx, tab, n, "corner-tabs");
    let outer = part(cx, tab, n, "outer");
    let (edge_x, line_y) = (f32::from(shell.left()) - 1.0, f32::from(outer.top()) - 1.0);
    let scale = shot.width as f32 / dims.0;
    let device = 1.0 / scale;
    let mut y = line_y - 2.0;
    while y < line_y {
        let mut x = edge_x;
        while x < edge_x + 1.0 {
            let p = super::pixel_at(&shot, size, x, y);
            assert!(
                super::near(p, c.outer),
                "{n}: the box's fill at ({x}, {y}) beside the step's curve, not its tab's edge ({p:?})"
            );
            x += device;
        }
        y += device;
    }
    let mut x = edge_x - crate::terminal_view::TAB_STEP;
    while x < edge_x - crate::terminal_view::TAB_STEP / 2.0 {
        let p = super::pixel_at(&shot, size, x, line_y);
        assert!(
            super::near(p, c.outer_edge),
            "{n}: the box's top line at ({x}, {line_y}) under the step ({p:?})"
        );
        x += device;
    }
}

/// The running turn, on the chat's status line: "Cultivating… · Running tests …", and on its
/// right the chips `⧉ 4 changed` and the checks' arc (a check runs: chore 22); the folder tab
/// drops its mark (chore 17).
pub(super) fn agent_ledger_running() {
    shoot("agent-ledger-running", DIMS, |n, cx, window, tab, _| {
        feed(cx, tab, |t, cx| t.send("Fix the client".into(), cx));
        give(cx, tab, ledger_turn(true).events());
        let _ = frame_fresh(cx, window);
        let (line, chips) = status(cx, tab);
        assert!(line.contains(" · Running tests"), "{n}: {line:?}");
        use crate::agent_status_line::{ChipKind, ChipState};
        assert_eq!(
            chips,
            [
                (ChipKind::Changed, ChipState::Quiet, "4 changed".to_string()),
                (ChipKind::Checks, ChipState::Running, "checks".to_string()),
            ],
            "{n}"
        );
        chips_on_the_row(n, cx, tab);
        // One working mark (chore 17, option B): the status line's; the folder tab drops its
        // own while the line shows.
        cx.read(|cx| tab.read(cx).shot_forget(&["status", "status-line"]));
        let _ = frame_fresh(cx, window);
        let _ = part(cx, tab, n, "status-line");
        assert!(
            !cx.read(|cx| tab.read(cx).shot_part_now("status")),
            "{n}: no status mark in the folder tab while the status line shows"
        );
    });
}

/// The same turn finished: Done, then "4 changed" and the checks; no running part.
pub(super) fn agent_ledger_done() {
    shoot("agent-ledger-done", DIMS, |n, cx, window, tab, _| {
        feed(cx, tab, |t, cx| t.send("Fix the client".into(), cx));
        let mut events = ledger_turn(false).events();
        events.push(Event::TurnDone {
            ok: true,
            message: None,
        });
        give(cx, tab, events);
        let _ = frame_fresh(cx, window);
        assert_eq!(cx.read(|cx| tab.read(cx).status()), Status::Done, "{n}");
        let (line, chips) = status(cx, tab);
        assert!(line.starts_with("Done"), "{n}: {line:?}");
        use crate::agent_status_line::{ChipKind, ChipState};
        assert_eq!(
            chips,
            [
                (ChipKind::Changed, ChipState::Quiet, "4 changed".to_string()),
                (ChipKind::Checks, ChipState::Passed, "checks".to_string()),
            ],
            "{n}: each scope's latest passed"
        );
        chips_on_the_row(n, cx, tab);
        // Your next message: its turn has no chips yet, so the open popover goes with them
        // (the review of chores 17–22: it stayed "open" unseen, holding the box's keys).
        click(cx, window, part(cx, tab, n, "status-changed"));
        assert!(cx.read(|cx| tab.read(cx).shot_menu()).is_some(), "{n}");
        feed(cx, tab, |t, cx| t.send("And the docs".into(), cx));
        let _ = frame_fresh(cx, window);
        assert!(
            cx.read(|cx| tab.read(cx).shot_menu()).is_none(),
            "{n}: no popover left open with nothing to show"
        );
    });
}

/// A narrow tab: the status line is cut with … before the chips, which stay on its row, inside
/// the window.
pub(super) fn agent_ledger_trimmed() {
    shoot(
        "agent-ledger-trimmed",
        (480.0, 640.0),
        |n, cx, window, tab, _| {
            feed(cx, tab, |t, cx| t.send("Fix the client".into(), cx));
            give(cx, tab, ledger_turn(true).events());
            let _ = frame_fresh(cx, window);
            let _ = frame_fresh(cx, window);
            let (line, _) = status(cx, tab);
            assert!(line.contains("Running tests"), "{n}: {line:?}");
            // The line gives way (its action cut with "…"), the chips keep their place (chore
            // 22), and its time stays whole after the action (the review's finding 3).
            chips_on_the_row(n, cx, tab);
            let tail = part(cx, tab, n, "status-tail");
            let chips = part(cx, tab, n, "status-chips");
            assert!(
                tail.size.width > px(20.) && tail.right() <= chips.left(),
                "{n}: the time whole, before the chips ({tail:?} vs {chips:?})"
            );
            let chips = part(cx, tab, n, "status-chips");
            assert!(
                chips.right() <= px(480.),
                "{n}: inside the window {chips:?}"
            );
        },
    );
}

/// A click on a chip on the status line's right (chore 22: each opens its part): the checks
/// chip lists the checks; the changed chip lists the changed files (`src/api.rs` with its +/−) and the
/// checks one row per scope ("3/7 passed"); a click on a file asks for its diff tab and closes
/// it; Esc closes it too.
pub(super) fn agent_ledger_popover() {
    shoot("agent-ledger-popover", DIMS, |n, cx, window, tab, told| {
        feed(cx, tab, |t, cx| t.send("Fix the client".into(), cx));
        let mut events = ledger_turn(false).events();
        events.push(Event::TurnDone {
            ok: true,
            message: None,
        });
        give(cx, tab, events);
        let _ = frame_fresh(cx, window);
        let l = cx
            .read(|cx| tab.read(cx).shot_ledger())
            .unwrap_or_else(|| panic!("{n}: a ledger"));
        assert_eq!((l.passed, l.total, l.scopes.len()), (3, 7, 3), "{n}");
        // Each chip opens its part (chore 22): the checks chip the checks, not the files.
        let scope = l.scopes[0].scope.clone();
        let listed = |cx: &VisualTestAppContext, label: &str| {
            cx.read(|cx| tab.read(cx).shot_menu_row(label)).is_some()
        };
        click(cx, window, part(cx, tab, n, "status-checks"));
        let _ = frame_fresh(cx, window);
        assert!(
            listed(cx, &scope) && !listed(cx, "src/api.rs"),
            "{n}: the checks chip lists the checks"
        );
        // The changed chip, with the checks' popover open: the files, not the checks.
        click(cx, window, part(cx, tab, n, "status-changed"));
        let _ = frame_fresh(cx, window);
        assert!(
            cx.read(|cx| tab.read(cx).shot_menu()).is_some(),
            "{n}: the other chip opens its part from that press"
        );
        assert!(
            listed(cx, "src/api.rs") && !listed(cx, &scope),
            "{n}: the changed chip lists the files"
        );
        let row = cx
            .read(|cx| tab.read(cx).shot_menu_row("src/api.rs"))
            .unwrap_or_else(|| panic!("{n}: src/api.rs listed"));
        click(cx, window, row);
        assert_eq!(
            told.borrow().as_slice(),
            [PathBuf::from(CWD).join("src/api.rs")],
            "{n}: its diff tab asked for"
        );
        assert!(
            cx.read(|cx| tab.read(cx).shot_menu()).is_none(),
            "{n}: the popover closed"
        );
        click(cx, window, part(cx, tab, n, "status-changed"));
        assert!(cx.read(|cx| tab.read(cx).shot_menu()).is_some(), "{n}");
        // The chip again closes it (the review's finding 5: the press outside closed it and
        // the click opened it again).
        click(cx, window, part(cx, tab, n, "status-changed"));
        assert!(
            cx.read(|cx| tab.read(cx).shot_menu()).is_none(),
            "{n}: a second click closes it"
        );
        click(cx, window, part(cx, tab, n, "status-changed"));
        cx.simulate_keystrokes(window, "escape");
        cx.run_until_parked();
        assert!(
            cx.read(|cx| tab.read(cx).shot_menu()).is_none(),
            "{n}: Esc closes it"
        );
        click(cx, window, part(cx, tab, n, "status-changed"));
        let _ = frame_fresh(cx, window);
    });
}

/// A realistic plan (the landing page's Plan card): its context, four steps and how it's
/// checked; `long` adds a code sketch, more steps and its risks (past the fold, 4 parts).
fn rich_plan(long: bool) -> String {
    let mut md = String::from(
        "# Plan: Retry against the current endpoint\n\n## Context\n`RetryState` keeps the \
         `base_url` of the first attempt, so after a failover every retry still hits the dead \
         host.\n\n## Steps\n1. In `retry.rs`, read the endpoint from `Client::current()` on \
         every attempt.\n2. Drop `base_url` from `RetryState` and its two call sites.\n3. Add a \
         test: a failover between attempts sends attempt 2 to the new host.\n4. Log the endpoint \
         each attempt used, at debug level.\n",
    );
    if long {
        md.push_str(
            "5. Cap the whole retry at the request's deadline.\n6. Keep the jitter in \
             `backoff.rs`, unchanged.\n\n```rust\nfn attempt(&self, n: u32) -> Result<Response> {\n    \
             let endpoint = self.client.current(); // was: self.state.base_url\n    \
             self.send(endpoint, n)\n}\n```\n\n## Risks\n- A failover mid-stream still ends the \
             stream; that stays out of scope.\n",
        );
    }
    md.push_str("\n## Verification\n- `cargo test retry`\n- Run the failover example against two local servers.\n");
    md
}

/// You ask for a plan; Claude, in plan mode, read and searched, then asks you to approve it.
fn to_plan(cx: &mut VisualTestAppContext, tab: &Entity<AgentTab>, long: bool) {
    feed(cx, tab, |t, cx| {
        t.send(
            "Retries keep hitting the old host after a failover. Plan the fix first.".into(),
            cx,
        )
    });
    let mut events = vec![Event::Mode("plan".into())];
    events.extend(
        Turn::new()
            .read("src/retry.rs")
            .read("src/client.rs")
            .say("The retry loop captures the endpoint once, before the first attempt. Here's how I'd fix it.")
            .events(),
    );
    let md = rich_plan(long);
    events.push(Event::Permission {
        id: "plan-1".into(),
        tool: "ExitPlanMode".into(),
        what: String::new(),
        detail: String::new(),
        always: None,
        by: None,
        plan: Some(crate::providers::PlanDraft {
            title: crate::providers::plan_title(&md),
            markdown: md,
            file: Some(PathBuf::from(format!(
                "{CWD}/.claude/plans/retry-current-endpoint.md"
            ))),
        }),
    });
    give(cx, tab, events);
}

/// The plan waiting for you: rendered, its three answers, the box asking what to change.
pub(super) fn agent_plan_waiting() {
    shoot("agent-plan-waiting", DIMS, |n, cx, window, tab, _| {
        to_plan(cx, tab, false);
        let _ = frame_fresh(cx, window);
        cx.read(|cx| {
            let t = tab.read(cx);
            assert_eq!(
                t.question().as_deref(),
                Some("Claude's plan is ready"),
                "{n}"
            );
        });
    });
}

/// A long plan opened whole: its outline chips, and the answers pinned over the box.
pub(super) fn agent_plan_reading() {
    shoot("agent-plan-reading", DIMS, |n, cx, window, tab, _| {
        to_plan(cx, tab, true);
        super::chrome::settle_frames(cx, window);
        feed(cx, tab, |t, cx| t.shot_open_plan(cx));
        super::chrome::settle_frames(cx, window);
        assert!(
            cx.read(|cx| tab.read(cx).shot_plan_bar()),
            "{n}: the answer bar pinned over the box"
        );
    });
}

/// Approved with edits accepted: the card folds to one line, the mode follows Claude to Accept
/// edits, and the first edits run.
pub(super) fn agent_plan_approved() {
    shoot("agent-plan-approved", DIMS, |n, cx, window, tab, _| {
        to_plan(cx, tab, false);
        feed(cx, tab, |t, cx| {
            t.answer(crate::agent_conversation::Answer::AlwaysAllowed, cx)
                .ok();
        });
        let mut events = vec![Event::Mode("acceptEdits".into())];
        // Its calls numbered after the planning turn's, so no id is used twice.
        let after_plan = Turn {
            lines: Vec::new(),
            n: 10,
        };
        events.extend(
            after_plan
                .say("Approved. Starting with `retry.rs`.")
                .edit(
                    "src/retry.rs",
                    "self.state.base_url",
                    "self.client.current()",
                )
                .edit("src/client.rs", "pub base_url: Url,", "")
                .call(
                    "Bash",
                    json!({"command": "cargo test retry", "description": "Run the retry tests"}),
                    None,
                )
                .events(),
        );
        give(cx, tab, events);
        let _ = frame_fresh(cx, window);
        cx.read(|cx| {
            let t = tab.read(cx);
            assert!(
                t.shot_items().iter().any(|i| matches!(
                    i,
                    crate::agent_conversation::Item::Permission {
                        answer: Some(crate::agent_conversation::Answer::AlwaysAllowed),
                        plan: Some(_),
                        ..
                    }
                )),
                "{n}: approved"
            );
        });
    });
}
