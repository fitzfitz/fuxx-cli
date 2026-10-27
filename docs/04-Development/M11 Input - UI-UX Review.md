---
tags: [development, input, ui]
---

> [!note] Input for [[40 - Roadmap]] M11 (UI/UX uplift), written 2026-10-27 by a reviewer agent from screenshots, the mockups and the code (it could not run the app). Nothing here is decided: M11 is planned with you when we reach it. You asked to keep M11 after M9 and M10, since those may change the layout.

# fuxx: UI/UX review and uplift plan (input for M11)

*Reviewer stance: product designer for developer tools. Sources: the owner's screenshots 2/4/7/8 (8 = current
main window with splits), the mockups in `mockups/`, and the code on `master` @ 15eea49. I could not run the app,
so every value below comes from the code. Contrast numbers are WCAG 2.x ratios computed from the real
`Theme::ui()` derivation (`src/theme.rs:101-119`) for each built-in theme (script and full table in the appendix).
GPUI claims were checked against the pinned source at `~/.cargo/git/checkouts/zed-a70e2ad075855582/1d2a4b3`.*

---

## 1. Verdict

fuxx works, and the engineering underneath is careful. Splits behave, the layout is restored, the IME works in the
rename box, and the UI colours follow the terminal theme. To look at, though, it is a 2019 developer prototype:
a grey system titlebar that clashes with every theme, a terminal grid that touches the window edges, and a sidebar
of same-size, same-weight rows where five of six rows have a filled background (8.png). Next to Ghostty
(edge-to-edge polish, padding, a themed titlebar), Warp (strong hierarchy, motion), Zed (dense, quiet chrome) or
cmux (a sidebar built for unread state), fuxx looks unfinished. That is mostly because it has **no visual
hierarchy and no design tokens**. Every surface uses `text_sm` (14 px) with `px_2`/`py_1`, and almost everything
is coloured `text_dim` or `row_active`.

The bigger problem is the product's core promise, "see which agent needs you at a glance". The UI doesn't deliver
it yet. A 10 px static dot, told apart only by hue, is the only signal. Two of its colour pairs are nearly
identical for colour-blind users. The light theme fails contrast on almost every element. And every unnamed
session, including its notification banner, is called "🖕", so even when a ring fires you can't tell which one it is.

What's good: the theme-derived chrome (the right instinct, it just needs better formulas), the cmd-1..9 /
ctrl-cmd-1..9 keyboard model, Ghostty-style split ergonomics (cmd-d, ⌥⌘-arrows, ⌘[ ⌘]), the bundled JetBrains Mono,
and a sidebar that already sorts by project. The bones are fine. The surface needs a system.

---

## 2. Findings, ranked by impact on someone who lives in fuxx all day

### F1. The status signal is too weak for the job the product exists to do (Critical)

- **Where:** `workspace.rs:949-957` `ring()`: `size(px(10.0))`; Idle is a `border_1()` in `ring_idle` (= `text_dim`).
  Working, Waiting and Done are solid fills of `palette[3]`, `palette[1]` and `palette[2]`. There's no motion, no
  shape difference and no row-level emphasis. The header repeats the same dot (`workspace.rs:855`).
