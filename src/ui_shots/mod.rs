//! Off-screen UI screenshots (M11), compiled only with the `ui-shots` cargo feature.
//!
//! `shoot` renders a view in a hidden GPUI window and keeps the shot in memory for the scene's
//! checks; with `--png` (or `UI_SHOTS_PNG=1`) it also saves `target/ui-shots/<name>-<slug>.png`,
//! so a UI task can look at its scenes in Catppuccin Mocha and Catppuccin Latte.
//!
//! AppKit only makes windows on the main thread and `cargo test` runs every test on a spawned one
//! (it aborts with "Rust cannot catch foreign exceptions"), so the scenes are not `#[test]`s.
//! They are plain functions declared in `SCENES` and run from `main` on the main thread:
//! `cargo run --features ui-shots -- __ui-shots [filter] [--png] [--all-themes]`. A scene panics
//! when its checks fail; the run goes on, writes that scene's shots to
//! `target/ui-shots/failed/<scene>/`, and exits 1 at the end.
//!
//! A process has one window-owning thread, so a run is spread over processes:
//! `scripts/ui-shots.sh` builds once, runs `__ui-shots --shard I/N` in N processes side by side
//! (the scenes that aren't `serial`, balanced by `target/ui-shots/timings.tsv`), then
//! `__ui-shots --serial-only` alone, and prints the run's summary.
//!
//! Reference fingerprints (C3, a trial): each `Kind::Visual` scene that passes has its shots'
//! fingerprints (`fingerprint`: greyscale at 1/8 of the point size) compared with
//! `tests/ui-shots-ref/<scene>/<scene>-<slug>.fp`; a shot further than `TOLERANCE` prints
//! "looks different" (or "differs in size") but never fails the run. `--update-refs` writes them
//! instead, in every theme: an unfiltered run replaces each scene's folder and removes the
//! folders no visual scene claims; a filtered or `--changed` run rewrites only its own shots.

use std::cell::Cell;
use std::panic::AssertUnwindSafe;
use std::path::PathBuf;
use std::process::exit;
use std::sync::OnceLock;
use std::time::Instant;

use gpui::VisualTestAppContext;
use gpui::{
    AnyView, AnyWindowHandle, App, BoxShadow, Context, Div, Font, Hsla, IntoElement, Pixels,
    Render, Size, Window, div, point, prelude::*, px, size,
};
use gpui_platform::current_platform;

use crate::appearance::Appearance;
use crate::chrome_cache::{self, Counted};
use crate::fonts;
use crate::procs::ProgramState;
use crate::split_tree::Axis;
use crate::status::Status;
use crate::status_mark::{self, BREATHS_PER_BEAT, TURNS_PER_BEAT};
use crate::terminal_view::hsla;
use crate::theme::{self, Theme, UiColors};
use crate::ui_tokens::{self, contrast};
use crate::workspace::{Overlay, Workspace};

mod agent;
mod agent_box;
mod args;
mod ask;
mod branch;
mod changed;
mod changes;
mod chrome;
mod chrome_parts;
mod file_tab;
mod file_tab_md;
mod file_tab_md_reader;
mod harness;
mod naming;
mod notes;
mod overlays;
mod palette;
mod panes;
mod registry;
mod right_sidebar;
mod sidebar;
mod status_bar;
mod terminal;
mod welcome;
mod window;

pub use args::{RunOptions, parse_args};
pub(super) use harness::*;
use registry::scenes;

/// How many themes a scene is shot in by default (`--all-themes` shoots every scene in both).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Themes {
    /// Catppuccin Mocha only.
    One,
    /// Catppuccin Mocha and Catppuccin Latte.
    Both,
}

/// What a scene's checks read.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Kind {
    /// Pixels.
    Visual,
    /// View state only (bounds, scroll, words, counts).
    State,
}

/// A scene, declared: its name (also its filter and file-name prefix), its function, and what
/// the runner needs to know to schedule it.
#[derive(Clone, Copy)]
pub(super) struct Scene {
    pub(super) name: &'static str,
    pub(super) run: fn(),
    pub(super) themes: Themes,
    /// Must run alone: never in a `--shard`, run by `--serial-only` after the shards. Only for
    /// one of three reasons (ruling 7), named in a comment beside `serial: true`:
    /// (a) it starts a child fuxx (`child_run`);
    /// (b) it changes shared mutable state other scenes read (the Files fixture);
    /// (c) a wall-clock animation or margin: a check's result depends on elapsed real time, so a
    /// slowed process could fail it. Polling a real process until a deadline, or sleeping for
    /// *at least* some time, is not a reason: load only makes those slower.
    pub(super) serial: bool,
    /// `Visual` scenes' shots are fingerprinted against `tests/ui-shots-ref/` (C3).
    pub(super) kind: Kind,
    /// The app files its views come from (and the `assets/` paths, a file or a `dir/`); read by
    /// `--changed`.
    pub(super) sources: &'static [&'static str],
}

/// Files a change to which can alter any scene: the harness, the theme and token tables, the
/// workspace, `main.rs` (the key bindings many scenes press), and the build. `--changed` runs
/// everything for them.
pub(super) const SHARED: &[&str] = &[
    "src/ui_shots/harness.rs",
    "src/ui_shots/mod.rs",
    "src/theme.rs",
    "src/ui_tokens.rs",
    "src/workspace.rs",
    "src/appearance.rs",
    "src/main.rs",
    "Cargo.toml",
    "Cargo.lock",
];

/// What a set of changed files means for the run.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Affected {
    /// Everything: a `shared` file changed, or these `src/` or `assets/` files are in no scene's
    /// sources (`unclaimed`). Both are listed, so neither hides the other.
    All {
        shared: Vec<String>,
        unclaimed: Vec<String>,
    },
    /// Only these scenes (in the scenes' order).
    Some(Vec<&'static str>),
    /// Nothing under `src/`, `assets/` or `Cargo.*` changed (or only runner files did).
    Nothing,
}

