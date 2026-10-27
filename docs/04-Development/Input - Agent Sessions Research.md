---
tags: [development, input, research]
---

> [!note] Input only, nothing decided (2026-10-27). A researcher agent surveyed multi-agent tools and proposed an "Agent Sessions" milestone. You chose "not yet": it is not on the roadmap. Claims marked unverified in the text are third-party only.

# Agent Sessions for fuxx: landscape research and a milestone proposal

*Written 2026-10-27 by a research agent. Read-only: nothing in the repo was changed. Sources are linked inline.
Where a claim comes from a single third-party source or couldn't be checked against official docs, it says
**(unverified)**. This is input for a decision. It is not a plan: per AGENTS.md rule 3, none of it is in scope until
you change [[12 - Scope]] and [[40 - Roadmap]].*

---

## 0. The one thing to know first

**Since fuxx's M7 plan was written, Claude Code has absorbed most of what the "orchestrator" apps were selling.**
- **Agent view (`claude agents`)** came out on 2026-05-11 as a research preview. It shows every background session in one screen with the states Working, Needs input, Idle, Completed, Failed and Stopped. You can peek at a session and reply inline, or attach to it.
- A supervisor daemon keeps those sessions alive.
- Each background session moves into its own worktree before it edits files.
- PR badges show CI state.
- `claude agents --json` is a stable way for scripts to read session state.
  ([agent view docs](https://code.claude.com/docs/en/agent-view.md), [announcement](https://claude.com/blog/agent-view-in-claude-code))
- Add `--worktree`, cross-session messaging, the Desktop app's diff, auto-fix and merge-when-ready flow, and agent teams on top of that.

Two of the standalone orchestrators have also folded:
- **Vibe Kanban is sunsetting.** Its company, Bloop, shut down in April 2026 because it couldn't make money from the product ([X post](https://x.com/tokengobbler/status/2042647208135123078), [README](https://github.com/BloopAI/vibe-kanban)).
- **Crystal was deprecated in February 2026** in favour of Nimbalyst ([README](https://github.com/stravu/crystal)).

So fuxx shouldn't try to be another Conductor. The opening that's left:
1. **One place for every vendor's agents.** Claude's tools only see Claude. Codex's tools only see Codex.
2. **Agents stay in real terminals you can see and type into.** Agent view and the GUI apps hide the PTY behind a chat or table UI.
3. **Exact, cross-vendor state, plus the reason an agent is waiting**, with the answer one keystroke away.
4. **Native speed.** cmux's recurring complaints are lag and CPU ([rywalker review](https://rywalker.com/research/cmux)), and most rivals run on Electron.

The proposal below goes after exactly those four.

> [!warning] Correction for the M9 note
> [[49 - Milestone 9 - Scripting API]] says cmux "doesn't know what an agent is doing". Its API page does list no
> event stream, no wait and no screen read ([cmux API](https://cmux.com/docs/api)), so that part holds. But cmux now
> has `cmux hooks setup` and a Claude hook wrapper that map Claude's session, notification and stop events onto
> workspaces ([README](https://github.com/manaflow-ai/cmux), [cmux Claude Code integration](https://www.mintlify.com/manaflow-ai/cmux/integrations/claude-code)).
> And Claude Code itself exposes agent state through `claude agents --json`. fuxx's "agent-aware API" is still unique
> **among terminals**, but not in the wider market. The note should say so.

---

## 1. Landscape

### 1.1 Table

| Tool | Approach | Standout features | Gaps / complaints |
|---|---|---|---|
| **Claude Code itself** (agent view, `--bg`, `-w`, hooks, Desktop) | CLI and TUI plus a supervisor daemon. Background sessions go into a worktree under `.claude/worktrees/` | Precise states from inside the agent. Inline peek and reply. PR and CI badges. `claude agents --json`. Cross-session messaging. Desktop app: line comments → new diff, auto-fix CI, merge when ready | **Claude only.** Agent view is a research preview with reported crash, resume and daemon-socket bugs ([issue digests](https://claudeissues.com/digest/2026-06-09), unverified individually). Its TUI replaces your terminal rather than living in it. Idle background sessions are reaped after about an hour unless pinned |
| **cmux** (Manaflow) | Native Swift terminal on libghostty. **No worktrees** | Blue pane rings and lit tabs. Notification panel. ⌘⇧U jumps to the latest unread. Sidebar shows branch, PR status, cwd, ports and the last notification. Socket and CLI. Browser. SSH. `cmux claude-teams` puts Claude teammates in splits ([README](https://github.com/manaflow-ai/cmux)) | Laggy scrolling and higher CPU than Ghostty. Over 2,600 open issues. No orchestration and no worktrees. GPL ([rywalker](https://rywalker.com/research/cmux)). API has no events, wait or screen read |
| **Conductor** (Melty Labs) | Mac app (Tauri/Rust). One worktree per workspace | Polished. Setup, run and archive scripts. Diff viewer with **line comments sent to the agent**. Create PR. Checks tab mirrors GitHub review threads. Workspaces from Linear or GitHub issues. "Spotlight" syncs a workspace to the repo root for testing ([diff viewer](https://www.conductor.build/docs/reference/diff-viewer), [scripts](https://www.conductor.build/docs/reference/scripts), [changelog](https://conductor.build/changelog)) | Closed source, Mac only, Apple Silicon only. Cloud workspaces are $50/month ([openalternative](https://openalternative.co/blog/best-ai-coding-agent-orchestrators), unverified). Checkpoints and multi-model compare are claimed by third parties only (unverified). Little public criticism found |
| **Superset** | Electron, xterm.js, node-pty. One worktree per workspace | **A 10-port block per workspace.** Setup scripts get `SUPERSET_ROOT_PATH`. Wrappers or plugins for Claude, Codex, OpenCode, Cursor and Copilot drive the states Running, Review and Idle. ⌘L opens the changes panel. CLI, SDK and MCP ([docs](https://mintlify.wiki/superset-sh/superset/concepts/agents), [Show HN](https://hn.svelte.dev/item/46368739)) | Electron. Licence is disputed: Elastic or Apache (unverified). Remote access is now paid. HN: "review is the real bottleneck"; people actually run 2–3 agents on real work |
| **Claude Squad** | TUI on tmux and worktrees | Lightweight. Diff tab, commit and push, pause and resume, auto-yes mode. Any CLI agent ([README](https://github.com/smtg-ai/claude-squad)) | tmux UX. Auto-yes is experimental. Small review surface. AGPL |
| **Crystal → Nimbalyst** (Stravu) | Electron. Worktrees | Rebase, squash and diff inside the app | **Crystal deprecated in Feb 2026.** Nimbalyst is a broader visual workspace ([README](https://github.com/stravu/crystal)) |
| **Vibe Kanban** (Bloop) | Kanban board → one worktree per card | Task intake from a board. Inline diff comments go to the agent. AI-written PR descriptions. Preview browser. About 10 agents ([README](https://github.com/BloopAI/vibe-kanban)) | **Sunsetting.** Hosted features were turned off; the project is now community-maintained. The board as a product didn't earn its keep |
| **Sculptor** (Imbue) | **A Docker container per agent** | Containment, so the host is safe. "Pairing Mode" syncs the agent's state into your local repo both ways. Merge UI that can send conflicts back to the agent. "Suggestions" ([product page](https://imbue.com/sculptor)) | Heavy on a laptop: high idle CPU and memory. Shared databases are hard. "Ok, and then what?" ([Show HN](https://hn.svelte.dev/item/45427697)). Mostly Claude, with Codex added |
| **Warp** | Its own terminal with vertical tabs. Agent Management Panel. Oz cloud | Third-party agents get a rich input editor (Ctrl-G), a code-review panel with comments, "Changes vs. main", attention indicators and Remote Control. `/orchestrate` hands child tasks to cloud agents. Oz runs Claude Code and Codex in the cloud, triggered by cron, Slack, Linear or webhooks ([multi-agent guide](https://docs.warp.dev/guides/agent-workflows/how-to-run-multiple-ai-coding-agents/), [3rd-party agents](https://docs.warp.dev/agent-platform/local-agents/third-party-cli-agents), [Oz](https://www.warp.dev/newsroom/2026/2/10/warp-launches-oz-the-orchestration-platform-for-cloud-coding-agents), [multi-harness](https://www.warp.dev/blog/multi-harness-cloud-agent-orchestration)) | Notifications for Claude need a Warp plugin. **Claude Code sessions don't appear in the Agent Management Panel.** Account-centric and cloud-centric |
| **Emdash** | Electron. Worktrees, local or over SSH | About 27 CLI agents. Issue intake from Linear, Jira and GitHub with the full issue as context. Diff, PR, CI and merge. Local SQLite ([search summary](https://www.mintlify.com/generalaction/emdash/features/integrations)) | Young. No mobile |
| **Codex app / Codex cloud** (OpenAI) | Mac app with worktrees and threads. Cloud containers | "Automations" put scheduled jobs into a review queue. Cloud runs best-of-N with `--attempts` 1–4 ([9to5mac](https://9to5mac.com/2026/02/02/openai-launches-codex-app-for-macos-here-are-the-details), [best-of-N blog](https://codex.danielvaughan.com/2026/04/01/codex-cloud-exec-best-of-n-attempts/), unverified) | OpenAI models only. The external `notify` hook fires only on turn complete, not on approval requests ([blog](https://codex.danielvaughan.com/2026/04/13/codex-cli-notification-pipeline-osc9-hooks-alerts/), unverified) |
| **Cursor** | IDE. Agents window. Cloud agents | Up to 8 agents on one prompt, in worktrees or remote machines (best-of-N). `/multitask` in 3.2. Moves a branch to the local foreground in one click ([3.2 changelog](https://www.cursor.com/changelog/04-24-26)) | Tied to the IDE. A user reports worktree support was narrowed ([forum](https://forum.cursor.com/t/worktrees-in-agents-window/158956)) |
| **Others** | GitHub Agent HQ / Mission Control (cloud, many vendors) ([blog](https://github.blog/news-insights/product-news/welcome-home-agents/)). Google Antigravity Agent Manager: review artifacts you can comment on ([codelab](https://codelabs.developers.google.com/getting-started-google-antigravity)). Orca, Paseo, T3 Code, Jean ([openalternative](https://openalternative.co/blog/best-ai-coding-agent-orchestrators), unverified) | | Cloud-first or IDE-first. Many are young and solo-maintained |

### 1.2 Per-tool detail on the five questions

**Isolation**
- Worktrees are the norm: Claude Code, Conductor, Superset, Claude Squad, Crystal, Vibe Kanban, Emdash, Codex app, Cursor.
- Containers: Sculptor, Codex cloud, Oz.
- None: cmux and Warp's local tabs (Warp can add a worktree per agent).
- The known worktree problems, from the [Upsun guide](https://developer.upsun.com/posts/ai/git-worktrees-for-parallel-ai-coding-agents) and the [Superset HN thread](https://hn.svelte.dev/item/46368739):
  - gitignored files such as `.env` and `node_modules` are missing;
  - **ports collide**;
  - databases and Docker are shared;
  - setup is slow.
- The answers on offer:
  - setup scripts (Conductor, Superset);
  - `.worktreeinclude` (Claude Code);
  - port blocks (Superset: 10 ports per workspace).

**Agent state**
- **Hooks or plugins (exact):** Claude agent view (native), cmux (Claude hooks), Superset (wrappers or plugins per agent), Warp (plugin for Claude and OpenCode, config for Codex).
- **OSC, BEL and screen heuristics (approximate):** cmux's base layer, and fuxx today.
- Claude Code's hooks are rich:
  - **Notification** types include `permission_prompt`, `idle_prompt`, `elicitation_dialog`, `agent_needs_input` and `agent_completed`, along with the message text;
  - **PermissionRequest** includes `tool_name` and `tool_input`;
  - **Stop**, **StopFailure**, **SessionStart** (with `source`), **SessionEnd**, **SubagentStart/Stop**, **PreCompact**, **CwdChanged** and **WorktreeCreate** are also available ([hooks](https://code.claude.com/docs/en/hooks)).
- Codex has `tui.notifications` (OSC 9 or BEL for turn-complete and approval-requested) and a `notify` program for turn-complete only (unverified, blog).
- OpenCode plugins get `session.idle`, `permission.asked` and `session.error` events ([plugins](https://opencode.ai/docs/plugins/)).

**Review and merge**
- GUI diff with line comments sent back to the agent is now standard: Conductor, Vibe Kanban, Claude Desktop, Warp and Antigravity.
- Then come PR creation, a CI and checks mirror, merge and archive.
- Claude Desktop adds **Auto-fix** (fixes failing CI and new review comments) and **Merge when ready** ([desktop](https://code.claude.com/docs/en/desktop.md)).
- Sculptor's twist is "pairing": bring the agent's state local and test it with your own tools.
- **The common complaint (HN, Substack): review is the bottleneck, not generation.** "The real bottleneck isn't human review per se, it's unstructured review" ([Superset HN](https://hn.svelte.dev/item/46368739)).

**Task intake**
- A free-text prompt everywhere.
- Issue pickers: Conductor (Linear and GitHub), Emdash (Linear, Jira and GitHub).
- A kanban board: Vibe Kanban, now sunsetting.
- Scheduled jobs: Codex Automations, Warp Oz, Claude routines.
- Same prompt to many agents: Cursor, and Codex `--attempts`.

**Complaints across the field**
1. Too many sessions to keep in your head ("the only thing holding the state of five parallel workstreams in your head", [Substack](https://engineeredintelligence.substack.com/p/orchestrate-to-survive-the-speed)).
2. Worktree setup, ports and databases.
3. Review load.
4. Electron weight and CPU (Sculptor, cmux, Superset).
5. Vendor lock-in.
6. No way to broadcast one clarification to several agents ([DEV](https://dev.to/skeptrune/llm-codegen-go-brrr-parallelization-with-git-worktrees-and-tmux-2gop)).
7. Notifications that don't say *which* agent or *what* it wants. This is cmux's founding complaint: Claude's notification always reads "Claude is waiting for your input" ([cmux blog](https://cmux.com/blog/show-hn-launch)).
8. Cost: running many agents multiplies token use.

---

## 2. What people actually value, ranked

1. **Knowing which agent needs me, and why, without looking.** Rings, a jump to the next one, and the question text. This is cmux's whole reason to exist, and it's agent view's headline.
2. **Isolation that just works.** A worktree plus env files plus ports, created for you. Conductor and Superset win here.
3. **Answering or unblocking quickly.** Peek and reply without fully switching context (agent view's Space, then reply).
4. **Reviewing what changed, with feedback going back to the agent.** Line comments → new diff.
5. **Landing it.** PR, CI status, merge, clean up.
6. **Using any agent, with no lock-in, on my subscription.** Bring-your-own-subscription and open source score highly ([openalternative](https://openalternative.co/blog/best-ai-coding-agent-orchestrators)).
7. **Native speed and low resource use.**
8. **Getting sessions back after a restart or crash.** Real resume, not a retyped command.

### Nobody does these well yet
- **Exact state across vendors in one place.** Claude's agent view is exact but Claude-only. cmux and Superset get exact state only where they've installed a hook, and fall back to heuristics. Warp doesn't list Claude sessions in its own panel.
- **A single attention queue for every vendor with inline answers**, including *what* is being asked ("Bash: `rm -rf node_modules`").
- **Comparing vendors on one task, locally.** Cursor and Codex best-of-N stay inside one vendor.
- **Review that stays in your own tools.** Every rival builds an IDE-style diff panel. People who live in the terminal use delta, lazygit, difftastic or tig, and no tool hands off to them properly.
- **Broadcasting one message to several agents.**
- **Cost and limit awareness across sessions.** Claude's status line exposes `rate_limits.five_hour/seven_day` and `context_window.used_percentage` per session ([statusline](https://code.claude.com/docs/en/statusline)), but no multi-session tool rolls them up.

---

## 3. The proposal: candidate features, ranked

Names are placeholders. Each feature builds on what fuxx already has: rings, groups, splits, restore, and the M9
socket, CLI and MCP. Effort is relative to fuxx milestones so far: S is under a third of M8, M is about one M8,
L is more than one M8.

### F1. Agent Bridge: exact status from inside the agent, and *why* it's waiting · **M**
**What you see:** Rings stop guessing. A Claude session that wants permission shows **waiting (permission)**, and its row reads `Bash · npm run migrate`. A Claude session that asked a question shows the question. A crashed turn shows **failed**, which is new: a red ✕. A session compacting its context shows a subtle "compacting". Notifications say "api-refactor wants to run `npm run migrate`", not "Claude is waiting for your input".

**How:**
- When fuxx starts `claude` from a preset or task, it adds `--plugin-dir <fuxx.app>/Contents/Resources/claude-plugin`. That flag loads a plugin for that one session only ([CLI ref](https://code.claude.com/docs/en/cli-reference)).
- The plugin's hooks call `fuxx hook <event>`, which reads the hook's stdin JSON and sends it over the M9 socket using `FUXX_SESSION`. The events are Notification, PermissionRequest, Stop, StopFailure, SessionStart, SessionEnd and Pre/PostCompact.
- **fuxx never edits `~/.claude/settings.json`.**
- Codex: pass `-c tui.notifications=[...]` (and `notify`) on its command line (flag syntax unverified, check before building).
- OpenCode: a small plugin, but the install location needs checking (unverified).
- Every other program keeps the M2 heuristics.
- The socket gains a `session.report` method. `fuxx status` and `fuxx question` return the exact source when there is one.

**Why it beats the others:**
- Agent view is exact but Claude-only and in a TUI.
- cmux and Superset wire this up per agent, but cmux only for Claude.
- fuxx would be the only terminal with exact states for Claude, Codex and OpenCode, plus the *reason*, all in the M9 API and events.

**Uses:** rings, M9 `status`, `question`, `wait` and `events` (they become exact), notifications.

**Risks:**
- Claude's richness is Claude-only. Codex's hooks are thinner: turn-complete and approval only (unverified).
- Hook schemas change often (dozens of versioned notes in the docs).
- Check that plugin hooks merge with, rather than replace, the user's own hooks (plugins are documented as a hook source, but merging wasn't verified).
- It only covers agents *fuxx* started. One typed by hand gets heuristics unless you set "bridge typed `claude` too" through a shell function (opt-in).

### F2. New Agent Task sheet (⌃⌘N): replaces the "useless" presets · **M**
**What you see:** Press ⌃⌘N and a sheet asks for:
- **Agent:** claude, codex, opencode or your own;
- **Project:** defaults to the current group;
- **Task:** a multi-line prompt, where ⌘↩ starts;
- **Where:** *This folder* or *New worktree*. With a worktree, the branch name is suggested from the prompt, e.g. `fuxx/fix-flaky-settings-test`;
- **Mode:** Claude permission modes when the agent is Claude, e.g. plan or acceptEdits.

fuxx then:
1. creates the worktree (F3);
2. opens a session in the project's group, named after the prompt (short title);
3. starts the agent **with the prompt as its argument**: `claude "<prompt>"`, `codex "<prompt>"` and so on, plus the F1 bridge flags;
4. focuses it, or keeps focus where it is if you held ⌥.

`fuxx task new --agent codex --worktree "<prompt>"` and an MCP `start_task` tool do the same.

**Why it beats the others:**
- Conductor and Superset have this, but they're IDE-shaped apps. Agent view's dispatch box is Claude-only.
- Here it's one keyboard sheet for every vendor, and the result is a real terminal you can type into straight away.

**Uses:** groups, presets (they become the agent list), M9 `new`, MCP.

**Risks:**
- How each agent takes a prompt and its flags differs, so each needs a small adapter.
- The prompt text has to be passed safely: argv, not typed into the shell.

### F3. Worktree lifecycle: isolation without an IDE · **M**
**What you see:**
- A worktree session shows a branch chip (`⎇ fix-flaky…`) in its sidebar row.
- On creation, fuxx copies files matching `.worktreeinclude`, the same file Claude Code reads, so one file serves both.
- It runs `.fuxx/setup` if present, visibly, as the first thing in the pane.
- It exports `FUXX_PORT_BASE`, a free block of 10 ports, as Superset does, and `FUXX_ROOT` for the main checkout.
- Closing the session asks: "**fix-flaky** has 3 changed files and 2 unpushed commits: Keep worktree / Remove / Cancel". A clean one is removed silently.
- Restore reopens it in the worktree.

**How:**
- `git worktree add <repo>/.fuxx/worktrees/<slug> -b fuxx/<slug>` from the default branch.
- For Claude you can choose "let Claude own it" (`claude -w <slug>`) so its own cleanup and resume rules apply.
- Every git command fuxx runs is shown in the pane.

**Why it beats the others:**
- cmux has no worktrees at all.
- fuxx gets the parts people praise in Conductor and Superset (setup, ports, safe cleanup) without becoming an app around git.

**Uses:** groups (a worktree session stays in its repo's group), restore, M9 `new --worktree`.

**Risks:**
- Needs git.
- Submodules, LFS (Claude Code skips the repo's own filter drivers for safety ([worktrees](https://code.claude.com/docs/en/worktrees.md)), and fuxx should too) and disk use.
- Port blocks only help if the project's dev server reads `FUXX_PORT_BASE`. Document that, don't promise more.

### F4. Attention Inbox: one queue, answer in place · **M–L**
**What you see:**
- ⌘J opens a small overlay listing every **waiting** session, oldest wait first, across all groups. Each card shows the agent, the session name, how long it's been waiting ("4m"), the question or permission request (from F1, or the screen tail as a fallback), and, for permissions, the exact command.
- Keys:
  - ↩ to jump;
  - ⌃1/⌃2/⌃3 to pick a menu option;
  - type a reply and ⌘↩ to send it;
  - ⌘⌫ to dismiss.
- ⌘⇧J jumps straight to the next waiting session, without the overlay (cmux has ⌘⇧U for this).
- The Dock badge and the window title show the waiting count.

**How:** Answers are **keystrokes typed into that session's PTY**, the same path as M9 `answer`. That's what you'd do by hand, so it works for every vendor. A later spike could try answering through Claude's `PermissionRequest` hook instead. That's more exact, but the hook blocks while it waits, so it's riskier.

**Why it beats the others:**
- Agent view does this for Claude only, inside a TUI.
- cmux only jumps to the latest unread.
- Warp's panel doesn't include Claude sessions.
- This is the feature most likely to make people say "I can't go back".

**Uses:** rings (the queue is the waiting rings, made actionable), M9 `question`, `answer` and `wait`, focus.

**Risks:**
- Keystroke mapping for each agent's menus. Claude's numbered choices are unverified, and menus change between versions.
- It's an overlay UI, so it overlaps M11's visual work. Build it plain and let M11 style it.
- Answering a permission you haven't fully read is a safety risk. Always show the full command, and never allow-all by default.

### F5. Rich session rows: branch, diff size, PR/CI, time waiting, context · **M**
**What you see:** Each sidebar row gets a second line: `⎇ fix-flaky · +120 −14 · PR #482 ✓ · ctx 63%`, plus "waiting 6m" in the waiting colour. The group header rolls these up, e.g. "3 PRs, 1 failing".

**How:**
- `git` polling in the session's cwd, slow and only for visible rows.
- `gh pr view --json` if `gh` is installed.
- Context %, and later cost, from the transcript path the F1 hooks report. Or from Claude's status-line JSON if you opt in to a `fuxx statusline` wrapper that passes your own status line through: `context_window.used_percentage`, `cost.total_cost_usd`, `rate_limits.*` ([statusline](https://code.claude.com/docs/en/statusline)).

**Why it beats the others:**
- cmux shows branch, PR and ports, but no diff size, no time waiting and no context.
- Agent view shows PR state for Claude only.

**Uses:** sidebar, groups, M9 `note` and `progress`. These become automatic, but scripts can still override them.

**Risks:**
- Polling cost with 20 sessions.
- `gh` is optional.
- The status-line wrapper touches the user's Claude config, so it must be opt-in.
- Strong overlap with M11's sidebar redesign. Agree the row layout once.

### F6. Review & Land, done the terminal way · **M**
**What you see:** When a worktree session turns **done**, its row offers **Review**, also on ⌘⇧R. Review opens a split next to the agent running *your* diff tool against the branch's base:
- set by `review-command = lazygit`, `git difftool -d`, `delta` or `difft`;
- default: `git -c core.pager=delta diff <base>...HEAD`, falling back to plain `git diff`.

Three actions follow:
- **Send feedback** (⌘⇧F): select lines in the review pane, type a comment, and fuxx sends `About <file>:<line>: <comment>` to the agent's session. Feedback goes back to the agent, as in Conductor, but from any terminal tool.
- **Open PR**: runs `gh pr create --fill --head <branch>` visibly in the review pane.
- **Land**: menu options *Squash into <base>*, *Rebase onto <base>* or *Discard*. Each runs as a visible git command, and conflicts are handed to the agent: "Rebase stopped on conflicts in X; please resolve".

After landing, fuxx offers to remove the worktree and close the session.

**Why it beats the others:**
- Every rival builds its own diff viewer, which is IDE territory. People who live in the terminal prefer their own tools, and fuxx is the only one that orchestrates them.
- It keeps Sculptor's good idea ("review with your own tools") without containers.

**Uses:** splits (the review pane sits next to the agent), worktrees (F3), M9 `send` and `paste`, rings.

**Risks:**
- Line selection in a pager is fuxx's own selection (M5). It needs a reliable "which file and line is this" heuristic for delta and git-diff output. Fall back to sending the selected text as a quote.
- The `gh` dependency.
- The squash and rebase flows must never force anything, and must always show the command.

### F7. Resume, don't retype: real session restore · **S–M**
**What you see:** After you quit and relaunch, each Claude session reopens **in its conversation** (`claude --resume <id>`), not as a fresh `claude`. Codex sessions do the same where Codex supports resume (CLI syntax unverified). Worktree sessions come back in their worktree.

**How:**
- The F1 SessionStart hook reports the `session_id`, and fuxx saves it in `sessions.json`.
- Only if the transcript still exists. Otherwise fuxx falls back to today's behaviour.
- Claude's `--resume` already returns a session to its worktree ([worktrees](https://code.claude.com/docs/en/worktrees.md)).

**Why it beats the others:** cmux added resume through hooks. This is parity, but cross-vendor and tied to fuxx's restore setting. Without it, F1–F6 lose work at every restart.

**Uses:** M7 restore (`session-restore = agents`).

**Risks:** Codex and OpenCode resume semantics need checking. Deleted transcripts.

### F8. Race: one task, several agents, pick the winner · **M** (needs F2, F3, F6)
**What you see:** In the F2 sheet, tick **Race** and choose agents, e.g. claude, codex and opencode, or claude ×2. fuxx:
- creates one worktree per entrant;
- opens them as an equal grid of splits in one layout;
- starts each with the same prompt.

When all are done, a summary row shows each entrant's diff size, time taken, and whether `.fuxx/check` (your test command) passed. **Pick** keeps one branch and offers to discard the others.

**Why it beats the others:** Cursor and Codex best-of-N are single-vendor and run in the cloud or the IDE. This is local, **cross-vendor**, and visible live in terminals. It's a demo-able "wow" that no terminal has.

**Uses:** splits and equalize, worktrees, the F1 done states, the M9 `wait --any`.

**Risks:**
- Cost: N times the tokens. Show the count before starting.
- Machine load.
- Only worth it for small, well-specified tasks, which HN also notes.

### F9. Broadcast: say it once to many agents · **S**
**What you see:** Select several sessions in the sidebar (⌘-click), press ⌘⇧B, type the message, and it's sent to each. Only to the waiting or idle ones, unless you tick "also interrupt working ones".

**Why it beats the others:** Asked for in the field ([DEV](https://dev.to/skeptrune/llm-codegen-go-brrr-parallelization-with-git-worktrees-and-tmux-2gop)). Nobody ships it outside Claude's own messaging.

**Uses:** sidebar multi-select (new), M9 `send` / `answer`.

**Risks:** Sending text to a session in the wrong state. Mitigate by targeting waiting and idle sessions only.

### F10. Cross-vendor teams over MCP · **S** (on M9)
**What you see:** A Claude "lead" in one pane can use the new MCP tools `start_task` (F2), `wait_any`, `review_summary` (diff stat and check result) and `send_feedback` to run Codex and OpenCode workers. Each worker is a visible pane in the same group.

**Why it beats the others:** Claude agent teams are Claude-only and experimental ([agents](https://code.claude.com/docs/en/agents)). `cmux claude-teams` is Claude-only. This one is cross-vendor and visible.

**Uses:** M9 MCP, splits, groups.

**Risks:** Runaway spawning. Cap concurrent tasks (`max-agent-tasks = 6`) and show "spawned by s3" on the row.

### F11. Claude background sessions in the sidebar · **S**
**What you see:** A "Claude background" section lists `claude --bg` sessions read from `claude agents --json`, with their state, which maps to fuxx's rings. Clicking one opens a pane running `claude attach <id>`.

**Why it beats the others:** It embraces agent view instead of fighting it. No terminal does this.

**Risks:**
- Research-preview interface, though `--json` is the documented, supported way for scripts to read state.
- Polling cost.

### F12. Usage and limits meter · **S–M** (needs the F5 status-line opt-in)
**What you see:** A small footer: `5h 72% · resets 14:30 · 7d 31%`, plus total estimated cost today across sessions. When the 5-hour window passes 90%, fuxx warns before you start a Race.

**Why it beats the others:** Running many agents spends one shared limit. Nobody rolls that up.

**Risks:**
- Claude-only data, and only for Pro and Max subscribers or behind a gateway.
- Cost is an estimate ([statusline](https://code.claude.com/docs/en/statusline)).

### F13. Start a task from a GitHub issue · **S** (on F2)
**What you see:** The F2 sheet's **From issue…** button lists `gh issue list` for the repo. Picking one fills the prompt with the issue's title and body, and the branch name with `fuxx/123-…`.

**Why:** Conductor and Emdash do this. Keeping to `gh` avoids becoming a Linear or Jira client.

**Risks:** Needs `gh`. Scope creep towards trackers (see §5).

### F14. Smarter notifications · **S**
**What you see:**
- Per-kind settings: notify on *permission* and *question* always, on *done* only if the agent worked over 2 minutes, never on *idle*.
- Bursts are coalesced: "3 agents need you".
- A per-group mute.
- "Don't notify while I'm typing in fuxx."

**Why:** Agent counts go up, notification fatigue sets in, and nobody's controls are this specific.

**Uses:** F1 kinds, M4 notifications.

**Risks:** Very little.

### F15. Timeline strip · **S–M**
**What you see:** Under a session's row, or on hover, a thin strip of its last hour: working, waiting and idle spans. A daily "agents waited on you 37 min" line.

**Why:** It makes the cost of waiting on *you* visible, which is the real bottleneck.

**Risks:** Low value next to F1–F7. Overlaps notification history (P2 in [[11 - Features]]).

---

## 4. Recommended cut

**"Agent Sessions" as one milestone of 7 features.** Together they make fuxx the best place to *run and watch* agents, and they stay on the terminal side of the line:

1. **F1 Agent Bridge.** The foundation. Everything below gets better with exact states and the reason for waiting.
2. **F2 New Agent Task sheet (⌃⌘N).** Fixes presets.
3. **F3 Worktree lifecycle.** Isolation, setup, ports and safe cleanup.
4. **F4 Attention Inbox (⌘J / ⌘⇧J).** The headline differentiator.
5. **F5 Rich rows.** Branch, diff size, PR/CI, waiting time, context.
6. **F6 Review & Land.** Review in your own tools, feedback to the agent, PR, land, clean up.
7. **F7 Resume on restore.** So none of it is lost on quit.

If that's too big for one milestone, split it:
- **"Agent Sessions I: know and start"**: F1, F2, F3, F7.
- **"Agent Sessions II: answer and land"**: F4, F5, F6.

The flagship follow-up is **F8 Race**, together with F9 Broadcast and F10 cross-vendor MCP teams. It's the most demo-able feature, but it depends on F2, F3 and F6 being solid.

**Leave for later:** F11 (Claude background import), F12 (limits meter), F13 (issue intake), F14 (fine notification rules; fold the cheap parts into F1), F15 (timeline).

**Roadmap implications (your call, not assumed):**
- This sits naturally right after M9, before M10's cmux extras (browser, SSH, tmux daemon). Those extras do less for "run and watch AI agents" than this does.
- M11 (UI/UX) should come *after* it, because F4 and F5 change the sidebar and add an overlay. That's the same reasoning you gave for keeping M11 after M9 and M10.
- Adopting it means editing [[12 - Scope]]. Worktree management and a review handoff aren't in scope today, and "agent presets" would change meaning.

---

## 5. Scope warnings: staying a terminal, not an IDE

[[12 - Scope]] says fuxx is "run and watch AI agents in one window" and "not a general IDE". The rivals show where
the slope is: Conductor, Superset, Emdash and Vibe Kanban all grew diff viewers, file trees, browsers and boards. Vibe
Kanban, the most ambitious board, is the one that died. The traps:

| Temptation | Why it's IDE (or worse) | Stay on the right side by |
|---|---|---|
| A built-in GUI diff viewer with inline comment threads | It's a code-review app | F6: launch *your* diff tool in a split. fuxx only routes selection → feedback |
| File tree or code editor for agent output | It's an editor | "Open in $EDITOR" on a path (M5's clickable paths already cover this) |
| A merge-conflict resolution UI | It's an IDE feature | Hand the conflict back to the agent, or leave it to your git tool, in a pane |
| A kanban board or Linear/Jira client | It's a project manager, and Vibe Kanban showed it doesn't pay | Only F13's `gh issue` picker, read-only. Never write back to trackers |
| Containers or cloud execution (Sculptor, Oz, Codex cloud) | Infrastructure, not a terminal | Worktrees only. A user can still run a container in a pane |
| An auto-approve policy engine ("always allow tests in this session") | A security product. Claude Code already has permission modes and auto mode | Never answer for the user automatically. F4 makes answering fast instead |
| Its own agent or LLM calls (summaries, AI PR descriptions, "suggestions") | fuxx becomes an agent vendor, with a cost and keys | Ask the agent in the pane to do it (`gh pr create --fill`, or prompt the agent) |
| Embedded dev-server preview browser | Already deferred to M10 | Keep it deferred. Clickable `localhost` links already open your browser |
| Mobile or remote control | A server product | Out. Claude Remote Control and Warp exist |

**The rule worth writing into the scope note:** *fuxx shows state, routes your attention and input, and runs your
own tools. It never edits code itself. Every git or gh action it takes runs as a visible command in a pane.* That
matches M9's existing principle that scripting "only does what you could do by hand".

---

## 6. Open questions and things to verify before planning

- Do hooks from a `--plugin-dir` plugin **merge** with the user's own hooks? Is `--plugin-dir` available on the user's Claude version? Check the [plugins docs](https://code.claude.com/docs/en/plugins/create.md).
- Codex: the exact `-c` override syntax for `tui.notifications` and `notify`, `codex resume`, and how it takes a prompt.
- OpenCode: can a plugin be loaded per session without writing to `~/.config/opencode`?
- Claude's permission menu keystrokes (F4) in the current version.
- Whether `claude -w` or fuxx-made worktrees should be the default for Claude. Claude's isolation checks, cleanup and resume only cover worktrees it creates.
- Conductor's "checkpoints" and "multi-model compare" are claimed only by third parties.
