# Milestone 11x — Preferences Panel

**Status:** 🟨 built 2026-11-12, committed (`7082308`) and released in **v0.10.0** (run 43208136200) on your word
("commit, push and release"); waiting for your on-screen check (below). Designed with you (mockups round 9:
`prefs-panel-v1`, `prefs-appearance-v1`, `prefs-rest-v1`; "all good, go ahead"). Plan [[M11x Plan - Preferences Panel]].
**Asked for:** 2026-11-12, by you, after v0.9.0: "I want to redesign the "Preference" window specifically on the tabs
(sidebar), category, fields, labels, window style, etc." with a screenshot of a settings panel → why: "It looks plain
and messy, everything crammed together. And the how it present the window, the image is much cooler (radius big and
its like no window top bar)" → "yes, a panel over the terminal like the screenshot" → "This is cool! and can you also
add a new sound field for notification as well, this is oot but its small" → "only with a banner" → "all good, go
ahead". Comes before M11p (your call: "what I want to do next").

## What it is

Preferences stops being a separate macOS window. **⌘,** (and File › Preferences…, the sidebar's gear, the title
bar's button, a setting picked in ⌘K) opens it as a **panel over the terminals, inside the fuxx window**, with the
window dimmed behind it: big rounded corners, no title bar. Inside: the sections down the left, and on the right each
section as a page — a big title, a line saying what the page is for, and its settings in **groups**, each group a
rounded **card of rows** (the setting's name and a short explanation at the left, its control at the right).

Same five sections and the same settings as today, plus one new setting: **Banner sound**.

## The panel

1. **Over the window:** a scrim dims the whole fuxx window (sidebars, panes, bars); the panel is centred on it, 880 ×
   580 pt, smaller when the window is (at least 24 pt from each edge; the page scrolls when it doesn't fit). Corners
   22 pt, a 1 pt border, a soft shadow. No title bar; a **✕** at its top right.
2. **Closing:** the ✕, **Esc**, a click on the dimmed part, **⌘W** (closes the panel, never the terminal behind it)
   or **⌘,** again.
3. **While it's open** the keys go to it, not to the terminal behind; the terminals keep running and drawing. fuxx's
   own shortcuts behave as they do while the ⌘K palette is open (the AI's call); ⌘+ and ⌘− still change the size.
4. **The left side** (200 pt, a shade apart from the page, a hairline at its right): a faint "Settings" at the top,
   then General, Appearance, Sessions, Browser, Sound (a soft rounded fill on the one shown), and fuxx's version at
   the bottom (`fuxx 0.9.0`). **↑ / ↓** move between sections (when no text field has the keys), as today.
5. **A page:** its title (26 pt, semibold), a line under it (14 pt, faint), then its groups: a group's name (15 pt,
   semibold) over a card (14 pt corners, a 1 pt border, rows split by hairlines). A row: the setting's name (14 pt)
   with its explanation under it (12.5 pt, faint), the control at the right. The title scrolls with the page.
6. **Controls:** On/Off settings become a **switch** (filled in the accent colour when on); settings with more than
   two choices stay **segmented** buttons; the font is a **dropdown**; size is **− 13 pt +**; buttons are soft pills.
7. **Follows your theme:** its colours come from fuxx's theme (Latte gives a light panel); it opens on the section last
   shown in this run, as today.

## The pages (words are the AI's, as in the mockups)

**General** — "How fuxx looks around your terminals and keeps itself up to date."
- *Interface:* Animations (switch; "Also off when macOS Reduce motion is on."), Status Bar (switch; "The line along the
  window's foot."), Tips card ("The welcome tips in the sidebar."; a **Show again** button).
- *Updates:* Check for updates (switch; "Asks GitHub once a day whether a newer fuxx is out.").

**Appearance** — "Colours, type and the cursor in every terminal."
- **The preview** across the top (the same fixed terminal screen as today, in the current theme, font, size and
  ligatures), "Preview — changes as you pick." under it.
- *Theme:* the seven themes as **tiles**, four to a row: each a small screen in the theme's background with a text line
  and five of its colours, its name under it; the one in use outlined in the accent colour with a ✓.
- *Text:* Font ("Monospaced fonts installed on this Mac."; a dropdown: the list opens under it, each font's name in its
  own face, Default first), Size ("⌘+ and ⌘− change it too."; − 13 pt +), Ligatures ("Joins pairs like != and => into
  one sign."; switch).
- *Cursor:* Style (Bar / Underline / Hollow / Block; "Programs that set their own cursor (vim, for example) still
  choose their shape."), Glide ("Slides to where you type instead of jumping."; switch), Breathing glow ("A soft pulse
  while the terminal waits."; switch), Colour follows the agent ("Tinted by what the agent is doing."; switch), Fun
  (Off / Trail / Sparks; "A little extra as you type.").

**Sessions** — "What fuxx keeps when you quit and opens when you come back."
- *Quitting and reopening:* Restore sessions (Off / Layout / Layout + agents; "Reopen your workspaces, and the agents in
  them, next time."), Keep running after quit (switch; "Takes effect the next time fuxx starts.").
- *Terminal:* Command blocks (switch; "Marks each command in zsh. New terminals only.").

**Browser** — "The browser tabs inside your panes."
- *Browsing:* Show address bar (switch; "The bar with the address, back and reload above each page."), Keep history and
  data (switch; "Turning this off deletes saved browser data. Applies to browser windows opened after the change.").

**Sound** — "A sound file or web link for each moment; leave it empty for silence."
- *Keys:* Enter ("When you press Enter in a terminal."), Ctrl + C ("When you press Ctrl + C in a terminal.").
- *Notifications* (new): **Banner sound** ("Plays when fuxx shows a banner; never for what only goes to the 🔔 list.").
- Each sound row: its switch, the link field (a file or `https://` link; placeholder "A file or https:// link") and
  ▶ to try it, with the problem line under it (can't play, downloading) as today.

## Banner sound (new)

- Settings-file keys `sound-banner` (the link) and `sound-banner-on` (on by default), as Enter's and Ctrl + C's.
- It plays **only when fuxx shows a banner** (the native banner or the `osascript` one): never for what goes only to
  the 🔔 list (the terminal in front, a Finished run under 10 s, …). Loaded, downloaded and played as the key sounds
  are (`NSSound`; a web link downloaded once, off the UI thread).
- macOS's own banner stays silent as today, so the sound is fuxx's alone.
- **"Shows a banner" means fuxx posted one** (the AI's call, after the builder check): fuxx can't know whether macOS
  then hid it (a Focus mode, Do Not Disturb, banners turned off for fuxx; the `osascript` route can't be asked at all),
  so the sound plays with every banner fuxx posts.
- Banners a script posts (`fuxx notify`, the scripting API) are banners too: they play it.
- **After you quit too** (your answer, 2026-11-12: "Yes, those too"): with Keep running after quit on, the background
  service's own banners (an agent finished or waiting, no fuxx window open) play it as well. The service reads
  `sound-banner` / `sound-banner-on` from the settings file when it posts and plays the sound itself; a web link plays
  from the copy the app downloaded (none yet: silent). Only one side ever plays a banner's sound: the service posts
  banners only while no window is attached.

## Wrap in the file tab (part 2, added 2026-11-12)

Your ask, mid-build: "beside File | Diff, on the left, add 1 more toggle called Wrap. 1 button but it shows like toggle
button contained if wrap true, button outlined when wrap is false".

- A **Wrap** button in a file tab's bar, left of File | Diff: filled when wrap is on, outlined when off; a click
  switches it.
- On: long lines wrap at the tab's width instead of scrolling sideways, in the File and the Diff view; a line's number,
  change bar and sign show on its first row only; selecting and Find work across the wrapped rows.
- The AI's calls: off by default (as files show today); one choice for every file tab, remembered (settings key
  `file-wrap`).

## Part 3 (added 2026-11-12, your asks mid-build)

1. "can you make "Graph" by default collapsed?" — GRAPH starts closed; its commits are read only once you open it.
2. "I cannot tell if my local branch is ahead/behind the source" (a branch not on origin yet, the bar said "Local only ·
   not on origin") — the bar now counts it against origin's default branch, the one it most likely came from
   (`origin/HEAD`, else `origin/main`, else `origin/master`; the AI's call): `Local only · ↑3 ahead of origin/main`,
   `↓2 behind origin/main`, `↑3 ↓2 vs origin/main`, `even with origin/main`. Publish stays the button.
3. "this resize separation color, is ugly.." — the line on a resize edge under the pointer (both sidebars, and pane
   splits, for one look; the AI's call) is a quiet grey (faint text at half strength) instead of the accent blue.

## How (the AI's calls)

- **One view, hosted in the workspace:** the Preferences view becomes an overlay of the workspace's window (as the ⌘K
  palette and the Welcome card are), not a window of its own; one panel per fuxx window. Its settings code (`prefs`,
  `settings`, `appearance`) is unchanged: every click still changes one line of the settings file and reloads.
- **The page is laid out from data:** each section's groups and rows (name, explanation, control) in a table, so the
  ⌘K palette's settings list, the heights and the screenshots all read the same table.
- **No animation** when it opens or closes (it appears at once).
- **No new library.**

## As built (2026-11-12, AI's calls)

- **The panel** is a workspace overlay (`Overlay::Preferences`, so in-pane browser pages hide under it, as under ⌘K);
  the scrim is the theme's shadow colour at 50 % over the whole window; the panel 880 × 580 pt (smaller windows: 24 pt
  in), 22 pt corners, a soft shadow. ⌘, is handled by the window itself (and the app's menu); ✕, Esc (not while a sound
  field has the keys: there Esc puts back the saved link), a click outside and ⌘W close it; ⌘W never reaches the
  terminal behind. While it's open, the keyboard is the panel's.
- **The pages** are one table (`Section::groups`: each group's title and rows; each row's explanation), which the ⌘K
  palette's settings list also reads (Banner sound is found by "banner", "notification", "sound-banner").
- **Controls:** switches (38 × 22 pt, the accent when on), segmented Restore / Style / Fun, the size stepper, "Show
  again" (greyed with "Shown" while the card shows), the font dropdown (its list above the page, at most eight rows
  then it scrolls, each font in its face; a click elsewhere or Esc closes it), theme tiles four to a row (the one in
  use outlined in the accent, ✓). A sound row: its switch, the link field (the placeholder "A file or https:// link"
  while empty), ▶; a sound's problem (no such file, not a sound, couldn't download, downloading) shows under its
  explanation.
- **The preview** across the top of Appearance puts the agent's question box beside the terminal text (on a short,
  wide screen the old stacked text scrolled its first lines away).
- **Banner sound:** a third sound (`sound-banner`, `sound-banner-on`); the window plays it beside every banner it posts
  (an agent's state, a script's `notify`); the background service, posting while no window is attached, plays the file
  the settings name with `/usr/bin/afplay` (a web link only once the app has downloaded it).
- **A crash found and fixed on the way:** the panel read the window while the window was opening it; it now gets what
  it needs passed in.
- **Wrap (part 2):** the button left of File | Diff (filled on, outlined off; for any file with text); rows made from
  each line's width in cells at the text's column count, kept until the file, the mode or the width changes; no
  sideways scroll while on; numbers, bars and signs on a line's first row; selection, Find, the change bars and the
  diff's "go to the change" follow the rows.
- ui-shots: `preferences-<section>` (both themes), `preferences-small`, the keys (Esc, ⌘, both ways, ⌘W, focus),
  `palette-settings` (a setting opens the panel on its section), `overlays` (the panel hides pages), `file-tab-wrap`.

## Not in it

A search field in the panel, the screenshot's Projects / account parts, new settings other than Banner sound,
per-project settings, a separate Preferences window as an option.

## Your check (on screen)

1. ⌘, opens the panel over the window, dimmed behind, big corners, no title bar; ✕, Esc, a click outside, ⌘W and ⌘,
   close it, and ⌘W never closes a terminal while it's open.
2. Typing while it's open never reaches the terminal behind it.
3. Each page reads as in the mockups: the title, the line under it, the groups' cards, the switches.
4. Appearance: the preview at the top; picking a theme tile, a font from the dropdown, the size and the cursor rows
   change the preview and every terminal at once.
5. Sound: Banner sound plays with a banner (try a long agent run with fuxx in the background) and stays silent for a
   short run or the terminal in front; with Keep running after quit on, quit fuxx and let an agent finish: the banner
   plays it too.
6. ⌘K → a setting opens the panel on its section.
7. Latte (light) theme: the panel reads well; a small window: the page scrolls inside the panel.
8. A file tab: Wrap (left of File | Diff) is outlined; a click fills it and long lines wrap at the tab's width (File and
   Diff), the numbers only on each line's first row; selecting across wrapped rows and Find (⌘F, ⌘G) still land right;
   click again: back to one row a line, scrolling sideways; it stays as you left it after a restart.
9. Changes tab: GRAPH is closed when fuxx starts; a branch not on origin says how far it is from origin's default
   branch; the resize edges (both sidebars, pane splits) show a quiet grey line on hover.