/// The scenes the `changed` files (repo-relative) touch: those whose `sources` claim one of
/// them. A `shared` file, or a `src/` or `assets/` file no scene claims, means all of them; a
/// file elsewhere (docs, scripts), or one in `registry::RUNNER_ONLY`, means none. A source
/// ending in `/` claims everything under it. (`registry::claiming` adds each area's own file to
/// its scenes' sources.)
pub(super) fn affected(changed: &[&str], scenes: &[Scene], shared: &[&str]) -> Affected {
    let claims = |source: &str, file: &str| {
        source == file || (source.ends_with('/') && file.starts_with(source))
    };
    let mut hit = vec![false; scenes.len()];
    let mut shared_hit: Vec<String> = Vec::new();
    let mut unclaimed: Vec<String> = Vec::new();
    let mut relevant = false;
    for file in changed {
        if registry::RUNNER_ONLY.contains(file) {
            continue;
        }
        if shared.contains(file) {
            if !shared_hit.iter().any(|u| u == file) {
                shared_hit.push(file.to_string());
            }
            continue;
        }
        if !(file.starts_with("src/") || file.starts_with("assets/") || file.starts_with("Cargo."))
        {
            continue;
        }
        relevant = true;
        let mut claimed = false;
        for (i, scene) in scenes.iter().enumerate() {
            if scene.sources.iter().any(|s| claims(s, file)) {
                hit[i] = true;
                claimed = true;
            }
        }
        if !claimed && !unclaimed.iter().any(|u| u == file) {
            unclaimed.push(file.to_string());
        }
    }
    if !shared_hit.is_empty() || !unclaimed.is_empty() {
        return Affected::All {
            shared: shared_hit,
            unclaimed,
        };
    }
    if !relevant {
        return Affected::Nothing;
    }
    Affected::Some(
        scenes
            .iter()
            .zip(hit)
            .filter(|(_, hit)| *hit)
            .map(|(s, _)| s.name)
            .collect(),
    )
}

/// `names` split into `n` shards of about equal time: longest first, each onto the shard with
/// the least time so far (the first such on a tie), each shard then in `names`' order. A name
/// with no time in `timings` counts the median of those that have one (1 s if none has, which
/// deals the names out round robin).
pub(super) fn shard_of<'a>(
    timings: &[(&str, f64)],
    names: &[&'a str],
    n: usize,
) -> Vec<Vec<&'a str>> {
    let n = n.max(1);
    let time = |name: &str| timings.iter().find(|(t, _)| *t == name).map(|t| t.1);
    let mut known: Vec<f64> = names.iter().filter_map(|&name| time(name)).collect();
    known.sort_by(f64::total_cmp);
    let median = match known.len() {
        0 => 1.0,
        len if len % 2 == 1 => known[len / 2],
        len => (known[len / 2 - 1] + known[len / 2]) / 2.0,
    };
    let mut longest: Vec<(usize, f64)> = names
        .iter()
        .enumerate()
        .map(|(i, &name)| (i, time(name).unwrap_or(median)))
        .collect();
    // Stable: equal times keep `names`' order.
    longest.sort_by(|a, b| b.1.total_cmp(&a.1));
    let mut load = vec![0.0f64; n];
    let mut picked: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, secs) in longest {
        let least = (0..n)
            .min_by(|&a, &b| load[a].total_cmp(&load[b]))
            .expect("n >= 1");
        load[least] += secs;
        picked[least].push(i);
    }
    picked
        .into_iter()
        .map(|mut shard| {
            shard.sort_unstable();
            shard.into_iter().map(|i| names[i]).collect()
        })
        .collect()
}

/// The scenes this run runs, in `ORDER`: those matching the filter; of them, with `--shard I/N`
/// the `I`th share of the scenes that aren't `serial` (`shard_of`, by `timings`), with
/// `--serial-only` the `serial` ones.
fn selected<'s>(all: &[&'s Scene], opts: &RunOptions, timings: &[(&str, f64)]) -> Vec<&'s Scene> {
    let matching = all
        .iter()
        .copied()
        .filter(|s| opts.filter.as_deref().is_none_or(|f| s.name.contains(f)));
    if opts.serial_only {
        return matching.filter(|s| s.serial).collect();
    }
    let Some((i, n)) = opts.shard else {
        return matching.collect();
    };
    let parallel: Vec<&Scene> = matching.filter(|s| !s.serial).collect();
    let names: Vec<&str> = parallel.iter().map(|s| s.name).collect();
    let mine = shard_of(timings, &names, n).swap_remove(i - 1);
    parallel
        .into_iter()
        .filter(|s| mine.contains(&s.name))
        .collect()
}

/// `timings.tsv`'s lines (`<scene>\t<seconds>`, and a shard's third column, `ok` or `FAILED`);
/// a line that doesn't read is skipped.
fn parse_timings(tsv: &str) -> Vec<(String, f64)> {
    tsv.lines()
        .filter_map(|line| {
            let mut cols = line.split('\t');
            let name = cols.next()?;
            let secs = cols.next()?.parse().ok()?;
            Some((name.to_string(), secs))
        })
        .collect()
}

static OPTIONS: OnceLock<RunOptions> = OnceLock::new();

/// The run's options (the defaults until `run` sets them).
pub(super) fn options() -> &'static RunOptions {
    OPTIONS.get_or_init(RunOptions::default)
}

thread_local! {
    /// The running scene's `themes` (set by `run` before each scene).
    static SCENE_THEMES: Cell<Themes> = const { Cell::new(Themes::Both) };
}

/// The themes the running scene is shot in: (file slug, built-in theme name). Mocha only for a
/// `Themes::One` scene, unless `--all-themes`.
pub(super) fn shot_themes() -> &'static [(&'static str, &'static str)] {
    const ALL: &[(&str, &str)] = &SHOT_THEMES;
    match SCENE_THEMES.get() {
        Themes::One if !options().all_themes => &ALL[..1],
        _ => ALL,
    }
}

/// Set in a child run of this program (`child_run`): it leaves the summary and `timings.tsv`
/// to its parent.
const CHILD_ENV: &str = "FUXX_SHOTS_CHILD";

