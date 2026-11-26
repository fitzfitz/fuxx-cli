//! The status line's popover (chore 22; moved out of `agent_box_menus.rs` and drawn in the
//! corner popovers' look by chore 36): `⧉ N changed` opens the files the turn touched, under
//! their folders (a click opens one's diff tab); `✓ checks` its checks, a row per scope.

use gpui::{
    AnyElement, AppContext, Context, FontWeight, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled, div, prelude::FluentBuilder, px,
};

use super::popover::{self, Mark};
use super::{AgentTab, AgentTabEvent, Colors};
use crate::agent_box::menus::MenuPlace;
use crate::agent_ledger::Scope;
use crate::agent_status_line::ChipKind;
use crate::appearance::Appearance;
use crate::changed_files::{FileStatus, Group, How};
use crate::terminal_view::hsla;

/// The list, at most this tall (it scrolls past it).
const LIST_MAX: f32 = 360.0;

impl AgentTab {
    /// The popover: the part its chip asked for (chore 22), `⧉ N changed` the files, `✓ checks`
    /// the checks; both when neither did. Empty parts are left out.
    pub(super) fn ledger_popover(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut content: Vec<AnyElement> = Vec::new();
        if self.ledger_part != Some(ChipKind::Checks) {
            self.files_part(&mut content, cx);
        }
        if self.ledger_part != Some(ChipKind::Changed) {
            self.checks_part(&mut content, cx);
        }
        let list = div()
            .id("agent-ledger-list")
            .flex()
            .flex_col()
            .max_h(px(LIST_MAX))
            .overflow_y_scroll()
            .children(content)
            .into_any_element();
        self.popover_frame(
            "ledger-popover",
            vec![list],
            crate::agent_box::LEDGER_MENU_WIDTH,
            MenuPlace::Right(0.),
            cx,
        )
    }

    /// The files: "Changed files" with how many and their +/−, then each folder once (it folds)
    /// and its files, those changed, scripted or read together (chore 36), an A/M/D mark and
    /// +/− on each. Paths a script only named, and folders, never show (`changed_files`).
    fn files_part(&self, out: &mut Vec<AnyElement>, cx: &mut Context<Self>) {
        use crate::changed_files::{files_list, total};
        let ui = cx.global::<Appearance>().theme.ui();
        let c = Colors::of(&cx.global::<Appearance>().theme);
        let Some(ledger) = self.ledger.as_ref() else {
            return;
        };
        let list = files_list(ledger, &self.ledger_facts, &self.project_roots());
        let (files, plus, minus) = total(&list.inside);
        if !list.inside.is_empty() {
            let count = div()
                .flex()
                .gap(px(6.))
                .child(SharedString::from(files.to_string()))
                .when(plus > 0, |d| {
                    d.child(
                        div()
                            .text_color(hsla(c.tool_ok))
                            .child(SharedString::from(format!("+{plus}"))),
                    )
                })
                .when(minus > 0, |d| {
                    d.child(
                        div()
                            .text_color(hsla(c.failed))
                            .child(SharedString::from(format!("\u{2212}{minus}"))),
                    )
                })
                .into_any_element();
            out.push(popover::header("Changed files", Some(count), &ui).into_any_element());
        }
        let mut n = 0;
        self.file_groups(false, list.inside, &mut n, out, cx);
        self.file_groups(true, list.outside, &mut n, out, cx);
    }

