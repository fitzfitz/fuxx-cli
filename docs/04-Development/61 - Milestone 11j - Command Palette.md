---
tags: [development, milestone, design]
status: built
---

# 61 - Milestone 11j - Command Palette (design)

**Status:: 🟨 Built, reviewed, merged and pushed 2026-11-09 (1141 tests; every detail AI's call; you skipped the design questions). Not released on its own: one release after M11n. On-screen check pending. Ships in v0.5.0 (the release after M11n; pending).**  ·  **Phase 4: an M11 extension.**  ·  **Plan:** [[M11j Plan - Command Palette]]
**Added to the plan 2026-11-08 by you** (see [[12 - Scope]], "A modern layout, a command palette…"). Comes after
M11i and M11h (both merged), before M11k.

## Goal

One box for everything: **⌘K** opens a palette near the top of the window where you type a few letters and get
**actions** (anything a menu, a key or the ⋯ menu can do), **workspaces** (what ⌘P listed), **files** (in the
followed terminal's folder) and **settings** (each Preferences control), ranked by how well they match. Return does
it. The ⌘P switcher panel is gone: **⌘P now opens the same palette on its Workspaces scope**, so it behaves as it
did.

## Why (your words, 2026-11-08)

- From the UI uplift suggestions, which you all took: "I love all 1-11". Suggestion 9 was "a ⌘K palette for
  actions, workspaces, files and settings" ([[12 - Scope]]); the roadmap reads "one search for actions,
  workspaces, files and settings; replaces ⌘P".
- You skipped the design questions ("lewati pertanyaan", "pakai cara mu", "go ahead do them all autonomously"), so
  **every value, rule and layout choice below is AI's call**. The ones you may most want to overrule are marked
  **(AI's call)** where they're made and listed again in "Settled before planning".
- This reverses an older line: M11's review said "no full VS Code-style command palette" ([[M11 Input - UI-UX
  Review]] §4, [[53 - Milestone 11 - UI-UX Uplift]] "Not in"). Your 2026-11-08 choice supersedes it.

## What exists today

- **⌘P switcher** (`switcher.rs`, M11/M11b): a 520 pt panel 48 pt from the top (`top_panel`), the shared
  `overlay_panel::shell` look (`panel` fill, `panel_border`, radius 10, a soft shadow) and filter box (a
  `NameInput`). One row per workspace in **urgency order** (`urgency_order`: Waiting longest first, Done longest
  first, then Working and Idle by number), each with its most urgent terminal's mark, `#n`, name, tab-colour dot,
  group, age, and a waiting one's question on a second line (28 / 42 pt rows). The filter (`matches`) finds the
  number (`3`, `#3`) or the text anywhere in the name, group or question. ↑/↓ wrap, Enter goes, Esc or a click
  outside closes. Its rows' marks animate with the sidebar's ticker (`switcher_shown`).
- **Overlays** (`workspace.rs`): `overlays!` builds `Overlay` and `Overlay::ALL`; one open at a time
  (`close_overlays`); each hides every in-pane browser page while open (`Overlays::any` → `page_hidden`). The
  `overlays` ui-shots scene opens each one to check that.
- **Key bindings** (`main.rs` `key_bindings`, `new_bindings`, `persistent_bindings`) and the **menu bar**
  (`menu_bar`: fuxx, File, Edit, Pane, View, Window, Help). **Keyboard Shortcuts** (M11d, `help_shortcuts.rs`) is
  built from the binding table: `describe` names and groups every action (`ShortcutGroup`), `key_label` writes keys
  the Mac way, and the test `every_binding_is_listed_or_marked_unlisted` forces a decision for each new binding.
- **The ⋯ menu** (`pane_actions.rs`, M11i): New Terminal Tab, New Browser Tab, Split Right, Split Down.
- **Files** (M11g): the right sidebar lists the followed terminal's repo (or folder) lazily, folder by folder; file
  clicks open a read-only **file tab** (`open_file_tab(path, FileMode::Plain)`) in the focused pane. git runs
  through the capped runner (`git_branch::run_git*`, `GIT_CAP` 2 s).
