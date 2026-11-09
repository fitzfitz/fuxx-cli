//! How command blocks look (M11l): the gutter bars in the terminal's left padding, the hairline
//! above each prompt, and the hover chip's words. Pure: `terminal_view` paints and hit-tests
//! with these.

use alacritty_terminal::vte::ansi::Rgb;

use crate::blocks::Outcome;
use crate::grid::BlockView;
use crate::theme::UiColors;

/// A block's bar: its width, its width while hovered or chosen, and its corner radius.
pub const GUTTER_BAR: f32 = 2.0;
pub const GUTTER_BAR_ACTIVE: f32 = 3.0;
pub const BAR_RADIUS: f32 = 1.0;
/// How far left of the grid's origin the bar's centre sits (less in a split).
pub const GUTTER_INSET: f32 = 8.0;
pub const SPLIT_GUTTER_INSET: f32 = 6.0;
/// Bars stop this far short of a block's first and last rows, so stacked blocks stay apart.
pub const BAR_END_GAP: f32 = 1.0;
/// The hairline above a prompt.
pub const SEPARATOR: f32 = 1.0;
/// The chip: its height, its gap right of the gutter, its corner radius and side padding.
pub const CHIP_HEIGHT: f32 = 24.0;
pub const CHIP_GAP: f32 = 4.0;
pub const CHIP_RADIUS: f32 = 6.0;
pub const CHIP_PAD: f32 = 8.0;
/// The command's first line is cut to this many characters (then `…`).
pub const CHIP_CMD_CHARS: usize = 40;
/// Below this grid width the buttons take their short names.
pub const CHIP_NARROW: f32 = 220.0;
/// The chip goes this long after the pointer leaves the gutter and the chip.
pub const CHIP_HIDE_MS: u64 = 150;

/// A block's bar colour: red for a failed command, the working colour while it runs, quiet
/// otherwise (exit 0, or not reported).
pub fn bar_color(outcome: Outcome, ui: &UiColors) -> Rgb {
    match outcome {
        Outcome::Ok | Outcome::Unknown => ui.block_ok,
        Outcome::Failed(_) => ui.block_failed,
        Outcome::Running => ui.block_running,
    }
}

/// How far left of the grid's origin the bars' centre sits.
pub fn gutter_x(split: bool) -> f32 {
    if split {
        SPLIT_GUTTER_INSET
    } else {
        GUTTER_INSET
    }
}

/// The chip's first words: the exit code, `running`, or nothing when it wasn't reported.
pub fn chip_outcome(outcome: Outcome) -> Option<String> {
    match outcome {
        Outcome::Ok => Some("exit 0".into()),
        Outcome::Failed(n) => Some(format!("exit {n}")),
        Outcome::Running => Some("running".into()),
        Outcome::Unknown => None,
    }
}

/// The command as the chip shows it: its first line, cut to `CHIP_CMD_CHARS` with `…`.
pub fn chip_command(cmd: &str) -> String {
    let first = cmd.lines().next().unwrap_or("");
    if first.chars().count() <= CHIP_CMD_CHARS {
        return first.to_string();
    }
    let mut cut: String = first.chars().take(CHIP_CMD_CHARS).collect();
    cut.push('…');
    cut
}

/// The chip's three buttons, short in a grid narrower than `CHIP_NARROW`.
pub fn chip_buttons(grid_width: f32) -> [&'static str; 3] {
    if grid_width < CHIP_NARROW {
        ["Cmd", "Output", "Rerun"]
    } else {
        ["Copy command", "Copy output", "Rerun"]
    }
}

/// The chip's left side as one line: the outcome, ` · `, and the command, whichever of them
/// there are (the chip lays the parts out side by side; the shots read them this way).
#[cfg(any(test, feature = "ui-shots"))]
pub fn chip_label(outcome: Outcome, cmd: Option<&str>) -> String {
    let parts: Vec<String> = [chip_outcome(outcome), cmd.map(chip_command)]
        .into_iter()
        .flatten()
        .collect();
    parts.join(" · ")
}

/// The outcome's colour on the chip: a failure in the red bar's colour held to text contrast
/// on the chip's `panel`, anything else in `text_dim`.
pub fn outcome_ink(outcome: Outcome, ui: &UiColors) -> Rgb {
    match outcome {
        Outcome::Failed(_) => {
            crate::ui_tokens::ensure_contrast(ui.block_failed, ui.panel, crate::ui_tokens::TEXT_MIN)
        }
        _ => ui.text_dim,
    }
}