    /// `groups`: each folder's row (a click folds it), then its files; those `outside` the
    /// project under "Outside the project", dimmed (chore 36).
    fn file_groups(
        &self,
        outside: bool,
        groups: Vec<Group>,
        n: &mut usize,
        out: &mut Vec<AnyElement>,
        cx: &mut Context<Self>,
    ) {
        if groups.is_empty() {
            return;
        }
        let id = if outside {
            "ledger-outside"
        } else {
            "ledger-files"
        };
        let ui = cx.global::<Appearance>().theme.ui();
        let mono = cx.global::<Appearance>().font.family.clone();
        if outside {
            out.push(popover::header("Outside the project", None, &ui).into_any_element());
        }
        for group in groups {
            let folded = self.ledger_folded.contains(&group.folder);
            if !group.folder.is_empty() {
                let key = group.folder.clone();
                let hover = ui.panel_hover;
                *n += 1;
                let chevron = if folded {
                    crate::icons::Icon::ChevronRight
                } else {
                    crate::icons::Icon::ChevronDown
                };
                let folder = popover::row()
                    .id((id, 10_000 + *n))
                    .cursor_pointer()
                    .hover(move |s| s.bg(hsla(hover)))
                    .child(popover::mark_cell(crate::icons::icon(
                        chevron,
                        px(12.),
                        hsla(ui.text_dim),
                    )))
                    // A long folder (one outside) keeps its end.
                    .child(
                        popover::mono(group.folder.clone(), ui.text_dim, mono.clone())
                            .text_ellipsis_start(),
                    )
                    .child(popover::tail(group.rows.len().to_string(), ui.text_faint))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !this.ledger_folded.remove(&key) {
                            this.ledger_folded.insert(key.clone());
                        }
                        cx.notify();
                    }));
                // Probed as `folder:<its folder>` (the scenes read the rows in order).
                out.push(
                    self.marked_row(format!("folder:{}", group.folder), folder)
                        .into_any_element(),
                );
            }
            if folded {
                continue;
            }
            let nested = !group.folder.is_empty();
            for file in group.rows {
                *n += 1;
                out.push(self.file_row(file, nested, outside, (id, *n), cx));
            }
        }
    }

    /// One file's row: its A/M/D mark, its name and its +/− (one a script touched says
    /// "script", one only read "read", dim); a click opens its diff tab (⌥: the file; a file
    /// only read has no diff, so the file). One `outside` the project is dim and opens the file.
    fn file_row(
        &self,
        file: crate::changed_files::FileRow,
        nested: bool,
        outside: bool,
        id: (&'static str, usize),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ui = cx.global::<Appearance>().theme.ui();
        let c = Colors::of(&cx.global::<Appearance>().theme);
        let mono = cx.global::<Appearance>().font.family.clone();
        let (letter, ink) = match file.status {
            FileStatus::Added => ("A", c.tool_ok),
            FileStatus::Modified => ("M", c.meter_caution),
            FileStatus::Deleted => ("D", c.failed),
            FileStatus::Unchanged => ("", ui.text_dim),
        };
        let unchanged = outside || file.status == FileStatus::Unchanged;
        // Outside the project: every part in the dim ink.
        let ink = if outside { ui.text_dim } else { ink };
        let (plus_ink, minus_ink) = if outside {
            (ui.text_dim, ui.text_dim)
        } else {
            (c.tool_ok, c.failed)
        };
        let counts = div()
            .flex_none()
            .flex()
            .gap(px(6.))
            .when_some(file.plus.filter(|p| *p > 0), |d, p| {
                d.child(popover::tail(format!("+{p}"), plus_ink))
            })
            .when_some(file.minus.filter(|m| *m > 0), |d, m| {
                d.child(popover::tail(format!("\u{2212}{m}"), minus_ink))
            })
            .map(|d| match file.how {
                How::Changed => d,
                How::Script => d.child(popover::tail("script", ui.text_dim)),
                How::Read => d.child(popover::tail("read", ui.text_dim)),
            });
        let name_ink = if outside || file.how == How::Read {
            ui.text_dim
        } else {
            ui.text
        };
        // Probed by its path from the project (the scenes read the popover's rows).
        let label = self
            .project_roots()
            .iter()
            .find_map(|r| file.path.strip_prefix(r).ok())
            .unwrap_or(&file.path)
            .to_string_lossy()
            .into_owned();
        let (path, hover) = (file.path.clone(), ui.panel_hover);
        let row = popover::row()
            .id(id)
            .when(nested, |d| d.pl(px(popover::PAD_X + popover::INDENT)))
            .cursor_pointer()
            .hover(move |s| s.bg(hsla(hover)))
            .child(popover::mark_cell(
                div()
                    .text_size(px(11.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(hsla(ink))
                    .child(letter),
            ))
            .child(popover::mono(file.name, name_ink, mono))
            .child(counts)
            .tooltip(move |_, cx| {
                cx.new(|_| {
                    crate::sidebar_toggle::Tip(
                        if unchanged {
                            "Open the file"
                        } else {
                            "Open the diff (\u{2325}-click: the file)"
                        }
                        .into(),
                    )
                })
                .into()
            })
            .on_click(cx.listener(move |this, event: &gpui::ClickEvent, _, cx| {
                this.close_menu(cx);
                if unchanged || event.modifiers().alt {
                    cx.emit(AgentTabEvent::OpenFile(path.clone()));
                } else {
                    cx.emit(AgentTabEvent::OpenDiff(path.clone()));
                }
            }));
        self.marked_row(label, row).into_any_element()
    }

    /// The checks: "Checks" with "P/T passed" (scopes, as the rows), then a row per scope: its
    /// latest run's mark, its command and how that run went (red only while it still fails for
    /// real).
    fn checks_part(&self, out: &mut Vec<AnyElement>, cx: &mut Context<Self>) {
        let ui = cx.global::<Appearance>().theme.ui();
        let c = Colors::of(&cx.global::<Appearance>().theme);
        let mono = cx.global::<Appearance>().font.family.clone();
        let Some(ledger) = self.ledger.as_ref() else {
            return;
        };
        if ledger.scopes.is_empty() {
            return;
        }
        let count = format!("{}/{} passed", ledger.passed, ledger.total);
        out.push(popover::header("Checks", Some(popover::count(count)), &ui).into_any_element());
        for (n, scope) in ledger.scopes.iter().enumerate() {
            let (mark, words) = check_row(scope);
            let outcome_ink = if scope.failing() {
                c.bar_failed
            } else {
                ui.text_dim
            };
            // Probed by its command (the scenes read the popover's rows).
            let row = popover::row()
                .id(("ledger-check", n))
                .child(popover::mark_cell(popover::mark(mark, &c, &ui)))
                .child(popover::mono(scope.scope.clone(), ui.text, mono.clone()))
                .child(popover::tail(words, outcome_ink));
            out.push(self.marked_row(scope.scope.clone(), row).into_any_element());
        }
    }
}

/// A checks row (chore 36): its latest run's mark and words — ✓ "2 passed", ✕ "1 of 3 failed"
/// (dim for a planned failure, never red), – "ran" when nothing says how it went.
fn check_row(scope: &Scope) -> (Mark, String) {
    use crate::agent_activity::{Activity, Check, Outcome};
    let Some((latest, planned)) = scope.runs.last() else {
        return (Mark::NoVerdict, "ran".into());
    };
    let a = Activity {
        check: Some(Check {
            what: scope.what,
            scope: scope.scope.clone(),
            outcome: latest.clone(),
        }),
        planned_failure: *planned,
        ..Default::default()
    };
    let words = crate::agent_step_words::outcome_phrase(&a).unwrap_or_else(|| "ran".into());
    let mark = match latest {
        Outcome::Passed { .. } => Mark::Done,
        Outcome::Failed { .. } if *planned => Mark::Stopped,
        Outcome::Failed { .. } => Mark::Failed,
        Outcome::Unknown => Mark::NoVerdict,
    };
    (mark, words)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_activity::{CheckWhat, Outcome};

    fn scope(runs: &[(Outcome, bool)]) -> Scope {
        Scope {
            what: CheckWhat::Tests,
            scope: "cargo test".into(),
            runs: runs.to_vec(),
        }
    }

    /// Chore 36, your picture's rows: one mark per row, the latest run's; no "ran" beside an
    /// empty circle.
    #[test]
    fn a_checks_row_shows_its_latest_run() {
        let failed = Outcome::Failed {
            failed: Some(1),
            of: Some(3),
            first_error: None,
        };
        let passed = Outcome::Passed { n: Some(1) };
        assert_eq!(
            check_row(&scope(&[(Outcome::Unknown, false)])),
            (Mark::NoVerdict, "ran".to_string())
        );
        assert_eq!(
            check_row(&scope(&[
                (Outcome::Unknown, false),
                (Outcome::Unknown, false)
            ])),
            (Mark::NoVerdict, "ran".to_string())
        );
        assert_eq!(
            check_row(&scope(&[(failed.clone(), false), (passed, false)])),
            (Mark::Done, "1 passed".to_string())
        );
        assert_eq!(
            check_row(&scope(&[(failed.clone(), false)])),
            (Mark::Failed, "1 of 3 failed".to_string())
        );
        let (mark, words) = check_row(&scope(&[(failed, true)]));
        assert_eq!(mark, Mark::Stopped, "a planned failure: a dim ✕, never red");
        assert_eq!(words, crate::agent_step_words::PLANNED);
    }
}