- **Preferences** (`preferences.rs`): a window with four sections (General, Appearance, Sessions, Browser), each a
  list of `Control`s; it opens on the section last shown (`LastSection`).
- **⌘K** is bound to nothing (checked: `main.rs`, `keys.rs`, the terminal view).

## 1. Opening, closing, ⌘P

- **⌘K** opens the palette on the **All** scope. **⌘P** opens it on the **Workspaces** scope. File menu items:
  **Command Palette…** (⌘K) and **Go to Workspace…** (⌘P, was "Go to Session…"), then Go to What Needs You.
  **(AI's call: ⌘P is kept, re-pointed, not dropped.)** Your hands know ⌘P; it lands on exactly the old list (same
  order, same rows, same filter), so nothing is lost, and the palette really does replace the switcher panel.
- The sidebar row menu's **Show All Workspaces** opens the palette on Workspaces (it opened the switcher).
- **Pressing a palette key while it's open** (AI's call): the key of the scope it shows **closes** it (⌘K on All,
  ⌘P on Workspaces), like ⌘/ does the Help panel; the other key **switches scope** and keeps the typed text.
- It's an **overlay** like the others: opening it closes any other box (and the + menu); Esc, a click outside, or
  losing the keyboard closes it; while it's open every in-pane browser page is hidden. Closing without a pick gives
  the keyboard back to the focused tab (`focus_active`), as every overlay does.
- From a window that isn't fuxx's main one (Preferences, a popped-out page), ⌘K and ⌘P do nothing, as ⌘P today.

## 2. What it searches

### Actions (AI's call: what counts)

**Every command you can already give from the menu bar, a key binding or the ⋯ menu**, once each, minus the ones
that make no sense from a palette. Nothing new is invented: an action runs the very same GPUI action (or, for New
Browser Tab, the same method the ⋯ menu calls).

