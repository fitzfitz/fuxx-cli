---
tags: [development, milestone, design]
status: built
---

# 63 - Milestone 11l - Command Blocks (design)

**Status:: 🟨 Built, reviewed, merged and pushed 2026-11-09 (1251 tests; every detail AI's call; you skipped the design questions, except the chip's shape, which you chose from mockups). On-screen check pending (22 items).
Not released on its own: it ships in the one combined release (M11i–M11n) after M11n (your rule, 2026-11-09).**  ·  **Phase 4: an M11 extension.**  ·
**Plan:** [[M11l Plan - Command Blocks]]
**Added to the plan 2026-11-08 by you** (see [[12 - Scope]], "A modern layout, a command palette, a first-run screen
and command blocks"). The last of M11i–M11l; after it come M11m (title bar) and M11n (performance), both listed 2026-11-09 and not designed here.

## Goal

In a zsh terminal, fuxx knows where each command starts, where its output starts and how it ended. It draws a
**quiet mark in the left padding** next to each command (red when it failed), a **hairline between commands**, and
on hover a small **chip** with the command, its exit code and three actions: **Copy command**, **Copy output**,
**Rerun**. **⇧⌘↑ / ⇧⌘↓** jump from command to command, **⇧⌘C** copies the chosen command's output. Without shell
marks (another shell, SSH, a TUI) the terminal is exactly what it is today.

## Why (your words, 2026-11-08)

- The roadmap: "M11l — Command blocks — requested 2026-11-08: Warp-style blocks per command and its output
  (collapse, copy, rerun). The riskiest; last, with its own careful design." ([[40 - Roadmap]], [[12 - Scope]],
  Progress Log 2026-11-08 "UI uplift requested: M11i–M11l": "I love all 1-11").
- You skipped the design questions ("lewati pertanyaan", "pakai cara mu", "go ahead do them all autonomously"), so
  **every rule, value and layout below is AI's call**. The ones you may most want to overrule are marked **(AI's
  call)** where they're made and listed again in "Settled before planning".
