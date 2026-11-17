//! The edit ledger: an app-wide record of agents' file edits and working spans, and the rule that
//! decides who changed a file (M11af, design note 82 section 3). Pure: times are passed in.
#![allow(dead_code)] // wired in by M11af Tasks 4-5

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gpui::{BorrowAppContext, Global};

use crate::layout_model::WorkspaceId;
use crate::live_lines::{AgentMark, Sure, Who, letter_of};

/// How long edits and finished working spans are remembered.
pub const KEEP: Duration = Duration::from_secs(600);
/// Allowance for the poll and the read around a tab's previous read.
pub const SLACK: Duration = Duration::from_secs(2);

struct Edit {
    call: String,
    agent: AgentMark,
    paths: Vec<PathBuf>,
    started: Instant,
    /// `(finish time, ok)` once the tool call has finished.
    finished: Option<(Instant, bool)>,
}

struct Span {
    ws: WorkspaceId,
    agent: AgentMark,
    key: u64,
    start: Instant,
    end: Option<Instant>,
}

/// An app-wide GPUI global, set once in `main`.
#[derive(Default)]
pub struct Ledger {
    edits: Vec<Edit>,
    spans: Vec<Span>,
}

impl Global for Ledger {}

impl Ledger {
    /// `paths` canonical (the caller joins relative ones to the agent's cwd and canonicalizes).
    pub fn edit_started(&mut self, call: &str, agent: AgentMark, paths: Vec<PathBuf>, at: Instant) {
        self.edits.push(Edit {
            call: call.to_string(),
            agent,
            paths,
            started: at,
            finished: None,
        });
    }

    /// Closes the open edit for `call`; an unknown or already finished call is ignored.
    pub fn edit_finished(&mut self, call: &str, ok: bool, at: Instant) {
        if let Some(e) = self
            .edits
            .iter_mut()
            .find(|e| e.call == call && e.finished.is_none())
        {
            e.finished = Some((at, ok));
        }
    }

    /// `key`: a terminal's session id or an agent tab's slot id; `on`: Working began or ended.
    pub fn working(&mut self, ws: WorkspaceId, agent: AgentMark, key: u64, on: bool, at: Instant) {
        let open = self
            .spans
            .iter_mut()
            .find(|s| s.ws == ws && s.key == key && s.end.is_none());
        match (open, on) {
            (None, true) => self.spans.push(Span {
                ws,
                agent,
                key,
                start: at,
                end: None,
            }),
            (Some(s), false) => s.end = Some(at),
            _ => {}
        }
    }

    /// Ends the open working span with `key`, whatever its workspace (its terminal or agent tab
    /// is gone); none open: nothing.
    pub fn end_working(&mut self, key: u64, at: Instant) {
        for s in self
            .spans
            .iter_mut()
            .filter(|s| s.key == key && s.end.is_none())
        {
            s.end = Some(at);
        }
    }

    /// Agent tab `slot` is gone: its open edits (call ids `a{slot}:..`) end as not ok and its
    /// working span ends, so nothing of it stays open.
    pub fn end_agent(&mut self, slot: u64, at: Instant) {
        let prefix = format!("a{slot}:");
        for e in self
            .edits
            .iter_mut()
            .filter(|e| e.finished.is_none() && e.call.starts_with(&prefix))
        {
            e.finished = Some((at, false));
        }
        self.end_working(agent_key(slot), at);
    }

