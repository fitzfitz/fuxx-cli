---
tags: [development, milestone, design]
status: built
---

# 60 - Milestone 11i - UI Uplift 2 (design)

**Status:: 🟨 Built, reviewed and merged to master 2026-11-08 (every detail AI's call; you skipped the design questions). Released in v0.5.0 (2026-11-10). On-screen check pending (22 items below). Changed 2026-11-10: in a repo the branch and
the folder are two lines (see "After release").**  ·  **Phase 4: an M11 extension.**  ·  **Plan:** [[M11i Plan - UI Uplift 2]]
**Added to the plan 2026-11-08 by you** (see [[12 - Scope]], "A modern layout…"). Comes **before M11h**, so
Changes and Activity are built into the new look.

## Goal

fuxx looks like a modern terminal for agents: every workspace and tab says **what runs there, or where**; the left
sidebar's rows are **compact** with a **quiet selection**; the panes and the right sidebar are **raised, rounded
cards** with gaps between them; the terminal text has **room to breathe**; a **status bar** at the bottom says what
the focused terminal is doing; the **tab bar** is clean; the **Files** tree has icons, guides and a root header.

## Why (your words, 2026-11-08)

- From a screenshot of fuxx: "I don't really like the current layout, can we do an uplift and turn the layout into a
  modern CLI with good UI/UX?"
- You chose all eight suggestions below for M11i ("I love all 1-11"; M11j–M11l take the rest).
- You skipped the design questions, so **every value, rule and layout choice below is AI's call**. The ones you may
  most want to overrule are marked **(AI's call)** where they're made and listed again in "Settled before planning".

## What exists today

- **Names:** a workspace is called by your name, else its focused terminal's name: your name for the terminal, else
  the program's OSC title, else the preset's name, else **╭∩╮** (M11c). A terminal at a plain prompt is usually
  ╭∩╮. Browser tabs use the page title; file tabs the file name.
- **Sidebar rows:** a 28 pt name line plus 14 pt per extra line (`branch • folder`, then a waiting question with a
  🔔 *Needs input* pill or a script note), so 42–56 pt each. The selected row is **filled with the accent**, its text
  and marks in `on_accent`.
- **Layout:** the sidebar, the panes and the right sidebar sit edge to edge, split by 1 px lines; split panes are
  split by a 1 px line, and the unfocused ones are veiled (40 %).
- **Terminal padding:** 10 pt at the sides and 8 pt top and bottom (8 and 6 in a split): `ui_tokens::size`.
- **Tab bar** (26 pt): tabs with an icon (or the status mark), the title, × on hover, the active one raised with a
  2 pt accent line on top; then **five** 20 pt icon buttons: new terminal, new browser tab, split right, split down,
  right sidebar.
- **Files:** a plain tree (22 pt rows, ▸/▾, the name in its git-mark colour, a letter mark), under a "Files" strip
  and a filter box.
- **Update Available:** a blue 28 pt button above the sidebar footer (M11e).
- **Ports:** fuxx knows nothing about listening ports today. It doesn't know a terminal's foreground program either;
  it knows each terminal's folder (`git_branch::Places`: OSC 7, else the shell's working folder from the kernel,
  else where it started) and branch.

## 1. Smart names everywhere

> [!warning] **Reverted by you on 2026-11-09** ("I still want the ╭∩╮ to be my default terminal name. It's my signature." … "we go back as before, unless its a process thing that auto rename the cli"). A terminal is called **your name, else the program's own title, else its preset, else ╭∩╮**; a workspace, your name, else its focused terminal's name, as before M11i. The `ps` look-up stays for the status bar, the port chips and the idle dot/ring only. The rules below are kept for the record.

### What a terminal is called (AI's call: the rules and their order)

The first that applies wins:

1. **Your name for it** (a rename of the tab, or `session.rename`): exactly that, nothing added.
2. **A program is running** in it (its foreground job isn't the shell itself): **`program · folder`**, for example
   `claude · fuxx-cli`, `npm run dev · invitee`, `vim · src`, `cargo test · fuxx-cli`.
3. **The shell is at its prompt**: **the folder**, for example `fuxx-cli`, or `~` in your home folder.
4. **SSH terminal**: **`host`**, then **`host · folder`** once the remote shell reports its folder (OSC 7). The
   program on the remote machine isn't looked up (it isn't on this Mac).
5. **The program can't be looked up** (a terminal run by the background service, `persistent-sessions = on`, whose
   shell pid fuxx doesn't have; or before the first lookup has answered): **`preset · folder`** if a preset started
   it, else **the folder**.
6. **Nothing is known yet** (no folder, no program, no preset): the program's OSC title if it set one, else
   **╭∩╮**. In practice this is only the first moment of a new terminal, or a lost background session.

- **The folder** is the last part of the terminal's folder (`fuxx-cli`), `~` for your home folder, `/` for the
  root. It comes from `Places`, as the sidebar's second line does today.
- **The separator** is ` · ` (a middle dot with a space each side).
- **The OSC title is no longer the name** (it was noisy: shell themes put `user@host:~/dir` there). It is shown in
  the tab's tooltip and the row's tooltip, under the name, when it says something the name doesn't.

### How the program is named (AI's call)

- fuxx runs one `ps -A -o pid=,ppid=,pgid=,tpgid=,args=` (`/bin/ps`) **off the UI thread**, at most once every
  **3 s**, only while the window can be seen and some terminal whose name is on screen is a local one with a shell
  pid. A run is capped at **2 s** (killed after). Checked on this Mac: the run takes about 20 ms for 760 processes.
- The shell's row says which process group is in the foreground (`tpgid`). If that is the shell's own group (or 0),
  the shell is **at its prompt**. Otherwise the **job leader** (the process whose pid is that group) is the program.
- Its `args` become a short name:
  - **Known agents** (anywhere in the arguments, by name or by package path): `claude` (also
    `@anthropic-ai/claude-code`), `codex` (`@openai/codex`), `opencode`, `aider`, `gemini`
    (`@google/gemini-cli`), `goose`, `amp` → that short name alone.
  - **Interpreters** (`node`, `python`, `python3`, `python3.N`, `ruby`, `perl`, `php`, `bun`, `deno`, and a shell
    running a script: `sh`, `bash`, `zsh`, `fish` with a script argument): named after the script they run, so
    `node /usr/local/bin/npm run dev` is named as `npm run dev`. A shell with no script (`bash` typed at the
    prompt) is named after the shell.
  - **Runners and tools with a subcommand** (`npm`, `pnpm`, `yarn`, `bun`, `npx`, `cargo`, `git`, `go`, `make`,
    `just`, `docker`, `uv`, `mise`): the tool and its next word that isn't a flag (`cargo test`, `git rebase`,
    `pnpm dev`); after `run` or `exec`, the word after that too (`npm run dev`).
  - **Anything else:** its file name (`vim`, `htop`, `less`, `ssh`).
  - A leading `-` (login shells) and the folder part of the path are dropped. `ps` writes a newline in an argument
    as `\012`; such a name is cut there. The name is at most **24 characters**, then `…`.
