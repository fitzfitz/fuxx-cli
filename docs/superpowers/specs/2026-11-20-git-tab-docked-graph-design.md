# Git tab: GRAPH docked at the bottom, smaller indented rows (design)

Date: 2026-11-20. A chore after M11q (recorded in note 68). Approved by you in chat ("yes go ahead").

## Intent

Your ask, with an example screenshot of another tool's Git panel: "make our git tab section graph to be like the
example? always on the bottom appeared flexed, can be dragged the height with min height of 1/4 of container and
max height of half the container", and "make the size of icon, text smaller, and make it indented".

## Today

The Changes panel (`changes_panel.rs`) is one `uniform_list`: STAGED and CHANGES (or "No changes"), then the
GRAPH's header and, open, its commits and "Load more" (`all_rows`). Rows are 32 pt (`right_sidebar::ROW`), a
16 pt check circle, a 13.5 pt name, from the header's edge. The branch bar is the panel's last stretch.

## Design

- **Two regions.** The top region is the STAGED / CHANGES list, scrolling on its own and taking what is left. The
  bottom region is the GRAPH: its header, and while open its own scrolling list of commits and "Load more". It
  always sits at the panel's bottom, right above the branch bar; closed, only its header shows there.
- **A divider** between them, while the GRAPH is open: drag it to set the GRAPH's height, held between 1/4 and 1/2
  of the room the two regions share (also when the window resizes). It looks and behaves like the sidebars'
  edges (a quiet line, lit on hover). The GRAPH opens at 1/3 the first time; the height you set is remembered
  across restarts (as a fraction), the way the right panel's width is.
- **Rows in the Git tab** (files and commits): 24 pt tall, the name 12 pt, the folder and counts 11 pt, the check
  circle 12 pt, the mark letter 11 pt; indented 16 pt so their content starts under the header's label (past its
  chevron). The Files tab keeps its rows.
- **Unchanged:** the GRAPH's header buttons (branch filter, ◎ scroll to HEAD, refresh) act on the GRAPH's own
  list; "Load more" stays at its end; a clean repo still says "No changes since the last commit." in the top region.

## Approaches considered

1. Two lists with a divider (chosen): matches the example exactly, each region scrolls on its own.
2. One list with the GRAPH header made sticky at the bottom: no own height or scroll for the commits, so the
   1/4–1/2 drag can't be had.

## Left to the builder

The divider's hit width and the fraction's storage key follow the right panel's width (`right_sidebar`).
