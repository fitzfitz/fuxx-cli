---
tags: [development, plan, milestone]
milestone: M8
status: in-progress
updated: 2026-10-27
---

# M8 Plan — Split Panes (nested, like Ghostty/iTerm)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

| At a glance | |
|---|---|
| **Milestone note** | [[48 - Milestone 8 - Split Panes]] (the spec) |
| **Status** | 🟨 In progress |
| **Tasks done** | 0 / 7 |
| **▶ Pick up here** | Task 1 |
| **Decisions used** | **Yours (2026-10-27):** split panes in scope, built right after M7; nested splits (⌘D right, ⌘⇧D down), draggable dividers, each pane its own session. The rest are the AI's calls listed in the spec. |
| **Needs your eyes** | 👀 Task 6, done together with the M6b and M7 checks |

**Goal:** Nested split panes, each a normal session, as the spec describes.

**Architecture:**

```
 split_tree.rs (pure, tested)                 workspace.rs                          panes.rs (GPUI)
 Node<T> = Leaf(T) | Split{axis, ratio,       layouts: Vec<Node<EntityId>>          render_layout(tree, …):
           first, second}                     (every session's view is in exactly    nested flex rows/columns,
 split, remove, leaves, rects, split_rect,     one layout; the shown one holds the   dividers (drag → ratio),
 set_ratio, neighbor, cycle, equalize, map,    active session)                       dim overlay on unfocused
 sanitize(saved trees)                         split / close / focus / save         panes, click-to-focus
 restore.rs: SavedState.layouts: Vec<Node<usize>>  (session indices; serde default = none)
```

**Tech Stack:** No new crates (serde from M7). GPUI at the pinned rev: `div().flex_row()/.flex_col()`, `flex_basis(relative(r))`/`flex_grow`/`flex_shrink`,
`canvas(prepaint, paint)` to capture bounds (as `terminal_view.rs` does), `.on_mouse_down` / `.on_mouse_move` / `.on_mouse_up`,
`.cursor_col_resize()` / `.cursor_row_resize()`, `Hsla::alpha` / `opacity` for the dim overlay.

**Spec:** [[48 - Milestone 8 - Split Panes]] · [[47 - Milestone 7 - Session Ergonomics]] · [[12 - Scope]]

## Global Constraints

- **Inherited:** everything from the M0–M7 plans. **No new crates.**
- **Gates:** 0 warnings from our code; `cargo fmt --check` clean; every test passes.
- **Behaviour:** exactly the spec's "How it behaves", and nothing under "Not in M8".
- **Layout invariant:** every session's view id is in exactly one layout tree, and every leaf is a live session. Opening a session without splitting adds a one-leaf layout; splitting adds the new session to the focused pane's tree; closing removes the leaf (its sibling takes the space) and drops empty trees.
- **Ratios:** always within `MIN_RATIO` 0.1 … `MAX_RATIO` 0.9; a new split is 0.5.
- **Keys:** Split Right `cmd-d`, Split Down `cmd-shift-d`, Focus Pane Left/Right/Up/Down `cmd-alt-left/right/up/down`, Previous/Next Pane `cmd-[` / `cmd-]`, Equalize Panes `ctrl-cmd-=`. All bound with no context (like `cmd-t`), before `set_menus`.
- **Menu:** a new **Pane** menu after File: Split Right · Split Down · separator · Previous Pane · Next Pane · Focus Pane Left/Right/Up/Down · separator · Equalize Panes.
- **Save file:** `SavedState` gains `#[serde(default)] layouts: Vec<Node<usize>>` (indices into `sessions`). `VERSION` stays 1 (old files load with no layouts → every session alone).
- **Failures never crash:** a saved layout that doesn't fit the sessions is dropped by `sanitize` and logged once as `[fuxx] restore: …`.
- **No `cargo run` by subagents.** GUI behaviour is verified in Task 6 by the user.

## Review Focus

1. **Closing a pane leaves a hole or a dangling leaf** (the tree keeps a dead session, or the wrong pane gets focus) → `remove_gives_the_space_to_the_sibling` (Task 1); Task 3 picks the focus from the tree before removing.
2. **Focus moves to the wrong pane with ⌥⌘arrows** in nested layouts → `neighbor_by_direction` (Task 1).
3. **A divider dragged to the edge collapses a pane to nothing** → `set_ratio_clamps` (Task 1); the drag computes the ratio from `split_rect`.
4. **A hand-edited or stale save file with a broken layout** (out-of-range or repeated indices, NaN ratios) crashes or duplicates a session → `sanitize_drops_bad_trees_and_fixes_ratios` (Task 1), `layouts_round_trip_and_default_to_none` (Task 2).
5. **Programs get the wrong size after a split or a drag** → each `TerminalView` already measures its own bounds on prepaint and resizes its PTY (M5); Task 6 checks `tput cols` per pane.