- Each has a **name** (verb first, title case, as the menu says it where there is a menu item), a **group** (the
  Help list's `ShortcutGroup`), **keywords** (a few extra words to find it by), **where it applies** (below), and its
  **keys**, read from the binding table and written as the Help list writes them (`key_label`; two keys joined
  ` / `), shown at the row's right. None when unbound.
- The full list is the table in the plan (Global Constraints). Names that would be ambiguous out of their menu get
  a word: View's Bigger / Smaller / Actual Size are **Bigger Text**, **Smaller Text**, **Actual Text Size**; the
  page's keys are **Reload Page**, **Page Back**, **Page Forward**, **Zoom Page In**, **Zoom Page Out**,
  **Page Actual Size**, **Go to Address Field**; Window's Zoom is **Zoom Window**; the scroll keys are **Scroll Up
  a Page** / **Scroll Down a Page**.
- **Agent presets:** one action per preset that has a key (the first nine), named **Start <preset name>** (keys
  ⌃⌘1…⌃⌘9), keywords `agent preset`.
- **Left out on purpose** (each named in the plan's `NOT_IN_PALETTE` list, so the parity test knows):
  - **Go to workspace 1–9** (⌘1–⌘9): the workspaces are rows of their own.
  - **Command Palette** and **Go to Workspace…**: you're already in it (Tab changes scope).
  - **Cut** and **Select All**: they do nothing in fuxx's window (only a popped-out page's web view answers them).
  - Keys that belong to a box or list (`name_input::*`, the host picker's, the Help panel's, the Files tree's, the
    address field's suggestions, the find bar's Esc, the menus' Dismiss), as in the Help list's `Unlisted`.
- **Where it applies** (AI's call): **Anywhere**, or only with a **terminal**, a **file tab** or a **browser tab**
  focused (Copy: a terminal or a file tab). It's decided from the **tab that had the focus when the palette
  opened**. Actions that don't apply there are **hidden**, not greyed (Find in File… only shows over a file tab;
  Reload Page only over a page). Quit fuxx and End All Sessions shows only while it's bound (persistent sessions on).
- **Parity is tested** (AI's call): every action in the binding table that the Help list shows, and every action
  item in the menu bar, is either a palette action or in `NOT_IN_PALETTE`. A new binding or menu item without a
  decision fails the test, as Help's does.

### Workspaces

Exactly the switcher's: one row per workspace, the same `Entry` facts (`switcher_entries`), the same row look, the
same number match (`3`, `#3` puts #3 first) and the same urgency order when nothing is typed. Picking one goes to its
most urgent terminal (its pane and tab), or shows a browser-only/file-only workspace, as Enter in ⌘P did. Terminals
inside a workspace aren't separate rows (AI's call, as ⌘P).

### Files (AI's call: what's covered and what a pick does)

- **The folder:** the **followed terminal's** (the status bar's terminal: the focused one, or the last one a browser
  or file tab showed, `right_sidebar.followed`), its folder as the status bar shows it (`Places`, else where it
  started). **Its root** is the repo's top folder, else that folder (`right_sidebar::find_root`), as the Files panel.
- **In a repo:** the files git would show: `git ls-files -z --cached --others --exclude-standard` (tracked and
  untracked, ignored left out), less `git ls-files -z --deleted` (gone from the disk), both through the capped
  runner. Paths are made NFC (`file_tree::nfc`) so a typed `ü` finds a decomposed name.
- **Outside a repo:** a walk of the folder: breadth first, `.git` and dot-folders skipped (the Files panel's rule
  outside a repo), symlinked folders not followed, at most **20,000** files, **12** levels and **2 s**.
- **At most 50,000 paths** are kept; past that a note says only the first 50,000 are searched.
- **Never on the UI thread:** the root lookup and the listing run on the background executor when the palette opens
  (or the scope turns to Files) and the window has no listing for that folder younger than **10 s**. Each window
  keeps its last listing per folder (in memory, for the run), so reopening shows files at once and refreshes behind.
  Ranking the paths also runs off the UI thread (they can be many), tagged with the query it answers; an answer for
  an older query or another folder is dropped.
- **SSH terminals:** no files (a note), as the Files panel. **No followed terminal:** a note.
- **Picking a file** opens it **read-only in a file tab** in the focused pane (`open_file_tab(path, Plain)`): the
  pane's tab for it if it has one, else a new tab, as a Files click does.

### Settings (AI's call: open Preferences, no toggling in place)

- One entry per **Preferences control** (16), named so it reads alone: Animations, Check for updates, Tips card;
  Theme, Font, Font size, Ligatures, Cursor style, Cursor glide, Cursor breathing glow, Cursor colour follows the
  agent, Cursor fun; Restore sessions, Keep running after quit; Show address bar, Keep history and data. Each shows
  its section at the right ("Appearance"), and is found by its settings-file key too (`cursor-glow`,
  `persistent-sessions`, …) and a few words (`dark`, `light`, `colours` for Theme; `trail`, `sparks` for Cursor fun).
- **Picking one opens Preferences on that control's section** (or brings it to the front, switched there). Why not
  toggle in place: two of the switches have side effects you should read first (Keep running after quit changes what
  Quit does; turning off Keep history and data deletes browser data), the theme and font want the live preview, and
  a second way to write settings is a second thing to get wrong. **No new settings** are added.
- The list is built from Preferences' own `Section::ALL` × `controls()`, so a control added later appears by itself
  (the Cursor heading isn't a control you set, so it's left out).

## 3. Scopes, typing and ranking

### Scopes (AI's call)

- Five scopes: **All · Actions · Workspaces · Files · Settings**, shown as small chips under the box. **Tab** moves
  to the next, **⇧Tab** to the previous (round), a click picks one. The typed text stays.
- **`>` prefix:** on All, a query starting with `>` searches Actions only (the Actions chip lights while it's
  there), as VS Code's. It's the only prefix; on other scopes `>` is just a character.
- Each scope has its own placeholder (exact copy in the plan): All "Search actions, workspaces, files and settings";
  Workspaces keeps the old "Go to a workspace — number, name, group or question".

### Matching (AI's call: a small in-house fuzzy matcher, no new crate)

A crate (nucleo, fuzzy-matcher, skim) would do more than fuxx needs; the lists are a few dozen items plus at most
50,000 paths ranked off the UI thread. A ~150-line matcher, fully tested, is enough and adds no dependency.

- Case-insensitive. The query is split on spaces; **every word** must match the item's main text as a
  **subsequence** (letters in order, gaps allowed). Words match independently, so `left pane` finds Focus Pane Left.
- Score of a word: the best placement (a small dynamic programme over query × text): **+16** per letter, **+24** when
  the letter starts a word (first letter, or after a space, `/`, `-`, `_`, `.`, `:`, `(`, or a lower→upper case
  change), **+16** when it follows the previous matched letter, **−1** per skipped letter between matches (at most −12
  per gap) and before the first (at most −8). The item's score is the sum over words, **+50** when its text starts
  with the whole query, **+1000** when it is the whole query.
- **Files:** the main text is the path from the root, and each matched letter inside the file's own name scores
  **+8** more, so `main` puts `src/main.rs` above `docs/maintenance.md` above `src/domain/mod.rs`.
- **The rest of an item** (a workspace's group and question; an action's keywords, group name and keys; a setting's
  section, key and words) is searched too, but only as plain text: if the main text doesn't match, the item still
  matches when every word is found somewhere in it, with score 1 (listed after every real match, nothing
  highlighted). Keys can be typed as symbols or words (`⌘d`, `cmd d`), as the Help list's search.
- **Workspace numbers:** a query that is a workspace's number (`3`, `#3`) puts it first (score 2000).
- **Order within a kind:** score, then **recently picked** first, then the kind's own order (workspaces by urgency,
  actions and settings by the table, files by shorter path then name).
- **Matched letters are drawn semibold** in the row's own text colour (AI's call: no colour, so nothing new to
  check for contrast).