/// This program re-run on the scenes matching `filter`, as a child (`FUXX_SHOTS_CHILD=1`),
/// with this run's `--png` and `--all-themes`. The caller adds its own environment.
pub(super) fn child_run(filter: &str) -> std::process::Command {
    let exe = std::env::current_exe().expect("this program");
    let mut command = std::process::Command::new(exe);
    command.args(["__ui-shots", filter]).env(CHILD_ENV, "1");
    if options().png {
        command.arg("--png");
    }
    if options().all_themes {
        command.arg("--all-themes");
    }
    command
}

/// `fuxx __ui-shots [filter] [--png] [--all-themes] [--shard I/N | --serial-only]` (`args` are
/// those after `__ui-shots`): run the scenes whose name contains `filter` (`selected`), each to
/// the end even if one fails, then exit (1 if any failed). A failed scene's shots are written to
/// `target/ui-shots/failed/<scene>/`. `scripts/ui-shots.sh` runs the shards side by side, then
/// the serial scenes, and prints the run's summary.
pub fn run(args: &[String]) -> ! {
    let mut opts = match parse_args(args) {
        Ok(opts) => opts,
        Err(e) => {
            eprintln!("ui-shots: {e}");
            eprintln!(
                "usage: fuxx __ui-shots [filter] [--png] [--all-themes] [--shard I/N | --serial-only] [--changed <base> [--plan]] [--update-refs]"
            );
            exit(2);
        }
    };
    opts.png |= std::env::var("UI_SHOTS_PNG").is_ok_and(|v| v == "1");
    let filter = opts.filter.clone();
    // A shard's or the serial run's own timings, beside `timings.tsv` (which the script merges
    // them into): `timings.<I>.tsv` or `timings.serial.tsv`.
    let part = match (opts.shard, opts.serial_only) {
        (Some((i, _)), _) => Some(i.to_string()),
        (None, true) => Some("serial".to_string()),
        (None, false) => None,
    };
    let recorded = std::fs::read_to_string(shots_dir().join("timings.tsv")).unwrap_or_default();
    let recorded = parse_timings(&recorded);
    let recorded: Vec<(&str, f64)> = recorded.iter().map(|(n, s)| (n.as_str(), *s)).collect();
    let mut every = scenes();
    if let Some(base) = &opts.changed {
        // Every part works the set out the same way (git is the same for each); one line says
        // what it picked: the direct run's, or `--plan`'s (the script prints that, once).
        // A filter that names no scene at all is a typo, not "the change touches none of them":
        // it fails here, as it does without `--changed`, so the gate can't pass with nothing run.
        if let Some(f) = changed::filter_matches_none(&every, opts.filter.as_deref()) {
            eprintln!("ui-shots: no scene matches {f:?}");
            exit(1);
        }
        let all = registry::claiming();
        let (only, line) = changed::decide(changed::changed_files(base), &all);
        let (only, line) = changed::with_filter(only, line, &all, opts.filter.as_deref());
        let direct = opts.shard.is_none() && !opts.serial_only;
        if direct || opts.plan {
            println!("ui-shots: {line}");
        }
        // Exit 3: nothing to run (no src change, or the filter's scenes aren't touched).
        if opts.plan {
            exit(if only.as_ref().is_some_and(|n| n.is_empty()) {
                3
            } else {
                0
            });
        }
        if let Some(only) = only {
            if only.is_empty() {
                exit(0);
            }
            every.retain(|s| only.contains(&s.name));
        }
    }
    let picked = selected(&every, &opts, &recorded);
    OPTIONS.set(opts).expect("the run's options are set once");
    let started = Instant::now();
    // A child process (cache-off, WebKit) is a filtered run of this binary: it leaves the
    // timings, the summary and the fingerprints to its parent.
    let child = std::env::var(CHILD_ENV).is_ok_and(|v| v == "1");
    // Fingerprints (C3): only reported, never a failure.
    let fingerprints = !child && picked.iter().any(|s| s.kind == Kind::Visual);
    let mut refs = RefCounts::default();
    if fingerprints {
        measure_fingerprint_scale();
    }
    // `--update-refs` over every scene replaces each scene's folder; narrowed by a filter or
    // `--changed`, it rewrites only the shots it took.
    let whole = options().filter.is_none() && options().changed.is_none();
    let ref_mode = match (options().update_refs, whole) {
        (false, _) => Refs::Compare,
        (true, true) => Refs::Replace,
        (true, false) => Refs::Overwrite,
    };
    let mut times: Vec<(&str, f64, bool)> = Vec::new();
    for scene in picked {
        let name = scene.name;
        let (rw, teardown, sleep, child) = (
            prof::get(prof::RENDER_WITH),
            prof::get(prof::TEARDOWN),
            prof::get(prof::SLEEP),
            prof::get(prof::CHILD),
        );
        forget_shots(name);
        // Process-wide test switches back to their defaults, should a failed scene before
        // have left one set (its unwind skipped the reset).
        chrome_cache::shots_off::set(false);
        SCENE_THEMES.set(scene.themes);
        let t = Instant::now();
        // A failed check panics (the default hook prints it); the run goes on to the next scene.
        let passed = std::panic::catch_unwind(AssertUnwindSafe(scene.run)).is_ok();
        let secs = t.elapsed().as_secs_f64();
        if passed {
            println!("ui-shots: {name} ok ({secs:.1} s)");
            if fingerprints && scene.kind == Kind::Visual {
                check_refs(name, ref_mode, &mut refs);
            }
        } else {
            let dir = write_failed(name);
            println!(
                "ui-shots: {name} FAILED ({secs:.1} s) \u{2014} pictures in {}",
                dir.display()
            );
        }
        if prof::enabled() {
            // `other`: outside `render_with` and the apps' teardown (scenes' own work, waits,
            // probes).
            let rw = prof::get(prof::RENDER_WITH) - rw;
            let teardown = prof::get(prof::TEARDOWN) - teardown;
            println!(
                "ui-shots: {name} profile: render_with {rw:.1} s, teardown {teardown:.1} s, other {:.1} s, sleep {:.1} s, child {:.1} s",
                (secs - rw - teardown).max(0.0),
                prof::get(prof::SLEEP) - sleep,
                prof::get(prof::CHILD) - child,
            );
        }
        times.push((name, secs, passed));
    }
    let failed = times.iter().filter(|t| !t.2).count();
    let total = started.elapsed().as_secs_f64();
    if fingerprints {
        let RefCounts {
            same,
            different,
            resized,
            missing,
            written,
        } = refs;
        if options().update_refs {
            println!("ui-shots: fingerprints: {written} written");
        } else {
            let resized = if resized > 0 {
                format!(", {resized} differ in size (another screen scale?)")
            } else {
                String::new()
            };
            println!(
                "ui-shots: fingerprints: {same} same, {different} look different{resized}, {missing} without a reference"
            );
        }
    }
    // An unfiltered `--update-refs`: the folders of scenes that are gone (or no longer visual)
    // go too. Once: in the direct run, or the serial run (the script runs it after the
    // shards), never in a shard or a child.
    if ref_mode == Refs::Replace && options().shard.is_none() && !child {
        let visual: Vec<&str> = scenes()
            .iter()
            .filter(|s| s.kind == Kind::Visual)
            .map(|s| s.name)
            .collect();
        remove_orphans(&visual);
    }
    if let Some(part) = &part {
        // A shard or the serial run: its share may be empty; the script sums the parts.
        let tsv: String = times
            .iter()
            .map(|(n, s, ok)| format!("{n}\t{s:.1}\t{}\n", if *ok { "ok" } else { "FAILED" }))
            .collect();
        let dir = shots_dir();
        std::fs::create_dir_all(&dir).expect("create target/ui-shots");
        std::fs::write(dir.join(format!("timings.{part}.tsv")), tsv)
            .expect("write the part's timings");
        println!(
            "ui-shots: {} passed, {failed} failed, {total:.1} s",
            times.len() - failed
        );
    } else if times.is_empty() {
        eprintln!("ui-shots: no scene matches {filter:?}");
        exit(1);
    } else if !child {
        if filter.is_none() && options().changed.is_none() {
            let tsv: String = times
                .iter()
                .map(|(n, s, _)| format!("{n}\t{s:.1}\n"))
                .collect();
            let dir = shots_dir();
            std::fs::create_dir_all(&dir).expect("create target/ui-shots");
            std::fs::write(dir.join("timings.tsv"), tsv).expect("write timings.tsv");
        }
        println!(
            "ui-shots: {} passed, {failed} failed, {total:.1} s",
            times.len() - failed
        );
        let mut slowest = times.clone();
        slowest.sort_by(|a, b| b.1.total_cmp(&a.1));
        for (name, secs, _) in slowest.iter().take(15) {
            println!("ui-shots:   {secs:6.1} s  {name}");
        }
    }
    if prof::enabled() {
        prof::print_sums();
    }
    exit(if failed > 0 { 1 } else { 0 })
}