## Files

| File | Change | Responsibility | Task |
|---|---|---|---|
| `src/split_tree.rs` | Create | `Axis`, `Dir`, `Rect`, `Node<T>` and its operations, `sanitize` | 1 |
| `src/restore.rs` | Modify | `SavedState.layouts` | 2 |
| `src/panes.rs` | Create | Rendering a layout: nested flex, dividers, dim overlay, click-to-focus | 3–4 |
| `src/workspace.rs` | Modify | `layouts`, split/close/focus/select, Pane actions, sidebar "on screen" highlight, save/restore layouts | 3–5 |
| `src/main.rs` | Modify | Pane menu and bindings | 3–4 |

---

### Task 1: The split tree, without GPUI — ⬜

**Files:** Create `src/split_tree.rs` · Modify `src/main.rs` (`mod split_tree;`).

**Interfaces (produces):**
- `#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)] #[serde(rename_all = "lowercase")] pub enum Axis { Row, Column }` — `Row` = side by side (split right), `Column` = stacked (split down).
- `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Dir { Left, Right, Up, Down }`
- `#[derive(Clone, Copy, Debug, PartialEq)] pub struct Rect { pub x: f32, pub y: f32, pub w: f32, pub h: f32 }`
- `pub const MIN_RATIO: f32 = 0.1; pub const MAX_RATIO: f32 = 0.9;`
- `#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)] #[serde(rename_all = "lowercase")] pub enum Node<T> { Leaf(T), Split { axis: Axis, ratio: f32, first: Box<Node<T>>, second: Box<Node<T>> } }`
- `impl<T: Copy + PartialEq> Node<T>`:
  - `pub fn leaves(&self) -> Vec<T>` — reading order (first before second, recursively).
  - `pub fn contains(&self, id: T) -> bool`
  - `pub fn split(&mut self, target: T, new: T, axis: Axis) -> bool` — the `target` leaf becomes `Split { axis, ratio: 0.5, first: Leaf(target), second: Leaf(new) }`; false if `target` is absent.
  - `pub fn remove(self, target: T) -> Option<Node<T>>` — the tree without `target`; the removed leaf's sibling takes its parent's place. `None` if the tree was only that leaf. Unchanged (`Some(self)`) if absent.
  - `pub fn rects(&self, area: Rect) -> Vec<(T, Rect)>` — each leaf's area, reading order. `Row`: first gets the left `w * ratio`; `Column`: first gets the top `h * ratio`.
  - `pub fn split_rect(&self, path: &[bool], area: Rect) -> Option<(Axis, Rect)>` — the area and axis of the split reached by `path` (`false` = first child, `true` = second) from the root; `None` if the path doesn't end at a split.
  - `pub fn set_ratio(&mut self, path: &[bool], ratio: f32) -> bool` — sets that split's ratio clamped to `MIN_RATIO..=MAX_RATIO` (NaN → 0.5); false if the path doesn't end at a split.
  - `pub fn neighbor(&self, from: T, dir: Dir) -> Option<T>` — on the rects of a unit square: candidates lie entirely on the `dir` side of `from` (with 1e-4 tolerance) and overlap it on the other axis by more than 1e-4; pick the smallest gap, then the largest overlap, then the first in reading order.
  - `pub fn cycle(&self, from: T, forward: bool) -> Option<T>` — next/previous leaf in reading order, wrapping; `None` if `from` is absent or the tree has one leaf.
  - `pub fn equalize(&mut self)` — every split's ratio = weight(first) / (weight(first) + weight(second)), where a child's weight along an axis is the sum of its children's weights if it's a split on the same axis, else 1.
  - `pub fn map<U>(&self, f: &impl Fn(T) -> U) -> Node<U>`
- `pub fn sanitize(trees: Vec<Node<usize>>, count: usize) -> (Vec<Node<usize>>, bool)` — drops any tree with an index `>= count` or an index already used (in it or an earlier kept tree); clamps every ratio as `set_ratio` does; then adds a `Leaf(i)` for every index `< count` not in a kept tree, in index order. The bool is true when anything was dropped or a ratio fixed.

- [ ] **Step 1: Failing tests:**

