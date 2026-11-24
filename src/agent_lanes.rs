//! Subagent lanes for the agent tab's UI (M11ar): which view is shown, the pills' labels, and
//! stepping between views. Pure and GPUI-free; the lanes themselves are the conversation's.

use crate::agent_conversation::{Lane, LaneStatus};

/// What the agent tab's chat shows: the main agent's conversation, or one subagent's.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum View {
    #[default]
    Main,
    Lane(String),
}

/// A pill's label: `label`, cut to `max` characters with an ellipsis.
pub fn pill_label(label: &str, max: usize) -> String {
    if label.chars().count() <= max {
        return label.to_string();
    }
    let kept: String = label.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", kept.trim_end())
}

/// Every lane's pill label, in order; a label already used gets " 2", " 3" ….
pub fn pill_labels(lanes: &[Lane], max: usize) -> Vec<String> {
    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    lanes
        .iter()
        .map(|l| {
            let n = seen.entry(l.label.clone()).or_insert(0);
            *n += 1;
            if *n == 1 {
                pill_label(&l.label, max)
            } else {
                // The number stays when the label is cut (it tells them apart).
                let suffix = format!(" {n}");
                let room = max.saturating_sub(suffix.chars().count()).max(2);
                format!("{}{suffix}", pill_label(&l.label, room))
            }
        })
        .collect()
}

/// The view `dir` pills from `view` (Main first, then the lanes newest first: `shown_order`;
/// no wrap). A lane no longer there goes back to Main.
pub fn step(view: &View, lanes: &[Lane], dir: i32) -> View {
    let order = shown_order(lanes);
    let at = match view {
        View::Main => 0,
        View::Lane(id) => match order.iter().position(|&i| &lanes[i].id == id) {
            Some(p) => p + 1,
            None => return View::Main,
        },
    };
    let to = (at as i64 + dir as i64).clamp(0, order.len() as i64) as usize;
    match to {
        0 => View::Main,
        p => View::Lane(lanes[order[p - 1]].id.clone()),
    }
}

/// The gap between pills, and the "+N" pill's width.
pub const PILL_GAP: f32 = 8.0;
pub const MORE_W: f32 = 26.0;

/// Which pills (by index, in order) fit in `avail` with these `widths`, and how many are left
/// behind "+N". Main (index 0) and the `active` pill always show when anything is hidden.
#[cfg(test)]
pub fn fit(widths: &[f32], avail: f32, active: usize) -> (Vec<usize>, usize) {
    fit_with(widths, avail, active, MORE_W)
}

/// `fit`, with room for a "+N" pill `more` wide (the lanes chore: it carries a live mark while
/// a lane runs).
pub fn fit_with(widths: &[f32], avail: f32, active: usize, more: f32) -> (Vec<usize>, usize) {
    let n = widths.len();
    let total: f32 = widths.iter().sum::<f32>() + PILL_GAP * n.saturating_sub(1) as f32;
    if total <= avail {
        return ((0..n).collect(), 0);
    }
    let room = |shown: &[usize], plus: Option<usize>| {
        let pills: f32 = shown.iter().chain(plus.iter()).map(|&i| widths[i]).sum();
        let count = shown.len() + plus.iter().count();
        pills + PILL_GAP * count as f32 + more <= avail
    };
    let mut shown: Vec<usize> = Vec::new();
    for i in 0..n {
        if !room(&shown, Some(i)) {
            break;
        }
        shown.push(i);
    }
    if active < n && !shown.contains(&active) {
        while shown.len() > 1 && !room(&shown, Some(active)) {
            shown.pop();
        }
        shown.push(active);
    }
    shown.sort_unstable();
    let hidden = n - shown.len();
    (shown, hidden)
}

/// How long a lane has run: to its end, else to `now`; none without a start (a resumed lane).
pub fn elapsed(
    started: Option<std::time::Instant>,
    ended: Option<std::time::Instant>,
    now: std::time::Instant,
) -> Option<std::time::Duration> {
    let start = started?;
    Some(ended.unwrap_or(now).saturating_duration_since(start))
}