/// What a shell shows in the scenes, the same every time.
const SHELL_TEXT: &str = "~/code/api $ cargo test\r\n   Compiling api v0.4.2 (/Users/me/code/api)\r\n    Finished `test` profile [unoptimized + debuginfo] target(s) in 2.31s\r\n     Running unittests src/main.rs\r\n\r\nrunning 42 tests\r\ntest result: ok. 42 passed; 0 failed; 0 ignored\r\n\r\n~/code/api $ ";

const AGENT_TEXT: &str =
    "~/code/api $ claude\r\n\r\n  Reading src/auth.rs\r\n  Editing src/auth.rs (+12 -3)\r\n\r\n> ";

/// What the waiting session shows: a Claude Code style prompt, drawn in a box 40 columns wide so
/// it fits a split pane's grid (about 42 columns, M11i's padding) without wrapping mid-word.
const CLAUDE_PROMPT: &str = "  Editing src/auth.rs (+12 -3)\r\n\r\n╭──────────────────────────────────────╮\r\n│ Edit file                            │\r\n│ src/auth.rs                          │\r\n│                                      │\r\n│ Do you want to edit auth.rs?         │\r\n│ ❯ 1. Yes                             │\r\n│   2. Yes, don't ask again            │\r\n│   3. No, tell Claude what to do      │\r\n╰──────────────────────────────────────╯\r\n";

/// A session in a sidebar scene: name, group folder, status, how long in it, its screen.
struct ShotSession {
    name: &'static str,
    project: &'static str,
    status: Status,
    age: u64,
    text: &'static str,
}

const fn shot(
    name: &'static str,
    project: &'static str,
    status: Status,
    age: u64,
    text: &'static str,
) -> ShotSession {
    ShotSession {
        name,
        project,
        status,
        age,
        text,
    }
}

const API: &str = "/Users/me/code/api";
const WEB: &str = "/Users/me/code/web";

/// How much taller the sidebar's chrome (band, search box with its bottom margin, footer) is
/// since M11al Task 7 than it was (38 + 30 + 0 + 36 before; `TOP_BAND`, `SEARCH` and its bottom
/// margin, `FOOTER` now): a scene whose window was sized for the old room adds this to its
/// height, so the list keeps the rows it showed (a tight window scrolls the list to the active
/// row, which a cached and a full redraw may do differently).
pub(super) const SIDEBAR_CHROME_GROWTH: f32 = (ui_tokens::size::TOP_BAND - 38.0)
    + (crate::sidebar::SEARCH - 30.0)
    + crate::sidebar::SEARCH_BOTTOM
    + (crate::sidebar::FOOTER - 36.0);

/// How much taller `rows` sidebar rows and `headers` group headings are since M11al Task 8
/// (rows 52 → `SIDEBAR_ROW_TWO`; headings 28, and 6 more above all but the first, →
/// `GROUP_HEADER`): a scene whose window was sized for the old rows adds this to its height, so
/// the list keeps the rows it showed.
pub(super) const fn sidebar_list_growth(rows: f32, headers: f32) -> f32 {
    rows * (ui_tokens::size::SIDEBAR_ROW_TWO - 52.0)
        + headers * (ui_tokens::size::GROUP_HEADER - 28.0)
}

