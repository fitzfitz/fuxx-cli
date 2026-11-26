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
use crate::agent_status_line::ChipKind;
use crate::appearance::Appearance;
use crate::changed_files::{FileStatus, Group};
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

    /// The files: "Changed files" with how many and their +/−, then each folder (it folds) and
    /// its files, an A/M/D mark and +/− on each; those scripts touched and those only read
    /// after them. Paths a script only named, and folders, never show (`changed_files`).
    fn files_part(&self, out: &mut Vec<AnyElement>, cx: &mut Context<Self>) {
        use crate::changed_files::{changed_groups, kept_paths, plain_groups, total};
        let ui = cx.global::<Appearance>().theme.ui();
        let c = Colors::of(&cx.global::<Appearance>().theme);
        let Some(ledger) = self.ledger.as_ref() else {
            return;
        };
        let facts = &self.ledger_facts;
        let cwd = &self.start.cwd;
        let groups = changed_groups(&ledger.changed, facts, cwd);
        if !groups.is_empty() {
            let (files, plus, minus) = total(&groups);
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
        self.file_groups(None, groups, "ledger-changed", &mut n, out, cx);
        self.file_groups(
            Some("Touched by scripts"),
            plain_groups(&kept_paths(&ledger.scripted, facts), facts, cwd),
            "ledger-scripted",
            &mut n,
            out,
            cx,
        );
        self.file_groups(
            Some("Read"),
            plain_groups(&kept_paths(&ledger.read, facts), facts, cwd),
            "ledger-read",
            &mut n,
            out,
            cx,
        );
    }

    /// `groups` under `title`: each folder's row (a click folds it), then its files.
    fn file_groups(
        &self,
        title: Option<&str>,
        groups: Vec<Group>,
        id: &'static str,
        n: &mut usize,
        out: &mut Vec<AnyElement>,
        cx: &mut Context<Self>,
    ) {
        if groups.is_empty() {
            return;
        }
        let ui = cx.global::<Appearance>().theme.ui();
        let mono = cx.global::<Appearance>().font.family.clone();
        if let Some(title) = title {
            out.push(popover::header(title, None, &ui).into_any_element());
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
                out.push(
                    popover::row()
                        .id((id, 10_000 + *n))
                        .cursor_pointer()
                        .hover(move |s| s.bg(hsla(hover)))
                        .child(popover::mark_cell(crate::icons::icon(
                            chevron,
                            px(12.),
                            hsla(ui.text_dim),
                        )))
                        .child(popover::mono(
                            group.folder.clone(),
                            ui.text_dim,
                            mono.clone(),
                        ))
                        .child(popover::tail(group.rows.len().to_string(), ui.text_faint))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if !this.ledger_folded.remove(&key) {
                                this.ledger_folded.insert(key.clone());
                            }
                            cx.notify();
                        }))
                        .into_any_element(),
                );
            }
            if folded {
                continue;
            }
            let nested = !group.folder.is_empty();
            for file in group.rows {
                *n += 1;
                out.push(self.file_row(file, nested, (id, *n), cx));
            }
        }
    }

    /// One file's row: its A/M/D mark, its name and its +/−; a click opens its diff tab (⌥: the
    /// file; a file only read has no diff, so the file).
    fn file_row(
        &self,
        file: crate::changed_files::FileRow,
        nested: bool,
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
        let unchanged = file.status == FileStatus::Unchanged;
        let counts = div()
            .flex_none()
            .flex()
            .gap(px(6.))
            .when_some(file.plus.filter(|p| *p > 0), |d, p| {
                d.child(popover::tail(format!("+{p}"), c.tool_ok))
            })
            .when_some(file.minus.filter(|m| *m > 0), |d, m| {
                d.child(popover::tail(format!("\u{2212}{m}"), c.failed))
            });
        let label = file
            .path
            .strip_prefix(&self.start.cwd)
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
            .child(popover::mono(file.name, ui.text, mono))
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

    /// The checks: "Checks" with "P/T passed", then a row per scope: its command, its runs'
    /// marks and its latest outcome (red only while that run still fails for real).
    fn checks_part(&self, out: &mut Vec<AnyElement>, cx: &mut Context<Self>) {
        use crate::agent_activity::{Activity, Check};
        let ui = cx.global::<Appearance>().theme.ui();
        let c = Colors::of(&cx.global::<Appearance>().theme);
        let mono = cx.global::<Appearance>().font.family.clone();
        let Some(ledger) = self.ledger.as_ref() else {
            return;
        };
        if ledger.scopes.is_empty() {
            return;
        }
        out.push(
            popover::header(
                "Checks",
                Some(popover::count(format!(
                    "{}/{} passed",
                    ledger.passed, ledger.total
                ))),
                &ui,
            )
            .into_any_element(),
        );
        for (n, scope) in ledger.scopes.iter().enumerate() {
            let Some((latest, planned)) = scope.runs.last() else {
                continue;
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
            let strip = scope.runs.iter().map(|(outcome, planned)| {
                use crate::agent_steps::{Glyph, check_glyph};
                let mark = match check_glyph(outcome, *planned) {
                    Glyph::Passed => Mark::Done,
                    Glyph::Failed | Glyph::Planned => Mark::Failed,
                    _ => Mark::NoVerdict,
                };
                popover::mark(mark, &c, &ui)
            });
            let outcome_ink = if scope.failing() {
                c.bar_failed
            } else {
                ui.text_dim
            };
            // Probed by its command (the scenes read the popover's rows).
            let row = popover::row()
                .id(("ledger-check", n))
                .child(popover::mono(scope.scope.clone(), ui.text, mono.clone()))
                .child(div().flex_none().flex().gap(px(2.)).children(strip))
                .child(popover::tail(words, outcome_ink));
            out.push(self.marked_row(scope.scope.clone(), row).into_any_element());
        }
    }
}
