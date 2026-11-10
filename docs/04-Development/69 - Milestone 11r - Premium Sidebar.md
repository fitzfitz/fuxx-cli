# Milestone 11r — Premium Sidebar

**Status:** 🟨 built on master 2026-11-10; waiting for your on-screen check (below).
**Asked for:** 2026-11-10, by you: "make it looks premium" → three rounds of browser mockups → "love the D and E
combined" → "no need filter below search and no need for lading prpgress" → "cool do it directly here on master".
**Order:** done **now, before M11p** (your call: "do it directly"). M11p (chrome caching) and M11q follow as planned.
**Plan:** [[M11r Plan - Premium Sidebar]].

## Intent

The sidebar should feel like a premium app: less boxed, more information per row, and status you can read at a
glance. The approved mockup is "D + E combined (v2)" (`.superpowers/brainstorm/…/sidebar-premium-de-v3.html`,
not committed): D's avatars, group tiles, search and New button; E's line under the name and the Open button on a
waiting row. The filter strip and the working progress bar from E were dropped by you.

## What you approved (from the mockup)

1. **An avatar per workspace:** a rounded square with its initials, coloured. Its status is a **badge** on the
   avatar's bottom-right corner.
2. **The line under the name** says what it is doing (see "AI's calls", 2), in a monospace font.
3. **A waiting row** keeps its orange tint and its question, and gets an **Open** button.
4. **Group headers:** a small letter tile, the group's name, its **branch** (moved up from the rows), the count,
   and the chevron at the right. **A collapsed group** shows small avatars of who's inside and "N waiting" when any
   of them wait.
5. **Group cards stay, but soft:** no border, rounder corners.
6. **"Jump to a workspace…"** at the top of the list (opens ⌘P), and a **"New Fuxxpace"** button at the bottom.
7. **No filter strip, no progress shimmer** (your call).

## AI's calls (details you left to me — say if any is wrong)

1. **Avatar colour:** the workspace's tab colour if you set one (so the separate colour dot goes); otherwise one of
   the ten tab colours (not grey), picked from its name, so it stays the same across restarts. Initials: the first
   letter of its first two words (split on space, `-`, `_`, `.`, `/`), or one letter for a one-word name.
   Colours are held to the contrast floors in every theme, like everything else (tested).
2. **The line under the name is the program's own title** (what Claude Code, vim etc. set as the terminal title),
   not the screen's last line. A TUI agent's last screen line is its input box, not what it's doing, and reading the
   screen would mean polling. The title is already known and arrives as an event, so it costs nothing extra. After
   M11q it will also say which command finished. Shown only when it says more than the name.
3. **Status badge:** a still dot in the status colour on the avatar's corner, in a ring of the row's colour (working:
   a soft halo; idle: dim while a program runs, fainter at the prompt). No animation (changed to match the mockup,
   below).
4. **Hovering a row** turns its avatar into the × that closes it (as the mark did before).
5. **Folder groups:** the rows inside drop their branch and folder lines (the header has both); a host group's rows
   keep their remote folder line. **Rows outside any group** keep their branch and folder lines.
6. **The branch on a header** is the **selected** workspace's when it is in the group, else the group's first
   one's; each workspace's branch (and folder) is read from its **focused terminal** (it was its first terminal).
   Your report, 2026-11-10: "if I have 2 active git in 2 workspace in group, the sidebar branch is not updated
   based on active cli". Still a button that opens the branch switcher, for that workspace's focused terminal.
7. **The New Fuxxpace button** opens the same menu as the old + (New Workspace ⌘N, in a folder ⌘O, presets).
8. **The selected row** is a neutral lift (changed to match the mockup, below; tested for contrast).
9. **Ages** stay as today: shown for every status but Idle, and redrawn only when the label changes.

## Matched to the mockup (2026-11-10, your report: "sekarang result dan mockup jauh berbeda")

The first build kept too much of the old sidebar. Compared side by side with the mockup, and changed:
1. **Every row has its line under the name**, in the terminal's font: the question, a note, the program's title, or
   (out of a group) the branch and the folder on **one** line, else its status in words ("Working…", "✓ Done",
   "Running", "Idle"). Every row is 52 pt.