```rust
fn three() -> Node<u32> {
    // 1 | (2 over 3)
    let mut t = Node::Leaf(1);
    assert!(t.split(1, 2, Axis::Row));
    assert!(t.split(2, 3, Axis::Column));
    t
}
const AREA: Rect = Rect { x: 0.0, y: 0.0, w: 100.0, h: 50.0 };

#[test]
fn split_keeps_reading_order() {
    let mut t = three();
    assert_eq!(t.leaves(), [1, 2, 3]);
    assert!(!t.split(9, 4, Axis::Row));
    assert!(t.contains(3) && !t.contains(9));
}

#[test]
fn remove_gives_the_space_to_the_sibling() {
    let t = three().remove(2).unwrap();
    assert_eq!(t, Node::Split { axis: Axis::Row, ratio: 0.5, first: Box::new(Node::Leaf(1)), second: Box::new(Node::Leaf(3)) });
    assert_eq!(Node::Leaf(1).remove(1), None);
    assert_eq!(three().remove(7), Some(three()));
}

#[test]
fn rects_follow_the_ratios() {
    assert_eq!(
        three().rects(AREA),
        vec![
            (1, Rect { x: 0.0, y: 0.0, w: 50.0, h: 50.0 }),
            (2, Rect { x: 50.0, y: 0.0, w: 50.0, h: 25.0 }),
            (3, Rect { x: 50.0, y: 25.0, w: 50.0, h: 25.0 }),
        ]
    );
    assert_eq!(three().split_rect(&[true], AREA), Some((Axis::Column, Rect { x: 50.0, y: 0.0, w: 50.0, h: 50.0 })));
    assert_eq!(three().split_rect(&[false], AREA), None);
}

#[test]
fn neighbor_by_direction() {
    let t = three();
    assert_eq!(t.neighbor(2, Dir::Left), Some(1));
    assert_eq!(t.neighbor(3, Dir::Left), Some(1));
    assert_eq!(t.neighbor(1, Dir::Right), Some(2)); // tie on gap and overlap: reading order
    assert_eq!(t.neighbor(2, Dir::Down), Some(3));
    assert_eq!(t.neighbor(3, Dir::Up), Some(2));
    assert_eq!(t.neighbor(1, Dir::Up), None);
}

#[test]
fn cycle_wraps() {
    assert_eq!(three().cycle(3, true), Some(1));
    assert_eq!(three().cycle(1, false), Some(3));
    assert_eq!(Node::Leaf(1).cycle(1, true), None);
}

#[test]
fn set_ratio_clamps() {
    let mut t = three();
    assert!(t.set_ratio(&[], 0.05));
    assert!(t.set_ratio(&[true], 0.95));
    assert!(!t.set_ratio(&[false], 0.5));
    let Node::Split { ratio, second, .. } = &t else { panic!() };
    assert_eq!(*ratio, MIN_RATIO);
    assert!(matches!(**second, Node::Split { ratio, .. } if ratio == MAX_RATIO));
    assert!(t.set_ratio(&[], f32::NAN));
    assert!(matches!(t, Node::Split { ratio, .. } if ratio == 0.5));
}

#[test]
fn equalize_counts_leaves_along_the_axis() {
    let mut t = three(); // the Column child counts as 1 across a Row
    t.set_ratio(&[], 0.8);
    t.equalize();
    assert!(matches!(t, Node::Split { ratio, .. } if ratio == 0.5));
    let mut row3 = Node::Leaf(1);
    row3.split(1, 2, Axis::Row);
    row3.split(2, 3, Axis::Row); // 1 | 2 | 3
    row3.equalize();
    let Node::Split { ratio, second, .. } = &row3 else { panic!() };
    assert!((ratio - 1.0 / 3.0).abs() < 1e-6);
    assert!(matches!(**second, Node::Split { ratio, .. } if ratio == 0.5));
}

#[test]
fn sanitize_drops_bad_trees_and_fixes_ratios() {
    let leaf = |i| Box::new(Node::Leaf(i));
    let ok = Node::Split { axis: Axis::Row, ratio: 2.0, first: leaf(0), second: leaf(1) };
    let out_of_range = Node::Split { axis: Axis::Row, ratio: 0.5, first: leaf(2), second: leaf(7) };
    let repeated = Node::Leaf(1);
    let (trees, changed) = sanitize(vec![ok, out_of_range, repeated], 3);
    assert!(changed);
    assert_eq!(
        trees,
        vec![
            Node::Split { axis: Axis::Row, ratio: MAX_RATIO, first: leaf(0), second: leaf(1) },
            Node::Leaf(2),
        ]
    );
    assert_eq!(sanitize(vec![Node::Leaf(0)], 1), (vec![Node::Leaf(0)], false));
}
```

