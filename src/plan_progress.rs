//! Plan progress (the 2026-11-24 chores: "I want to be able to track each progress, so I won't
//! always prompt status"): a turn's task list, whatever workflow the agent follows, for the
//! burger tab's `4/6` and its popover. From the agent's own todo list when it keeps one, else
//! from the checklist file it works from (`- [ ]` / `- [x]`, a plan's `### Task N:` headings),
//! with the branch's commits marking what's done. Pure: the tab reads the file and git off the
//! UI thread and hands the text here.

use crate::providers::{TodoItem, TodoState};

/// Where a task stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TaskState {
    /// Done, with the commit that made it, when one is known.
    Done {
        commit: Option<String>,
    },
    /// Being built now.
    Building,
    Next,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanTask {
    pub title: String,
    pub state: TaskState,
}

/// The fewest tasks that make a list worth a tab.
pub const MIN_TASKS: usize = 2;

/// The agent's own todo list as tasks.
pub fn from_todos(todos: &[TodoItem]) -> Vec<PlanTask> {
    todos
        .iter()
        .map(|t| PlanTask {
            title: t.text.clone(),
            state: match t.state {
                TodoState::Done => TaskState::Done { commit: None },
                TodoState::Active => TaskState::Building,
                TodoState::Pending => TaskState::Next,
            },
        })
        .collect()
}

/// A line's checkbox: ticked or not, and the words after it.
fn checkbox(line: &str) -> Option<(bool, &str)> {
    let rest = line.trim_start().strip_prefix(['-', '*'])?.trim_start();
    let (ticked, words) = if let Some(w) = rest.strip_prefix("[x]").or(rest.strip_prefix("[X]")) {
        (true, w)
    } else {
        (false, rest.strip_prefix("[ ]")?)
    };
    Some((ticked, words.trim()))
}

/// A `### Task N: …` heading's words.
fn task_heading(line: &str) -> Option<&str> {
    let words = line.trim_start().trim_start_matches('#');
    (line.trim_start().starts_with('#') && words.trim_start().starts_with("Task "))
        .then(|| words.trim())
}

/// A checklist file's tasks: its `### Task N: …` headings (a task done when every box under it
/// is ticked, being built when some are), else each `- [ ]` / `- [x]` line.
pub fn from_checklist(md: &str) -> Vec<PlanTask> {
    let headed = has_task_headings(md);
    if !headed {
        return md
            .lines()
            .filter_map(checkbox)
            .map(|(ticked, words)| PlanTask {
                title: words.to_string(),
                state: if ticked {
                    TaskState::Done { commit: None }
                } else {
                    TaskState::Next
                },
            })
            .collect();
    }
    // Each heading with how many of its boxes are ticked, of how many.
    let mut tasks: Vec<(String, usize, usize)> = Vec::new();
    for line in md.lines() {
        if let Some(title) = task_heading(line) {
            tasks.push((title.to_string(), 0, 0));
        } else if let (Some((ticked, _)), Some(task)) = (checkbox(line), tasks.last_mut()) {
            task.1 += usize::from(ticked);
            task.2 += 1;
        }
    }
    tasks
        .into_iter()
        .map(|(title, ticked, all)| PlanTask {
            title,
            state: match (ticked, all) {
                (t, a) if a > 0 && t == a => TaskState::Done { commit: None },
                (0, _) => TaskState::Next,
                _ => TaskState::Building,
            },
        })
        .collect()
}

/// Which of `files` (each: written by the turn, its text), in order, is the plan: the first
/// with at least `MIN_TASKS` tasks, a file only read needing `### Task N` headings (not a README).
/// Chore 14 of the 2026-11-25 batch: a note the turn wrote no longer hides the plan.
pub fn first_plan(files: &[(bool, &str)]) -> Option<usize> {
    files.iter().position(|(written, text)| {
        (*written || has_task_headings(text)) && from_checklist(text).len() >= MIN_TASKS
    })
}

/// The files to look for the plan in: the turn's own (`turn`, newest first), then the plan
/// shown already (`kept`), so a turn that never touches it keeps it; each path once.
pub fn plan_candidates(
    turn: &[(std::path::PathBuf, bool)],
    kept: Option<&(std::path::PathBuf, bool)>,
) -> Vec<(std::path::PathBuf, bool)> {
    let mut out = turn.to_vec();
    if let Some(kept) = kept
        && !out.iter().any(|(p, _)| *p == kept.0)
    {
        out.push(kept.clone());
    }
    out
}

/// Each `### Task N` heading's planned commit message (chore 15): the first backticked text on a
/// line of its section that says "commit"; none when the task names none.
pub fn planned_subjects(md: &str) -> Vec<Option<String>> {
    let mut out: Vec<Option<String>> = Vec::new();
    for line in md.lines() {
        if task_heading(line).is_some() {
            out.push(None);
            continue;
        }
        let Some(slot) = out.last_mut() else { continue };
        if slot.is_some() || !line.to_lowercase().contains("commit") {
            continue;
        }
        let at = line.to_lowercase().find("commit").unwrap_or(0);
        let rest = &line[at..];
        if let Some(open) = rest.find('`')
            && let Some(len) = rest[open + 1..].find('`')
        {
            let text = rest[open + 1..open + 1 + len].trim();
            if !text.is_empty() {
                *slot = Some(text.to_string());
            }
        }
    }
    out
}

/// Marks done the tasks whose planned commit message (`planned`, by task) a commit's subject
/// starts with, with that commit (chore 15: plans whose commits say what, not "Task N").
pub fn match_planned(
    tasks: &mut [PlanTask],
    commits: &[(String, String)],
    planned: &[Option<String>],
) {
    for (task, want) in tasks.iter_mut().zip(planned) {
        let Some(want) = want.as_deref().map(str::to_lowercase) else {
            continue;
        };
        if let Some((hash, _)) = commits
            .iter()
            .find(|(_, subject)| subject.trim().to_lowercase().starts_with(&want))
        {
            task.state = TaskState::Done {
                commit: Some(hash.clone()),
            };
        }
    }
}

