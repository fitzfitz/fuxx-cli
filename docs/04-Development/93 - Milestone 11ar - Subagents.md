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
