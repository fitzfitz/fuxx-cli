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

- A box per subagent, per-subagent model or mode (you talk to Main). **Typing to a subagent moved into scope on
  2026-11-24** (your words: "Why I cannot send a message to a currently running subagent?" … "yes do both. also make
  sure I cannot send a message when subagent is done"): Claude's Main passes your message on with SendMessage (a
  relay) to a subagent running in the background; a finished one can't be messaged (the closing bar). See "The
  lanes chore" under "Chores after M11ar". agy still can't (its Main has no way in).
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

Everything below up to the 2026-11-24 "browser opened from a terminal" chore is **released in v0.13.0** (2026-11-24, run 45303453320).
The chores after it, through the agent activity view and its reviews, are **released in v0.13.1** (2026-11-24, run 45339699037).
The paste chore after that is **released in v0.13.2** (2026-11-24, run 45352171051).
The long-chat performance chore after it is **released in v0.13.3** (2026-11-24, run 45367550134).
The chore batch of 2026-11-24 and its four minors (below) are **released in v0.13.5** (2026-11-25, run 45538971154).

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
- **Rewind and Fork from your messages** (batch 3, chore 2; core in wave 1, the UI in wave 2b; your words, 2026-11-23:
  "when I just prompt and suddenly stop it, I want to have the rewind/fork feaeture like in antigravity IDE/claude
  code"): hovering one of your messages shows **Copy**, **Rewind** and **Fork** left of its bubble (Copy only until
  the agent has read it). **Rewind** goes back to before it: a turn running stops first ("Stopping…", forced after
  10 s), the chat is cut there, its text and attachments come back in the box, and Claude goes on in a branch of the
  conversation (`--resume S --resume-session-at P --fork-session --session-id N`; the old transcript is never
  touched). If no file changed after it (Claude's dry run says), that is one click and "Rewound · **Undo**" (Undo, until
  your next send, puts the chat back on the old conversation). If files changed, a native sheet asks: "Rewind to this
  message?", "The agent changed 2 files after it (+12 −4). Undo those changes too, or keep them and rewind only the
  chat.", **Rewind and Undo Changes** · Rewind Chat Only · Cancel; the files go back through Claude's own checkpoints,
  never fuxx's ledger. If the agent can't undo them, the sheet says so ("… can't undo file changes from fuxx, so they
  stay as they are. You can revert them in Changes.", Rewind Chat · Cancel). **After a stop** the notice reads
  "Stopped · **Rewind to edit your message**", that message's actions stay shown until you send, and the empty box
  says "Esc twice to rewind". **Esc Esc** (box empty, Main idle) rings your newest message: ↑/↓ move, ↩ rewinds, ⌘↩
  forks, ⌘C copies, Esc leaves. The palette has **Rewind Last Prompt**. **Fork** opens a new tab right after the
  source, shown and focused, titled "⟨source⟩ (fork)" ("(fork 2)" for a fork's fork), with the chat before the
  message, the message unsent in its box, and a first row "Forked from **⟨source⟩**" (the name shows that tab while it
  is open) over "Shares the folder with ⟨source⟩: both agents can edit the same files."; its tooltip's second line
  says "Forked from ⟨source⟩ at “…”". The source goes on untouched. **agy** (no rewind or fork headless) gets
  **Edit** only: the text back in the box, the chat as it is, the tooltip pointing at `/rewind` in Open in Classic
  View. New `src/agent_rewind.rs`; `Icon::Fork` (lucide 0.460.0 `git-fork.svg`).
  - **AI's calls:** the actions are transparent at rest, not hidden (GPUI registers no group hover on a hidden
    element, so they would never appear); 24 pt buttons, 14 pt icons, 2 apart, 6 from the bubble, `card_hover` under
    the pointer; Edit's icon is `file-pen`. The pick's ring is 2 pt `focus_ring` with the padding a point less, so
    nothing moves. Without a process to ask (it ended), the sheet says the files "may have" changed and the undo is
    Claude's one-off `--rewind-files`; a dry run unanswered within 10 s, or one that can't tell and lists nothing, is
    one click (chat only). A file undo that fails still rewinds the chat, with "Couldn't undo the file changes: …".
    Undo Rewind puts the chat back only (files undone stay undone). Until your next send the tab's saved conversation
    is still the old one (quitting first reopens the whole chat). A branch that ends before Claude says it started
    puts the old conversation back with "Couldn't rewind: …" and what you sent back in the box, never a fresh one.
    Task 0 is not done (your OK is still needed): if Claude names the branch another id than fuxx's, the tab takes
    Claude's. The fork's first row and tooltip line live for the session only (after a restart it is an ordinary tab
    on its own conversation), and name the source by its title at the fork. Rewind is hidden in a tab paused for its
    classic view. The first Esc in an empty idle box no longer reaches Stop (there was nothing to stop). The pick's
    keys are left out of Help's list (they work only while picking). `Conversation::restore` drops what came after the
    cut (the "Rewound" notice, a failed send) and the lanes started since. The latest-prompt strip's quick action
    ("can carry", in the spec) is not built.
  - **Checks:** unit tests for the hover actions (read, unread, no id, agy's Edit), their tooltips, the stopped link,
    the fork's title and tooltip and warning, the confirm rule (no change, changes it can undo, can't undo, no process,
    no answer), the pick's steps, Esc twice within 400 ms, `restore` after something was added, `forked_before`, the
    pick's keymap, agy's `edit_tip`, the palette's Rewind Last Prompt (an agent tab only), the fork's tooltip line, and
    `Icon::Fork`, each watched failing first (agy's default `edit_tip` passed at once). Scenes `agent-rewind` (both
    themes, shot after a stop: the link, the kept actions, the hovered ones, the placeholder; checks: the link's
    one-click rewind with the flags, Undo, the files sheet's words and the undo request, the next send taking the
    branch, Claude's own id adopted, a failed branch back to the old conversation, Esc Esc ↑ Esc ↩, the palette
    command, a rewind mid-turn stopping first), `agent-fork` (both themes: beside the source, focused, titled, the
    first row, the box, the flags, the tooltip, the source untouched, the name showing the source) and
    `agent-rewind-agy` (Copy and Edit only, Edit not cutting, no pick). Not yet seen in the real window.
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
    one, so it is untested. **Wave 3 built** the rest: in an agent tab's chat a right-click (or ⌃-click) opens the
    menu on what you selected and the link under the pointer (Open Link · Open in Default Browser · Copy Link, then
    Copy and Add to ⟨Agent⟩ Chat, which quotes the selection as a chip in that same tab's box, "Quoted from Claude");
    with nothing selected, Copy (disabled) and Select All ⌘A. The message box's right-click or ⇧F10 is Cut ⌘X · Copy ⌘C
    · Paste ⌘V · ─ · Select All ⌘A (Cut and Copy disabled with nothing selected; Paste takes an image as ⌘V does). The
    Markdown Preview's text can now be selected as the chat's is (drag, double-click a word, triple-click a
    paragraph), so its Copy and ⌘C work, and a right-click on a link offers the link's items. New `chat_menu` and
    `editable_menu`; `chat_select` keeps each piece's links (`link_at`, since `InteractiveText`'s clicks are the left
    button's only); `AgentTabEvent::ContextMenu` / `BoxMenu`, `ComposerEvent::ContextMenu`. AI's calls: a file link
    in the chat or the Preview offers no link items (a click opens it); ⇧F10 in the agent tab opens the box's menu
    (the chat has no caret to open at); the right-clicked chat link isn't underlined while the menu is open (the
    terminal's is). Checks: `the_chat_menu_adds_select_all_with_nothing_selected`,
    `the_box_menu_is_cut_copy_paste_and_select_all`, `link_at_finds_the_link_under_a_place`, the keymap (⇧F10 and the
    menu key in the box), each watched failing first; scenes `context-menu-chat` (both themes, shot with the menu on
    a link: Copy Link, a plain click opening the link in a split with your focus kept, Select All, Copy, Add to Claude
    Chat), `context-menu-composer` (Cut, Copy, Paste, Select All, ⇧F10) and `context-menu-file-tab` (now also the
    Preview's link menu and a double-clicked word copied).
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
    text is its address unless the program wrote an OSC 8 link). **Wave 3 built** the agent chat's links through
    `open_link` (`AgentTabEvent::OpenLink`): a web or mail link goes where "Open links in" says, and a link to a file
    (from the agent's folder, or a whole path) opens a file tab when it is one (`AgentTabEvent::OpenFile`). Scene
    `context-menu-chat` clicks one into a split.
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
    a new file is made empty and not opened; no permanent Delete. **Wave 3 built** Add to ⟨Agent⟩ Chat after Open to
    the Side (a file) or Open in Terminal (a folder): a path pill in the agent tab Ask would go to (`ask_pick` from the
    focused tab's workspace, else a new one; left out on another host). Scene `files-menu` now lists it (its
    reference is to refresh on the 1× display: one more row).
  - **Checks:** unit tests for the menus, the stem (`main` of `main.rs`, `archive.tar` of `archive.tar.gz`, a dot-file
    and a folder whole), the name problems, where the New box's row goes, the trash and put back (kept apart, never over
    a new one; in a temp folder), the sheet's words, `FileTree::has_entry` / `is_open`, and the tree's keys (in the tree,
    not in its boxes), each watched failing first but the sheet's words and the keys' test (written with their code).
    Scene `files-menu` (both themes, on a folder of its own; never the
    repo or yours; its "Trash" is a folder of the scene's, so your Trash is never touched): a marked file's menu and a
    folder's, Copy Path, ⌥⇧⌘C, ⇧F10 on the selected row, Reveal, Rename on disk (the stem replaced), a clash under the
    box and Esc, New File, New Folder, Move to Trash and ⌘Z, Open in Terminal, Open to the Side; shot with Move to
    Trash lit in `menu_danger`.
- **Files as pills in the agent chat** (batch 3, chore 5, wave 3; your words, 2026-11-23: "on file tree, I can drag
  and drop it to put to agent chat as a phill (same as image)"): drag a Files row onto an agent tab (its page, its
  box or its tab's pill in a bar) and it becomes a pill in the box: the path as the `@` list would insert it (from
  the agent's folder, else whole; a folder ends in "/"), editing as one (⌫ takes it whole), at the caret when the box
  has the keyboard, else at the end, with a space after. While files are over the page it is tinted, the box ringed
  in `focus_ring` and an empty box says "Drop to add to your message"; the dragged row follows the pointer as a
  28 pt chip with its icon and name, with the copy cursor. A pill's click opens its file tab (a folder's shows it in
  the Files tree). Sent, the message lists them as chips with the file icon (its name; two with the same name show
  their parent), each opening its file, its path the tooltip; the agent reads the paths where you put them in the
  text. **UX gaps built:** files dropped from Finder become path pills, images inline image pills as pasted ones
  (several files, several pills); files dropped on a terminal are typed as their shell-escaped paths, apart by spaces,
  as macOS terminals do. The tree menu's Add to Chat (above) adds one too. `composer::atom_ranges` now matches only on
  word edges (`src/main.rs` is not a pill inside `code/src/main.rs` or `src/main.rs.bak`), of two starting together
  the longer. New `src/agent_pills.rs` (`DraggedPaths`, `PathsChip`, `pill_text`, `pill_labels`, `spaced`,
  `pills_in`, `shell_escaped`, `typed_paths`), `Attachment::PathPill`.
  - **AI's calls:** the box shows the pill's whole inserted path, not the name only: the box draws its text as typed
    (a pill is a highlight over the text, as `[Image #1]` is with its brackets drawn clear), and showing other words
    than the text would mean a second text model under the caret and the IME; the name-only words and the parent on a
    clash are the sent chip's. The pill in the box has no tooltip (its path is in it). The copy cursor shows for the
    whole drag, not only over a target. A Finder drop's "is it a folder" is asked off the UI thread; dropped on a
    tab's pill the tab is shown first and the pills go at the end. A pasted image's brackets are still drawn clear; a
    path pill keeps every character. Finder drops on the box used to make file and image chips; they make pills now
    (the File… and Image… menu items still make chips). A rewound message's path pills come back as pills.
  - **Checks:** unit tests `an_atom_matches_only_on_word_edges`, `only_a_bracketed_atom_hides_its_brackets`,
    `a_pill_says_the_path_as_the_at_list_would`, `chips_say_the_name_and_a_parent_where_two_names_clash`,
    `pills_go_in_with_spaces_around_them`, `the_pills_in_a_text_are_found_on_word_edges_once_each`,
    `a_dropped_path_is_typed_shell_escaped` and `a_sent_path_pill_is_in_the_text_and_listed_as_a_chip`, each watched
    failing first. Scenes `files-drag-pill` (both themes, on a folder of its own; shot with a row held over the page:
    the tint, the ring, the words, the chip; checks: a file dropped on the page, a folder on the tab's pill, the
    menu's Add to Claude Chat, a pill's click opening its file tab, the sent message's three file chips and the paths
    in what Claude reads), `agent-box-finder-drop` (a file, a folder and an image from Finder: two path pills and an
    image pill) and `terminal-drop-paths` (the typed, escaped paths). Not yet seen in the real window.
- **Taking back a message the agent hasn't read** (batch 3, wave 3's UX gap; from "I want all the UX
  recommendations"): a message you sent while Claude works waits until it reads it; × beside its bubble (with Copy,
  while you point at it) takes it back. Claude Code 2.1.330 has the control request for it (`cancel_async_message`
  with the message's id, found in its binary; no model called): asked, Claude drops it from its queue and the bubble
  leaves the chat (no turn is waited on for it); if Claude had already taken it in, the bubble stays and a note says
  "Already read: it can't be cancelled". A message fuxx itself still holds (the agent not started yet, shown under the
  chat as "Starting Claude…") has a × too and goes before it is ever written, for any agent. agy can't take one back
  (its print mode has no such request), so its sent messages have no ×. Provider capability `cancels_queued()`
  (default false), `Action::CancelQueued`, `Event::QueuedCancelled`; `Conversation::cancel_unread`; new
  `src/agent_cancel.rs`.
  - **AI's calls:** the × is one of the bubble's hover actions (transparent at rest, as Copy is) and a "×" glyph (the
    icon set has no cross); a cancelled message's words don't come back into the box; an error answer (an older
    Claude) counts as not cancelled.
  - **Checks:** unit tests `an_unread_message_can_be_cancelled_and_a_read_one_cannot`,
    `a_provider_cant_cancel_a_queued_message_unless_it_says`,
    `cancelling_a_queued_message_asks_claude_and_its_answer_decodes` and
    `only_an_unread_named_message_can_be_taken_back_and_only_by_an_agent_that_can`, each watched failing first. Scene
    `agent-queued-cancel` (a held message dropped and never written; × on an unread note asks Claude with its id, the
    answer takes it out; a late one stays with the note; no × on a read message). Not yet seen in the real window.
- **The agent's question picker** (batch 3, chore 8, wave 4; your words, 2026-11-23: "we don't have open question
  popup yet" → "that open question, should we add it here as well as chore?" → yes): Claude's AskUserQuestion is a
  question card in the chat. Before, it fell into the generic permission card and Allow sent no answers ("The user did
  not answer the questions"). Now each question shows with its options, one at a time; a digit, a click, ↑↓ with
  Enter, or your own words typed in the message box (its "Other") answer it, and the answers go back keyed by each
  question's exact words (a multi-select's picks joined by ", ", a label holding ", " or a quote JSON-quoted, a chosen
  preview as an annotation); Esc twice skips it (a deny saying "The user skipped these questions."). Claude waits on
  it like a permission: the tab, its sidebar row and the banner say "Claude asks: ⟨question 1⟩ (+N more)". Answered,
  skipped or expired (the turn ended first), it leaves one line ("Answered · Test runner: Vitest · …") that opens to
  the full questions. A subagent's question makes its lane wait and shows in every view. A restored conversation
  replays a question as answered (its result exists) or not answered, never open. agy: no translator change — its
  print mode answers its own `ask_question` (skipped) and has no channel for yours; it joins the same event once a
  recording shows a shape. Provider `Event::Question`, `Action::Answer` (skip is `Action::Deny`); `Item::Question`,
  `Reply`, the `open_asks` cache; new `src/agent_ask.rs` (the card's keys, pure), `src/agent_ask_view.rs` (drawn),
  `src/ui_shots/agent_ask.rs`; the fixture `claude-ask.jsonl` is synthetic (marked so) until a recording exists.
  - **AI's calls:** the card takes the keys through a key context on the box (`AgentAsk > Composer`, as the rewind
    pick does) rather than a focus handle on the chat, so the box keeps its caret and letters type there; a single
    question's Enter sends at once; history marks an answered question with no answers (the transcript's result text
    isn't parsed); the Other text is never quoted; a question's tool result closes it (failed: not answered).
  - **AI's calls — the user's UX bar, 2026-11-23** ("make it easy to use and easy to access, feels like required and
    have the UX great! we make this different not like any others"), as the lead designer specified it:
    - It can't be lost: below its place, the same card (the same state and keys) docks over the chat's bottom — whole,
      compact (the question clamped, labels only, the options scrolling inside) or, in a short chat, its head only —
      with "Show in chat"; it swaps at the pixel the real card reaches, no fade while scrolling. The strip makes room
      in a short chat.
    - It feels required: "Claude is waiting for your answer" with the app's Waiting mark; the send circle is ✓ with
      "Answer with what you typed (⏎)"; the box's placeholder names the question; the `/` list stays shut (a `/` is
      answer text); one lap of the box rim in the waiting colour when it opens.
    - Easy to reach: "Answer Waiting Question" (⇧⌘A, free; in the palette only while a question waits) shows the
      oldest waiting question's tab with the keys on its card; a click on the tab or an agent's banner does the same;
      the latest-prompt strip's click brings the card into sight.
    - Different: the digit mark is the radio or checkbox and the key at once; "(Recommended)" shows as a tag with the
      cursor on it, the label sent unchanged; a review step after two or more questions (a missing answer lit first,
      "Send anyway"); step chips; a neutral light (`ask_cursor`) instead of red; new tokens `ask_cursor`, `ask_text`,
      `ask_mark`, `ask_mark_ink`, `ask_key_edge`, `ask_done` (in the contrast test); a younger second question waits
      as a quiet "Next · …" line.
    - Left out for now: previews side by side on a wide card (they sit under the options); the narrow-card rules
      (chips and hint hidden under 440/460 pt); "Show in chat" glides (it jumps); a queued permission's own small
      variant; an agent banner names no tab, so its click goes to the oldest waiting question, not that banner's tab.
      (The glide and the banner's tab: fixed in "Opens fixed (2026-11-23)" below.)
  - **Checks:** unit tests (each watched failing first, apart from a few written with their code, said so in the
    report): the translator's `ask_user_question_becomes_a_question_not_a_permission`, `its_tool_row_is_not_shown`,
    `an_answer_keeps_the_input_and_keys_answers_by_question_text`,
    `multi_select_joins_with_comma_space_and_quotes_labels_with_commas`,
    `other_text_is_the_answer_and_joins_picks_in_multi`, `a_chosen_preview_goes_back_as_an_annotation`,
    `skip_denies_with_a_message`, `an_unknown_id_writes_nothing`, `a_malformed_question_falls_back_to_the_permission_card`,
    `history_replays_a_question`; the model's `a_question_waits_and_answering_works_again`,
    `a_subagents_question_makes_its_lane_wait`, `turn_end_and_process_end_expire_it`, `its_tool_result_closes_it`,
    `history_never_leaves_one_open`, `oldest_open_ask_wins`, `open_asks_matches_a_full_scan`,
    `an_open_question_shows_in_every_view_at_the_end`; the card's 15 (`cursor_starts_on_one_and_enter_sends_it`,
    `digit_picks_and_advances_except_on_the_last`, `space_toggles_in_multi_and_enter_needs_a_check`, the review,
    Recommended, the guard, Esc Esc, Other, the summary, the dock's rule and kinds, the preview well); the keymap,
    the palette row and the tokens' contrast. Scenes `agent-tab-question`, `-multi`, `-preview`, `-review`,
    `-answered`, `-other`, `-lane`, `-restored`, `-dock` (both themes), `-keys`, `-skip`, `-typing-guard` and
    `agent-question-anywhere` (⇧⌘A, a banner click). Not yet seen in the real window.
  - **After the design review** (the lead designer looked at the real 2× shots; the user's bar again: "make it easy
    to use and easy to access … we make this different not like any others"). Fourteen fixes, most important first:
    1. The latest-prompt strip hides while the open ask's row is in sight with its top under the strip: the "waiting"
       line is never covered (`strip_covers_card`, the same `bounds_for_item` check the dock uses, re-checked after
       layout).
    2. Opening an answered line keeps it where it was: following the chat's end stops and the view is scrolled so the
       row's top stays put (before, the last row grew upward and the line went under the strip).
    3. A restored question keeps its answers: the result's text (`User has answered your questions: "Q"="A", …`) or
       its `toolUseResult.answers` is read back in claude.rs — each question matched by its words, a multi-select
       split on ", " against its labels, the rest your own words — and a skip (fuxx's deny message) restores as
       "Skipped", not "Not answered". New `Event::QuestionReplied`.
    4. A lane's ask goes by the subagent's type: "Explore is waiting for your answer · Read the test setup",
       "Explore asks: …"; "Open ⟨task⟩" stays in Main.
    5. "Recommended" has its own token `ask_tag` (Latte's was 3.3:1).
    6. Previews sit beside the options on a wide card (inner width ≥ 600: options 42 %, at least 240, gap 12), under
       them otherwise; the card's width comes from the chat's, laid out again on a resize.
    7. The preview well has its own fill `ask_well` (and `ask_well_text`); code doesn't wrap, so the fixed height
       holds; in the opened summary the well is the page with a 1 pt edge (it was lost on the panel in Mocha).
    8. "No preview" is an outline (`ask_key_edge`) at the same height, not a filled slab.
    9. The review's counter says "2 of 3 answered", in the waiting colour while one is missing.
    10. The review's placeholder reads "Send your answers with ⏎" (the caret covered the ⏎).
    11. Under 440 pt of card width only the current chip and "2 of 3"; under 460 pt no key hint.
    12. A permission behind an older ask is a quiet "Next · Claude wants to run Bash: npm test" line, not
        interactive, "Answer the one above first" (it becomes its card when it is the oldest).
    13. No light while the window isn't the key window; a change of key window draws the card again.
    14. A question with no header goes by its words (cut) in the summary, the review and the tooltips, and by its
        first three words and "…" on its chip.
    - Nits: "esc" in `ask_dim` on Skip; 2 pt between a multi-select's question and "Choose any that apply", 8 to the
      list; the opened summary's preview 20 pt in; a touch warmer light in light themes (floors kept); an opened
      summary shows no header line for a question without one.
    - The designer's second look ("ship once these are in"): the preview well clips inside its right padding and a
      line running past it fades out over 24 pt (from the well's colour to clear); a label ending a sentence joins
      its answer with a space ("Should I add a coverage report? No", not "?:"); the lane link in Main reads "Open
      lane", its tooltip "Show ⟨task⟩'s view" (the task isn't said twice); the ⊘ sits in a 12 pt box so "Skipped"
      lines up with "Answered" and "Not answered".
    - **AI's calls:** the queued permission says "wants to run ⟨tool⟩: ⟨what⟩" (the spec's "wants to run: npm test"
      would read oddly for a file tool); the scenes draw the window as the key window (the light shows only then).
    - **Checks:** 2662 → 2674 tests, each new one watched failing first: `the_strip_hides_while_it_would_cover_the_card_s_head`,
      `the_review_counts_the_answered`, `the_placeholder_names_the_question_and_the_review_s_enter`,
      `the_card_s_width_decides_its_layout`, `a_question_with_no_header_goes_by_its_words`,
      `a_lane_asks_by_its_type_and_its_task_follows`, `a_restored_answer_reads_back_its_picks_and_words`,
      `a_restored_answer_prefers_the_result_record`, `a_restored_skip_is_skipped`,
      `a_replayed_reply_closes_it_with_your_answers`, `a_label_ending_a_sentence_joins_its_answer_with_a_space`,
      `a_preview_running_past_the_well_fades_at_its_edge`; the contrast test gains `ask_tag` and the well. New scenes
      (both themes) `agent-tab-question-lane-main`, `-queued`, `-narrow`, `-anyway`, `-armed`, `-dock-tiny`,
      `-answered-line`, and the state check `-head-clear` (the strip over the head, the light in a back window; it
      and `-answered-line`'s "stays put" check were each seen failing with their fix taken out). References refreshed
      for the question scenes (their look changed on purpose) and written for the new ones; the full
      `scripts/ui-shots.sh` 327 passed, 0 failed. Not yet seen in the real window.
- **Opens fixed (2026-11-23)** (your words: "go fix the all the opens"; performance and ease of use win). Five items
  left open by chore 8 and the designer:
  - **The caret over the placeholder** (the lead designer: "Send your answers…" read like "$end"): as in a macOS
    field, a beam caret sits just before the placeholder, never over its first letter. In the agent box the
    placeholder (or the agent's guess) starts the beam's width and 1 pt right of it (`cursor_look::PLACEHOLDER_IN`,
    3 pt) whenever the cursor setting is Bar, focused or not, so the words never move; the hollow cell without focus
    moves with them. A block or underline stays on the first letter (your words, 2026-11-22). Every other field with
    a placeholder over its caret goes 3 pt in from the caret's start too: the filter boxes (⌘K palette, Help, branch
    switcher, host picker, new tab menu, find in file, the folder picker), the Files filter and the commit line (it
    was over the caret); the address field already was.
  - **A banner click goes to its own tab:** an agent's banner names its tab (`About::Tab`, request id
    `fuxx-t{tab}-r{run}-…`, the window's run); its click shows that tab with the card it waits on taking the keys —
    a question's card, or a permission's brought into sight with the box (its Enter allows) taking the keyboard. Only
    a banner about nothing, or about a tab gone since, goes to the oldest waiting question as before
    (`native_notify::landing`). Its banners replace each other as a terminal's do.
  - **Claude's `control_cancel_request`:** Claude Code 2.1.330 withdraws a pending ask ("Permission request withdrawn
    unanswered: the person pressed Stop, or the session is ending") with `{"type": "control_cancel_request",
    "request_id": …}` (at times with `takes_late_allow`, `tool_use_id`, `session_id`; found in its binary, no model
    called). claude.rs decodes it to `Event::AskWithdrawn { id }` (by the card's id: a permission's request id, a
    question's tool call); the card settles at once instead of at the turn's end. A question shows "Not answered · …"
    with "Claude stopped waiting before you answered."; a permission "Not answered · Bash npm test" with a dim circle
    and the same tooltip (`Answer::Withdrawn`). An answer to it afterwards writes nothing; an unknown id says nothing.
    The fixture `claude-withdrawn.jsonl` is synthetic (marked so) until a recording exists.
  - **"Show in chat" glides** to the chat's end with the latest-prompt strip's glide (`GLIDE_MS`, at most 1.5
    screens of it, the end chased as it grows), a jump with Reduce motion on, then follows the end again.
  - **Rewind on the latest-prompt strip:** after a Stop (while the "Stopped" notice offers "Rewind to edit your
    message"), the strip shows a 24 pt Rewind button (`Icon::Revert`, 14) at its right, "Rewind to edit this message",
    running the same rewind; only in Main, where the agent can rewind and the message was read, not in a paused tab;
    gone after the next send. Its click is its own (not the strip's jump).
  - **Screenshots the same on any display** (the last open: the shots came out at 1× or 2× by which display was
    main, so the references drifted and five scenes failed at 1×). Cause: GPUI sizes a window by its screen's
    `backingScaleFactor` and `open_offscreen_window` puts the window on the primary screen. The runner now makes every
    screen report 2× for its own process (`force_scale`, `-[NSScreen backingScaleFactor]` replaced; the shot is GPUI's
    own Metal render, so no real 2× screen is needed) and checks a 16 pt window's shot is 32 px (`check_shot_scale`).
    `UI_SHOTS_SCALE=1` shoots at 1× to check the scenes there (no fingerprints compared; `--update-refs` refuses it).
    The five 1× failures were the checks' own 2× assumptions, now scale-correct: `sidebar-band` sampled New's old fill
    at mid height, inside the `w` of "New" (its top lands higher at 1×), now above the words; `card-split` read the
    lower bar's line at a half point (drawn there at 2×, snapped to a whole pixel at 1×), now counts one line's pixels
    within 2 pt, and its hover check finds the quiet line's first pixel over the hairline too; `file-code-bars` expects
    a round end's corner pixel as GPUI's quad shader covers it at the shot's scale (`corner_cover`: none at 2×, 0.59 at
    1×) with the bar's edges snapped as GPUI draws them (`device_snap`); `file-tab-md-reader-column` expects lines of
    GPUI's pixel-snapped height (26.4 is 26.5 at 2×, 26 at 1×); `file-tab-header-narrow` expects Where's floor as laid
    out (`pixel_snap` of 12.5: 12 at 1×). Then a second cause of drift: since 2026-11-22 a scene's scratch sat in
    `$TMPDIR/fuxx-t/<pid>`, and scenes draw their scratch paths, so about 100 shots changed every run; a scene's
    scratch is now its own folder, `$TMPDIR/fuxx-t/shots/<hash of its name>`, the same every run in any process,
    held by a lock (`test_temp::begin_scene`; a concurrent run falls back to the pid folder). All 733 references
    re-recorded once at 2× (`--update-refs`, every scene: 181 folders, `files-menu`, `context-menu-chat` and
    `files-drag-pill` among them; 5,372,612 bytes, just over the C3 trial's 5 MB mark).
 the placeholder moves only for a beam (a block or underline is drawn on the letter, as you asked
    on 2026-11-22); a withdrawn question reuses "Not answered" (`Reply::Expired`, whose tooltip already said it), a
    withdrawn permission gets its own `Answer::Withdrawn` so a turn-end expiry keeps "Expired"; a banner-landed
    permission takes the box's keyboard (the box answers it) rather than a new key context.
  - **Checks:** 2674 → 2682 tests, each watched failing first: `the_withdrawn_fixture_says_it_is_synthetic`,
    `a_withdrawn_ask_settles_its_card_by_the_card_s_id`, `a_withdrawn_ask_can_t_be_answered_after`,
    `a_withdrawn_ask_settles_now_and_an_answered_one_keeps_its_answer`, `request_ids_carry_an_agent_tab_and_its_run`,
    `a_banner_click_lands_on_its_own_tab_and_falls_back_when_it_is_gone`, `a_glide_down_starts_within_the_last_stretch_too`,
    `the_strip_offers_rewind_while_the_stopped_notice_does`. Scene checks (the first three seen failing first; the
    strip's written with its code, its own-click check seen failing with the fix taken out):
    `agent-box-terminal-text` (the placeholder clear of the beam, unmoved without focus, the hollow cell on its first
    letter), `agent-question-anywhere` (a banner naming the younger question's tab lands there with the keys; a gone
    tab falls back), `agent-tab-question-dock-tiny` ("Show in chat" glides, or jumps with Reduce motion, then follows
    the end), `agent-rewind` (`-strip`: the button at the strip's right, its click the same dry run as the notice's
    link and not the jump; `-strip-sent`: gone after the next send). The full
    `scripts/ui-shots.sh` 322 passed, 5 failed (the five that fail on this Mac's 1× display on old commits too);
    references not refreshed. Not yet seen in the real window. The screenshots' fix: 2682 → 2683 tests
    (`a_scene_folder_is_the_same_each_time_made_empty_and_refused_while_held`; with the `ui-shots` feature also
    `an_edge_snaps_to_a_device_pixel_as_gpui_draws_it` and `a_round_corners_first_pixel_is_covered_by_the_scale`);
    the full `scripts/ui-shots.sh` on this Mac's 1× display: 327 passed, 0 failed, fingerprints 693 same, 27 look
    different, 0 differ in size, 0 without a reference; with `UI_SHOTS_SCALE=1` 327 passed, 0 failed. The 27 vary
    between runs of the same code in a serial run too (the agent box's menus, the shell cards, the sliding sidebar
    and title bar, `card-slide`): not the display; still open.
- **The registry's scene check reads helpers too** (the lead, closing the opens): `each_area_file_is_repo_relative_and_
  declares_its_own_scenes` (ui-shots tests) failed at HEAD because `agent_ask.rs` declares its scenes through
  `visual("…")` / `state("…")`, wrapped by rustfmt. It now also accepts a helper call with the name first, whitespace
  ignored. `cargo test --bin fuxx --features ui-shots`: 2733 passed.
- **The lanes chore: the resume fix** (2026-11-24; the Lead Architect's binary research, Claude Code 2.1.330/2.1.331,
  no recording): a subagent Main resumed with SendMessage (or a wake) re-sends `task_started` for the same task, its
  `tool_use_id` the SendMessage call's. fuxx opened "Explore 2" for it, and the original lane — revived by its late
  steps and never ended — spun forever. Lanes are now keyed by task: the same task again is
  `Event::SubagentResumed` (its lane keeps its place and label, its clock starts again, what Main said shows "from
  Main" after a divider "✓ Done in 1:33 · resumed by Main"; the header's time is this run's, its tooltip "This run …
  · first run …"; the pill's mark goes back to the arc with one flash and its tooltip adds "· resumed"). Steps under
  the resuming call land in the lane (`aliases`). A step after the end no longer reopens a Claude lane
  (`LaneControls::woken_by_steps`: agy's lanes still wake on steps), and an ask only marks a live lane Waiting. A
  lane whose source stops with no end (the agent exited, a history cut off) is **Ended**: a hollow ring, "ended
  without a report", never a spinner. Synthetic fixture `claude-subagent-resume.jsonl` (marked so).
- **The lanes chore: the relay** (your ask above): in a Claude subagent running in the background the box says
  "To Explore · via Main" (a pill first in its folder tab, with the lane's own mark) and "Message Explore — Main
  passes it on". Enter sends Main your words framed (`claude::relay_frame`: pass it to the subagent named by its
  agent id with SendMessage, word for word; if it finished, don't resume it, say so) and keeps you in the lane. It is
  a message to Main in every way (its id, its echo, the count, × while unread). In the lane your bubble's caption
  says where it is: held, "Main is passing it on…", "delivered ✓" (Main's SendMessage to it seen; the pill flashes),
  "Main didn't pass this on" (Send again), or "Explore had finished" (Ask Main instead). In Main it is led by
  "→ Explore", and Main's SendMessage row reads "Passed on to Explore ✓"; the frame is never shown, history included
  (`transcript_events` reads it back). `agent_lanes::lane_box` decides, one pure rule: agy → the box to Main as
  before; Claude live in the background and ready → relay (an image turns its send off: "Images can't be passed on:
  send them to Main"); in the foreground, finished or not ready → the closing bar. `Provider::relays_to_subagents`
  (Claude only); the frames live in `providers/claude.rs`.
- **The lanes chore: the closing bar** (the Lead UI/UX Designer's pick over hiding or disabling the box): a finished
  lane's box (and a live one inside Main's step) keeps its exact height but shows, in the UI font, the lane's end
  ("Explore finished in 1:33", "failed · reason", "was stopped after 0:48", "ended without a report", "is working
  inside Main's step · messages reach it only in the background"), then "Ask Main about it", "or start typing" and
  "Back to Main ⏎", the notch a Back circle. ⏎ goes Back to Main; typing, a paste or ↑ turns it into the box in place
  ("To Main · about Explore", "Ask Main about Explore's result…"), and sending switches to Main with an "About
  Explore" chip, Main told not to resume it (`claude::about_frame`); ⌫ in the empty box brings the bar back. A lane
  finishing while you type to it never swaps the box: its pill retargets to "about" and flashes, and an Enter within
  600 ms only flashes again. The list's end row lost its "Back to Main ›". The pill row's tooltips say each state
  ("Running 1:02 · task · you can message it", "Done in 1:33 · …"); "+N" carries a live mark (tinted when one waits)
  and "18 more · 3 running"; its list groups Running and Finished, each newest first, and keeps a finishing lane's
  row until it opens again. New tokens `bar_*` and `relay_*`, held to the floors in every theme. Scenes
  `agent-lanes-resumed`, `-relay`, `-relay-narrow`, `-closing-bar` (and `-failed`, `-stopped`, `-ended`,
  `-foreground`), `-bar-narrow`, `-bar-typing`, `-bar-about`, `-finish-mid-draft`, `-list-grouped`,
  `-relay-history`; `agent-lanes-narrow` re-recorded (its Claude lane relays and stays).
  - **AI's calls:** the closing bar's not-ready words ("is working · you can message it once Main is ready") and the
    live "about" placeholder ("Ask Main about Explore…") and frame ("which is still working inside your current
    step") aren't in the spec; agy's lane keeps "Message Main — it can pass this to …" (the spec's ToMain); a
    relay's other attachments go in its text, images can't; a message for a lane held while Main restarts waits as
    any message does; the pills' width estimate is 7 pt a character (6.6 ran short once "+N" grew its mark).
  - **After the design review** (2026-11-24; the Lead UI/UX Designer on the real lane screenshots, every item done):
    (1) your rule for every provider — `lane_box` checks a finished lane first, so a finished agy lane gets the
    closing bar too (only a running agy lane keeps "Message Main — it can pass this to …"); agy's "Ask Main about
    it" goes to Main as an ordinary turn naming the subagent (`providers/agy.rs`; it used to be dropped).
    (2) Foreground words: "Explore is working · Main is waiting on it, so messages can't reach it yet", tooltip
    "Main waits for Explore to finish, so it can't pass a message on until then. Ask Main now and it reads it when
    Explore is done."; the pill tooltip says "Main is waiting on it". Not ready: "Explore is working · you can
    message it once Main has started". The words are one pure function, `agent_lanes::bar_words`. (3) Every bar mark
    sits in the 12 pt slot, so row 1's words don't move by state. (4) Failed is a small ✕ (`status_mark::
    failed_cross`, `failed_panel`) in the strip, the "+N" list and the bar, no longer a dot like Waiting's.
    (5) "Ask Main about it" has an edge in every theme: token `bar_edge`, in the contrast test. (6) No "Explore
    Explore": the header drops a kind equal to the label (`agent_lanes::header_kind`). Nits: "Passed on to
    Explore" without the purple ✓ or the raw "Sent."; " · " before "Send again" (and before "Ask Main instead");
    under 480 pt the relay placeholder is "Message Explore via Main"; `agent-lanes-closing-bar` freezes its clock at
    1:33. agy-latte's orange-brown "Done": the box tab's 200 ms ink blend from Cultivating's peach, caught at its
    start (it predates this work; the real window settles grey); the blend and the Done check's pop now honour
    reduce motion and Animations off, and `agent-lanes-agy` shoots still. The lead's nit: agy's question banner is
    titled "agy asked a question" (`notifier::asked_title`), not Waiting's "Needs your input"; the body stays
    "agy asked: …".
    - **AI's calls:** the " · " before "Ask Main instead" too (the same line as "Send again"); agy's about message
      reads `About your subagent "Explore", which has finished:` then your words (agy has no history fuxx reads
      back, so no tag).
  - **On screen:** not yet seen in the real window (see the Progress Log).
- **agy's skipped questions, answered by a message** (2026-11-24; your words: "can you take a look at agy, it doesn't
  want to show the popup, it says it skips it. but claude is done great" … "give me the nearest solutions first for
  this without ACP. We will have our milestone later for ACP integration, because this is big" — that milestone is
  "ACP client" in the roadmap). agy's print mode answers its own `ask_question` with "User Skipped" and goes on; its
  stdout shows only a bare `step_type: "unknown"` step. The question is in the conversation's main transcript
  (`<gemini dir>/antigravity-cli/brain/<id>/.system_generated/logs/transcript.jsonl`, the gemini dir
  `$ANTIGRAVITY_GEMINI_DIR` or `~/.gemini`), so agy's `init` now follows it (`Event::Follow` with `main`, read every
  0.5 s like a working lane; once per conversation id, again after a new id) and `decode_followed` turns a skipped
  `ask_question` of this run (its step at or after the run's first stdout step) into `Event::Question`, never twice;
  an answered one shows nothing, and the transcript adds no rows. The card is Claude's card with three labels: the
  head "agy asked — it couldn't wait for your answer" (its Waiting mark still), the footer line "agy's print mode
  skips questions, so it went on without one. Your picks go back as a new message." and "Send as message ⏎". Your
  picks go through the normal send as "My answers to your questions:\n- ⟨question⟩ → ⟨answer⟩" (the line agy took in
  the recording; unanswered left out, the Recommended mark dropped; nothing answered: the button waits); Skip closes
  it here; another message while it is open leaves it not answered. The tab never shows Waiting for it; the banner
  says "agy asked: …", and ⇧⌘A and the palette reach it. A generic `Provider::questions_by_message` (agy only) and
  `question_note` carry it; Claude's card is unchanged. The card's Recommended tag also reads agy's leading
  "(Recommended) ".
  - **agy's real mode:** an account without Auto makes agy say "--mode auto is not available … continuing in the
    default mode". Stderr lines now reach the translator as they come (`ProcessEvent::ErrLine`, the first 64;
    `Translator::decode_stderr`; a plain stdout line is checked too), agy's parser turns it into
    `Event::ModeRefused`, and the tab's mode and chip follow agy's real mode, with one quiet notice "Auto isn't
    available on this agy account — running in Accept edits". `auto` stays agy's default as asked, but this Mac
    remembers the refusal (`mode_memory.rs`, `~/Library/Application Support/fuxx/agent-modes.json`, `-dev` for a test
    build) and starts later agy tabs in Accept edits, with no warning.
  - **AI's calls:** "the default mode" names no mode, so it reads as Accept edits (the closest of fuxx's three to agy's
    own default, `request-review` in its `init`); a mode named after "continuing in" wins. Where agy prints the
    warning wasn't recorded: stderr is assumed (as its "conversation … not found" warning), and a non-JSON stdout line
    is read too. A by-message question survives the turn's end and the process ending (only you close it); "not
    answered" is the existing Expired summary, its words "You sent another message instead." The banner for it is
    the Waiting banner (its title "Needs your input"), sent only when you aren't looking at the tab. Tests and scenes
    read agy's data under `/Users/me/.gemini` (never yours) and never touch the mode file.
  - **Checks:** fixtures `agy-ask.jsonl` (stdout) and `agy-ask-transcript.jsonl` (both turns), scrubbed; tests
    `agy_init_follows_the_main_transcript` (with the env dir), `agy_skipped_ask_question_becomes_a_question_card`,
    `agy_main_transcript_adds_no_rows`, `agy_resumed_run_ignores_old_questions` (first step 7),
    `agy_answered_ask_shows_no_card`, `agy_multi_select_and_option_objects`, `agy_answers_questions_by_message`,
    `agy_refused_mode_is_its_real_mode`, `answer_message_says_each_answer_on_its_line` (single, multi, Other,
    unanswered left out, empty → None, Recommended stripped, the exact tested string),
    `a_leading_recommended_mark_is_a_tag_too`, `a_question_not_waited_on_says_it_was_asked`,
    `a_question_answered_by_message_waits_on_nothing`, `stderr_lines_come_one_by_one`, the mode memory's two, and
    `a_finished_lanes_file_is_read_less_often` (the main follow's rate); scenes `agent-tab-question-agy` (both
    themes: the head, the footer line, "Send as message"; Enter sends the bubble text agy took, Skip sends nothing,
    another message leaves it not answered), `agent-tab-agy-mode-refused` (both themes: the notice once, the chip
    on Accept edits, new agy tabs start there) and `agent-question-agy-anywhere` (in a workspace: the banner says
    "agy asked: …" though the tab never waits, ⌘⇧A lands on the card, a new agy tab after the refusal starts in
    Accept edits).
  - **On screen:** not yet seen in the real window; no real agy was run (the fixtures are the recorded calls).
- **The suggested prompt is greyed out and italic** (your words, 2026-11-24: "1 more fast chore, for the suggested
  prompt on agent chat input, make it greyed out and italic"): Claude's guess in the empty box draws in the box
  font's italic (JetBrains Mono Italic, bundled) and a grey quieter than the placeholder (`composer::ghost_ink`:
  `text_dim` taken 35 % toward the box fill), so it never reads as typed text; its "⇥" stays.
  - **Checks:** `the_suggestion_is_italic_and_greyer_than_the_placeholder` (every theme: quieter than the
    placeholder, still over 1.5:1; the font italic), watched failing first; `agent-box-suggestion` reference
    refreshed (both themes).
- **A program's own browser opening lands like a clicked link** (your words, 2026-11-24: "the url click is cool,
  open an new tab on right split pane. 1 thing is missed on this, I want the same treatment happening when there is
  a browser autotrigger from the terminal inside fuxxpace. For example running superpower mockup preview, doing an
  npm run dev with auto open, etc."): a new `fuxx link open URL` (protocol `link.open`, `src/auto_open.rs`,
  `link_route.rs`) opens an http(s) address as a link clicked in the terminal (`FUXX_SESSION`) or agent tab (new
  `FUXX_AGENT_TAB=aN`) it runs in, through `Workspace::open_link`, so "Open links in" decides: a split on the right,
  a tab in this pane, or your default browser (handed to macOS as before). It answers at once, never after the page.
  At start fuxx writes three POSIX `sh` shims to `~/.local/share/fuxx/shims` (`~/.local/share/fuxx-dev/shims` for a
  debug build), each only when it changed: `open`, `xdg-open` and `fuxx-browser`. A lone `http://`/`https://` address (also
  `-u URL`, `--url URL`) goes to `fuxx link open`; anything else (`-a`, `-R`, files, `file:`, `mailto:`, two
  addresses) and any failure (fuxx closed, scripting off, no calling tab) runs `/usr/bin/open` with the arguments
  untouched. Every fuxx terminal (local, background-service, an SSH session's local side) and agent process gets the
  folder first on `PATH` (never twice), `BROWSER=<folder>/fuxx-browser` and `FUXX_SHIMS`; the zsh integration's
  `.zshenv` puts the folder first again at the first prompt, once (a one-shot `precmd` hook), after
  `/etc/zprofile`'s `path_helper` and your own files. The page's tab pill always flashes (a new split's too, as a
  reused pane's), and your focus stays where it is; from a terminal in a workspace you aren't looking at, the page
  opens beside that terminal and neither your focus nor that workspace's moves.
  - **AI's calls:** a new command, not `browser open --flag`: `browser open` is the agent's automation browser (the
    workspace's most recently used browser tab, which the other `browser.*` commands then drive, and it waits for
    the page), while an auto-open is your link and must follow "Open links in" and return at once; one command with
    two placement rules would be harder to read. `-a <fuxx-browser> URL` (the full path or the bare name) is taken
    over too: with `BROWSER` set, Vite's `open` package runs `open -a "$BROWSER" URL` on macOS, which the real
    `open` would refuse (a script is no app); any other `-a` passes through. `fuxx link open` gives up after 3 s
    (`api_client::call_within`), so a stuck window never holds the program; the shim then runs the real `open`. The
    shims run `/usr/bin/open` unless `FUXX_REAL_OPEN` names another (the tests' stub), `xdg-open` too. Terminals get
    the shims only while this fuxx serves its socket (scripting on); otherwise `FUXX_SHIMS` and `FUXX_AGENT_TAB` are
    removed with the other variables. An agent process gets `FUXX_SOCKET`, `FUXX_AGENT_TAB` and an empty
    `FUXX_SESSION` (so one inherited never names a terminal), set before its first start. Your own `BROWSER`, if
    you had one, is replaced inside fuxx (the shim's fallback is macOS's default browser). The Command Reference in
    Help lists `link open` with the browser commands. `keep_focus` replaces the old "select the tab you were on":
    the same for a click, and it also keeps the other workspace's focus for an auto-open from there.
  - **Limits:** macOS login shells run `path_helper`, which moves the folder behind `/usr/bin`; only the zsh
    integration (on with command blocks or a helper plugin) puts it back. With bash, fish, or the integration off,
    a plain `open URL` in a login shell reaches the real `open`; `$BROWSER` still works (Python's `webbrowser`,
    Vite with `BROWSER`, anything that reads it). Claude Code's Bash tool
    may restore your shell's `PATH`; its commands then reach fuxx through `BROWSER` only. SSH sessions' remote
    shells get no shims, and a background-service terminal started before this build gets them when reopened.
  - **Moved (the lead's must-fix, same day):** the first cut put the shims under `~/Library/Application
    Support/fuxx/shims`, and that space broke `$BROWSER` for tools that split it (gh's `auth login`). They now live
    in `~/.local/share/fuxx/shims` (`fuxx-dev` for a debug build), a path with no space; at start fuxx removes the
    old `shims` and `shims-dev` folders under "Application Support" when they hold nothing but the three shims.
    Tests `the_shim_folder_and_browser_have_no_whitespace` (`BROWSER` is the folder's `fuxx-browser`) and
    `the_old_shim_folder_goes_only_when_it_holds_just_shims`, both watched failing first.
  - **Checks:** `auto_open` tests (the rule `auto_open_target`; the `open` script run through `sh` in a temp folder
    with a fake `fuxx` and a stub real `open`: interception for a lone address, `-u`, `--url`, `-a <fuxx-browser>`,
    `xdg-open` and `fuxx-browser`; an odd address with quotes, `$( )`, backticks and spaces reaches fuxx as it is and
    runs nothing; pass-through untouched for files, `file:`, `mailto:`, `-a Safari`, `-R`, two addresses, no
    arguments, outside fuxx and when `fuxx link open` fails; every case of the rule run through the script too; the
    folder's dev split; `PATH` once; the agent's variables; writing only when changed, `0o755`);
    `link_open_parses_with_its_caller`, `link_open_from_an_agent_tab_names_it`,
    `a_reply_that_never_comes_is_given_up_on`, `methods_match_the_spec`, the four `keep_focus`/`caller_tab` tests,
    `a_terminal_gets_the_shims_first_and_its_browser`, a real zsh's
    `the_shims_are_first_on_path_again_after_your_files` (command blocks alone and with a helper plugin; a folder
    with a space and glob characters; the hook gone after), `link_open_sits_with_the_browser_commands`; each watched
    failing first. New state scene `link-auto-open` (a dev server's auto-open splits right and flashes, focus stays;
    a second address is a tab there; the same one selects; refusals; a terminal in another workspace opens beside
    itself; "Your default browser" goes to the OS).
  - **On screen:** not yet seen in the real window; no real `npm run dev`, Vite or mockup preview was run.
- **"Divining…" is now "Orchestrating…"** (your words, 2026-11-24: "\"Divining\" label status indicator, change it to
  \"Orchestrating\". It's better"): the status pill's word while only subagents run (`agent_lanes::running_word`).
  - **Checks:** `subagents_still_running_keep_the_tab_working` updated and watched failing first; 2748 tests; the
    `agent-lanes` scenes (22) pass.

- **A subagent's row shows it's a subagent** (your words, 2026-11-24: "If you can see the 'Write activity view ...' on
  chat box, its a subagent running. Make it different if its a subagent so that I knew its a subagent running. Is
  there an outlined robot icon for this or colored ones would be awesome, I give this to Lead UI/UX Designer agent to
  decide"): the Lead UI/UX Designer's spec, built as written. A subagent's row in Main's chat reads
  `[mark] [robot] name task … words ›`: the state's 12 pt mark stays where it was (in line with the tool dots, the
  same shapes as the pills), then lucide's outlined `bot` (lucide-static 0.460.0, `assets/icons/lucide/bot.svg`,
  `Icon::Bot`) 14 pt at the mark + 8, the name in `panel_text` MEDIUM and the task in `panel_dim`. The robot is a new
  `lane_ink` while the subagent runs (working or waiting) and `panel_dim` once it ended; it never animates. The
  right side leads with a verb: "Running Read · 0:12" (under 480 pt "Read · 0:12"; no tool yet "Running · 0:12"),
  "Needs input · 1:02" in `ask_head` on an `ask_fill` row (`ask_cursor` under the pointer), "14 steps · 2:10",
  "Failed · 0:48" in `failed_panel`, "Stopped · 0:48", "Ended · 12 steps". Its tooltip has two lines: the pill's
  ("Running 0:12 · Read the test setup · you can message it"), then "Explore subagent · click to open its lane"
  ("Subagent · …" for an empty or generic kind). One robot after Main heads the pill strip's subagents (14 pt,
  `PILL_GAP` each side, its slot taken off the room before the pills are fitted; `lane_ink` while any lane runs, else
  `dim`; not clickable; tooltip "29 subagents · 3 running · 1 needs input", zero parts dropped), and a lane's header
  starts with it. The "+N" list, the permission card, "Passed on to Explore" rows and the closing bar are unchanged.
  - **AI's calls:** **the name no longer uses `tool_name`, replacing M11ar's label rule** (the spec's "label
    (`tool_name`)"). `lane_ink` (`agent_tab::Colors`, derived, not a chrome key): of the theme's blue, magenta and
    cyan (`palette[4..=6]`) the hue farthest from the accent, working, waiting and done colours
    (`agent_lanes::lane_hue`), `panel_dim` when the best is under 25° away, then held to `MARK_MIN` on `panel`,
    `row_hover`, `ask_fill`, `ask_cursor` and the page; the built-in themes pick Latte teal, Mocha teal, Dracula
    pink, Nord purple, Gruvbox aqua and TokyoNight magenta. Which kind is "generic" is the provider's to say: a new
    `Provider::generic_lane_kind` (default: an empty kind), and Claude's file adds its catch-all `general-purpose`.
    The strip's tooltip counts a waiting lane under "needs input", not "running". Narrow drops "Running " only when a
    tool is known (else the row would read just a time). The waiting row gets its fill and no left edge. The row is
    drawn in a new `src/agent_lane_row.rs`; the tool group's shell (fill, edges and corners by its place in the
    group, the divider from 30 pt, hover) moved out of `tool()` into one `tool_shell` both rows use, with the status
    slot, the right-side words and the chevron as shared helpers; the scenes' `ink_in` moved to the shots harness so
    file-tab and lane scenes share it. `\n` does break the line in the nowrap `Tip` box (seen in the
    `agent-lanes-row-tip` picture).
  - **Checks:** unit tests `a_lane_row_says_what_its_subagent_is_doing`, `a_lane_row_s_tooltip_names_its_kind_on_a_second_line`,
    `the_strip_robot_counts_the_subagents`, `the_lane_ink_stands_apart_from_the_accent_and_the_status_colours` (the
    six themes' picks), `the_lane_ink_stands_apart_or_falls_back_to_dim`, `general_purpose_is_a_generic_subagent_kind`,
    and the icon tests with `Icon::Bot` (54 icons, 52 files); each watched failing first. `lane_ink` added to
    `the_tab_s_text_meets_the_floor_in_every_theme`. New state scenes `agent-lanes-row` (the robot's pixels in
    `lane_ink` at the mark + 8, the name in `panel_text` not `tool_name`, the mark column in line with Bash's, the
    divider from 30 pt, the row 30 tall, the strip's robot in `lane_ink`) and `agent-lanes-row-tip` (both tooltip
    lines, the `row_hover` fill); `agent-lanes-states` gains an Ended lane (cut off in a resumed history) and checks
    Main's rows (robot coloured only on live rows, the waiting row's `ask_fill`, each state's words), its window
    860 pt tall so all of them are in the picture. The other lanes scenes are state scenes (no references), all
    passing; references refreshed for `agent-tab-question-lane`, `agent-tab-question-lane-main` and
    `agent-chat-latest-prompt` (the strip's robot moves the pills along).
  - **On screen:** not yet seen in the real window; no claude or agy was run.
- **The box's target pill caps the subagent's name** (your words, 2026-11-24: "have maxwidth on the subagent title,
  make it ellipsis, I don't accept long text, we already have tooltip to see full title … just change the code
  directly without tests"): "To ⟨name⟩ · via Main" and "To Main · about ⟨name⟩" cut the name at 180 pt with "…"
  (`TARGET_NAME_MAX`); the tooltip (both modes now) holds the full name. No new tests, as asked; `cargo check` clean.

- **The agent activity view** (your words, 2026-11-24, on a subagent lane full of `python3 - <<'EOF'` / `sed -n …`
  rows: "run superpowers mockup to enhance the UI and UX on this", then "deep dive it", then "I love C"; design
  `docs/superpowers/specs/2026-11-24-agent-activity-view-design.md`, plan
  `docs/superpowers/plans/2026-11-24-agent-activity-view.md`, both approved; built in the session, 6 tasks). An agent
  tab now says what each step touched and how its checks went:
  - **One provider-neutral `Activity` per tool call** (`src/agent_activity.rs`, shaped after ACP's tool call: kind,
    the agent's own words, fuxx's derived words, the files it read / searched / changed / named in a script, a failed
    call's first error line, a denial, a check with its kind, scope and outcome, a planned failure, the exit code).
    Shared pure readers: `agent_activity_command` (a shell command split on `&&`/`;`/`|`/new lines with heredoc
    bodies kept apart, a verb table, file arguments, `sed` addresses, check runners and a normalised scope) and
    `agent_activity_checks` (cargo / pytest / jest / vitest / go test summaries; a summary in the output wins over an
    exit code a pipe hid; a piped check with no summary is Unknown, never green). Claude's and agy's translators fill
    it in `src/providers/claude/activity.rs` and `src/providers/agy/activity.rs` (Claude's Bash `description`, `Exit
    code N`, `<tool_use_error>`, Grep's files from its result, denied calls; agy's exit header, a lane's
    `toolSummary`), sent as `Event::Activity` after each start and finish and kept on the tool item; history rebuilds
    the same. A check failure the narration or the call announced ("red first", "(red)", "failing test first", "should
    fail", "expect … to fail", "TDD") is planned. Tool calls record live start and finish times.
  - **A step of 3+ calls draws as file tracks** (`src/agent_tracks.rs`, from the pure `src/agent_steps.rs` model,
    rebuilt only when the step's calls change): a header ("7 actions · 4 files · 2 failed · 0:12"), a checks track
    and a track per file (changed first), one bead per call in time order (● read · ▲ search · ■ edit · ◆ script · ✓ ·
    ✕ · ○ planned · the running arc; a call that failed gets a small red ✕ and changes nothing; denied or stopped
    calls are dim), a summary per track ("edited · +2 −1", "read 80–400 · script 1×", "touched by a script",
    "searched · 3 matches", "edit failed"), the step's open real failure under the group in the failure ink, "+N
    files" past 6 tracks, the latest 24 calls with "+M earlier" (a click shows all, sideways). Hover a column for its
    call, click it for the detail modal; a click on the header gives the keyboard (←/→, Enter, Esc). Under 560 pt
    tracks fall back to rows. A subagent's lane row and a "Passed on" row break a step (tracks start after them) but
    stay in their readable rows' group.
  - **Smaller steps are readable rows**: the agent's words first ("Run the CLI tests"), else fuxx's ("Edited
    link_route.rs", "Ran tests cli::"), the command as a dim mono trace, outcome chips ("2 passed", "+41 −0", "build
    failed · E0425", "red, as planned", "failed"); a planned failure's mark is a hollow red ring.
  - **The turn ledger in the box's folder tab**, after the status pill (`src/agent_ledger.rs`): the running call and
    its time ("Running tests 0:14 · +1 running"), "4 changed", "checks 3/7" (or "tests failing" while the latest check
    of a kind failed for real and no pass of the same kind and scope came since), cut from the right to fit with the
    running call kept; Main's covers the whole tab, a lane's only itself; hidden while a question or permission is
    open; only for a live turn. A click opens a popover (the box menus' frame): Changed (a click opens the file's diff
    tab), Touched by scripts, Read, Checks, Planned failures.
  - **AI's calls (rulings):** the header's "N failed" counts calls that themselves failed, not check failures (the
    spec's header mock); the popover uses the box menus' `menu_frame`, not `overlay_panel::shell`; lane and "Passed
    on" rows break a step but not the readable group (the subagent row chore's one-group look kept; its
    `agent-lanes-row` checks pass unchanged); the fixture's first step has 7 calls (the 8th follows the "red first"
    narration), so the busy scene checks "7 actions"; the ledger scenes' turn reads "checks 2/6" running and "checks
    3/7" done. Every ruling is in the branch's ledger and the hand-off.
  - **Checks:** unit tests in the new modules (among them `the_output_wins_over_an_exit_code_a_pipe_hid`,
    `a_piped_check_with_no_summary_is_unknown_never_passed`, `bash_reads_its_description_command_and_exit`,
    `a_failed_edit_touches_nothing_changed`, `history_rebuilds_the_same_activities`,
    `sixty_calls_show_the_latest_24_and_plus_36_earlier`,
    `a_failed_call_beads_its_kind_with_a_mark_and_counts_in_the_header`,
    `a_real_failure_stays_until_the_same_kind_and_scope_passes`, `parallel_subagents_editing_one_file_count_it_once`,
    `script_touched_files_are_listed_apart_and_never_counted`, `it_trims_from_the_right_keeping_the_running_call`,
    `the_track_keys_stay_in_the_tracks`), each watched failing first; new colour tokens in
    `the_tab_s_text_meets_the_floor_in_every_theme`. New scenes (both themes) `agent-tracks-busy`, `-running`,
    `-failure`, `-planned`, `-more-files`, `-earlier`, `-narrow`, `agent-steps-rows`, `agent-ledger-running`, `-done`,
    `-trimmed`, `-popover`.
  - **On screen:** not yet seen in the real window; no claude or agy was run.
  - **After the review** (the whole-branch code review and the Lead UI/UX finish review, one fix wave):
    - **Checks read right.** A check's exit counts only when it is the check's own (the check last, or followed only
      by `&&`, never piped): `; echo done`, `|| true` and a pipe leave it Unknown unless its output holds a summary. A
      failing exit beats a passing summary, a crashed test binary (`process didn't exit successfully`, `error: test
      failed`) fails though every result line was ok, and a failed jest suite / vitest file fails it. A scope keeps
      what follows `--` but output flags (`-- cli::`, `--skip api`, `-D warnings` no longer cleared by a laxer run),
      and a `cd` before a check is part of it (relative paths after a `cd` are keyed under it, so the popover opens
      the right file; a `cd` to the tab's folder is the same scope as none). Several checks in one command are one
      compound check ("Ran lint + tests"), so only the same command clears it. `timeout 600` wraps, `sed -Ei` edits, a
      script's `s/…/…/` strings and plain words aren't files, Windows line endings are read, an empty grep / rg (exit
      1) is no failure, and a cargo panic reads as one sentence ("cli::tests::bad: assertion `left == right` failed
      (src/cli.rs:42)").
    - **Planned failures.** Only the first red run of a check in a run of calls is the announced one (the same check
      failing again after an edit is real); "nothing should fail", "I don't expect it to fail" and a bare "TDD"
      announce nothing ("TDD" counts beside a red / fail word).
    - **Providers.** Claude's `Exit code N` line is read as the exit and left out of the output, so it is never a first
      error; fuxx's denial now says "The user denied this in fuxx: …", so a denied call reads the same in the history
      (`history_rebuilds_the_same_activities` has a deny case). An agy result's exit header is its own command's, not
      every call of the step.
    - **Calm vs noise, as the design review asked.** Beads in the text's dim ink, a read's dot 2 pt smaller, a script a
      hollow ◇ in the edit ink, a real failure a heavy ✖, a planned one ○, an Unknown check ◌; a call that failed draws
      dim under an 8 pt ✕ (the change that didn't happen). The bead lane snaps to 6 columns. Labels are 148 pt wide,
      a read-only file's in `panel_dim`, a changed one's in `panel_text`, "Checks" and "Commands" MEDIUM. A summary
      that says a failure (a scope still failing, "edit failed", a failed command) and the header's "N failed" are in
      `failed_panel`.
    - **Words.** The checks track says each scope's latest state, not a chain: "tests failed · cli:: 1 of 3", "tests ·
      red → green · 2 passed", "tests · 3 passed · fixed", "3 passing · 2 fixed", the kind always named. A failed call's
      chip is its first error (40 characters), its title the plain verb ("Edit cli.rs", "List files"); a listing's own
      title is "Ran ls nope" (fuxx's fallback; an agent's description still leads). A last "Commands" track beads the
      calls with no file that aren't checks ("2 commands · 1 failed"). "+6 earlier" moved into the header and reads
      "latest only" once open; "+N files" reads "fewer" once open. The header hides a time under a second; the running
      row and the tool modal use the one clock format ("0:12").
    - **Ledger.** The folder tab says "checks pass" when every scope's latest run passed, "tests failing" (red) while
      one fails for real, else "checks ran"; the popover has "P/T passed" beside its Checks heading (T counts every
      run, Unknown and planned ones too: the lead's call) and one row per scope with a strip of its runs (✓ ✖ ○ ◌) and
      its latest outcome, red only while that scope still fails. A changed file's row has the tooltip "Open the diff".
      The pill's status word and ledger parts sit in one gapless run (the 5 pt gap only after the mark).
    - **Rows and keys.** A readable row with an Unknown check gets a dim ring, never the green dot. Its words are built
      once per change and cached on the row (its trace capped at 160 characters); a step's tracks come from the row
      that draws them (no scan). ⌥↑ in the box puts the keyboard on the latest busy step's tracks (Esc gives it back;
      listed in the shortcuts as "Step Through the Latest Busy Step").
    - **Lean files.** The step model's words (header, summaries, outcome phrases, row words, tips) moved from
      `agent_steps.rs` to `src/agent_step_words.rs`; `agent_ask::cut` is shared for the cut titles, traces and chips.
    - **AI's calls:** "+2 −1" stays calm (no tint); the checks track's failure ink follows the step's own scopes (as
      its summary does); a Commands bead for a failed call is the ✖ itself; the scenes keep their collapsed state as
      `…-collapsed` pictures before the click.
    - **Checks:** 2827 → 2850 tests (`--features ui-shots`: 2877 → 2900), among them
      `a_checks_exit_is_trusted_only_when_nothing_after_it_can_replace_it`, `a_failing_exit_beats_a_passing_summary`,
      `words_after_a_bare_dashdash_stay_in_the_scope_but_output_flags`,
      `several_checks_in_one_command_share_one_compound_scope`, `a_cd_scopes_the_check_and_prefixes_relative_paths`,
      `only_the_first_red_run_of_a_check_is_planned`, `claudes_exit_code_line_is_never_a_first_error`,
      `an_agy_result_s_exit_is_only_its_command_s`, `checks_summaries_say_each_scopes_latest_state`,
      `an_unknown_check_never_marks_its_row_green`, `calls_with_no_file_that_arent_checks_bead_a_last_commands_track`,
      `the_lane_snaps_to_six_columns`, each watched failing first; clippy 14/29 and 18/33; fmt clean. The activity
      scenes check the new words, ⌥↑, "latest only" and "fewer"; references refreshed for `agent-tracks-running` and
      `agent-tab-question-agy` (its failed listing row: "Run ls -la …" with its first error). Full
      `scripts/ui-shots.sh`: 359 passed, 1 failed (`agent-tab-wide`, display-dependent: the screen here is 1512 pt
      wide, so its 1600 pt window is clamped; it fails the same on the branch before the fix wave), 32 look different
      (the animation set), 8 differ in size (the same clamp).
    - **After the designer's re-check** (everything had landed; four last items, one commit): a compile error is the
      build's whatever ran it ("build failed · E0425" under `cargo test` too, on the row, the checks track and the
      ledger's "build failing"); a real ✖ that a later pass of the same kind and scope cleared draws as the light ✕ in
      the same ink (the history stays, the alarm goes); the unknown check is a drawn hollow ring, not the font's ◌
      (which drew only faint dots at bead size, about 4 × 3.5 pt of half ink), on the track and in the popover's
      strips; the "+6 earlier" toggle reads "fewer" once open (as the files toggle) and both toggles are in
      `panel_text`; a row's chip drops the kind ("1 of 3 failed": its title says what ran), the checks track keeps it.
      Checks: 2850 → 2852 tests (`a_compile_error_under_cargo_test_is_the_build_failing`,
      `a_failure_a_later_pass_cleared_draws_as_the_light_cross`, the chip and summary tests extended), each watched
      failing first; new visual scene `agent-tracks-unknown` (the ring's ink round its cell, hollow in the middle;
      reference recorded); the `agent-tracks`, `agent-steps` and `agent-ledger` scenes pass, no other reference moved.
    - **On screen:** not yet seen in the real window; no claude or agy was run.

- **A long notice or error no longer runs under the next message** (your words, 2026-11-24: "why the layout below is
  broken?", on Claude Code's six-paragraph safety-check notice in Main's chat). Root cause: each chat row is a flex
  column (`items_center`) holding the column (`w_full`, at most `COLUMN_MAX`); taffy sizes a flex column's child — its
  flex basis, so its height — at its `w_full` width, not clamped by `max_w`. On a tab wider than `COLUMN_MAX` the
  notice's text was wrapped on the wider lines (fewer of them) when the row's height was measured, then drawn on the
  narrower ones, so its last lines ran under the next row. The agent's own message escaped only because its body
  carries its own `max_w`. Fix: a row centres its column in a flex row (`agent_rows::centred`), where the width is
  clamped before the height is measured; the lane's task and end rows, which had the same shape, use it too.
  - **Checks:** new state scene `agent-notice-long` (a 6-paragraph notice and a 3-paragraph error, each followed by
    the agent's message, at 720 and 1100 pt): each body's drawn bottom is at or above the next row's top and inside
    its own row; it failed at 1100 first (the notice's text ended at 401, the next row began at 364). The agent and
    lanes scenes (155) pass.
  - **On screen:** not yet seen in the real window.

- **⌘V in the agent box never stays silent** (your words, 2026-11-24, after an image wouldn't paste: ⌘⇧4 had saved
  the screenshot to a file, so the clipboard held no new image and fuxx did nothing; "add those three as a chore"):
  1. **Something is said** when ⌘V finds nothing the box can use: a notice row in the chat, "Nothing to paste: the
     clipboard has no text, image or file fuxx can use."
  2. **Files copied in Finder** (⌘C, then ⌘V) come in as a Finder drop does: image files as image pills, anything
     else (a folder too) as path pills, at the caret. Before, the file's name went in as text.
  3. **TIFF and BMP clipboard images are made PNG** and pasted as an image pill. SVG, ICO and PNM say "fuxx can't
     paste ⟨FORMAT⟩ images. Try PNG or JPEG."; a TIFF or BMP that can't be read says "Couldn't read that image from
     the clipboard."
  - **How:** the composer asks a pure `agent_paste::pasted` what the clipboard holds (files first, then text, then
    the first image, else nothing) and emits `PastePaths`, `PasteImage` or `PasteNothing`; the tab's side reuses the
    drop's `drop_external` (no copy of it) and the existing notice row. `attach_pasted_image` moved into the new
    `src/agent_paste.rs` (`agent_tab.rs` shrinks); `image_take` says per format: take, make PNG, or refuse.
  - **AI's calls:** the PNG comes from macOS's own reader (`NSBitmapImageRep` → PNG, the objc2 feature fuxx already
    had), not a new crate: the agent browser's `png_bytes` already did TIFF → PNG this way, so its last two steps are
    now the shared `browser::png_from_data`, and the clipboard's conversion runs in `add_image`'s background job
    (`ImageFrom::ClipboardToPng`), off the UI thread, then meets the same size limit. The notices go in the chat's
    notice row, as "Image too big" does: the box has no notice of its own, and a new surface wasn't asked for.
    SVG, ICO and PNM are refused rather than tried. Terminal panes, rename boxes and the palette paste as before (the
    composer is only the agent box). A paste that fails after it starts (an unreadable TIFF, as a too-big image
    already did) still uses up its "Image #n" number.
  - **Checks:** 2852 → 2860 tests (`--features ui-shots`: 2902 → 2910): the routing as a pure function (Finder's
    files beat their names, text beats an image, the first image, empty parts paste nothing), the per-format
    decision, the notices' words, and a real conversion of a TIFF and a BMP built in the test (a PNG of the same
    size; bytes that are no image give the notice), each watched failing first. New state scene `agent-box-paste`
    feeds the scene's own clipboard (never the real one): the empty notice, a TIFF as a PNG pill, the SVG and
    unreadable-TIFF notices, Finder's files as two path pills and an image pill (not their names), text still text,
    nothing sent; it failed with the empty-paste notice taken out. Clippy 14/29 and 18/33 (none in the changed
    files); `cargo fmt --check` clean. The agent-box scenes: 59 passed (16 shots "look different"; `agent-box-effort-list`,
    checked on master without this change, differs the same).
    Full `scripts/ui-shots.sh`: 361 passed, 1 failed (`agent-tab-wide`, display-dependent: it needs a screen at
    least 1600 pt wide and only the 1512 pt one was attached), 33 look different, 8 differ in size (the same clamp).
  - **On screen:** not yet seen in the real window: the real clipboard (a Finder copy, a TIFF from Preview) wasn't
    tried in the app.
- **A long conversation no longer makes the agent tab lag** (your words, 2026-11-24: "when the conversations get too
  long, when I scroll and sometime when Im not scroll either, its laggy. trace it for me, find root cause and come
  up with permanent solution", then "yes build it as a chore"). Branch `chore/agent-tab-perf`.
  - **Root cause:** a live `sample` of v0.13.1 on a very long Claude session had the main thread busy 22 % of the
    time while the agent worked, nearly all of it the agent tab's prepaint, paint and layout
    (`agent_chat::shows_thinking`, `rows_of`, `agent_lanes::pill_labels`, `item_row`). Two things together: (1) GPUI
    marks every ancestor view dirty when a child view notifies, so each tick of a spinner, the caret or a clock, and
    each scroll frame, re-renders the whole tab (GPUI's design, kept); (2) that render, and the sync after every
    streamed line, did work in proportion to the whole conversation: the last row's thinking line rebuilt Main's
    rows and scanned every item, the pill labels were rebuilt per frame, `status()` scanned for permissions, and
    `sync_rows` rebuilt the order, roles and every row's print, hashing each message's full text, per line.
  - **What changed:** the conversation's items are one store (`src/agent_items.rs`) changed only through methods
    that note what changed: a version per item, the lowest index changed since the tab looked, and indexes kept as
    items change (running calls, open permissions, tool ids, items per parent), so `status()`, the pending
    permission, the oldest ask, a lane's step count, the tool marks and the ticker scan nothing. Each view's rows
    live in a `ViewCache` (`src/agent_view_cache.rs`): its order, roles, prints, open check failures and thinking
    flag, derived again only from the first changed item (back to the start of its run of tool rows), with row and
    step prints going by item version instead of text. It starts again only when the lanes or the relays passed on
    change, the tracks' fit changes, or a hidden view is shown after changes. The render path only reads: the
    thinking line from the status and the running calls, pill labels kept per lanes' shape and width, the "Stopped"
    rewind link found with the ledger. The ledger isn't rebuilt while only the agent's words stream. The pump hands
    the tab everything already read as one batch (at most 256 lines): each line handled as before, the rows synced
    once per run of lines.
  - **AI's calls:** the fix makes each render and each sync cheap rather than fighting GPUI's ancestor
    invalidation. A view not shown is rebuilt once when shown again after changes (cheap now that nothing is
    hashed), rather than following every change. A lane's view finds its order again each sync (a plain scan, no
    hashing): its relays and waiting questions sort out of item order, so only Main's order is extended
    incrementally. The ledger stays a per-turn rebuild, now only on changes other than streamed words. The burst
    batching keeps every line's own handling and events (the status, the question, queued sends) and only defers
    the rows, instead of deferring to the next frame, so nothing reads stale rows. No new crate.
  - **Checks:** 2860 → 2870 tests and one ignored bench (`--features ui-shots`: 2910 → 2920), each failing first (as a missing type or function; the store's tests were
    written with it): work counts on
    10,000 items (a new message, 100 streamed deltas, a call and its finish each derive at most 4 rows; a shown-again
    view, a rewind, an Undo and a width change rebuild once; nothing changed derives nothing), counters proving no
    item's text is hashed and `rows_of` doesn't run on Main's streaming path, a fixed-seed property test of 1,500
    random edits (messages, deltas, calls, finishes, turn ends, lanes started, stepped, ended and resumed, relays
    passed on, permissions, check runs, rewinds and Undos, width flips) where the incremental Main and lane caches
    equal a full rebuild and the kept indexes equal a scan (three mutations of the incremental rules each made it
    fail), the store's own tests, the labels cache and the burst drain. **Before/after (release build, this
    machine, indicative only):** one long-chat sync after a delta on 10,706 items 4.9 ms → 13 µs; the last row's
    thinking check 186 µs → about 1 ns. New state scene `agent-long-chat` (3,212 items, 3,004 rows, lanes,
    markdown) draws, streams ten pieces, scrolls to the top and back; it logs frame times (about 120 ms each in the
    debug scene build against 115 ms for an empty tab: the frame read back dominates, so no budget is asserted).
    Clippy 14/29 and 18/33; `cargo fmt --check` clean. Full `scripts/ui-shots.sh`: 362 passed, 1 failed
    (`agent-tab-wide`, which needs a screen at least 1600 pt wide), 38 look different (the animation-heavy agent-box
    menus, the shell cards, the graph card and the sliding chrome, which vary run to run: one rerun alone moved from
    0.19 to 0.05), 8 differ in size (the same display clamp as before).
  - **After the review** (no must-fix; one fix wave): the open check failures keep the turn's check runs and find
    them again only from the change on; the ledger is kept (`LedgerCache`: its sums up to the first running call
    and 16 items behind the end); a lane's rows and its ledger come from what the store keeps (its items, what it
    asked, the relays, the open asks; no scan, no set of every tool id); Main's order is extended in place; the
    pump yields after a full batch so the window draws during a flood. The property test now checks against code
    it doesn't share (fingerprints and versions under the noted index, `rows_of`, `shows_thinking`, a full
    recompute of the failures and the ledger), on 6 × 1,200 random edits of many more kinds, Main and every lane at
    once; it found Undo Rewind taking your latest message from the cut, stale once an earlier unread message was
    cancelled (it is found again now). Release, this machine, indicative, per event: open failures in a 3,000-call
    turn 226 µs → 15 µs (the whole Main sync); its ledger 233 → 16 µs; a lane's sync per delta at 10.7k items 183 →
    3 µs; a lane's ledger 17 → 3.4 µs. Tests 2870 (`--features ui-shots` 2920; one moved, one dead helper's test
    gone, one new); clippy 14/29 and 18/33; `cargo fmt --check` clean. The agent and
    lanes scenes 157 passed, 1 failed (`agent-tab-wide`); full `scripts/ui-shots.sh` 362 passed, 1 failed (the same,
    display width), 34 look different (the animation-heavy set), 8 differ in size (the same clamp).
  - **On screen:** not yet seen in the real window: the feel of scrolling and streaming in a long session is yours
    to judge.
- **The 2026-11-24 chore batch** (twelve chores agreed with you item by item; design
  `docs/superpowers/specs/2026-11-24-chore-batch-design.md`, plan `docs/superpowers/plans/2026-11-24-chore-batch.md`;
  branch `chore/batch-2026-11-24`, merged to master, not pushed). Built small → medium → big, the tests per tier:
  - **Small.** The usage gauge opens on a 0.8 s rest (no click; `hover_delay.rs`); the status mark sits closer to the
    folder chip; tabs mark only Working and Waiting, plus a small green "done, not seen" dot until you open the tab;
    a blinking caret in every one-line field (`caret_blink.rs`: a 0.5 s timer that stops after 20 s idle, out of
    focus or in the background; follows the cursor-glow switch); the zsh suggestion list as a greyed gutter list in
    fuxx-completes (branch `chore/gutter-list` there, **not committed**: its AGENTS.md waits for your word).
  - **Medium.** Cost and context behind a clock between the model pill and the gauge (same hover; a warning dot past
    80 %; the top bar keeps only its margin); the live status moved out of the folder tab into a strip over the box
    (`agent_status_line.rs`: "Task 5/6 · Cultivating… · Running tests · 0:31", "Waiting for you · …", "Done · 0:42",
    the counts under it open the ledger's popover); the folder tab keeps the mark, folder and branch; the model
    name gives way first in a narrow footer. A workspace keeps its folder whatever tabs it holds (`Workspace.home`,
    saved; an older save learns it from its terminal). Closing an agent or terminal tab asks first, with "Don't Ask
    Again" (Preferences → General → "Ask before closing a tab" turns it back on); a workspace asks once.
  - **Big.** The activity tracks as icon tiles (eye, magnifier, pen, terminal, ✓, ✕) under phase bands (Looking
    around → Ran → Changing → Checking; a legend when a band is too narrow for its word), a numbered column hover
    ("#5 · Read …"), states in words ("1st try failed", "expected — test written first"). ⌘Z / ⇧⌘Z in every text
    field (`undo_history.rs`: word-sized steps; paste, delete and a ↑ recall one each; a send clears it); at a shell
    prompt ⌘Z sends the shell's own undo (⌃_). Plan progress: a burger tab "☰ 4/6" at the folder-tab row's right
    (⌥⌘P), its popover listing ✓ done (with its commit), ◐ being built, ○ next, then the subagents.
  - **Rulings that changed what you asked** (the rest are in the plan's ledger): ⇧⌘Z does nothing at a shell prompt
    (no shell binds a redo key fuxx could rely on; sending one typed junk); closing the window doesn't ask (it is the
    quit flow, where sessions may keep running); removing an attachment chip isn't undone (the box's pasted-image
    pills are); the plan comes from Claude's checklist file, since Claude 2.1.331 in `-p` keeps no task list (a
    file it only read counts only with `### Task N` headings, and commits mark tasks only after the file existed);
    subagents show as rows, not lists; the shell tab had no status pill to move closer; bash and fish draw no
    suggestion list to restyle.
  - **Checks.** Tests 2933 green; clippy: no new warnings (the rest is the baseline); `cargo fmt --check` clean;
    fuxx-completes' zsh suite 181 green. Full `scripts/ui-shots.sh`: 363 passed, 1 failed (`agent-tab-wide`, which
    fails on master too); references refreshed. One whole-branch review on the most capable model: no critical,
    seven important (all fixed with tests), the minors fixed or noted.
  - **Your on-screen check** (not seen in the real window yet):
    1. Rest on the gauge and on the clock: each opens after about a second, not before, and closes when you leave.
    2. A tab that finishes while you're elsewhere shows a small green dot until you open it; idle and seen tabs
       show nothing.
    3. The caret blinks in the palette, search and rename fields; not in the agent box or a terminal.
    4. Run an agent: the strip over the box names what it runs and its time; the counts open the popover.
    5. Close an agent or terminal tab: it asks; "Don't Ask Again" stops it.
    6. A workspace with only an agent tab left stays in its folder.
    7. A busy step's tracks: icons, bands and the column hover read clearly in both themes.
    8. ⌘Z / ⇧⌘Z in the box and a field; ⌘Z at a shell prompt undoes the typing.
    9. With a plan file (`### Task N` headings) the burger shows its count; ⌥⌘P opens it.
    10. In zsh with fuxx-completes from its branch, the suggestion list looks like the gutter mockup.
- **The chore batch's four minors** (your words, 2026-11-25: "fix them"; on master, not pushed):
  - **The plan tab never meets a long folder tab.** The folder tab and the plan tab share one row: the folder tab
    gives way first (its folder and branch cut with "…"). New scene `agent-plan-tab-narrow` (460 pt, a long folder
    and branch); it failed on the old layout (the branch chip ran under the burger) and passes now.
  - **A run of ⌫ undoes in one step**, in the box and every field, as macOS fields do; deleting a selection, and a
    new line in the box, stay steps of their own (`undo_history::a_run_of_deletes_undoes_as_one_step`).
  - **One hover rule.** The status bar's usage strip now uses the agent box's `HoverDelay` (a press still opens it
    at once: `HoverDelay::open_now`), its own copy gone.
  - **Less work per frame.** The status line is built once a frame (the scenes ask for it apart); the clock's
    panel only while it is open; a caret key in the box no longer copies its text.
  - **Checks.** Tests 2935 green; no new clippy warnings; `cargo fmt --check` clean; the scenes these touch 364
    passed, 1 failed (`agent-tab-wide`, as on master).
- **Scroll lag: the status marks are sprites, not paths** (your words, 2026-11-25: "when large content, the scroll
  seems laggy" … "go ahead, swap the marks to svg"; released in v0.13.6):
  - **Found.** A 20 s `sample` of the installed app (v0.13.5) while you scrolled a long agent chat: the main thread
    spent about 2.2 s blocked in `CAMetalLayer nextDrawable` (all three drawables still with the GPU), while fuxx's
    own layout and paint took about 1.4 s. GPUI draws every batch of paths by ending the render pass, clearing and
    resolving a whole-window 4× MSAA texture, and starting a new pass; the chat's finished steps each painted a
    check as a path, between text, so a long chat cost the GPU many whole-window passes a frame.
  - **Changed.** `status_mark` now builds each mark from layers (`layers`, tested): its discs and rings are rounded
    quads, and Working's arc, the check and the ✕ are SVG glyphs (`assets/icons/marks/`, `Icon::MarkArc`,
    `MarkCheck`, `MarkCross`), tinted like the Lucide icons. The arc turns by an SVG rotation, so it is drawn once and
    never re-tessellated; the sizes in a 12 px box are the paths' (ring 10 px, halo 12, dots 7 and 10, check 8).
    `mark_on`, `done_check` and `failed_cross` keep their signatures, so no caller changed.
  - **The other fixed shapes too** (your words: "any icon that is not svg? we can make it as svg then just release
    it"): a composer tab's concave step (its wedge and edge, `terminal_view::paint_tab_step`, both boxes), the agent
    box's send notch (its fill and lit edge, `agent_box::notch`) and the tab colour menu's "no colour" slash are SVGs
    in `assets/icons/shapes/`, painted with `window.paint_svg` in the same places. A test per shape checks its SVG
    still fits the box its constants give (the notch's 58 pt from `SEND`, `NOTCH_GAP`, `JOINT`; the step's
    `TAB_STEP` 8; the swatch's inside).
  - **Still paths, on purpose:** the agent box's sweep (a comet whose length, colours and fade change every frame
    along the box's outline) and the Graph panel's lanes (lines and curves laid out from each commit's parents).
    Neither is an icon: both are drawn from data each frame.
  - **Checks.** Tests 2942 green (seven new); no clippy warnings in the files touched (the existing ones are in
    other files); `cargo fmt --check` clean; the scenes these touch 364 passed, 1 failed (`agent-tab-wide`, as on
    master); `marks-mocha`, the lanes' ✓/✕, the notch and the tab step look as before (no scene opens the colour
    menu, so the slash is checked by its test only).
  - **Toolchain.** Homebrew's Rust 1.100 comes before rustup's 1.97.1 on this Mac's PATH and has no Intel standard
    library, so `scripts/bundle-mac.sh` failed with it; the build and gates here ran with `~/.cargo/bin` first.
  - **Your on-screen check:** in a build with this, scroll the same long agent chat, a long terminal and a big file
    tab; it should keep up. (The terminal and file-tab lag weren't profiled yet: if either still lags, it's another
    cause.)
- **The 2026-11-25 chore batch** (14 chores, your words with each; branch `chore/batch-2026-11-25`, not merged or
  pushed yet; design `docs/superpowers/specs/2026-11-25-chore-batch-design.md`, plan
  `docs/superpowers/plans/2026-11-25-chore-batch.md`, one task per chore):
  1. **Commit box: a 2-line text area** ("need to be textarea, max 2 lines. buttons, lightgreyed them out when
     disabled"): the field is a bare `Composer` capped at 2 rows (`set_max_rows`), ↵ commits, ⇧↵ a new line, ⌘↵
     still commits. ✦ and Commit take an off ink halfway to the card's fill when they can't act (the theme's faint
     ink is its dim one, so they used to look on). It types in the terminal's mono font now (`3d5c7dc`).
  2. **The clock panel centred** over its icon ("make it appear in the middle"): `menu_frame_at` (`5277a7b`).
  3. **Your message's hover icons stay** ("icons disappear when I hover them"): the icon strip blocks the mouse,
     which makes the row behind read as not hovered (GPUI's `BlockMouseExceptScroll`), so it faded out under the
     pointer; its own hover keeps it shown (`7182cf7`).
  4. **The plan tab flush right**, mirroring the folder tab (your picture): its step mirrored on its left
     (`tab-step-*-left.svg`), the box's top-right corner square under it (`44ab245`).
  5. **A click elsewhere clears a track column's cursor** ("the 'active' border is not removed"), unless its call's
     detail is open; the keyboard goes back to the box only if the tracks still held it (`ff9ab09`).
  6. **The plan popover over the chat** ("should be left aligned"): it opens leftward, its right edge on the tab's
     (the box's edge). Built so because, with the tab flush right, a popover hung from the tab's left would hang
     280 pt past the box. Your call after a sample of both (2026-11-25): "keep A" (`b59964e`).
  7. **Your bubble wraps in a narrow pane** ("user's chat not as responsive"): the bubble could not shrink below
     its text's one-line width, so it ran off the left; `min_w(0)` (`b0566de`).
  8. **A copy icon on the chat's code blocks** ("no copy icon to copy the command"): top-right, dim, ✓ for 1.5 s
     (`34db382`, `d423609`).
  9. **Folder picker, mockup A**: the name over its place, ✓ and the accent tile on the current one, ⌘1–⌘8 on
     Recent's rows (Recent keeps 8), ↑/↓ and ↵, icons on the actions, a key-hint footer, Home's house (`bd4081c`).
  10. **Changed files, mockup A**: a total, files under foldable folders, A/M/D and +/− from git
      (`changed_files.rs`, read off the UI thread). Your duplicates were paths a Python script only named
      (`icons/shapes/…` in `icons.rs`'s text) and a folder; only real files show now (`10eed67`).
  11. **The Plan card** ("We cannot read it… let me see the mockup"; refined on the fuxx-look mockup): Claude's
      `ExitPlanMode` request becomes a card with the plan rendered, folded past 12 lines (Show the whole plan /
      Show less, outline chips for long plans), its file ↗, and Approve · auto-accept edits ⏎ (sends Claude's
      `setMode acceptEdits` permission update), Approve · ask per edit, Keep planning… (your note as the reply).
      Typing a note hides the buttons; Esc brings them back; a slim answer bar is pinned over the box while the
      card's buttons are out of view; answered, it folds to one line (`82e75b1`).
  12. **No ◐ before work has started** (your pick): a task shows in progress only after a task is done, a box is
      ticked, or the agent's todo says so (`5bd68a3`).
  13. **The branch chip keeps up** ("branch on outer chatbox is not align"): read again after each tool call, at a
      turn's end and when fuxx comes back to the front, at most every 500 ms (`2d75124`).
  - **Review** (one whole-branch review): 5 Important and 7 Minor. Fixed: Esc while typing a plan note stopped
    Claude; a leftover note hid the next plan's buttons; a plan's buttons answered the oldest permission, not their
    own; read files showed an M badge; the pinned bar read a stale footer place; a long note ran past its card;
    folders still listed as changed (`2982506`). Deferred minors: one fold set shared by the popover's sections;
    ↑/↓ in the picker doesn't scroll the lit row into view; the ledger's git read runs as Read paths grow, popover
    closed; `plan_title` can take a `#` line from a code fence; two doc comments on the wrong item.
  - **Checks.** Tests 2980 green; no new clippy warnings in the files touched; `cargo fmt --check` clean; the scenes
    these touch 366 passed, 1 failed (`agent-tab-wide`, display-dependent, as on master); new scenes
    `agent-tab-plan`, `agent-tab-plan-long`, `agent-rewind-hover-stays`, `agent-you-narrow`.
  14. **The plan tab keeps its plan** (added after the review, your word "go ahead"): it hid this batch's tasks
      mid-build, because the last `.md` a turn wrote was taken as the plan (the progress ledger, no tasks). Now the
      turn's `.md` files are tried newest first (written, then read) and the first that is a plan wins
      (`first_plan`: 2+ tasks; a file only read needs `### Task N` headings); none, the plan shown already stays
      (`plan_candidates`), until its tasks are done.
  - **Checklist for you** (GUI only, in a build with this):
    - [ ] the commit box: wraps to 2 rows then scrolls; ✦ and Commit light grey when they can't act
    - [ ] the clock panel opens centred over its icon
    - [ ] your message's hover icons stay while the pointer is on them
    - [ ] the plan tab sits flush with the box's right edge, its curve on its left
    - [ ] a click elsewhere clears a track column's border
    - [ ] the plan popover opens over the chat, right edge on its tab
    - [ ] your bubble wraps in a narrow pane
    - [ ] a chat code block's copy icon copies, then shows ✓
    - [ ] the folder picker: name over place, ⌘1–⌘8, ↑/↓ and ↵
    - [ ] the changed-files popover: groups, A/M/D, +/−, ⌥-click opens the file, no duplicates or folders
    - [ ] a real Plan Mode run in each of the three answers, a long plan's bar, and Esc while typing a note
    - [ ] the plan tab shows no ◐ before work starts
    - [ ] the branch chip follows a `git switch` by the agent and in another terminal
    - [ ] the plan tab keeps a plan's tasks while the agent writes other `.md` notes, and across turns
- **Chore 15: the plan tab follows a plan built in another repo** (your words, 2026-11-25: "the tasks progress isnt
  updated on the outter right chatbox, is it a bug? if it is fix it as a chore"). Two causes: the tab read commits
  (and the plan's start) with `git -C <the tab's folder>`, so a plan living in another repo (here fuxx-landing's,
  built from a fuxx-cli tab) never saw its commits; and only a subject saying "Task N" counted, while those commits
  used the messages the plan itself gives each task. Now git runs in the plan file's own folder, and a commit whose
  subject starts with a task's planned commit message (the backticked text on its "commit" line,
  `planned_subjects`) marks it done (`match_planned`). Tests 2984 green.
  - [ ] Your check: a plan in another repo ticks its tasks off as their commits land.
- **Chore 16: background shells, beside the tasks tab** (your words, 2026-11-25: "we now dont have shells. I cannot
  see what shells are running. where do you think we should put it" → mockups → "love the popover, but is it good
  to put it beside tasks?" → "make it: :terminalicon x/n just like tasks" → "make it a chore and build it"; design
  `docs/superpowers/specs/2026-11-25-background-shells-design.md`, plan `docs/superpowers/plans/2026-11-25-background-shells.md`).
  Claude's `run_in_background` Bash calls (a dev server, a long build, a watcher) ran unseen.
  - **Built:** Claude's reader (`providers/claude/shells.rs`) turns a `local_bash` task with `is_backgrounded:
    true` into `ShellStarted` once its call's result names the output file, and `task_notification` into
    `ShellEnded` (`completed` with a non-zero exit is a failure). A foreground Bash is a `local_bash` task too, so
    it is left out. ■ sends `stop_task`, the same request a subagent's stop uses. A recording
    (`claude-background-shells.jsonl`, new `background-shells` scenario) showed this works for a shell. The state
    (`agent_shells.rs`) lives in the conversation. The **shells tab** `▣ 1/3` (Lucide terminal) sits on the plan
    tab's left in one corner tab, flush right; alone it takes the corner. It is peach while a shell runs, green
    `3/3 ✓` until your next message, and red `1/2 ✕` after a failure until you open it. The **popover** (over the
    chat, its right edge on the box's) shows "1 of 3 done · 2 running", then one row per shell: the running arc,
    ✓ or ✕, the description, a ticking time, the command, the last output line, and ■. Finished rows dim and drop
    off after 2 minutes. A row opens its output file in a file tab. The last lines are read off the UI thread once
    a second, only while the popover is open (`agent_shells_view.rs`). **In the chat** the Bash row reads "running
    in the background · 4:12" under the arc. A **banner** goes out when a shell ends while you aren't on the tab:
    "Background shell done", then "Run the tests · done in 6:40" ("· failed", "· stopped"). It goes through the
    agent banner's shared tail (`tell_agent`). A background `cargo test` is no longer counted as "checks pass"
    while it runs.
  - **Rulings:** the plan tab and the shells tab share one corner tab. Stopped shells count as done and show a dim
    ✕, and only a failure turns the tab red. The tab counts finished shells until your next message, while popover
    rows drop off after 2 minutes ("All finished" when none are left). A row click closes the popover as the file
    tab opens. **Claude only:** agy's `run_command` goes async on its own (`WaitMsBeforeAsync`) and sends no task
    events.
  - **Not covered:** when a step has 3 or more calls it folds into tracks, so "running in the background" shows only
    on readable rows (the tab and popover still show every shell). `/clear` leaves the shells as they are (not
    recorded whether Claude stops them).
  - **Review** (one whole-change review): 2 Important findings, both fixed. After a crash or restart, a Bash row
    kept saying "running in the background". After your message took the tab, its popover stayed open unseen
    (and `/` opened nothing). Deferred minors: an open popover with nothing running doesn't redraw to drop old
    rows; there may be two banners (the shell's, then Claude's wake turn's Done); after a crash the stopped shells
    turn the tab green; in a resumed conversation a background `cargo test` may read as passed; the reader keeps
    the command of a background call that never started.
  - **Commits:** `37a70c3` (events and the reader), `7a2753f` (the state), `9bf9010` (tab, popover, chat row,
    banner), `c5e6861` (scenes, the background-check fix), `25ae2a3` (review fixes).
  - **Checks.** Tests 3001 green. No new clippy warnings in the files touched. `cargo fmt --check` clean. The
    scenes these touch: 375 passed, 1 failed (`agent-tab-wide`, display-dependent, as on master). New scenes:
    `agent-shells-with-plan`, `-alone`, `-done`, `-failed`, `-popover`, `-cleared`.
  - **Checklist for you** (GUI only, in a build with this):
    - [ ] ask Claude for a background shell (a dev server): `▣ 0/1` appears beside the plan tab (or alone in the corner)
    - [ ] the popover: its time ticks, the last output line follows the shell, and ■ stops it
    - [ ] a row opens the output file in a file tab that follows it as it grows
    - [ ] the Bash row reads "running in the background · M:SS" and stops when the shell ends
    - [ ] a shell that ends while you're on another tab sends one banner (watch for a second one from Claude's wake turn)
    - [ ] a failed shell turns the tab red with ✕ until you open the popover; all done shows green `n/n ✓` until your next message
- **Chore 17: one working mark, not two** (your words, 2026-11-26: "the loading is kind of a duplicated, it looks
  not good" → four options → "go with B, put it in a chore list"). **Built:** while the chat's status line shows,
  the folder tab drops its status mark (`status_line_shown`, shared by the line and the tab); the mark comes back
  when the line goes (a subagent's view, a restored conversation). Commit `cfcda49`.
  - [ ] Your check: a working turn shows one spinner (the status line's), none in the folder tab.
- **Chore 18: the plan tab's left step meets the box's line** (your words, 2026-11-26: "the line is offside").
  **Cause** (a pixel dump of both steps in `agent-plan-tab`): the folder tab's step is drawn after its tab, so its
  wedge covers the tab's straight edge; the corner tab's mirrored step was drawn *before* its tab, so the tab's
  straight left edge ran on down to the box's line beside the curve. **Built:** the step is drawn after the corner
  tab; a pixel check pins the curve alone meeting the box's line (both themes, 2x; 1x not testable here).
  Commit `25521df`.
  - [ ] Your check: the corner tab's left curve meets the box's top line cleanly, both themes.
- **Chore 19: subagents move to a corner tab; the top row becomes a way back** (your words, 2026-11-26: "Love
  subagents on the popover list. can we just move there and make it clickable? we remove the one on the top. Or
  maybe we add 1 more section beside the task and the shell, add icon ':robot #N'" → "queue it"). **Built:** a third
  corner pill `🤖 1/2` (`agent_subagents.rs`, `agent_subagents_view.rs`): peach while one runs, green `n/n ✓`,
  red while one needs you. Its popover: "1 of 2 done · 1 running", the ones that need you first, then running
  (arc and current action), then finished, newest first; ten rows, then it scrolls. A row opens that subagent's
  view; a "Needs you" row (red) opens it with the keys on its question. The plan popover keeps only its tasks. The
  pill row and its "+N" list are gone; inside a subagent the header row is the way back:
  `← Main › 🤖 <label> ◌ kind … time · tokens · Stop`. ⌥⌘[ / ⌥⌘] still step. A corner pill now opens its popover from
  the press that closed another pill's (it took two clicks). Commit `9bcdd46`.
  - **Rulings:** the breadcrumb is the header's row, not a second row (the label would show twice); order 🤖 ▣ ☰
    (the plan stays flush right); a "Needs you" row opens that subagent's view (its question shows in every view);
    the popover regroups live.
  - [ ] Your check: subagents show `🤖 n/m` in the corner; its popover opens each one; `← Main` goes back; no
    pill row on Main; ⌥⌘[ / ⌥⌘] step.
- **Chore 20: the plan tab ticks off chore 16's plan** (seen in your 2026-11-26 picture: "Plan · 0 of 5").
  **Cause** (the real plan and `git log`): its commits say "Chore 16: …" (no "Task N"), the plan gives no planned
  commit messages, its boxes were never ticked; the plan's start was fine. **Built:** a task is done when the
  commits since the plan touched more than half of the files its section names (`task_files`, `match_files`;
  `git log --name-only`); a commit counts for a task only if it is newer than the commit an earlier task was done
  by, or touches a file only this task names (the review: one commit ticked the next tasks too). Chore 16's plan:
  0 of 5 → 5 of 5 (tests from its text and commits, and a replay of this batch's own plan, commit by commit).
  Commits `e178869`, `dc848de`.
  - [ ] Your check: a plan whose commits don't say "Task N" ticks off one task per commit.
- **Chore 21: no empty band above the subagent row** (your words, 2026-11-26: "what's the empty gap?").
  **Cause** (measured): the agent tab's old top bar kept its 32 pt after its status and cost moved away, plus the
  pill row's 5 pt (74 px in a 2x picture). **Built:** the bar keeps only its width probe; Main's first row and a
  subagent's `← Main` row sit the chat's 16 pt under the tab bar (pinned in `agent-subagents-tab`). Commit `5d7ebd6`.
  - [ ] Your check: no empty band at the top of an agent tab, on Main and inside a subagent.
- **Chore 22: the status line's counts become chips on its right** (your words, 2026-11-26: "the changed/checks,
  it doesnt look good, can you suggest me the best place to place it?" → "queue it"). **Built:** one row: on the
  left the mark and the line (only its action cut with "…"; the word, time and "+N more" stay), pinned right
  `⧉ 4 changed` and the checks chip (✓ passing, an arc while a check runs, red ✕ "tests"/"build"/"lint" when one
  fails, a ring when they ran but not all pass). Each chip opens its part of the ledger popover (files, or checks),
  hung from the chips; it closes when the chips go (your next message, a subagent's view). The status row lives in
  `agent_status_view.rs`. Commits `8b52ed1`, `dc848de`.
  - [ ] Your check: a running turn shows `⧉ N changed` and the checks' arc on the status row's right; each chip
    opens its list; a narrow pane cuts the action, not the time.
- **Chore 23: no overlap after a plan is approved; a smaller, tighter plan card** (your words, 2026-11-26: "also
  fix this layout messy overlapping … The plan mode floating buttons, looks very bad and everything too big.
  redesign it and don't make too much spaces" → "yes, build 23 too and release with the rest"). **Cause**
  (reproduced in `agent-plan-long-command`): an answered permission's line was a fixed 22 pt row whose text kept
  the command's newlines; the 111 pt block spilled up and down over the rows round it (the list's own heights were
  right). **Built:** the line shows the command's first line with "…" (a long one-liner is cut too); a click opens
  it whole and the rows below move down. The card: chat-size text, 12 pt in, one header line
  `☰ Claude's plan · retry-current-endpoint.md ↗ · 4 steps`, folded after 12 lines ("Show all ▾"), the answers in one
  row of 24 pt buttons (they wrap in a narrow pane), and with them scrolled away a slim pill
  `☰ Plan waiting · Approve ⏎ · Keep planning` over the box. Answered, it folds to one line even if open
  (`☰ Plan approved · auto-accept edits ✓ … Show plan ›`); opened again it reads only (Show less at its top).
  Found on the way: the pill showed with the buttons in view (its footer probe sat under the buttons).
  Commits `aba2de8`, `f0f5e51`.
  - **Rulings:** the hint under the buttons went (the box's placeholder says it); no separate 16 pt title line.
  - [ ] Your check: approve a real plan; no rows overlap; a long command shows one line until clicked; the card
    and pill look right; the answered plan folds.
- **Checks for chores 17–23.** Tests 3008 green. No new clippy warnings in the files touched. `cargo fmt --check`
  clean. Scenes since `5103f17`: 379 passed, 1 failed (`agent-tab-wide`, display-dependent, as on master). New
  scenes: `agent-subagents-tab`, `-popover`, `-waiting`, `-many`, `agent-plan-long-command`,
  `agent-plan-waiting-narrow`; removed: `agent-lanes-overflow`, `agent-lanes-list-grouped` (the "+N" list is gone).
  The visual fingerprints (`tests/ui-shots-ref`) were not refreshed.
  - **Review** (one whole-change review of 17–22 on Opus, then a fresh one of 23's diff): 17–22 had 3 Important,
    all fixed (the plan tab ticked tasks early; the ledger popover stayed open unseen, holding the box's keys;
    the whole status line was cut instead of its action); 23 had 1 Important (the command text could neither cut
    nor wrap) plus a narrow-pane minor and a long one-liner, fixed. Deferred minors: a "Needs you" row lands on the
    oldest question when two subagents wait; the breadcrumb's label doesn't shorten in very narrow panes; the
    subagents and shells popovers share a frame shape not yet one helper; `PlanDraft.title` is now unused.
- **Chore 24: a scene that ends on the folded approved plan** (your words, 2026-11-26: "queue it as a chore", after
  the landing update for v0.13.9; built with the rest: "include 24 too"). **Built:** a new scene,
  `agent-plan-approved-folded`, ends on the folded one-line card `☰ Plan approved · auto-accept edits ✓ … Show plan`
  (chore 30's chevron), checking its words, the chevron on the line and no plan body drawn; chore 26's
  `agent-plan-approved-auto` ends the same way on "Plan approved · Auto mode ✓". `agent-plan-approved` still checks
  the plan opened again. Commit `af083d6`. **Left for after the release (the landing side, done separately):** point the landing's `plan-approved`
  shot at `agent-plan-approved-folded` (`pnpm sync-shots` in fuxx-landing) and ship the site.
  - [ ] Your check: the landing's "Plan approved" picture comes from the new scene, uncropped.
- **Chore 25: a calmer, shorter permission card** (your words, 2026-11-26, with a picture of a Bash request for a
  long `git commit -F - <<'MSG' …`: "redesign this."; the design below confirmed 2026-11-26: "yes both, build 25 to
  30 and release"). **Built** (`agent_permission_card.rs`): the title is the tool's own description ("Create the
  commit and show it") with the tool as a small tag (`Bash`; a subagent's reads `Reader · Bash`; no description: "Claude
  wants to run Write", no tag); the command once, mono at the chat's small size, folded after 6 lines ("… 7 more
  lines · Show all" and a chevron; Show less folds it back); no raw JSON for Bash (Claude leaves its input out, the
  command and description say it all); any other tool's input folded behind "Show input"; the answers in one row of
  24 pt buttons shared with the plan card (`agent_card_parts.rs`): `Allow ⏎`, `Always allow git commit *` (a Bash
  rule now reads as its command, `:*` as " *"), `Deny`, and "or type to redirect" quiet on the row; a 1.5 pt accent
  edge and the shield on the panel fill, no tinted fill. New field `about` on the permission event (Claude: a Bash
  call's `description`). Scenes: `agent-permission-short`, `-heredoc`, `-input`, `-narrow`. Commit `4ccfb3d`.
  - **Rulings:** no tag without a description (the title already names the tool); Write/Edit keep their input
    behind "Show input" (a file name alone isn't the content); the fold shares chore 23's open state with the
    answered line.
  - [ ] Your check: a long heredoc commit asks with one short card; Show all / Show less; an edit's "Show input".
- **Chore 26: approve a plan into Auto mode** (your words, 2026-11-26: "on plan mode, no accept and goes auto."; you
  confirmed 2026-11-26, "yes both": an answer "Approve · Auto mode" that approves and switches the session to Auto,
  as Claude Code's own plan approval offers). **Protocol** (recorded, `claude-plan-auto.jsonl`, Claude Code 2.1.331):
  the `ExitPlanMode` prompt answered `allow` with `updatedPermissions: [{"type":"setMode","mode":"auto",
  "destination":"session"}]` leaves plan mode into Auto: `system/status permissionMode: "auto"` at once, and the Write
  after it asks nothing. **Built:** the plan card's row reads `Approve · auto-accept ⏎`, `Approve · Auto mode`,
  `Approve`, `Keep planning`; the new answer (`Answer::ApprovedInto`, `Action::AllowInto`, the provider's
  `plan_into_mode`, Claude: `auto`) sends that allow, the mode chip shows Auto at once, and the card folds to "Plan
  approved · Auto mode ✓". Scene `agent-plan-approved-auto`. Commit `e12608d`.
  - **Ruling:** the new answer sits second, after the Enter default (auto-accept), so Enter keeps its meaning.
  - [ ] Your check: approve a real plan with Approve · Auto mode; the chip reads Auto and the next actions don't ask.
- **Chore 27: switching to Auto takes effect on the next action** (your words, 2026-11-26: "when I accept the plan
  mode with accepts edits, then I switch to mode Auto, next action is not auto. check when I change it to auto on a
  normal mode."). **Cause** (traced, then recorded): fuxx held any mode chosen while a turn ran or a card waited
  (`step_for` → after the turn, M11ak's "a change never cuts the turn"): the chip showed Auto, but Claude was only
  told when the turn ended, so the rest of the turn ran in the old mode. The same from a plain session mid-turn;
  idle, it was already live. Claude takes the change mid-turn: recorded (`claude-mode-mid-turn.jsonl`), Auto asked
  at the turn's Write prompt is answered `{"mode":"auto"}`, reported at once, and the turn's next Write asks nothing
  (the Agent SDK docs say the same). **Built:** a provider says it takes a mode mid-turn (`mode_mid_turn`, Claude:
  yes; agy still waits), so the change is sent at once; the chip shows the asked mode at once. Tests on `step_for`
  and on the recorded control messages; the recorder gained `mode-mid-turn` and `plan-auto`. Commits `8b1e0ca`,
  `868dca3`.
  - [ ] Your check: approve a plan with accept edits, pick Auto mid-turn: the next tool call doesn't ask; the same
    from a plain session.
- **Chore 28: a thin blinking caret in the commit box** (your words, 2026-11-26: "make the cursor on this to be like
  this with blinking, do not put the glow cursor, we need to keep performance brilliant"). **Built:** the commit box
  (a bare `Composer`) draws the one-line boxes' thin bar (`set_plain_caret`), blinking by the branch picker's rule
  (`caret_blink::Blinker`, now shared with `NameInput`): one timer at a time that redraws only the box; no glide,
  glow, trail or sparks. Pinned in `changes-commit` (a bar 2 pt wide, no breath frames). Commit `9ef8d2a`.
  - **Ruling:** the same blink rule as the one-line boxes (solid with the cursor-glow setting off, in the
    background, or after 20 s idle).
  - [ ] Your check: the commit box's caret is a thin blinking bar, nothing glows.
- **Chore 29: a little padding inside every text input** (your words, 2026-11-26: "any input text, make it has small
  padding inside so its neat"). **Built:** one token, `ui_tokens::input` (6 pt across, 2 pt up and down), set where
  the one-line field draws its frame (`NameInput`: rename, palette, branch picker, search and settings fields, host
  picker); the filter boxes' placeholders follow it. Pinned in `branch-switcher`. Commit `a01d116`.
  - **Ruling:** the bare fields (the agent box, the commit box, the right sidebar's filter, the browser's address)
    sit in boxes that already pad them, so the token is set once, in the frame.
  - [ ] Your check: the rename box, the branch picker and the palette's field have room round their text and caret.
- **Chore 30: chevrons, not arrow glyphs, in the confirmation cards** (your words, 2026-11-26: "for all popup
  confirmation, can you make all the arrows to be chevron?"). **Built:** Lucide's chevron-up added; one shared fold
  link (`agent_card_parts.rs`: the words, then a chevron at their size and colour): the plan card's `Show plan`
  (right), `Show all` (down), `Show less` (up), the permission card's folds (chore 25), and the shell card's fold row
  (`Show less` up, `N earlier lines` right, drawn before the words where the glyph was). Keyboard legends and the `→`
  in answers sent to the agent kept. A test keeps arrow glyphs out of the cards' sources; pinned in
  `agent-plan-approved`, the permission scenes and `shell-cards`. Commits `35000d5`, `1a0872d`.
  - [ ] Your check: every fold link on the cards ends (or, on a shell card, starts) with a chevron.
- **Chore 31: only the chat's running mark spins** (your words, 2026-11-26, with a picture of three spinners at once:
  "in here still too much loading indicator" → "yes, build 31 too and release with the rest"). **Built:** chore
  17's rule now reaches a subagent's view: the chat's running mark is Main's status line, or a live subagent's own
  view (its header and rows), one shared rule (`chat_mark_shown`). While it shows, the "To … · via Main" chip shows a
  still 🤖, the folder tab no mark, and the 🤖 corner tab only its count; the tab spins (a running subagent's arc)
  only while the chat shows no mark. The review found the folder tab's red "Waiting" lost in a subagent's view: it
  now hides there only while work runs. Pinned in `agent-lanes-relay` and `agent-subagents-tab`. Commits `10f87f2`,
  `868dca3`, `15ec0d5`.
  - **Rulings:** the relay chip lives only in a subagent's view, where Main's status line never shows, so the rule
    takes that view as the chat's mark; the subagent's own header arc stays.
  - [ ] Your check: inside a running subagent, one spinner (its own); the relay chip shows 🤖; a wait elsewhere
    still shows red on the folder tab.
- **Checks for chores 24–31.** Tests 3018 green. No new clippy warnings in the files touched. `cargo fmt --check`
  clean. Scenes since `5ba8ffc`: all passed but `agent-tab-wide` (display-dependent, as on master); two scenes that
  pinned the old behaviour (`agent-box-mode-waits`, `agent-subagents-tab`) now pin the new. New scenes:
  `agent-plan-approved-auto`, `agent-plan-approved-folded`, `agent-permission-short`, `-heredoc`, `-input`,
  `-narrow`. The visual fingerprints (`tests/ui-shots-ref`) were not refreshed. Plan:
  `docs/superpowers/plans/2026-11-26-chores-25-30.md`.
  - **Review** (one whole-change review on Opus): 1 Important (the folder tab's Waiting lost in a subagent's view),
    fixed with a test first. Deferred minors: "Hide input" uses chevron-up and the open input is still capped with
    "…"; chore 29 is pinned only on the branch picker; `toggle_input` repeats `toggle_command`'s shape; in Main with
    no status line a background subagent's arc shows in its tab and its chat row; approving into Auto while another
    mode change is pending shows that one until Claude reports Auto.
- **Chore 32 (queued, not built): the plan tab ticks a closing task that only runs things** (your words, 2026-11-26,
  with a picture of the 🤖 10/10 ✓ and ☰ 8/9 tabs, "Task 9: Docs, screenshots, review" still ◐: "last task is still
  hanging when its actually done" → my take → "queue it"). Cause, found: chore 20 ticks a task once commits touched
  most files it names; plan 2026-11-26-chores-25-30's Task 9 names `scripts/prune-target.sh` (one it runs, never
  edits; `is_file` in src/plan_progress.rs takes any spaceless backticked path) and its `- [ ]` box was never ticked,
  so it can never tick. (1) A file a task only runs doesn't count as one it changes (a `scripts/…` named as a step
  to run, or under no `**Files:**` line); a task naming no changed files ticks once a later commit lands after the
  previous task's (the closing docs commit). (2) Builds tick their plan's boxes as each task finishes (say so in
  the milestone build workflow, docs/05-Agentic-Workflow/50 - Agentic Workflow.md). (3) A test from that real plan
  and its commits: 9 of 9.