2. **Avatars** are a gradient of their colour, lit at the top left, with pale letters (a bright yellow is washed back
   until its letters read: 4.5:1 on both ends, every theme).
3. **The selected row** is a neutral lift (`card_lift` in a card, `row_active` out of one), not the accent's tint.
4. **A waiting row** is edged in the waiting colour; **Open** is a filled button.
5. **Badges are still dots** (working with a soft halo), not the rings, spinner and ✓ (no animation in the rows).
6. **Groups are 10 pt apart**, as in the mockup.

Still different, on purpose: the status colours are the theme's (Mocha: yellow = working, red = needs you, green =
done), not the mockup's blue and orange; the line under the name is the title or the status, not the screen's last
line (see the AI's calls, 2); ⌘P (the key that exists) instead of ⌘K; the 📌 of a pinned row.

## Current folder and the header's folder icon (2026-11-10, your calls: "current folder info also good?" → "ya, kerjakan dan jga ganti avatar group title jadi ikon folder")

- In a folder's group, a row with nothing else to say (no question, note or title; not working) shows where its
  terminal is now, from the group's folder: `./` there, `./src/ui` under it, else the whole place (`~/…`). It follows
  `cd` as the folder is looked up (`relative_place`, `LineTwo::Here`). A working row still says "Working…".
- The group header's letter tile is a folder icon (open while the group is open), or a globe for an SSH host.

## Performance

No new timers, no polling. Avatars, tiles and badges are a few rectangles and text per row. The badge animations
are today's. The line under the name comes from the program's title, already tracked.

## Not in it

The filter strip, a progress shimmer, reading the screen's last line, new statuses, a light/dark-specific design
(the colours come from the theme as today), changes to drag and drop.

## As built (2026-11-10)

- `src/sidebar.rs`: `initials`, `avatar_color`, `avatar`, `Workspace::avatar_slot` (replaces the mark slot), `LineTwo::Title`,
  `WorkspaceFacts::title` / `WorkspaceRow::{title, folder_group}`, `HeaderRow::{branch, waiting, faces}`; the header,
  card, search button, footer and drag copy redrawn. Avatar 28 pt (radius 8), badge 14 pt (3 pt past the corner, the
  mark 10 pt), tile 18 pt, small avatars 14 pt overlapping 4 pt (placed, not negative margins), cards radius 12.
- `src/theme.rs`: `UiColors::avatars` (fill and ink per tab colour; ink ≥ 4.5:1 on its fill, in `drawn_pairs`).
- `src/ui_tokens.rs`: a one-line row is 42 pt (the avatar and 7 pt above and below it).
- Rows outside any group are inset like the rows in a card, so every avatar is in line.
- Tests: 1343 pass (new: initials, avatar colour, the title line, rows in a folder's group, header facts). ui-shots
  updated: rows in a folder's group are 42 pt; the selection scene reads the badges; the drag scene holds the copy
  past the long row's middle; the slide scene samples the search button.

## On-screen checklist (for you)

1. Rows have coloured avatars with initials; a workspace with a tab colour uses it.
2. Badges are dots: working (with a halo), waiting, done in their colours; idle dim or faint.
3. Hovering a row shows × on its avatar; clicking it closes the workspace.
4. Claude Code's title (what it's working on) shows under the name in monospace.
5. A waiting row is orange with its question and an Open button that takes you to the waiting terminal.
6. Group headers: folder icon (open/closed; globe for a host), name, branch (clickable → branch switcher), count, chevron at the right.
   With two repos in one group, selecting the other workspace (or focusing another terminal) changes the branch.
7. A collapsed group shows small avatars and "N waiting" when one waits.
8. Cards have no border; every row has one line under its name (in a group, idle or done: `./sub/folder` where its
   terminal is, following `cd`); rows inside a folder group show no branch/folder
   (the header does); rows outside show the branch and folder on one line.
9. "Jump to a workspace… ⌘P" at the top opens the workspace switcher.
10. "New Fuxxpace" at the bottom opens the New menu.
11. Dragging rows in, out and between groups still works and looks right.
12. Light themes (e.g. Latte) look right too.