- **No flicker:** a terminal's program changes only after the **same answer twice in a row** (3–6 s), so a shell
  loop running many short commands keeps a steady name. A **known agent** is taken on its first sighting (you'd want
  to see `claude` at once), and so is the shell coming back to its prompt after a known agent.

### Where the smart name shows

- **Pane tabs:** the terminal's smart name, in full (`claude · fuxx-cli`); the tab truncates it at its end.
- **Sidebar rows:** a workspace you named keeps your name. Otherwise it's named after **its focused pane's
  terminal** (as today), so `claude · fuxx-cli`. A workspace of only browser tabs takes the shown page's title
  (else "Browser"); of only file tabs, the shown file's name (as today).
- **The window title, the top band's name (sidebar hidden), ⌘P and fuxx's own banners** use the same names.
- **Browser tabs** keep the page title ("New tab" before a page). **File tabs** keep the file name.
- **Renaming:** the rename box starts with the shown name selected. Enter on an untouched name keeps it automatic
  (as today); an empty name goes back to automatic; anything else is your name and **always wins**.
- **The scripting API is unchanged** (AI's call): `session.list` still reports `name` as before (your name, else
  the title, else the preset, else ╭∩╮), and targets by name resolve as before, so no script breaks.
- **╭∩╮** stays as rule 6's fallback, and as the background service's banner name when no window is open.

## 2. Compact two-line rows

- **Two lines, about 3/4 the height of today's typical row:** a row with a second line is **32 pt** (was 42; a
  waiting row with its question was 56, now also 32). A row with nothing under its name is **22 pt** (was 28).
  Rows stay **2 pt** apart, inset **6 pt** from the sidebar's sides, with **8 pt** padding left and right, and
  corners of **6 pt** radius.
- **Line 1 (17 pt):** the mark slot (14 pt), **8 pt** gap, the name (13 pt, **semibold**; was bold), then on the
  right in 11 pt `text_faint` tabular figures: the age (rows ≥ 220 pt wide, as today), 📌 if pinned, the tab-colour
  dot, and the ⌘1–⌘9 shortcut.