/// Whether backticked `token` names a file: no spaces, a name, then a dot and a 1–5 letter
/// extension (`src/retry.rs`, `registry.rs`; not `Icon::Bot`, `c.todo_active` or `.output`).
fn file_token(token: &str) -> Option<&str> {
    let path = token.split(':').next()?.trim();
    let (stem, ext) = path.rsplit_once('.')?;
    let name = stem.rsplit('/').next()?;
    let ok = !path.contains(char::is_whitespace)
        && !name.is_empty()
        && (1..=5).contains(&ext.len())
        && ext.chars().all(|c| c.is_ascii_alphabetic())
        && path
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "/._-".contains(c));
    ok.then_some(path)
}

/// Each `### Task N` section's files (chore 20): the backticked paths its `**Files:**` block
/// names (that line and the lines under it, up to a blank line, a step's box or another bold
/// label), each once, in order; a bare name (`claude.rs`) a path of the same section ends in is
/// that file, so it goes. A path only in a step (`scripts/prune-target.sh`, run, never edited)
/// isn't one the task changes (chore 32).
pub fn task_files(md: &str) -> Vec<Vec<String>> {
    let mut out: Vec<Vec<String>> = Vec::new();
    let mut in_files = false;
    for line in md.lines() {
        if task_heading(line).is_some() {
            out.push(Vec::new());
            in_files = false;
            continue;
        }
        let Some(files) = out.last_mut() else {
            continue;
        };
        let text = line.trim_start();
        if text.starts_with("**Files") {
            in_files = true;
        } else if text.is_empty()
            || text.starts_with('#')
            || text.starts_with("**")
            || checkbox(line).is_some()
        {
            in_files = false;
        }
        if !in_files {
            continue;
        }
        // The odd pieces between backticks are the backticked ones.
        for token in line.split('`').skip(1).step_by(2) {
            if let Some(path) = file_token(token)
                && !files.iter().any(|f| f == path)
            {
                files.push(path.to_string());
            }
        }
    }
    for files in &mut out {
        let full = files.clone();
        files.retain(|f| f.contains('/') || !full.iter().any(|p| p != f && names_file(p, f)));
    }
    out
}

/// Whether a commit's changed `path` (from the repo's root) is the plan's `named` file: the
/// same, or `named` its tail (a bare `registry.rs`).
fn names_file(path: &str, named: &str) -> bool {
    path == named || path.ends_with(&format!("/{named}"))
}

/// Marks done the tasks not done yet whose named files (`files`, by task) the commits since the
/// plan (`commits`: hash and changed paths, newest first) touched, more than half of them, with
/// the oldest commit that touched the most of them (chore 20: commits saying "Chore 16: …"); a
/// task done by its ticked boxes gets its commit the same way, so later tasks count from it.
/// A commit counts for a task only when it is newer than the newest commit an earlier task was
/// done by, or touches a file this task names and no earlier one does (the review: an earlier
/// task's commit touching a later task's files ticked it before it was built).
/// A task naming no files (a closing "docs, screenshots, review") is done, once the task before
/// it is, by the first commit after the newest an earlier task was done by that touches none of
/// the plan's files (chore 32; the review: not a fix to an earlier task).
pub fn match_files(
    tasks: &mut [PlanTask],
    commits: &[(String, Vec<String>)],
    files: &[Vec<String>],
) {
    let touches = |paths: &[String], f: &str| paths.iter().any(|p| names_file(p, f));
    let same = |a: &str, b: &str| names_file(a, b) || names_file(b, a);
    let at = |hash: &str| commits.iter().position(|(h, _)| h == hash);
    // The newest commit an earlier task was done by, as its place (newest first: smaller).
    let mut floor: Option<usize> = None;
    let planned: Vec<&String> = files.iter().flatten().collect();
    for (k, named) in files.iter().enumerate().take(tasks.len()) {
        // Not done, or done by its boxes with no commit known.
        let open = !matches!(tasks[k].state, TaskState::Done { commit: Some(_) });
        if open && named.is_empty() {
            let not_done = !matches!(tasks[k].state, TaskState::Done { .. });
            let after_previous = k > 0 && matches!(tasks[k - 1].state, TaskState::Done { .. });
            let closing = floor.and_then(|f| {
                commits[..f]
                    .iter()
                    .rev()
                    .find(|(_, paths)| !planned.iter().any(|p| touches(paths, p)))
            });
            if not_done
                && after_previous
                && let Some((hash, _)) = closing
            {
                tasks[k].state = TaskState::Done {
                    commit: Some(hash.clone()),
                };
            }
        } else if open {
            let earlier: Vec<&String> = files[..k].iter().flatten().collect();
            let own: Vec<&String> = named
                .iter()
                .filter(|f| !earlier.iter().any(|e| same(e, f)))
                .collect();
            let counting: Vec<&(String, Vec<String>)> = commits
                .iter()
                .enumerate()
                .filter(|(i, (_, paths))| {
                    floor.is_none_or(|f| *i < f) || own.iter().any(|f| touches(paths, f))
                })
                .map(|(_, c)| c)
                .collect();
            let touched = named
                .iter()
                .filter(|f| counting.iter().any(|(_, paths)| touches(paths, f)))
                .count();
            if touched * 2 > named.len() {
                // Of those touching the most, the oldest: the one that built it (later ones fix
                // it).
                let best = counting
                    .iter()
                    .rev()
                    .map(|(hash, paths)| (named.iter().filter(|f| touches(paths, f)).count(), hash))
                    .fold(None::<(usize, &String)>, |best, (n, hash)| match best {
                        Some((m, _)) if m >= n => best,
                        _ => Some((n, hash)),
                    });
                tasks[k].state = TaskState::Done {
                    commit: best.map(|(_, hash)| hash.clone()),
                };
            }
        }
        if let TaskState::Done { commit: Some(hash) } = &tasks[k].state
            && let Some(i) = at(hash)
        {
            floor = Some(floor.map_or(i, |f| f.min(i)));
        }
    }
}

/// Whether commit subject `subject` names task number `n` ("Task 3:", "Task 3 ", "task 3").
fn names_task(subject: &str, n: usize) -> bool {
    let lower = subject.to_lowercase();
    let want = format!("task {n}");
    lower.match_indices(&want).any(|(at, _)| {
        !lower[at + want.len()..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit())
    })
}

