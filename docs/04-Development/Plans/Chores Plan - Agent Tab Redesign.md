# Agent Tab Redesign (chores after M11ab) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans (the project's milestone build mode: built in the session, one whole-branch review at the end). Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Tool rows open a detail modal (chevron, no inline dropdown); the agent tab's top bar becomes a live status pill + cost + context meter; then (part 2, spec B) provider logos, the chat redesign and quieter inactive tabs.

**Architecture:** Pure models and colour tokens first (unit-tested), then GPUI rendering on top. The modal is state on `AgentTab` (a stack of tool ids) drawn as an overlay inside the tab below its top bar, with its own focus handle and key context `AgentToolModal`. The top bar's mark is a `MarkView` on the Workspace's shared `MarkClock`, so only the mark redraws per tick.

**Tech Stack:** Rust, GPUI (pinned rev), the app's `status_mark` / `mark_view` / `ui_tokens` / `theme`.

**Spec:** the lead designer's specs A (tool chevron + modal + top bar) and B (logos, chat, inactive tabs), 2026-11-21, recorded in [[88 - Milestone 11ab - Chat-Style Shell Tabs]] "Chores: the agent tab redesign". Task 3's rows follow spec B-B's tool-row anatomy (status dot, groups) as well as spec A's chevron.

## Global Constraints