- **Why:** You watch the terminal, not the sidebar. A 10 px dot in your peripheral vision is close to invisible,
  and hue is the only thing that changes. Measured problems:
  - **TokyoNight:** Working (#e0af68) and Done (#9ece6a) differ by a luminance ratio of 1.09. Under a deuteranopia
    simulation they are **7 RGB units apart**, which means identical. In Catppuccin Mocha the gap is 34 and in the
    default theme 38: barely separable.
  - **Waiting vs Done (red vs green)** is the classic colour-blind failure: distance 27 in Latte, 46 in TokyoNight.
  - **Contrast against the sidebar** (UI components need 3:1): in Catppuccin Latte, Working is **2.04** and Done
    **2.60**. In Gruvbox Dark, Waiting is **2.94**.
  - The rolled-up group ring is useful, but when a group is expanded it repeats the child's dot right above the child.
- **Fix:** status needs **shape + motion + row emphasis**, not just hue:
  - **Idle:** hollow circle, 1.5 px stroke, `text_dim` at 70 %. 10 px glyph in a 14×14 slot.
  - **Working:** open arc, 270° of a 10 px circle, 2 px stroke in the working colour, turning once every 1.2 s.
    Draw it with `PathBuilder::stroke(px(2.)).arc_to(..)` + `window.paint_path` inside a `canvas`. Both exist in
    the pinned GPUI (`path_builder.rs:88,155`, `window.rs:3853`). Drive it from a ~10 fps timer, not a 60 fps
    animation (see the battery caveat in §3).
  - **Waiting:** solid 10 px dot plus a 3 px halo at 30 % alpha. The halo pulses opacity 0.25 → 0.6 over 1.4 s with
    `Animation::new(1400ms).repeat().with_easing(pulsating_between(0.25, 0.6))` (`elements/animation.rs:36,275`).
    It is also the **only** state that tints the whole row: `ring_waiting` at 12 % alpha over the sidebar
    (8 % on light themes).
  - **Done:** solid 8 px dot. It's smaller than Waiting and has no halo, so it differs by size and halo even
    without colour. Optionally draw a 1.5 px check in the sidebar background colour inside it.
  - **Contrast floor:** every ring colour must reach at least 3:1 against the sidebar. Implement
    `ensure_contrast(c, against, 3.0)`, which mixes toward `fg` (or toward black on light themes) in 0.05 steps
    until the ratio is met.
  - **Tooltip on the ring:** "Waiting for you · 2 m". GPUI `.tooltip()` exists (`elements/div.rs:634`).
  - **Group headers:** show the rolled-up ring only when the group is **collapsed**. When it's expanded, show a
    count of urgent children instead, e.g. a red "2" pill.

### F2. Sessions can't be told apart: every default name is "🖕", including in notifications (Critical)

- **Where:** `groups.rs:7` `DEFAULT_NAME = "🖕"`. The row label is `format!("{}  {}", index + 1, label)`
  (`workspace.rs:935`). The window title is `"fuxx — {label}"` (`workspace.rs:961-966`). The notification summary
  is `format!("fuxx — {}", session.label())` (`workspace.rs:460`).
- **Why:** This is about function, not taste. In 8.png, five of six sessions read "N 🖕". The number is the only
  identifier, and it is a **global index that changes when you close or reorder sessions**. A background
  notification arrives as "fuxx — 🖕 / Needs your input". With four agents running, that tells you nothing.
  Also, at 14 px the emoji renders as a yellow blob about 12 px wide. It reads as "some emoji", not as a name.
- **Fix (keeps the owner's choice):**
  1. Quick: build notification and window titles from context, as `fuxx — 3 · api` (index + group) or
     `fuxx — 3 · api · Claude` when there is a preset. Show 🖕 only when there's nothing else.
  2. Quick: the preset name is already a fallback. Make sure presets always label their sessions ("claude",
     "codex"). Plain shells stay 🖕.
  3. Medium, **needs a plan change**: name an unnamed session after its PTY's foreground process (`zsh`, `npm`,
     `cargo`), the way iTerm2 and Ghostty title tabs. 🖕 stays the fallback and the brand glyph. The vault says
     "fuxx doesn't follow a shell's cd", so this is new behaviour. See open question Q1.

### F3. The system titlebar is the single biggest "amateur" tell (High)

- **Where:** `main.rs:173-176` `TitlebarOptions { title: Some("fuxx"), ..Default::default() }`, so
  `appears_transparent: false`. Same for Preferences (`preferences.rs:58-61`).
- **Why:** In 8.png the titlebar is the system's grey (~#23262b) above a Catppuccin #1e1e2e window and a #191926
  sidebar: three unrelated greys stacked up. It costs 28 px of height and repeats information the sidebar already
  shows. Ghostty (`macos-titlebar-style = tabs/hidden`), Warp, Zed, Arc and cmux all draw into the titlebar.
- **Fix:** `TitlebarOptions { title: Some(..), appears_transparent: true, traffic_light_position:
  Some(point(px(13.), px(12.))) }` and `app_owns_titlebar_drag: true`. This is the same setup Zed uses
  (`crates/zed/src/zed.rs:395-409`).
  - Let the **sidebar run to the top of the window**. Its first 38 px is a band holding the traffic lights on the
    left and a 22×22 "+" button (new session; hold or click its ▾ for presets) on the right.
  - The terminal area **also runs to the top**: no band and no centred title. The sidebar and the pane headers
    (F6) carry identity.
  - Mouse-down on the band calls `window.start_window_move()` (`window.rs:2385`). Double-click calls
    `window.titlebar_double_click()` (`window.rs:5557`), which honours the user's Zoom/Minimize preference.
  - In fullscreen (`window.is_fullscreen()`, `window.rs:2323`) the band shrinks to 8 px.
  - Keep calling `set_window_title`. Mission Control, the Window menu and cmd-` still use it. Prefix it with
    urgency, e.g. `fuxx — ● 2 waiting`.
  - **Caveat:** the content view now sits under the traffic lights, so test the hit-testing of the "+" button
    near them. Preferences can keep a native titlebar (macOS settings windows do) or get the same treatment for
    consistency.

### F4. The terminal grid has no padding and unbalanced leftover space (High)

- **Where:** `terminal_view.rs:670-675` `cell_origin` starts at `bounds.origin`. `dims_for`
  (`terminal_view.rs:627-632`) floors columns and rows, so the remainder (up to one cell minus 1 px) all collects
  on the right and bottom.
- **Why:** Text touches the sidebar divider and the top edge, and in splits it touches every divider. Ghostty
  defaults to 2 px with `window-padding-balance`. iTerm2 uses about 5 px. The original mockup had `padding: 6px 10px`.
  A grid with no padding is the second-biggest reason fuxx looks unfinished.
- **Fix:** inset the grid by **10 px horizontally and 8 px at the top**. Compute dims from the inset bounds. Then
  split the leftover between top and bottom: `pad_y = 8 + floor(rem_y / 2)`, and the same on x. Paint the
  background over the full bounds (already done at line 654) so the padding takes the theme background. In split
  panes use 8 px / 6 px. Make it a settings key with Ghostty's name, `window-padding-x/y`, if the owner wants it
  configurable (the settings file already uses Ghostty key names).

### F5. The sidebar's selection hierarchy is broken: "on screen" looks like "active" (High)

- **Where:** `workspace.rs:883` `on_screen`. Lines 907-909 fill `row_active` for the active row and `row_hover` for
  every other on-screen row. Measured contrast between those two fills: **1.15–1.21:1** across themes. The active
  label only changes `text_dim` → `text` (`workspace.rs:906`).
- **Why:** In 8.png rows 1, 2, 4, 5 and 6 are all filled pills. Only row 3 ("claude", not on screen) is bare, so it
  looks like the odd one out, the opposite of what's meant. You can't tell at a glance which row has keyboard focus.
- **Fix:**
  - **Active:** fill `mix(sidebar, fg, 0.14)` on dark, `0.10` on light; label in `fg`, weight 500; a **2 px accent
    bar** on the left edge, inset 6 px top and bottom, radius 1.
  - **On screen but not focused:** **no fill**. Label in `fg` at weight 400, plus a 4×4 rounded "pane" pip at the
    right edge in `text_dim`.
  - **Hover:** `mix(sidebar, fg, 0.06)`.
  - **Accent:** fuxx has no accent colour today. Add `UiColors.accent = ensure_contrast(palette[4], sidebar, 3.0)`
    (the theme's blue) and use it for the active bar, the rename border, drop indicators, the focused pane header
    and focus rings.

### F6. Split focus is nearly invisible and panes have no identity (High)

- **Where:** `panes.rs:63-72` lays a veil of `theme.background` at `a: 0.3` (`workspace.rs:969-973`) over unfocused
  panes. Dividers are 1 px of `sidebar_border` = `mix(bg, fg, 0.12)` (`panes.rs:97-103`), only **1.31–1.46:1**
  against the background.
- **Why:** A veil the colour of the background only changes *text*. Empty space looks the same. In 8.png, with
  mostly empty panes, the only focus cue is the solid versus hollow cursor. The ratio drops from 11.3 to 6.2 in
  Mocha, which is noticeable on dense agent output and invisible on an idle prompt. Panes also don't say which
  session they are, so with four agents in a 2×2 you have to look back at the sidebar. The mockup
  (`3-full-vision.html:83`) planned a 24 px pane header with a ring and a name. It was dropped.
- **Fix:** when a layout has more than one pane, give each pane a **22 px header**:
  - Contents: ring (8 px) · number · name (12 px, truncate) · on the right, the state's age (11 px, e.g. "4 m").
  - Background: `mix(bg, black, 0.08)`. Bottom border 1 px `mix(bg, fg, 0.10)`.
  - Focused header: text `fg` plus a **2 px accent top edge**. Unfocused: `text_dim`.
  - A Waiting pane's header is tinted `ring_waiting` at 15 %.
  - Raise the veil to `a: 0.4` (Ghostty's `unfocused-split-opacity` default is 0.7, i.e. 0.3; fuxx has a header
    on top, so 0.4 is fine).
  - Dividers: `mix(bg, fg, 0.16)`. On hover over the 6 px grab strip, and while dragging, show a 2 px accent line
    with a 120 ms fade.
  - No header for a single-pane layout, so you don't lose a row.

### F7. Light themes are broken: the UI derivation assumes dark (High, accessibility)

- **Where:** `theme.rs:105-117`. Sidebar is `mix(bg, black, 0.06)` on light themes, but `row_hover`, `row_active`,
  `text_dim` and `sidebar_border` are mixed from the **terminal** background, not the sidebar.
- **Measured, Catppuccin Latte** (sidebar #e1e3e6):
  - `text_dim` #8d90a1 on the sidebar: **2.46:1**. That fails AA (4.5) for every inactive label and every group
    header.
  - `row_hover` #e5e7ed is **lighter than the sidebar** (L 0.799 vs 0.767), so hover *brightens* the row
    (inverted).
  - `row_active` against the sidebar: **1.05:1**. The selected row is effectively invisible.
  - Rings: Working 2.04, Done 2.60 (both below 3:1).
- **Dark themes pass** (text_dim/sidebar 4.89–6.75). TokyoNight is closest to the edge at 4.89, and `text_dim`
  over a hover fill falls to 3.80 in Nord.
- **Fix:** derive every chrome colour **from the sidebar colour** and enforce contrast:
  - Dark sidebar: `mix(bg, black, 0.18)` (unchanged). Light sidebar: `mix(bg, fg, 0.04)`, i.e. darken *toward
    the theme's own ink*, not toward black.
  - `hover = mix(sb, fg, 0.06)`, `active = mix(sb, fg, dark ? 0.14 : 0.10)`, `border = mix(sb, fg, 0.12)`.
  - `text_dim = ensure_contrast(mix(sb, fg, 0.62), sb, 4.5)`. Latte needs t ≈ 0.85+, which the helper finds by
    itself.
  - Rings: `ensure_contrast(palette[n], sb, 3.0)`.
  - Add a unit test that loops over `BUILTIN_THEMES` and asserts those ratios. The pure `theme.rs` makes this a
    proper TDD job.

### F8. Sidebar layout: airy, unscrollable, fixed width, and long names wrap (High)

- **Where:** `workspace.rs:802-818`: `p_1`, `gap_1`, fixed `w(px(220.))` (`SIDEBAR_WIDTH`, line 71), **no
  `overflow_y_scroll`**. Rows (`895-936`) use `text_sm` = 14 px (rem 16) at GPUI's default line-height of 1.618,
  so about 22.6 px of line + 8 px padding + 4 px gap = **a 35 px pitch** (8.png measures 35 px). The label is a
  plain string child with **no `.truncate()`**.
- **Why:**
  - At the default 640 px window, about 17 rows fit, and the 18th session is **clipped and unreachable** by mouse.
  - Claude Code sets long OSC titles ("✳ Refactor the auth middleware…"). GPUI wraps text by default, so a long
    title turns its row into two lines and the list jumps.
  - Zed's project panel uses 24–26 px rows and Finder 24–28 px. At 35 px, ten agents need 350 px.
  - The number and the name share colour, size and weight, separated by two literal spaces. They're not tabular,
    so "9" and "10" misalign.
- **Fix (target row spec):**
  - **Row:** 28 px tall (`h(px(28.))`, `items_center`), radius 6, padding-x 8, gap 8.
  - **Ring slot:** 14×14.
  - **Number:** 11 px, `font_features` `tnum` (`styled.rs:714`), right-aligned in 16 px, colour `mix(sb, fg, 0.45)`.
  - **Label:** 13 px, `.truncate()` (`styled.rs:139`).
  - **Meta slot (optional):** 11 px dim, right-aligned (age, ⌘N hint on hover).
  - **Gap between rows:** 2 px.
  - **Grouped rows:** indent with **padding** (left 24 px), not `ml_4()` (line 903). The current margin shrinks the
    highlight pill, so grouped and ungrouped highlights no longer share a left edge.
  - **Scrolling:** `.id("sidebar").overflow_y_scroll()` with a `ScrollHandle`, and scroll the active row into view
    when it's selected by keyboard.
  - **Width:** default 240 px. Drag-resizable from 180 to 400 px using the same 6 px grab strip as the panes;
    persist it in the restore file. Add View → Toggle Sidebar (⌘\ or ⌘⇧B) for a full-width terminal.

### F9. Group headers have no hierarchy (Medium)

- **Where:** `workspace.rs:837-867`: same `text_sm`, `px_2`, `py_1` and `text_dim` as a row; ▸/▾ text glyphs at
  14 px; a `+` that is always visible.
- **Fix:**
  - Header 26 px tall with **10 px of space above every group except the first**.
  - Label 11.5 px, weight 600, colour `mix(sb, fg, 0.78)`, ellipsis on the left (`text_ellipsis_start`,
    `styled.rs:97`) so `(parent)` disambiguation survives.
  - Chevron as a 9 px glyph in `text_dim`, rotating 90° in 120 ms on toggle.
  - On the right: rolled-up ring only when collapsed (F1), then a session count in 11 px `tnum` dim. The `+` is a
    20×20 button that appears **on header hover only**, and on hover shows a tooltip "New session in api ⌘T".

### F10. Rename appends to the old name instead of replacing it (Medium, a usability bug)

- **Where:** `workspace.rs:634` seeds the box with `session.label()`. `name_input.rs:59-64` puts the caret at the
  end with no selection, and Backspace pops one character (`name_input.rs:77-81`). The box style is a
  `border_1` in `text_dim`, the sidebar background, `px_1` (`name_input.rs:259-266`).
- **Why:** Double-click a 🖕 row and type "api" and you get "🖕api". Every other macOS rename (Finder, Xcode,
  VS Code, Zed) selects all text first. The border is the same grey as the label, so the field barely reads as
  editing.
- **Fix:**
  - Open with a select-all state: a `replace_on_input: bool` that the first insert or Backspace honours. Paint the
    selection as a `mix(accent, sb, 0.6)` fill behind the text.
  - Border 1 px `accent`. Background `theme.background`. Radius 4. Padding-x 4.
  - Keep the row at **28 px**: size the input to the row's line box and use the same font size (13 px), so the
    border doesn't push the row taller.
  - Bonus: ⌘A, ⌥⌫ (delete word) and ←/→ caret movement. The caret is currently pinned to the end.

### F11. Drag-to-reorder gives weak, misleading feedback (Medium)

- **Where:** `DragPreview` (`workspace.rs:110-122`): a flat `row_active` pill with no shadow and no ring. The drop
  target gets `drag_over` → `row_active` (line 928), **the same fill as the active row**. The source row stays as
  it was. Dragging into another group is silently refused.
- **Fix:**
  - Preview: the full row (ring + number + name) at 95 % opacity, `shadow_md()` (`gpui_macros/src/styles.rs:441`),
    radius 6, fixed width = sidebar width − 16.
  - Source row: drop to `opacity(0.4)` while dragging.
  - Drop target: a **2 px accent insertion line** above or below the target row, depending on which half the
    pointer is in. No fill.
  - Refused targets: `cursor_not_allowed`, no line.

### F12. The Preferences window is functional but generic, with no preview (Medium)

- **Where:** `preferences.rs:24-31` sets 560×734, fixed. Two 180 px `uniform_list`s with 24 px rows. Section
  headings are `text_dim` at 14 px (`325-327`). It's a single column with `p_4 gap_2`. The background is the
  sidebar colour. Font names are drawn in the system font. There's no theme preview. The 28 px ± buttons
  (`345-357`) have no keyboard shortcuts.
- **Why:** Picking a theme by name from a plain list and checking the result in another window is 2010-era UX.
  Ghostty has `+list-themes` with live preview, and Warp and Zed show the theme in place.
- **Fix:**
  - Layout: a two-column macOS form, 720×560. Labels right-aligned in a 140 px column (13 px, `text_dim`), controls
    on the left, 20 px between sections.
  - **Theme list:** each row has a 4-chip swatch (bg, fg, palette 1, 2, 3, 4 as 10×10 squares, radius 2).
  - **Font list:** each name is rendered *in that font*.
  - **Live preview:** a 300×140 terminal-like box under the lists, painted with the existing `paint_grid` on a
    canned `GridSnapshot`. It shows a prompt, ligatures (`=> != ===`), a red/green diff line and all four ring
    states.
  - The size stepper gets ⌘+ / ⌘− / ⌘0, also bound in the main window (see F14).
  - Selection in the lists: an accent fill at 20 % plus a check mark, not just `row_active`.

### F13. No motion or feedback anywhere (Medium)

- **Where:** status changes, hover, collapse, pane focus and drag are all instant. The only animation in the app
  is none.
- **Why:** At the moment a session goes Waiting, nothing moves, so you notice it late. Motion is the
  cheapest way to catch peripheral vision. It's also what makes Warp and Zed feel finished.
- **Fix (budget: one moving thing at a time):**
  - **On a status change:** a 160 ms colour cross-fade on the ring. For entering **Waiting**, a one-shot 600 ms
    "ping" (halo scales 1 → 1.8 and fades out), then the steady halo pulse (F1).
  - **Hover fills:** 80 ms.
  - **Group collapse:** chevron rotation 120 ms. Don't animate list height.
  - **Pane focus change:** header accent fades in over 120 ms.
  - **Caveat (verified):** GPUI's animation honours `App::reduce_motion()` (`elements/animation.rs:161`), but
    nothing in `gpui_macos` reads the macOS "Reduce motion" setting. fuxx must read
    `NSWorkspace.sharedWorkspace.accessibilityDisplayShouldReduceMotion` at startup (objc) and call
    `cx.set_reduce_motion(true)`.

### F14. Gaps against macOS conventions (Medium)

- **Where:** `main.rs:139-166`. The app menu has only Quit. There's **no Edit menu** (the code comments at
  `main.rs:119` work around the missing Emoji & Symbols item), **no View menu** and **no Window menu**. There are
  no ⌘H / ⌥⌘H / ⌘M / ⌘` / font-zoom bindings (`main.rs:70-125`). The window opens at 1100×640
  (`main.rs:168`) and its frame isn't remembered.
- **Why:** ⌘H and ⌘M are muscle memory. ⌘+ / ⌘− are in every terminal (Ghostty, iTerm2, Terminal.app, Warp).
  Copy and Paste exist (⌘C/⌘V in the Terminal context), but they aren't discoverable in a menu.
- **Fix:**
  - **fuxx menu:** About fuxx · Preferences… ⌘, · Hide ⌘H · Hide Others ⌥⌘H · Show All · Quit.
  - **Edit:** Copy · Paste · Select All · Emoji & Symbols ⌃⌘Space.
  - **View:** Toggle Sidebar ⌘\ · Bigger ⌘+ · Smaller ⌘− · Actual Size ⌘0 · Enter Full Screen ⌃⌘F.
  - **Window:** Minimize ⌘M · Zoom · Bring All to Front.
  - Default window 1280×800, clamped to the display. Save the window frame in the restore file.

### F15. Keyboard-first gaps: no switcher, no "take me to what needs me" (Medium, close to signature)

- **Where:** switching is ⌘1–9 (global index, first nine only), ⌘{ / ⌘}, or the mouse.
- **Why:** With 10+ agents the question you ask fifty times a day is "who needs me?". fuxx has no one-key answer.
  cmux has ⌘⇧U ("jump to latest unread") and Zed and Warp have fuzzy switchers. "Quick-switch" is already P0 in
  `docs/01-Overview/11 - Features.md` (B), so this is in plan.
- **Fix:**
  1. **⌘⇧U, "Jump to session that needs you":** cycles Waiting sessions, oldest first, then unseen Done. If there
     are none, a 1.2 s toast: "Nothing needs you".
  2. **⌘P (or ⌘K), session switcher:** a 520 px wide overlay, 64 px from the top, shadow_xl, radius 10. It has a
     fuzzy filter over name, group and preset, rows of 32 px showing ring, name, group (dim) and age, **sorted by
     urgency then recency**, with Enter to switch. Not a full command palette (see §4).

### F16. First launch and empty states teach nothing (Low–Medium)

- **Where:** first launch shows one "1 🖕" row and a prompt. Presets (⌃⌘1–9), groups (⌘O), splits (⌘D) and rename
  (⌘R or double-click) have no visible entry points except the File menu.
- **Fix:**
  - **Sidebar footer:** 32 px tall, 1 px top border. On the left, a "+ New session" ghost button with a ▾ preset
    menu. On the right, a gear for Preferences. Both are 11.5 px.
  - **First-run hint card:** dismissible, inside the sidebar, 11.5 px, 3 lines:
    "⌘T new · ⌘O in a folder · ⌃⌘1 Claude", plus "Double-click to rename". It disappears after the first rename
    or 3 launches.

### F17. Terminal rendering details (Low; verify on screen)

- **Line height:** `LINE_HEIGHT_FACTOR = 1.3` (`terminal_view.rs:40`) is fine. JetBrains Mono's natural metrics
  are about 1.32. Keep it, but verify box-drawing joins: Claude Code's UI uses `─ │ ╭ ╮`, and fuxx has **no
  procedural box drawing** (none in `grid.rs`). Any line height above the font's natural metrics leaves vertical
  gaps in `│`. If gaps show at user-chosen fonts or sizes, draw U+2500–257F and U+2580–259F as quads, as Ghostty,
  Kitty and WezTerm do (M effort).
- **Cursor:** the focused block inverts its cell, the beam is 2 px and unfocused is a hollow box
  (`terminal_view.rs:656-748`). All good. Don't add blink by default.
- **Default size:** 13 pt JetBrains Mono (`fonts.rs:19`) is right for a 1280-wide window.

### F18. How well each theme is honoured (Low)

- **Good:** the chrome follows bg/fg/palette. TokyoNight's sidebar (#15161f) and Mocha's (#191926) look intended.
- **Not themed:** the titlebar (F3) and the Preferences window titlebar.
- **Ignored theme signals:** the cursor colour (Mocha's rosewater, a natural accent) and `selection-background`.
  F5's accent should prefer `palette[4]`, falling back to `cursor` when `palette[4]` fails contrast.
- **Gruvbox Dark:** the waiting red #cc241d is only 2.94:1 on its sidebar. The F7 helper fixes it.

---

## 3. The plan

Effort: S ≤ 1 day, M = 2–4 days, L = 1–2 weeks. The order inside each tier is the recommended build order.

### (a) Quick wins (each ≤ a day)

| # | Change | Finding | Effort | GPUI notes |
|---|---|---|---|---|
| Q1 | Contrast-enforced `UiColors` derived from the sidebar colour, plus an `accent` token and a test over all built-in themes | F7, F5, F18 | S | Pure `theme.rs`, TDD-able |
| Q2 | Terminal padding 10/8 px with balanced remainder (8/6 in splits) | F4 | S | `cell_origin` + `dims_for` only |
| Q3 | Row spec: 28 px rows, 13 px label `.truncate()`, 11 px `tnum` number column, padding indent instead of `ml_4` | F8 | S | `truncate` `styled.rs:139`, `font_features` `:714` |
| Q4 | Sidebar `overflow_y_scroll` + scroll active row into view | F8 | S | needs `.id()` on the sidebar |
| Q5 | Active row = fill + 2 px accent bar; on-screen rows lose their fill (pip instead) | F5 | S | |
| Q6 | Waiting row tint (12 % / 8 %) + ring contrast floor | F1 | S | |
| Q7 | Rename: select-all on open, accent border, fixed row height | F10 | S | `name_input.rs` |
| Q8 | Notification and window titles include number + group (+ preset) | F2 | S | |
| Q9 | Group header typography, count, hover-only `+`, rolled-up ring only when collapsed | F9 | S | |
| Q10 | Full macOS menus + ⌘+/⌘−/⌘0 + ⌘H/⌘M; default window 1280×800 | F14 | S | `Menu`/`MenuItem` already used |
| Q11 | Divider 1 px `mix(bg,fg,0.16)`, 2 px accent on hover/drag; veil 0.3 → 0.4 | F6 | S | |

### (b) Medium

| # | Change | Finding | Effort | GPUI notes (verified in the pinned source) |
|---|---|---|---|---|
| M1 | Integrated titlebar: transparent, sidebar runs to the top with a 38 px band, traffic lights at (13, 12), app-owned drag | F3 | M | `TitlebarOptions{appears_transparent, traffic_light_position}` (`platform.rs:1867`), `WindowOptions.app_owns_titlebar_drag` (`platform.rs:1706`), `start_window_move` (`window.rs:2385`), `titlebar_double_click` (`:5557`), `is_fullscreen` (`:2323`). Zed ships exactly this. |
| M2 | Shape-coded status glyphs (hollow / spinning arc / dot+halo / small dot) with motion and reduce-motion | F1, F13 | M | `PathBuilder::stroke` + `arc_to` + `paint_path`; `with_animation` + `pulsating_between`. **Battery caveat:** a GPUI animation requests a frame every vsync, so an always-spinning arc for "Working" (the usual state) redraws the window at 60–120 Hz all day. Drive the spinner from a 100 ms `cx.background_executor().timer` → `cx.notify()` (10 fps, like the existing 50 ms ticker), and keep real `Animation`s for short one-shots and the Waiting halo. Reduce-motion needs objc (F13). |
| M3 | Pane headers (22 px) for multi-pane layouts: ring, number, name, age, accent on focus, Waiting tint | F6 | M | plain divs in `panes.rs::render_node` |
| M4 | ⌘⇧U jump-to-needs-you + ⌘P urgency-sorted session switcher | F15 | M | overlay div + `uniform_list`; needs a NameInput-like filter field (reuse `name_input.rs`) |
| M5 | Preferences redesign: 2-column form, theme swatches, fonts in their own face, live preview via `paint_grid` on a canned snapshot | F12 | M | `uniform_list` already used |
| M6 | Resizable (180–400 px, persisted) and toggleable sidebar | F8 | M | same grab-strip pattern as `panes.rs` |
| M7 | Drag polish: shadowed row preview, 40 % source, insertion line, not-allowed cursor | F11 | S–M | `shadow_md`, `opacity`, `drag_over` |
| M8 | Sidebar footer (+ New ▾ presets, gear) and first-run hint card | F16 | S–M | |
| M9 | Seen/unseen semantics: Done stays "unseen" (solid) until the session has been focused for 1.5 s, then drops to "done, seen" (hollow ring with check, or plain Idle) | F1 | M | touches `status.rs` and the M2 detector: **plan change** (Q4) |

### (c) Ambitious "signature" ideas (what would make fuxx *the* tool for watching agents)

Ranked by value for "many agents, one human":

1. **"What is it asking?" subtitle (M–L).** When a session is Waiting, show the question line under the
   name in 11 px, one line, truncated: e.g. *"Do you want to make this edit to auth.rs?"*. The detector already
   reads `core.tail_text(TAIL_LINES)` and matches `WAITING_PATTERNS` (`status.rs:27-38`), so return the
   matching line. Rows grow from 28 → 42 px only while Waiting. This turns the sidebar from "something's red"
   into "here's the decision", and no other terminal does it. *Needs owner sign-off (Q5).*
2. **Hover peek (M).** Hover a row for 350 ms to get a 420×180 popover (shadow_lg, radius 8) showing that
   session's last 8 screen lines, painted with `paint_grid` at 11 px. You answer "is it done or stuck?" without
   switching. Same data source as #1.
3. **Attention queue + Dock (M–L).** Pin a "Needs you" section at the top of the sidebar: Waiting sessions, then
   unseen Done, each a 24 px row linking to the real row. It collapses to nothing when empty. Add a Dock badge
   with the waiting count (`NSApp.dockTile.badgeLabel`: **no GPUI API, needs objc**) and a Dock menu listing
   waiting sessions (`cx.set_dock_menu` exists, `app.rs:2282`). Pairs with ⌘⇧U.
4. **Time-in-state meta (S–M).** A right-aligned 11 px `tnum` age on every row: "4m" working, "32s" waiting.
   It's colour-free information that also helps colour-blind users, and it tells you which agent has been stuck
   longest.
5. **Overview grid (L).** ⌘⇧O tiles every session as a live mini-terminal (paint at 0.35 scale) with its ring and
   name, like Mission Control for agents. Click to go there. Visually distinctive, but expensive. Do it after #1–#4.

---

## 4. What NOT to do

- **No vibrancy or blur behind the terminal or sidebar.** `WindowBackgroundAppearance::Blurred` exists but is
  "not always supported" (`platform.rs:1953`), it lowers text contrast, and it fights every theme. A themed
  opaque window is what Ghostty and Zed ship.
- **No tab bar on top of the sidebar.** The sidebar *is* the tab strip (Features D, "or the sidebar list is the
  primary switcher"). Two navigation systems mean twice the attention cost.
- **No 60 fps "breathing" on every working row.** It looks alive for a minute, then it drains the battery and
  turns into noise. Motion is reserved for "needs you".
- **No separate UI theme system or theme marketplace.** Keep deriving chrome from the terminal theme. Just derive
  it properly (F7).
- **No icon font or icons on every row.** Status glyph + number + name is enough. Icons per agent type are a
  maybe only if presets get a single-letter monogram.
- **No full VS Code-style command palette** with every action. The switcher plus ⌘⇧U covers 95 % of the need.
  The menus hold the rest.
- **No onboarding wizard or modal.** One dismissible hint card at most.
- **No sound on status changes by default**, and no notification for the focused session (the current
  focus-aware rule is right).
- **Don't pull in the browser pane, SSH, or the activity feed as a separate panel yet.** The hover peek and the
  attention queue give most of the feed's value inside the sidebar (Scope note).
- **Don't add per-row settings, colour pickers or custom ring colours.** Contrast-enforced theme colours beat
  user tweaking.

---

## 5. Open questions for the owner (only the ones that change the plan)

1. **Naming (F2):** may an unnamed session show its foreground program (`claude`, `npm`, `zsh`) with 🖕 as the
   fallback? That needs process lookup on the PTY, which is new behaviour. Or should 🖕 stay the label for every
   unnamed session, with only notifications and titles gaining the number and group?
2. **Titlebar (F3/M1):** hide it fully (the sidebar runs under the traffic lights, terminal edge to edge, as in
   Warp and cmux), or keep a thin themed strip with a centred session title?
3. **Pane headers (F6/M3):** OK to spend 22 px per pane on a header with ring and name when split? It's the best
   fix for split focus, but it's chrome.
4. **Status model (M9):** may "Done" become "unseen until viewed", which changes the M2 detector's
   transitions? It decides whether green means "finished, go look" or "finished at some point".
5. **Sidebar subtitles and hover peek (signature #1, #2):** are these inside "run and watch AI agents"? I think
   they're the most on-mission features available, but they extend the sidebar beyond a list of rings.
6. **Motion budget (M2):** acceptable to redraw at about 10 fps while any session is Working, or should Working
   be static and only Waiting animate?
7. **Light themes:** is Catppuccin Latte a real target? If so, F7/Q1 becomes the first task. It's needed either
   way, but the urgency changes.

---

## Appendix: measured contrast (from `Theme::ui()` as written today)

| Theme | text_dim / sidebar | active vs on-screen fill | Waiting / Done / Working vs sidebar | Working↔Done deuteranopia distance |
|---|---|---|---|---|
| Default (#282c34) | 6.75 | 1.21 | 4.19 / 7.73 / 9.64 | 38 |
| Catppuccin Mocha | 5.23 | 1.17 | 7.51 / 11.69 / 13.68 | 34 |
| **Catppuccin Latte** | **2.46** ✗ | **1.09** (active vs sidebar **1.05**) | 4.22 / **2.60** ✗ / **2.04** ✗ | 53 (Waiting↔Done **27**) |
| Dracula | 6.48 | 1.22 | 5.03 / 11.51 / 14.13 | 40 |
| Gruvbox Dark | 5.29 | 1.17 | **2.94** ✗ / 5.20 / 6.49 | 34 |
| Nord | 5.03 | 1.17 | 3.47 / 6.97 / 9.10 | 46 |
| **TokyoNight** | 4.89 | 1.15 | 6.80 / 9.85 / 9.00 | **7** ✗ |

Unfocused-pane text contrast with the 0.3 veil: Mocha 11.34 → 6.24, default theme 14.0 → 7.67, Latte 7.06 → 3.45.
The deuteranopia distance is the Machado 2009 simulation (severity 1.0) followed by RGB Euclidean distance; under
about 40 is hard to tell apart at 10 px. Script: `/Users/fitzgeral/.claude/jobs/1f9e4e3f/tmp/scripts/contrast.py`.