/// Marks done the tasks a commit names (`Task 3:` in its subject), with that commit.
fn match_named(tasks: &mut [PlanTask], commits: &[(String, String)]) {
    for (i, task) in tasks.iter_mut().enumerate() {
        let named = commits
            .iter()
            .find(|(_, subject)| names_task(subject, i + 1))
            .map(|(hash, _)| hash.clone());
        if let Some(hash) = named {
            task.state = TaskState::Done { commit: Some(hash) };
        }
    }
}

/// If nothing is being built and some task is done already, the first task not done is (the
/// 2026-11-25 chores: a plan nobody has started shows no ◐).
fn mark_building(tasks: &mut [PlanTask]) {
    let started = tasks
        .iter()
        .any(|t| matches!(t.state, TaskState::Done { .. }));
    if started
        && !tasks.iter().any(|t| t.state == TaskState::Building)
        && let Some(first) = tasks.iter_mut().find(|t| t.state == TaskState::Next)
    {
        first.state = TaskState::Building;
    }
}

/// Done and all.
pub fn counts(tasks: &[PlanTask]) -> (usize, usize) {
    let done = tasks
        .iter()
        .filter(|t| matches!(t.state, TaskState::Done { .. }))
        .count();
    (done, tasks.len())
}

/// Whether a checklist file is a plan: it has `### Task N` headings. Only then is a file the
/// agent merely read taken for one, and only then do commits name its tasks (the review: a README
/// with boxes, or "Task 3" in an old milestone's commit, isn't this plan).
pub fn has_task_headings(md: &str) -> bool {
    md.lines().any(|l| task_heading(l).is_some())
}

/// A commit as the plan tab reads it: its short hash, subject, time (seconds since the epoch)
/// and the paths it changed (from the repo's root).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitCommit {
    pub hash: String,
    pub subject: String,
    pub at: i64,
    pub files: Vec<String>,
}

/// The commits that can mark a plan's tasks: those made since the plan file was first there
/// (`since`, seconds since the epoch).
pub fn commits_since(commits: &[GitCommit], since: i64) -> Vec<GitCommit> {
    commits.iter().filter(|c| c.at >= since).cloned().collect()
}

/// A plan's tasks marked by `commits` (newest first): by their planned messages (chore 15),
/// "Task N", then the files they name (chore 20; after the others, so a task naming no files
/// can follow a task "Task N" marked); then, when git's uncommitted paths are known (`dirty`,
/// from the repo's top), those being built (`mark_working`), and the next one in progress.
pub fn mark_done(
    tasks: &mut [PlanTask],
    md: &str,
    commits: &[GitCommit],
    dirty: Option<&[String]>,
) {
    let subjects: Vec<(String, String)> = commits
        .iter()
        .map(|c| (c.hash.clone(), c.subject.clone()))
        .collect();
    let touched: Vec<(String, Vec<String>)> = commits
        .iter()
        .map(|c| (c.hash.clone(), c.files.clone()))
        .collect();
    match_planned(tasks, &subjects, &planned_subjects(md));
    match_named(tasks, &subjects);
    let files = task_files(md);
    match_files(tasks, &touched, &files);
    if let Some(dirty) = dirty {
        mark_working(tasks, &files, dirty);
    }
    mark_building(tasks);
}

/// The tasks not done by the files they name (`files`, by task) and git's uncommitted paths
/// (chore 36): one whose own files change is ◐, any other ○ (some boxes ticked early is no
/// building). A file an earlier unfinished task names too is that task's. A plan naming no
/// files keeps its boxes' word.
fn mark_working(tasks: &mut [PlanTask], files: &[Vec<String>], dirty: &[String]) {
    if files.iter().all(Vec::is_empty) {
        return;
    }
    let same = |a: &str, b: &str| names_file(a, b) || names_file(b, a);
    let mut claimed: Vec<&String> = Vec::new();
    for (k, task) in tasks.iter_mut().enumerate() {
        if matches!(task.state, TaskState::Done { .. }) {
            continue;
        }
        let named = files.get(k).map_or(&[][..], Vec::as_slice);
        let changing = named
            .iter()
            .any(|f| dirty.iter().any(|p| names_file(p, f)) && !claimed.iter().any(|c| same(c, f)));
        task.state = if changing {
            TaskState::Building
        } else {
            TaskState::Next
        };
        claimed.extend(named);
    }
}

/// The uncommitted paths that can mark a plan's tasks (from `git status --porcelain=v1 -z`'s
/// `out`, run at the repo's `top`): every one but the plan file itself (`plan`), which changes
/// as its boxes are ticked.
pub fn plan_dirty(top: &std::path::Path, plan: &std::path::Path, out: &str) -> Vec<String> {
    crate::changed_files::porcelain(out)
        .into_iter()
        .map(|(_, path)| path)
        .filter(|path| top.join(path) != plan)
        .collect()
}

/// `git log --name-only --format=%x1e%h%x09%ct%x09%s`'s output as commits (a record each,
/// its changed paths on the lines after its header).
pub fn parse_log(out: &str) -> Vec<GitCommit> {
    out.split('\u{1e}')
        .filter_map(|record| {
            let mut lines = record.lines();
            let mut parts = lines.next()?.splitn(3, '\t');
            let hash = parts.next()?.to_string();
            let at = parts.next()?.parse::<i64>().ok()?;
            let subject = parts.next()?.to_string();
            let files = lines
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect();
            Some(GitCommit {
                hash,
                subject,
                at,
                files,
            })
        })
        .collect()
}

/// The tasks to show, given a finished plan hidden after your next message (`done`): none while
/// the list is still that one; the list, and `done` forgotten, once it changes (the review: a
/// finished plan came straight back after the next message).
pub fn after_done(done: &mut Option<Vec<PlanTask>>, tasks: Vec<PlanTask>) -> Vec<PlanTask> {
    match done {
        Some(finished) if *finished == tasks => Vec::new(),
        Some(_) => {
            *done = None;
            tasks
        }
        None => tasks,
    }
}

/// The task being built, as `(number, of)` for the status line's `Task 5/6`.
pub fn building(tasks: &[PlanTask]) -> Option<(usize, usize)> {
    if !shows(tasks) {
        return None;
    }
    let at = tasks.iter().position(|t| t.state == TaskState::Building)?;
    Some((at + 1, tasks.len()))
}

