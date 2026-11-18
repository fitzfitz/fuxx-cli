# Milestone 11al — UI Uplift 3: one floating card, roomier sections

**Status:** 📝 design written 2026-11-18 on branch `m11al-ui-uplift`; the user said 'go ahead with M11al' — direction
approved in mockups; plan next.

**Asked for:** 2026-11-17, by you: "let's brainstorm on the UI/UX uplift … What do you think we need to improve on the
layout part?" Then, after four structure options and their comfortable versions: "I don't like them all. I love the
current positioning, but each sections feels off and too compact". Then "more options" → "I love ours now the floating
panel on the middle and we need to merge the floating panel on the right with the middle. Left stays unfloating as the
layout is better to stay sticks" → option **1** ("1 is good, but still dont like how the tab presented") → tab style
**E, minimal** ("I like that minimal") → "looks right, do it right after M11ak".

**Mockups:** `.superpowers/brainstorm/34352-1794905233/content/` — `current-refined.html` (R), `floating-merged.html`
(option 1), `tab-styles.html` (E), `combined.html` (the chosen whole). Names in them are made up.

## The direction (yours)

- **Positions stay as today:** workspaces on the left, tabs and terminal in the middle, Files / Changes / Activity on
  the right.
- **Roomier ("comfortable") everywhere:** 13–14 px UI text, ~40–56 px rows and tabs, about double today's padding.
- **Left sidebar stays attached** to the window edge (not floating), on the darkest background.
- **The middle and the right panel are one floating card:** rounded corners, a soft shadow, inset from the window
  edges; inside it the terminal and the right panel sit side by side with a hairline between them (option 1). The
  right panel keeps its own Files / Changes / Activity switch, level with the tab bar.
- **Minimal tabs (E):** quiet text tabs, each with a small icon tile and a status dot, thin separators between them;
  the active tab gets a faint accent wash. `＋ ⋯ ▥` on the right.

## Per section (from R, the refined current layout)

- **Sidebar:** 56 px rows; 14 px names; the second line says who and where ("✳ Claude · api", "›_ zsh · my-app")
  instead of repeating the full path; group headings are quiet text, not cards; the selected row has a soft fill and
  an accent bar on its edge; a status dot on each avatar; ages top-right; a roomy "＋ New Fuxxpace ⌘N" button, 🔔, ⚙.
- **Terminal:** more inner padding (about 28 px sides, 22 px top). Its text size still follows your font setting.
- **Right panel:** Files / Changes / Activity as one segmented switch; a repo card ("my-app · 2 changed · 0 staged");
  Changes open when there are changes, with +/− per file; the branch footer with clear ⟳ and ⇡ buttons.

## Order

**Right after M11ak** (the agent box), before M11ag — your call. The agent box lives inside the card, so they should
match.

---

## Why

- You said each section "feels off and too compact". Today's rows are 42–52 px, tab bars 30 px, terminal padding
  16 / 12 px, UI text 11–13 px.
- Today every pane is its own small card (M11i), and the right sidebar is another card, with 6 px canvas gaps between
  them. It reads as many boxes. You asked for one surface for the work (terminal + right panel) and a plain, attached
  sidebar beside it.
- The pane tabs merge into the card from a darker strip (M11i). You didn't like how the tabs were presented; you
  picked quiet tabs (E).

## Goals

1. One floating card holds every pane and the right panel. Rounded, softly shadowed, inset from the window edges.
2. The left sidebar stays attached to the window's left edge, on the darkest colour (the canvas).
3. Minimal tabs (E) in every pane's tab bar.
4. Comfortable sizes everywhere the mockups show: sidebar, tab bar, terminal padding, right panel, status bar, agent
   box, the file tab's outer bar.
5. Works the same in light and dark themes, in splits (M8), with either sidebar hidden, in narrow windows and in full
   screen.
6. Nothing slower at idle (M11p).

## Non-goals and anti-goals

- **No feature changes.** Every click, key, menu, drag, tooltip and setting does what it does today. The one content
  change is the sidebar's line two (see 2.4), which the direction asks for.