    /// Who changed `path` between `since` (the tab's previous read) and `now`; the bool: an edit to
    /// `path` is still open (started, not finished).
    pub fn decide(
        &self,
        path: &Path,
        ws: Option<WorkspaceId>,
        since: Instant,
        now: Instant,
    ) -> (Who, Sure, bool) {
        let from = since.checked_sub(SLACK).unwrap_or(since);
        // Exact: the latest agent-tab edit to `path` that finished ok since `from`, or is still open.
        let mut best: Option<(Instant, &AgentMark)> = None;
        let mut open = false;
        for e in self
            .edits
            .iter()
            .filter(|e| e.paths.iter().any(|p| p == path))
        {
            let when = match e.finished {
                Some((f, true)) if f >= from => f,
                None if e.started <= now => {
                    open = true;
                    e.started
                }
                _ => continue,
            };
            if best.is_none_or(|(w, _)| when >= w) {
                best = Some((when, &e.agent));
            }
        }
        if let Some((_, agent)) = best {
            return (Who::Agent(agent.clone()), Sure::Exact, open);
        }
        // Guess: an agent in the tab's own workspace was working at some point in `since..=now`.
        if let Some(ws) = ws {
            let guess = self
                .spans
                .iter()
                .filter(|s| s.ws == ws && s.start <= now && s.end.unwrap_or(now) >= since)
                .max_by_key(|s| s.start);
            if let Some(s) = guess {
                return (Who::Agent(s.agent.clone()), Sure::Guess, false);
            }
        }
        (Who::You, Sure::Exact, false)
    }

    /// An agent edit to `path` that finished ok in `read_at ..= read_at + SLACK`.
    pub fn revise(&self, path: &Path, read_at: Instant) -> Option<AgentMark> {
        let until = read_at + SLACK;
        self.edits
            .iter()
            .filter(|e| e.paths.iter().any(|p| p == path))
            .filter_map(|e| match e.finished {
                Some((f, true)) if f >= read_at && f <= until => Some((f, &e.agent)),
                _ => None,
            })
            .max_by_key(|(f, _)| *f)
            .map(|(_, a)| a.clone())
    }

    /// Drops edits and ended spans older than [`KEEP`]; a span still on stays.
    pub fn prune(&mut self, now: Instant) {
        let Some(cut) = now.checked_sub(KEEP) else {
            return;
        };
        self.edits
            .retain(|e| e.finished.map_or(e.started, |(f, _)| f) >= cut);
        self.spans.retain(|s| s.end.is_none_or(|e| e >= cut));
    }
}

/// An agent tab's working-span key: its slot id with the top bit set, so it can't meet a
/// terminal's session id.
pub fn agent_key(slot: u64) -> u64 {
    (1u64 << 63) | slot
}

/// Runs `f` on the app's ledger at the present time, then prunes it. A scene that never set the
/// global gets an empty ledger.
pub fn update(cx: &mut gpui::App, f: impl FnOnce(&mut Ledger, Instant)) {
    if !cx.has_global::<Ledger>() {
        cx.set_global(Ledger::default());
    }
    cx.update_global::<Ledger, _>(|ledger, _| {
        let now = Instant::now();
        f(ledger, now);
        ledger.prune(now);
    });
}

/// The mark drawn for agent program `name`: a provider fuxx knows has its own colour; any other
/// agent is drawn grey (`rgb: None`).
pub fn agent_mark_for(name: &str) -> AgentMark {
    match crate::providers::by_name(name) {
        Some(p) => {
            let info = p.info();
            AgentMark {
                name: info.name.into(),
                letter: letter_of(info.name),
                rgb: Some(info.badge_rgb),
            }
        }
        None => AgentMark {
            name: name.into(),
            letter: letter_of(name),
            rgb: None,
        },
    }
}