/// Whether the list is worth the burger tab.
pub fn shows(tasks: &[PlanTask]) -> bool {
    tasks.len() >= MIN_TASKS
}

#[cfg(test)]
mod real_plans;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn todo(text: &str, state: TodoState) -> TodoItem {
        TodoItem {
            text: text.into(),
            state,
        }
    }

    fn done(t: &PlanTask) -> bool {
        matches!(t.state, TaskState::Done { .. })
    }

    /// The tasks commits name ("Task 3:"), then the next one in progress, as `mark_done` reads
    /// subjects.
    fn match_commits(tasks: &mut [PlanTask], commits: &[(String, String)]) {
        match_named(tasks, commits);
        mark_building(tasks);
    }

    #[test]
    fn todos_become_tasks() {
        let tasks = from_todos(&[
            todo("Write the test", TodoState::Done),
            todo("Make it pass", TodoState::Active),
            todo("Commit", TodoState::Pending),
        ]);
        assert_eq!(tasks.len(), 3);
        assert!(done(&tasks[0]));
        assert_eq!(tasks[1].state, TaskState::Building);
        assert_eq!(tasks[2].state, TaskState::Next);
        assert_eq!(counts(&tasks), (1, 3));
    }

    #[test]
    fn a_checklist_becomes_tasks() {
        let md = "# Plan\n- [x] Task 1\n- [ ] Task 2\n";
        let tasks = from_checklist(md);
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].title, "Task 1");
        assert!(done(&tasks[0]));
        assert_eq!(tasks[1].title, "Task 2");
        assert_eq!(tasks[1].state, TaskState::Next);
    }

    #[test]
    fn a_plans_headings_are_its_tasks() {
        let md = "\
### Task 1: The model
- [x] **Step 1: test**
- [x] **Step 2: code**
### Task 2: The view
- [x] **Step 1: test**
- [ ] **Step 2: code**
### Task 3: Docs
- [ ] **Step 1: write**
";
        let tasks = from_checklist(md);
        assert_eq!(
            tasks.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(),
            ["Task 1: The model", "Task 2: The view", "Task 3: Docs"]
        );
        assert!(done(&tasks[0]));
        assert_eq!(tasks[1].state, TaskState::Building);
        assert_eq!(tasks[2].state, TaskState::Next);
    }

    #[test]
    fn a_commit_named_task_3_marks_task_3() {
        let mut tasks = from_checklist(
            "### Task 1: A\n- [ ] x\n### Task 2: B\n- [ ] x\n### Task 3: C\n- [ ] x\n",
        );
        match_commits(
            &mut tasks,
            &[
                ("abc1234".into(), "Task 3: C, built".into()),
                ("def5678".into(), "Task 1: A".into()),
            ],
        );
        assert_eq!(
            tasks[0].state,
            TaskState::Done {
                commit: Some("def5678".into())
            }
        );
        assert_eq!(
            tasks[2].state,
            TaskState::Done {
                commit: Some("abc1234".into())
            }
        );
        // "Task 13" is not "Task 1".
        let mut one = from_checklist("### Task 1: A\n- [ ] x\n### Task 2: B\n- [ ] x\n");
        match_commits(&mut one, &[("1111111".into(), "Task 13: other".into())]);
        assert!(!done(&one[0]));
    }

    /// The 2026-11-25 chores (your pick): a plan still being discussed shows no task in progress.
    #[test]
    fn nothing_started_shows_nothing_building() {
        let mut tasks = from_checklist(
            "### Task 1: A\n- [ ] x\n### Task 2: B\n- [ ] x\n### Task 3: C\n- [ ] x\n",
        );
        match_commits(&mut tasks, &[]);
        assert!(
            tasks.iter().all(|t| t.state == TaskState::Next),
            "{tasks:?}"
        );
    }

    #[test]
    fn after_a_done_task_the_next_is_building() {
        let mut tasks = from_checklist(
            "### Task 1: A\n- [ ] x\n### Task 2: B\n- [ ] x\n### Task 3: C\n- [ ] x\n",
        );
        match_commits(
            &mut tasks,
            &[("abc1234".into(), "Chore (Task 1): A".into())],
        );
        assert!(done(&tasks[0]));
        assert_eq!(tasks[1].state, TaskState::Building);
        assert_eq!(tasks[2].state, TaskState::Next);
    }

    #[test]
    fn a_ticked_step_is_building() {
        let tasks =
            from_checklist("### Task 1: A\n- [x] x\n- [ ] y\n- [ ] z\n### Task 2: B\n- [ ] x\n");
        assert_eq!(tasks[0].state, TaskState::Building);
    }

    /// Chore 14 (the 2026-11-25 batch: the tab vanished when a turn wrote a progress note): the
    /// first file that is a plan wins; a written `.md` with no tasks, or a read one with no
    /// `### Task N` headings, is passed over.
    #[test]
    fn the_first_real_plan_wins() {
        let note = "# SDD ledger\nTask 1: complete\n";
        let plan = "### Task 1: A\n- [ ] x\n### Task 2: B\n- [ ] y\n";
        let readme = "# Readme\n- [ ] a\n- [ ] b\n";
        let checklist = "- [ ] a\n- [x] b\n";
        assert_eq!(first_plan(&[(true, note), (false, plan)]), Some(1));
        assert_eq!(first_plan(&[(false, readme), (false, plan)]), Some(1));
        assert_eq!(first_plan(&[(true, checklist), (false, plan)]), Some(0));
        assert_eq!(first_plan(&[(true, note), (false, readme)]), None);
    }

    /// Chore 14: the turn's own files first (newest first), then the plan already shown, so a
    /// turn that never touches it keeps it.
    #[test]
    fn the_shown_plan_is_the_last_resort() {
        let p = |s: &str| PathBuf::from(s);
        let turn = [(p("/w/note.md"), true), (p("/w/plan.md"), false)];
        let kept = (p("/w/old-plan.md"), true);
        assert_eq!(
            plan_candidates(&turn, Some(&kept)),
            [
                (p("/w/note.md"), true),
                (p("/w/plan.md"), false),
                kept.clone()
            ]
        );
        let same = (p("/w/plan.md"), true);
        assert_eq!(
            plan_candidates(&turn, Some(&same)),
            [(p("/w/note.md"), true), (p("/w/plan.md"), false)],
            "each once"
        );
        assert_eq!(plan_candidates(&[], Some(&kept)), [kept]);
    }

    /// Chore 15 (the ☰ tab never moved while a plan in another repo was built): each task's
    /// planned commit message, from its commit step's backticks.
    #[test]
    fn a_task_names_its_planned_commit() {
        let md = "### Task 1: Icons\n- [ ] Step 3: commit `Lucide icons for every faked icon`.\n\
                  ### Task 2: Intro\n- [ ] Step 3: browser check. Commit `A first-load intro`.\n\
                  ### Task 3: Ship\n- [ ] Push.\n";
        assert_eq!(
            planned_subjects(md),
            [
                Some("Lucide icons for every faked icon".to_string()),
                Some("A first-load intro".to_string()),
                None
            ]
        );
    }

    /// A commit whose subject starts with a task's planned message marks it done, with that
    /// commit; then the next is in progress.
    #[test]
    fn a_planned_commit_marks_its_task() {
        let md = "### Task 1: Icons\n- [ ] commit `Lucide icons`\n### Task 2: Intro\n- [ ] commit `A first-load intro`\n";
        let mut tasks = from_checklist(md);
        let commits = [(
            "06c04f3".to_string(),
            "Lucide icons for every faked icon".to_string(),
        )];
        match_planned(&mut tasks, &commits, &planned_subjects(md));
        match_commits(&mut tasks, &commits);
        assert_eq!(
            tasks[0].state,
            TaskState::Done {
                commit: Some("06c04f3".into())
            }
        );
        assert_eq!(tasks[1].state, TaskState::Building);
    }

    /// Chore 16's plan (docs/superpowers/plans/2026-11-25-background-shells.md), its task
    /// sections' lines that name files, verbatim.
    const CHORE_16_PLAN: &str = "\
### Task 1: Events, Claude's reading and the stop
**Files:**
- Modify: `src/providers/mod.rs` (Event, Action), `src/providers/claude.rs` (system/user/assistant/encode + tests), `src/providers/agy.rs` (encode arm)
- Create: `src/providers/fixtures/claude-background-shells.jsonl` (recorded: `scripts/record-agent-fixture.py claude background-shells`; scenario added)
- [ ] Tests (`claude.rs`): `background_shells_fixture` (2 starts with descriptions, commands and `.output` paths; one `Done`, one `Stopped`); `stop_shell_writes_stop_task`. Run `cargo test --bin fuxx shell` → FAIL, implement, → PASS.
### Task 2: The shells' state
**Files:** Create `src/agent_shells.rs` (pure); modify `src/agent_conversation.rs` (field, apply, `you_to`, `process_gone`/`process_ended`), `src/main.rs` (mod).
- [ ] Tests (`agent_shells.rs`): counts/tone through start → end Done; header \"1 of 3 done · 2 running\"; `last_line` skips trailing blank lines and `\\r` progress; `gone` stops running ones.
### Task 3: The tab and its popover
**Files:** Create `src/agent_shells_view.rs` (impl AgentTab: `shells_tab`, `shells_popover`, `toggle_shells`, `stop_shell`, `open_shell_output`, `poll_shell_lines`); modify `src/agent_box.rs` (corner group: `folder_tab` takes the corner pills + popover; shared `corner_pill`; `BoxMenu::Shells`), `src/agent_box_menus.rs` (plan popover hung from the group; Shells closes on press-out), `src/agent_tab.rs` (fields: `shell_poll: Option<Task<()>>`), `src/main.rs`.
- [ ] Gates; GUI checked in Task 5's scenes.
### Task 4: The chat row, the ticking and the banner
**Files:** `src/agent_rows.rs` (a Bash row whose call has a running shell: the arc and \"running in the background · M:SS\"), `src/agent_tab.rs` (`keep_ticking` counts running shells and splices their rows; on `ShellEnded`, emit `AgentTabEvent::ShellEnded { title, failed }`), `src/workspace_agent.rs` (not looking: the 🔔 entry and the banner, through a shared tail of `notify_agent`).
- [ ] Tests: `row_words` above; the workspace's looking rule is the existing `focused_tab && window_active`.
### Task 5: Scenes, docs, commits
**Files:** Create `src/ui_shots/agent_shells.rs` (scenes `agent-shells-with-plan`, `agent-shells-alone`, `agent-shells-done`, `agent-shells-failed`, `agent-shells-popover`); register in `registry.rs` (AREAS, ORDER) and `mod.rs`; note 93 chore 16 + Progress Log.
- [ ] Assertions: the tab's words (`1/3`, `3/3 ✓`, `1/2 ✕`), a row click → `OpenFile`, ■ → a `stop_task` written.
- [ ] `scripts/ui-shots.sh --changed master`; look at the new PNGs; commit by name; `scripts/prune-target.sh`.
";

    /// Chore 16's commits on master (newest first): subject and the files each touched.
    fn chore_16_commits() -> Vec<(String, String, Vec<String>)> {
        let c = |hash: &str, subject: &str, files: &[&str]| {
            (
                hash.to_string(),
                subject.to_string(),
                files.iter().map(|f| f.to_string()).collect::<Vec<_>>(),
            )
        };
        vec![
            c(
                "a9eb5f2",
                "docs: chore 16 (background shells) in note 93 and the Progress Log; its design and plan",
                &[
                    "docs/04-Development/93 - Milestone 11ar - Subagents.md",
                    "docs/06-Progress-Log/60 - Progress Log.md",
                    "docs/superpowers/plans/2026-11-25-background-shells.md",
                    "docs/superpowers/specs/2026-11-25-background-shells-design.md",
                ],
            ),
            c(
                "25ae2a3",
                "Chore 16 review fixes: a crash or restart stops the Bash row's 'running in the background'; the shells popover closes when your message takes its tab",
                &[
                    "src/agent_conversation.rs",
                    "src/agent_shells.rs",
                    "src/agent_shells_view.rs",
                    "src/agent_tab.rs",
                    "src/ui_shots/agent_shells.rs",
                    "src/ui_shots/registry.rs",
                ],
            ),
            c(
                "c5e6861",
                "Chore 16: scenes for the shells tab (with the plan, alone, done, failed) and its popover; a background check has no outcome until it ends; the corner tabs keep the plan tab's old layout",
                &[
                    "src/agent_box.rs",
                    "src/agent_box_menus.rs",
                    "src/agent_conversation.rs",
                    "src/agent_shells_view.rs",
                    "src/agent_tab.rs",
                    "src/providers/claude.rs",
                    "src/providers/claude/shells.rs",
                    "src/ui_shots/agent_activity.rs",
                    "src/ui_shots/agent_shells.rs",
                    "src/ui_shots/mod.rs",
                    "src/ui_shots/registry.rs",
                ],
            ),
            c(
                "9bf9010",
                "Chore 16: the shells tab beside the plan tab, its popover (time, command, last line, stop, open the output), the chat's 'running in the background' row, and a banner when one ends",
                &[
                    "src/agent_box.rs",
                    "src/agent_box_menus.rs",
                    "src/agent_rows.rs",
                    "src/agent_shells_view.rs",
                    "src/agent_tab.rs",
                    "src/workspace_agent.rs",
                ],
            ),
            c(
                "7a2753f",
                "Chore 16: the background shells' state — counts, tone, rows that drop off, the last line, kept in the conversation",
                &[
                    "src/agent_conversation.rs",
                    "src/agent_shells.rs",
                    "src/main.rs",
                ],
            ),
            c(
                "37a70c3",
                "Chore 16: Claude's background shells as events — ShellStarted/ShellEnded from backgrounded local_bash tasks, StopShell as stop_task (recorded fixture)",
                &[
                    "scripts/record-agent-fixture.py",
                    "src/agent_conversation.rs",
                    "src/providers/agy.rs",
                    "src/providers/claude.rs",
                    "src/providers/claude/shells.rs",
                    "src/providers/fixtures/claude-background-shells.jsonl",
                    "src/providers/mod.rs",
                ],
            ),
        ]
    }

    /// Chore 20 (the plan tab said "Plan · 0 of 5" for chore 16's built plan): its commits say
    /// "Chore 16: …", not "Task N", and the plan names no commit messages; what each task
    /// shares with its commits is the files it names.
    #[test]
    fn chore_16s_plan_is_ticked_by_its_files() {
        let all = chore_16_commits();
        let subjects: Vec<(String, String)> =
            all.iter().map(|(h, s, _)| (h.clone(), s.clone())).collect();
        let touched: Vec<(String, Vec<String>)> =
            all.iter().map(|(h, _, f)| (h.clone(), f.clone())).collect();
        // The bug, as it was: nothing marks a task.
        let mut before = from_checklist(CHORE_16_PLAN);
        match_planned(&mut before, &subjects, &planned_subjects(CHORE_16_PLAN));
        match_commits(&mut before, &subjects);
        assert_eq!(counts(&before), (0, 5));
        // With the files: all five, each with the commit that touched the most of its files.
        let mut tasks = from_checklist(CHORE_16_PLAN);
        match_planned(&mut tasks, &subjects, &planned_subjects(CHORE_16_PLAN));
        match_files(&mut tasks, &touched, &task_files(CHORE_16_PLAN));
        match_commits(&mut tasks, &subjects);
        assert_eq!(counts(&tasks), (5, 5), "{tasks:?}");
        let commit = |t: &PlanTask| match &t.state {
            TaskState::Done { commit } => commit.clone(),
            _ => None,
        };
        assert_eq!(commit(&tasks[0]).as_deref(), Some("37a70c3"));
        assert_eq!(commit(&tasks[1]).as_deref(), Some("7a2753f"));
        // 9bf9010 and the later c5e6861 touch as many of its files: the first one built it.
        assert_eq!(commit(&tasks[2]).as_deref(), Some("9bf9010"));
        assert_eq!(commit(&tasks[3]).as_deref(), Some("9bf9010"));
        assert_eq!(commit(&tasks[4]).as_deref(), Some("c5e6861"));
    }

    /// A task's files: backticked paths in its section, each once (`:line` cut off); not
    /// commands, globs of words or names without an extension.
    #[test]
    fn a_task_names_its_files() {
        let files = task_files(CHORE_16_PLAN);
        assert_eq!(files.len(), 5);
        assert_eq!(
            files[1],
            [
                "src/agent_shells.rs",
                "src/agent_conversation.rs",
                "src/main.rs"
            ],
            "a bare name a path of the section ends in is that file, once"
        );
        assert_eq!(
            files[4],
            ["src/ui_shots/agent_shells.rs", "registry.rs", "mod.rs"],
            "a script the task only runs (`scripts/prune-target.sh`, in a step) isn't one it changes"
        );
        assert_eq!(
            task_files(
                "### Task 1: A\n**Files:**\n- Modify: `src/retry.rs:12-40`, `Icon::Bot`, `c.todo_active`, `.output`\n"
            ),
            [vec!["src/retry.rs".to_string()]]
        );
    }

    /// Chores 17–22's plan (docs/superpowers/plans/2026-11-26-chores-17-21.md), its tasks'
    /// `**Files:**` lines, verbatim.
    const CHORES_17_22_PLAN: &str = "\
### Task 1: Chore 20 — commits that touched a task's files tick it
**Files:** Modify `src/plan_progress.rs` (+ tests), `src/agent_tab.rs` (`refresh_plan`, `recent_commits`).
### Task 2: Chore 17 — one working mark
**Files:** `src/agent_tab.rs` (`status_line_shown`, `status_line_now`, `status_pill`), `src/agent_box.rs` (the status slot keeps the ledger popover without the pill), `src/ui_shots/agent_activity.rs` (`agent-ledger-running`).
### Task 3: Chore 18 — the left step meets the box's line
**Files:** `src/agent_box.rs` (`corner_tabs`: the step painted after the tab, in a wrapper with the step's 15 pt in flow), `src/ui_shots/agent_activity.rs` (`plan_scene`).
### Task 4: Chore 19 — the subagents corner tab and the breadcrumb
**Files:** Create `src/agent_subagents_view.rs` (impl AgentTab: `subagents_tab`, `subagents_popover`, `toggle_subagents`, `open_subagent`); create `src/ui_shots/agent_subagents.rs` (scenes); modify `src/agent_box.rs` (`BoxMenu::Subagents`, `corner_tabs` order: shells, plan, subagents), `src/agent_box_menus.rs` (plan popover: tasks only; Subagents closes on press-out), `src/agent_lanes_view.rs` (strip removed; `lane_header` → breadcrumb row), `src/agent_lanes.rs` (strip-only helpers removed, a pure `subagent_counts`/`tab_words`), `src/agent_tab.rs` (fields, render), `src/ui_shots/agent_lanes.rs` + registry.
### Task 5: Chore 21 — no empty band
**Files:** `src/agent_tab.rs` (`top_line` keeps only its width probe, no height; the scrims from the top), `src/agent_lanes_view.rs` (the breadcrumb row `pt(chat::FIRST_TOP − (row − pill)/2)`), scenes.
### Task 6: Chore 22 — the counts become chips on the status row's right
**Files:** `src/agent_ledger.rs` (`Ledger::checking`: a running call is a check), `src/agent_status_line.rs` (`Chip`, `ChipKind`, `ChipState`, `chips(ledger)`; `status_line` returns the main line only), `src/agent_tab.rs` (`status_strip`: one row, the main line truncating, the chips pinned right; `ledger_part`; the ledger popover hung from the chips), `src/agent_box.rs` (the status slot holds only the mark again), `src/agent_box_menus.rs` (`ledger_menu` shows the clicked chip's section), `src/ui_shots/agent_activity.rs`.
### Task 7: Whole change
- [ ] `scripts/ui-shots.sh --changed 5103f17`; one Opus review; `scripts/prune-target.sh`.
";

    /// The review of chores 17–22 (finding 1): replaying that plan's own commits one by one,
    /// each commit ticks its own task only — a later task whose files an earlier task's commit
    /// touched waits for a commit of its own. Task 7 names no files it changes (chore 32): the
    /// first commit after Task 6's ticks it, 7 of 7.
    #[test]
    fn a_plan_replayed_commit_by_commit_ticks_one_task_at_a_time() {
        let all: [(&str, &[&str]); 7] = [
            (
                "e178869",
                &[
                    "docs/superpowers/plans/2026-11-26-chores-17-21.md",
                    "src/agent_tab.rs",
                    "src/plan_progress.rs",
                ],
            ),
            (
                "cfcda49",
                &[
                    "src/agent_box.rs",
                    "src/agent_tab.rs",
                    "src/ui_shots/agent_activity.rs",
                ],
            ),
            (
                "25521df",
                &["src/agent_box.rs", "src/ui_shots/agent_activity.rs"],
            ),
            (
                "9bcdd46",
                &[
                    "src/agent_box.rs",
                    "src/agent_box_menus.rs",
                    "src/agent_lane_row.rs",
                    "src/agent_lanes.rs",
                    "src/agent_lanes_view.rs",
                    "src/agent_shells_view.rs",
                    "src/agent_subagents.rs",
                    "src/agent_subagents_view.rs",
                    "src/agent_tab.rs",
                    "src/icons.rs",
                    "src/main.rs",
                    "src/ui_shots/agent_lanes.rs",
                    "src/ui_shots/agent_subagents.rs",
                    "src/ui_shots/mod.rs",
                    "src/ui_shots/registry.rs",
                ],
            ),
            (
                "5d7ebd6",
                &[
                    "src/agent_lanes_view.rs",
                    "src/agent_tab.rs",
                    "src/ui_shots/agent.rs",
                    "src/ui_shots/agent_subagents.rs",
                ],
            ),
            (
                "8b52ed1",
                &[
                    "docs/superpowers/plans/2026-11-26-chores-17-21.md",
                    "src/agent_box.rs",
                    "src/agent_box_menus.rs",
                    "src/agent_ledger.rs",
                    "src/agent_status_line.rs",
                    "src/agent_status_view.rs",
                    "src/agent_tab.rs",
                    "src/ui_shots/agent_activity.rs",
                ],
            ),
            (
                "cd13597",
                &["docs/04-Development/93 - Milestone 11ar - Subagents.md"],
            ),
        ];
        for landed in 1..=all.len() {
            // Newest first, as git gives them.
            let touched: Vec<(String, Vec<String>)> = all[..landed]
                .iter()
                .rev()
                .map(|(h, f)| (h.to_string(), f.iter().map(|p| p.to_string()).collect()))
                .collect();
            let mut tasks = from_checklist(CHORES_17_22_PLAN);
            match_files(&mut tasks, &touched, &task_files(CHORES_17_22_PLAN));
            let done: Vec<usize> = (0..tasks.len()).filter(|&i| done(&tasks[i])).collect();
            assert_eq!(
                done,
                (0..landed).collect::<Vec<_>>(),
                "after {}",
                all[landed - 1].0
            );
        }
    }

    /// The review focus: a plan nobody has built ticks nothing by its files, nor does a commit
    /// that touched one of a task's three files.
    #[test]
    fn untouched_files_tick_nothing() {
        let md = "### Task 1: A\n**Files:** Modify `src/retry.rs`, `src/agent_tab.rs`, `src/main.rs`\n\
                  ### Task 2: B\n**Files:** Create `src/backoff.rs`\n";
        let mut tasks = from_checklist(md);
        match_files(&mut tasks, &[], &task_files(md));
        assert_eq!(counts(&tasks), (0, 2));
        let one = [("abc1234".to_string(), vec!["src/agent_tab.rs".to_string()])];
        match_files(&mut tasks, &one, &task_files(md));
        assert_eq!(counts(&tasks), (0, 2), "1 of 3 files is not the task");
    }

    #[test]
    fn the_first_unfinished_is_building() {
        let mut tasks = from_checklist("- [x] a\n- [ ] b\n- [ ] c\n");
        match_commits(&mut tasks, &[]);
        assert_eq!(tasks[1].state, TaskState::Building);
        assert_eq!(tasks[2].state, TaskState::Next);
    }

    #[test]
    fn the_status_line_names_the_task_being_built() {
        let tasks = from_checklist("- [x] a\n- [x] b\n- [ ] c\n");
        let mut tasks = tasks;
        match_commits(&mut tasks, &[]);
        assert_eq!(building(&tasks), Some((3, 3)));
        assert_eq!(building(&from_checklist("- [x] a\n- [x] b\n")), None);
        assert_eq!(
            building(&from_checklist("- [ ] only one")),
            None,
            "no tab, no task"
        );
    }

    #[test]
    fn only_a_headed_file_is_a_plan() {
        assert!(has_task_headings("### Task 1: A\n- [ ] x\n"));
        assert!(has_task_headings("## Task 2: B"));
        assert!(!has_task_headings("# Readme\n- [ ] item\n- [x] other\n"));
    }

    fn git(hash: &str, subject: &str, at: i64, files: &[&str]) -> GitCommit {
        GitCommit {
            hash: hash.into(),
            subject: subject.into(),
            at,
            files: files.iter().map(|f| f.to_string()).collect(),
        }
    }

    #[test]
    fn old_commits_mark_nothing() {
        let commits = [
            git("new1", "Task 1: A", 200, &[]),
            git("old9", "M11au Task 2: x", 50, &[]),
        ];
        assert_eq!(commits_since(&commits, 100), [commits[0].clone()]);
    }

    /// The log the tab reads: a record per commit, its paths under its header.
    #[test]
    fn a_log_with_names_parses() {
        let out = "\u{1e}9bf9010\t1795662889\tChore 16: the shells tab\n\nsrc/agent_box.rs\nsrc/agent_rows.rs\n\
                   \u{1e}7a2753f\t1795662544\tChore 16: the state\n\nsrc/main.rs\n";
        assert_eq!(
            parse_log(out),
            [
                git(
                    "9bf9010",
                    "Chore 16: the shells tab",
                    1795662889,
                    &["src/agent_box.rs", "src/agent_rows.rs"]
                ),
                git(
                    "7a2753f",
                    "Chore 16: the state",
                    1795662544,
                    &["src/main.rs"]
                ),
            ]
        );
    }

    /// Chore 20 end to end: the real log's records through `mark_done` tick all five.
    #[test]
    fn mark_done_reads_files_too() {
        let commits: Vec<GitCommit> = chore_16_commits()
            .into_iter()
            .map(|(hash, subject, files)| GitCommit {
                hash,
                subject,
                at: 0,
                files,
            })
            .collect();
        let mut tasks = from_checklist(CHORE_16_PLAN);
        mark_done(&mut tasks, CHORE_16_PLAN, &commits, None);
        assert_eq!(counts(&tasks), (5, 5));
    }

    #[test]
    fn a_finished_plan_stays_hidden_until_its_list_changes() {
        let finished = from_checklist("- [x] a\n- [x] b\n");
        let mut done = Some(finished.clone());
        assert!(after_done(&mut done, finished.clone()).is_empty());
        assert!(done.is_some(), "still held");
        let next = from_checklist("- [x] a\n- [x] b\n- [ ] c\n");
        assert_eq!(after_done(&mut done, next.clone()), next);
        assert!(done.is_none(), "forgotten once the list changed");
        let mut none = None;
        assert_eq!(after_done(&mut none, finished.clone()), finished);
    }

    #[test]
    fn no_list_no_burger() {
        assert!(!shows(&from_checklist("just prose, no boxes")));
        assert!(!shows(&from_checklist("- [ ] only one")));
        assert!(shows(&from_checklist("- [ ] one\n- [ ] two")));
    }

    #[test]
    fn all_done_counts_6_of_6() {
        let md = (1..=6)
            .map(|n| format!("- [x] step {n}\n"))
            .collect::<String>();
        assert_eq!(counts(&from_checklist(&md)), (6, 6));
    }

    /// Chore 36, chores 33–35's plan: Task 3 showed ◐ because its first check was ticked early,
    /// Task 2 ○ though its files were changing. A task is ◐ while uncommitted changes touch the
    /// files its `**Files:**` block names; some boxes ticked and none of its files changing is ○.
    const BUILDING_PLAN: &str = "\
# Chores Implementation Plan

### Task 1: The send button

**Files:** Modify `src/agent_tab.rs`, `src/ui_shots/agent_box.rs`.

- [x] Test first.
- [x] Implement.

### Task 2: Corner tabs

**Files:** Modify `src/agent_box.rs`, `src/corner_tabs.rs`, `src/agent_tab.rs`.

- [ ] Test first.
- [ ] Implement.

### Task 3: Local pages

**Files:** Modify `src/auto_open.rs`, `src/agent_tab.rs`.

- [x] First, the check.
- [ ] Implement.

### Task 4: Docs

**Files:** Modify `docs/plan.md`.

- [ ] Review.
";

    fn states(dirty: Option<&[&str]>) -> Vec<TaskState> {
        let dirty: Option<Vec<String>> = dirty.map(|d| d.iter().map(|p| p.to_string()).collect());
        let mut tasks = from_checklist(BUILDING_PLAN);
        mark_done(&mut tasks, BUILDING_PLAN, &[], dirty.as_deref());
        tasks.into_iter().map(|t| t.state).collect()
    }

    #[test]
    fn the_task_being_built_follows_its_files() {
        use TaskState::{Building, Next};
        let done = TaskState::Done { commit: None };
        assert_eq!(
            states(Some(&["src/corner_tabs.rs"])),
            [done.clone(), Building, Next, Next],
            "Task 2's files change"
        );
        assert_eq!(
            states(Some(&[])),
            [done.clone(), Building, Next, Next],
            "nothing changing: the one after the done one, not the early-ticked Task 3"
        );
        assert_eq!(
            states(Some(&["src/agent_tab.rs"])),
            [done.clone(), Building, Next, Next],
            "a file both name: the earlier"
        );
        assert_eq!(
            states(Some(&["src/auto_open.rs", "src/agent_box.rs"])),
            [done.clone(), Building, Building, Next],
            "each one's own files"
        );
        assert_eq!(
            states(Some(&["src/auto_open.rs"])),
            [done.clone(), Next, Building, Next]
        );
        assert_eq!(
            states(None),
            [done, Next, Building, Next],
            "no git: by its boxes, as before"
        );
    }

    /// The plan file itself changes while its boxes are ticked: no task is built by that.
    #[test]
    fn the_plan_file_itself_is_never_a_change() {
        let out = " M docs/plan.md\0?? src/new.rs\0R  src/b.rs\0src/a.rs\0";
        assert_eq!(
            plan_dirty(
                std::path::Path::new("/w"),
                std::path::Path::new("/w/docs/plan.md"),
                out
            ),
            ["src/new.rs", "src/b.rs"]
        );
    }
}