- **No new settings.** A "compact / comfortable" density switch is **out** (AI's call). Reasons: you asked for
  comfortable everywhere; two size sets would double the ui-shots matrix; nobody asked for compact. The sizes stay
  named constants, so a switch can be added later if you ask.
- **Nothing slower at idle** (M11p). No new timers, no animation loops, no `request_animation_frame` outside the
  existing slides. The card is drawn by the Workspace (not a cached part), like today's cards. No new work in
  `render` beyond drawing.
- **Light themes must look right.** Every new surface is derived from the theme, never a fixed hex, and goes through
  the contrast floors in `ui_tokens`.
- **Don't redesign the file tab's inside.** M11ah (code view) is built in parallel on another branch. Only the file
  tab's outer bar (fill, hairline, side padding) changes here.
- **Don't change the terminal's font size or line height.** They follow your settings. The mockup's 1.8 line height is
  not adopted (AI's call: it is a font setting, not chrome).
- **No overlay redesign.** The palette, menus, popovers, Welcome and Preferences keep their look (they only pick up
  token changes they already read).
- **Don't change status meanings.** Chore 2 (note 82) holds: Done is a muted ✓, a closed group shows status only (no
  avatar squares), an exited agent's row goes back to a shell.

## Constraints

- **Tokens, never hex.** Every colour below maps to a `UiColors` field from `Theme::ui()` (`src/theme.rs`). New
  surfaces are mixed from the theme and added to the contrast lists there and to `drawn_pairs`.
- **M11p** (note 77): nothing visible changes inside `Workspace::render`; the sidebar, status bar, tab bars and title
  area stay cached parts (`chrome_cache`), each embedded at a definite size (so `TAB_BAR`, `STATUS_BAR` change in one
  place). No always-on timer. The marks' ticker stays the only continuous redraw.
- **GPUI's clip is a rectangle.** `overflow_hidden` doesn't follow rounded corners (note 60 §5). Anything with its own
  fill that touches a card corner must round its own corner.
- **The traffic lights are placed once,** at window creation (`main.rs`, `TRAFFIC_LIGHTS`). They can't move when the
  sidebar hides.
- **M11ah is in flight on another branch** and edits `src/file_tab.rs`. Keep M11al's edits there to the outer bar's
  few lines, and do them last, so the merge stays small.
- **The agent box's sizes** are the named constants at the top of `src/agent_box.rs` (M11ak left them there for this).
- **Repo rules** (AGENTS.md): stay on M11al; Context7 before any GPUI API you haven't used here; GUI claims need your
  on-screen check.

## Approaches

### Tokens

- **A. Edit the values in place.** Change the numbers in `ui_tokens::size` / `text` and the per-file constants; add a
  few new colour fields to `UiColors`. Small diff, no new concept. A later density switch would need a refactor.
- **B. One token set swapped in.** A `Metrics` struct (all sizes) with a `COMFORTABLE` instance, read everywhere
  through one accessor. Ready for a density switch. Touches every call site of every size, for a switch nobody asked
  for.
- **C. A, plus grouping.** Edit in place, but put the card's new sizes in one place (`ui_tokens::size`, a "card"
  block) and keep each file's local constants local.

**AI's pick: C.** It is A's small diff with the card's numbers easy to find. B is out with the density switch.

### The floating card

- **1. One wrapper element around the pane area and the right panel.** The Workspace draws one card (fill, radius,
  edge, shadow). Inside: the pane layout, then (when shown) the hairline and the right panel. Splits become hairlines
  inside the card. Panes stop being cards. The right panel moves from the root row into the card.
- **2. Keep per-pane frames, close the gaps.** Each pane and the right panel stay cards; gaps go to 0; only outer
  corners round, and shadows are turned off on inner sides. Fragile: every pane must know which card corners it
  owns, and shadows overlap at the seams.
- **3. A frame layer behind siblings.** The pane column and the right panel stay where they are in the root row; an
  absolute card (fill, shadow) is drawn behind both and an edge layer over both. Keeps today's right-sidebar slide
  code, but needs two extra layers that must track both widths each frame.

**AI's pick: 1.** One element owns the shape, the shadow and the clip; nothing has to agree on seams. The right
panel's slide becomes a slide inside the card's clip (2.5).

## The design

Colour names below: mockup variable → token. Mockup Mocha values are shown only to say which step is meant.

### 1. Window background (the canvas)

- The root fills with **`canvas`** — the left sidebar's colour, the darkest surface (mockup `#11111b`).
- **Change `sidebar_background` in place** (it already is the canvas): dark themes `mix(bg, black, 0.42)` (was 0.18);
  light themes `mix(bg, fg, 0.08)` (was 0.04). AI's call on the amounts; the test below decides the final step.