### Recent (AI's call)

- Each window remembers the **last 20 things picked** from the palette (actions, workspaces, files, settings),
  newest first, in memory for the run. Nothing is saved to disk (no new settings, nothing in `sessions.json`).
- They show as a **Recent** section first when nothing is typed (at most 5; a workspace or file tab that has since
  gone, or an action that doesn't apply where you are, is skipped), and they break ties in ranking.

## 4. The list

### What shows (AI's call)

- **Nothing typed, All:** **Recent** (≤ 5), then **Workspaces** (≤ 5, urgency order), then **Actions** (≤ 5, table
  order). Each capped section ends with a **`… N more workspaces`** row; Return or a click on it switches to that
  scope. Files and Settings don't list on an empty All (thousands of files, a dozen settings to scroll past); the
  footer and the chips say they're there. An item shown under Recent isn't repeated in its own section (nor counted
  in its `… more`).
- **Typing, All:** no Recent section (recent picks rank higher instead); one section per kind that has matches,
  **sections ordered by their best match** (ties: Workspaces, Actions, Files, Settings), each **at most 5 rows** then
  `… N more <kind>`.
- **A narrowed scope:** every match of that kind (files: the best **200**, then `… N more files` as a plain note).
  Nothing typed: Recent of that kind (≤ 5), then all of it: Workspaces by urgency (exactly the old ⌘P list); Actions
  under their Help group headings (Workspaces, Tabs, Panes, Terminal, Browser pages, Window & app); Settings under
  their section headings; Files a note "Type to search files in fuxx-cli".
- **Section headings:** Recent, Workspaces, Actions, **Files in <root name>**, Settings.
- **Notes** (not selectable): `Listing files…` (first listing for this root), `Files aren't searched for SSH
  terminals.`, `No terminal folder to search.`, `Files couldn't be listed.` (git failed or hit its cap), `Only the
  first 50,000 files are searched.` (AI's call: they show in the Files scope; on All, Files appears only once it has
  matches, followed by the 50,000 note when that applies, so a listing or an SSH note never pushes real results down.)
- **Nothing at all matches:** `Nothing matches`.

### Rows

Every row: 8 pt side padding, corners 6 pt, 2 pt apart; a 14 pt icon slot (`MARK_SLOT`) then 8 pt; the name (13 pt,
`text`, truncated at its end); something faint at the right (11 pt).

| Kind | Height | Icon slot | Name | Right side |
|---|---|---|---|---|
| Workspace | 28 pt; 42 with a waiting question | its status mark (animated, as ⌘P) | `#n` (faint), the name, its colour dot, its group (faint) | its age |
| Action | 28 pt | empty (AI's call: names line up; no icon set to invent) | the name | its keys, `text_dim`, tabular |
| File | 28 pt | the file-tab page glyph tinted by type (M11i's colours, held at 3:1 on the palette's surfaces) | the file name, then 6 pt, its folder in `text_faint` (its start cut) | — |
| Setting | 28 pt | the gear glyph (the sidebar footer's), `text_dim` | the setting's name | its section, `text_faint` |
| `… N more` | 28 pt | empty | `… 3 more files`, `text_dim` | — |

- **The selected row** is filled `row_active` (as ⌘P) and its name is medium weight; a hovered row `panel_hover`.
- **Headings** look like the Help panel's (11 pt `text_faint`, 8 pt above, 2 above the first).

### Keyboard and mouse

- **↑ / ↓** move between selectable rows (items and `… more` rows), round from the last to the first, skipping
  headings and notes; the selected row is kept in view. **Return** does the selected row. **Esc** closes. **Tab /
  ⇧Tab** change scope. Everything else types into the box (⌘⌫ clears it, ⌘V pastes, as every box).
- **Typing** selects the first row. **While it's open** the workspaces' statuses and the files can change: the
  selection stays on its item if it's still there.
- **Mouse:** hover highlights, a click does the row, a click on a chip changes scope, the wheel scrolls, a click
  outside closes.

### Size and place (AI's call)

- **600 pt wide** (the switcher was 520, Help 560: room for a name, a folder and keys), less in a narrow window
  (16 pt from each edge, as today's panels); 48 pt from the window's top, centred (`top_panel`).
- The box, then the chip row (22 pt chips, 10 pt padding each side, 4 pt apart, corners 5, 11 pt; the current chip
  filled `row_active` in `text`, the others no fill in `text_dim`, `panel_hover` on hover), then the list, then a
  footer line over a 1 pt `panel_border`: `Enter open · ↑↓ choose · Tab scope · Esc close`.
- The list is at most **420 pt** tall, and never closer than 200 pt to the window's bottom (at least 120 pt).

## 5. Doing the pick

- **The palette closes first, then the keyboard goes back to the focused tab, then the pick runs** (AI's call). So an
  action goes exactly where its key would have gone: it's dispatched from the focused tab (`window.dispatch_action`
  in the pinned GPUI is deferred and starts from the window's focus at the time of the call, checked at rev
  1d2a4b3), so a terminal's Copy, a file tab's Find, a page's Reload and the app's own (Preferences, Quit,
  Keyboard Shortcuts) all land where their keys land. The palette never runs an action on itself.
- A **workspace** goes there (as Enter in ⌘P). A **file** opens in a file tab. A **setting** opens Preferences on its
  section. A **`… more`** row switches scope and stays open.
- Each pick (not a `… more` row) goes to the front of Recent.

## 6. Everything else it touches

- **Browser pages:** hidden while it's open (it's an overlay), shown again after.
- **File tabs:** decide which actions show (Find, Find Next, Find Previous, Copy); a picked file opens as one.
- **The right sidebar:** unchanged. The palette follows the same terminal it does, but needs it neither open nor on
  Files; it lists files itself and shares nothing with the Files tree (AI's call: the tree is lazy and folder by
  folder; the palette needs a flat list).
- **Animations / Reduce motion:** nothing in the palette moves except the workspaces' status marks, which follow the
  sidebar's rules (still with Animations off or Reduce motion on). It opens and closes without animation, as today.
- **Contrast:** the palette draws only on `panel`, `panel_hover` and `row_active`, with `text`, `text_dim` and
  `text_faint` (already held at 4.5:1 there on every built-in theme), the status marks (3:1) and the file-type tints,
  pushed to **3:1** on those three surfaces. A test checks every built-in theme.
- **Help panel parity:** **Command Palette…** (⌘K) is listed under "Window & app", **Go to Workspace…** (⌘P) under
  Workspaces (renamed from "Go to Session…"); the palette's own keys are "unlisted", as the switcher's were.
- **The scripting API, MCP and the CLI are unchanged.** Nothing here is scriptable.

## New design tokens

None in `ui_tokens` or `UiColors` (AI's call): the palette uses the overlay surfaces and the switcher's row sizes,
with its own constants in `palette.rs` (`PANEL_WIDTH` 600, `CHIP` 22, `LIST_MAX` 420, `SECTION_CAP` 5,
`FILE_RESULTS` 200). The file-type ink on the palette's surfaces is a small helper, not a token.

## Not in M11j

- Toggling settings or picking a theme or font **inside** the palette; new settings of any kind.
- New actions that no menu, key or ⋯ menu has today (switching right-sidebar panels, tab colours, pinning, the row
  and tab right-click menus' items).
- Terminals as rows of their own (only workspaces, as ⌘P); browser history or open pages as rows; searching file
  **contents**; files of SSH terminals; files outside the followed terminal's root.
- Saving Recent across launches; a palette in Preferences or popped-out page windows.
- Changing the first-run tips card (M11k's first-run screen teaches the shortcuts).
- Scripting the palette.

## M11j as built (2026-11-09)

Where the build settled something the plan left open, or changed it (AI's rulings after each task's review and the
final whole-branch review):

- **⌘P lands on exactly the old list**: on the Workspaces scope with nothing typed there is no Recent section (this
  note's §1 promise won over the Lists rule). Other scopes' empty lists show their heading even with no Recent, and
  leave Recent items out of the list below them.
- **Toggling:** ⌘K / ⌘P follow the chip you chose, so ⌘K with `>` typed on All closes. Choosing any scope while `>`
  is typed on All drops the `>` from the box.
- **⌘V pastes** in the box (a binding for `Palette > NameInput`, as the address bar has).
- **Enter never acts on an old answer:** while the files for what you typed are still being ranked, or a first
  listing is still running and you've typed something, Enter waits and then opens the best row of *that* query
  (your ↑/↓ choice is kept if it is still listed). Typing again or changing scope cancels the waiting Enter.
- **Files:** a folder inside a repo that is already listed reuses that listing (no new `git ls-files` on `cd`); a
  failed listing waits 2 → 4 → … 30 s before trying again; a listing that was turned away (another running) or a
  folder that moves while the palette is open is listed again. Outside a repo the walk skips, wherever it passes
  through your home folder, `Library`, `Desktop`, `Documents`, `Downloads`, `Pictures`, `Movies` and `Music`
  (so macOS doesn't ask for access out of the blue), and at `/` the system folders (`/Volumes`, `/System`, …).
  A cut-short walk says **"Only part of this folder is searched."** (not "first 50,000"). The folder followed is
  the terminal's own reported folder first. Recent files show while listed or open in a tab.
- **Matching:** an exact match scores EXACT on top of PREFIX; a quick subsequence check skips paths that can't match.
- **⌘W while the palette is open** closes the tab behind it and leaves the palette open.
- Known and parked: key hints are the Help list's even where a focused browser page takes the same key (⌘R shows
  on Rename Workspace and Reload Page); the list isn't virtualised (at most about 210 rows); a git submodule or a
  link to a folder is listed as a file (picking it shows the "too large or binary" note); a nested repo shows the
  outer repo's listing for up to 10 s; a click on a chip during an IME composition may leave the IME briefly out of
  step.

## On-screen checklist (pending, after the combined release)

1. ⌘K opens the palette near the top: the box, the chips All · Actions · Workspaces · Files · Settings, then Recent
   (once you've used it), Workspaces and Actions. Esc closes it and typing goes back to the terminal.
2. ⌘P opens it on Workspaces: every workspace, most urgent first, as the old ⌘P; type a name you gave one and Enter
   goes there; `3` puts workspace 3 first.
3. Type `split r`: Split Right shows ⌘D at its right; Enter splits the pane.
4. Type `>`: only actions are listed and the Actions chip lights.
5. Tab and ⇧Tab move along the chips; clicking a chip works; the typed text stays.
6. With a file tab focused, `find` lists Find in File…; Enter opens its find bar. With a terminal focused it isn't
   listed.
7. In a repo, type part of a file name: Files rows show the name (matched letters bolder) and its folder; Enter opens
   it read-only in a tab of the focused pane.
8. In a big repo (or a terminal in your home folder), typing stays smooth; `Listing files…` shows briefly the first
   time, then results.
9. With an SSH terminal followed, Files says they aren't searched for SSH terminals.
10. Type `glow`: Cursor breathing glow · Appearance; Enter opens Preferences on Appearance (or switches the open
    window to it).
11. A waiting workspace's row has its question under it; the marks turn and pulse, and stand still with Animations
    off or macOS Reduce motion on.
12. Over a browser tab, the page hides while the palette is open and comes back after.
13. ⌘K while it's open on All closes it; ⌘P while it's open on All switches to Workspaces.
14. Help → Keyboard Shortcuts lists Command Palette… ⌘K and Go to Workspace… ⌘P; the File menu has both.
15. Mouse: rows highlight on hover, a click does the row, the wheel scrolls, a click outside closes.
16. Reopen ⌘K after picking a few things: they're under Recent, newest first.
17. Right-click a sidebar row → Show All Workspaces opens the palette on Workspaces.
18. Light and dark themes (Catppuccin Latte and Mocha, plus one other of each): rows, chips, the selection and the
    file icons read clearly.
19. A narrow window (~600 pt): the palette fits with a margin, long names and folders are cut cleanly.

## Settled before planning (AI's calls after a builder's read, 2026-11-09)

Re-reading the design as the builder who must plan it, these were the places they would have had to guess:

1. **How an action is run.** Close the palette, `focus_active`, then `window.dispatch_action(action, cx)`. In the
   pinned GPUI it reads the focus at the call and dispatches on the next turn from that node of the rendered frame;
   the focused tab is in that frame (it was drawn under the palette), and app-level actions (`cx.on_action` in
   `main`) are reached after the tree, as from a key. New Browser Tab calls `new_browser_tab` on the focused pane,
   as `run_pane_action` does.
2. **Where an action applies** is a field of the table (`Place`), compared with the focused tab's kind at open
   (`model.focused_tab()` → terminal, browser or file). GPUI's `is_action_available_in` was considered and not used:
   it ignores app-level actions, and a pure table is testable.
3. **Keys on a row** come from the same binding table the Help list reads, formatted by one shared helper
   (`help_shortcuts::keys_for`, extracted from `shortcut_rows`), so the palette and Help never disagree.
4. **The action table is pure** (`palette_items.rs`): name, group, keywords, place, and how to run it (a function
   returning the `Box<dyn Action>`, or New Browser Tab). Parity is tested against `key_bindings()` +
   `new_bindings()` + `persistent_bindings()` and against the menu bar's action items (a new
   `main::menu_action_names()` walks `menu_bar`, submenus included).
5. **The switcher's fate:** `switcher.rs` keeps `Entry`, `urgency_order`, `next_needing_you`,
   `switcher_entries`, `jump_to_needing_you`, the shared row sizes the Help panel uses (`ROW`, `ROW_GAP`,
   `LIST_MAX_HEIGHT`) and its workspace-row drawing (moved to a free function the palette calls). The `Switcher`
   view, `SwitcherEvent`, `filter_rows` / `matches` and the `switcher::SelectPrev/Next` actions go.
   `Overlay::Switcher` → `Overlay::Palette`, `Workspace.switcher` → `palette`, `Overlays.switcher` → `palette`,
   `refresh_switcher` → `refresh_palette`, `switcher_shown` → `palette_shown` (the sidebar's ticker still sees the
   palette's marks). The action `OpenSwitcher` keeps its name (⌘P); a new `OpenPalette` (⌘K) joins `workspace::`.
6. **Palette keys** (`palette::SelectPrev`, `SelectNext`, `NextScope`, `PrevScope`) are bound to up, down, tab and
   shift-tab in a `Palette` key context, as the host picker's; Help marks `palette::` unlisted.
7. **The files' folder** is the one the status bar shows for the followed terminal: `places.folder(id)`, else the
   session's `project`; SSH → note. The root lookup (`find_root`) runs in the same background task as the listing.
8. **Freshness and one-at-a-time:** a window's `PaletteFiles` keeps, per folder asked, `{root, paths, more,
   listed_at}` (the root is only known once the lookup answers) and whether a listing is running; a new one starts
   only when none runs and that folder's list is missing or older than 10 s. An answer for a folder no longer
   followed is kept in the cache but not shown.
9. **Ranking files off the thread:** each query change spawns one background ranking; it carries a generation number
   and checks a shared counter every 1,024 paths, stopping when a newer query has started. The UI keeps the last
   answer until the new one lands. Workspaces, actions and settings are few and ranked on the UI thread.
10. **Selection identity:** every selectable row has a key (`Workspace(id)`, `Action(name)`, `Preset(n)`,
    `File(path)`, `Setting(name)`, `More(kind)`); refreshes keep the key, typing resets to the first row.
11. **Recent lives on the window's `Workspace`** (so it survives the palette closing), keyed the same way; at most
    20; shown only when still valid (workspace exists, action applies, file exists by the last listing or the open
    tabs, setting exists).
12. **Settings come from Preferences:** a pure `preferences::palette_settings()` builds the 16 entries from
    `Section::ALL` × `controls()` with a `Control::palette_name()` and keywords (`OnOff::key()` and the listed
    words). `preferences::open_at(section, cx)` sets `LastSection` and opens the window, or switches the open one
    there and brings it to the front.
13. **The `>` prefix** is parsed purely (`parse_query(scope, text) -> (Scope, query)`); the box's text keeps the `>`.
14. **Highlights:** match positions are character indices; a helper turns them into the byte ranges
    `StyledText::with_highlights` takes (semibold `HighlightStyle`), used in the pinned GPUI already
    (`status_bar.rs`).
15. **Existing screenshots that change:** the `switcher` scenes become palette scenes (the same eight-workspace
    fixture and expected orders); `tabs-switcher` opens ⌘P (now the palette); `overlays` opens `Overlay::Palette`;
    the `help` scene finds the two new rows. Key tests: `fuxx_keys_still_reach_fuxx_from_a_browser_tab` adds
    `cmd-k`.
16. **Version:** no release of its own; it ships in the one release after M11n (your rule).

## Left to the builder

The matcher's internals within the scoring rules above; the palette's internal split between `palette.rs` and its
pure modules; how the chip row and footer are drawn within the sizes given; the exact hover and pressed shades within
the existing tokens; whether the gear glyph is drawn at 12 pt or scaled from the footer's 16 pt drawing.
