---
tags: [development, milestone, design]
status: built
---

# 64 - Milestone 11m - Title Bar (design)

**Status:: 🟨 Built, reviewed, merged and pushed 2026-11-09 (1259 tests; the full ui-shots set passes). The on-screen check is pending (20 items, below). Not released on its own: it ships in the one combined release (M11i–M11n) after M11n (your rule, 2026-11-09). Ships in v0.5.0 (the release after M11n; pending).**  ·  **Phase 4: an M11 extension.**  ·
**Plan:** [[M11m Plan - Title Bar]]
**Added to the plan 2026-11-09 by you** (see [[12 - Scope]], "Title bar"), as its own milestone after M11l
("own milestone after M11l"). After it comes M11n (performance), then the one combined release.

## Goal

The window's top band becomes a title bar. **Sidebar open:** the sidebar's band shows the traffic lights, the app
title **"the Fuxx?"** in its middle and the sidebar button at its right edge. **Sidebar collapsed:** the title goes,
the selected workspace's name sits in the middle of the window's band, and **[🔔] [⚙] [sidebar]** sit at its far
right. Nothing else about the window changes.

## Why (your words, 2026-11-09)

- "Can we make sidebar icon move across it and we put app title in the middle? The app title is "the Fuxx?". And
  when we collapse it the icon move to the very right and the app title hidden. Workspace name when sidebar
  collapsed should be put to middle and we put setting and notification icon beside sidebar icon. icon placement:
  [notification] [setting] [sidebar]."
- The summary you confirmed ("Yes, like this"):
  - **Sidebar open:** the sidebar toggle moves to the right edge of the sidebar's top band (beside the traffic
    lights); "the Fuxx?" (exact copy, with the question mark) is centred in that band; 🔔 and ⚙ stay in the sidebar
    footer, as today.
  - **Sidebar collapsed:** the title is hidden; the selected workspace's name is centred in the window's top band;
    **[🔔] [⚙] [sidebar]** sit at the far right of that band, in that order.
