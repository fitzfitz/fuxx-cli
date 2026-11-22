//! The Changes tab's commit line (M11aq, note 92; the Lead UI/UX Designer's spec): above the
//! branch bar while the repo has changes, a message field (a bare `NameInput`), ✦ to have an
//! agent write the message from the staged diff, and ✓N to commit the staged files (⌘↵ in the
//! field). A failed commit keeps the message and shows git's output under it on request; the
//! draft is kept per repo for the run.
//!
//! The pure parts (`commit_blocked`, `message_agent`, `CommitState`'s draft and answer rules) are
//! tested here; the git side is `git_commit`, the agent's run `agent_catalog::run_probe_stoppable`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use gpui::{
    AnyElement, AnyView, ClickEvent, Context, Entity, Focusable, FontWeight, SharedString,
    Subscription, Window, div, prelude::*, px,
};

use crate::git_branch::find_git;
use crate::icons::{Icon, icon};
use crate::name_input::{NameInput, NameInputEvent};
use crate::providers::Provider;
use crate::terminal_view::hsla;
use crate::theme::UiColors;
use crate::workspace::Workspace;

gpui::actions!(commit_line, [CommitStaged]);

/// How long ✦'s agent may take.
const WRITE_DEADLINE: std::time::Duration = std::time::Duration::from_secs(60);
/// The line's room: its height, its sides (as the bar's), the output block's tallest.
const LINE_H: f32 = 36.0;
const SIDE: f32 = 16.0;
const OUTPUT_MAX_H: f32 = 8.0 * 15.0;

/// What the line is doing for a repo: committing, or an agent writing its message (the flag
/// stops it).
#[derive(Clone, Debug)]
pub(crate) enum Busy {
    Committing,
    Writing(Arc<AtomicBool>),
}