/// A workspace showing `sessions` (each over a silent `cat`), statuses set the way the app
/// sets them (so the waiting question is read off its screen). Session `split` is split from
/// the one before it; `active` is focused; the `collapsed` folders are closed.
fn sidebar_workspace(
    sessions: &[ShotSession],
    split: Option<usize>,
    active: usize,
    collapsed: &[&str],
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> Workspace {
    use crate::groups::GroupKey;
    use std::time::{Duration, Instant};

    let mut ws = Workspace::for_shots(window, cx);
    for (i, s) in sessions.iter().enumerate() {
        let axis = (split == Some(i)).then_some(Axis::Row);
        ws.open_shot_session(
            s.name,
            std::path::Path::new(s.project),
            s.text,
            axis,
            window,
            cx,
        );
    }
    for (i, s) in sessions.iter().enumerate() {
        ws.set_status(i, s.status, false, cx);
        let session = ws.sessions.get_mut(i).expect("just opened");
        session.status_since = Instant::now() - Duration::from_secs(s.age);
        session.pop = None; // no pop half-way in a still
    }
    ws.select(active, window, cx);
    ws.collapsed = collapsed
        .iter()
        .map(|p| GroupKey::Folder(p.into()))
        .collect();
    ws
}

/// The appearance for sidebar scenes: the built-in presets, for the + menu.
fn sidebar_appearance(theme: Theme, cx: &App) -> Appearance {
    Appearance {
        presets: crate::presets::builtins(),
        ..shot_appearance(theme, cx)
    }
}

type Slot = std::rc::Rc<std::cell::RefCell<Option<gpui::Entity<Workspace>>>>;

/// The workspace a scene built (`Slot`), to act on and check.
fn slotted(slot: &Slot) -> gpui::Entity<Workspace> {
    slot.borrow().clone().expect("the workspace")
}

/// The canvas round the one card (M11al §3.1): above it (the drag strip), at its right and
/// under it.
const CARD_INSET: f32 = ui_tokens::size::CARD_INSET;

/// The right sidebar's panel's left (just right of its hairline) in a window `window_right` wide
/// with it `width` wide (M11al §3.2): it runs from there to the card's outer right.
fn right_panel_left(window_right: f32, width: f32) -> f32 {
    window_right - CARD_INSET - width
}

/// The right sidebar's panel's right in a window `window_right` wide: inside the card's 1 pt
/// edge, `CARD_INSET` from the window's right (M11al §3.2).
fn right_panel_right(window_right: f32) -> f32 {
    window_right - CARD_INSET - ui_tokens::size::HAIRLINE
}

/// A workspace for the tab bar scenes, focused on api. `four`: two panes, the left one with
/// four tabs (api idle and active, claude waiting, codex working, a browser tab), the right one
/// with tests (done); else `many` (≥ 1) idle terminals in one pane, the last one shown.
fn tab_bar_workspace(
    four: bool,
    many: usize,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> Workspace {
    use crate::layout_model::TabKind;
    use std::time::{Duration, Instant};

    let mut ws = Workspace::for_shots(window, cx);
    ws.sidebar.hint_dismissed = true;
    let project = std::path::Path::new(API);
    if !four {
        let names = [
            "api", "claude", "codex", "tests", "docs", "lint", "server", "agent",
        ];
        ws.open_shot_session(names[0], project, SHELL_TEXT, None, window, cx);
        for name in names.iter().cycle().skip(1).take(many - 1) {
            ws.open_shot_tab(name, project, SHELL_TEXT, window, cx);
        }
        return ws;
    }
    let tabs = [
        shot("api", API, Status::Idle, 0, SHELL_TEXT),
        shot("claude", API, Status::Waiting, 4 * 60 + 12, CLAUDE_PROMPT),
        shot("codex", API, Status::Working, 32, AGENT_TEXT),
    ];
    ws.open_shot_session(tabs[0].name, project, tabs[0].text, None, window, cx);
    for s in &tabs[1..] {
        ws.open_shot_tab(s.name, project, s.text, window, cx);
    }
    let pane = ws.model.focused_pane().expect("a pane");
    ws.model.add_tab(pane, TabKind::Browser { url: None });
    ws.sync();
    ws.open_shot_session("tests", project, SHELL_TEXT, Some(Axis::Row), window, cx);
    let tests = shot("tests", API, Status::Done, 720, "");
    for s in tabs.iter().chain([&tests]) {
        let i = ws
            .sessions
            .position(|x| x.label() == s.name)
            .expect("opened");
        ws.set_status(i, s.status, false, cx);
        let session = ws.sessions.get_mut(i).expect("opened");
        session.status_since = Instant::now() - Duration::from_secs(s.age);
        session.pop = None;
    }
    let api = ws.sessions.position(|x| x.label() == "api").expect("api");
    ws.select(api, window, cx);
    ws
}

/// Where tab `i` of `pane`'s bar is drawn, from the bar's own scroll; none before it's laid out.
fn tab_bounds(
    cx: &mut VisualTestAppContext,
    ws: &gpui::Entity<Workspace>,
    pane: crate::layout_model::PaneId,
    i: usize,
) -> Option<gpui::Bounds<Pixels>> {
    cx.read(|cx| {
        let scrolls = ws.read(cx).tab_scroll.borrow();
        let (scroll, _) = scrolls.get(&pane)?;
        let mut b = scroll.bounds_for_item(i)?;
        b.origin.x += scroll.offset().x;
        Some(b)
    })
}

/// The panes of the focused workspace, in layout order.
fn shown_panes(
    cx: &mut VisualTestAppContext,
    ws: &gpui::Entity<Workspace>,
) -> Vec<crate::layout_model::PaneId> {
    cx.read(|cx| {
        ws.read(cx)
            .shown_layout()
            .map(|t| t.leaves())
            .unwrap_or_default()
    })
}

/// How far left of its pane's right edge the + button's centre is: past the buttons' room, the
/// right sidebar's and ⋯ (each a button and a gap), then half a button (M11al §4.6).
const PLUS_FROM_EDGE: f32 = crate::tab_bar::BUTTONS_PAD
    + 2.0 * (crate::tab_bar::BUTTON + crate::tab_bar::BUTTON_GAP)
    + crate::tab_bar::BUTTON / 2.0;
/// What the browser tab scenes' page shows (no page is made off-screen: the bar is given it).
const PAGE_URL: &str = "http://localhost:3000/dashboard";
const PAGE_TITLE: &str = "Acme dashboard";

/// A workspace for the browser tab scenes: api's terminal on the left; on the right, focused, a
/// browser tab on `PAGE_URL` (`loading`: still loading, so its third button is stop).
fn browser_workspace(loading: bool, window: &mut Window, cx: &mut Context<Workspace>) -> Workspace {
    use crate::browser_tab::BarState;
    use crate::layout_model::TabKind;

    let mut ws = Workspace::for_shots(window, cx);
    ws.sidebar.hint_dismissed = true;
    let project = std::path::Path::new(API);
    ws.open_shot_session("api", project, SHELL_TEXT, None, window, cx);
    let pane = ws.model.focused_pane().expect("a pane");
    let url = Some(PAGE_URL.to_string());
    let opened = ws
        .model
        .split(pane, Axis::Row, TabKind::Browser { url: url.clone() })
        .expect("split");
    ws.sync();
    let view = ws.make_browser_tab(opened.tab, url, window, cx);
    view.update(cx, |view, _| {
        view.show_state(BarState {
            url: PAGE_URL.into(),
            title: PAGE_TITLE.into(),
            loading,
            can_back: true,
            can_forward: false,
        })
    });
    ws.browser_titles.insert(opened.tab, PAGE_TITLE.into());
    ws.activate(window, cx);
    ws
}

/// The ⌘P list's eight sessions (the palette scenes'): api (waiting with a boxed question,
/// working, done, idle), web (waiting with no question found, done) and the SSH host build-box
/// (working, idle).
const SWITCHER_EIGHT: [ShotSession; 8] = [
    shot("claude", API, Status::Waiting, 4 * 60 + 12, CLAUDE_PROMPT),
    shot("codex", API, Status::Working, 32, AGENT_TEXT),
    shot("tests", API, Status::Done, 12 * 60, SHELL_TEXT),
    shot("shell", API, Status::Idle, 0, SHELL_TEXT),
    shot("agent", WEB, Status::Waiting, 95, SHELL_TEXT),
    shot("build", WEB, Status::Done, 40, SHELL_TEXT),
    shot("deploy", OPS, Status::Working, 3 * 60, AGENT_TEXT),
    shot("logs", OPS, Status::Idle, 0, SHELL_TEXT),
];

/// The folder build-box's sessions open in, before they're made SSH sessions to it.
const OPS: &str = "/Users/me/ops";

/// `SWITCHER_EIGHT` with shell (4) active, its last two as SSH sessions to build-box.
fn switcher_workspace(window: &mut Window, cx: &mut Context<Workspace>) -> Workspace {
    let mut ws = sidebar_workspace(&SWITCHER_EIGHT, None, 3, &[], window, cx);
    ws.sidebar.hint_dismissed = true;
    for i in [6, 7] {
        ws.sessions.get_mut(i).expect("opened").ssh = Some(crate::ssh_target::SshTarget {
            host: "build-box".into(),
            tmux: false,
            tmux_name: None,
        });
    }
    ws
}

/// The open palette; none while it's closed.
fn palette_of(
    cx: &mut VisualTestAppContext,
    ws: &gpui::Entity<Workspace>,
) -> Option<gpui::Entity<crate::palette::Palette>> {
    cx.read(|cx| ws.read(cx).palette.clone())
}

/// Open the palette on `scope` as ⌘K / ⌘P do, then let it take the keyboard.
fn open_palette_on(
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    ws: &gpui::Entity<Workspace>,
    scope: crate::palette_items::Scope,
) {
    cx.update_window(window, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.open_palette(scope, window, cx))
    })
    .expect("open the palette");
    settle(cx, window);
}

