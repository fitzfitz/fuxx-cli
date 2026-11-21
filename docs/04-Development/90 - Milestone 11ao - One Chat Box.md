---
tags: [milestone, m11ao, placeholder]
status: placeholder
---

# Milestone 11ao - One chat box with add-ons (placeholder)

Added 2026-11-21 on your words: "is it possible to have 1 chat box that we can reuse? this chat box can be have pills
on the left/right of the chat box, and we can have folders info/picker, branch, etc at the top on the first layer as
well, it can be as an addon. 1 reusable component with addons", then "yes write the placeholder, ask the designer
agent". **Not planned.** The lead designer's outline is below; when we come back: a design talk, then a plan.

## The problem

The shell tab's composer and the agent tab's composer look alike but are two implementations, and they drift (your
words: "the chat is different"); every look change has been made twice.

- **Shell tab** (`card_view.rs`): not a text field. It's a window onto zsh's live line in the terminal grid; its
  frame and folder tab are painted in card view's canvas, lined up with the grid's cells.
- **Agent tab** (`agent_box.rs`, `agent_tab.rs` `folder_tab`, `composer.rs`): fuxx's own text box, built from UI
  elements, with chips above the text and a controls row (`+`, model, mode, send) below it.

## The lead designer's outline (2026-11-21)

**Share the frame, the slots and the pills; never the middle.** The shell's typing stays zsh's line in the grid.

### Anatomy and slots

Three layers: the folder tab, the outer box, the inner box.

- **Tab** (top-left only): context pills left to right in a fixed order: the running-command chip, the folder, the
  branch, then later add-ons. Nothing on the tab's right: that's the stepped-down edge, and pills there would float
  in the notch; anything for the right goes in the controls row.
- **Chips row** (optional): inside the inner box, above the input (attachments today).
- **Input slot** (always): owned by the tab using the box.
- **Controls row** (optional): a leading group (`+`, then pickers such as the model), a flexible gap, a trailing
  group (pickers such as the mode, then one primary action, send).
- **No footer or status line**: status goes in the tab as a pill, as the running chip does.
- **Who fills what:** the shell tab fills the tab and the input only (exit status stays on the cards' gutter marks;
  no run button: Enter runs the line, and a button would suggest a text field that isn't one). The agent tab fills
  everything.

### Rules every user of the box shares

- **Tab:** rises 25; pills 22 tall, radius 11; 3 inside the tab, 4 between pills; the tab's top corners 14, the
  concave step 8. Pill fill `composer_inner`, text and icon `text_dim`.
- **Boxes:** outer radius 15, inner 12; the outer 3 around the inner; text inset 14 sideways, 12 up and down. Outer
  `composer_outer` with a 1 px `composer_edge`; inner `composer_inner`, no border.
- **Focus:** nothing beyond the caret, in both tabs.
- **Controls row:** 28 tall, 8 in from the inner box's left, right and bottom; 6 between controls in a group, 8 before
  the primary action. Pills radius 14, padding 10 left and 8 right; circles 28 across. Control fill
  `mix(composer_inner, foreground, 0.06)`, 0.10 under the pointer; the primary action in the accent.
- **Tab overflow** (pills wider than the outer box minus 48), in order: (1) labels, already capped at 160 with "…",
  shorten down to 80, the lowest-priority pill first; (2) pills go icon-only, lowest priority first, the tooltip
  keeping the full label; (3) what's left folds into a last "…" pill with a menu. The folder pill collapses last.
- **Controls overflow:** below an inner width of 360, pickers go icon-only; `+` and the primary action never hide.

### What a pill add-on provides

- An id, a slot (tab, leading or trailing) and an overflow priority.
- A Lucide icon (required): 13 pt in the tab, 14 in the controls row.
- A label (empty for icon-only) and a tooltip with the full value.
- An action: none, a click, or a menu. With none, no hover and the default cursor.
- A locked state: looks like no action; the tooltip says why (the agent's folder after the first message).
- A maximum width (160 by default); an optional colour, only as a status dot.
- It may **not** set its own fill, edge, height, font size, or use an emoji.

### Risks, and what not to unify

- **Not the middle, and no controls row on the shell box**: pickers or send there would make a terminal line a fake
  form.
- **Two ways of drawing**: the shell frame is painted to grid cells, the agent box is elements. Share one geometry
  module instead: the constants plus a pure layout function giving the tab, notch, outer and inner rectangles and
  corners; both tabs draw the frame through one painter and lay their content into the slots it returns.
- **Height comes from the input slot**: the shell's snaps to cell rows; the agent's is free up to its maximum.
- **The shell's tab pills need hit areas inside the canvas** for tooltips and clicks: the main build risk.
- **Popups stay the agent's** (`@`, `/`, the menus): add-ons of the agent tab, not part of the box.

## Order

The designer's call: **a separate milestone, after [[89 - Milestone 11an - Full-Width Typing Line]]**. M11an decides
how the shell's first typing row is laid out (way A or B), which sets the shell's input-slot contract and its height
rules; building the shared box first would mean reworking its shell side straight after.

## What we gain, what it costs

- **Gain:** the two boxes can't drift; a look change is made once; a later tab gets the box and its pills for free.
- **Cost:** the shell frame moves onto the shared geometry and painter, and card view's resize and cropping behaviour
  (and their scenes) must be re-proved; the shell's pills need hit areas in the canvas. Milestone-sized, not a chore.

## Open questions (for the design talk)

1. Is the designer's split right for you: the shell box keeps no controls row? **Yes** (you, 2026-11-21).
2. Which add-ons first? **"the pills, buttons"** (you, 2026-11-21): the first version carries what the boxes have
   today onto the shared box (the tab's pills: folder, branch, the running chip; the agent's buttons and pickers:
   `+`, model, mode, send), nothing new.
3. Order against M11ad and M11am? You: "I don't know". The AI's recommendation, waiting for your word: M11ad stays
   next (background agents don't touch the box), then M11an → M11ao → M11am, so the agent tab's rework builds on
   the finished shared box instead of redoing it.

## Related

[[88 - Milestone 11ab - Chat-Style Shell Tabs]] · [[89 - Milestone 11an - Full-Width Typing Line]] ·
[[40 - Roadmap]] · [[12 - Scope]]
