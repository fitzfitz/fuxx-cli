---
tags: [development, milestone, design]
status: designed
---

# 62 - Milestone 11k - First-Run Screen (design)

**Status:: ⬜ Designed and planned 2026-11-09 (every detail AI's call; you skipped the design questions). Not released on its own: one release after M11l.**  ·  **Phase 4: an M11 extension.**  ·  **Plan:** [[M11k Plan - First-Run Screen]]
**Added to the plan 2026-11-08 by you** (see [[12 - Scope]], "A modern layout, a command palette, a first-run screen…").
Comes after M11j (merged), before M11l.

## Goal

The first time fuxx opens, a **Welcome card** sits over the window and does three things in one place: **pick a
folder** (your home folder unless you choose another), **start an agent** in it (one of your agent presets, marked
by whether it is installed, or a plain terminal), and **learn three shortcuts** (⌘K, ⌘N, ⇧⌘U). Return starts the
agent in that folder; Esc skips to the terminal fuxx opens today. It shows once, and Help → **Welcome to fuxx**
brings it back.

## Why (your words, 2026-11-08)

- From the UI uplift suggestions, which you all took: "I love all 1-11". The roadmap reads "First-run screen — pick
  a folder, start an agent, learn three shortcuts. After M11j." ([[40 - Roadmap]], [[12 - Scope]]).
- You skipped the design questions ("lewati pertanyaan", "pakai cara mu", "go ahead do them all autonomously"), so
  **every value, rule and layout choice below is AI's call**. The ones you may most want to overrule are marked
  **(AI's call)** where they're made and listed again in "Settled before planning".

## What exists today

- **The first launch** (`Workspace::new` → `restore` / `connect_service` → `reopen`): with nothing saved (no
  `sessions.json`, or `session-restore = off`, or nothing in it could start), `open_first` opens **one plain
  terminal in your home folder** (`Launch::default()`: no project, no preset) and `finish_restoring` starts saving.
  The window is never empty for long: when the last terminal (and page) goes, fuxx quits (`all_gone`).
- **The sessions file** (`restore::SavedState`, `~/Library/Application Support/fuxx/sessions.json`) already keeps
  state that isn't a setting: the sidebar's width and hidden flag, `hint_dismissed`, the right sidebar's state. A
  field missing from an older file takes a chosen default (`right_sidebar_hidden` uses `closed()` → true).
- **The tips card** (M11, `sidebar.rs` `render_hint`): "Three things to know" — `⌘N workspace · ⌘T tab · ⇧⌘S SSH ·
  ⇧⌘B browser`, the rings' meaning, `⇧⌘U jumps to what needs you`; its × hides it for good (`hint_dismissed`).
  Help → **Show Welcome Tips** (`help::ShowWelcomeTips`, M11d) and Preferences' **Show the tips card again** bring it
  back (the palette lists "Show Welcome Tips" too).
- **Agent presets** (`presets.rs`): the built-in `claude`, `opencode`, `codex`, changed by `agent-preset` lines (at
  most 9). `launch_preset(n)` opens a new workspace in the active terminal's folder with `run_preset: true`, which
  **types the preset's command into the new login shell** (`start_in`). ⌃⌘1…⌃⌘9 start them; the + menu lists them.
- **New Workspace in Folder…** (⌘O, `new_session_in_folder`): the native folder panel (`cx.prompt_for_paths`,
  folders only, one, button "Open"), then a plain terminal in that folder's group (`open_in`).
- **Your login shell's environment** (`workspace_ssh::login_env`, M10b): `$SHELL -ilc 'command env -0'`, read once
  per run off the UI thread, capped at 3 s (`LOGIN_ENV_CAP`), cached in a `OnceLock`. It holds the `PATH` your
  terminals get (a Finder-launched fuxx's own `PATH` is only `/usr/bin:/bin:/usr/sbin:/sbin`).
- **Overlays** (`workspace.rs` `overlays!`, `close_overlays`): one open at a time; each hides every in-pane browser
  page while open (`Overlays::any`); the `overlays` ui-shots scene opens each. Most are placed by `top_panel` and
  drawn with `overlay_panel::shell` (`panel`, `panel_border`, radius 10, a soft shadow).
- **Keys:** the binding table in `main.rs`; Help's list is built from it (`help_shortcuts::describe`, `keys_for`,
  `key_label`) and the palette shows the same keys. ⌘K = Command Palette, ⌘N = New Workspace, ⇧⌘U = Go to What Needs
  You, ⌘/ = Keyboard Shortcuts.
- **Contrast:** `text`, `text_dim`, `text_faint` are held at 4.5:1 on `panel`, `panel_hover` and `row_active` on every
  built-in theme (M11j's test), and `on_accent` on `accent` (theme.rs).

## 1. When it shows (AI's call: once, tracked as state, not a setting)

- **First run = the sessions file has never recorded the welcome.** A new field `welcomed` in `SavedState`: a fresh
  state is `false`; a file written before M11k (no field) reads as **`true`**, so **upgrading isn't a first run**
  (you already know fuxx), exactly as `right_sidebar_hidden` defaults for older files. No file at all (or one that
  can't be read) is a first run. It is state in `sessions.json`, like `hint_dismissed`; **no new setting**.
- **It shows when the first terminal has opened**, from `open_first`'s callback (after `finish_restoring`), and only
  if `welcomed` is false. So it covers every path that ends in today's first terminal (no file; `session-restore =
  off`; the service path's `reopen`), and never one that restored your work.
- **It counts as seen the moment it shows** (AI's call): `welcomed = true` and save. Simpler and sturdier than
  tracking every way it can close, and it never nags; if you quit with it open, Help → Welcome to fuxx brings it
  back.
- **Over the first terminal, not instead of it** (AI's call). Today's plain terminal in `~` opens as now and the card
  sits over it. So Esc (or anything that closes the card) leaves you exactly where fuxx leaves you today, the
  window is never empty, and nothing about restore, persistent sessions or quitting-when-empty changes.

## 2. What it is (AI's call: a centred card overlay with a scrim)

- An **overlay** (`Overlay::Welcome` in `overlays!`, `Workspace.welcome: Option<Entity<Welcome>>`), so it gets the
  existing rules for free: one overlay at a time, in-pane browser pages hidden while it's open, the keyboard back to
  the focused tab after it closes.
- **Centred in the window** (both ways), **520 pt wide** (less in a narrow window: 16 pt from each edge), on the
  `overlay_panel::shell` surface with 20 pt inner padding. Behind it a **scrim**: the theme's `shadow` colour at
  35 % alpha over the whole window **below the 38 pt top band** (`TOP_BAND`), so the window can still be dragged by
  its top and the traffic lights work.
- **What closes it:** Esc, the **Skip** button, a click on the scrim, a pick (§5), or another overlay opening (⌘K,
  ⌘/…). **Not** losing the keyboard (AI's call): the folder panel takes the keyboard while it's up, and the card
  must survive that. A click on the scrim counts as Skip.
- **No new key bindings** (AI's call): the card reads ↑, ↓, Return and Esc itself (`on_key_down` on its focused
  root, as Preferences and the terminal view do), so nothing joins the binding table and Help's list is unchanged.
  Every other key works as anywhere (⌘K opens the palette and closes the card; ⌘N opens a workspace behind it).
- **Reopened** from Help → **Welcome to fuxx** (§6), the same card shows over whatever is open.

## 3. The card, top to bottom

```
┌──────────────────────────────────────────────┐
│ Welcome to fuxx                              │  18 pt semibold, text
│ A terminal for running AI coding agents.     │  13 pt, text_dim
│                                              │
│ PICK A FOLDER                                │  11 pt, text_faint
│ ┌──────────────────────────────┐ [Choose…]   │  28 pt row
│ │ ~                            │             │
│ └──────────────────────────────┘             │
│ START AN AGENT                               │
│ ▌claude                             ⌃⌘1      │  28 pt rows; the chosen one row_active
│  opencode   Not found on your PATH  ⌃⌘2      │
│  codex                              ⌃⌘3      │
│  Plain terminal                              │
│ THREE SHORTCUTS                              │
│ [⌘K]  Command palette — do anything          │  keycaps
│ [⌘N]  New workspace for another agent        │
│ [⇧⌘U] Go to the agent that needs you         │
│ ──────────────────────────────────────────── │
│ Enter start · ↑↓ choose · Esc skip  [Skip] [Start claude] │
└──────────────────────────────────────────────┘
```

### Pick a folder (AI's call: home by default, the native panel to change it)

- The **folder row**: a 28 pt field (`panel_hover` fill, radius 6, 8 pt side padding) showing the folder as the
  sidebar writes folders (`home_relative`: `~`, `~/code/app`), its start cut when long; then a **Choose…** button
  (24 pt, `panel_border` edge, `text`, `panel_hover` on hover).
- **Default:** at first run, your **home folder**. Reopened later: the **active terminal's folder**
  (`active_project()`), else home.
- **Choose…** (a click, or Return while the folder row is chosen) opens the same native panel as ⌘O (folders only,
  one, "Open"). A pick replaces the folder; Cancel keeps it. The card stays open meanwhile and takes the keyboard
  back after, with the agent row you had chosen chosen again.
- **No recent-folders list** (AI's call): on a first run there are none, and reopened later the active folder is the
  likely one; ⌘P and the sidebar already reach the rest.

### Start an agent (AI's call: your presets, marked by detection; a plain terminal always offered)

- **One row per preset** (all of them, at most 9, in their order, so the keys line up), then **Plain terminal**. A
  preset row: its name (13 pt `text`), then a faint status word, and its key at the right (`⌃⌘1`… from the binding
  table, `text_dim`). Plain terminal has no key.
- **Detection, off the UI thread:** when the card opens, one background task reads your login shell's environment
  (`workspace_ssh::login_env`, ≤ 3 s, cached for the run) and looks each preset's **program** up in its `PATH`. The
  program is the command's first word, after any leading `NAME=value` words and with surrounding quotes removed; a
  program with a `/` is checked as that path. Found = an executable regular file in one of the `PATH` folders (at
  most **64** folders; empty and relative entries skipped). Nothing is run and **nothing is installed**.
- **What a row says:** while checking, `Checking…`; found, nothing (a clean row); not found, **`Not found on your
  PATH`**; unknown (no login environment, or a command that isn't a plain program such as `$(…)`), nothing. All
  `text_faint`. **Every row stays pickable** (AI's call): an alias or a shell function isn't on `PATH` but works when
  typed, and if the program really is missing the shell says so in the new terminal.
- **The chosen row** starts on the **first preset found**; while checking (or when nothing could be checked), the
  first preset; when every preset is known missing, or there are none, **Plain terminal**. When detection lands it
  moves the choice to the first found **unless you've moved it** (↑/↓ or a click).
- **No presets at all** (every `agent-preset` removed): only Plain terminal, under a note `No agent presets set.`

### Three shortcuts (AI's call: ⌘K, ⌘N, ⇧⌘U; static)

| Keys | Line |
|---|---|
| ⌘K | Command palette — do anything |
| ⌘N | New workspace for another agent |
| ⇧⌘U | Go to the agent that needs you |

- **Why these:** ⌘K reaches every other action and shows each one's key, so it teaches the rest; ⌘N is how you run
  a second agent beside the first, fuxx's whole point; ⇧⌘U is what makes many agents bearable. ⌘T, ⇧⌘S, ⇧⌘B and
  the rings stay the tips card's.
- **Static** (AI's call): keycaps and a line, nothing to press. The keys are read from the binding table
  (`help_shortcuts::keys_for` on the real bindings), so they never disagree with Help or the palette.
- **Keycaps:** 22 pt tall, at least 28 pt wide, 6 pt side padding, radius 5, `panel_hover` fill, 1 pt
  `panel_border` edge, the keys in 12 pt `text`, tabular; the line 13 pt `text_dim`, 10 pt after the widest cap.

### Footer

- A 1 pt `panel_border` line, then on one row: the hint `Enter start · ↑↓ choose · Esc skip` (11 pt `text_faint`)
  at the left; at the right **Skip** (no fill, `text_dim`, `panel_hover` on hover) and the primary **Start …**
  button: `accent` fill, `on_accent` text, 13 pt medium, 28 pt tall, 12 pt side padding, radius 6. Its label
  follows the chosen row: **`Start claude`** (the preset's name) or **`Open Terminal`**.
- The rows' looks: 8 pt side padding, radius 6, 2 pt apart; chosen `row_active` with the name medium weight;
  hovered `panel_hover`. Section headings 11 pt `text_faint`, uppercase, 14 pt above, 6 pt below.

## 4. Keyboard and mouse

- **The choosable rows**, in order: the folder row, each preset, Plain terminal. **↓ / ↑** move between them, round
  from the last to the first. **Return**: on the folder row, Choose…; on an agent row, Start. **Esc**: Skip.
- **Mouse:** a click on an agent row chooses it (Start or Return then starts it; AI's call: no double-click, so a
  stray second click never launches anything); a click on the folder field or Choose… opens the panel; Start and Skip are buttons; a click on the scrim is Skip.
- **Tall content in a short window:** the card is never taller than the window less 38 pt (the top band) and 16 pt
  below; the agent list scrolls when it must (the chosen row kept in view); the title, folder, shortcuts and footer
  never scroll.

## 5. Doing the pick (AI's call)

- **The card closes first**, then the keyboard goes back to the focused tab, then the pick runs.
- **Start an agent** opens a **new workspace** in the folder running the preset, exactly as ⌃⌘n would there:
  `Launch { project, preset, run_preset: true }`, `At::New`. **Open Terminal** opens a plain one there (as ⌘O).
  `project` is the folder you chose; **left at home, it is none** (AI's call), as today's first terminal, so no `~`
  group appears.
- **At first run, the agent takes the first terminal's place:** the card remembers the terminal it opened over. If,
  when you pick, that terminal still exists and is the only tab of its workspace, it is **closed once the new one
  has opened** (`open_session_then`'s callback, then `close_at`), so you end with one workspace, the agent's, as #1.
  If the new one fails to open, the first stays. With persistent sessions on, closing it ends its service session,
  as ⌘W would.
- **Plain terminal in home at first run** just closes the card and leaves the first terminal (it is already that).
- **Reopened from Help**, a pick always opens a new workspace; nothing is closed.
- **SSH** (AI's call): the welcome starts local work only; ⇧⌘S stays in the tips card and the menus. Reopened over an
  SSH terminal, the folder defaults to home.

## 6. The Help menu and the tips card (AI's call: kept distinct)

- **Help → Welcome to fuxx** (new action `help::ShowWelcome`, no key) opens the card on the main window, over what
  is open; it is the **first** Help item. The palette lists it (its parity test requires it): **Welcome to fuxx**,
  group Window & app, keywords `first run start agent folder intro`.
- **"Show Welcome Tips" is renamed "Show Tips Card"** (menu item and palette name; the action stays
  `help::ShowWelcomeTips`, the palette keeps `welcome tips` as keywords), so the Help menu doesn't have two
  "Welcome" items, and it matches Preferences' "Tips card".
- **The tips card is unchanged**: it shows after the welcome until you close it (it explains the rings and ⌘T,
  ⇧⌘S, ⇧⌘B, which the welcome doesn't), and it is under the scrim while the card is open. The welcome neither
  closes nor reopens it.

## 7. Everything else it touches

- **Session restore and persistent sessions:** unchanged; the card shows only after today's first terminal, and only
  on a first run. A restored launch never shows it.
- **The scripting API, MCP and the CLI are unchanged.** A script may open terminals while the card is up; they
  appear behind it, and the card stays until you close it. Nothing here is scriptable.
- **Browser pages:** hidden while it's open (it's an overlay). At first run there are none.
- **Animations / Reduce motion:** nothing in the card moves. It opens and closes without animation; `Checking…` is
  plain text (no spinner); the chosen row changes at once.
- **Contrast:** the card draws text only in `text`, `text_dim`, `text_faint` on `panel`, `panel_hover` and
  `row_active` (held at 4.5:1), and `on_accent` on `accent` (held at 4.5:1). A test re-checks exactly these pairs on
  every built-in theme for the welcome, so a theme change that breaks them fails here too. The scrim carries no text.
- **Help parity:** no new key binding; `help::ShowWelcome` is a menu item only.

## New design tokens

None in `ui_tokens` or `UiColors` (AI's call). Constants in `welcome.rs`: `CARD_WIDTH` 520, `CARD_PADDING` 20,
`ROW` 28, `KEYCAP` 22, `SCRIM_ALPHA` 0.35, `EDGE` 16; `PATH_CAP` 64 in `agent_detect.rs`.

## Not in M11k

- Installing an agent, or links to install pages; running a program to test it (`--version`); detecting CLIs that
  aren't presets.
- A recent-folders list; SSH from the welcome; choosing a theme, font or any setting there; **new settings** of any
  kind.
- Interactive shortcut practice ("press ⌘K now"); a tour or step-by-step pages; animation.
- Changing the tips card's content, or retiring it.
- A welcome in Preferences or popped-out windows; scripting the welcome.

## On-screen checklist (pending, after the combined release)

1. **A first run** (with Keep running after quit off): quit fuxx, rename `~/Library/Application Support/fuxx/sessions.json`
   to `sessions.json.bak`, open fuxx: the terminal opens and the Welcome card sits over it, centred, with the window
   dimmed behind it. (After the checks, quit and put your file back.)
2. Folder shows `~`; Choose… opens the macOS folder panel; picking a folder shows it (`~/…`); Cancel keeps `~`.
3. Agents: your presets in order with ⌃⌘1… at the right, then Plain terminal; one that isn't installed says
   "Not found on your PATH"; `Checking…` shows only briefly. The first installed one is chosen.
4. ↑/↓ move the choice and wrap (through the folder row); the Start button's label follows (`Start codex`, `Open
   Terminal`).
5. Return starts the chosen agent in a new workspace in the chosen folder; the first terminal is gone; the agent
   runs in the right folder.
6. Esc (on another first run, or after reopening): the card goes and the terminal has the keyboard.
7. A click on the dimmed area skips; the window can still be dragged by its top band.
8. Relaunch fuxx: no Welcome card.
9. Help → Welcome to fuxx brings it back over your workspaces; picking an agent opens a new workspace and closes
   nothing. Help's last group says "Show Tips Card", which still brings back the tips card.
10. ⌘K while the card is open: the palette opens and the card closes; "welcome" in the palette finds Welcome to fuxx.
11. The three shortcuts read ⌘K, ⌘N, ⇧⌘U, with their lines.
12. A browser tab focused, Help → Welcome to fuxx: the page hides while the card is open and comes back after.
13. Light and dark themes (Catppuccin Latte and Mocha, plus one other of each): rows, keycaps, the chosen row and
    the Start button read clearly.
14. A small window (~500 × 420 pt): the card fits with a margin; the agent list scrolls; nothing is cut off.
15. Nothing moves in the card with Animations on or off.

## Settled before planning (AI's calls after a builder's read, 2026-11-09)

Re-reading the design as the builder who must plan it, these were the places they would have had to guess:

1. **The field:** `SavedState.welcomed: bool` with `#[serde(default = "already_welcomed")]` (→ `true`);
   `Default for SavedState` sets `false`; `saved_state()` writes `self.welcomed`. `Workspace.welcomed` is set in
   `new` from `read_saved_quietly().is_some_and(|s| s.welcomed)`; `blank` (and so `for_shots`) starts it `true`, so
   screenshots never see a welcome they didn't ask for. A pure `restore::welcome_due(saved: Option<&SavedState>) ->
   bool` states the rule for tests.
2. **The hook:** `open_first`'s `done` callback becomes: `finish_restoring`, then `if !self.welcomed {
   self.open_welcome(Origin::FirstRun { over: id }) }` with `id` the opened terminal's (none: no welcome; the first
   terminal failed). `open_welcome` sets `welcomed = true` and saves before showing.
3. **`hold_saving`:** when the sessions file is held (a leftover service may run its sessions), `save` writes
   nothing, so a first run's `welcomed` isn't kept and the card may show again next launch. Accepted (rare; fixing it
   would write the file the hold exists to protect).
4. **Origin:** `enum Origin { FirstRun { over: u64 }, Help }`. Default folder: `FirstRun` → home; `Help` →
   `active_project()` else home.
5. **The pick's effect is pure:** `fn pick_effect(origin, over_alone: bool, choice: &Choice, folder_is_home: bool)
   -> Effect` with `Effect::{JustClose, Open { close_after: Option<u64> }}`: FirstRun + Plain + home → `JustClose`;
   FirstRun with `over` existing and alone in its workspace → `Open { close_after: Some(over) }`; anything else →
   `Open { close_after: None }`. "Alone" = `model.terminal_tab(over)` → `model.locate(tab)` → its workspace, and
   `model.tabs_of(ws).len() == 1`.
6. **The launch is pure:** `fn start_launch(folder: &Path, home: Option<&Path>, choice: &Choice) -> Launch`;
   `Choice::{Preset(Preset), Plain}`; `project = (Some(folder) != home).then(folder)`; `run_preset` only for a preset.
7. **Detection is pure plus one background call:** `agent_detect::program(command) -> Option<String>`,
   `agent_detect::on_path(program, path_var, is_exec) -> bool` (`is_exec` injected for tests; the real one: a regular
   file with any execute bit, by `std::fs::metadata` + `PermissionsExt`), `Found::{Checking, Yes, No, Unknown}`, and
   `agent_detect::check(commands: Vec<String>) -> Vec<Found>` which calls `login_env()` (blocking, so only from
   `cx.background_spawn`) and returns all `Unknown` when it is `None` or has no `PATH`. The card spawns it on open;
   the answer goes to the card through its weak entity (a closed card drops it).
8. **The chosen row** is an index into the choosable rows (`Row::{Folder, Preset(usize), Plain}`), plus a `moved`
   flag set by ↑/↓ or a click; `default_row(found: &[Found]) -> Row` gives the start and post-detection choice.
9. **Keys on rows** (⌃⌘n) and the three shortcuts come from `help_shortcuts::keys_for(&table, action)` with `table`
   = `palette::binding_table(persistent)` (made `pub(crate)`; the same table the palette's rows and Help read), for
   `workspace::LaunchPreset{n+1}`, `workspace::OpenPalette`, `workspace::NewWorkspace`, `workspace::NextNeedingYou`.
10. **The scrim and centring** are drawn by the Workspace's render: a full-window absolute layer from `TOP_BAND` down,
    `occlude`, `on_mouse_down` → skip; the card in a flex box centred both ways inside that layer, with 16 pt padding.
    The top band above stays live.
11. **Focus:** the card has its own `FocusHandle` (`Focusable`), focused on open; `on_key_down` handles `up`, `down`,
    `enter`, `escape` with no modifiers and stops them; other keys propagate. After the folder panel answers (or is
    cancelled) the card focuses itself again if it is still open.
12. **Overlay plumbing:** `Overlay::Welcome` in `overlays!`; `close_overlays` drops `welcome` unless kept;
    `Overlays.welcome` in `pane_browser.rs` and `workspace_browser::overlays`; the `overlays` ui-shots scene gets an
    arm (`open_welcome(Origin::Help)`). Events from a card that isn't `self.welcome` are ignored (as the palette).
13. **Help menu:** `actions!(help, [ShowWelcome, …])`; `help_menu::menu()` items become: Welcome to fuxx, —,
    Keyboard Shortcuts, fuxx Command Reference, —, Notifications Status…, Show Tips Card, —, Report an Issue…, Show
    Logs; `cx.on_action(ShowWelcome)` → `with_help_window(.., open_welcome(Origin::Help))`. The menu test and the
    palette table (`palette_items::COMMANDS`) change with it.
14. **Copy (exact):** title `Welcome to fuxx`; subtitle `A terminal for running AI coding agents.`; headings `Pick a
    folder`, `Start an agent`, `Three shortcuts` (drawn uppercase); `Choose…`; `Plain terminal`; `Checking…`; `Not
    found on your PATH`; `No agent presets set.`; the three lines in the table above; hint `Enter start · ↑↓ choose ·
    Esc skip`; buttons `Skip`, `Start {name}`, `Open Terminal`.
15. **Existing screenshots that change:** `overlays` (new arm); `help` only if it checks the menu (it doesn't: the
    panel's list is bindings, unchanged). No other scene changes pixels: `for_shots` starts `welcomed = true`.
16. **Version:** no release of its own; it ships in the one release after M11l (your rule).

## Left to the builder

How `welcome.rs` splits its pure parts from the view; the exact hover and pressed shades within the existing
tokens; whether the scrim and card are drawn by the Workspace or by the card's own root (within the rules above);
how the agent list's scroll keeps the chosen row in view (`ScrollHandle::scroll_to_item` or a manual offset).
