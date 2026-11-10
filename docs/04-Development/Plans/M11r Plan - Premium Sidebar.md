# M11r Plan — Premium Sidebar

> Executed natively on master (your call, 2026-11-10: "cool do it directly here on master").

**Goal:** the sidebar of [[69 - Milestone 11r - Premium Sidebar]]: avatars with status badges, the program's title
under the name, a waiting row's Open button, lettered group headers with the branch, soft borderless cards, a
"Jump to a workspace…" button and a "New Fuxxpace" button.

**Architecture:** pure helpers in `src/sidebar.rs` (initials, avatar colour pick, the line under the name, row
heights) tested first; avatar colours computed once per theme in `src/theme.rs` (`UiColors::avatars`) and held to
the contrast floors; the render functions in `src/sidebar.rs` (`render_workspace_row`, `render_header`,
`render_sidebar`, `render_footer`, `DragPreview`) draw them. No new dependency, timer or poll.

## Global constraints

- Every colour drawn on a surface is in `Theme::drawn_pairs` and passes the contrast test in every built-in theme.
- Drag and drop (in, out, between groups) behaves exactly as before; its tests stay green.
- ⌘1–⌘9, rename (double-click), close (× and middle-click), right-click menu, tab drop onto a row keep working.
- `cargo test`, `cargo fmt --check`, `cargo clippy --all-targets --features ui-shots` (no new warnings), the sidebar
  ui-shots scenes.

## Review focus

1. A name with no letters or digits (emoji, "╭∩╮"): initials fall back to its first character.
2. A very long title or question: cut with … on one line; the Open button keeps its room.
3. A workspace with no terminal (browser only): avatar, no badge, no title line.
4. A collapsed group of 10: at most 3 small avatars.
5. Light themes: avatar ink and tile ink readable (contrast test).

## Tasks

### Task 1: pure helpers (sidebar.rs, theme.rs)
- `pub fn initials(label: &str) -> String` — tests: "New Fuxxpace"→"NF", "John Cena"→"JC", "api-server"→"AS",
  "fuxx"→"F", "  "→"", "╭∩╮"→"╭".
- `pub fn avatar_color(color: Option<TabColor>, label: &str) -> TabColor` — the set colour, else a stable pick
  (FNV-1a of the label) among `TabColor::ALL` without Grey; test: same label → same colour, Grey never picked.
- `WorkspaceRow.title: Option<String>` (from `groups::extra_title`), `LineTwo::Title(String)`, priority NotLive →
  Waiting → Note → Title → Place; `WorkspaceRow.in_folder_group` drops Place; tests for each.
- `UiColors::avatars: [(Rgb, Rgb); 11]` (fill, ink), ink readable at `TEXT_MIN` on its fill; in `drawn_pairs`.

### Task 2: rows (render_workspace_row, close_slot, DragPreview)
- Avatar 28 px, radius 8, initials 11 px bold; badge = the status mark in a 14 px circle of the row's surface at
  its bottom-right; hover shows × in the avatar.
- Line two in monospace (title) or as before; waiting: question + Open (selects the subject, focuses it).
- Row heights: one line 42, two 52, three 68 (`ui_tokens`).

### Task 3: headers, cards, search, footer
- Header: tile 18 px (first letter, or the globe for a host), name 12 px semibold, branch button, spacer, collapsed:
  ≤3 small avatars + "N waiting" pill, count / hover +, chevron last.
- Cards: no border, radius 12.
- "Jump to a workspace…" button (30 px, ⌘P) above the list → `OpenSwitcher`.
- Footer: "+ New Fuxxpace" button (flex) opening the New menu; bell and gear as before.

### Task 4: verify and document
- ui-shots expectations updated for the new geometry; all sidebar scenes pass.
- Note 69 "As built", Roadmap, Scope, tracker, Progress Log.