/// Whether the active terminal's view has the keyboard.
fn active_terminal_focused(
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    ws: &gpui::Entity<Workspace>,
) -> bool {
    use gpui::Focusable;
    cx.update_window(window, |_, window, cx| {
        let ws = ws.read(cx);
        let active = ws.sessions.active().expect("an active terminal");
        active.view.focus_handle(cx).is_focused(window)
    })
    .expect("read the focus")
}

/// `switcher_workspace` with the app's whole key-binding table bound (⌘K, ⌘P, the palette's
/// keys, the box's Enter and Esc).
fn palette_build(
    theme: Theme,
    keep: Slot,
) -> impl FnOnce(&mut Window, &mut App) -> AnyView + Clone {
    move |window: &mut Window, cx: &mut App| -> AnyView {
        cx.set_global(sidebar_appearance(theme, cx));
        cx.bind_keys(crate::key_bindings());
        let ws = cx.new(|cx| switcher_workspace(window, cx));
        *keep.borrow_mut() = Some(ws.clone());
        ws.into()
    }
}

/// The branch switcher's made-up repo (`/Users/me/code/api`) and git (none: a seeded or a
/// faked switcher runs no git).
fn switcher_repo() -> crate::branch_switcher::RepoAt {
    crate::branch_switcher::RepoAt {
        git: PathBuf::from("/nonexistent/git"),
        dir: PathBuf::from(API),
        label: "~/code/api".into(),
    }
}

/// The branch switcher scenes' repo: on main; local main, feature/rows (↑2 ↓1), spike (local
/// only) and feature/old, newest first; origin has main, feature/rows and fix-login; one file is
/// dirty.
fn switcher_snapshot() -> crate::branch_git::Snapshot {
    use crate::branch_list::{Head, LocalRef, RemoteRef, Track};
    let local = |name: &str, upstream: Option<&str>, track: Track, date: i64| LocalRef {
        name: name.into(),
        upstream: upstream.map(String::from),
        track,
        date,
    };
    let remote = |name: &str, date: i64| RemoteRef {
        name: name.into(),
        date,
    };
    crate::branch_git::Snapshot {
        root: PathBuf::from(API),
        head: Head::Branch("main".into()),
        local: vec![
            local("main", Some("origin/main"), Track::Even, 50),
            local(
                "feature/rows",
                Some("origin/feature/rows"),
                Track::Diverged {
                    ahead: 2,
                    behind: 1,
                },
                40,
            ),
            local("spike", None, Track::None, 30),
            local("feature/old", None, Track::None, 20),
        ],
        origin: vec![
            remote("main", 50),
            remote("feature/rows", 40),
            remote("fix-login", 10),
        ],
        pushed: std::collections::HashMap::new(),
        has_origin: true,
        dirty: [PathBuf::from("src/main.rs")].into_iter().collect(),
        orphans: 0,
    }
}