/// Where a block's gutter can be pointed at, in window points (`top..bottom`): its prompt and
/// command rows, or the whole block when chosen. `id_index` points into the list of ids painted
/// with it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GutterHit {
    pub id_index: usize,
    pub top: f32,
    pub bottom: f32,
}

/// The hit under window `y`, if any.
pub fn hit_at(hits: &[GutterHit], y: f32) -> Option<GutterHit> {
    hits.iter().copied().find(|h| h.top <= y && y < h.bottom)
}

/// The viewport rows a block's bar covers, clamped to the `rows` shown: its prompt and command,
/// or, chosen, the whole block. None when they are all off the screen.
pub fn bar_rows(block: &BlockView, rows: usize) -> Option<(i32, i32)> {
    let (top, mut bottom) = block.prompt;
    if block.chosen
        && let Some((_, last)) = block.output
    {
        bottom = bottom.max(last);
    }
    let (top, bottom) = (top.max(0), bottom.min(rows as i32 - 1));
    (top <= bottom).then_some((top, bottom))
}

/// The viewport rows that get a hairline at their top edge: each block's first prompt row and
/// the live prompt's, when on screen and not `first_line`, the very first line of scrollback
/// (in viewport rows). Top to bottom, each once.
pub fn separator_rows(
    blocks: &[BlockView],
    live_prompt: Option<i32>,
    rows: usize,
    first_line: i32,
) -> Vec<i32> {
    let mut out: Vec<i32> = blocks
        .iter()
        .map(|b| b.prompt.0)
        .chain(live_prompt)
        .filter(|&r| r >= 0 && r < rows as i32 && r != first_line)
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// The chip's top: the command row's top, moved up so the chip stays above `grid_bottom` (and
/// no higher than `grid_top`).
pub fn chip_top(cmd_top: f32, grid_top: f32, grid_bottom: f32) -> f32 {
    cmd_top.min(grid_bottom - CHIP_HEIGHT).max(grid_top)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::every_theme;
    use crate::ui_tokens::{MARK_MIN, QUIET_LINE_MIN, TEXT_MIN, contrast, ensure_contrast};

    #[test]
    fn block_inks_meet_the_floors() {
        for (name, theme) in every_theme() {
            let ui = theme.ui();
            let bg = theme.background;
            assert_eq!(
                ui.block_ok,
                ensure_contrast(theme.palette[8], bg, MARK_MIN),
                "{name}"
            );
            assert_eq!(
                ui.block_failed,
                ensure_contrast(theme.palette[1], bg, MARK_MIN),
                "{name}"
            );
            assert_eq!(
                ui.block_running,
                ensure_contrast(ui.ring_working, bg, MARK_MIN),
                "{name}"
            );
            for (ink, c) in [
                ("block_ok", ui.block_ok),
                ("block_failed", ui.block_failed),
                ("block_running", ui.block_running),
                ("accent", ui.accent),
            ] {
                let r = contrast(c, bg);
                assert!(r >= MARK_MIN - 0.01, "{name}: {ink} on the terminal {r:.2}");
            }
            for (ink, c) in [
                ("text", ui.text),
                ("text_dim", ui.text_dim),
                ("text_faint", ui.text_faint),
            ] {
                for (on, s) in [("panel", ui.panel), ("panel_hover", ui.panel_hover)] {
                    let r = contrast(c, s);
                    assert!(r >= TEXT_MIN - 0.01, "{name}: {ink} on {on} {r:.2}");
                }
            }
            let failed = outcome_ink(Outcome::Failed(1), &ui);
            let r = contrast(failed, ui.panel);
            assert!(r >= TEXT_MIN - 0.01, "{name}: exit 1 on the chip {r:.2}");
            assert_eq!(outcome_ink(Outcome::Ok, &ui), ui.text_dim, "{name}");
            let r = contrast(ui.card_border, bg);
            assert!(r >= QUIET_LINE_MIN - 0.01, "{name}: separator {r:.2}");
            // Each outcome its colour.
            assert_eq!(bar_color(Outcome::Ok, &ui), ui.block_ok);
            assert_eq!(bar_color(Outcome::Unknown, &ui), ui.block_ok);
            assert_eq!(bar_color(Outcome::Failed(2), &ui), ui.block_failed);
            assert_eq!(bar_color(Outcome::Running, &ui), ui.block_running);
        }
    }

    #[test]
    fn chip_texts() {
        assert_eq!(chip_outcome(Outcome::Failed(1)).as_deref(), Some("exit 1"));
        assert_eq!(chip_outcome(Outcome::Ok).as_deref(), Some("exit 0"));
        assert_eq!(chip_outcome(Outcome::Running).as_deref(), Some("running"));
        assert_eq!(chip_outcome(Outcome::Unknown), None);
        assert_eq!(chip_command("npm test\nsecond"), "npm test");
        let fifty = "x".repeat(50);
        let cut = chip_command(&fifty);
        assert_eq!(cut, format!("{}…", "x".repeat(40)));
        assert_eq!(
            chip_command(&"é".repeat(40)),
            "é".repeat(40),
            "40 chars stay"
        );
        assert_eq!(chip_buttons(219.0), ["Cmd", "Output", "Rerun"]);
        assert_eq!(
            chip_buttons(220.0),
            ["Copy command", "Copy output", "Rerun"]
        );
        assert_eq!(gutter_x(false), 8.0);
        assert_eq!(gutter_x(true), 6.0);
        assert_eq!(
            chip_label(Outcome::Failed(1), Some("false")),
            "exit 1 · false"
        );
        assert_eq!(chip_label(Outcome::Unknown, Some("ls")), "ls");
        assert_eq!(chip_label(Outcome::Running, None), "running");
        assert_eq!(chip_label(Outcome::Unknown, None), "");
    }

    fn view(prompt: (i32, i32), output: Option<(i32, i32)>, chosen: bool) -> BlockView {
        BlockView {
            id: "b".into(),
            outcome: Outcome::Ok,
            prompt,
            cmd_row: prompt.1,
            cmd: None,
            output,
            chosen,
        }
    }

    #[test]
    fn bars_cover_the_prompt_or_the_chosen_block() {
        let b = view((2, 3), Some((4, 9)), false);
        assert_eq!(bar_rows(&b, 20), Some((2, 3)));
        assert_eq!(
            bar_rows(&view((2, 3), Some((4, 9)), true), 20),
            Some((2, 9))
        );
        // Clamped to the screen; gone when off it.
        assert_eq!(bar_rows(&view((-3, 1), None, false), 20), Some((0, 1)));
        assert_eq!(
            bar_rows(&view((18, 19), Some((20, 30)), true), 20),
            Some((18, 19))
        );
        assert_eq!(bar_rows(&view((-3, -1), Some((0, 4)), false), 20), None);
        assert_eq!(bar_rows(&view((20, 21), None, false), 20), None);
        let hits = [
            GutterHit {
                id_index: 0,
                top: 10.0,
                bottom: 20.0,
            },
            GutterHit {
                id_index: 1,
                top: 30.0,
                bottom: 40.0,
            },
        ];
        assert_eq!(hit_at(&hits, 10.0).map(|h| h.id_index), Some(0));
        assert_eq!(hit_at(&hits, 19.9).map(|h| h.id_index), Some(0));
        assert_eq!(hit_at(&hits, 20.0), None);
        assert_eq!(hit_at(&hits, 35.0).map(|h| h.id_index), Some(1));
    }

    #[test]
    fn separators_sit_above_prompts() {
        let blocks = [view((0, 0), Some((1, 1)), false), view((3, 4), None, false)];
        // Block 1 starts on the first line of scrollback: no line there.
        assert_eq!(separator_rows(&blocks, Some(5), 10, 0), vec![3, 5]);
        // Scrolled so it isn't: a line above it too.
        assert_eq!(separator_rows(&blocks, Some(5), 10, -4), vec![0, 3, 5]);
        // Off-screen rows get none; the live prompt may be a block's row only once.
        assert_eq!(separator_rows(&blocks, Some(12), 4, -1), vec![0, 3]);
        assert_eq!(separator_rows(&blocks, Some(3), 10, -1), vec![0, 3]);
        assert_eq!(separator_rows(&[], None, 10, 0), Vec::<i32>::new());
        // The chip stays inside the grid.
        assert_eq!(chip_top(50.0, 10.0, 200.0), 50.0);
        assert_eq!(chip_top(190.0, 10.0, 200.0), 176.0);
        assert_eq!(chip_top(5.0, 10.0, 200.0), 10.0);
    }
}