/// Each lane no longer going gets its end time (once), so its clock stops — also one stopped by
/// its process ending, which sends no `SubagentEnded`.
pub fn stamp_ends(
    times: &mut std::collections::HashMap<String, (std::time::Instant, Option<std::time::Instant>)>,
    lanes: &[Lane],
    now: std::time::Instant,
) {
    for lane in lanes.iter().filter(|l| !l.status.live()) {
        if let Some((_, end)) = times.get_mut(&lane.id) {
            end.get_or_insert(now);
        }
    }
}

/// What the tab shows (its mark, the sidebar, the status pill): Main's status — but Working
/// while a subagent still runs after Main finished (your words, 2026-11-22: "it says done …
/// but in reality there are subagents running under the hood").
pub fn shown_status(main: crate::status::Status, lanes: &[Lane]) -> crate::status::Status {
    use crate::status::Status;
    match main {
        Status::Done | Status::Idle if lanes.iter().any(|l| l.status.live()) => Status::Working,
        other => other,
    }
}

/// The status pill's word while only subagents run (your words, 2026-11-22: "if subagents are
/// working, change cultivating to divining … if main is working while subagents working as
/// well, cultivating wins"; renamed 2026-11-24: "'Divining' label status indicator, change it to
/// 'Orchestrating'"): "Orchestrating…"; none when none does.
pub fn running_word(lanes: &[Lane]) -> Option<String> {
    lanes
        .iter()
        .any(|l| l.status.live())
        .then(|| "Orchestrating…".to_string())
}

/// The lanes' places in the strip, after Main: newest first (your words, 2026-11-22: "make
/// sure it sorts by newest"). Indexes into `lanes` (start order).
pub fn shown_order(lanes: &[Lane]) -> Vec<usize> {
    (0..lanes.len()).rev().collect()
}

/// How often a lane's followed file is read: often while it works, rarely once it ended (a
/// woken subagent still shows up), so a long session's finished lanes cost next to nothing.
/// The main conversation's own record (`main`: agy's questions are only there) is read as
/// often as a working lane's.
pub fn follow_every(lane: Option<&Lane>, main: bool) -> std::time::Duration {
    match lane {
        _ if main => std::time::Duration::from_millis(500),
        Some(l) if l.status.live() => std::time::Duration::from_millis(500),
        _ => std::time::Duration::from_secs(5),
    }
}

/// What the box shows in a lane's view (the lanes chore, 2026-11-24).
#[derive(Clone, Debug, PartialEq)]
pub enum LaneBox {
    /// The box talks to Main (a running lane of an agent whose Main can't pass a message on:
    /// agy).
    ToMain,
    /// Main passes your message on to the lane's subagent; with `images` (which can't be
    /// passed on) its send is off.
    Relay { images: bool },
    /// The closing bar, in place of the box's contents: the lane can't be messaged.
    Bar(Bar),
}

/// Why a lane's box is the closing bar.
#[derive(Clone, Debug, PartialEq)]
pub enum Bar {
    /// It runs inside Main's step: Main can't pass a note on until it ends.
    Foreground,
    /// It ended so (Done, Failed, Stopped, Ended).
    Finished(LaneStatus),
    /// Main's process isn't ready to take a message.
    NotReady,
}

/// The box for `lane`'s view: `relays` (the provider passes messages on), Main's process
/// `ready`, and images waiting in the box. One pure rule, so you can never message a finished
/// subagent.
pub fn lane_box(lane: &Lane, relays: bool, ready: bool, images: bool) -> LaneBox {
    // First, for every provider (agy too): a finished lane can't be messaged.
    if !lane.status.live() {
        return LaneBox::Bar(Bar::Finished(lane.status.clone()));
    }
    if !relays {
        return LaneBox::ToMain;
    }
    if !lane.controls.message {
        return LaneBox::Bar(Bar::Foreground);
    }
    if !ready {
        return LaneBox::Bar(Bar::NotReady);
    }
    LaneBox::Relay { images }
}

/// A lane header's kind and model, "Plan · Haiku"; the kind goes when it is the label (any
/// case), so a lane named for its kind doesn't read "Explore Explore".
pub fn header_kind(lane: &Lane) -> String {
    let kind = Some(lane.kind.clone()).filter(|k| !k.eq_ignore_ascii_case(&lane.label));
    [kind, lane.model.clone()]
        .into_iter()
        .flatten()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" \u{b7} ")
}