/// v0.3.2, the newer release in the update scenes.
fn shot_release() -> crate::update_release::Release {
    crate::update_release::Release {
        version: crate::update_release::Version(0, 3, 2),
        page_url: "https://github.com/fitzfitz/fuxx-cli/releases/tag/v0.3.2".into(),
        notes: "Faster startup".into(),
        zip: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_reference_folder_belongs_to_a_visual_scene() {
        // A deleted or renamed scene's references would linger unnoticed: `--update-refs`
        // (unfiltered) removes them; this keeps one from being committed.
        let visual: Vec<&str> = scenes()
            .iter()
            .filter(|s| s.kind == Kind::Visual)
            .map(|s| s.name)
            .collect();
        let found = ref_folders();
        assert!(!found.is_empty(), "tests/ui-shots-ref/ has the references");
        assert_eq!(
            orphans(&found, &visual),
            Vec::<String>::new(),
            "folders in tests/ui-shots-ref/ that no visual scene claims"
        );
    }

    /// Each shard's summed seconds, from `timings` (an untimed name counts `untimed`).
    fn loads(timings: &[(&str, f64)], shards: &[Vec<&str>], untimed: f64) -> Vec<f64> {
        shards
            .iter()
            .map(|shard| {
                shard
                    .iter()
                    .map(|n| {
                        timings
                            .iter()
                            .find(|(t, _)| t == n)
                            .map_or(untimed, |t| t.1)
                    })
                    .sum()
            })
            .collect()
    }

    #[test]
    fn shards_balance_longest_first_onto_the_least_loaded() {
        let timings = [("a", 10.0), ("b", 9.0), ("c", 2.0), ("d", 2.0), ("e", 1.0)];
        let names = ["a", "b", "c", "d", "e"];
        let shards = shard_of(&timings, &names, 2);
        assert_eq!(shards.len(), 2);
        let mut got = loads(&timings, &shards, 0.0);
        got.sort_by(f64::total_cmp);
        assert!(
            got == [12.0, 12.0] || got == [11.0, 13.0],
            "loads {got:?} from {shards:?}"
        );
    }

    #[test]
    fn every_name_lands_in_exactly_one_shard_in_its_order() {
        let timings = [("b", 5.0), ("d", 1.0), ("f", 3.0)];
        let names = ["a", "b", "c", "d", "e", "f", "g"];
        for n in 1..=9 {
            let shards = shard_of(&timings, &names, n);
            assert_eq!(shards.len(), n, "{n} shards");
            let mut all: Vec<&str> = shards.concat();
            all.sort_unstable();
            assert_eq!(all, names, "{n} shards: every name once");
            for shard in &shards {
                let at = |s: &&str| names.iter().position(|n| n == s);
                assert!(
                    shard.is_sorted_by_key(at),
                    "{shard:?} keeps the names' order"
                );
            }
        }
    }

    #[test]
    fn an_untimed_name_counts_the_median() {
        // The median of 1, 4 and 10 is 4: x (untimed) gets a shard of its own, as b does, and
        // c joins b (the first least loaded). Counted as 0 s, x would have joined c instead.
        let timings = [("a", 10.0), ("b", 4.0), ("c", 1.0)];
        let names = ["a", "b", "c", "x"];
        let shards = shard_of(&timings, &names, 3);
        assert_eq!(
            loads(&timings, &shards, 4.0),
            [10.0, 5.0, 4.0],
            "{shards:?}"
        );
        assert_eq!(shards, [vec!["a"], vec!["b", "c"], vec!["x"]]);
    }

    #[test]
    fn with_no_timings_the_names_go_round_robin() {
        let names = ["a", "b", "c", "d", "e"];
        let shards = shard_of(&[], &names, 2);
        assert_eq!(shards, [vec!["a", "c", "e"], vec!["b", "d"]]);
    }

    #[test]
    fn one_shard_is_every_name_in_order() {
        let timings = [("c", 9.0), ("a", 1.0)];
        let names = ["a", "b", "c"];
        assert_eq!(shard_of(&timings, &names, 1), [vec!["a", "b", "c"]]);
    }

    #[test]
    fn a_shard_never_has_a_serial_scene_and_serial_only_has_them_all_in_order() {
        let all = scenes();
        let timings: Vec<(&str, f64)> = Vec::new();
        let opts = |shard, serial_only| RunOptions {
            shard,
            serial_only,
            ..RunOptions::default()
        };
        let mut sharded: Vec<&str> = Vec::new();
        for i in 1..=4 {
            let picked = selected(&all, &opts(Some((i, 4)), false), &timings);
            assert!(
                picked.iter().all(|s| !s.serial),
                "shard {i}/4 has a serial scene"
            );
            sharded.extend(picked.iter().map(|s| s.name));
        }
        let serial: Vec<&str> = selected(&all, &opts(None, true), &timings)
            .iter()
            .map(|s| s.name)
            .collect();
        let want: Vec<&str> = all.iter().filter(|s| s.serial).map(|s| s.name).collect();
        assert_eq!(serial, want, "the serial scenes, in ORDER");
        sharded.extend(serial);
        sharded.sort_unstable();
        let mut every: Vec<&str> = registry::ORDER.to_vec();
        every.sort_unstable();
        assert_eq!(
            sharded, every,
            "the shards and the serial run cover every scene once"
        );
    }

    #[test]
    fn timings_tsv_reads_two_or_three_columns_and_skips_bad_lines() {
        let read = parse_timings("a\t1.5\nb\t2.0\tok\nbad line\nc\tx\n\nd\t0.3\tFAILED\n");
        assert_eq!(
            read,
            [("a".to_string(), 1.5), ("b".into(), 2.0), ("d".into(), 0.3)]
        );
    }

    #[test]
    fn every_declared_scene_runs_once_in_order() {
        let declared: usize = registry::AREAS.iter().map(|(_, area)| area.len()).sum();
        let run: Vec<&str> = scenes().iter().map(|s| s.name).collect();
        assert_eq!(run, registry::ORDER);
        assert_eq!(run.len(), declared);
    }

    #[test]
    fn png_has_the_signature_header_and_known_checksums() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
        assert_eq!(adler32(b"Wikipedia"), 0x11e6_0398);
        let shot = Shot {
            width: 2,
            height: 1,
            rgba: vec![1, 2, 3, 255, 4, 5, 6, 255],
        };
        let bytes = png(&shot);
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(&bytes[12..16], b"IHDR");
        assert_eq!(&bytes[16..24], &[0, 0, 0, 2, 0, 0, 0, 1]);
        assert!(bytes.ends_with(&[0, 0, 0, 0, b'I', b'E', b'N', b'D', 0xae, 0x42, 0x60, 0x82]));
    }

    /// A scene named `name` that covers `sources`.
    fn covering(name: &'static str, sources: &'static [&'static str]) -> Scene {
        Scene {
            name,
            run: || {},
            themes: Themes::One,
            serial: false,
            kind: Kind::State,
            sources,
        }
    }

    fn some_scenes() -> [Scene; 3] {
        [
            covering("tab", &["src/file_tab.rs", "src/file_view.rs"]),
            covering("tab-md", &["src/file_tab.rs", "assets/themes/x.json"]),
            covering("side", &["src/sidebar.rs"]),
        ]
    }

    #[test]
    fn a_changed_source_runs_the_scenes_that_claim_it() {
        let got = affected(&["src/file_tab.rs"], &some_scenes(), SHARED);
        assert_eq!(got, Affected::Some(vec!["tab", "tab-md"]));
        let got = affected(&["src/sidebar.rs", "README.md"], &some_scenes(), SHARED);
        assert_eq!(got, Affected::Some(vec!["side"]));
    }

    #[test]
    fn a_shared_file_runs_everything() {
        for &file in SHARED {
            let got = affected(&[file], &some_scenes(), SHARED);
            assert_eq!(
                got,
                Affected::All {
                    shared: vec![file.to_string()],
                    unclaimed: vec![]
                },
                "{file}"
            );
        }
        assert!(SHARED.contains(&"src/theme.rs") && SHARED.contains(&"src/main.rs"));
    }

    #[test]
    fn an_unclaimed_source_runs_everything_and_is_named() {
        let got = affected(&["src/file_tab.rs", "src/foo.rs"], &some_scenes(), SHARED);
        assert_eq!(
            got,
            Affected::All {
                shared: vec![],
                unclaimed: vec!["src/foo.rs".to_string()]
            }
        );
    }

    #[test]
    fn changes_outside_src_assets_and_cargo_run_nothing() {
        let files = ["docs/x.md", "scripts/ui-shots.sh", ".gitignore"];
        assert_eq!(affected(&files, &some_scenes(), SHARED), Affected::Nothing);
        assert_eq!(affected(&[], &some_scenes(), SHARED), Affected::Nothing);
    }

    #[test]
    fn an_asset_runs_the_scenes_that_claim_it_else_everything() {
        let got = affected(&["assets/themes/x.json"], &some_scenes(), SHARED);
        assert_eq!(got, Affected::Some(vec!["tab-md"]));
        let got = affected(&["assets/logo/y.png"], &some_scenes(), SHARED);
        assert_eq!(
            got,
            Affected::All {
                shared: vec![],
                unclaimed: vec!["assets/logo/y.png".to_string()]
            }
        );
    }

    #[test]
    fn every_scene_source_is_a_real_file() {
        for scene in scenes() {
            for source in scene.sources {
                assert!(
                    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join(source)
                        .exists(),
                    "{}: source {source} is not a file",
                    scene.name
                );
            }
        }
    }

    #[test]
    fn a_shared_change_does_not_hide_an_unclaimed_file() {
        let got = affected(&["src/theme.rs", "src/foo.rs"], &some_scenes(), SHARED);
        assert_eq!(
            got,
            Affected::All {
                shared: vec!["src/theme.rs".to_string()],
                unclaimed: vec!["src/foo.rs".to_string()]
            }
        );
    }

    #[test]
    fn runner_files_alone_run_nothing_but_do_not_hide_a_real_change() {
        let files = [
            "src/ui_shots/registry.rs",
            "src/ui_shots/args.rs",
            "src/ui_shots/changed.rs",
        ];
        assert_eq!(affected(&files, &some_scenes(), SHARED), Affected::Nothing);
        let files = ["src/ui_shots/registry.rs", "src/sidebar.rs"];
        let got = affected(&files, &some_scenes(), SHARED);
        assert_eq!(got, Affected::Some(vec!["side"]));
    }

    #[test]
    fn an_area_file_runs_its_own_scenes_and_a_runner_file_none() {
        let all = registry::claiming();
        for (file, area) in registry::AREAS {
            let own: Vec<&str> = area.iter().map(|s| s.name).collect();
            assert!(!own.is_empty(), "{file} has scenes");
            let got = affected(&[*file], &all, SHARED);
            let Affected::Some(got) = got else {
                panic!("{file}: {got:?}");
            };
            for name in &own {
                assert!(got.contains(name), "{file} runs {name}");
            }
            // Any other scene run claims the file in its own `sources`.
            for name in got.iter().filter(|n| !own.contains(n)) {
                let scene = registry::AREAS
                    .iter()
                    .flat_map(|(_, a)| a.iter())
                    .find(|s| s.name == *name)
                    .unwrap();
                assert!(scene.sources.contains(file), "{file} runs {name}");
            }
        }
        let got = affected(&["src/ui_shots/registry.rs"], &all, SHARED);
        assert_eq!(got, Affected::Nothing);
    }

    #[test]
    fn a_scene_file_edit_runs_that_areas_scenes_only() {
        let all = registry::claiming();
        let Affected::Some(got) = affected(&["src/ui_shots/file_tab.rs"], &all, SHARED) else {
            panic!("file_tab.rs should pick scenes");
        };
        let own: Vec<&str> = file_tab::SCENES.iter().map(|s| s.name).collect();
        assert_eq!(got, own);
    }
}