- You chose "own milestone after M11l". The program is autonomous, so **every detail not in the lines above is AI's
  call**, marked **(AI's call)** where it is made.

## What exists today

Read in the code on master `47c0550`:

- **The top band** (`ui_tokens::size::TOP_BAND` = 38 pt) is drawn by `Workspace::render_top_band`
  (`workspace.rs`). Pressing it and moving drags the window, and a double-click does what the macOS setting
  "Double-click a window's title bar to" says (`drag_window_by`: `start_window_move` on the first move after a
  press, `titlebar_double_click` on a double click). In full screen it is an empty 38 pt strip that moves nothing.
  The window has no system title bar (`appears_transparent`, `app_owns_titlebar_drag`), traffic lights at
  `TRAFFIC_LIGHTS` (13, 12).
- **Sidebar open:** the band is the **sidebar's** top (`render_sidebar` → `render_top_band("sidebar-band", None)`).
  The content column has **no band**: the pane tab bars are the window's top under the 6 pt drag strip
  (`render_drag_strip`), and the selected row already names the workspace.
- **Sidebar collapsed:** a band runs over the pane column (`"pane-band"`) with the workspace's name centred over
  that column (`title_area::render_workspace_name`, 13 pt semibold `text`, truncated, left out below `NAME_MIN` =
  60 pt). With the right sidebar open, the right sidebar has **its own** band above its card
  (`"right-sidebar-top"`), so the window's top is two bands side by side.
- **The sidebar button** (`sidebar_toggle.rs`) is **one element at the window's root**, at a fixed place in both
  states: x = 80 windowed (just right of the green light), 13 in full screen, hit area 24 pt (`HIT`), radius 6,
  `panel_hover` on hover, tooltip "Hide sidebar (⌘\\)" / "Show sidebar (⌘\\)". It stops the press from reaching
  the band, so it never drags the window. ⌘\\ and View → Toggle Sidebar do the same (`ToggleSidebar`). With motion
  on, the sidebar slides over 160 ms over the pane column (which takes its final width at once).
- **The footer** (`sidebar::render_footer`, 36 pt): + (menu), then 🔔 and ⚙ at the right, 28 pt buttons
  (`FOOTER_BUTTON`), 4 pt apart, 6 pt (`ROW_INSET`) from the sidebar's edges. The 🔔 has the tooltip
  "Notifications" and an accent dot ringed in `sidebar_background` while some are unseen; the **⚙ has no tooltip**
  and opens Preferences (`preferences::open_or_activate`, the same as ⌘,). **Collapsed, the footer goes with the
  sidebar, so there is no 🔔, no ⚙ and no unseen dot anywhere.**
- **The notifications popover** (`title_area.rs`) opens **upward** from the footer: bottom-left anchor just above
  the footer, right edge at the gear's, 228–340 pt wide (`popover_width`), its list up to 420 pt and kept under the
  top band (`list_room`), `snap_to_window_with_margin(8)`. Hiding the sidebar closes it (`toggle_sidebar`).
- **Keyboard and menus:** ⌘\\ (sidebar), ⌘, (Preferences), ⇧⌘U (next that needs you). Nothing opens the popover
  from the keyboard. None of this changes.
- **No minimum window size** is set.

## 1. Sidebar open

```
 ┌─ sidebar (240 pt) ──────────────────┐┌─ cards ──────────────────────────────
 │ ● ● ●          the Fuxx?        [▥] ││ 6 pt drag strip
 │                                     ││┌ tab ┬ tab ┐                 + ⋯ ▥ ┐
 │  ▾ ~/code/fuxx                      │││                                    │
 │  ▌● fuxx-cli               ⌘1       │││   terminal                         │
 │   ● web                    ⌘2       │││                                    │
 │                                     ││└────────────────────────────────────┘
 │ ─────────────────────────────────── │
 │ [+]                       [🔔] [⚙] ││
 ├─────────────────────────────────────┴┴──── status bar ──────────────────────
```

- **The sidebar button** sits at the right edge of the sidebar's band, its centre on the footer gear's centre
  (`open_button_x(width) = width − BAND_EDGE − HIT`, `BAND_EDGE` = 8 pt so that `BAND_EDGE + HIT/2 = ROW_INSET +
  FOOTER_BUTTON/2`) **(AI's call: the button and the gear line up vertically, top and bottom of the sidebar)**.
  Vertically centred in the band (y = 7), on the traffic lights' line, as today. Same look, hover and tooltip.
- **It lives in the sidebar's band**, so it slides with the sidebar **(AI's call)**: a hide slide carries it out
  with the sidebar and a show slide brings it in. No new animation.
- **The title "the Fuxx?"** (exact copy, lower-case "the", capital F, the question mark) is centred on the
  sidebar's middle (`width / 2`), vertically centred in the band.
  - **Look (AI's call):** `text::NAME` (13 pt), semibold, `text_dim`: the same size and weight as the workspace name
    in the collapsed band, but quieter, so the selected row stays the brightest thing in the sidebar. `text_dim` is
    already held to 4.5:1 on `sidebar_background` on every built-in theme.
  - **Never truncated (AI's call):** it is a fixed nine characters; a cut "the Fu…" reads worse than none. It is
    drawn whole or not at all.
  - **When the sidebar is too narrow (AI's call):** the title shows only while a `TITLE_ROOM` (64 pt) box centred
    on the sidebar's middle clears the traffic lights on the left (x ≥ `lights_clear` = 80 pt windowed, the line
    the sidebar button has kept since M11; 13 pt in full screen, where there are no lights) and stays `TITLE_GAP`
    (8 pt) short of the button on the right. Windowed, that is sidebars **224 pt and wider** (the default 240 pt
    shows it; 180–223 pt shows only the button). In full screen it shows at every width. The button never moves
    for the title.
  - It takes no clicks: the band under it still drags the window and a double-click still zooms.
- **🔔 and ⚙ stay in the footer**, unchanged, with one exception: the footer ⚙ gets the tooltip
  **"Preferences (⌘,)"** **(AI's call: the band's ⚙ needs one to match the 🔔 and the sidebar button, and the two
  copies of one button shouldn't differ)**.
- **The old place (x = 80) is empty band**: it drags like the rest.

## 2. Sidebar collapsed

```
 ┌─ window ───────────────────────────────────────────────────────────────────┐
 │ ● ● ●                       fuxx-cli                       [🔔] [⚙] [▥] │
 │┌ tab ┬ tab ┐                                       + ⋯ ▥ ┐                  │
 ││   terminal                                              │                  │
 │└─────────────────────────────────────────────────────────┘                  │
 ├──── status bar ────────────────────────────────────────────────────────────┤
```

- **The title is hidden.** It belongs to the sidebar's band and goes with it.
- **The three buttons** sit at the window's far right, in your order **[🔔] [⚙] [sidebar]**:
  - each a 24 pt (`HIT`) box, 4 pt (`BAND_GAP`) apart, y = 7, the sidebar button `BAND_EDGE` (8 pt) from the
    window's right edge, the same distance it had from the sidebar's edge when open **(AI's call)**:
    `band_buttons(W) = [W − 88, W − 60, W − 32]` (left edges);
  - **look (AI's call): the sidebar button's**, the band's existing button: the icon in `text_dim`, `text` and a
    `panel_hover` fill on hover, radius 6. The 🔔 and ⚙ use the footer's 16 pt line glyphs (`footer_icon`), so
    they are the same icons in a smaller box;
  - **tooltips (AI's call):** "Notifications", "Preferences (⌘,)", and the sidebar button's own "Show sidebar
    (⌘\\)";
  - **the 🔔 does exactly what the footer's does** (opens the popover, which sees everything); while the popover is
    open it stays lit on its `panel_hover` fill. **The ⚙ does exactly what the footer's does**
    (`preferences::open_or_activate`, as ⌘,). The sidebar button shows the sidebar.
  - **The unseen dot (AI's call):** the footer's dot (`unseen_dot`: 10 pt, accent, ringed in `sidebar_background`,
    which is the band's colour) at the band 🔔's top right, 1.5 pt in. **New:** until now there was no sign of
    unseen notifications with the sidebar hidden.
  - They are at the window's root and stop the press, so they never drag the window or zoom it.
  - **With the right sidebar open (AI's call):** the buttons stay at the window's far right, over the right
    sidebar's band (the same canvas colour, so the two bands read as one bar). The tab bar's own right-sidebar
    button is not touched.
- **The workspace's name** is centred on the **window** (not the pane column) **(AI's call: a title bar centres on
  the window, and with the right sidebar open the old pane-column centre sat off to the left)**. It is drawn at the
  window's root over both bands and takes no clicks, so the bands under it still drag and zoom.
  - Look as today: 13 pt semibold `text`, the same name the window title and the rows use.
  - **Narrow windows (AI's call: truncate, don't shift):** it keeps the same inset on both sides so it stays truly
    centred: `name_inset = max(lights_clear, 88) + NAME_GAP(12)` = **100 pt** (windowed and full screen alike),
    clear of the lights on the left and the three buttons on the right. It is cut with an ellipsis between the
    insets and left out when less than `NAME_MIN` (60 pt) remains: windows **under 260 pt** show no name.
- **What leaves the band:** the sidebar button at x = 80 (and x = 13 in full screen). The left of the band holds only
  the traffic lights.

## 3. Behaviour in both states

- **Toggling:** ⌘\\, View → Toggle Sidebar, or either sidebar button. The state flips at once: the band's three
  buttons appear (or go) on the click, the title rides out (or in) on the sliding sidebar. **No new animation
  (AI's call)**: the buttons don't glide across, the title doesn't fade. With motion off nothing moves at all. During
  a 160 ms hide slide the sliding sidebar still carries its own button while the band's three already show; a click
  on the sliding one turns the slide back, as today.
- **The popover from the band (AI's call):** with the sidebar collapsed, the 🔔's popover **drops down** from under
  the band: its top-right corner at (`W − BAND_EDGE`, `TOP_BAND + 4`), i.e. its right edge on the sidebar button's
  right edge and its top 4 pt under the band; width `(W − 16)` clamped to 228–340 pt; its list up to 420 pt and short
  enough that the panel ends 8 pt above the status bar (the rest scrolls); `snap_to_window_with_margin(8)` keeps it
  inside the window. Everything else (header, entries, closed terminals greyed, click to jump, Escape and a click
  outside close it, shortcuts that move the keyboard close it) is the popover as it is.
- **The popover closes on every toggle (AI's call):** today hiding the sidebar closes it; now showing it does too,
  since the 🔔 it hung from leaves the band. The keyboard goes back to the focused terminal.
- **Drag and double-click:** the band drags the window everywhere except on the buttons: the title, the name, the
  empty band and the old button place all drag; a double-click there does what the macOS setting says. In full
  screen nothing drags (as today); the buttons still work.
- **Full screen (AI's call):** the buttons keep their places (they never depended on the lights). Only the
  clearances change: the title's left clearance is 13 pt (it shows at every sidebar width); the name's inset stays
  100 pt (the right-hand buttons set it).
- **Light and dark:** only existing tokens (`text`, `text_dim`, `panel_hover`, `accent`, `sidebar_background`), all
  already held to their floors on every built-in theme. Checked in Catppuccin Mocha and Latte shots.
- **The Welcome card's scrim** dims the band (painted only) and so dims the band buttons with it; their clicks still
  land, as the band's do today.
- **Keyboard and menus:** unchanged. No new shortcut, no Help change, no palette change.
- **The macOS window title** (Mission Control, the Window menu) stays "N · name" (M11b): "the Fuxx?" is drawn, not
  set as the window's title **(AI's call)**.

## New design tokens

No new colours. Constants (in `sidebar_toggle.rs` and `title_area.rs`): `BAND_EDGE` 8.0, `BAND_GAP` 4.0,
`LIGHTS_CLEAR` 80.0 (windowed) / 13.0 (full screen; these replace `X_WINDOWED` / `X_FULL_SCREEN`), `TITLE`
`"the Fuxx?"`, `TITLE_ROOM` 64.0, `TITLE_GAP` 8.0, `PREFERENCES_TIP` `"Preferences (⌘,)"`. `NAME_GAP` 12.0 and
`NAME_MIN` 60.0 stay.

## AI's calls, in one place

1. The open-state button's centre lines up with the footer gear's (`BAND_EDGE` 8 pt); collapsed it keeps the same
   8 pt from the window's edge.
2. The title: 13 pt semibold `text_dim`, centred on the sidebar's middle, never truncated, hidden below 224 pt
   (windowed) so it never meets the lights or the button; the button never moves for it.
3. The open-state button and the title ride the sliding sidebar; no new animation anywhere.
4. The band buttons: 24 pt boxes 4 pt apart, the sidebar button's look and hover; the footer's 16 pt glyphs.
5. Tooltips "Notifications", "Preferences (⌘,)", and the footer ⚙ gets the same "Preferences (⌘,)".
6. The unseen dot shows on the band 🔔 (new: a sign of unseen notifications with the sidebar hidden).
7. The collapsed name is centred on the window, drawn over both bands; it truncates with an ellipsis at a fixed
   100 pt inset each side and hides under 260 pt; it never shifts off centre.
8. With the right sidebar open, the band buttons stay at the window's far right, over its band.
9. The popover drops down from under the band, right edge on the sidebar button's, kept inside the window.
10. Every toggle closes the popover.
11. Full screen: buttons in the same places, only the lights' clearance changes.
12. The macOS window title is unchanged.

## Not in M11m

- The + (new workspace menu) in the band when collapsed: you didn't ask; ⌘N, ⌘T and the palette still work.
- A title that does something (a menu, a click to rename) or a setting to hide or change it.
- Changing the macOS window title to "the Fuxx?".
- Any animation of the buttons moving or the title fading.
- Moving the pane tab bar's right-sidebar button, or putting anything in the band over the content when the sidebar
  is open (the tab bars stay the window's top there).
- A keyboard shortcut for the popover.
- Any other change to the footer (the +, 🔔 and ⚙ stay where they are), beyond the ⚙'s tooltip.
- A minimum window size (see Limits).
- M11n's performance work.

## Limits (known, accepted)

- **No minimum window size:** collapsed, a window narrower than about 170 pt lets the three buttons reach the
  traffic lights. Nobody works at that width; a minimum size would be a change of its own.
- **The lights' clearance is the M11 value (80 pt), not measured from macOS.** The sidebar button has sat at 80 pt,
  clear of the green light, since M11; if a later macOS draws the lights wider, `LIGHTS_CLEAR` is the one number to change.
- **`TITLE_ROOM` (64 pt) is checked against the UI font** in the ui-shots (the drawn ink must fit it); a different
  system font could need it adjusted.
- **Nothing is verified at 2x or in the real app.** The ui-shots ran at 1x (the external display); your on-screen
  checklist covers 2x and the real window.
- **The Latte hover fill is faint** (`panel_hover` is close to the band's colour). It was so before M11m and is left
  for your on-screen check (item 20).
- **Full screen can't be screenshotted** off screen: it is covered by unit tests on the geometry and by your check.

## On-screen checklist (pending, after the combined release)

1. **Sidebar open:** the sidebar's top band shows the traffic lights at the left, **"the Fuxx?"** in the middle and
   the sidebar icon at the band's right edge, straight above the footer's ⚙. Nothing sits right of the lights where
   the icon used to be.
2. Hover the sidebar icon: a soft fill and "Hide sidebar (⌘\\)". Click it: the sidebar slides out and the icon goes
   with it.
3. **Sidebar collapsed:** no "the Fuxx?"; the workspace's name is in the middle of the window's top band; at the far
   right **🔔 ⚙ and the sidebar icon**, in that order, the sidebar icon as far from the window's edge as it was from
   the sidebar's edge.
4. Click the sidebar icon at the right: the sidebar comes back, the three icons leave the band, the title is back.
5. ⌘\\ and View → Toggle Sidebar do the same, both ways.
6. **Dragging:** press and drag on the title (open), on the name (collapsed) and on empty band: the window moves.
   Press and drag starting on any of the icons: it doesn't.
7. **Double-click** the title, the name and empty band: the window zooms (or does what your macOS setting says).
   Double-clicking an icon doesn't.
8. Collapsed, let a notification arrive (an agent finishing in another workspace, or `fuxx notify hi`): the band's 🔔
   gets the accent dot.
9. Click the band's 🔔: the list drops down from under the band, its right edge lined up with the sidebar icon, inside
   the window; the dot goes. Click an entry: its terminal shows and the sidebar stays collapsed. Escape and a click
   outside close the list.
10. Open the list (from the band or from the footer), then toggle the sidebar any way (⌘\\, either icon): the list
    closes and typing goes to the terminal.
11. Collapsed, click ⚙: Preferences opens, as from the footer. Hover ⚙ and 🔔: a fill and the tooltips "Preferences
    (⌘,)" and "Notifications". The footer's ⚙ now shows "Preferences (⌘,)" too.
12. **Narrow sidebar:** drag the sidebar's edge to its narrowest: the title disappears (below about 224 pt) and the
    icon stays at the right edge; widen it again: the title comes back. The title never touches the lights or the
    icon.
13. **Narrow window, collapsed**, with a long workspace name: the name is cut with "…" and stays clear of the lights
    and the icons, still centred; narrower than about 260 pt, the name goes and the icons stay.
14. Collapsed with the right sidebar open (⌥⌘\\): the three icons sit at the window's far right above the right
    sidebar; the name is in the window's middle.
15. **Full screen** (the green button or ⌃⌘F), open and collapsed: the same layout; the title shows even with a
    narrow sidebar; the band doesn't move anything.
16. **Light and dark** (Latte, Mocha, one more of each): the title, the name and the icons are readable; hover fills
    and the dot are visible.
17. With the sidebar collapsed, Help → Welcome to fuxx: the band and its icons are dimmed with the rest. Click the
    band's 🔔: the card closes and the list opens (one overlay at a time).
18. **The name waits for the slide:** with the sidebar open, hide it (⌘\\) and watch the top band: the workspace's
    name does not show over the outgoing sidebar; it appears at once when the slide ends (about 0.16 s). With
    animations off it is simply there.
19. **A window narrower than the sidebar:** shrink the window to about 200 pt wide with the sidebar open: the
    sidebar icon is still inside the window and clickable (it follows the window's right edge). Collapsed, under about
    170 pt the three icons can reach the traffic lights (known, see Limits).
20. **Latte hover:** in Latte, hover the band's 🔔, ⚙ and sidebar icon: the fill is faint (pre-existing); say if it
    should be stronger.

## M11m as built (2026-11-09)

Where the build settled something the design left open, or changed it (AI's rulings after each task's review and the
final whole-branch review):

- **`TITLE_ROOM` stays 64 pt.** The title's ink measured 60 pt wide, so the 224 pt threshold holds. **`TITLE_INK_SHIFT`
  = −1 pt:** the title's ink centre sits 1 pt left of the sidebar's middle, measured; it is GPUI's text-origin
  snapping, not a centring bug. The shots hold the ink to that baseline (±0.5) and to ±1.5 of the middle as a sanity
  bound; rebaseline it if the system font or the GPUI rev changes. `TITLE_GAP` (8 pt) never binds at today's
  widths; it is the safeguard if `TITLE_ROOM` grows past 100.
- **The name waits for the hide slide (AI's call):** while the sidebar slides out, the collapsed name is not drawn;
  it appears at once when the slide ends. Before this it painted over the outgoing sidebar for up to 160 ms.
- **The band 🔔 closes the Welcome card**, as any overlay does (one overlay at a time). You accepted this.
- **The open button is clamped to the window** (`open_button_at(sidebar_w, window_w)`): a window narrower than the
  sidebar keeps it reachable, and the title's rule measures against the clamped button.
- **Both ⚙ show "Preferences (⌘,)"** and the 🔔 tooltip is one shared constant (`NOTIFICATIONS_TIP`); the band
  buttons share one button shell (`sidebar_toggle::band_button`).
- **Every sidebar toggle closes the popover**, and the keyboard returns to the terminal.
- **The popover drops down from the band 🔔:** right edge at W − 8, top at 42 (`TOP_BAND` + 4), width 228–340. The
  list's first entry starts at `band_entry_top`.
- **With the sidebar shown there are no band buttons:** a click where the band's sidebar slot would be is the tab
  bar's ◧ (tested).
- **The M11l `command-blocks-rprompt-tight` shot was fixed here.** It was a test bug, not a product bug: GPUI rounds
  the chip's left edge and its width separately, so the 0.5 pt tolerance failed at 1x. The tolerance is now 1 device
  pixel; a real overlap is a whole cell, so it is still caught. The RPROMPT stayed fully visible throughout.
- **M11m used its own build directory** (`CARGO_TARGET_DIR`), because the shared one served a stale binary from
  another worktree; the merge gate ran there.

## Links

[[40 - Roadmap]] · [[12 - Scope]] · [[M11m Plan - Title Bar]] · [[60 - Milestone 11i - UI Uplift 2]] (the band,
the footer, the canvas) · [[54 - Milestone 11b - Workspaces, Pane Tabs and In-Pane Browser]] (the name in the band,
the 🔔 and its popover) · [[53 - Milestone 11 - UI-UX Uplift]] (the sidebar button) ·
[[63 - Milestone 11l - Command Blocks]] · [[60 - Progress Log]]