/// The closing bar's row 1 words for lane `name` (`time`: its run's clock): the strong words,
/// the quiet rest, and a tooltip on them (a failure's full reason; why a foreground lane can't
/// be reached).
pub fn bar_words(
    why: &Bar,
    name: &str,
    time: Option<&str>,
) -> (String, Option<String>, Option<String>) {
    match why {
        Bar::Finished(LaneStatus::Done) => (
            format!("{name} finished"),
            time.map(|t| format!(" in {t}")),
            None,
        ),
        Bar::Finished(LaneStatus::Failed(reason)) => {
            let reason = reason.trim();
            (
                format!("{name} failed"),
                (!reason.is_empty()).then(|| format!(" \u{b7} {reason}")),
                (!reason.is_empty()).then(|| reason.to_string()),
            )
        }
        Bar::Finished(LaneStatus::Stopped) => (
            format!("{name} was stopped"),
            time.map(|t| format!(" after {t}")),
            None,
        ),
        Bar::Finished(_) => (
            format!("{name} ended"),
            Some(" without a report".to_string()),
            None,
        ),
        Bar::Foreground => (
            format!("{name} is working"),
            Some(" \u{b7} Main is waiting on it, so messages can't reach it yet".to_string()),
            Some(format!(
                "Main waits for {name} to finish, so it can't pass a message on until then. \
                 Ask Main now and it reads it when {name} is done."
            )),
        ),
        Bar::NotReady => (
            format!("{name} is working"),
            Some(" \u{b7} you can message it once Main has started".to_string()),
            None,
        ),
    }
}

/// A lane pill's tooltip (the lanes chore): its state and time, its task's first line, whether
/// you can message it (an agent whose Main `relays`), and "resumed" once it ran again.
pub fn pill_tip(lane: &Lane, time: Option<std::time::Duration>, relays: bool) -> String {
    let t = time.map(clock);
    let state = match (&lane.status, t) {
        (LaneStatus::Working | LaneStatus::Waiting, Some(t)) => format!("Running {t}"),
        (LaneStatus::Working | LaneStatus::Waiting, None) => "Running".into(),
        (LaneStatus::Done, Some(t)) => format!("Done in {t}"),
        (LaneStatus::Done, None) => "Done".into(),
        (LaneStatus::Failed(_), _) => "Failed".into(),
        (LaneStatus::Stopped, Some(t)) => format!("Stopped after {t}"),
        (LaneStatus::Stopped, None) => "Stopped".into(),
        (LaneStatus::Ended, _) => "Ended without a report".into(),
    };
    let task = lane.task.lines().next().map(str::trim).unwrap_or("");
    let message = (relays && lane.status.live()).then_some(if lane.controls.message {
        "you can message it"
    } else {
        "Main is waiting on it"
    });
    [
        Some(state.as_str()),
        Some(task),
        message,
        (lane.runs > 1).then_some("resumed"),
    ]
    .into_iter()
    .flatten()
    .filter(|w| !w.is_empty())
    .collect::<Vec<_>>()
    .join(" \u{b7} ")
}

/// The "+N" pill's tooltip: "18 more · 3 running".
pub fn more_tip(hidden: usize, running: usize) -> String {
    match running {
        0 => format!("{hidden} more"),
        n => format!("{hidden} more \u{b7} {n} running"),
    }
}

/// The "+N" list's rows (`hidden`: lane indexes), as two groups — those `running` when it
/// opened, then the finished — each newest first.
pub fn list_groups(
    lanes: &[Lane],
    hidden: &[usize],
    running: &std::collections::HashSet<String>,
) -> (Vec<usize>, Vec<usize>) {
    let mut newest: Vec<usize> = hidden.to_vec();
    newest.sort_unstable_by(|a, b| b.cmp(a));
    newest
        .into_iter()
        .partition(|&i| running.contains(&lanes[i].id))
}

/// Whether a lane's header offers Stop: it is still going and its agent can stop it alone.
pub fn shows_stop(lane: &Lane) -> bool {
    lane.controls.stop && lane.status.live()
}

