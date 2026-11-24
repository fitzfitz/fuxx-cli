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
    let headed = md.lines().any(|l| task_heading(l).is_some());
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

/// Marks done the tasks a commit names (`Task 3:` in its subject), with that commit; then, if
/// nothing is being built, the first task not done is.
pub fn match_commits(tasks: &mut [PlanTask], commits: &[(String, String)]) {
    for (i, task) in tasks.iter_mut().enumerate() {
        let named = commits
            .iter()
            .find(|(_, subject)| names_task(subject, i + 1))
            .map(|(hash, _)| hash.clone());
        if let Some(hash) = named {
            task.state = TaskState::Done { commit: Some(hash) };
        }
    }
    if !tasks.iter().any(|t| t.state == TaskState::Building)
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
mod tests {
    use super::*;

    fn todo(text: &str, state: TodoState) -> TodoItem {
        TodoItem {
            text: text.into(),
            state,
        }
    }

    fn done(t: &PlanTask) -> bool {
        matches!(t.state, TaskState::Done { .. })
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
}