- **Text colours don't drift.** `text_dim` and `text_faint` are mixed from the sidebar colour today. Mix them from a
  fixed base (today's formula, kept as a local `base`) so overlays and text keep today's hierarchy; only surfaces get
  darker. AI's call.
- New **`raised`** = `mix(canvas, card, 0.5)` (mockup `#181825`): the sidebar's search box fill.
- Light themes: the canvas is the darkest surface, the card the lightest; the card's shadow does the rest.

### 2. The left sidebar

Attached at the window's left edge, full height above the status bar, on `canvas`. No edge line on its right (the
card's own edge and shadow separate them). Width: saved width kept; **the default for a new window goes 240 → 290**
(the mockup's; AI's call). Min 180, max 400 unchanged.

**2.1 Top band.** `TOP_BAND` 38 → **52**; `TRAFFIC_LIGHTS` (13, 12) → **(18, 20)** (centred in 52). The app title
("the Fuxx?", 13 px semibold, `text_dim`) centred; the sidebar button at the right, 15 px glyph, `text_faint`.

**2.2 Search ("Jump to a workspace… ⌘P").** `SEARCH` 30 → **38** tall, margin 4 / 14 / 10 (top / sides / bottom),
radius 10, fill `raised`, 1 px `card_border` edge, 12 px inner side padding, text 13.5 px `text_faint`. The ⌘P key
chip: 11.5 px, 1 px `card_border`, radius 6, 1 × 6 padding.

**2.3 Rows.**
- Height **56** for every row (`SIDEBAR_ROW` and `SIDEBAR_ROW_TWO` → 56; since M11r every row has line two). If
  `SIDEBAR_ROW_THREE` is then unused, it goes.
- Margin 2 px top/bottom, `ROW_INSET` 6 → **10** from the sidebar's sides; `ROW_PAD` 8 → **12**; radius 8 → **12**.
- **Avatar** 28 → **34**, radius 8 → **10**, letters 11 → **12 px bold**, gap to the text 10 → **12**. Colours: the
  existing `avatars` gradients.
- **Status badge on the avatar:** the existing badge (Chore 2 meanings: Working's live mark, Waiting's colour, Done's
  muted ✓ in `text_dim`, Idle unchanged), a **10 px** mark in a **2.5 px** ring of the row's own fill (canvas,
  `row_hover`, or `row_selected`), set 3 px past the avatar's bottom-right corner.
- **Name** 13 → **14 px semibold** `text`, one line, ellipsis. **Line two** 11 → **12.5 px** `text_faint`, 3 px under
  the name, ellipsis.