/// "1 step", "14 steps".
pub fn steps_label(n: usize) -> String {
    match n {
        0 => "No steps".into(),
        1 => "1 step".into(),
        n => format!("{n} steps"),
    }
}

/// `1:02`.
pub fn clock(d: std::time::Duration) -> String {
    let s = d.as_secs();
    format!("{}:{:02}", s / 60, s % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_conversation::Lane;
    use crate::providers::LaneControls;
    use std::time::{Duration, Instant};

    fn lane(id: &str, label: &str) -> Lane {
        Lane {
            id: id.into(),
            parent: None,
            label: label.into(),
            kind: "Explore".into(),
            task: String::new(),
            model: None,
            status: LaneStatus::Working,
            tokens: None,
            tools: None,
            last_tool: None,
            controls: LaneControls::default(),
            runs: 1,
        }
    }

    #[test]
    fn a_pill_s_tooltip_says_its_state_its_task_and_a_resume() {
        let mut l = lane("a", "Explore");
        l.task = "Read the test setup\nand report".into();
        l.controls.message = true;
        let t = Some(Duration::from_secs(62));
        assert_eq!(
            pill_tip(&l, t, true),
            "Running 1:02 \u{b7} Read the test setup \u{b7} you can message it"
        );
        l.controls.message = false;
        assert_eq!(
            pill_tip(&l, t, true),
            "Running 1:02 \u{b7} Read the test setup \u{b7} Main is waiting on it"
        );
        assert_eq!(
            pill_tip(&l, t, false),
            "Running 1:02 \u{b7} Read the test setup",
            "agy: no message words"
        );
        l.status = LaneStatus::Done;
        let done = Some(Duration::from_secs(93));
        assert_eq!(
            pill_tip(&l, done, true),
            "Done in 1:33 \u{b7} Read the test setup"
        );
        l.runs = 2;
        assert_eq!(
            pill_tip(&l, done, true),
            "Done in 1:33 \u{b7} Read the test setup \u{b7} resumed"
        );
        l.runs = 1;
        l.status = LaneStatus::Failed("no such file".into());
        assert_eq!(
            pill_tip(&l, done, true),
            "Failed \u{b7} Read the test setup"
        );
        l.status = LaneStatus::Stopped;
        assert_eq!(
            pill_tip(&l, Some(Duration::from_secs(48)), true),
            "Stopped after 0:48 \u{b7} Read the test setup"
        );
        l.status = LaneStatus::Ended;
        assert_eq!(
            pill_tip(&l, None, true),
            "Ended without a report \u{b7} Read the test setup"
        );
        assert_eq!(more_tip(18, 3), "18 more \u{b7} 3 running");
        assert_eq!(more_tip(2, 0), "2 more");
    }

    #[test]
    fn the_more_list_groups_running_then_finished_newest_first() {
        let lanes = [
            lane("a", "A"),
            lane("b", "B"),
            lane("c", "C"),
            lane("d", "D"),
        ];
        let running: std::collections::HashSet<String> = ["a".to_string(), "c".to_string()].into();
        let (live, done) = list_groups(&lanes, &[0, 1, 2, 3], &running);
        assert_eq!(live, [2, 0], "running, newest first");
        assert_eq!(done, [3, 1], "finished, newest first");
        // All running: one group (no headers).
        let all: std::collections::HashSet<String> = lanes.iter().map(|l| l.id.clone()).collect();
        let (live, done) = list_groups(&lanes, &[1, 3], &all);
        assert_eq!((live, done), (vec![3, 1], vec![]));
    }

    #[test]
    fn lane_box_states() {
        let mut bg = lane("a", "Explore");
        bg.controls = LaneControls {
            stop: true,
            message: true,
            woken_by_steps: false,
        };
        // agy (no relay): a running lane's box talks to Main.
        assert_eq!(lane_box(&bg, false, true, false), LaneBox::ToMain);
        // Claude, live, in the background, ready: a relay.
        assert_eq!(
            lane_box(&bg, true, true, false),
            LaneBox::Relay { images: false }
        );
        let mut waiting = bg.clone();
        waiting.status = LaneStatus::Waiting;
        assert_eq!(
            lane_box(&waiting, true, true, false),
            LaneBox::Relay { images: false }
        );
        // Images can't be passed on: the relay box, its send off.
        assert_eq!(
            lane_box(&bg, true, true, true),
            LaneBox::Relay { images: true }
        );
        // Not ready.
        assert_eq!(
            lane_box(&bg, true, false, false),
            LaneBox::Bar(Bar::NotReady)
        );
        // Live in the foreground: inside Main's step.
        let mut fg = bg.clone();
        fg.controls.message = false;
        assert_eq!(
            lane_box(&fg, true, true, false),
            LaneBox::Bar(Bar::Foreground)
        );
        // Finished, however: the closing bar.
        for status in [
            LaneStatus::Done,
            LaneStatus::Failed("no such file".into()),
            LaneStatus::Stopped,
            LaneStatus::Ended,
        ] {
            let mut done = bg.clone();
            done.status = status.clone();
            assert_eq!(
                lane_box(&done, true, true, false),
                LaneBox::Bar(Bar::Finished(status.clone()))
            );
            assert_eq!(
                lane_box(&done, true, false, true),
                LaneBox::Bar(Bar::Finished(status))
            );
        }
    }

    /// The lanes review, fix 6: no "Explore Explore" — the header's kind goes when it is the
    /// label (any case); the model stays.
    #[test]
    fn the_header_drops_a_kind_that_is_the_label() {
        let mut l = lane("a", "Explore");
        l.kind = "explore".into();
        assert_eq!(header_kind(&l), "");
        l.model = Some("Haiku".into());
        assert_eq!(header_kind(&l), "Haiku");
        l.kind = "Plan".into();
        assert_eq!(header_kind(&l), "Plan \u{b7} Haiku");
        l.model = None;
        assert_eq!(header_kind(&l), "Plan");
    }

    /// The closing bar's words (the lanes review, fix 2 and the not-ready copy): the strong
    /// words, the quiet rest, and a tooltip on the words.
    #[test]
    fn the_closing_bar_says_what_became_of_the_lane() {
        let t = Some("1:33");
        let words = |why: &Bar, time| bar_words(why, "Explore", time);
        assert_eq!(
            words(&Bar::Finished(LaneStatus::Done), t),
            ("Explore finished".into(), Some(" in 1:33".into()), None)
        );
        assert_eq!(
            words(&Bar::Finished(LaneStatus::Done), None),
            ("Explore finished".into(), None, None)
        );
        assert_eq!(
            words(
                &Bar::Finished(LaneStatus::Failed(" no such file ".into())),
                t
            ),
            (
                "Explore failed".into(),
                Some(" \u{b7} no such file".into()),
                Some("no such file".into())
            )
        );
        assert_eq!(
            words(&Bar::Finished(LaneStatus::Failed(String::new())), t),
            ("Explore failed".into(), None, None)
        );
        assert_eq!(
            words(&Bar::Finished(LaneStatus::Stopped), Some("0:48")),
            (
                "Explore was stopped".into(),
                Some(" after 0:48".into()),
                None
            )
        );
        assert_eq!(
            words(&Bar::Finished(LaneStatus::Ended), t),
            (
                "Explore ended".into(),
                Some(" without a report".into()),
                None
            )
        );
        assert_eq!(
            words(&Bar::Foreground, t),
            (
                "Explore is working".into(),
                Some(" \u{b7} Main is waiting on it, so messages can't reach it yet".into()),
                Some(
                    "Main waits for Explore to finish, so it can't pass a message on until \
                     then. Ask Main now and it reads it when Explore is done."
                        .into()
                )
            )
        );
        assert_eq!(
            words(&Bar::NotReady, t),
            (
                "Explore is working".into(),
                Some(" \u{b7} you can message it once Main has started".into()),
                None
            )
        );
    }

    /// The user's rule ("make sure I cannot send a message when subagent is done"), for every
    /// provider: a finished agy lane gets the closing bar too, never a box to Main.
    #[test]
    fn a_finished_agy_lane_gets_the_closing_bar() {
        let mut agy = lane("a", "Explore");
        agy.controls = LaneControls {
            stop: false,
            message: false,
            woken_by_steps: true,
        };
        for status in [
            LaneStatus::Done,
            LaneStatus::Failed("no such file".into()),
            LaneStatus::Stopped,
            LaneStatus::Ended,
        ] {
            let mut done = agy.clone();
            done.status = status.clone();
            for (ready, images) in [(true, false), (false, false), (true, true)] {
                assert_eq!(
                    lane_box(&done, false, ready, images),
                    LaneBox::Bar(Bar::Finished(status.clone())),
                    "{status:?}"
                );
            }
        }
        // Only a running agy lane keeps the box to Main.
        let mut waiting = agy.clone();
        waiting.status = LaneStatus::Waiting;
        assert_eq!(lane_box(&waiting, false, true, false), LaneBox::ToMain);
    }

    #[test]
    fn pills_that_fit_show_the_rest_behind_plus_n() {
        // Main (60) and three lanes (50 each), 8 apart; "+N" takes 32.
        let widths = [60.0, 50.0, 50.0, 50.0];
        assert_eq!(fit(&widths, 400.0, 0), (vec![0, 1, 2, 3], 0), "all fit");
        let (shown, hidden) = fit(&widths, 210.0, 0);
        assert_eq!(shown, [0, 1, 2], "as many as fit with room for +N");
        assert_eq!(hidden, 1);
        // The active pill is always shown (in place of the last that fit).
        let (shown, hidden) = fit(&widths, 210.0, 3);
        assert!(shown.contains(&3) && shown.contains(&0), "{shown:?}");
        assert_eq!(hidden, 1);
        assert_eq!(fit(&[], 100.0, 0), (vec![], 0));
        // A wider "+N" (its live mark: the lanes chore) leaves room for one pill fewer.
        let (shown, hidden) = fit_with(&widths, 210.0, 0, MORE_W + 17.0);
        assert_eq!(shown, [0, 1], "{shown:?}");
        assert_eq!(hidden, 2);
    }

    #[test]
    fn elapsed_stops_when_the_lane_ends() {
        let t0 = Instant::now();
        let t1 = t0 + Duration::from_secs(62);
        let later = t1 + Duration::from_secs(100);
        assert_eq!(elapsed(Some(t0), None, t1), Some(Duration::from_secs(62)));
        assert_eq!(
            elapsed(Some(t0), Some(t1), later),
            Some(Duration::from_secs(62)),
            "it stops at the end"
        );
        assert_eq!(elapsed(None, None, later), None, "no start: no time");
        assert_eq!(clock(Duration::from_secs(62)), "1:02");
        assert_eq!(clock(Duration::from_secs(5)), "0:05");
    }

    #[test]
    fn a_lane_that_stopped_without_an_event_stops_its_clock() {
        // Review 4: a lane stopped by the process ending gets its end stamped.
        let t0 = Instant::now();
        let mut times = std::collections::HashMap::from([
            ("live".to_string(), (t0, None)),
            ("gone".to_string(), (t0, None)),
        ]);
        let mut live = lane("live", "A");
        live.status = LaneStatus::Working;
        let mut gone = lane("gone", "B");
        gone.status = LaneStatus::Stopped;
        let now = t0 + Duration::from_secs(5);
        stamp_ends(&mut times, &[live, gone], now);
        assert_eq!(times["live"].1, None);
        assert_eq!(times["gone"].1, Some(now));
        // Stamped once: a later pass keeps the first end.
        stamp_ends(
            &mut times,
            &[lane("gone", "B")],
            now + Duration::from_secs(9),
        );
        assert_eq!(times["gone"].1, Some(now));
    }

    #[test]
    fn subagents_still_running_keep_the_tab_working() {
        // Your report (2026-11-22): "it says done … but in reality there are subagents running".
        use crate::status::Status;
        let running = lane("a", "A");
        let mut done = lane("b", "B");
        done.status = LaneStatus::Done;
        assert_eq!(
            shown_status(Status::Done, &[done.clone(), running.clone()]),
            Status::Working
        );
        assert_eq!(
            shown_status(Status::Idle, std::slice::from_ref(&running)),
            Status::Working
        );
        assert_eq!(
            shown_status(Status::Done, &[done.clone()]),
            Status::Done,
            "all finished"
        );
        assert_eq!(
            shown_status(Status::Waiting, std::slice::from_ref(&running)),
            Status::Waiting,
            "a question wins"
        );
        // Your words (2026-11-22): "if subagents are working, change cultivating to divining";
        // renamed 2026-11-24: "'Divining' label status indicator, change it to 'Orchestrating'".
        assert_eq!(
            running_word(std::slice::from_ref(&running)),
            Some("Orchestrating…".into())
        );
        assert_eq!(
            running_word(&[running.clone(), lane("c", "C")]),
            Some("Orchestrating…".into())
        );
        assert_eq!(running_word(&[done]), None);
    }

    #[test]
    fn a_finished_lanes_file_is_read_less_often() {
        let mut l = lane("a", "A");
        assert_eq!(
            follow_every(Some(&l), false),
            Duration::from_millis(500),
            "working"
        );
        l.status = LaneStatus::Done;
        assert_eq!(
            follow_every(Some(&l), false),
            Duration::from_secs(5),
            "done: a woken one still shows"
        );
        assert_eq!(follow_every(None, false), Duration::from_secs(5), "gone");
        // agy's main transcript (2026-11-24): a question shows within half a second.
        assert_eq!(follow_every(None, true), Duration::from_millis(500), "main");
    }

    #[test]
    fn stop_shows_only_when_the_lane_can() {
        let mut working = lane("a", "A");
        working.controls.stop = true;
        assert!(shows_stop(&working), "Claude's, working");
        let mut waiting = working.clone();
        waiting.status = LaneStatus::Waiting;
        assert!(shows_stop(&waiting));
        let mut done = working.clone();
        done.status = LaneStatus::Done;
        assert!(!shows_stop(&done), "nothing to stop");
        assert!(!shows_stop(&lane("b", "B")), "agy's: no control");
    }

    #[test]
    fn steps_say_one_or_many() {
        assert_eq!(steps_label(1), "1 step");
        assert_eq!(steps_label(14), "14 steps");
        assert_eq!(steps_label(0), "No steps");
    }

    #[test]
    fn a_pill_label_is_cut_with_an_ellipsis() {
        assert_eq!(pill_label("Workspace File Reader", 12), "Workspace F…");
        assert_eq!(pill_label("Reader", 12), "Reader");
        assert_eq!(pill_label("Reader", 6), "Reader", "exactly the limit");
    }

    #[test]
    fn repeated_labels_are_numbered() {
        let lanes = [
            lane("a", "Reader"),
            lane("b", "Reader"),
            lane("c", "Tester"),
        ];
        assert_eq!(pill_labels(&lanes, 20), ["Reader", "Reader 2", "Tester"]);
        // Cut short, the number stays (it tells them apart).
        let long = [
            lane("a", "Workspace File Reader"),
            lane("b", "Workspace File Reader"),
        ];
        assert_eq!(pill_labels(&long, 12), ["Workspace F…", "Workspace… 2"]);
    }

    #[test]
    fn stepping_goes_through_main_then_the_lanes() {
        // Pills newest first after Main (your words, 2026-11-22: "make sure it sorts by
        // newest"): `b` started after `a`.
        let lanes = [lane("a", "A"), lane("b", "B")];
        assert_eq!(shown_order(&lanes), [1, 0]);
        assert_eq!(
            step(&View::Main, &lanes, 1),
            View::Lane("b".into()),
            "the newest"
        );
        assert_eq!(step(&View::Main, &lanes, -1), View::Main, "Main is first");
        assert_eq!(step(&View::Lane("b".into()), &lanes, -1), View::Main);
        assert_eq!(
            step(&View::Lane("b".into()), &lanes, 1),
            View::Lane("a".into())
        );
        assert_eq!(
            step(&View::Lane("a".into()), &lanes, 1),
            View::Lane("a".into()),
            "no wrap"
        );
        assert_eq!(step(&View::Main, &[], 1), View::Main, "no lanes");
        assert_eq!(
            step(&View::Lane("gone".into()), &lanes, 1),
            View::Main,
            "a lane that went: back to Main"
        );
    }
}