- Every colour is derived from the theme (`theme.ui()`, `Colors::of`), never hard-coded per theme; text ≥ 4.5:1 (`TEXT_MIN`), marks/lines ≥ 3:1 (`MARK_MIN`).
- No continuous redraw besides the shared 10 fps mark clock; animations are `with_animation`, keyed by change counters (never restarted by the tab's 1 s redraw), and off with Reduce motion.
- Cost stays exact (updates at turn end); context is live (commit `5ec0b41`).
- Provider-specific code stays in `src/providers/<agent>.rs`.
- Per-task gate: `cargo test --bin fuxx`, `cargo clippy --all-targets`, `cargo clippy --all-targets --features ui-shots` (at the baseline: 30 / 32 warnings), `cargo fmt --check`. No screenshot scenes per task; the full `scripts/ui-shots.sh` at the end.

## Review Focus

1. **Esc with the modal open** must go back/close and never stop the agent; ↑/↓ must not reach the composer's history. (Task 3 keymap test.)
2. **The opened tool vanishes** (restart, new conversation, history replaced): the modal closes, no panic on a stale id. (Task 3 test.)
3. **↑/↓ at the ends and inside a sub-step** walk only that level's openable siblings, no wrap. (Task 3 test.)
4. **A provider with no cost/context (agy)**: the bar's right side is empty, the pill still works. (Task 2 test.)
5. **Narrow panes**: delta, "Context" label and meter drop at 480 / 420 / 320 px. (Task 1 test.)

---

### Task 0: Live context ✅ (commit `5ec0b41`)

Each main-thread `assistant` line emits `Event::Usage { context_pct, cost_usd: None }` once the model's window is known (learned per model, process-wide, from `result`). Tests in `src/providers/claude.rs`.

### Task 1: Colour tokens and the top bar's pure model

**Files:** Create `src/agent_top.rs` (pure: pill, meter, cost delta, trimming). Modify `src/agent_tab.rs` (`Colors`), `src/main.rs` (`mod agent_top`).

**Interfaces — produces:**
- `Colors` new fields: `row_hover`, `well` (= `page`), `well_text`, `well_dim`, `meter_track`, `meter_ok`, `meter_caution`, `meter_critical`, `pct_caution`, `pct_critical`, `scrim: (Rgb, f32)`; `tool_name` becomes `readable(ui.accent, &[ui.panel, row_hover], TEXT_MIN)`.
- `pub enum Band { Ok, Caution, Critical }`, `pub fn band(pct: u8) -> Band` (<70 / 70–89 / ≥90).
- `pub fn meter_fill(pct: u8, width: f32) -> f32` (pct/100·width, ≥ 2 when pct > 0).
- `pub struct Pill { pub word: &'static str, pub fill: Option<Rgb>, pub word_ink: Rgb, pub mark: PillMark }`, `pub enum PillMark { Status(Status), DoneCheck }`, `pub fn pill(status: Status, stopping: bool, theme: &Theme) -> Pill` — values per spec A §C table (`PILL_TINT` 0.16 dark / 0.12 light; Done/Stopping neutral `mix(page, fg, 0.06)`; Idle no fill, word `c.dim`; "Working", "Needs input", "Done", "Stopping…", "Idle").
- `pub fn cost_delta(before: Option<f64>, now: Option<f64>) -> Option<String>` → `"+$0.42"` only when both exist and now > before.
- `pub struct Trim { pub delta: bool, pub label: bool, pub meter: bool }`, `pub fn trim(width: f32) -> Trim` (480 / 420 / 320).
- `status_word` returns the pill words (capitalised; "Needs input").

- [x] **Step 1: failing tests** in `agent_top.rs`: `band(69)==Ok, band(70)==Caution, band(89)==Caution, band(90)==Critical`; `meter_fill(0,56.)==0., meter_fill(1,56.)==2., meter_fill(50,56.)==28., meter_fill(100,56.)==56.`; `cost_delta(None,Some(1.))==None, cost_delta(Some(3.57),Some(3.99))==Some("+$0.42"), cost_delta(Some(1.),Some(1.))==None`; `trim(500.)` all true, `trim(450.)` delta false, `trim(400.)` label false, `trim(300.)` meter false; `pill(Waiting,false,..).word=="Needs input"`, `pill(Working,true,..).word=="Stopping…"`, `pill(Idle,..).fill==None`; a contrast test over every built-in theme: pill word on pill fill ≥ 4.5, meter fills on page and on track ≥ 3.0, `tool_name` on `row_hover` ≥ 4.5, `well_text`/`well_dim` on `well` ≥ 4.5, `panel_text` (chevron hover) on `row_hover` ≥ 4.5. Update `status_word` tests.
- [x] **Step 2:** run `cargo test --bin fuxx agent_top` → FAIL (module missing).
- [x] **Step 3:** implement.
- [x] **Step 4:** gate green. Commit `Chore: the agent top bar's model and the redesign's colours`.

### Task 2: Draw the top bar

**Files:** Modify `src/agent_tab.rs` (`top_line`, `AgentTab::new` takes `clock: Entity<MarkClock>`), `src/workspace_agent.rs:501` (pass the Workspace's `clock`), ui_shots constructors of `AgentTab`.

**Interfaces — consumes:** Task 1. **Produces:** `AgentTab { top_mark: Entity<MarkView>, state_changes: u64, cost_before: Option<f64>, delta: Option<String>, context_changes: u64, context_from: Option<u8> }`.

- [x] **Step 1: failing tests** (state on `AgentTab` through its event handling): after a turn ends with cost 3.57 then a turn with 3.99, the delta reads "+$0.42"; it clears when the next turn starts Working; the first-ever cost shows no delta; a new context value bumps `context_changes`, the same value doesn't; with no cost and no context (agy) neither group is drawn.
- [x] **Step 2:** run → FAIL.
- [x] **Step 3:** render per spec A §C: 32 px, `AREA_X` padding, pill 22 px r11 (pad 8/10, gap 6, META MEDIUM, fill hangs at `AREA_X − 8`), 12 px `MarkView` (`set` each render; Done → `done_check` + `pop_curve` over `POP`); state blend 200 ms ease-out-cubic keyed by `state_changes`; cost group (META MEDIUM tabular `c.text`, delta in `c.dim` 6 px left, fades in 200 ms) and a 16 px gap; context group ("Context" `c.dim`, 8, meter 56×4 r2 on `meter_track`, width animated old→new 400 ms keyed by `context_changes`, 8, pct min-w 32 right-aligned, band colours); tooltips "Conversation cost · updates when each turn ends" and "Context window used · updates as the agent works"; groups absent when their data is; `trim(width)` from the tab's last laid-out width.
- [x] **Step 4:** gate green. Commit `Chore: the agent top bar — status pill, cost, live context meter`.

### Task 3: The modal's state, navigation and keys; the chevron rows

**Files:** Modify `src/agent_tab.rs` (replace `expanded` with `modal: Option<ToolModal>`; `fn tool` header → chevron, whole-row hover, no inline body), `src/main.rs` (keymap: context `AgentToolModal`: `escape`→`ModalBack`, `up`→`ModalPrev`, `down`→`ModalNext`), the ui_shots that call `shot_toggle`.

**Interfaces — produces:**
- `pub(crate) struct ToolModal { pub stack: Vec<String> /* tool ids, last = shown */, pub opened: u64 /* animation key */, pub follow: bool }`.
- `fn openable(item: &Item, items: &[Item]) -> bool` (today's rule: diff, Bash command, output (not Read), or children).
- `fn siblings(items: &[Item], parent: Option<&str>) -> Vec<String>` (openable tools at that level, in order).
- `fn step(items: &[Item], modal: &ToolModal, dir: i8) -> Option<String>` (no wrap).
- `AgentTab::open_tool(&mut self, id: String, window, cx)` (focus the modal), `close_modal` (focus back to the agent box), `modal_back` (pop; empty → close), `prune_modal` (from `after_change`: drop ids no longer in `items`; empty → close).
- Actions `ModalBack`, `ModalPrev`, `ModalNext` in `agent_tab`.

- [x] **Step 1: failing tests:** `siblings` returns only top-level openable tools for `None` and a Task's children for `Some(task)`; `step` from the first with −1 is `None`, from the last with +1 is `None`, middle moves; `prune_modal` closes when the shown id is gone and pops to the parent when only the child is gone; keymap test (in `main.rs`' style): in `["Workspace","AgentTab","AgentToolModal"]` `escape` runs `ModalBack` (not `composer::Stop`), `up`/`down` run `ModalPrev`/`ModalNext`; ⌘K, ⌘T, ⌘W still reach fuxx.
- [x] **Step 2:** run → FAIL.
- [x] **Step 3:** implement; rows: `Icon::ChevronRight` in a 12 px slot (`right_sidebar::CHEVRON`), `panel_dim`, `panel_text` + `row_hover` fill on hover, empty slot kept when not openable; click opens the modal. `shot_toggle` becomes `shot_open_tool`.
- [x] **Step 4:** gate green. Commit `Chore: tool rows open a modal (chevron; navigation and keys)`.

### Task 4: Draw the modal

**Files:** Create `src/agent_tool_modal.rs` (the modal's drawing and its pure sizing), modify `src/agent_tab.rs` (an overlay child inside the tab's root, `absolute` below the bar).

- [x] **Step 1: failing tests (pure):** `modal_width(pane_w)`: 1000 → 720, 600 → 552, 340 → 324; `modal_max_h(pane_h)` = pane_h − 72; the header's right text per state (running → elapsed, diff → `+a`/`−r`, failed → result without "✗"); `follow` stays true when within 24 px of the bottom and goes false after scrolling up.
- [x] **Step 2:** run → FAIL.
- [x] **Step 3:** draw per spec A §B: scrim `shadow` α 0.35 dark / 0.25 light below the 32 px bar (click closes; bottom corners rounded with the card's radius); surface `overlay_panel`-style, radius 12, top 48 px, centred; header 44 px (pad 14/8, gap 10, divider) with Back (sub-step), status mark (running arc via a second `MarkView`; ok `done_check` `text_dim` + pop if it finished while open; failed `Icon::Octagon` `failed_panel`), name (UI MEDIUM `tool_name`), summary (prefix "Task › " in a sub-step), "N steps", right text, ✕ (28 px r8, 15 px, `panel_hover` on hover); body scroll (pad 16, gap 12) with labelled wells (KEYS MEDIUM `panel_dim`, 6 px above; well `page`, edge `panel_border`, r8, pad 10/12, mono 12, lh 1.45): Command, Changes (`diff_block`), Output ("… N earlier lines"; "No output yet" while running), Steps (`item_body` rows; a click pushes onto the stack). Open: scrim 140 ms ease-out, panel fade + 8 px rise 180 ms ease-out-cubic keyed by `opened`; close and content switches instant. Sticky bottom via the scroll handle. Copy buttons only if `md_reader`'s copy is a straight reuse; otherwise left out (say so in the note).
- [x] **Step 4:** gate green. Commit `Chore: the tool detail modal`.

### Task 5: `/clear` clears the chat (the user's bug report)

**Files:** `src/agent_tab.rs` (where a new conversation id in a running run is followed), `src/agent_conversation.rs`.

- [x] **Step 1:** superpowers:systematic-debugging — trace what `/clear` produces today (a later `Started` with a new
  id in the same run; the comment at "a later id in this run (an agent's `/clear`)"), and why the items stay.
- [x] **Step 2: failing test:** after a `/clear` (a second `Started` with a new id in one run) the conversation's
  items are emptied (a notice "Conversation cleared" may stay, as Claude's own `/clear` shows nothing else), the
  cost/context are reset, and the tab's `cleared` counter rises (it keys the clearing animation).
- [x] **Step 3:** implement; the stream fades out (opacity 1 → 0 over 220 ms) then shows the emptied list. agy: check
  whether it has `/clear`; if it reports nothing, say so in the note.
- [x] **Step 4:** gate green. Commit `Fix: /clear clears the agent tab's chat`.

### Task 6: Logos (spec B-A) and inactive tabs (spec B-C)

**Files:** Create `assets/logos/claude.svg`, `assets/logos/antigravity.svg`, `assets/logos/NOTICE.md`,
`src/logos.rs` (`pub enum Logo { Claude, Antigravity }`, `pub fn image(logo: Logo, size: f32) -> Arc<gpui::Image>`
cached per (logo, size), the SVG's root `width`/`height` rewritten to `size`). Modify `src/providers/mod.rs`
(`ProviderInfo.logo`), `claude.rs`/`agy.rs` (their logos), `src/tab_bar.rs` (`TabIcon::Agent` logo; `tab_idle`,
`tab_idle_hover`; the 0.75 inactive opacity on dark themes), `src/theme.rs` (`UiColors.tab_idle`,
`tab_idle_hover`, `drawn_pairs`), `src/agent_tab.rs` (`Look.logo`; avatar and empty state).

- [x] **Step 1: failing tests:** `logos::sized_svg(src, 14.)` sets the root's width and height to 14 and keeps the
  viewBox; `CLAUDE.info().logo == Some(Logo::Claude)`, `AGY.info().logo == Some(Logo::Antigravity)`; `tab_idle`
  lies between `card` and `card_hover` and the tab titles meet 4.5 on `tab_idle`/`tab_idle_hover` in every theme;
  `look_inactive_is_a_quiet_pill_with_faint_text` updated to `tab_idle`; inactive opacity 0.75 dark, 1.0 light,
  1.0 when active or hovered.
- [x] **Step 2:** run → FAIL. **Step 3:** implement. **Step 4:** gate green. Commit
  `Chore: Claude and Antigravity logos on agent tabs and in the chat; quieter inactive tabs`.

### Task 7: The chat redesign (spec B-B)

**Files:** `src/ui_tokens.rs` (`pub mod chat`), `src/agent_tab.rs` (rows, gaps, avatar, You bubble, tool groups —
on top of Task 3's chevron rows —, todo, permission, notice, error, thinking line, queued, empty state; `Colors`:
`you_fill`, `you_edge`, `you_text`, `you_dim`, `tool_ok`, `tool_divider`, `todo_track`, `error_fill`,
`error_edge`, `error_mark`, `error_text`, `queued_fill`, `queued_text`), `src/md_preview.rs` (`Sizes::CHAT`, no
H1/H2 underline without `reader`), `src/icons.rs` (Lucide circle, circle-dot, paperclip, list-checks, shield, info
if missing).

- [x] **Step 1: failing tests:** `group_shape(items, ix)` → Solo/First/Middle/Last over a run (children skipped);
  `gap_before(prev, item)` per the token table; `shows_thinking(status, last_item)`; `summary_shown(name, summary)`
  hides an equal summary; the row fingerprint changes when a row's shape changes or it stops being last; the new
  colour pairs meet their floors in every theme; `Sizes::CHAT`'s new values.
- [x] **Step 2:** run → FAIL. **Step 3:** implement (running tools' `MarkView`s made in `after_change`, keyed by tool
  id, dropped when they finish). **Step 4:** gate green. Commit `Chore: the agent chat redesigned`.

### Task 8: Docs, screenshots, review

- [x] ui-shots scenes: each pill state dark + light, meter at 30/75/95 %, the cost delta, an agy tab, narrow trimming; the modal for Bash running→done, failed Bash, Edit diff, Task steps (in, Back), split pane.
- [x] Full `scripts/ui-shots.sh`; one whole-branch review on the most capable model; one fix wave; the full run again.
- [x] Note 88 + Progress Log entry; the on-screen checklist (spec A §E, spec B-E).
- [x] Ask the user about committing the two brand logos to the public repo before the push (spec B-E 4).
- [x] Private-name grep of the outgoing diff, then push to master.

## Outcome (2026-11-21)

All tasks done on master (`5ec0b41`…`3f82e6c`), plus your two additions (usage beside the model pill, "Cultivating…").
Final review (opus): "with fixes"; one fix pass, each fix test-first. Full ui-shots 268 / 268. Details and the
deferred minors in [[88 - Milestone 11ab - Chat-Style Shell Tabs]].