- **One change to what you asked for (AI's call): collapse is not in M11l.** Copy and rerun are; collapse is
  explained under "Not in M11l". Everything else Warp-like that M11l does is an overlay on the terminal you have,
  not a new terminal.

## What exists today

- **The terminal** (`terminal.rs`, `terminal_view.rs`): an `alacritty_terminal` 0.26 `Term` (scrollback 10,000
  lines, `term_config()`), fed by the PTY reader thread (`pty::spawn_reader`, 8 KiB reads) under one `FairMutex`
  with its `vte` 0.15 `Processor`. The view paints a `GridSnapshot` (`grid::snapshot`) of the viewport each frame
  inside 16 pt side / 12 pt top-bottom padding (12 / 10 in a split; `ui_size::TERMINAL_PADDING`). Selection, links
  (⌘-hover, `links.rs`: OSC 8 first, then plain URLs, only `http`, `https`, `mailto`), mouse reporting and the wheel
  all map window points to grid points 1:1 (`GridLayout::grid_point`). **There is no terminal find** (⌘F is the
  file tab's).
- **OSC handling that alacritty skips:** `detector.rs` scans the raw bytes for OSC 9 / 99 / 777 (notifications) and
  OSC 7 (`CwdReports` → `reported_cwd`, the sidebar folder), with a per-byte `Intro` matcher that survives a split
  read. **Checked in the crate sources:** `vte` 0.15's `osc_dispatch` handles 0, 2, 4, 8, 10–12, 22, 50, 52, 104,
  110–112 and logs everything else as unhandled; **OSC 133 is not handled**, and `alacritty_terminal` 0.26 has no
  prompt or semantic-zone concept. Its `Cell` has `set_hyperlink` / `hyperlink()` (an `Arc`'d `id` + `uri`); 15 of
  the 16 `Flags` bits are taken and any write resets a cell's flags; rows carry no flags. `Processor::sync_bytes_count()` tells whether a DEC
  2026 synchronized update is being buffered.
- **No shell integration** anywhere: nothing sets `ZDOTDIR`, `XDG_DATA_DIRS`, `--rcfile` or `ENV`. A local terminal
  runs your login shell (`CommandBuilder::new_default_prog()`, `$SHELL` as `-zsh`) with `pty::ShellEnv`
  (`session_env`: `FUXX_SOCKET`, `FUXX_SESSION`, `PATH`); a preset's command is **typed into** that shell
  (`start_in` → `startup`). An SSH session runs `ssh` itself as the program (`workspace_ssh::ssh_program`).
- **Persistent sessions** (M10a, `daemon_session.rs`): the background service owns the authoritative `Term` and
  spawns the shell with the GUI's whole environment (`create_env`); the window's replica (`terminal.rs` `Replica`)
  starts from `vt_snapshot::encode` and is fed the same raw bytes as numbered `Output` frames. **`vt_snapshot`
  round-trips OSC 8 hyperlinks** (`ESC ]8;id=…;uri ESC \`) but not SGR-less flag bits.
- **The agent-state detector** (`status.rs` `StatusTracker`) works from output timing, bells, notifications and
  the screen's tail text; it knows nothing of commands. **Activity** (`activity.rs`, M11h) keeps status changes,
  notes and progress per terminal (last 200).
- **Keys** (`main.rs`, context `Terminal`): ⌘C / ⌘V, **⌘↑ / ⌘↓ = scroll a page** (and ⇧PageUp / ⇧PageDown), ⌃Tab.
  ⇧⌘↑, ⇧⌘↓ and ⇧⌘C are free in every context. `keys::mac_shortcut` gives ⌘← ⌘→ ⌘⌫ ⌥← ⌥→ to the shell; any other
  ⌘ key goes to the app. Help's list (`help_shortcuts::describe`) and the palette (`palette_items`) are built from
  the binding table and tested for parity.
- **Paste** (`TerminalView::paste`): bracketed (`ESC [200~ … ESC [201~`, ESC stripped) when the program enabled
  mode 2004; zsh's line editor does.

## 1. How commands are found (AI's call: OSC 133 marks from fuxx's own zsh integration)

### The marks

The FinalTerm / OSC 133 convention Ghostty, Kitty, WezTerm, iTerm2 and VS Code share, read from the raw bytes by a
new scanner (`shell_marks.rs`, the same per-byte style as `detector.rs`), terminated by BEL or ST:

| Mark | Sent | fuxx reads |
|---|---|---|
| `ESC ]133;A` | at the start of the prompt (embedded at the start of `PS1`) | prompt starts here |
| `ESC ]133;B` | at the end of the prompt (end of `PS1`) | your typing starts here |
| `ESC ]133;C;cmdline_url=<%-encoded>` | in `preexec`, after Return | output starts; the command line as the shell had it (Kitty's key) |
| `ESC ]133;D;<exit>` | first thing in `precmd`, only after a command ran | the command finished with `<exit>` |

Other `133` letters and keys are ignored. A mark longer than 8 KiB is dropped. `D` with no number = exit unknown.

### Where a mark is kept (AI's call: in the grid itself, on the command row's first cell)

Alacritty numbers lines relative to the screen and drops the oldest at the 10,000-line cap; a resize **reflows**
every wrapped line. A side table of line numbers would drift through all three. So fuxx keeps each command's
record **inside the grid, on the cell it describes**, where alacritty already carries per-cell data through
scrolling, the cap, reflow, clears and the service's snapshot: a **hyperlink on column 0 of the command row**.

- **The tag:** `Hyperlink { id: "fuxx-block-<n>", uri: "fuxx-block:?v=1&s=<state>&p=<n>&c=<n>&o=<n>&x=<exit>&cmd=<%>" }`
  - `s` = `prompt` (shown, nothing run yet), `running`, `done`;
  - `p` = how many logical lines the prompt has above the command row (the LFs between A and B; capped at 8);
  - `c` = logical lines from the command row to the first output row (set at C);
  - `o` = logical lines of output (set at D);
  - `x` = exit code (set at D; absent = unknown); `cmd` = the command, percent-encoded, at most **2,048 bytes** of
    UTF-8 before encoding (longer: no `cmd`, and `t=1`).
  - Only ASCII 0x21–0x7E in the URI, no `;`, so `vt_snapshot` writes it and `vte` reads it back unchanged.
- **"Logical lines"**, not rows: a line and its soft-wrapped continuations (`WRAPLINE` on the row above's last cell)
  count as one. Reflow keeps logical lines and keeps every logical line's first cell in column 0, so the tag and its
  counts stay right at any width.
- **When it's written**, with the processor advanced exactly up to each mark (the bytes are split at the end of each
  mark; `vte` carries its state across `advance` calls, so the screen is the same as unsplit):
  - **A**: remember the cursor; count LFs from here to B.
  - **B**: tag column 0 of the cursor row's logical-line start with `s=prompt&p=<LFs>`. Remember the cursor as the
    **input start** (for rerun). A prompt redrawn (resize, `zle reset-prompt`, a theme's async refresh) re-sends A…B
    (they live in `PS1`) and re-tags; the overwritten cells lost the old tag on their own.
  - **C**: find the nearest tag at or above the cursor row (column 0 only, at most 500 rows up). If it is
    `s=prompt`, rewrite it `s=running&c=…&cmd=…`. If it is already `running` (a second C, e.g. another integration
    also sending marks), only fill a missing `cmd`.
  - **D**: find the nearest tag at or above the cursor (up to the top of scrollback); if `running`, rewrite it
    `s=done&o=…&x=…`. Anything else: ignore.
- **Never written:** on the alternate screen (vim, htop, tmux, an agent TUI that uses it); while a synchronized
  update is buffering (`sync_bytes_count() > 0`; the cursor isn't where the bytes say); past the scan limits. In all
  of these the mark is simply dropped.
- **This is a hidden link.** `links.rs` ignores hyperlinks whose URI starts with `fuxx-block:` (the plain-URL search
  on that cell still runs); nothing else in fuxx reads hyperlinks. Copy, selection and the status detector see the
  same text as before. If your prompt's first character carried its own OSC 8 link, that one cell loses it (rare).
- **One code path for every Term:** a `blocks::advance(processor, term, tagger, scanner, bytes)` replaces the bare
  `processor.advance` in the local reader, `show_local` (so screenshots can draw blocks), the window's replica, and
  the background service's session. Service and replica tag the same bytes the same way, and the service's snapshot
  carries the tags to a window that reattaches.

### Shell integration (AI's call: zsh, injected without touching your dotfiles)

- **When:** only for a **login shell** fuxx starts (no `program`: not SSH's `ssh`, not the screenshots' `cat`), only
  when `$SHELL`'s file name is `zsh`, and only with `command-blocks = on` (§6). Local and persistent sessions alike
  (the env goes into `ShellEnv` either way).
- **How (Ghostty's and Kitty's method):** fuxx sets `ZDOTDIR=<integration>/zsh` and, if you had a `ZDOTDIR`, keeps
  it in `FUXX_ZSH_ZDOTDIR`. zsh reads fuxx's `.zshenv` first, which:
  1. puts `ZDOTDIR` back exactly (yours, or unset) and drops `FUXX_ZSH_ZDOTDIR`;
  2. sources **your** `${ZDOTDIR:-$HOME}/.zshenv` if it exists;
  3. for an interactive shell, sources `fuxx-integration.zsh`.
  zsh then reads your `.zprofile`, `.zshrc` and `.zlogin` from your own `ZDOTDIR` as always. Child processes never
  see fuxx's `ZDOTDIR`.
- **`fuxx-integration.zsh`** (zsh ≥ 5.1; macOS ships 5.9), as built:
  - **`_fuxx_preexec`** sends C with the command, percent-encoded in zsh, skipped above 2,048 bytes. It sends **no C
    for an `exec` that replaces the shell**: the first word is `exec`, also after `builtin` / `command` and after
    `NAME=value` words (`FOO=1 exec zsh`), unless the next word is a redirection (`exec >file` stays a block).
  - **`_fuxx_done`** sends D with `$?` when a command ran. It is **first** in `precmd_functions` (put back first if
    something moved it), so another hook's prints (a timing plugin, pure's spacing) land after D and not in the
    finished command's output.
  - **`_fuxx_precmd`** moves itself **last** in `precmd_functions` at **every** prompt, and the line-init hook runs
    at **every** prompt (not once), marking the `PS1` the theme just built: `%{ESC]133;A BEL%}` at its start and
    `%{ESC]133;B BEL%}` at its end, with any stray inner A stripped first (pure's split prompt). Powerlevel10k
    moves its own hook last and rebuilds `PROMPT` from scratch at every prompt, so marking only once lost every
    block after the first.
  - **powerlevel10k:** when p10k is loaded and you haven't set it, fuxx turns on `POWERLEVEL9K_TERM_SHELL_INTEGRATION`
    at the first prompt (not in `.zshenv`: `.p10k.zsh` unsets `POWERLEVEL9K_*`). Once p10k reports it marks, fuxx
    stops touching `PS1` and turns off p10k's own C and D (they fire on an empty Return and on ⌃C and made bogus
    blocks). A missing p10k internal falls back to the hook-order path.
  - **`add-zle-hook-widget`** is autoloaded behind a guard (`autoload -Uz +X … 2>/dev/null`); without it the
    fallback `zle-line-init` path is used.
  - No marks in `PS2` or `RPROMPT`. Guarded so sourcing it twice does nothing.
- **Where the files live:** compiled into fuxx (`include_str!`), written at startup to
  `~/Library/Application Support/fuxx/shell-integration/zsh/` (`.zshenv`, `fuxx-integration.zsh`) only when missing
  or different, before the first terminal starts. A failed write = no injection (logged), never a broken shell.
  The scripts are checked again at **each spawn** (`.zshenv` must exist, or fuxx re-installs it and doesn't inject
  when that fails).
- **What can stop it, and what then:** `ZDOTDIR` set in `/etc/zshenv`, `zsh -f`, a nested `zsh` (it doesn't inherit
  the injection), `sudo -s`, `su`: no marks, so no blocks, a plain terminal.
- **bash: not injected (AI's call).** macOS's `/bin/bash` is 3.2 (no `PS0`); injecting means rewriting the argv to
  `bash --posix` with `ENV` and re-creating bash's login startup by hand, which Ghostty itself doesn't do for
  macOS's bash. **fish: not injected (AI's call):** it would be cheap (`XDG_DATA_DIRS` + `vendor_conf.d`), but there
  is no fish on this Mac or the CI runner to verify it, and an untested script runs at every fish start. **Any shell
  that sends OSC 133 itself** (your own setup, another tool's integration) gets blocks anyway: marks, not shells,
  drive this. Without `cmdline_url` such blocks have no Copy command or Rerun.

## 2. What a block looks like (AI's call: a light overlay, not a Warp-style block UI)

The grid stays alacritty's grid, cell for cell: nothing is inserted between rows, so selection, links, mouse
reports, the wheel, resize and the cursor work exactly as today. Everything below is painted **over** the grid or
**in its left padding**.

```
 padding│ grid
        │
   ▌    │~/code/app main                ← prompt line 1 (p = 1)
   ▌    │❯ npm test                     ← command row (the tag lives on its first cell)
        │> app@1.0 test …               ← output
        │✗ 2 failing
 ───────┼───────────────────────────────  ← separator: 1 pt hairline above the next prompt
   ▌    │~/code/app main                   (the bar is red: exit 1)
   ▌    │❯ ▌                            ← the live prompt
