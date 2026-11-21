# M11ab Rework — Composer and Rail Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Card view as approved in the rework mockups: soft-surface cards on a rail, and the live prompt as a
composer (outer box with the context row, inner box holding only what you type).

**Architecture:** `card_view.rs` keeps the model: segments gain the prompt's tag row, so the live box starts at the
typing and a running command's at its output; cards cover their wrapped command rows; new heights and sizes; a pure
crop of the first live row (from the input column, the right-hand prompt cut). `theme.rs` gains the composer and card
surface shades. `terminal_view.rs` paints the rail, the surfaces and the two boxes; the context row is elements
(Lucide `Folder`, `Branch`, `ChevronDown`) placed from the last frame, as the card buttons are.

**Spec:** `docs/04-Development/88 - Milestone 11ab - Chat-Style Shell Tabs.md`, section "Rework (2026-11-21)".

## Global Constraints

- Branch `m11ab-rework` from master. Gate per task: `cargo test --bin fuxx`, both clippy runs at the baseline (14 / 27
  default, 16 / 29 ui-shots), `cargo fmt --check`. Full `scripts/ui-shots.sh` in Task 4, then one whole-branch review.
- Sizes: card surface radius 8, padding 8 / 12 / 10; outer box radius 16, 1 pt edge, the inner box inset 5 pt from its
  sides and bottom; inner box radius 12, 1 pt edge (lightest along its top), padding 12 / 14; rail at the pane's left
  padding, dots 8 pt (a ring for the chosen card).
- Icons: the app's own `icons::Icon::{Folder, Branch, ChevronDown}` only.
- Colours from `UiColors`, text held to `TEXT_MIN` on its surface.

## Review Focus

1. A prompt that wraps in a narrow pane: the typing still starts where B was, and the wrapped part stays in view.
2. A session reattached mid-prompt (no input start known): the row shows whole, no crash, no blank box.
3. Typing past the box's width: the second row starts at column 0, not cropped.
4. A right prompt absent (plain `$ `) or present on a two-line prompt: nothing of yours is cut.
5. Light themes: the composer's shades are lighter / darker in the right direction and text reads.

---

### Task 1: The model (`card_view.rs`, `blocks.rs`)

- `Segment::Prompt` gains `row: Line` (its tag's row, where the typing starts).
- `extent` of a command covers its command's rows to the row before its output (`cmd_end`), so a wrapped command line
  is inside its card. Test `a_wrapped_command_row_stays_in_its_card`.
- `column`: the live box starts at the shown prompt's `row` (its upper prompt rows hidden, not loose), or at a running
  command's output start (its prompt and command rows hidden). Tests `the_live_box_starts_at_the_typing`,
  `a_running_command_shows_only_its_output`.
- `pub fn crop(line: &[CellView], input_col: Option<usize>) -> Crop { pub from: usize, pub cut: Option<usize> }`: from
  the input column (0 when unknown), cut at the right-hand prompt (`block_view::right_run`). Test
  `the_prompt_and_its_right_side_are_left_out` (left `~/code/api main ` + typed `ls`, right `✔ 08:00` → from 16, cut
  at the right run; no right run → none; unknown input → from 0).
- Sizes: `CTX_ROW` 30, `OUTER_INSET` 5, `INNER_PAD_X` 14, `INNER_PAD_Y` 12, `SURF_PAD` (8, 12, 10); `card_dims` = the
  inner box's inside; `card_height` / `item_height` for surfaces (no panel) and the composer. Tests updated:
  `card_dims_are_the_box_inside`, `heights_add_up`.

### Task 2: The shades (`theme.rs`)

- `UiColors`: `card_surface`, `composer_outer`, `composer_edge`, `composer_inner`, `composer_inner_edge`,
  `composer_inner_top`, stepped from the terminal background toward its ink (the reference's steps: outer a little,
  inner a little more, edges a little more again, the inner top edge most). Test
  `the_composer_and_cards_step_up_and_text_reads` (each step apart from the last; `text`, `text_dim` at `TEXT_MIN` on
  the surfaces; every built-in theme).

### Task 3: Painting and the context row (`terminal_view.rs`)

- Cards: the rail line and a dot per card (green / red / the accent ring when chosen) in the left padding; the surface;
  header, `❯ cmd`, fold row, output inside it.
- The composer: outer box, inner box; the first live row drawn from `crop` (its hidden part left of the box, clipped;
  cells from `cut` blank), a `❯` in the accent before it; `Type a command` in `text_dim` while the input is empty;
  later rows from column 0.
- The context row as elements over the outer box: the running chip (peach), `Folder` + folder name + `ChevronDown`
  (shows the folder in Finder), `Branch` + branch + `ChevronDown` (opens the branch switcher).
- Regions: the first live row its own region with its shifted x (pointing still lands on the right cell).

### Task 4: Scenes, review, docs

- Update the `shell-cards*` scenes to the new sizes; add `shell-cards-prompt` (a two-sided prompt: none of its left or
  right text inside the inner box, the typed text and `❯` there). Full ui-shots; whole-branch review; one fix pass;
  note 88 "Rework as built"; Progress Log; commit. Merge and push on your word.
