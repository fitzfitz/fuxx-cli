---
tags: [milestone, agents, design]
updated: 2026-11-22
---

# 93 - Milestone 11ar - Subagents

**Status:** 🟨 built 2026-11-22 (8 tasks, one whole-branch review and its fix pass), merged to master and **released
in v0.12.6** (2026-11-23, run 45070831970) with the chores below; waiting for your on-screen check (the checklist below). Plan: [[M11ar Plan - Subagents]] (revised on the three leads'
picks).
Subagents**, then M11as Agent browser, then M11at Browser testing (each designed on its own, later).

## Why

Your words (2026-11-22): "can we switch chat view between subagents?", "I want the full power of both claude and
agy", "I want future proof, scallable, easy to code and integrate if we want to add more features/tools", then "go
with A" and "go with yours" (the subagent's chat **replaces** the main one, switched by pills). Agents now hand work
to subagents all the time; today fuxx hides a Claude subagent's steps inside a tool modal and drops agy's subagents
entirely, so you can't see who is doing what.

## Goals

- **See every subagent** of Claude and agy: a row in the main chat with its name, task and live state.
- **Switch the chat view** to one subagent and watch its conversation live, then back to Main.
- **Control where the agent allows it:** stop one subagent (Claude); a Claude subagent's permission request says who
  asks.
- **Built to grow:** one shared subagent model; each agent's file translates its own signals and says what it can do;
  the screen shows only what the agent supports. A new agent, or a later agy (a stop, ACP subagents), is a change in
  that agent's file, not on screen.

## Not in it (and why)

- Typing to a subagent, a box per subagent, per-subagent model or mode (neither agent lets the host talk to a
  subagent; you talk to Main).
- Stopping one agy subagent (agy has no way for the host to do it; Stop stops the whole turn, as today).
- Answering agy subagents' questions (agy's headless mode skips them).
- agy swarms (`/swarm`, a preview), split view, auto-switching, toasts or sidebar badges for subagents, closing,
  pinning or reordering pills, a tree of nested subagents (they are flat pills in start order).
- Browser work: M11as and M11at.

## Approaches

- **A. One shared subagent model, a translator per agent (chosen, "go with A").** Each provider file turns its own
  signals into the same events (started, step, done) and declares capabilities; the UI reads only those. Scales to new
  agents and new abilities without UI changes.
- B. Move both agents to ACP now: the standard, but agy's ACP (1.5.14) shows less than its stream-json (a subagent is
  one opaque call; its permission request never arrived) and Claude has no native ACP. Revisit when agy implements
  ACP's draft subagents.
- C. Claude first, agy later: quicker, but not "both".

## What the agents give (researched 2026-11-22, real runs; Claude 2.1.330, agy 1.5.14)

| | Claude (`claude -p` stream-json) | agy (`agy -p` stream-json) |
|---|---|---|
| Started | `system` `task_started` (task_id, tool_use_id, description, subagent_type, is_backgrounded, prompt) | `step_update` `step_type: "subagent"`, `subagent_info.subagents[]` (type_name, role, initial_prompt; on DONE conversation_id, log_uri) — one step per subagent |
| Its steps | whole `assistant`/`user` messages with `parent_tool_use_id` = the Agent call (no typing stream) | not in the stream: its `transcript.jsonl` (at `log_uri`) grows one finished step per line while it runs |
| Progress / done | `task_progress` (tokens, tool count, last tool), `task_notification` (completed/failed/killed, usage) | its transcript stops; its report arrives in the parent as a `system_message` step (content only in the parent's transcript) |
| Stop one | control request `stop_task {task_id}` | none (the parent model's `manage_subagents` kill only) |
| Permission | `can_use_tool` carries `agent_id` | skipped in headless |
| Message it | via Main only | via Main only, after the turn |

The builder records fixtures from real runs (both agents) before parsing.

**agy, the timings that matter** (a recorded run): the `subagent` step goes DONE about 1.5 s after it starts, when
the subagent is **spawned**, not finished, and that DONE already carries `conversation_id` and `log_uri`; the
transcript then grew from 1 line (1 s later) to 16 (32 s later). So the live view follows the file from that DONE. The
parent's own transcript (the same place, under the stream's `conversation_id`) gets a `SYSTEM_MESSAGE` line
`[Message] … sender=<subagent conversation id>` when a subagent reports; a killed subagent's transcript just stops (no
end marker).

## Rulings (made while writing this; yours to overrule)

- **The label** (my call): agy: its `role` ("Workspace File Reader"); its `type_name` is the type. Claude: a custom
  `subagent_type` (anything but the built-ins `general-purpose`, `Explore`, `Plan`), else the call's `description`
  (Claude keeps it to a few words); `subagent_type` is the type. The mockups' "Reader" is only an example.