/// The commit line's state, in the Changes panel.
#[derive(Default)]
pub(crate) struct CommitState {
    /// The field (made with the Workspace: it needs a window).
    pub(crate) input: Option<Entity<NameInput>>,
    /// Each repo's draft but the one the field shows (`shows`).
    pub(crate) drafts: HashMap<PathBuf, String>,
    pub(crate) shows: Option<PathBuf>,
    /// What runs, for which repo (one at a time per panel).
    pub(crate) busy: Option<(PathBuf, Busy)>,
    /// A failed commit's output, for its repo, and whether it is shown.
    pub(crate) error: Option<(PathBuf, String, bool)>,
    /// A note under the field for a repo ("Couldn't write a message").
    pub(crate) note: Option<(PathBuf, &'static str)>,
    /// The agents that can run here (on the login PATH), once looked up.
    pub(crate) agents: Option<Vec<&'static str>>,
    /// What stops a commit in a repo (a merge, a rebase …), as last read for it.
    pub(crate) in_progress: Option<(PathBuf, Option<&'static str>)>,
    /// The field's own subscriptions.
    pub(crate) subs: Vec<Subscription>,
    /// Where its parts were drawn last frame (the UI screenshots: `line`, `button`, `output`).
    #[cfg(feature = "ui-shots")]
    pub(crate) drawn:
        std::rc::Rc<std::cell::RefCell<HashMap<&'static str, gpui::Bounds<gpui::Pixels>>>>,
}

/// `el`, and under `ui-shots` where it was drawn kept as `name` (`CommitState::drawn`).
fn drawn<E: ParentElement>(state: &CommitState, name: &'static str, el: E) -> E {
    #[cfg(feature = "ui-shots")]
    {
        let drawn = state.drawn.clone();
        el.child(
            gpui::canvas(
                move |bounds, _, _| {
                    drawn.borrow_mut().insert(name, bounds);
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
        let _ = (state, name);
        el
    }
}

impl CommitState {
    /// What stops a commit in `root` now, as last read.
    pub(crate) fn in_progress_for(&self, root: &Path) -> Option<&'static str> {
        self.in_progress
            .as_ref()
            .filter(|(r, _)| r == root)
            .and_then(|(_, w)| *w)
    }

    /// What runs for `root`.
    fn busy_for(&self, root: &Path) -> Option<&Busy> {
        self.busy
            .as_ref()
            .filter(|(r, _)| r == root)
            .map(|(_, b)| b)
    }

    /// The field moves to `root`: the draft it held is kept for its repo, and `root`'s is
    /// returned to show (empty when it had none). None when it already shows `root`.
    pub(crate) fn swap_draft(&mut self, root: &Path, holding: &str) -> Option<String> {
        if self.shows.as_deref() == Some(root) {
            return None;
        }
        if let Some(old) = self.shows.take()
            && !holding.is_empty()
        {
            self.drafts.insert(old, holding.to_string());
        }
        self.shows = Some(root.to_path_buf());
        Some(self.drafts.remove(root).unwrap_or_default())
    }

    /// A message an agent wrote for `root`: shown when the field shows `root` (Some: put it in
    /// the field), else kept as that repo's draft. Dropped when the writing was stopped or
    /// another took its place (`flag` no longer the running one).
    pub(crate) fn wrote(
        &mut self,
        root: &Path,
        flag: &Arc<AtomicBool>,
        message: String,
    ) -> Option<String> {
        let running =
            matches!(&self.busy, Some((r, Busy::Writing(f))) if r == root && Arc::ptr_eq(f, flag));
        if !running {
            return None;
        }
        self.busy = None;
        if self.shows.as_deref() == Some(root) {
            Some(message)
        } else {
            self.drafts.insert(root.to_path_buf(), message);
            None
        }
    }

    /// A commit for `root` ended: done (Some(true): clear the field, which shows it; its draft
    /// cleared otherwise) or failed (its output kept for `root`).
    pub(crate) fn committed(&mut self, root: &Path, result: Result<(), String>) -> Option<bool> {
        if matches!(&self.busy, Some((r, Busy::Committing)) if r == root) {
            self.busy = None;
        }
        match result {
            Ok(()) => {
                self.error = None;
                if self.shows.as_deref() == Some(root) {
                    Some(true)
                } else {
                    self.drafts.remove(root);
                    None
                }
            }
            Err(output) => {
                self.error = Some((root.to_path_buf(), output, false));
                None
            }
        }
    }
}

/// Why the staged files can't be committed now (✓'s tooltip, and ⌘↵ does nothing); none: they
/// can.
pub fn commit_blocked(staged: usize, message: &str, in_progress: Option<&str>) -> Option<String> {
    if let Some(what) = in_progress {
        return Some(format!("Finish {what} first"));
    }
    if staged == 0 {
        return Some("Stage files to commit".into());
    }
    if message.trim().is_empty() {
        return Some("Write a message".into());
    }
    None
}

/// The agent that writes the message (✦): the followed agent tab's, if it can run; else Claude,
/// else Antigravity; none when none can.
pub fn message_agent(
    followed: Option<&str>,
    can_run: impl Fn(&str) -> bool,
) -> Option<&'static dyn Provider> {
    let writes = |name: &str| {
        crate::providers::all()
            .iter()
            .copied()
            .find(|p| p.info().name == name)
            .filter(|p| p.message_probe("").is_some() && can_run(p.info().name))
    };
    followed
        .and_then(writes)
        .or_else(|| writes("claude"))
        .or_else(|| writes("agy"))
}

impl Workspace {
    /// Before the Changes panel draws (with a window): the field made once (and the agents that
    /// can write looked up once), then moved to the panel's repo, its draft swapped in.
    pub(crate) fn prepare_commit_line(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.right_sidebar.changes.commit.input.is_none() {
            let input = cx.new(|cx| {
                let mut input = NameInput::new(String::new(), window, cx);
                input.bare();
                input
            });
            // Esc (the field still focused; a blur also says Cancel): leave the field, and stop
            // an agent writing.
            let keys = cx.subscribe_in(&input, window, |this, input, event, window, cx| {
                if matches!(event, NameInputEvent::Cancel)
                    && input.focus_handle(cx).is_focused(window)
                {
                    window.blur();
                    this.stop_writing(cx);
                }
            });
            // Typing redraws the line (✓ dims and lights with the message).
            let typed = cx.observe(&input, |_, _, cx| cx.notify());
            let commit = &mut self.right_sidebar.changes.commit;
            commit.input = Some(input);
            commit.subs.extend([keys, typed]);
            self.look_up_message_agents(cx);
        }
        let changes = &self.right_sidebar.changes;
        let (Some(root), Some(input)) = (changes.repo_root(), changes.commit.input.clone()) else {
            return;
        };
        let holding = input.read(cx).text().to_string();
        if let Some(text) = self
            .right_sidebar
            .changes
            .commit
            .swap_draft(&root, &holding)
        {
            input.update(cx, |input, _| input.set_text(text));
            self.check_in_progress(root, cx);
        }
    }

    /// Which agents can write a message here (on the login PATH), off the UI thread.
    fn look_up_message_agents(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let found = cx
                .background_executor()
                .spawn(async move {
                    let path = crate::agent_catalog::login_path();
                    crate::providers::all()
                        .iter()
                        .filter(|p| p.message_probe("").is_some())
                        .map(|p| p.info().name)
                        .filter(|name| crate::providers::on_path(name, path.as_deref()))
                        .collect::<Vec<_>>()
                })
                .await;
            this.update(cx, |this, cx| {
                this.right_sidebar.changes.commit.agents = Some(found);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Read what stops a commit in `root` (a merge, a rebase …), off the UI thread.
    fn check_in_progress(&mut self, root: PathBuf, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let at = root.clone();
            let what = cx
                .background_executor()
                .spawn(async move {
                    find_git().and_then(|git| crate::git_commit::in_progress(&git, &at))
                })
                .await;
            this.update(cx, |this, cx| {
                this.right_sidebar.changes.commit.in_progress = Some((root, what));
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// ⌘↵ or ✓: commit the staged files with the field's message, off the UI thread (unless
    /// something stops it: `commit_blocked`). Done: the field clears and the list and the bar
    /// read again (↑N lights Push); failed: git's output kept, the message too.
    pub(crate) fn commit_now(&mut self, cx: &mut Context<Self>) {
        let changes = &mut self.right_sidebar.changes;
        let (Some(root), Some(input)) = (changes.repo_root(), changes.commit.input.clone()) else {
            return;
        };
        if changes.commit.busy.is_some() {
            return;
        }
        let message = input.read(cx).text().trim().to_string();
        let staged = changes.staged_count();
        let stops = changes.commit.in_progress_for(&root);
        if commit_blocked(staged, &message, stops).is_some() {
            return;
        }
        let commit = &mut changes.commit;
        commit.busy = Some((root.clone(), Busy::Committing));
        commit.error = None;
        commit.note = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let at = root.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    match find_git() {
                        Some(git) => crate::git_commit::commit(&git, &at, &message),
                        None => Err("git not found".to_string()),
                    }
                })
                .await;
            this.update(cx, |this, cx| {
                let commit = &mut this.right_sidebar.changes.commit;
                if commit.committed(&root, result) == Some(true)
                    && let Some(input) = commit.input.clone()
                {
                    input.update(cx, |input, _| input.set_text(String::new()));
                }
                this.check_in_progress(root, cx);
                this.right_sidebar.changes.refreshed = None;
                this.refresh_changes(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The agent tab the right panel follows: its agent's name (✦ asks it first).
    fn followed_agent_name(&self, cx: &Context<Self>) -> Option<&'static str> {
        let tab = self.right_sidebar.followed_agent?;
        let slot = self.agents.get(&tab)?;
        slot.view.read(cx).provider.map(|p| p.info().name)
    }

    /// ✦: an agent writes the message from the staged diff (secret-looking files left out),
    /// off the UI thread; it lands in the field selected, never committed. Esc stops it.
    pub(crate) fn write_message(&mut self, cx: &mut Context<Self>) {
        let followed = self.followed_agent_name(cx);
        let changes = &mut self.right_sidebar.changes;
        let Some(root) = changes.repo_root() else {
            return;
        };
        if changes.commit.busy.is_some() {
            return;
        }
        let agents = changes.commit.agents.clone().unwrap_or_default();
        let Some(provider) = message_agent(followed, |name| agents.contains(&name)) else {
            return;
        };
        let flag = Arc::new(AtomicBool::new(false));
        changes.commit.busy = Some((root.clone(), Busy::Writing(flag.clone())));
        changes.commit.note = None;
        cx.notify();
        let stop = flag.clone();
        cx.spawn(async move |this, cx| {
            let at = root.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    let git = find_git().ok_or("git not found")?;
                    let diff =
                        crate::git_commit::staged_diff(&git, &at, crate::git_commit::DIFF_CAP)?;
                    if diff.text.is_empty() {
                        return Err("nothing staged to describe".to_string());
                    }
                    let prompt = crate::providers::commit_message_prompt(&diff);
                    let probe = provider
                        .message_probe(&prompt)
                        .ok_or("this agent can't write one")?;
                    let path = crate::agent_catalog::login_path();
                    let said = crate::agent_catalog::run_probe_stoppable(
                        &probe,
                        &at,
                        path.as_deref(),
                        WRITE_DEADLINE,
                        &stop,
                    )?;
                    crate::providers::clean_message(&said).ok_or("no message".to_string())
                })
                .await;
            this.update(cx, |this, cx| {
                let commit = &mut this.right_sidebar.changes.commit;
                match result {
                    Ok(message) => {
                        if let Some(text) = commit.wrote(&root, &flag, message)
                            && let Some(input) = commit.input.clone()
                        {
                            input.update(cx, |input, cx| {
                                input.set_text(text);
                                input.select_all();
                                cx.notify();
                            });
                        }
                    }
                    Err(_) => {
                        // Not when it was stopped (Esc): that said nothing.
                        let running = matches!(&commit.busy, Some((r, Busy::Writing(f))) if r == &root && Arc::ptr_eq(f, &flag));
                        if running {
                            commit.busy = None;
                            commit.note = Some((root, "Couldn't write a message"));
                        }
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Esc while an agent writes: it is stopped (its process killed), its answer dropped.
    pub(crate) fn stop_writing(&mut self, cx: &mut Context<Self>) {
        let commit = &mut self.right_sidebar.changes.commit;
        if let Some((_, Busy::Writing(flag))) = &commit.busy {
            flag.store(true, std::sync::atomic::Ordering::Relaxed);
            commit.busy = None;
            cx.notify();
        }
    }

    /// The line above the branch bar, while the repo has changes (or a commit, a write or a
    /// failure is its own): the field, ✦ when an agent can write, ✓N; under it a failure's line
    /// (its output on request) or ✦'s note.
    pub(crate) fn render_commit_line(
        &self,
        ui: UiColors,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let changes = &self.right_sidebar.changes;
        let root = changes.repo_root()?;
        let commit = &changes.commit;
        let input = commit.input.clone()?;
        let busy = commit.busy_for(&root).cloned();
        let error = commit.error.clone().filter(|(r, _, _)| r == &root);
        let note = commit
            .note
            .as_ref()
            .filter(|(r, _)| r == &root)
            .map(|(_, n)| *n);
        if !changes.has_changes() && busy.is_none() && error.is_none() {
            return None;
        }
        let message = input.read(cx).text().to_string();
        let staged = changes.staged_count();
        let blocked = match &busy {
            Some(Busy::Committing) => Some("Committing…".to_string()),
            Some(Busy::Writing(_)) => Some("Writing the message…".to_string()),
            None => commit_blocked(staged, &message, commit.in_progress_for(&root)),
        };
        let placeholder = match &busy {
            Some(Busy::Committing) => "Committing…",
            Some(Busy::Writing(_)) => "Writing…",
            None => "Message (⌘↵ to commit)",
        };
        let agents = commit.agents.clone().unwrap_or_default();
        let writer = message_agent(self.followed_agent_name(cx), |name| agents.contains(&name));
        let tip = |text: String| {
            move |_: &mut Window, cx: &mut gpui::App| -> AnyView {
                cx.new(|_| crate::sidebar_toggle::Tip(text.clone().into()))
                    .into()
            }
        };
        let field = drawn(
            commit,
            "field",
            div()
                .id("commit-field")
                .key_context("CommitInput")
                .on_action(cx.listener(|this, _: &CommitStaged, _, cx| this.commit_now(cx)))
                .relative()
                .flex_1()
                .min_w(px(0.0))
                .h(px(LINE_H))
                .flex()
                .items_center()
                .text_size(px(crate::changes_panel::GIT_NAME))
                // The bare field takes its ink from here (it sets none itself).
                .text_color(hsla(ui.text))
                .cursor_text()
                .when(message.is_empty(), |d| {
                    d.child(
                        div()
                            .absolute()
                            .left_0()
                            .text_color(hsla(ui.text_faint))
                            .child(placeholder),
                    )
                })
                .child(input),
        );
        let write = writer.map(|provider| {
            let label = provider.info().label;
            let writing = matches!(busy, Some(Busy::Writing(_)));
            let enabled = busy.is_none() && staged > 0;
            drawn(
                commit,
                "write",
                div()
                    .id("commit-write")
                    .flex_none()
                    .size(px(crate::right_sidebar::HEADER_BUTTON))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.0))
                    .text_size(px(14.0))
                    .text_color(hsla(if writing { ui.accent } else { ui.text_dim }))
                    .child("✦")
                    .tooltip(tip(if staged == 0 {
                        "Stage files for the agent to describe".to_string()
                    } else {
                        format!("Write the message with {label}")
                    }))
                    .when(!enabled && !writing, |d| d.opacity(0.4))
                    .when(enabled, |d| {
                        d.cursor_pointer()
                            .hover(move |s| s.bg(hsla(ui.side_tab_hover)))
                            .on_click(
                                cx.listener(|this, _: &ClickEvent, _, cx| this.write_message(cx)),
                            )
                    }),
            )
        });
        let can = blocked.is_none();
        let button = drawn(
            commit,
            "button",
            div()
                .id("commit-button")
                .flex_none()
                .h(px(crate::right_sidebar::HEADER_BUTTON))
                .px(px(6.0))
                .flex()
                .items_center()
                .gap(px(4.0))
                .rounded(px(6.0))
                .text_size(px(crate::changes_panel::GIT_NAME))
                .font_weight(FontWeight::MEDIUM)
                .text_color(hsla(ui.text_dim))
                .child(icon(
                    Icon::Check,
                    px(crate::right_sidebar::HEADER_ICON),
                    hsla(ui.text_dim),
                ))
                .child(SharedString::from(staged.to_string()))
                .tooltip(tip(blocked.clone().unwrap_or_else(|| "Commit (⌘↵)".into())))
                .when(!can, |d| d.opacity(0.4))
                .when(can, |d| {
                    d.cursor_pointer()
                        .hover(move |s| s.bg(hsla(ui.side_tab_hover)).text_color(hsla(ui.text)))
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.commit_now(cx)))
                }),
        );
        let failure = error.map(|(_, output, shown)| {
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .pb(px(8.0))
                .text_size(px(crate::changes_panel::GIT_SMALL))
                .child(
                    div()
                        .id("commit-failed")
                        .flex()
                        .gap(px(6.0))
                        .text_color(hsla(ui.ring_waiting))
                        .child("Commit failed")
                        .child(div().text_color(hsla(ui.text_dim)).child("·"))
                        .child(
                            div()
                                .id("commit-output-toggle")
                                .cursor_pointer()
                                .text_color(hsla(ui.text_dim))
                                .hover(move |s| s.text_color(hsla(ui.text)))
                                .child(if shown { "Hide output" } else { "Show output" })
                                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                    if let Some((_, _, shown)) =
                                        &mut this.right_sidebar.changes.commit.error
                                    {
                                        *shown = !*shown;
                                    }
                                    cx.notify();
                                })),
                        ),
                )
                .when(shown, |d| {
                    d.child(drawn(
                        commit,
                        "output",
                        div()
                            .id("commit-output")
                            .max_h(px(OUTPUT_MAX_H))
                            .overflow_y_scroll()
                            .px(px(8.0))
                            .py(px(6.0))
                            .rounded(px(6.0))
                            .bg(hsla(ui.side_field))
                            .font_family(
                                cx.global::<crate::appearance::Appearance>()
                                    .font
                                    .family
                                    .clone(),
                            )
                            .text_color(hsla(ui.text_dim))
                            .child(SharedString::from(output)),
                    ))
                })
        });
        let quiet = note.map(|note| {
            drawn(
                commit,
                "note",
                div()
                    .pb(px(8.0))
                    .text_size(px(crate::changes_panel::GIT_SMALL))
                    .text_color(hsla(ui.text_faint))
                    .child(note),
            )
        });
        Some(
            drawn(
                commit,
                "line",
                div()
                    .id("commit-line")
                    .flex_none()
                    .w_full()
                    .px(px(SIDE))
                    .border_t_1()
                    .border_color(hsla(ui.card_border))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(2.0))
                            .child(field)
                            .children(write)
                            .child(button),
                    )
                    .children(failure)
                    .children(quiet),
            )
            .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_commit_says_what_stops_it() {
        assert_eq!(
            commit_blocked(2, "Fix it", Some("a merge")).as_deref(),
            Some("Finish a merge first")
        );
        assert_eq!(
            commit_blocked(0, "Fix it", None).as_deref(),
            Some("Stage files to commit")
        );
        assert_eq!(
            commit_blocked(2, "   ", None).as_deref(),
            Some("Write a message")
        );
        assert_eq!(commit_blocked(2, "Fix it", None), None, "it can");
    }

    #[test]
    fn each_repo_keeps_its_own_draft() {
        let mut s = CommitState::default();
        let (a, b) = (Path::new("/a"), Path::new("/b"));
        assert_eq!(
            s.swap_draft(a, "").as_deref(),
            Some(""),
            "a fresh repo: empty"
        );
        assert_eq!(s.swap_draft(a, "half"), None, "already showing a");
        assert_eq!(s.swap_draft(b, "half").as_deref(), Some(""), "b: empty");
        assert_eq!(
            s.swap_draft(a, "b's").as_deref(),
            Some("half"),
            "back: a's draft"
        );
        assert_eq!(s.drafts.get(b).map(String::as_str), Some("b's"), "b's kept");
    }

    #[test]
    fn an_answer_lands_on_its_own_repo() {
        let mut s = CommitState::default();
        let (a, b) = (Path::new("/a"), Path::new("/b"));
        s.swap_draft(a, "");
        // Writing for a, then the panel moves to b: a's message waits as a's draft.
        let flag = Arc::new(AtomicBool::new(false));
        s.busy = Some((a.to_path_buf(), Busy::Writing(flag.clone())));
        s.swap_draft(b, "");
        assert_eq!(
            s.wrote(a, &flag, "Fix a".into()),
            None,
            "not into b's field"
        );
        assert_eq!(s.drafts.get(a).map(String::as_str), Some("Fix a"));
        // Stopped (Esc): a late answer is dropped.
        let late = Arc::new(AtomicBool::new(true));
        assert_eq!(s.wrote(b, &late, "late".into()), None);
        assert!(!s.drafts.contains_key(b), "dropped");
        // A commit for a that failed while b showed: its output is kept for a.
        s.busy = Some((a.to_path_buf(), Busy::Committing));
        assert_eq!(s.committed(a, Err("nope".into())), None);
        assert!(matches!(&s.error, Some((r, o, false)) if r == a && o == "nope"));
        assert!(s.busy.is_none());
    }

    #[test]
    fn the_followed_agent_writes_else_claude_else_antigravity() {
        let name = |p: Option<&'static dyn Provider>| p.map(|p| p.info().name);
        let both = |_: &str| true;
        assert_eq!(name(message_agent(Some("agy"), both)), Some("agy"));
        assert_eq!(name(message_agent(None, both)), Some("claude"));
        assert_eq!(
            name(message_agent(Some("agy"), |n: &str| n == "claude")),
            Some("claude"),
            "the followed one can't run"
        );
        assert_eq!(name(message_agent(None, |n: &str| n == "agy")), Some("agy"));
        assert_eq!(name(message_agent(None, |_: &str| false)), None, "none");
    }
}