- **Line 2 (13 pt, 11 pt text),** indented to the name: one of these, the first that applies:
  1. a lost background session: `not live` in `text_faint`;
  2. **waiting:** `Needs input` in `ring_waiting`, then ` · ` and the question in `text_dim`, cut at its end (the
     🔔 pill is gone; the whole question is in the row's tooltip, in ⌘P and in the banner);
  3. a script's note or progress label, in `text_faint`;
  4. **`branch • folder`**, in `text_faint`, fitted as today (`fit_place`: the branch loses its end first, then the
     folder its start). *(Since 2026-11-10: the branch and the folder each on its own line, see "After release".)*
- A script's **progress bar** stays: 2 pt along the row's bottom edge, inside its padding.
- **A meaningful mark at the left** (AI's call): Working, Waiting and Done keep their M11 marks (spinning ring,
  pulsing halo, ✓). **Idle** now says one more thing: a **filled 6 pt dot** in `text_dim` when a program is running
  but quiet (a server, an idle agent), and a **hollow 6 pt ring** in `text_faint` when the shell is at its prompt or
  it isn't known. The × still replaces the mark on hover.
- **The shortcut shows only on hover** (AI's call), to keep rows quiet; ⌘1–⌘9 still work.
- Group headers (24 pt) don't change.

## 3. A subtle selection

- The selected row gets a **soft tint**, `row_selected`, and a **2 pt accent bar** at its left edge: 2 pt wide,
  inset **5 pt** from the row's top and bottom, corners of 1 pt radius, in `accent`.
- Its text and marks keep their **own colours** (`text`, the status colours), not `on_accent`. The name stays
  semibold.
- `row_selected` = the sidebar colour mixed toward `accent` by **18 %** on dark themes and **12 %** on light ones,
  lowered in 2 % steps while `text` would read below **4.5:1** on it.
- A waiting row that isn't selected keeps its `waiting_tint`; selected, it takes the selection's tint and keeps its
  waiting mark and line 2.
- Hover: an unselected row shows `row_hover` (as today); the selected row doesn't change on hover.
- The tab-colour dot drops its `on_accent` ring (it has no accent fill to stand out from).

## 4. The Files tree

Order in the right sidebar, top to bottom: the **"Files" strip** (as today; M11h puts its panel tabs there), the
**root header**, the **filter box**, the **tree**.

- **Root header** (40 pt, padding 10 pt left, 6 pt right):
  - line 1 (20 pt): a chevron **▾** (▸ while the root is collapsed), **4 pt** gap, the root folder's name (13 pt,
    semibold, `text`); at the right, **collapse all** (a 20 pt button like the tab bar's, glyph: two chevrons
    pointing up, tooltip "Collapse all folders").
  - line 2 (14 pt): the root's path, home-relative (`~/Kerja/proj/fuxx-cli`), 11 pt `text_faint`, its start cut
    (`…/proj/fuxx-cli`, never shorter than the folder's own name); the full path in its tooltip.
  - Clicking line 1 (not the button) collapses or expands the **root**: collapsed, the filter and tree are hidden and
    only the header shows. Remembered per root for the run, like the expanded folders.
- **Collapse all** closes every folder (their listings are kept, so reopening is instant), keeps the root open, and
  moves the selection to the selected row's top-level ancestor. It does nothing to the filter.
- **Rows** (22 pt as today): the chevron column (12 pt), the **icon** (12 pt) and a **6 pt** gap, the name, the git
  mark letter. Each level indents **14 pt** (was 12).
- **Icons** (AI's call: drawn line glyphs like the tab bar's, no icon font or images, so no new crates or assets):
  - folders: a **folder** glyph, an **open folder** while expanded, in `text_dim`;
  - files: the **page** glyph (the file tab's), tinted by type from the theme's ANSI colours, pushed to 3:1 on the
    sidebar's surface:

    | Tint | Extensions |
    |---|---|
    | red (1) | `rs`, `rb`, `swift`, `scala`, `html`, `htm` |
    | green (2) | `sh`, `bash`, `zsh`, `fish`, `vue`, `lua` |
    | yellow (3) | `js`, `jsx`, `mjs`, `cjs`, `py` |
    | blue (4) | `ts`, `tsx`, `mts`, `cts`, `c`, `h`, `cpp`, `hpp`, `cc`, `css`, `scss`, `java`, `kt` |
    | magenta (5) | `png`, `jpg`, `jpeg`, `gif`, `svg`, `webp`, `ico`, `pdf` |
    | cyan (6) | `go`, `json`, `jsonc`, `yaml`, `yml`, `toml`, `xml`, `sql` |
    | `text_dim` | `md`, `txt`, `lock`, dotfiles (`.gitignore`, `.env`), no extension, anything else |

  - A file's **name** keeps its git-mark colour (blue M, green A, red D) as today; the icon keeps its type tint.
- **Indent guides:** a **1 pt** vertical line in `guide` for each level above a row, at the centre of that level's
  chevron column, the full row height (so they join into lines). No guide at depth 0. No highlight of the selected
  row's guide.
- The `… N more` row and the empty states ("No file matches.", the SSH note) are as today.

## 5. Floating panels

- **The canvas:** the window's background behind everything is the sidebar's colour (`sidebar_background`; AI's
  call: no new canvas token, so the left sidebar looks as it does today).
- **The left sidebar is drawn on the canvas, not as a card** (AI's call, Arc's way). It holds the traffic lights and
  the window's drag band; inside a card the lights would crowd its corner, and above a card they'd cost a 38 pt band.
  Its right border line goes (the gap does that job). Its resize strip stays at its edge.
- **Cards:** each **pane** (its tab bar and shown tab) and the **right sidebar** is a raised card:
  - fill `card` (the terminal's background), corners **8 pt** radius;
  - a **1 pt** edge in `card_border`;
  - a shadow: **0 pt** x, **1 pt** y, **6 pt** blur, black at **30 %** on dark themes, the theme's shadow colour at
    **8 %** on light ones (`card_shadow_alpha`).
- **Gaps:** **6 pt** (`CARD_GAP`) between cards, between the sidebar and the first card, and between cards and the
  window's top and right edges and the status bar. With the sidebar hidden, also from the window's left edge, and
  the top band (38 pt) stays above the cards with the workspace's name, as today.
- **Splits:** each pane of a split is its own card; the 1 pt divider becomes the 6 pt gap. The gap is the grab strip
  (resize cursor), and while hovered or dragged a **2 pt** accent line is drawn in its middle, as today.
- **Focus ring:** with more than one pane shown, the focused pane's card edge is drawn in `accent` (1 pt) instead of
  `card_border`. A lone pane has no ring. The unfocused panes keep today's **40 %** veil, with rounded bottom
  corners to match.
- **Rounded corners and content** (AI's call; GPUI clips content to rectangles only): the tab bar's top corners
  are rounded with the card; the terminal and file views paint their background with **8 pt bottom corners**, and
  their padding (below) keeps the text and cell colours out of the corners. A **browser page** is a native view GPUI
  can't round: its bottom corners stay square inside the card (noted in "Not in").
- The 6 pt canvas strip above the cards drags the window, like the tab bars.
- **Narrow windows:** the gaps stay 6 pt; the right sidebar still leaves the panes at least 200 pt (as today, now
  counting the gaps).
- **Animations / Reduce motion:** nothing here moves; the sidebars' slides are as today.

## 6. Inner padding

- The terminal's padding grows to **16 pt** at the sides and **12 pt** top and bottom (was 10 and 8); in a split,
  **12 pt** and **10 pt** (was 8 and 6). The padding is painted with the terminal's background, and a click in it
  lands on the nearest cell, as today.
- Both are larger than the card's 8 pt corner, so nothing the grid paints can poke into a rounded corner.
- File tabs get the same left and top room before their gutter (12 pt top, the gutter starts 12 pt in).

## 7. The status bar

A **22 pt** bar along the window's whole bottom edge, on the canvas (no card), under the sidebars and the panes.
Always shown (no toggle; AI's call). Text is 11 pt (`text::META`); items are **14 pt** apart; **10 pt** padding at
each end.

### Which terminal it describes
The **followed terminal**: the focused one, or with a browser or file tab focused, the last terminal it showed (the
right sidebar's rule, `right_sidebar.followed`), whether or not the right sidebar is open. With no terminal at all,
the left side is empty.

### Left, in order
1. **Status:** the terminal's mark (10 pt box, animated like the sidebar's unless Animations are off or Reduce
   motion is on), then a word in `text_dim`:
   - Working → `Working · 4m`; Waiting → `Needs input · 2m` (in `ring_waiting`); Done → `Done · 1m`;
   - Idle with a program running → `Running`; Idle at a prompt → `Ready`; Idle and not known → `Idle`.
2. **Branch:** a **branch** glyph (a small fork, 12 pt) and the branch name, in `text_dim`. Only inside a repo. An
   SSH terminal shows `ssh` and its host here instead.
3. **Folder:** the folder, home-relative, in `text_faint`, its start cut if needed (never shorter than its own name).

### Right, in order
4. **Ports:** the TCP ports the terminal's programs are **listening** on, as chips: `:3000`, 11 pt tabular,
   16 pt tall, 6 pt side padding, corners 4 pt, fill `row_hover`, `row_active` on hover. Sorted, IPv4 and IPv6
   merged, **at most 3** then a `+2` chip (its tooltip lists them all). A port chip's tooltip: "Open
   localhost:3000"; **clicking it opens `http://localhost:3000`** in the workspace's browser tab, or a new one split
   right of the pane (AI's call: where an agent's page goes today, `browser_in`).
5. **Update button:** moved here from above the sidebar footer (**moved, not mirrored**; AI's call: the status bar is
   always on screen, so the offer is never hidden with the sidebar, and the sidebar gets its room back). A **16 pt**
   pill, corners 8 pt, M11e's fixed blue (`UPDATE_FILL`, `UPDATE_HOVER`), white 11 pt medium text, the 12 pt package
   glyph, **6 pt** gap; the same states and words as today (`updater::button`), the version dropped when short of
   room. Hidden when there's nothing to offer, as today.

### How ports are found (AI's call)
- Only for the **followed terminal**, only for a **local** terminal with a shell pid (not SSH, not a background
  service's), only while a program is running in it and the window can be seen.
- From the last `ps` table (the names' lookup): the shell's **descendants**; then one
  `/usr/sbin/lsof -nP -iTCP -sTCP:LISTEN -a -p <pids> -Fpn`, off the UI thread, capped at **2 s**, at most every
  **5 s**, and at once when the followed terminal changes. Checked on this Mac: about 16 ms.
- lsof missing or failing: no chips, and the next try waits 30 s.
- A server started outside the terminal's own processes (Docker, a launchd service) isn't shown.

### Short of room
The status mark and the update button always show. As the bar narrows: first the update button drops its version,
then the ports collapse into one `N ports` chip, then the branch is cut to 8 characters and `…`, then the folder
loses its start down to its own name, then the status word goes (the mark stays), then the folder goes, then the
branch.

## 8. A cleaner tab bar

- **30 pt** tall (was 26), in `tab_bar` (between the canvas and the card, so the bar reads as the card's top).
- **Tabs** float in it: **4 pt** from its top, flush with its bottom, corners **6 pt** on top only, **8 pt** left
  padding and 4 pt right, 6 pt between icon and name; min and max widths as today (90–200 pt). Each: the 12 pt icon
  (`>_`, globe, page) or the terminal's status mark while it isn't Idle, the name (13 pt), and **× on hover**.
- **The active tab** is filled with `card` (the terminal's background), so it **merges into the content** below it.
  Its name is `text`, the others `text_dim`. The **2 pt accent line on top is gone** (the focus ring shows the
  focused pane). An inactive tab has no fill, and `tab_hover` on hover. A waiting inactive tab keeps `waiting_tint`.
- **Buttons at the right** (3 instead of 5), 20 pt each, 2 pt apart, 4 pt from the edge:
  1. **+** (plus glyph): a new terminal tab (tooltip "New terminal tab (⌘T)");
  2. **⋯** (three dots): a menu of the pane's actions;
  3. **the right-sidebar toggle** (as M11g: its glyph, lit while the sidebar is open, tooltip with ⌥⌘\\), always
     shown at the far right, so it stays one click away.
  - AI's call: shown all the time, not only on hover (hidden buttons are hard to find, and hovering a bar to find
    the sidebar toggle would be worse than today).
- **The ⋯ menu** (one of the overlays: one open at a time, Escape or a click elsewhere closes it): 200 pt wide,
  26 pt items, anchored under the ⋯ with its right edge at the button's, kept 8 pt from the window's edges:
  - New Terminal Tab — ⌘T
  - New Browser Tab
  - Split Right — ⌘D
  - Split Down — ⇧⌘D
  - Clicking an item focuses that pane first, then acts, as the buttons do today. The ⋯ stays lit while its menu is
    open.
- Dragging tabs, the right-click menu, renaming and ⌃Tab are as today.

## New design tokens (AI's call)

In `ui_tokens::size`: `CARD_GAP = 6`, `CARD_RADIUS = 8`, `FOCUS_RING = 1`, `STATUS_BAR = 22`, `TAB_BAR = 30` (was
26), `TAB_TOP_GAP = 4`, `SIDEBAR_ROW = 22` (was 28) and `SIDEBAR_ROW_TWO = 32` (replacing `WAITING_ROW`),
`SELECTION_BAR = 2`, `TERMINAL_PADDING = (16, 12)`, `SPLIT_TERMINAL_PADDING = (12, 10)`.

In `UiColors` (from `Theme::ui()`; dark / light):

| Token | Value |
|---|---|
| `card` | the theme background |
| `card_border` | background mixed toward the foreground 10 % / 12 %, held at the quiet-line floor (1.4:1) on the canvas and the card |
| `card_shadow_alpha` | 0.30 / 0.08 |
| `row_selected` | sidebar → accent 18 % / 12 %, stepped down until `text` reads at 4.5:1 |
| `card_hover` | card → foreground 6 % (the right sidebar's hovered row) |
| `card_selected` | card → accent 18 % / 12 %, the same text rule (the right sidebar's selected row) |
| `tab_bar` | halfway between the card and the canvas |
| `tab_hover` | `tab_bar` → foreground 6 % |
| `guide` | card → foreground 12 %, at the quiet-line floor on the card |

The new surfaces join the list `text`, `text_dim` and `text_faint` are checked against, so every text colour still
reads at 4.5:1 on every built-in theme, light or dark.

## Not in M11i

- M11h's Changes, diff view and Activity; M11j's command palette (⌘K); M11k's first-run screen; M11l's command
  blocks.
- Settings for any of this: no density choice, no toggle for the status bar or the cards, no custom gaps or radii.
- An icon font, Nerd Fonts, images or per-file-name icons in the Files tree.
- Program names and ports for **background-service terminals** (`persistent-sessions = on`) and for programs on the
  far side of **SSH**; UDP ports; servers outside the terminal's own processes.
- Clicking the branch (switching branches) or the folder in the status bar.
- Rounded corners on **browser pages** (a native view).
- The left sidebar as a card; a redesign of its footer (the +, 🔔 and gear stay; only the update button leaves).
- Showing smart names in the scripting API.

## After release

- **2026-11-10, branch and folder on their own lines** (your words: "bisa ga kalau branch dan folder jadi 2 baris?
  hilangkan baris branch jika bukan git", then "A langsung kerjakan di master dan langsung push"). A row whose line
  under the name is where it is, and which knows both its branch and its folder, is three lines and **45 pt**
  (`SIDEBAR_ROW_THREE`: 17 + 13 + 13 and 2 to spare): the name, the branch (cut at its end when long), the folder
  (losing its start when long, its own name kept: `fit_place(None, folder)`). Outside a git repo there is no branch
  line: the folder alone, 32 pt as before. Waiting, a script's note and "not live" keep their single line (32 pt).
  A dragged row shows the same lines (`place_lines`). AI's calls: no icon on the branch line (the status bar keeps
  its); both lines in `text_faint`. Code: `sidebar::lines_under`, `row_height`, `place_lines`; the `sidebar-rows`
  ui-shot checks the heights. Not yet released.

## On-screen checklist (pending; released in v0.5.0)

1. A new terminal at a prompt is called `╭∩╮` in its tab and its sidebar row (your names change, 2026-11-09).
2. A program that sets its own terminal title (e.g. Claude Code) shows that title; when it stops, the name goes
   back to `╭∩╮` (or the preset's name for a preset terminal).
3. `npm run dev`, `cargo test` or `vim` that don't set a title leave the name as it was; the status bar still says
   `Running`, and the row's idle dot is filled.
4. Rename a tab (double-click it, or right-click → Rename Tab…) and a workspace (double-click its row, or ⌘R),
   on an idle terminal and on one running a program: the name sticks; clearing it brings the automatic one back.
5. An SSH terminal reads as its title or `╭∩╮`; the status bar shows `ssh <host>`.
6. The sidebar rows are tight: name, then in a git repo the branch on one line and the folder under it (outside a
   repo, the folder alone: no branch line) (changed 2026-11-10); a waiting row's second line reads
   `Needs input · <question>`.
7. An idle row's mark: a hollow ring at a prompt, a filled dot while a quiet program (a dev server) runs.
8. The selected row has a soft tint and a thin accent bar at its left, with its text in normal colours; the
   shortcut ⌘N shows only on hover.
9. The panes and the right sidebar are rounded cards with small even gaps; the left sidebar sits on the window. No
   square corner pokes out of a terminal card, even in `htop` or `vim` with a coloured background.
10. Split a pane: two cards with a gap; the focused one has an accent edge; dragging the gap resizes; the gap lights
    up while hovered.
11. Terminal text no longer touches the card's edges (16 pt sides, 12 pt top and bottom; a bit less in splits).
12. The status bar shows the focused terminal's status word and mark, branch and folder; switching tabs or panes
    updates it at once; with a file tab focused it keeps the last terminal's.
13. Start a dev server (e.g. `npx serve` or `python3 -m http.server 8000`): `:8000` appears within about 5 s;
    clicking it opens the page in a browser tab; stopping the server removes it.
14. Update Available (when there is one) is in the status bar's right end, not above the sidebar footer; clicking it
    offers the update as before.
15. The tab bar: tabs with icon, name and × on hover; the active tab merges into the terminal; at the right only
    +, ⋯ and the right-sidebar toggle. ⋯ opens New Terminal Tab, New Browser Tab, Split Right, Split Down; Escape
    closes it.
16. The Files tree: a root header with the folder's name ▾ and its path, a collapse-all button that closes every
    folder, coloured file icons, folder icons that open, and faint indent guides.
17. Narrow the window to ~720 pt: the status bar drops its pieces in order without overlapping, the rows and tabs
    truncate cleanly.
18. Light and dark themes (Catppuccin Latte and Mocha, plus one other of each): cards, selection, guides and the
    status bar all read clearly.
19. Animations off or macOS Reduce motion on: the status bar's mark is still, nothing slides that didn't before.
20. Activity Monitor: fuxx idle in front uses no noticeably more CPU than v0.4.1 (the 3 s names lookup and the 5 s
    ports lookup are the only new work).
21. Switch workspaces and back: names don't change by themselves (no blinking).
22. Hide the left sidebar, press ⌘P and type a name you gave a workspace: it's found.

## Settled before planning (AI's calls after a builder's read, 2026-11-08)

Re-reading the design as the builder who must plan it, these were the places they would have had to guess:

1. **Which terminals get their folders looked up.** Today only each visible row's *first* terminal. Now: every
   terminal whose **name is on screen**: the terminal each visible row is named after (its focused pane's, else its
   first), every terminal tab in the shown workspace's panes, and the followed terminal (status bar). Same `Places`
   cadence (5 s, visible only).
2. **Background-service terminals have no shell pid in the window** (`Terminal::pid()` is `None`). They fall back to
   rule 5 (`preset · folder`, else the folder). No protocol change in M11i.
3. **Program lookups run for all named-on-screen local terminals from one `ps` table**, kept in a new
   `Programs` beside `Places`, with a `frozen` switch and a `set_for_shots` for the screenshots. Unknown (never
   looked up) and "at prompt" are different states, so rule 5 can tell them apart.
4. **The stable-name rule** is a small pure helper: a pending answer and its count; known agents and "back to
   prompt after a known agent" are taken at once.
5. **The rows' name and the tabs' name are the same string** (no short form for tabs). Simpler, and what you asked
   for ("smart names everywhere").
6. **Rounded corners:** GPUI's content masks are rectangles (checked in the pinned GPUI: `ContentMask` has bounds
   only), so the card can't clip its children. The tab bar is drawn with rounded top corners; the terminal view and
   the file view paint their background quad with 8 pt bottom corner radii (`PaintQuad::corner_radii`); the veil is
   rounded too. Browser pages stay square (above).
7. **The split gap and the divider maths:** the line element becomes `CARD_GAP` wide; the accent line's offset,
   which assumed a 1 pt line, scales with the gap's width. `drag_to`'s ratio maths is unchanged.
8. **Window dragging:** the 6 pt canvas strip above the cards drags the window; the side gaps don't (they're the
   panes' grab strips or the sidebar's resize strip).
9. **The status bar's terminal** is `right_sidebar.followed`, which is already updated on every frame whether or not
   the right sidebar is open. The status mark joins `visible_statuses`, so the marks' ticker runs for it.
10. **Port clicks** use the existing `browser_in(ws, pane, url, by_person = true)`.
11. **The ⋯ menu is a new overlay** (`Overlay::PaneActions`), so the `overlays` screenshot checks it hides in-pane
    browser pages like the others.
12. **Root collapse and collapse all** live in `FileTree` (pure): `collapse_all()` and a `root_collapsed` flag kept
    in the per-root memory with the expanded set.
13. **No new key bindings, menu items or settings.** The Help panel's "every binding is listed" test is unaffected.
14. **The rename comparison** ("an untouched automatic name stays automatic") compares against the smart name the
    box was filled with, for both tabs and workspaces.
15. **Existing screenshots change:** most scenes' pixels move (padding, gaps, rows, tab bar). Each task updates the
    scenes it touches and the checks inside them (heights, hover positions, the `tab-bars-button` hover maths for
    3 buttons, the `update-button` scenes now in the status bar).
16. **Version:** M11i is a visible new look, released as **0.5.0**.

## M11i as built (2026-11-08)

Where the build settled something the plan left open, or changed it (AI's rulings while building, after each
task's review and the final whole-branch review):

- **Names never show flag values or paths.** A tool's subcommand word (`npm run dev`, `cargo test`) is shown only
  when it comes *right after* the tool and is a clean word (letters, digits, `. _ : -`, at most 32 characters, no
  `=`, `/`, `~`, leading `+` or `.`), and never after `--`. Otherwise the tool's name stands alone
  (`docker run -e TOKEN=…` → `docker`). Window titles and banners are therefore safe to show.
- **`ps`** runs with `LC_ALL=en_US.UTF-8`, reads its output as lossy UTF-8 and skips zombies (`stat=`), only for
  what shows it (each visible row's terminal and the followed one); a failed or timed-out `ps` waits 30 s and keeps
  the last answer. **Names no longer come from it** (your revert, 2026-11-09, below).
- **Names reverted by you (2026-11-09):** your name, else the program's own title, else the preset, else `╭∩╮`
  (`Session::label`), everywhere: tabs, rows, the window title, banners, ⌘P. `smart_name.rs` is gone. The folder
  look-up runs only for each visible row's first terminal and the followed one. Renaming was checked by a
  screenshot test that types a new name (double-click a tab, double-click a row, ⌘R; idle and running; the
  program changing its title meanwhile): the name sticks; no fault was found in the code, so check it on screen.
- **`ring_waiting`** is pushed to 4.5:1 on the waiting tints and the selection, in every theme (hue kept).
- **A waiting row's whole question** is in the row's tooltip (line 2 cuts it; below 200 pt it isn't drawn).
- **The card shadow's blur of 6 is a GPUI Gaussian sigma**, about CSS `0 1px 12px`: soft and wide, subtle in the
  screenshots.
- **The split's grab strip is exactly the gap** at any ratio; the accent line is centred in it. Known and parked:
  the split model's rectangles don't count the gaps, so in same-axis nested splits (A | [B | C]) dragging the inner
  gap runs 1.5–3 pt off the pointer, and Equalize leaves three-across panes about 3 pt unequal.
- **File tabs have an 8 pt room at the bottom** (as well as the top), so no change tint, selection or find highlight
  paints into the card's rounded bottom corners.
- **No right-sidebar card at all** when the window is too narrow for it (no 2 pt stub).
- **The tab bar has no separators between tabs** (top-rounded tabs don't need them). The first tab sits inside the
  card's top-left curve, checked by pixel in the screenshots. A half-scrolled tab at the left edge of a narrow bar is
  cut square by the row's clip (GPUI masks are rectangles); it stays inside the card's 1 pt edge.
- **Files:** git marks and type tints reach 3:1 on the card, its hover and its selection (Gruvbox's red D was 2.15
  on a selected row). Reopening a folder shows its kept listing at once and lists it again in the background, so a
  file an agent created while it was closed appears. The root's path is cut at a `/` (`…/proj/fuxx-cli`). The
  tree keeps refreshing while the root is folded, so unfolding shows a fresh tree.
- **Status bar:** when even the branch is gone and it still doesn't fit (a window under about 230 pt), the ports
  chip goes last, so the update pill always shows. The footer's notifications popover and + menu open 22 pt higher,
  above the bar. Ports are looked up 5 s after the last answer, and at once when the followed terminal changes.
  Timings on this Mac (from a shell; fuxx has no debug log): `ps` 0.02 s, `lsof` 0.02–0.05 s.
- **The idle mark and the name describe the same terminal** (the focused pane's) on an all-idle row.
- Left for a later clean-up (unused but harmless): `on_accent`, `look`'s `_pane_focused`, `BarButton::enabled`,
  `divider` (tokens board only).

## Left to the builder

The exact glyph drawings (folder, open folder, branch, three dots, plus, collapse all) within the 12 pt and 16 pt
boxes, following the existing glyph style (1.25 pt lines); the internal split of the status bar's drawing; how
`Programs` and the ports lookup share the `ps` table; the exact hover and pressed shades within the tokens above.