- **How an agy subagent ends** (my call): **done** when its report reaches the parent (the parent transcript's
  `sender=` matches the lane's conversation id — the key for parallel subagents); **stopped** when the turn is stopped,
  the process ends (Force stop, a restart for a model or mode change — agy kills its subagents with it, verified), or
  the parent's turn ends while it still runs without a report. agy lanes don't show "failed" (agy gives no such
  signal); Claude lanes do (`task_notification`).
- **When the process ends or restarts** (my call): any working lane of that process becomes **stopped** and its
  follower stops. Claude's background subagents outlive the turn inside the same process: fuxx keeps reading after the
  turn's `result`, and their `task_notification` ends them.
- **Order** (my call): on your approval this note goes on the [[40 - Roadmap]] and into [[12 - Scope]] as M11ar;
  M11aq's on-screen check stays pending alongside, as earlier milestones' checks do (batched), not blocking the plan.

## The design (the Lead UI/UX Designer's spec, 2026-11-22)

### The model (provider-neutral)

- A **lane** per subagent: id, label, kind (type), task, status (working · waiting on you · done · failed · stopped),
  started, stats (tokens, tools, last tool) if the agent reports them, its items (the same `Item`s as Main's).
- The view: **Main** or **a lane**.
- Each provider declares three capabilities: `stop_subagent`, `subagent_asks`, `subagent_stats` (Claude: all three;
  agy: none). The UI shows Stop, the "wants to run" heading, and stats only when they're on.
- Provider files own the translation (Claude from its stream; agy by following the subagent's transcript file from
  `log_uri`), per "provider code in provider files".

### The pill row

```
                       $0.36  Context ▮▯▯ 4%       <- top bar, unchanged
   [Main ✓]  [◜ Reader]  [◉ Tester]  [✓ Lint]  +3    <- 32-tall strip, sticky
   ─ the column (≤ 760, centred), left edges aligned ─
```

- Its own strip under the top bar, aligned to the column's left edge; doesn't scroll. Shows once the first subagent
  exists (also on resume) and stays after they finish.
- Pill: a status mark in the tool row's 12 pt slot, then the label — the agent's custom type if named
  (`code-reviewer`), else the task description, ≤ 20 characters with an ellipsis; a repeat gets " 2". Tooltip: type,
  task, elapsed. Height and radius of the top bar's status pill, `text::META`.
- Marks (the tool row's, the shared clock): working arc `ring_working`; waiting mark plus the pill tinted like "Needs
  input"; done check `ring_done`; failed dot `failed_panel`; stopped hollow ring `text_dim`.
- Active pill: the top pill's neutral fill with `text` ink; others no fill, `text_dim`, `side_tab_hover` on hover.
  Main first (the main agent's mark), the rest in start order.
- Overflow on one line: "+N" opens a tab-menu popup (mark, label, elapsed); the active pill always in view.
- Keys: ⌥⌘[ / ⌥⌘] previous / next pill (agent tab context). Esc stays Stop.

### In Main's chat

```
 ◜ Reader  Find the config loader      Read src/cfg.rs · 1:02 ›
 ✓ Tester  Run the test suite                    14 steps · 2:10 ›
```

- The subagent's tool row: label (`tool_name`), task (`panel_text`, truncates), the last tool while running or "N
  steps" when done (`panel_dim`), elapsed. Click or › switches to its view (replacing the modal for subagent rows; the
  modal's sub-agent steps section goes; other tools keep their modal). agy's subagent rows are new (today dropped).

### The subagent view (replaces Main's column; the pill row stays)

```
 Reader · Explore · Sonnet           1:02 · 18.2k tok · 12 tools  [Stop]
                         ╭ from Main ────────────────────────────╮
                         │ Find where config is loaded and ...   │
                         ╰───────────────────────────────────────╯
 ✳ I'll start with the entry point.
 ┃● Read  src/main.rs                                    210 lines
 ┃◜ Grep  load_config                                         0:03
   Thinking…
 ✓ Done in 1:42 · 18.2k tokens · Back to Main ›
```

- Header: label SEMIBOLD `text`, type and model `text_dim`; elapsed, tokens, tools at the right (`text_faint`,
  tabular), each only if reported. **Stop** (the existing chip) only with `stop_subagent`.
- The task prompt as the first bubble, captioned "from Main". Then the same rows as Main; new steps append (Claude's
  whole messages, agy's finished steps); "Thinking…" between steps while it runs.
- End: "✓ Done in … · Back to Main ›" (to its row in Main) · "✕ Failed · reason" (`failed_panel`) · "Stopped".
- **The agent box** stays (its status is Main's). Placeholder "Message Main — it can pass this to Reader". **Sending
  switches the view to Main** so your bubble shows. ■ / Esc stop the whole turn.
- **A Claude subagent's permission request:** today's permission card, headed "Reader wants to run Bash", at the end
  of whichever view is active (Enter = Allow works); Reader's pill takes the waiting tint; Main's notch "Needs input".
  The view never switches by itself.
- Background subagents that outlive the turn keep their pills working; Main shows Done.

### Sizes and themes

- Wide: strip and header inside the centred 760 column. Under ~480: labels ≤ 12 characters, "+N" sooner, the header
  keeps elapsed and Stop only. Light vs dark: only the waiting tint's strength (as today).

## Left to the builder

The exact event and struct names; how agy's transcript is followed (a watcher or a poll, off the UI thread, stopping
when the subagent ends or the tab closes); recorded fixtures for both agents (subagent runs, parallel subagents, a
stop, a permission request); what a resumed conversation restores (lanes from Claude's history and agy's
transcripts); scenes for the strip, the rows, the view, the card, narrow and wide, both themes.

## Checklist for you (once built)

1. Ask Claude to use two subagents: two pills appear, each working; their rows show in Main.
2. Click a pill: its chat shows live; Back to Main or ⌥⌘[ returns.
3. Stop on a Claude subagent: only it stops; Main goes on.
4. A Claude subagent asks to run a command: the card names it; Allow works from any view.
5. The same with agy: pills, rows and live views appear; no Stop on its pills.
6. Type while viewing a subagent: the view returns to Main with your message.

## As built (2026-11-22)

- **The model** (`agent_conversation`, `providers/mod.rs`): a lane per subagent, keyed by the call that started it;
  events `SubagentStarted` (with per-lane `LaneControls { stop }`), `SubagentProgress`, `SubagentEnded`, `Follow`;
  `Permission.by`; `Action::StopSubagent { lane }`; `Translator::decode_followed`. A turn's settling leaves a live
  lane's steps and questions alone; a gone process (exit, Force stop, a restart) stops its lanes.
- **Claude** (`providers/claude.rs`): `task_started` (only `local_agent` tasks: a subagent's own Bash is a
  `local_bash` task), `task_progress`, `task_notification`; `can_use_tool.agent_id` (= the task id) names who asks;
  `stop_task` stops one; a foreground lane still open at `result` ends Stopped, a background one goes on; resumed
  conversations get their lanes back from the Agent calls in the transcript (label by the live rule).
- **agy** (`providers/agy.rs`, `lane_follow.rs`, `agent_lanes_view.rs`): its `subagent` step is a row and a lane
  (`<step id>#<n>`); spawned, its transcript is followed (a GPUI task per lane, 500 ms, for the process's life) and
  decoded by agy's translator; its report (`send_message` to this conversation) ends it; at `result` an unreported one
  is Stopped. No Stop (agy offers none).
- **The tab** (`agent_tab.rs`, `agent_rows.rs` — the row drawing moved out first, `agent_lanes_view.rs`,
  `agent_chat.rs`): a list per view (`rows_of`), the pill strip (Main, then lanes in start order; marks; waiting tint;
  "+N" list; ⌥⌘[ / ⌥⌘]), Main's subagent rows from their lanes (label, task, last tool or "N steps", time; a click
  opens the lane; the tool modal's Steps section is gone), the lane view (header with kind, model, time, tokens, tools
  and Stop; "from Main"; its steps; "✓ Done in … · Back to Main ›", "✕ Failed · why", "Stopped"), a question in every
  view at its end, headed by who asks, the box's "Message Main — it can pass this to …", sending returns to Main.
- **Fixtures** (recorded, scrubbed by `scripts/record-agent-fixture.py`'s new scenarios): `claude-subagents`,
  `claude-subagent-stop`, `claude-subagent-ask`, `agy-subagents` and its two transcripts; a test keeps any real home
  folder out of the fixtures.
- **The review** (one whole-branch review, Opus): fixed a Critical (Force stop or a restart left lanes working — and a
  question open forever) and six Important (resumed lanes without a result, stopped lanes revived by late lines,
  clocks after a stop, Claude's foreground lanes at the turn's end, a question at the end of the shown lane, steps
  with no lane staying in Main). Deferred minors: ↑/↓ in a modal opened from a lane step; followers polling finished
  lanes; small wording and state nits (the plan's ledger lists them).
- **Checks:** 2,448 tests; clippy at the baseline; 7 new scenes (`agent-lanes-strip`, `-overflow`, `-view`, `-ask`,
  `-narrow`, `-agy`, `-states`); the full ui-shots run before and after the fix pass.

## Chores after M11ar

- **Working while subagents run** (your words, 2026-11-22: "the status indicator is false. it says done … but in
  reality there are subagents running under the hood"): the tab shows Working — its mark, the sidebar, the status
  pill ("1 subagent running"), and closing asks first — while any subagent still runs after Main finished
  (`AgentTab::shown_status`; Main's own `status` still drives Stop, notes and sending). Scene
  `agent-lanes-background`.
- **Bigger tab marks, a green Done** (your words, 2026-11-22: "the loading and the checklist is too tiny and make the
  checklist have backgrounds (green background)"): a tab's status mark is 12 pt (was 8) in the ×'s slot; Done is the
  check on a `ring_done` disc, cut out in the tab's fill. Tab widths unchanged (the old room kept). Scenes
  `tabs-minimal` (Done now green) and `tab-drag` (its chip check samples clear of the mark under it).
- **A welcome in an empty shell tab** (your words, 2026-11-22: "on empty terminal (cleared or new terminal tab) can
  you add sentences and icons on top on the middle like in claude/agy on empty"): while the card view's column holds
  only the prompt (`card_view::is_empty_column`), its middle shows the terminal's mark, "Run a command" and the
  folder, as the agent tab's empty state; it takes no clicks. Scene `shell-cards-empty`.
- **Claude's tab stuck on "Cultivating…"** (your report, 2026-11-22: "still loading when no background task is
  running … when I esc the status stuck on stoping"; Antigravity was fine): a note sent mid-turn is read by Claude in
  that same turn — one `result` for two messages (recorded: its echo inside the turn, one result) — so a message
  stayed counted as unanswered and Main read Working; Esc then waited on a turn that wasn't running. Claude now runs
  with `--replay-user-messages`: each message counts as read on its echo (`Event::Taken`); an agent with no echo
  (agy, which queues each as its own turn) still counts one per `result`.
- **The deferred minors, done** ("go"): ↑/↓ in a tool window opened from a subagent step walk its steps; a finished
  subagent's file is read every 5 s (500 ms while it works); a lane with another open question still waits; the
  number stays on a cut label ("Workspace… 2"); a refused stop says "Couldn't stop the subagent: …"; the box's hint
  after `/clear`; gone lanes' lists dropped; tokens on the Done line ("✓ Done in 1:42 · 18.2k tokens").
- **The detached-daemon test** waits 10 s for its fake daemon (it raced the app's 2 s under the full suite's load).
- **Inactive tabs' marks faint** (your words, 2026-11-22: "inactive tab status indicator, should be almost transparent
  (0.20)"): a tab not shown draws its status mark at 0.2 (`tab_bar::INACTIVE_MARK`); the shown tab's is whole, and a
  waiting tab keeps its tint. Scene `tabs-minimal`.
- **"Divining…", newest first** (your words, 2026-11-22: "if subagents are working, change cultivating to divining
  … if main is working while subagents working as well, cultivating wins … for subagents list on top, make sure it
  sorts by newest"): the status word is "Divining…" while only subagents work (Main's own work says
  "Cultivating…"); the pills after Main go newest first (`agent_lanes::shown_order`), as do ⌥⌘[ / ⌥⌘] and the "+N"
  list (the oldest fold away first). Scene `agent-lanes-background` shows "Divining…".
- **Tests clean up their temp folders** (your words, 2026-11-22: "make the tests clean up their temp folders"; about
  98,000 old `fuxx-*`, `fuxxc-*` and `fuxxd-*` folders had piled up in `$TMPDIR`): every test and ui-shots scene makes
  its scratch through `test_temp::dir`, inside the run's own folder `$TMPDIR/fuxx-t/<pid>`, removed when the run
  exits (only by the process that made it, not a forked child); a run that was killed leaves its folder for the next
  run to sweep. A test fails if any other file names the system temp folder (the app's pasted-image folder and a few
  paths nothing is made in are allowed). The old folders are not touched: delete them by hand.
- **A new agent tab read "Cultivating…"** (your report, 2026-11-23: "the status on new agent tab is cultivating even
  when nothing running"): the stuck-"Cultivating…" fix counted the agent's start-up (`Started`) as Main working, and
  agy says `init` as soon as it launches, before anything is sent (recorded: one `init` line, nothing else, with no
  message). Main now counts as working only from what was sent, its echo, and its own words and tools.
- **A smaller build folder** (your call, 2026-11-23: "go with your pick"; `target/debug` had grown to 74 GB in a day
  and a half): on macOS a dev build keeps its debug info in the compiler's object files, so every rebuild left a new
  set of 256 beside the binary (123,000 of them). `[profile.dev]` now keeps only line tables (none for
  dependencies; `--profile debugging` brings full debug info back), and `scripts/prune-target.sh` — run at the end
  of `scripts/ui-shots.sh` and after a gate — deletes the object files no current binary uses. Measured from clean:
  a full build's folder 2.9 → 1.9 GB, one set of object files 193 → 124 MB, a rebuild after an edit 4–5 → 3–4 s,
  the full build unchanged (66 s); after pruning a panic's backtrace still shows file and line, and the next build
  is "Fresh". At a milestone's end, `cargo clean` if `target` is over 30 GB (note 50).
- **The graph screenshot's made-up history was real** (your word, 2026-11-23: "go for it", after I found it while
  building the landing site): the `changes-graph` scenes' commits (since M11w) carried a real author's name and real
  commit messages copied from a work repository. They are made up now (author "Sam Lee", neutral messages, tag
  `v0.4.0`), and so is the name in [[74 - Milestone 11w - Graph]]'s example. The six fingerprints were rewritten
  (`changes-graph`, `-card`, `-closed`, both themes). The old strings are still in the pushed history: rewriting
  that needs a force-push, so it waits for your word.
- **Chat text can be selected and copied** (released in v0.12.7; your report, 2026-11-23: "on agent chatbox, I cannot block texts then
  do a copy or any other stuff that can be done to any texts normally"): the chat is drawn with GPUI's
  `StyledText`, which has no selection, and the chat never had one of its own (the terminal, file tabs and shell
  cards each built theirs). New `src/chat_select.rs`: every piece of chat text (your messages, and the agent's
  paragraphs, headings, list items, table cells, quotes and code) is wrapped in a `SelectableText` that paints the
  selection's wash (`ui.selection`, as the box's) behind it and, once painted, registers its layout for the tab's
  mouse to hit. What you get: drag to select across messages (the chat scrolls on its own above or below it);
  double-click a word, triple-click a paragraph, ⇧-click to extend; a press on no text clears; the text cursor over
  text (a link keeps its pointer); ⌘C and Edit → Copy copy it; ⌘A with an empty box selects the whole chat. ⌘C
  and ⌘A still act on the box when it has a selection or text (the box hands them on only when it has nothing to
  act on), and a selection begun in the chat drops the box's, so ⌘C copies what you just picked. Each view (Main,
  each subagent) keeps its own selection.
  - **AI's calls:** pieces of one message copy one per line, messages apart by a blank line; tool rows, the todo
    card, notices and the tool modal are not selectable (not prose); no right-click menu (fuxx's menus are workspace
    overlays and the terminal has none either); the Markdown reader in file tabs is unchanged (`select: None`); a
    selection reaching rows that never came into sight copies only what was drawn.
  - **On screen:** drag across a reply → wash, ⌘C, paste elsewhere; double-click a word; ⌘A in an empty box; type
    in the box, ⌘A/⌘C there still act on the box; a link in a reply still opens.
  - **Checks:** 6 new unit tests (`chat_select`), a new ui-shots scene `agent-select` (double-click word, ⌘A,
    clear, drag across heading and paragraph, the box's own ⌘C), watched failing with the hook off.
- **`chrome-agent-tabs` / `chrome-agent-status` were flaky** (your word, 2026-11-23: "try the failed … again"): they
  failed 5 runs in 6 with 3–28 bytes "stale". The differing pixels were all ±1 in one channel, inside the
  "Cultivating…" row (the avatar and the word): its fade-in's last frame. The agent tab is drawn cached, and it can
  keep the frame just before the fade ends (≈ 99.7 % opacity) while a full redraw draws 100 %. The fade
  (`agent_chat::thinking_opacity`, tested) now lands exactly on full for its last few percent, so any frame kept
  from its tail equals the last one: 10 runs in 10 pass, and all 92 `agent` scenes.
- **"Built with Rust and chaos"** (your word, 2026-11-23): the README's badge now reads *built with · Rust · Chaos*
  (`assets/badges/built-with-rust-n-chaos.svg`, its last segment widened for the longer word; the old badge file
  removed), and the landing site's footer says the same.
- **⇧Tab changes the agent mode** (released in v0.12.8; your words, 2026-11-23: "on agent tab, I want to add shortcut shift+tab to change
  agent mode … fast as chore"): ⇧Tab anywhere in an agent tab, the message box included, chooses the next mode
  (Claude: Ask before edits → Accept edits → Plan → Auto → back; agy: Accept edits → Auto → Plan → back), the same
  way the mode chip's menu does (`agent_box::next_mode`, the `agent_tab::CycleMode` action). Listed in Keyboard
  Shortcuts as "Next Agent Mode".
  - **AI's calls:** with no mode known, or one not in the list, it goes to the first; pressed twice before the agent
    answers, it moves on from the mode asked, not the one shown; a tab with no modes ignores it; left out of the
    command palette (the palette has the focus, not the tab; the chip is the way in).
  - **Checks:** unit tests for `next_mode` and the binding (in the tab and in the box); `agent-box-mode-menu` presses
    ⇧Tab twice and sees `auto` then `default` asked live.
- **Agent tabs start in Auto** (your words, 2026-11-23: "agent tab: mode use auto for both agy and claude"): a new
  Claude or agy tab opens in Auto (`ProviderInfo::default_mode` `auto` in both provider files; agy's argv falls back
  to `auto` too). Claude gets `--permission-mode auto`.
  - **AI's calls:** a restored tab keeps the mode it had; a mode agy doesn't have falls back to Auto (it was Accept
    edits).
  - **Checks:** the provider tests now pin Auto, and a new one checks Claude's argv has `--permission-mode auto`;
    `agent-tab-agy` starts on Auto and chooses Plan → Accept edits → Auto, each one a real change.
- **New Fuxxpace lost its preset rows** (your words, 2026-11-23: "remove this on New Fuxxpace"): the sidebar's +
  menu is New Fuxxpace and New Fuxxpace in Folder…, nothing under them.
  - **AI's calls:** ⌃⌘1–9, ⌘K and Welcome still launch presets; the menu's width and its two rows are unchanged;
    `agent_badge` stays (the pane bar's new + menu draws it).
  - **Checks:** `sidebar-menu`'s reference refreshed; the scene that checked the removed rows' badges
    (`agent-preset-menu`) is gone, its checks moved into `new-tab-menu`.
- **A pointing hand on image pills** (your words, 2026-11-23: "in agent chat input, make the image attachment when
  user hover, it becomes pointer hand icon"): over an `[Image #N]` pill in the message box the cursor is a pointing
  hand, and the pill's fill deepens (mixed 0.20 toward the text instead of 0.12) while the pointer is on it. A click
  still opens the image.
  - **AI's calls:** no underline, tooltip or motion; a pill split over two rows lights both halves; each pill's
    rectangle is worked out once per frame (`composer::atom_spans`), for its fill and its hitbox, and only a move
    that changes which pill is under the pointer redraws.
  - **Checks:** a unit test for `atom_spans` (an atom split over two rows, atoms off the row, two on one row);
    `agent-box-image-chip` checks the pill is drawn inside the typing in the pill's colour. The cursor itself can't
    be seen by a scene.
- **The pane bar's + opens a new tab menu** (your words, 2026-11-23: "on the panel + button, I want to design it like
  this"): a click on + opens a menu under it (+ lit while it's open), its search box ready. With nothing typed:
  Terminal, Browser and File buttons, then a row per agent preset (logo or badge, a preset not on your PATH greyed
  "Not installed"; none in an SSH workspace). Typing lists Open URL (the address, or a web search), Agents, New,
  Tabs, History and Files. ↑↓ and the pointer light one thing at a time, ←→ move along the buttons, Enter runs it,
  Escape clears then closes (`new_tab_menu.rs`).
  - **AI's calls:** no chevron or submenu, no "Agent settings…" row (Preferences has no Agents section), no gear and
    no simulator; the ⋯ menu stays as it was; file search goes to ⌘K on Files (the files row hands it the query,
    the File button opens it empty); the light starts on whatever ⌘T would make, so Enter with nothing typed is ⌘T,
    and the row ⌘T would open says ⌘T; a preset without a provider (opencode, codex) opens as a terminal tab
    running it; the PATH check runs off the UI thread and is asked again on open only after 30 s or when the
    presets change; history is read off the UI thread the first time; at most 8 agent rows show before they scroll.
  - **Checks:** 12 unit tests for the menu's model (rows, order, the light, ↑↓ ←→, the PATH cache) and one for its
    keys; scenes `new-tab-menu` (both themes: placement, + lit, logos, the greyed row, pointer and keys, Escape,
    opencode/codex's path), `new-tab-menu-search` (tab, history and URL rows, the files row opening ⌘K), a
    `tab-bars-plus` case, and the `overlays` scene opening it among the other boxes.
- **⌘T makes a tab like the last one** (your words, 2026-11-23: "when cmd + t, it will open a new tab based on last
  tab…"): after a terminal, a terminal (SSH as before); after a Claude or agy tab, the same agent in that tab's
  folder; after a browser tab, a browser tab; after a file tab, or with an agent fuxx doesn't know, a terminal.
  ⇧⌘T stays a terminal in the workspace folder.
  - **AI's calls:** ⌘T carries the agent tab's model and effort, and resets the mode to Auto; browser → browser;
    file → terminal; "the last tab" is the focused pane's shown tab; the new tab goes at the end of the bar.
  - **Checks:** unit tests for `new_tab_menu::plan_like` (every kind of tab); `new-tab-like-last` presses ⌘T after
    Claude, agy, browser, file and terminal tabs, ⇧⌘T after Claude, and Enter on the empty + menu.
- **Claude's prompt suggestions** (your words, 2026-11-23: "can I get the agent suggestion on what to type? just like
  in claude cli, it has prompt suggestions"): after a turn, Claude's guess at your next message shows faint in the
  empty box with a faint "⇥" after it, in place of the placeholder; Tab takes it into the box (the caret at its end)
  and Enter sends it. Typing hides it, emptying the box brings it back. Claude Code 2.1.330 sends it as a
  `prompt_suggestion` line about a second after a turn's `result` when started with `--prompt-suggestions true`
  (`Event::Suggestion`; the fixture `claude-prompt-suggestion.jsonl`). A switch in Preferences → General →
  Interface: "Prompt suggestions — Claude guesses your next message; Tab takes it." (`agent-suggestions = on|off`).
  - **AI's calls:** on by default, and fuxx also sets `CLAUDE_CODE_ENABLE_PROMPT_SUGGESTION=1` on Claude's process
    (recordings showed the guess rarely came without it), both from Claude's provider file
    (`suggestion_flags`, `suggestion_env`; agy keeps the empty defaults); the switch applies to agents started after
    the change, and a preset that names the flag wins. **Enter on the empty box never sends the guess**: only Tab
    takes it. A guess is kept only while Main is idle (no turn, nothing queued or unread, no permission waiting),
    else dropped as stale; any send, a message read (`Taken`), Main at work again, `/clear` and a restart clear it; a
    tab switch keeps it; it never shows in a lane view or with a permission waiting; it is never saved. Tab with the
    `/` or `@` list open still completes (its binding comes after the box's).
  - **Checks:** unit tests for the decode (and a blank guess), Claude's flag and env vs agy's none, the env reaching
    the process, `with_suggestions` (on, off, a preset naming the flag), `keeps_suggestion`, `ghost_for`, the
    setting and the keymap; scenes `agent-box-suggestion` (both themes: the ghost and "⇥" drawn, no placeholder,
    typing hides it), `agent-box-suggestion-tab-takes` (Tab fills, Enter sends, Enter on the empty box sends
    nothing, Tab in the `/` list completes), `agent-box-suggestion-stale` (dropped while working, `Taken` clears,
    a lane view hides it and Main shows it again, a permission hides it, a restart clears it),
    `agent-box-suggestion-flags` (argv and env on, off, with a preset's flag, agy); the `preferences` reference
    refreshed for the new switch.
- **↑/↓ bring back what you sent** (your words, 2026-11-23: "when I focus on chatbox, and I hit up or down, can it
  pick by the prompt history?"): in an agent tab's box, ↑ (⌃P) on the first row recalls the older prompt, the caret
  at its end, so a multi-line one walks its rows first; ↓ (⌃N) on the last row comes back, and past the newest puts
  your unsent draft back. Editing a recalled prompt ends browsing. The tab's own prompts come first (a resumed
  conversation's too), then the folder's stored ones, without repeats (`prompt_history.rs`).
  - **AI's calls:** history is per folder and kept in a file, `~/Library/Application Support/fuxx/prompt-history.jsonl`
    (`-dev` for a test build), one line per send appended off the UI thread, read and compacted once at app start
    off the UI thread (until then only the tab's own prompts); at most 100 prompts per folder and 40 folders (the
    least recently used go first), a prompt over 8,000 characters isn't kept, a repeat moves to the front, the file
    is compacted once it holds twice what is kept. Only what goes to the agent is kept (messages, notes, `/commands`
    it runs), not fuxx's own commands or permission answers; images are stripped (`[Image #N]` markers out, the
    spaces tidied, an image-only prompt skipped). The `/` and `@` lists keep ↑/↓ while open; **↑ doesn't recall with
    a permission waiting**; recall works mid-turn and in lane views; with a guess showing, ↑ recalls and the guess
    hides, and ↓ past the newest brings the empty box and the guess back. A recalled `/command` keeps the `/` list
    shut (as Esc would), so ↑/↓ go on through the history; ↓ back to your own draft lets its list open again.
  - **Checks:** unit tests `recall_older_newer_and_draft_restored`, `recall_skips_an_entry_equal_to_the_box`,
    `editing_a_recalled_prompt_ends_browsing`, `end_of_history_stays`, `record_moves_a_repeat_to_front_and_caps`,
    `image_markers_stripped_and_image_only_skipped`, `oversized_not_kept`, `own_tab_first_then_folder_deduped`,
    `jsonl_round_trip_and_garbage_ignored`, `compaction_keeps_newest_per_folder_and_lru_folders`, the file path, and
    `first_and_last_row_follow_wrapping` (each also shown to catch a planted fault); keymap and shortcut-list tests;
    scenes `agent-box-history` (two prompts, rows walked, the draft back, the `@` list keeps ↑, no recall with a
    permission, the marker stripped, a recalled `/command` leaves the `/` list shut — watched failing first) and
    `agent-box-history-folder` (seeded with `set_for_shots`, no file touched).
- **The ⋯ menu's New Terminal Tab lost its "⌘T"** (a fix after "⌘T makes a tab like the last one"): ⌘T follows the last
  tab now, so the hint was wrong.
  - **AI's calls:** the item keeps its place; ⌘D and ⇧⌘D still show.
  - **Checks:** `the_menu_lists_the_pane_actions_with_their_keys` updated (watched failing first).
- **Your latest message pinned over the chat** (your words, 2026-11-23: "can you also on chatbox add the floating
  latest chat on top, when clicked, it goes to that chat. just like in claude code so that its easy to know what I
  prompt when the agent is running"): while your latest message is scrolled above the chat, one line of it floats
  over the chat's top edge: a card in the panel's fill with its 1 pt edge and a soft shadow, a tool row tall, an ↑
  mark and your words (whitespace folded, cut with "…"), a step dimmer than in your bubble. A step lighter and the
  pointing hand under the pointer, tooltip "Jump to your message". A click glides the chat back until your message
  is at the top and lights its bubble for 0.6 s; the strip then hides. On screen, or nothing said yet: no strip. A
  new message replaces it at once. In a subagent's view it says the task Main gave it (its "from Main" bubble).
  New `src/agent_latest.rs`.
  - **AI's calls:** it overlays the chat and never moves a row; a band of the page colour behind it, fading out
    under it, so no row shows as a cut-off sliver above it. A message counts as off screen once what is left of it
    would sit under the strip. Visibility comes from the list's scroll top and your message's row: the conversation
    keeps your latest's index as items come and `sync_rows` keeps its row and its one line (built once per message).
    The check runs before the list lays out, so a canvas after the list checks again and asks for one more frame
    when they differ (output pushing your message off while the chat follows its end). The click stops following
    the chat's end, as a scroll up does; scrolling back to the end resumes it (GPUI's own follow). The glide is
    0.22 s and covers at most the last 1.5 screens (from further down it jumps that close first); the strip fades in
    over 0.12 s. With reduce motion (or Animations off) it jumps without gliding, appears without a fade and the
    message isn't lit. fuxx's icon set has no image glyph, so `[Image #N]` stays as text; the ↑ is the box's own
    send arrow. One line keeps at most 200 characters before GPUI cuts it to its width.
  - **Checks:** unit tests `the_latest_you_follows_what_is_pushed`, `the_strip_line_is_one_line_cut_with_an_ellipsis`,
    `the_strip_line_keeps_image_markers_as_text`, `the_strip_shows_only_while_your_message_is_above_the_view`,
    `the_jump_highlight_holds_then_fades_to_nothing` and `the_jump_glides_at_most_the_last_stretch` (each watched
    failing first); scene `agent-chat-latest-prompt` (both themes: no strip before a message or while it is on
    screen; output scrolls it off and the strip says its folded line, the chat not moved; the panel's fill, lit
    under the pointer, its tooltip; a real click glides back, your message at the top, the strip gone, following
    stopped; the wheel to the end follows again and shows it again; a lane's view says its task and its click
    works; a second message replaces the line). `agent-tab-done` now shows the strip ("Now fix notes.txt" is
    scrolled off); it keeps no reference.
- **The ⋯ is gone; Split Right and Split Down sit by the right sidebar's button** (batch 3, chore 1; your words,
  2026-11-23: "on three dots accross tabs, remove it, and make the split right and down out of that and appear beside
  right sidebar toggle"): every pane's bar ends in two buttons, Split Right (⌘D) and Split Down (⇧⌘D), Lucide's
  `square-split-horizontal` and `square-split-vertical` (24 pt, 14 pt glyph, faint until hovered, then `text` on
  `card_hover`, never lit). A click focuses that pane, then splits it. On the top-right pane's bar the right
  sidebar's button follows, 8 pt after them (`SPLIT_TO_TOGGLE`; 4 between the two splits). `src/pane_actions.rs` is
  deleted: the menu, its overlay, its field and its render; the + menu already makes terminal, browser and agent
  tabs.
  - **AI's calls:** a pane under 320 pt wide hides the splits (`chrome_cache::HIDE_SPLITS_BELOW`; ⌘D and ⇧⌘D still
    work). The width comes from the pane area as last painted and the layout's ratios (`narrow_panes`), part of the
    bar's cached key (`BarPlace::narrow`), so only a change of narrowness draws a bar again; a resize asks for one
    more frame only when the area's size changed. The palette's New ⟨Agent⟩ Tab rows now come straight from the
    providers. The unused ⋯ icon (`ellipsis.svg`) went with it.
  - **Checks:** unit tests `the_buttons_are_plus_the_splits_and_the_right_sidebar`, `the_pinned_buttons_follow_the_bars_place`,
    `a_pane_under_320_pt_is_narrow` and `every_bar_button_action_is_a_palette_command` (watched failing first); the
    pane-menu tests and the palette parity test are gone with the menu. Scene `tab-bars-more` became
    `tab-bars-split` (the left bar's Split Right hovered, Split Down not, both icons drawn); `tab-bars-act` clicks
    the left bar's Split Right and the top-right bar's Split Down (each splits its own pane, the new one focused);
    `tabs-minimal` follows the new buttons (a narrow pane keeps only the right sidebar's); `agent-pane-menu` is
    gone; `overlays` and `chrome-agent-tabs` no longer open the ⋯; `right-sidebar` reads the other pane's last button
    as unlit (narrow there, its splits hide).
- **One selection blue on every theme** (batch 3, chore 3; your words, 2026-11-23: "What I block text on any
  place, make the blue lighter", then "should it be blue apply everywhere? because its like a default color for
  text blocking" → yes): one colour for every selection (the boxes, the chat, the file tab, the terminal), on
  every theme whatever its accent: `theme::selection_wash`, macOS's own text selection blue — #3f638b on a dark
  card (was a grey slate, #384451 on the default theme), #b3d7ff on a light one — deepened toward the system blue
  in 5 % steps where a light page is too pale for it to show (Latte: #aad2ff), eased toward the card only where the
  text would read under 4.5:1; always visible (1.25:1) on the page, the box fill and the sidebar. The terminal's
  selection is that same blue on every built-in and plugin theme (no more white block on the default theme); only
  a `selection-background` in your own config still wins.
  - **AI's calls:** the lead's fix after wave 1: the first build followed each theme's accent (mauve on Catppuccin, brown
    on Gruvbox); you asked for blue, so it is now a fixed blue. A selected glyph keeps its colour where it reads (4.5:1) on the wash, else takes the
    chrome's text: in the terminal from a table made once per theme (`theme::selected_ink`, the 16 colours and the
    foreground), in the file tab by recolouring the selected stretch's runs only (no new layout); the boxes already
    draw `text`, which reads at 7:1. The chat keeps its colours (recolouring would lay it out again). A theme's own
    `selection-foreground` goes with its background; yours stays.
  - **Checks:** unit tests `the_selection_is_one_blue_on_every_theme` (every theme blue, the floors, the targets; watched failing on Gruvbox Dark's
    brown #6b401d),
    `terminal_selection_is_the_chrome_selection_unless_you_set_one`, `a_plugin_themes_selection_background_is_replaced`,
    `a_selected_glyph_keeps_its_colour_only_where_it_reads`, `selected_glyphs_keep_their_colour_only_where_it_reads`
    (the grid) and `selected_text_takes_its_ink_only_inside_the_selection` (the file tab), each watched failing
    first. New scene `selection-colors` (both themes: a terminal drag and the rename box, each in `ui.selection`,
    the grey selected text in `text`). The screenshot themes now come from `theme::resolve`, as your config's do.
- **Rewind and Fork: the provider and conversation core** (batch 3, chore 2; your words, 2026-11-23: "when I just
  prompt and suddenly stop it, I want to have the rewind/fork feaeture like in antigravity IDE/claude code"). Core
  only, UI in wave 2: each message goes with an id fuxx mints (Claude's `uuid`, `Action::Send{id}`), its echo marks it
  read (`Event::Taken{id}`, `Item::You{id, taken}`), a resumed conversation's messages keep theirs (`Past::You`);
  `Start.branch` starts Claude with `--resume S --resume-session-at P --fork-session --session-id N` (the old
  transcript untouched); `Provider::branch_point` reads P (the message's `parentUuid`) off the UI thread;
  `Conversation::cut_before` / `restore` cut and put back; Claude runs with
  `CLAUDE_CODE_ENABLE_SDK_FILE_CHECKPOINTING=true` (`Provider::process_env`, which replaces `suggestion_env`);
  `Action::RewindFiles` / `Event::FilesRewound` and the one-off `rewind_files_probe`; `Attachment::Quote` for wave 3;
  agy reports no branching. 14 unit tests, each watched failing first. Nothing on screen yet.
- **One right-click menu, on the terminal and in file tabs** (batch 3, chore 4, wave 2a; your words, 2026-11-23: "on
  chatbox or terminal or whenever when its an hyperlink I can right click it and add option "Copy Link". On text, I can
  right click it and has "Copy" on it. On agent tab, add more option "Add to chat""): a right-click (or ⌃-click) in a
  terminal or a file tab's text opens fuxx's own menu at the pointer, flipped up or left where it wouldn't fit, 8 pt
  from the window's edges. On a link: Open Link · Open in Default Browser · Copy Link, a line, then the rest. A
  terminal's: Copy ⌘C (and Add to ⟨Agent⟩ Chat with a selection) · Paste ⌘V · ─ · Select All; a file tab's: Copy ⌘C
  and, with a selection, Add to ⟨Agent⟩ Chat. The selection stays, and the link right-clicked stays underlined while
  the menu is open. ↑/↓ and the pointer move one light (nothing is lit on open), ↩ chooses, Esc or a click outside
  closes it, and a right-click elsewhere opens it again there. **Keyboard access** (a UX gap): ⇧F10, or a PC
  keyboard's menu key, opens it at the terminal's selection or caret, under a file tab's selection or current line, or
  on the Files tree's selected row. New `src/context_menu.rs` (the items, the light, the size and placement: pure),
  `src/workspace_menu.rs` (one overlay, `Overlay::ContextMenu`, in `Overlays`, so browser pages hide under it).
  - **AI's calls:** standard edit items show disabled when they don't apply (Copy with nothing selected), context items
    (a link's, Add to Chat) are left out; the menu is the panel's surface, 220 wide, items 26 tall, no icons; Move to
    Trash's colour is a new `menu_danger` (the waiting colour held to read on the menu and its lit row). In a program
    that takes the mouse (vim, htop), a right-click is the program's and ⇧-right-click opens fuxx's; ⌃-click is the
    program's there. Add to Chat names the agent the selection would go to (`ask_pick`: the last-used agent tab in the
    workspace, else the first; none opens a Claude tab, as Ask does) and is left out in an SSH workspace; from a
    terminal it is a quote chip (`Attachment::Quote`, "Quoted from Terminal api"), from a file tab the code chip Ask
    makes (path and lines). A right-click elsewhere while the menu is open: the menu closes and, two frames on (once
    the window is drawn without its backdrop), the press goes to what is under the pointer. The old tab menus stay as
    they were. The menu key's character (`NSMenuFunctionKey`) is bound as GPUI passes it through; no Mac keyboard has
    one, so it is untested. **Wave 3 adds** the chat's and the message box's menus (Select All in the chat, Cut/Paste in
    the box) and a link's menu in the Markdown Preview (today the Preview offers Copy, disabled).
  - **Checks:** unit tests for the item sets (text, terminal, file and folder rows), the arrows skipping separators and
    disabled items, the height and the placement's flips, `opens_menu` (right, ⌃, ⇧ with mouse reporting), the
    terminal's `select_all`, `menu_danger` on every theme, and the keymap (⇧F10 and the menu key in the terminal, file
    tab and tree; ↑↓↩Esc in the menu), each watched failing first but the keymap test (written with its bindings).
    Scenes `context-menu-terminal` (both themes: the
    selection kept by a right-click on a link, the link's menu, its underline, ↓ lights, Esc gives the keys back,
    ⇧F10, ⌃-click, a right-click elsewhere opens it there, Copy, Copy Link, Select All; then Add to Claude Chat makes a
    Claude tab with the quote chip) and `context-menu-file-tab` (Copy disabled with nothing selected, Copy and Add to
    Claude Chat with a selection, ⇧F10, the code chip); `overlays` opens it among the other boxes.
- **Links open in a browser split on the right, as Preferences says** (batch 3, chore 6, wave 2a; your words,
  2026-11-23: "when link is clicked on fuxx-cli, it will open a new browser tab on split pane on right", then "open
  link to browser, make it configurable on settings"): Preferences → Browser → "Open links in": **Split on the right**
  (the default) · Tab in this pane · Default browser (`open-links-in = split|tab|browser`). It applies to ⌘-clicked
  terminal links, the file tab's Preview links and the menu's Open Link. A split: the first link splits a browser pane
  to the right of the pane you clicked in; the next ones are new tabs in that pane (the workspace's last link pane,
  else its most recently used browser pane, else a right neighbour holding a browser); a URL already open there shows
  its tab. Your focus stays where you clicked, and a reused pane's new or found tab pill flashes once (0.6 s; none with
  reduce motion or Animations off). `mailto:` goes to the OS (Mail). New `src/link_route.rs` (`link_place`, pure;
  `Workspace::open_link`). **UX gaps built:** bare `http(s)://…` addresses and `localhost:port` in an agent's replies
  are links (not in code); file paths in a terminal's output (`src/main.rs:42`, an absolute one, `~/…`) are
  ⌘-clickable into a file tab at that line, resolved against the terminal's folder, only when the file is there
  (looked up off the UI thread, at most 256 answers kept), underlined with ⌘ held once known to be a file; hovering a
  Markdown link (the chat, the Preview) shows where it goes in a tooltip after the usual delay.
  - **AI's calls:** a pane under 560 pt isn't split (the link opens as a tab in it); "the same URL" ignores a trailing
    `/`; only http, https and mailto ever open (`links::allowed`), as before; the chosen tab's page loads without
    taking the keyboard; SSH workspaces behave the same, and `fuxx browser open`'s own `browser_place` is unchanged; a
    path in an SSH session's output opens nothing (the path is the host's); the file opens in the terminal's pane (not
    where links go), its line at the top and current; bare-URL linking is the agent chat's only (the Preview keeps
    CommonMark's `<…>` and `[…](…)`); the terminal's ⌘-underline stays its hover signal (no tooltip there: the link's
    text is its address unless the program wrote an OSC 8 link). **Wave 3 adds** the agent chat's links routed through
    `open_link` (today they open in your default browser, and its file links do nothing).
  - **Checks:** unit tests for `link_place` (mail and the default browser to the OS; the first link splits, a narrow
    pane takes a tab; the next goes to the last link pane, the same URL selects, a closed one is found again by its
    browser; the most recently used browser pane, never the source; "a tab in this pane" reusing its own tab),
    `same_url`, the setting and its values, the path finder (`path_ref` with line and column and trailing punctuation,
    `resolve_path`, a path found under a grid point), the bare-URL pass (prose, a list, a quote; code spans, code
    blocks, written links and non-addresses left alone) and `link_under` (a link's tooltip), each watched failing first
    (the setting's written values were tested with their code; the path tests by disabling the functions after).
    Scene `link-opens-split` (⌘-clicks: the split on the right with your focus kept, a tab in it for the next, the same
    URL shown and flashing, mail to the OS, a path to a file tab at line 42, "Tab in this pane", "Default browser").
    The `preferences` pages show the new row (its reference not refreshed: no scene shoots the Browser page).
- **Right-click in the Files tree** (batch 3, chore 7, wave 2a; your words, 2026-11-23: "add right click on each item
  on file explorer"): a row's right-click (or ⌃-click) selects it, gives the tree the keyboard and opens the shared
  menu. A file: Open ↩ · Open to the Side · Show Changes (a changed file only) · ─ · Reveal in Finder · Copy Path ⌥⌘C ·
  Copy Relative Path ⌥⇧⌘C · ─ · Rename… · ─ · Move to Trash ⌘⌫. A folder: New File… · New Folder… · ─ · Open in Terminal
  · Reveal in Finder · Copy Path · Copy Relative Path · ─ · Rename… · ─ · Move to Trash. Rename and New are a box in the
  row (Rename selects the name without its extension; ↩ makes it, Esc leaves it; a clash, a `/` or an empty name says
  so under the box). Move to Trash asks first in a native sheet ("Move “main.rs” to the Trash?", "You can put it back
  with ⌘Z or from the Trash in Finder.", Move to Trash · Cancel); **⌘Z in the tree** puts the last item trashed this
  session back (a UX gap). The keys work while the tree has the keyboard (not in its filter or Rename box). New
  `src/files_menu.rs`.
  - **AI's calls:** Open to the Side uses the focused pane's right neighbour (its tab of the file, else a new one), else a
    new split on the right, shown and focused; Show Changes opens the file's Diff; Open in Terminal is a terminal tab in
    the focused pane; Reveal runs `open -R` off the UI thread; the trash is Finder's own (`NSFileManager`
    `trashItemAtURL`, which says where the item went, for ⌘Z), off the UI thread, and ⌘Z never puts it back over
    something now at its place; the clash check reads the tree's own listing, the renaming and making happen off the
    UI thread (a name taken meanwhile still says so); a made or renamed entry is selected and its folder listed again;
    a new file is made empty and not opened; no permanent Delete. **Wave 3 adds** Add to Chat (a path pill in the
    `ask_pick` agent tab): the item is built (`file_tree_menu`'s target) but left out until pills come
    (`files_add_target` is the seam).
  - **Checks:** unit tests for the menus, the stem (`main` of `main.rs`, `archive.tar` of `archive.tar.gz`, a dot-file
    and a folder whole), the name problems, where the New box's row goes, the trash and put back (kept apart, never over
    a new one; in a temp folder), the sheet's words, `FileTree::has_entry` / `is_open`, and the tree's keys (in the tree,
    not in its boxes), each watched failing first but the sheet's words and the keys' test (written with their code).
    Scene `files-menu` (both themes, on a folder of its own; never the
    repo or yours; its "Trash" is a folder of the scene's, so your Trash is never touched): a marked file's menu and a
    folder's, Copy Path, ⌥⇧⌘C, ⇧F10 on the selected row, Reveal, Rename on disk (the stem replaced), a clash under the
    box and Esc, New File, New Folder, Move to Trash and ⌘Z, Open in Terminal, Open to the Side; shot with Move to
    Trash lit in `menu_danger`.