- **Age** top-right, **12 px** `text_faint`, level with the name's top (2 px down). Rows narrower than `AGE_MIN_ROW`
  leave it out (220 → **240**, AI's call, for the wider avatar).
- **Selected row:** fill `row_selected` (the existing accent tint, now over the darker canvas) plus a **3 px** accent
  bar (was 2) at the sidebar's very left edge, inset 12 px top and bottom, right corners rounded 3. Its badge ring
  takes the row's fill.
- Hover `row_hover`; a waiting row keeps `waiting_tint`; the hover-only colour dot, ⌘1–⌘9 hint, × and "Open" stay,
  sized to the row.

**2.4 Line two: "who · where"** (the content change the direction asks for).
- Precedence unchanged for: not live; Waiting (with its question or "Needs input"); a script's note; the program's own
  title when it says more than the name (`extra_title`).
- Where today's line shows **Place** (branch • folder), **Here** (the folder it is in now) or **State** words
  ("Cultivating…", "✓ Done", "Running", "Idle"), it shows **who · where** instead:
  - *who*: the agent in front, as its provider glyph and name ("✳ Claude", "A agy"); else a program in front
    ("› npm"); else the shell ("›_ zsh").
  - *where*: the last part of the folder it is in now ("api", "my-app"); an SSH row: its host.
- The branch leaves the row. It stays in the group header, the status bar and the right panel's footer.
- The status words leave line two: the badge already says the status. AI's call, flagged as question 1.
- Data: the program's name is already looked up (`procs::ProgramState::Running(name)`) and the agent from
  `workspace_agent`; the facts gain a *who*. No new lookups, no new timers.

**2.5 Group headings: quiet text, not cards.** This replaces the group cards (your 2026-11-10 call "make it cool so I
can tell the workspaces are grouped"; R replaced them and you approved R). Group `CARD_PAD` / `CARD_RADIUS` go.
- `GROUP_HEADER` 28 → **36**: 16 px above, 8 px below the words, 20 px from the sidebar's sides.
- Folder glyph (the existing one), the name **uppercase, 12 px semibold, 0.04 em letter spacing**, `text_faint`; the
  count at the right (12 px, regular, no spacing); the chevron after it. Uppercasing the name is the mockup's; AI's
  call.
- The letter tile (`TILE`) goes; the folder glyph takes its place.
- Collapsed: status only, as Chore 2 ("N waiting", else Working's live mark, else the muted ✓, else nothing).
- The hover-only ＋ (new workspace in the group) stays.

**2.6 Footer.** `FOOTER` 36 → **62**: 1 px `card_border` line on top, 12 / 14 padding. "＋ New Fuxxpace ⌘N" button
fills the width left of the icons: **38** tall, radius 10, 1 px `card_border`, 13.5 px `text_dim`, the key chip as
2.2. 🔔 and ⚙: 15 px glyphs in `FOOTER_BUTTON` 28 → **34** hit boxes, gap 12. The unseen dot unchanged.

**2.7 Sidebar behaviour.** ⌘B hide/show slide unchanged (it slides over the canvas; the card already has its final
place). Resize edge on its right unchanged. Narrow sidebar (180): ages drop first (2.3), names ellipsize.

### 3. The card

**3.1 Shape.**
- Fill **`card`** (the terminal background, mockup `#1e1e2e`).
- `CARD_RADIUS` 8 → **14**.
- Edge 1 px **`card_border`**, drawn over the content (as `panes::card` does today).
- Shadow: **0, 8 px down, 28 px blur**, `shadow` at **`card_shadow_alpha`**: dark 0.30 → **0.45**, light 0.08 →
  **0.12** (AI's call on light).
- Inset — new **`CARD_INSET` = 8** (replaces `CARD_GAP` 6): 8 from the window's right edge, 8 above the status bar
  (or the window's bottom), 8 below the window's top (the drag strip, as today's 6). Its left edge sits **flush
  against the sidebar** (the mockup's margin is 0 there); with the sidebar hidden, 8 from the window's left edge.

**3.2 Inside.** One row: the pane layout (`flex_1`), then, when the right panel shows, a 1 px **`card_border`**
hairline and the right panel at its drawn width.

**3.3 Splits (M8) inside the card.**
- Panes stop being cards. A split's 6 px canvas gap becomes a **1 px `card_border` hairline**.
- Its grab strip stays **6 px wide**, now centred on the hairline (`grab_shift` / `accent_offset` reworked for a 1 px
  line). Hovered or dragged: the 2 px `resize_line` in its middle, as today.
- **Focus:** no per-pane ring (a ring inside a card reads as a nested box). Unfocused panes keep the 0.4 veil
  (`UNFOCUSED_VEIL`). The focused pane's active tab has the accent wash (`tab_active`); an unfocused pane's active
  tab has a neutral wash (`card_hover`). AI's call.
- A lower pane's tab bar sits directly under a horizontal hairline.

**3.4 Corners.**
- The pane tab bars have no fill now (4.1), so they need no rounding.
- The terminal already paints its bottom corners rounded at `CARD_RADIUS`. The terminal background equals the card,
  so rounding an inner pane's corner shows the same colour. Keep it as is, at the new radius.
- The right panel's fill differs from the card: it rounds its **top-right and bottom-right** corners
  (`CARD_RADIUS − 1`).
- Builder: check every tab view that paints its own background to the bottom (file, browser, agent tab) for the same
  rule. They read `CARD_RADIUS`; the agent tab doesn't today; check it.

**3.5 Shadow and clipping.** Today the pane column clips (`overflow_hidden`) so card shadows show in the gaps. The
new shadow reaches ~36 px below the card. It must show at least in the 8 px under the card, and should fall across
the status bar (3.6). The column's clip may move to the root. Nothing of the card may poke past the window's edge.
Left to the builder how.

**3.6 Status bar under the card.** The status bar draws **no fill of its own** (the root's canvas shows), so the card's
shadow can fall over it. AI's call.

**3.7 Window drag.** The 8 px strip above the card still drags the window (`render_drag_strip`), not in full screen.
The empty part of a top tab bar still drags it. The right panel's header padding drags it too (6.2).

### 4. Tab bar (E, minimal)

**4.1 The bar.** `TAB_BAR` 30 → **52**, in every pane, splits included (so the top bars are level with the right
panel's header). No fill: the card shows through (the `tab_bar` strip colour retires for pane bars). A 1 px
`card_border` line along its bottom. `TAB_TOP_GAP` goes: tabs are centred.

**4.2 Tabs.**
- New `TAB` height **34** (9 px above and below in the bar), radius **8** on all corners (they float; they no longer
  merge into the card).
- `TABS_LEFT` 6 → **12**; padding 12 px each side; 8 px between parts; **4 px** between a tab and a separator.
- Width: `TAB_MIN_WIDTH` 90 → **100**, `TAB_MAX_WIDTH` 200 → **220**. Too many still shrink, then scroll.
- Text **13.5 px**. Inactive `text_faint` (the mockup's dim); active `text`.
- **Active:** fill **`tab_active`** (new: the accent at 10 % over the card on dark, 8 % on light, lowered in 2 % steps
  while `text` and the marks would fall below their floors — like `row_selected`). In an unfocused split pane:
  `card_hover` (3.3).
- **Hover** (inactive): `card_hover`. A waiting tab not shown keeps its `waiting_tint` (and `waiting_hover`).
- **Separators:** 1 × 16 px `card_border` between every two tabs (as the mockup, also beside the active one).
- Preview tab: italic, as M11t.

**4.3 The icon tile.**
- Every tab has one: **20 × 20**, radius 6, fill **`card_lift`** (mockup `#313244`), glyph 11.5 px in the middle.
- Glyph colour by kind: an agent's provider glyph in its provider colour; a terminal's `›_` in `accent`; a browser's
  globe in `accent`; a file's page in `text_dim`. AI's call (agent badges today are their own coloured squares; the
  tile makes all kinds read alike).
- The status mark **no longer replaces the icon** (today a busy terminal shows its mark in the icon's place).

**4.4 The status dot.**
- After the title, **8 px**: the existing marks at a smaller size, so shape and motion still carry the state.
- Working: the turning arc in `ring_working`. Waiting: the breathing dot in `ring_waiting`. **Done: the muted ✓**
  (`text_dim`), the sidebar's Chore 2 look (tabs kept the green check then; AI's call, flagged as question 3). Idle:
  nothing.
- Agent tabs already show their mark after the title (`AGENT_MARK` view); terminal tabs move to the same place. Both
  stay their own mark views, ticked by the shared clock (M11p).

**4.5 Close.** × at the tab's end, 16 px, shown on hover (as today), after the status dot.

**4.6 Buttons ＋ ⋯ ▥.** Pinned right. Hit box `BUTTON` 20 → **28**, glyph `ICON` 12 → **16**, `text_faint`, hover
fill `card_hover`; `BUTTON_GAP` 2 → **4**; `BUTTONS_PAD` 4 → **10**. ▥ lit while the right panel is open; ⋯ while its
menu is (unchanged).

**4.7 Drag chip.** The tab's new look: 34 tall, radius 8, `panel` fill, `panel_border` edge, shadow (as today).

### 5. Terminal padding

- `TERMINAL_PADDING` (16, 12) → **(28, 22)**: 28 px sides, 22 px top and bottom.
- `SPLIT_TERMINAL_PADDING` (12, 10) → **(20, 14)** (AI's call; splits stay a bit tighter, as today).
- Both stay larger than `CARD_RADIUS` (14), so no cell paints into a rounded corner.
- Font size and line height follow your settings (unchanged).
- The command blocks' gutter bars (M11l) sit in the left padding; they keep their offset from the grid, not the edge.

### 6. Right panel (inside the card)

**6.1 Place and size.** Right of the hairline, the full height of the card. Width as saved (default 300 = the
mockup's). Fill new **`side`** = `mix(card, canvas, 0.33)` (mockup `#1a1a29`; a step below the card, both modes).

**6.2 Header — the segmented switch.** **52** tall (`TAB_BAR`), level with the tab bars; the same 1 px `card_border`
line along its bottom, so one line runs across the card.
- Track: 14 px from the panel's sides, centred; fill **`canvas`**, 1 px `card_border`, radius 10, 3 px padding.
- Three equal segments, ~28 tall, radius 8, 13 px. Inactive `text_faint`, hover `text_dim`. Active: fill
  **`card_lift`**, `text`, semibold.
- The header's padding (outside the track) drags the window when at the top (not in full screen).

**6.3 Repo card** (the Files root header and the Changes header, restyled; same content and clicks).
- Margin 4 / 14 / 6, padding 12 / 14, radius 12, fill **`card_hover`** (AI's call; builder may mix a `side`-based lift
  if contrast or hover clashes ask for it).
- Glyph, name **14 px semibold** `text`; meta at the right ("2 changed · 0 staged", "N files") **12.5 px**
  `text_faint`. Changes' progress bar and totals keep their place under it.

**6.4 Section headings** (STAGED, CHANGES, GRAPH, Activity's): 16 / 20 / 8 padding (top / sides / bottom), **12 px
semibold uppercase**, 0.04 em spacing, `text_faint`; the chevron; a count pill at the right: `card_lift` fill,
`text_dim`, radius 10, 1 × 8 padding. Open/closed behaviour unchanged.

**6.5 Rows** (Files tree, Changes, Activity): `ROW` 28 → **32**; pill inset `PILL_INSET` 6 → **10**, `PILL_PAD` 6 →
**12**, `PILL_RADIUS` 7 → **9**; name **13.5 px**; folder **12 px** `text_faint`; mark letter 12 px mono semibold in
its existing colour; counts 12 px mono (`count_inks`). Hover / selected: a hover and a selected tint mixed from
`side` (as `card_hover` / `card_selected` are from the card), added to the contrast lists. The tree's indent guides
unchanged (`guide`, re-checked on `side`). The filter field: 34 tall, radius 10, `raised`-like fill on `side` (builder
picks; must pass the floors).

**6.6 Branch footer** (M11v bar). 1 px `card_border` line on top; padding `BAR_PAD_X` 12 → **16**, `BAR_PAD_Y` 8 →
**14**. Branch glyph 16 px in its existing colour; branch name semibold `text`; under it "↑1 ahead of origin"
12.5 px `text_faint`. Buttons ⟳ ⇡: `BAR_BUTTON` 26 → **34**, radius 6 → **9**, 1 px `card_border` edge, 15 px icons
`text_dim`. Same actions, tooltips, error line.

**6.7 Show / hide / resize.**
- ⌥⌘\ and ▥ unchanged. **Slide inside the card:** the pane area takes its final width at once (as today); the panel
  (with its hairline) slides in from, or out to, the card's right edge, clipped by the card.
- **Resize:** the grab strip is 6 px centred on the hairline (was the 6 px gap). `width_for_edge` and `drawn_width`
  change for the new insets (8 right, 0 or 8 left, 1 px hairline). `PANE_COLUMN_MIN` 200 unchanged.
- Hidden: the card is the panes alone; the right-hand panes' terminals own the right corners (3.4).

### 7. Status bar

- Kept, as Settings → General → Status Bar says (on by default). Mockups don't show it; dropping it would be a
  feature change. AI's call, flagged as question 2.
- Full window width under the sidebar and the card (as today), on the canvas, **no fill** (3.6).
- `STATUS_BAR` 22 → **26**; `PAD` 10 → **16**; text **12 px** (`text::META`); mark 10 px. Contents and fitting order
  unchanged. Off: the card goes down to 8 px from the window's bottom.

### 8. The agent box inside the card

M11ak's constants in `src/agent_box.rs`, so the box lines up with the terminal's text column and the tabs:

| Constant | Today | M11al |
|---|---|---|
| `AREA_X` | 14 | **28** (the terminal's side padding) |
| `AREA_TOP` / `AREA_BOTTOM` | 6 / 12 | **8 / 20** |
| `PLACE_GAP` | 6 | **8** |
| `BOX_RADIUS` | 12 | **14** (the card's) |
| `BOX_X` / `BOX_Y` / `BOX_GAP` | 12 / 10 / 8 | **14 / 12 / 10** |
| `CHIP_H` / `CHIP_X` / `CHIP_RADIUS` / `CHIP_GAP` | 24 / 8 / 6 / 4 | **28 / 10 / 8 / 6** |
| `BUTTON` | 24 | **28** |
| `MENU_WIDTH` | 240 | **260** |
| `ROW_H` / `ROW_RADIUS` | 26 / 6 | **32 / 8** (as the right panel's rows) |

Colours unchanged (the box's own tokens). The conversation's side room above it should match `AREA_X` (builder:
check the agent tab's page padding; if it is a local constant, set it to 28 too). AI's call on all numbers.

### 9. The file tab's (and browser tab's) outer bar

Only the frame, not the inside (M11ah).
- File bar (`FILE_TOOLBAR` 42): fill `panel` → **`card`**; bottom line `panel_border` → **`card_border`**; side
  padding `BAR_PAD + 4` → **16**. Height stays 42 (AI's call: its two-line title already fits; changing it would touch
  M11ah's layout).
- Browser bar (`browser_tab::BAR` 34): same fill and line change. AI's call (it sits in the same card).
- Find bar: unchanged.

### 10. How it behaves

| Case | Behaviour |
|---|---|
| **Split panes (M8)** | One card; hairlines between panes (3.3); every pane keeps its 52 px tab bar; split terminal padding (20, 14). |
| **Narrow window** | Right panel narrows, then isn't drawn, as today (`drawn_width`). Tabs shrink to 100, then scroll. Sidebar rows drop ages under 240. |
| **Full screen** | No traffic lights; the band and strip stay as gaps that don't drag; insets unchanged. |
| **Sidebar hidden (⌘B)** | The 52 px window band above the card (M11m: name centred, 🔔 ⚙ ▤ at the right); the card starts under it, 8 px from the window's left. |
| **Right panel hidden** | The card holds only panes; the right-hand terminals own the right corners. |
| **Both hidden** | The band above; the card 8 px from left, right and bottom (or the status bar). |
| **Light themes** | `canvas` darker than `card`, `side` between; shadow 0.12 of the darkened ink; every text and mark on every new surface held to its floor. |

### 11. Tokens summary

- **Changed:** `sidebar_background` (darker; is the canvas), `card_shadow_alpha`, sizes in 2–9.
- **New:** `raised`, `side` (+ its hover / selected tints), `tab_active`. Added to the surfaces / text-surfaces lists
  in `ui_uncached` and to `drawn_pairs`.
- **Retired if unused after the build:** `tab_bar` / `tab_hover` for pane bars (and `TAB_BAR_MIN`,
  `TAB_BAR_LIGHT_CAP`, `TAB_BAR_LOW_MIN` and their tests); `drop_tab` re-based on the card; `CARD_GAP`, `TAB_TOP_GAP`,
  `SIDEBAR_ROW_THREE`, the sidebar's group-card constants and `TILE`.
- **Text sizes:** `text::NAME` 13 → **14**, `META` 11 → **12.5**, `GROUP_HEADER` 11 → **12**; new `UI` **13.5** (tabs,
  search, buttons, right-panel rows) and `KEYS` **11.5**. Builder: if 12.5 / 13.5 render soft on a 1× screen, round
  and say so.

## Verification

**Unit tests (pure, red first where they guard a rule):**
- Theme: in every built-in theme and the default, light and dark: `canvas` darker than `card` and `side` between them;
  `card_border` at least `QUIET_LINE_MIN` on canvas, card and side; text, dim and faint text at 4.5:1 on `raised`,
  `side`, its tints and `tab_active`; marks at 3:1 on `tab_active`, `side` and `card_lift`; `tab_active` keeps the
  accent's hue; `text_dim` / `text_faint` unchanged from today (the fixed base).
- Geometry: `drawn_width` and `width_for_edge` with the new insets; the split grab strip centred on a 1 px hairline at
  any ratio; tab look (`look`) for active, focused / unfocused pane, waiting, hover.
- Sidebar: line two precedence with who · where (agent, program, shell; SSH host; in and out of a folder group).

**New ui-shots scenes** (`Themes::Both`, `Kind::Visual`, each in its area file's `SCENES` and in `ORDER` beside its
neighbours, e.g. after `cards-corners`):
- `card` — one card: 8 px from top, right and bottom; flush with the sidebar; canvas just outside each rounded corner;
  shadow darker than the canvas under the card; the hairline between panes and right panel; the tab bar's and the
  right header's bottom lines at the same y.
- `card-split` — a 2 × 2 split: hairlines, no canvas between panes, the veil on unfocused panes, `tab_active` only on
  the focused pane's active tab.
- `card-sidebar-hidden`, `card-right-hidden`, `card-narrow` (the panel not drawn), `card-slide` (the panel clipped by
  the card mid-slide).
- `tabs-minimal` — tile, title, status dot (working, waiting, done ✓, idle none), separators, wash, a preview's italic,
  × on hover, a waiting inactive tab's tint.
- `sidebar-comfortable` — 56 px rows, badge ring, ages, accent bar, quiet uppercase headings, a closed group's status
  only, who · where lines.
- `right-panel-comfortable` — switch, repo card, section headings with pills, 32 px rows, footer buttons.
- `agent-box-card` — the box's edges at the terminal's 28 px.

**Existing scenes whose pixels or positions change** — rewrite their checks, don't delete them:
- `cards`, `cards-split`, `cards-corners` (canvas gaps between cards, per-pane rings, 6 px offsets): rewrite to the
  one-card rules; keep each risk covered (nothing square past a corner; the first tab clear of the curve).
- `tab-bars` and its variants, `tab-drag`, `tabs`, `splits`, `workspaces`, `browser-tab` (tabs 4 px under the top,
  merging into the card, the mark in the icon's place).
- `window`, `title-area`, `title-bar`, `sidebar-button`, `welcome*`, `overlays`, `panels` (band 52, traffic lights,
  anchors).
- `sidebar`, `sidebar-rows`, `sidebar-quiet`, `sidebar-selection`, `sidebar-marks`, `agent-sidebar-waiting` (row
  heights, group cards, line two, 2 → 3 px bar).
- `right-sidebar`, `files`, `files-follow`, `right-panels`, `changes*`, `activity`, `branch*` (position, header, rows,
  footer).
- `status-bar*`, `update-button` (height 26, no fill).
- `cursor`, `command-blocks*` (padding 28 / 22 moves the grid).
- `agent-tab-*`, `agent-box-*` (box sizes and place); `file-tab*`, `file-live-*`, `file-follow-*` (outer bar colour).
- `chrome-*` geometry checks (`chrome-bars-and-edges`, `chrome-sidebar-scroll`, `chrome-file-tab-toolbar`).
- Where a check hard-codes a number ("974–1274", "300 wide", "6 pt"), derive it from the tokens instead. List every
  rewritten scene in this note's build log.
- **Must pass unchanged (M11p proofs):** `chrome-breath-skips-parts`, `chrome-notify-redraws-parts`,
  `chrome-pixels-same` (cached = uncached), `chrome-kill-switch`, `chrome-working-marks`, the counters scenes.

**Reference fingerprints are stale.** Visual scenes will report "looks different"; that never fails a run. **Never run
`--update-refs`** in this milestone. New references wait for your on-screen check.

**Idle and speed (M11p):** grep the branch's diff for new `timer(`, `request_animation_frame`, `spawn` loops: none
expected outside the existing slides. Sample the debug build idle for 60 s against master, as M11ak did, and record
the numbers.

**Your on-screen check** (GUI can't be claimed without it): Catppuccin Mocha and Latte at least; one pane; a 2 × 2
split; sidebar hidden; right panel hidden; both hidden; full screen; a narrow window; an agent tab with the box; a
file tab; a waiting and a working tab.

Run the shots and the app by the built binary's path, never by the bare name `fuxx` (on this Mac that is the real
app).

## Calls on the drafter's open questions (AI's, on your go-ahead — for you to check)

1. **Sidebar rows' second line is "who · where"** (e.g. "✳ Claude · api", "›_ zsh · my-app"), as the R mockup you approved; a waiting agent's line says "needs you". The branch and the other status words leave the rows (status shows through the dot); the branch stays in the group heading, the status bar and the right panel's footer.
2. **The status bar stays**, restyled (26 px, no fill of its own) and still switchable in Settings — dropping it would remove a feature.
3. **Done in tabs is the muted ✓**, the same as the sidebar since Chore 2 — colour means activity only.

## Left to the builder

- How the card's shadow escapes the column's clip (3.5), and the corner check for the agent / file / browser tab
  bodies (3.4).
- Whether `tab_bar` / `tab_hover` and the `TAB_BAR_*` floors can go, or stay for `drop_tab` (11).
- The right panel's row hover / selected tints on `side`, the filter field's fill, and the repo card's fill if
  `card_hover` clashes with row hover.
- Where *who* comes from per row (2.4): the facts the sidebar already gets; the shell's name from the shell the
  terminal runs.
- Exact pixel nudges the mockups leave open (a glyph's baseline, a chevron's size), within the sizes above.
- The plan's task order. Suggested: tokens and colours (with tests) → the card wrapper and splits → tab bar →
  terminal padding → sidebar → right panel → status bar → agent box → file / browser outer bar last (M11ah) → scenes
  and docs.
- If GPUI's API for something here is new to the repo (shadow spread, per-corner radius), check Context7 first.

## Related

[[40 - Roadmap]] · [[83 - Milestone 11ak - Agent Box]] · [[60 - Milestone 11i - UI Uplift 2]] · [[12 - Scope]] ·
[[77 - Milestone 11p - Chrome Caching]] · [[82 - Milestone 11af - Live Changes]]