- [ ] **Step 2: Run** `cargo test split_tree`. Expected: compile errors.
- [ ] **Step 3: Implement** as specified. GPUI-free. Narrow `#[allow(dead_code)]` until Task 3 uses it (ledger ruling, as in M7).
- [ ] **Step 4: Run** `cargo test`. Expected: all pass (M7 count + 8), 0 warnings.
- [ ] **Step 5: Commit:** `git add src && git commit -m "feat(m8): pure split tree (split, close, neighbours, ratios, sanitize)"`

### Task 2: Layouts in the saved state — ⬜

**Files:** Modify `src/restore.rs`.

**Interfaces:** `SavedState` gains `#[serde(default)] pub layouts: Vec<split_tree::Node<usize>>`. Update every `SavedState { .. }` literal (tests' `sample()`, the workspace's `save()` — `layouts: vec![]` there until Task 5).

- [ ] **Step 1: Failing test** (and add a two-pane layout to `sample()` so the existing round trip covers it):

```rust
#[test]
fn layouts_round_trip_and_default_to_none() {
    let mut s = sample();
    s.layouts = vec![Node::Split { axis: Axis::Column, ratio: 0.3, first: Box::new(Node::Leaf(0)), second: Box::new(Node::Leaf(1)) }];
    assert_eq!(decode(&encode(&s)), Some(s));
    assert_eq!(decode("{\"version\": 1}").unwrap().layouts, vec![]);
}
```

- [ ] **Step 2: Run** `cargo test restore`. Expected: compile error (no field).
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test`. Expected: all pass (+1), 0 warnings.
- [ ] **Step 5: Commit:** `git add src && git commit -m "feat(m8): saved sessions carry their split layouts"`

### Task 3: Split, close and show panes — ⬜

**Files:** Create `src/panes.rs` · Modify `src/workspace.rs`, `src/main.rs`.

**Interfaces:**
- Workspace gains `layouts: Vec<Node<EntityId>>` (keyed by each session's `view.entity_id()`), kept to the Layout invariant:
  - `open_session(launch, split: Option<Axis>, …)`: without a split, push `Node::Leaf(new)`; with one, `split` the active session's tree at the active leaf. The new session is inserted in the sidebar by `groups::insert_index` as before and becomes active. Every existing caller passes `None`.
  - `close_at`: before removing, if the closed session's tree has other leaves, pick `tree.cycle(closed, false)` as the session to activate next; then `remove` the leaf (drop the tree if `None`), remove the session, and activate the picked one (else the list's own fallback).
- Actions `SplitRight` (`cmd-d`) and `SplitDown` (`cmd-shift-d`): a new session in the active session's project (no preset), split `Axis::Row` / `Axis::Column`.
- `panes::render_layout(...)` renders the tree that contains the active session:
  - A `Leaf` is the session's view in a `relative()` box. When it isn't the active session, add an `absolute()` full-size overlay filled with the theme background at 30% alpha (the dim). The overlay must not block mouse events to the terminal.
  - A `Split` is a `flex_row` (Row) or `flex_col` (Column) with the first child sized `ratio` and the second `1 - ratio` (e.g. `flex_basis(relative(r))`, no grow/shrink, `overflow_hidden`), and a 1 px divider in `UiColors.sidebar_border` between them. Task 4 makes it draggable.
  - Clicking a pane selects its session (`select`), even though the terminal also handles the click (use the capture phase or a listener that doesn't stop propagation).
  - The tree is passed by reference with a lookup from `EntityId` to the session's view. `panes.rs` holds no state.
- Sidebar: a session row whose session is in the shown tree but isn't active gets `UiColors.row_hover` as its background (the active one keeps `row_active`).
- Pane menu (Global Constraints) with Split Right / Split Down now. The remaining items arrive in Task 4.

- [ ] **Step 1:** Implement. No new unit tests (GPUI); the tree is Task 1's.
- [ ] **Step 2: Run** `cargo build`, `cargo test`. Expected: 0 warnings, all pass.
- [ ] **Step 3: Commit:** `git add src && git commit -m "feat(m8): split right/down, closing a pane, dimmed unfocused panes"`

### Task 4: Moving between panes, dragging dividers, equalizing — ⬜

**Files:** Modify `src/panes.rs`, `src/workspace.rs`, `src/main.rs`.

**Interfaces:**
- Actions `FocusPaneLeft/Right/Up/Down` (`cmd-alt-left/right/up/down`) → `tree.neighbor(active, dir)`; `PrevPane`/`NextPane` (`cmd-[` / `cmd-]`) → `tree.cycle(active, false/true)`; `EqualizePanes` (`ctrl-cmd-=`) → `tree.equalize()`. Each selects the resulting session (nothing if `None`).
- Divider drag:
  - Capture the layout area's bounds each frame (a `canvas` prepaint writing into an `Rc<Cell<…>>`, as `terminal_view.rs` does for its grid).
  - The divider gets a wider invisible hit area (about 6 px, centred on the 1 px line) with `cursor_col_resize` for Row and `cursor_row_resize` for Column.
  - Mouse down on a divider stores `(layout id, path)` in the workspace.
  - Mouse move with the left button held → `ratio = (pos - split.x) / split.w` (Row) or `(pos - split.y) / split.h` (Column), with the split's area from `tree.split_rect(path, area)`, then `set_ratio` and notify.
  - Mouse up (anywhere in the window) ends the drag.
  - The terminals resize themselves on the next prepaint.
- Pane menu complete as in Global Constraints.

- [ ] **Step 1:** Implement. No new unit tests (GPUI); geometry is Task 1's.
- [ ] **Step 2: Run** `cargo build`, `cargo test`. Expected: 0 warnings, all pass.
- [ ] **Step 3: Commit:** `git add src && git commit -m "feat(m8): focus panes by direction or order, drag dividers, equalize"`

### Task 5: Save and restore the layouts — ⬜

**Files:** Modify `src/workspace.rs`.

**Interfaces:**
- `save()` fills `layouts` by mapping each tree's `EntityId`s to the session's current index (`tree.map`). It's called as in M7, plus after split, close, a divider drag ends, and equalize.
- Restore:
  1. Open the saved sessions as in M7. Each becomes its own leaf meanwhile.
  2. Then `split_tree::sanitize(state.layouts, sessions.len())`. If `changed`, log `[fuxx] restore: some split layouts didn't match the saved sessions; those sessions open on their own`.
  3. Replace `layouts` with the sanitized trees mapped from index to the restored sessions' view ids.
  4. Then select the saved active session as in M7.