/// The canonical path of a file an agent's tool call writes, as the provider read it: empty is
/// nothing; a leading `~` is the home folder; a relative path is the agent's `cwd`'s. The path is
/// canonicalized when the file exists, else its folder is and the file name is put back, else it
/// is kept as joined.
pub fn resolve_write(raw: &str, cwd: &Path, home: Option<&Path>) -> Option<PathBuf> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let expanded = match (raw.strip_prefix('~'), home) {
        (Some(""), Some(h)) => h.to_path_buf(),
        (Some(rest), Some(h)) if rest.starts_with('/') => h.join(rest.trim_start_matches('/')),
        _ => PathBuf::from(raw),
    };
    let joined = cwd.join(expanded); // an absolute path replaces the cwd
    if let Ok(c) = joined.canonicalize() {
        return Some(c);
    }
    if let (Some(parent), Some(name)) = (joined.parent(), joined.file_name())
        && let Ok(c) = parent.canonicalize()
    {
        return Some(c.join(name));
    }
    Some(joined)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mark(name: &str) -> AgentMark {
        AgentMark {
            name: name.into(),
            letter: letter_of(name),
            rgb: None,
        }
    }
    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }
    fn secs(t: Instant, n: u64) -> Instant {
        t + Duration::from_secs(n)
    }
    const WS1: WorkspaceId = WorkspaceId(1);
    const WS2: WorkspaceId = WorkspaceId(2);

    fn edit(
        l: &mut Ledger,
        call: &str,
        who: &str,
        path: &str,
        start: Instant,
        fin: Option<Instant>,
        ok: bool,
    ) {
        l.edit_started(call, mark(who), vec![p(path)], start);
        if let Some(f) = fin {
            l.edit_finished(call, ok, f);
        }
    }

    #[test]
    fn an_agent_tab_edit_since_the_last_read_is_exact() {
        let t0 = Instant::now();
        let mut l = Ledger::default();
        edit(
            &mut l,
            "c1",
            "claude",
            "/a/f.rs",
            secs(t0, 10),
            Some(secs(t0, 11)),
            true,
        );
        let (who, sure, open) = l.decide(&p("/a/f.rs"), Some(WS1), secs(t0, 5), secs(t0, 12));
        assert_eq!(who, Who::Agent(mark("claude")));
        assert_eq!(sure, Sure::Exact);
        assert!(!open);
        // another workspace than the agent's: the path is what matters
        let (who, sure, _) = l.decide(&p("/a/f.rs"), Some(WS2), secs(t0, 5), secs(t0, 12));
        assert_eq!((who, sure), (Who::Agent(mark("claude")), Sure::Exact));
        // a different path is not it
        let (who, _, _) = l.decide(&p("/a/g.rs"), Some(WS1), secs(t0, 5), secs(t0, 12));
        assert_eq!(who, Who::You);
    }

    #[test]
    fn an_edit_finished_before_since_minus_slack_does_not_count() {
        let t0 = Instant::now();
        let mut l = Ledger::default();
        edit(
            &mut l,
            "c1",
            "claude",
            "/a/f.rs",
            secs(t0, 10),
            Some(secs(t0, 11)),
            true,
        );
        // since - SLACK = 12.5 > 11
        let since = secs(t0, 11) + SLACK + Duration::from_millis(500);
        let (who, sure, open) = l.decide(&p("/a/f.rs"), None, since, secs(t0, 30));
        assert_eq!((who, sure, open), (Who::You, Sure::Exact, false));
    }

    #[test]
    fn one_within_the_slack_counts() {
        let t0 = Instant::now();
        let mut l = Ledger::default();
        edit(
            &mut l,
            "c1",
            "claude",
            "/a/f.rs",
            secs(t0, 10),
            Some(secs(t0, 11)),
            true,
        );
        let since = secs(t0, 11) + SLACK - Duration::from_millis(500);
        let (who, sure, _) = l.decide(&p("/a/f.rs"), None, since, secs(t0, 30));
        assert_eq!((who, sure), (Who::Agent(mark("claude")), Sure::Exact));
    }

    #[test]
    fn an_open_edit_counts_as_exact_and_is_still_open() {
        let t0 = Instant::now();
        let mut l = Ledger::default();
        edit(&mut l, "c1", "claude", "/a/f.rs", secs(t0, 10), None, true);
        let (who, sure, open) = l.decide(&p("/a/f.rs"), None, secs(t0, 5), secs(t0, 11));
        assert_eq!(
            (who, sure, open),
            (Who::Agent(mark("claude")), Sure::Exact, true)
        );
    }

    #[test]
    fn a_failed_edit_does_not_count() {
        let t0 = Instant::now();
        let mut l = Ledger::default();
        edit(
            &mut l,
            "c1",
            "claude",
            "/a/f.rs",
            secs(t0, 10),
            Some(secs(t0, 11)),
            false,
        );
        let (who, sure, open) = l.decide(&p("/a/f.rs"), None, secs(t0, 5), secs(t0, 12));
        assert_eq!((who, sure, open), (Who::You, Sure::Exact, false));
        assert_eq!(l.revise(&p("/a/f.rs"), secs(t0, 11)), None);
    }

    #[test]
    fn a_working_agent_in_the_same_workspace_is_a_guess() {
        let t0 = Instant::now();
        let mut l = Ledger::default();
        l.working(WS1, mark("claude"), 7, true, secs(t0, 3));
        let (who, sure, open) = l.decide(&p("/a/f.rs"), Some(WS1), secs(t0, 5), secs(t0, 12));
        assert_eq!(
            (who, sure, open),
            (Who::Agent(mark("claude")), Sure::Guess, false)
        );
        // a span that ended before `since` does not overlap
        l.working(WS1, mark("claude"), 7, false, secs(t0, 4));
        let (who, _, _) = l.decide(&p("/a/f.rs"), Some(WS1), secs(t0, 5), secs(t0, 12));
        assert_eq!(who, Who::You);
    }

    #[test]
    fn a_working_agent_in_another_workspace_is_not() {
        let t0 = Instant::now();
        let mut l = Ledger::default();
        l.working(WS2, mark("claude"), 7, true, secs(t0, 3));
        let (who, sure, _) = l.decide(&p("/a/f.rs"), Some(WS1), secs(t0, 5), secs(t0, 12));
        assert_eq!((who, sure), (Who::You, Sure::Exact));
        let (who, _, _) = l.decide(&p("/a/f.rs"), None, secs(t0, 5), secs(t0, 12));
        assert_eq!(who, Who::You);
    }

    #[test]
    fn two_agents_working_the_last_started_wins() {
        let t0 = Instant::now();
        let mut l = Ledger::default();
        l.working(WS1, mark("claude"), 1, true, secs(t0, 3));
        l.working(WS1, mark("codex"), 2, true, secs(t0, 6));
        let (who, sure, _) = l.decide(&p("/a/f.rs"), Some(WS1), secs(t0, 5), secs(t0, 12));
        assert_eq!((who, sure), (Who::Agent(mark("codex")), Sure::Guess));
    }

    #[test]
    fn nothing_else_is_yours() {
        let l = Ledger::default();
        let t0 = Instant::now();
        let r = l.decide(&p("/a/f.rs"), Some(WS1), secs(t0, 1), secs(t0, 2));
        assert_eq!(r, (Who::You, Sure::Exact, false));
    }

    #[test]
    fn two_writers_in_one_read_agent_wins() {
        let t0 = Instant::now();
        let mut l = Ledger::default();
        edit(
            &mut l,
            "c1",
            "claude",
            "/a/f.rs",
            secs(t0, 6),
            Some(secs(t0, 7)),
            true,
        );
        // your save lands in the same read; the ledger has nothing for it
        let (who, sure, _) = l.decide(&p("/a/f.rs"), Some(WS1), secs(t0, 5), secs(t0, 12));
        assert_eq!((who, sure), (Who::Agent(mark("claude")), Sure::Exact));
    }

    #[test]
    fn revise_finds_a_late_finish() {
        let t0 = Instant::now();
        let mut l = Ledger::default();
        edit(
            &mut l,
            "c1",
            "claude",
            "/a/f.rs",
            secs(t0, 9),
            Some(secs(t0, 11)),
            true,
        );
        assert_eq!(l.revise(&p("/a/f.rs"), secs(t0, 10)), Some(mark("claude")));
        // exactly at the window's end still counts
        assert_eq!(l.revise(&p("/a/f.rs"), secs(t0, 9)), Some(mark("claude")));
    }

    #[test]
    fn revise_ignores_one_after_the_window() {
        let t0 = Instant::now();
        let mut l = Ledger::default();
        edit(
            &mut l,
            "c1",
            "claude",
            "/a/f.rs",
            secs(t0, 9),
            Some(secs(t0, 13)),
            true,
        );
        assert_eq!(l.revise(&p("/a/f.rs"), secs(t0, 10)), None);
        // one that finished before the read is not a late finish either
        assert_eq!(l.revise(&p("/a/f.rs"), secs(t0, 14)), None);
    }

    #[test]
    fn prune_drops_what_is_older_than_ten_minutes() {
        let t0 = Instant::now();
        let mut l = Ledger::default();
        edit(
            &mut l,
            "old",
            "claude",
            "/a/f.rs",
            t0,
            Some(secs(t0, 1)),
            true,
        );
        edit(
            &mut l,
            "new",
            "claude",
            "/a/g.rs",
            secs(t0, 590),
            Some(secs(t0, 595)),
            true,
        );
        l.working(WS1, mark("claude"), 1, true, t0); // still on
        l.working(WS1, mark("codex"), 2, true, t0);
        l.working(WS1, mark("codex"), 2, false, secs(t0, 2)); // ended long ago
        l.prune(secs(t0, 700));
        assert_eq!(l.edits.len(), 1);
        assert_eq!(l.edits[0].call, "new");
        assert_eq!(l.spans.len(), 1);
        assert_eq!(&*l.spans[0].agent.name, "claude");
    }

    #[test]
    fn agent_mark_for_known_and_unknown_agents() {
        let claude = agent_mark_for("claude");
        assert_eq!(&*claude.name, "claude");
        assert_eq!(claude.letter, 'C');
        let badge = crate::providers::by_name("claude")
            .unwrap()
            .info()
            .badge_rgb;
        assert_eq!(claude.rgb, Some(badge));
        let codex = agent_mark_for("codex");
        assert_eq!(&*codex.name, "codex");
        assert_eq!(codex.letter, 'C');
        assert_eq!(codex.rgb, None);
    }

    #[test]
    fn resolve_write_joins_expands_and_canonicalizes() {
        let cwd = Path::new("/nonexistent-fx/proj");
        let home = Path::new("/nonexistent-home");
        assert_eq!(resolve_write("", cwd, Some(home)), None);
        assert_eq!(resolve_write("  ", cwd, Some(home)), None);
        assert_eq!(
            resolve_write("a/b.rs", cwd, Some(home)),
            Some(p("/nonexistent-fx/proj/a/b.rs"))
        );
        assert_eq!(
            resolve_write("/x/y.rs", cwd, Some(home)),
            Some(p("/x/y.rs"))
        );
        assert_eq!(
            resolve_write("~/n.txt", cwd, Some(home)),
            Some(p("/nonexistent-home/n.txt"))
        );
        // An existing file is canonical (`..` and symlinks resolved); a new one in an existing
        // folder gets the folder's canonical path.
        let dir = std::env::temp_dir().canonicalize().unwrap();
        assert_eq!(
            resolve_write("new-file-fx.txt", &dir.join("."), None),
            Some(dir.join("new-file-fx.txt"))
        );
        assert_eq!(resolve_write(".", &dir, None), Some(dir.clone()));
    }

    #[test]
    fn a_gone_agent_and_a_gone_terminal_leave_nothing_open() {
        let t0 = Instant::now();
        let mut l = Ledger::default();
        l.edit_started("a1:t1", mark("claude"), vec![p("/a/f.rs")], t0);
        l.edit_started("a11:t1", mark("claude"), vec![p("/a/g.rs")], t0);
        l.working(WS1, mark("claude"), agent_key(1), true, t0);
        l.working(WS1, mark("codex"), 7, true, t0);
        l.end_agent(1, secs(t0, 5));
        assert!(l.edits[0].finished == Some((secs(t0, 5), false)));
        assert!(l.edits[1].finished.is_none(), "a11 is not a1");
        assert!(l.spans[0].end.is_some());
        assert!(l.spans[1].end.is_none(), "a terminal's key is its own");
        l.end_working(7, secs(t0, 6));
        assert_eq!(l.spans[1].end, Some(secs(t0, 6)));
    }
}