```

- **The gutter bar:** 2 pt wide, radius 1, centred in the left padding (8 pt left of the grid's origin; 6 pt in a
  split), from the prompt's first row to the command's last row. Colour by outcome:
  - done, exit 0 or unknown: `block_ok` = the theme's bright black (`palette[8]`) held at 3:1 on the background;
  - done, exit ≠ 0: `block_failed` = the theme's red (`palette[1]`) held at 3:1;
  - running: `block_running` = `ring_working` held at 3:1.
- **The separator:** a 1 pt line, `card_border` (quiet, ≥ 1.4:1, the hairline rule), across the grid's width at
  the top edge of each block's first prompt row and of the live prompt's, except when that row is the very first
  line of scrollback.
- **Empty prompts** (Return on an empty line, ⌃C at the prompt) leave `s=prompt` tags behind; only the newest one
  (the live prompt) gets a separator. Stacked empty prompts get no lines between them.
- **The chosen block** (§4): its bar becomes 3 pt `accent` and runs the block's whole height, prompt to last output
  row (to the cursor row while running).
- **No tint on failed output (AI's call):** a background wash fights programs' own colours and the selection; the
  red bar and the chip's `exit 1` carry failure.
- **Hover:** the pointer over the gutter strip (the left padding) at a block's rows widens its bar to 3 pt and shows
  the chip; no animation.

### The chip

```
┌──────────────────────────────────────────────────────────┐
│ exit 1 · npm test           Copy command  Copy output  Rerun │
└──────────────────────────────────────────────────────────┘
```

- `overlay_panel` look: `panel` fill, 1 pt `panel_border`, radius 6, 8 pt side padding. **Built as one terminal row
  tall with no shadow** (you chose this from mockups on 2026-11-09, "oke begitu saja"; the first design was 24 pt
  with a soft shadow and covered the rows around it). Anchored 4 pt right of the gutter on the command row, so it
  covers no row above or below; kept inside the grid's bounds. Its text is 11 pt and shrinks to fit the row at small
  terminal fonts.
- **A right prompt stays visible:** on the command row, if a text run at the right follows at least 2 blank cells
  (zsh's RPROMPT), the chip stops 1 cell before that run; otherwise it may reach the grid's right edge. It is never
  narrower than `CHIP_MIN_WIDTH` (197 pt).
- **Left:** the outcome — `exit 0` (`text_dim`), `exit 1` (`block_failed` held at 4.5:1 on `panel`), `running`,
  or nothing when unknown; then ` · ` and the command's first line, at most 40 characters with `…` (`text`). It
  shows exactly what Copy command and Rerun will use, so a faked mark shows its fake command.
- **Right:** three text buttons, `text::META` size, `text_dim`, `panel_hover` on hover; a disabled one at
  50 % opacity of `text_dim` (Copy command / Rerun with no command; Rerun when not at an empty prompt, §4).
- **Narrow panes:** the command text is cut first, then the outcome. The buttons read `Cmd`, `Output`, `Rerun` below
  220 pt of grid (`CHIP_NARROW`), and also when the room to the right prompt is under 282 pt (`CHIP_LONG_MIN`).
- Shown while the pointer is over the block's gutter or the chip; hidden 150 ms after it leaves both (no fade);
  hidden on any output that moves the block, on typing, and while a mouse button is down in the grid.

## 3. Hard cases

| Case | What happens |
|---|---|
| Alternate screen (vim, htop, tmux, a TUI) | No marks are written there; nothing is drawn while it's up. Back on the main screen, the blocks are as they were. |
| An agent TUI on the main screen (Claude Code draws inline) | Typed into zsh, `claude` is one **running** block for as long as it runs; nothing inside it. If it clears scrollback (`ESC [3J`) the tags above go with it. |
| A command that never finishes | Stays `running`: running bar; Copy output copies output so far; no Rerun until a prompt. |
| Huge output | Nothing is stored outside the grid. If its tag scrolls out past the 10,000-line cap, the block is gone; its leftover output reads as plain lines. |
| No marks (bash, fish, `zsh -f`, `command-blocks = off`) | No tags, nothing drawn, keys do nothing: today's terminal. |
| SSH | `ssh` is a program, not a login shell: no injection. Typed into a local zsh, `ssh host` is one running block; the remote shell has no marks. |
| Persistent sessions | Service and window tag alike; the snapshot carries the tags. A service started by a fuxx without M11l doesn't tag, so after reattaching to it only new commands get blocks. Rerun waits for the next prompt after a reattach. |
| `clear`, `reset`, ⌘K-style clears | `ESC [2J` pushes the screen into scrollback with its tags; `ESC [3J` and `ESC c` drop them. A `D` that finds no running tag is ignored. |
| Resize and reflow | Tags move with their cells; counts are logical lines; nothing to fix up. |
| Duplicate marks (iTerm2's integration in your `.zshrc` too) | A repeated A/B re-tags the same row; a second C fills only a missing command; a second D finds a `done` tag and is ignored. |
| Faked marks in output (`cat file`) | They can make a fake block. Rerun never presses Return, the chip shows the command it would type, and control characters are refused (§4). |
| Mark inside a synchronized update | Dropped (the cursor isn't settled). |

## 4. Interaction

- **The chosen block:** none at first. Set by ⇧⌘↑ / ⇧⌘↓ or a click on a gutter bar; cleared by typing, by a click in
  the grid, and when its tag disappears. **Actions with nothing chosen apply to the newest finished block.**
- **⇧⌘↑ Previous Command / ⇧⌘↓ Next Command (AI's call; ⌘↑ / ⌘↓ stay page scroll):** the blocks in order (`running`
  and `done`, oldest first). ⇧⌘↑ with nothing chosen picks the newest block whose prompt starts above the live
  prompt; with one chosen, the one before it (none: stays). ⇧⌘↓ picks the next; past the newest it clears the
  choice and scrolls to the bottom. The view scrolls so the chosen block's first prompt row is the top row, unless
  the whole block is already in view; at once (no smooth scroll, so nothing to reduce for Reduce motion). On the
  alternate screen or with no blocks: nothing.
- **⇧⌘C Copy Command Output (AI's call):** the output's logical lines as text (trailing blank lines dropped). A
  running block: up to the cursor row. Built on the background executor (`TerminalCore` is `Arc`'d; the Term lock is
  held only for the read), then written to the clipboard.
- **Copy output with no output** leaves the clipboard alone, and the chip's button is disabled.
- **Copy command:** the shell-reported `cmd`, exactly. **Copy command and output** (palette only): `$ <command>` +
  newline + the output, ready to paste to an agent.
- **Rerun (AI's call: types it, never runs it):** pastes the command at the prompt and **does not press Return**;
  you press Return. Allowed only when: the shell is at a prompt (the last mark was A or B, and not on the alternate
  screen), the cursor is at the input start B recorded (nothing typed), the block has a `cmd` (not `t=1`), the
  command has no control characters other than tab and newline, and a multi-line command only with bracketed paste
  on. Sent through `TerminalView::paste` (bracketed when the shell asked), so it snaps to the bottom like typing.
  Otherwise the button is disabled and the palette command does nothing.
- **Mouse:** hover the gutter → chip; click a bar → choose that block (no scroll); click a chip button → its action
  (the chip stays, the choice is kept). A press in the gutter on a bar never starts a selection or a mouse report; a press
  in the gutter **off a bar behaves as before**; a press in the grid behaves as today.
- **The palette** (Terminal place): `Previous Command` (⇧⌘↑), `Next Command` (⇧⌘↓), `Copy Command Output` (⇧⌘C),
  `Copy Command`, `Copy Command and Output`, `Rerun Command`. Keywords: `block prompt jump`, `block copy`, `block
  run again`.
- **Help parity:** Help's Terminal group lists `Previous command` (⇧⌘↑), `Next command` (⇧⌘↓), `Copy command
  output` (⇧⌘C); the parity tests cover them.
- **Agent-state detector: unchanged (AI's call).** Marks are not output the tracker counts any differently, and the
  tag doesn't change the tail text.
- **Activity: unchanged (AI's call).** Recording every shell command would flood the agent timeline's 200 entries;
  listed under "Not in M11l".
- **Scripting API, MCP and the CLI: unchanged (AI's call).**

## 5. Look and contrast

- `block_ok`, `block_failed`, `block_running` ≥ **3:1** on the terminal background on every built-in theme
  (`ensure_contrast(…, background, MARK_MIN)`); `accent` likewise for the chosen bar.
- Chip text: `text`, `text_dim`, `text_faint` on `panel` and `panel_hover` ≥ **4.5:1** (already held since M11j);
  `exit N` in `block_failed` re-held at 4.5:1 on `panel`.
- Separator `card_border` ≥ 1.4:1 on the background (`QUIET_LINE_MIN`).
- Nothing animates; Reduce motion has nothing to turn off. The cursor's breathing is untouched.

## 6. The one new setting (AI's call: `command-blocks = on | off`, default on)

- **Why a setting, against the rule:** M11l is the first time fuxx changes how your shell starts. If the injection
  ever clashes with a zsh setup, there must be a way out that isn't editing fuxx or your dotfiles. No existing
  setting covers it.
- `off`: new terminals get no `ZDOTDIR` injection, and the window draws no blocks and ignores the block keys (tags
  from marks you send yourself stay hidden). Terminals already running keep the shell they started with.
- In the settings file and in **Preferences → Sessions** as a switch: `Command blocks` / sub-line `Marks each
  command in zsh. New terminals only.`

## New design tokens

In `theme.rs` `UiColors` (fitted per theme, tested on every built-in theme): `block_ok`, `block_failed`,
`block_running`. Constants in the new `block_view.rs`: `GUTTER_BAR` 2.0, `GUTTER_BAR_ACTIVE` 3.0, `GUTTER_INSET` 8.0
(6.0 in a split), `CHIP_GAP` 4.0 (the chip is one row tall: `CHIP_HEIGHT` was dropped), `CHIP_MIN_WIDTH` 197.0, `CHIP_LONG_MIN` 282.0, `CHIP_HIDE_MS` 150, `CHIP_NARROW` 220.0, `CHIP_CMD_CHARS` 40.
In `blocks.rs`: `CMD_CAP` 2048, `PROMPT_LINES_CAP` 8, `C_SCAN_ROWS` 500, `MARK_CAP` 8 KiB.

## Not in M11l

- **Collapse (AI's call, from your list).** Alacritty has no way to fold rows: hiding a block's output would mean a
  view-row ↔ grid-line map threaded through paint, hit-testing, selection (and what a copy of a selection spanning a
  fold contains), links, mouse reports to programs, the wheel's line counting, the cursor's glide and page scrolling,
  plus a rule for output arriving inside a folded block. That is a second terminal view, and the riskiest part of
  the riskiest milestone. If you want it, it is its own later milestone with its own design.
- **bash and fish injection** (§1): bash because macOS's is 3.2 and injection needs an argv rewrite; fish because it
  can't be verified here. Shells that send OSC 133 themselves still get blocks.
- Blocks in remote shells over SSH (no remote integration), inside tmux, or on the alternate screen.
- A tint on failed output; durations ("took 3.2 s"); a sticky header for a long block; block search or filters;
  selecting several blocks; sharing a block; "ask the agent about this output".
- Activity entries for shell commands; status changes from exit codes; notifications when a long command ends.
- Exposing blocks to the scripting API, MCP or the CLI (`fuxx text` stays as is).
- A terminal find (⌘F) and "select the block's output" as a selection.
- M11m's title bar and M11n's performance work (listed in the roadmap, designed separately).

## On-screen checklist (pending, after the combined release)

1. **A fresh terminal (zsh)**: run `echo hi`, then `false`. Each command gets a thin bar in the left padding beside
   its prompt; `false`'s bar is red; a hairline sits above each prompt; the live prompt has a hairline above it too.
2. Your prompt, `.zshrc`, aliases and history behave as before; `echo $ZDOTDIR` prints what it printed in your old
   terminal (empty for most).
3. Hover a bar: a **one-row chip** shows `exit 0 · echo hi` (or `exit 1 · false`) and Copy command / Copy output /
   Rerun; it covers no row above or below and has no shadow; moving off hides it.
4. **Right prompt:** with a right prompt (RPROMPT, or powerlevel10k's right side), hover its command's bar: the chip
   stops before the right prompt and the prompt stays readable.
5. Copy output of `ls -la`, paste somewhere: exactly the listing, no prompt. Copy command: `ls -la`. Copy output on a
   command with no output leaves your clipboard as it was and its button is greyed.
6. Rerun on `echo hi`: `echo hi` appears at the prompt, not run; Return runs it. With something typed at the prompt,
   Rerun is greyed.
7. ⇧⌘↑ ×3 walks back through the last three commands (the chosen one gets a full-height accent bar and its prompt
   comes to the top); ⇧⌘↓ walks forward and past the newest returns to the bottom. ⌘↑ / ⌘↓ still scroll a page.
8. ⇧⌘C with nothing chosen copies the last command's output; after ⇧⌘↑ it copies the chosen one's.
9. `vim` (or `htop`): no bars or lines while it's open; after quitting, the blocks above are as before.
10. Start an agent preset (e.g. claude): its row is one running block while it runs; Claude Code looks and works as
    before.
11. Resize the window narrower and wider over a long command: bars and hairlines stay on the right lines.
12. `seq 1 20000`: fuxx stays responsive; afterwards the block for `seq` is gone once its prompt scrolled past the
    scrollback (the output tail is plain); later blocks are fine.
13. `clear`: the screen clears; new commands get blocks.
14. An SSH workspace (⇧⌘S): no bars inside the remote shell.
15. Persistent sessions on: quit and reopen fuxx; the blocks from before are still there; new ones appear.
16. ⌘K, type "command": Previous Command, Next Command, Copy Command Output, Copy Command, Copy Command and Output,
    Rerun Command. ⌘/ lists ⇧⌘↑, ⇧⌘↓, ⇧⌘C under Terminal.
17. Preferences → Sessions → Command blocks off; open a new terminal: no bars, `echo $ZDOTDIR` as before; the old
    terminal keeps its bars until closed. Turn it back on.
18. Light and dark (Latte, Mocha, one more of each): bars visible, red bar clearly red, chip text readable.
19. A split pane: bars sit in the narrower padding; the chip stays inside the pane.
20. **powerlevel10k** (your own setup): every command gets a block, not just the first; an empty Return or ⌃C at the
    prompt leaves no bogus block.
21. `FOO=1 exec zsh`, then `echo hi`: the new shell's prompt appears and no block stays "running" from the `exec`.
    (After an `exec zsh` the new shell has no integration, so no further blocks: a known limit.)
22. **The darker tab bar:** the empty part of a pane's tab bar is clearly darker than the tab box (card), on Mocha
    and on a light theme; the right sidebar's panel strip matches.

## M11l as built (2026-11-09)

Where the build settled something the plan left open, or changed it (AI's rulings after each task's review and the
final whole-branch review):

- **Ids start from a random u64** (Settled 3): the first review found clock-seeded ids guessable, so a local process
  could forge an OSC 8 id and make Rerun accept it. The range check stays.
- **§1 shell integration was rewritten after the p10k bug and the final review** (see §1): `_fuxx_done` first in
  `precmd_functions`, the PS1 marking last at every prompt and the line-init at every prompt, p10k's own shell
  integration on and its C/D off, the guarded `add-zle-hook-widget`, no C for an `exec` that replaces the shell
  (also after `builtin` / `command` / `NAME=value`), and the scripts checked at each spawn. A multi-line prompt that
  starts `\r\n ESC[A` no longer counts `p` one too high.
- **§2 chip:** one row tall, no shadow, 11 pt text that shrinks at small fonts, 1 cell before a right prompt,
  short labels also under 282 pt (see §2). You chose this from mockups on 2026-11-09. The chip closes only when its
  block moves, not on any output (the first build closed it on any output and was unusable while an agent printed).
- **§4 gutter press:** a press off a bar behaves as before.
- **Copy output with no output** leaves the clipboard alone and is disabled.
- **Rerun** sends the bracketed state it checked (one read). Only blocks this terminal's tagger issued this run can be
  rerun; a restored or forged one offers Copy only (tooltip: "Only commands run since fuxx connected to this
  session can be rerun"). Palette Rerun with nothing chosen targets the newest finished block.
- **Task 2b, a darker tab bar (your request, 2026-11-09: "the color of the empty tab is hard to tell apart with the
  tab box"):** `tab_bar` is the canvas colour pushed away from `card` until the contrast reaches `TAB_BAR_MIN` (1.3),
  capped below 1.6:1 against the canvas; light themes cap at `TAB_BAR_LIGHT_CAP` (1.15) to keep the text hierarchy.
  `tab_hover` follows; text and mark floors on both are kept. It applies to pane tab bars and the right sidebar's
  panel strip.
- **The title-area ui-shot failed on master too:** M11i removed the sidebar's right edge, but `footer_buttons` and
  `popover_width` still subtracted 1 pt, and the check read one sample per point, which hid it at 1x. The model now
  matches what is drawn, and the check reads every device pixel.
- **Recorded from the reviews:** a service and its replica may diverge on a stalled sync update; attaching between
  A and B gives `p=0` once; without `PROMPT_SP` (bash, fish) an output's last row shared with the next prompt goes to
  the next command row; a bare A then the shell's D (a `cat` of a recorded session) strands a block running; nested
  shells: the outer D takes the nearest (inner) tag; the first command at a reattach-restored prompt is copy-only;
  forged OSC 133 marks in `cat`-ed output can make a "this run" block with a chosen command (the plan accepts it:
  Rerun never presses Return and the chip shows the command); ui-shots restore the user's clipboard as text only.
- **Collapse is NOT in M11l** (AI's call: alacritty can't fold rows; see "Not in M11l").

## Limits (known, accepted)

- A plain `precmd()` function's prints land in the previous block (zsh runs it before `precmd_functions`); pinned by
  a test.
- `echo x; exec zsh` strands a block (only the first word is checked), and integration is lost after any `exec zsh`
  (no re-arm).
- p10k with an explicit `POWERLEVEL9K_TERM_SHELL_INTEGRATION=false` and a transient prompt loses blocks (likely async
  p10k redraws too).
- A user `zle-line-init` that returns non-zero, or is redefined after the first prompt, in the fallback mode.
- A prompt that goes up and back down inside `PS1` undercounts `p`.
- p10k's dump cache invalidates between terminals.
- Rerun's "nothing typed" means the cursor is at the input start: ⌃A then Rerun prepends (it never runs).
- An RPROMPT with an inner double space is partly covered by the chip.
- With a one-line prompt the chip covers that row's own hairline.
- Very narrow splits (under about 200 pt) and a command row scrolled off the top behave as before.
- Copy output stays enabled for an all-blank output (it does nothing).
- bash and fish aren't injected.

## Settled before planning (AI's calls after a builder's read, 2026-11-09)

Re-reading the design as the builder who must plan it, these were the places they would have had to guess:

1. **Module split:** `shell_marks.rs` (pure scanner: `MarkScanner::feed(&[u8]) -> Vec<(usize, Mark)>`, offsets just
   past each terminator); `blocks.rs` (the tag's encode/decode, `Tagger`, `advance`, and readers over `&Term`:
   `list`, `visible`, `output_text`, `prompt_ready`); `shell_integration.rs` (scripts, `install`, `env_for`);
   `block_view.rs` (the view's pure rules: jump, rerun bytes, chip labels, colours) plus paint and events in
   `terminal_view.rs`.
2. **`Mark`:** `PromptStart`, `CommandStart`, `OutputStart { cmd: Option<String>, truncated: bool }`,
   `Done { exit: Option<i32> }`. `cmdline_url` decoding as `detector::cwd_from_report` does (`%XX`); bad encoding →
   `cmd: None`.
3. **Ids:** `fuxx-block-<n>`, `n` from a per-`Tagger` counter seeded from a **random u64** (as built: a clock seed was guessable, and Rerun trusts
   an id only if this terminal's tagger issued it), so a window's and the service's tags don't collide; an id found by a scan is kept when rewriting. The view refers to a
   block by id; an id it can't find any more clears the choice.
4. **The tagger's own state** (`Unknown`, `Prompt { a: Point, lfs: usize, input_start: Option<Point> }`, `Running`)
   lives next to the Term in `Shared`, in the `Replica`, and in the service's `State`. It starts `Unknown` (and after a
   reattach); `prompt_ready()` is true only in `Prompt` with `input_start` set. `C` and `D` work from scans, never
   from the state alone, so an `Unknown` tagger still finishes a block it finds.
5. **Splitting `advance`:** for each mark, `processor.advance(term, &bytes[from..end_of_mark])`, then apply the
   mark; then the rest. Byte scanners that already run (`OscDetector`, `CwdReports`, `ClearCounter`) keep seeing the
   whole chunk as today.
6. **`visible()`** is computed inside `TerminalCore::snapshot_with_blocks(chosen)` (the snapshot plus blocks) under the same lock and put in `GridSnapshot.blocks:
   Vec<BlockView>` (`id`, `outcome`, viewport rows of prompt, command and output, each possibly starting above row 0
   or ending below the last). It scans the viewport rows plus 8 below (a prompt above a tag just under the view) and,
   for the chosen id only, the whole scrollback (≤ 10,000 column-0 reads). Empty on the alternate screen.
7. **Output text** = `Term::bounds_to_string` from (first output row, column 0) to (last output row, last column),
   lines right-trimmed, trailing empty lines dropped; for a running block the last row is the cursor's.
8. **Jump scroll:** `display_offset = clamp(−prompt_top_line, 0, history_size)` via `TerminalCore::scroll(Scroll::Delta)`.
9. **Rerun bytes** are pure: `rerun_bytes(cmd, prompt_ready, cursor_at_input_start, bracketed, alt_screen) ->
   Result<Vec<u8>, Refusal>`; refusals: `NotAtPrompt`, `Typed`, `NoCommand`, `Control`, `MultiLineUnbracketed`.
10. **The gutter's hit area** is the left padding from the pane's inner edge to the grid's origin, rows of a block's
    prompt and command (and, for the chosen block, all its rows). The painted geometry is shared with the mouse
    handlers through an `Rc<RefCell<…>>` written in paint, as `layout` is today.
11. **The chip** is an absolutely positioned child of the terminal view's root `div`, built in `render` from
    `hovered_block` (id + anchor), not painted in the canvas, so its buttons are ordinary GPUI elements with
    `on_mouse_down`. The 150 ms hide is a GPUI timer task, cancelled by re-entering.
12. **Installing the scripts** runs in `main` before the first window opens (two small files, a read-compare-write);
    `env_for(shell: &Path, user_zdotdir: Option<&OsStr>, enabled: bool, dir: Option<&Path>) -> Vec<(String, String)>`
    returns the variables to add to `ShellEnv.set` (`ZDOTDIR`, and `FUXX_ZSH_ZDOTDIR` only when you had one). Called
    from `session_env` (both `open_local` and `daemon_create` paths) only when `program` is `None`.
13. **The setting:** `appearance.rs` `command_blocks_from_entries` (`switch_from_entries(…, "command-blocks",
    true)`); `OnOff::CommandBlocks` in Preferences → Sessions after Keep running; read when a terminal is spawned and
    when the view paints / handles a block action.
14. **New actions:** `actions!(terminal, [PrevCommand, NextCommand, CopyCommandOutput, CopyCommand,
    CopyCommandAndOutput, RerunCommand])`; bindings `cmd-shift-up`, `cmd-shift-down`, `cmd-shift-c` in context
    `Terminal`; the last three without keys.
15. **Existing screenshots that change:** none by pixels: shot terminals are `cat` with no marks. New scenes are
    named `command-blocks*` (not `marks`, which already exists).
16. **Version:** no release of its own. The one combined release (M11i–M11n) ships after M11n (your rule,
    2026-11-09: "push the release after performance milestone"); its version is that release's call.

## Left to the builder

How `blocks.rs` lays out its scan helpers; the exact zsh percent-encoding loop and hook guards within the rules
above; whether the chip's shadow reuses `overlay_panel::shell` or a smaller copy of it; the exact hover and pressed
shades within the existing tokens; how the chosen block's tag line is cached between frames (a cache is optional,
the full scan is cheap).