- [ ] **Step 1:** Implement. No new unit tests (format and sanitize are Tasks 1–2's).
- [ ] **Step 2: Run** `cargo build`, `cargo test`. Expected: 0 warnings, all pass.
- [ ] **Step 3: Commit:** `git add src && git commit -m "feat(m8): split layouts are saved and restored"`

### Task 6: On screen — 👀 ⬜

Done together with the M6b and M7 checks (your choice, 2026-10-27). Run `cargo run` and report.

- [ ] **Split:** ⌘D → two panes side by side, the new one focused and in the sidebar under the same group. ⌘⇧D in it → it splits into two stacked panes. Each runs its own shell; `tput cols` in each matches its width.
- [ ] **Focus:** clicking a pane focuses it, and the others dim slightly. ⌥⌘←/→/↑/↓ move to the pane in that direction. ⌘[ / ⌘] go through them in order.
- [ ] **Resize:** dragging a divider resizes both sides (the pointer is a resize arrow over it), and `tput cols` changes. It stops at about 10%. Pane → Equalize Panes (⌃⌘=) evens them out.
- [ ] **Sidebar:** the on-screen sessions have a faint highlight and the focused one the normal one. Clicking a session in another layout switches the whole area to that layout.
- [ ] **Close:** `exit` or ⌘W in a pane gives its space to the neighbour, which gets the focus. Closing the last session still quits.
- [ ] **Restore:** quit with ⌘Q and reopen → the same panes, splits and divider positions.
- [ ] **Unchanged:** M7 (groups, rename, reorder, presets, restore) and earlier.

### Task 7: Update the docs — ⬜

- [ ] Milestone note 48 ✅ with the checklist ticked; this plan's tasks ✅; roadmap M8 ✅; tracker.
- [ ] `STRUCTURE.md`: `split_tree.rs`, `panes.rs`.
- [ ] API Fact Sheet: flex-basis splits, `canvas` bounds capture for drags, resize cursors.
- [ ] Progress Log entry.

## Verification log

_(filled in as tasks complete)_

## Known limitations

- Hidden layouts' terminals re-fit when they're shown again (M5 behaviour: only on-screen views measure).
- No zoom, no moving panes between layouts (see the spec's "Not in M8").

## Related

- [[48 - Milestone 8 - Split Panes]] · [[40 - Roadmap]] · [[00 - Plan Tracker (Start Here)]]
