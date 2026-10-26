---
tags: [development, milestone]
status: not-started
---

# 46 - Milestone 6 - Fonts and Config

**Status:: ⬜ Not started (plan written 2026-10-27).**  ·  **Phase 2: fuxx looks like *your* terminal.**
**Plan:** [[M6 Plan - Fonts and Config]] (step-by-step, with status per task)

## Goal

fuxx reads your **Ghostty config** and uses the same **font, size, ligatures and colors**, so it looks
like the terminal you already use, as in Screen 2 of the mockups (`mockups/2-daily-driver.html`, items
#5 and #6). **Prompt icons** (Nerd Font symbols, `?` boxes today) draw correctly for everyone. The
sidebar takes its shades from the same theme. ([[40 - Roadmap]] M6 line; [[11 - Features]] A.)

## How it behaves (the calls made for this plan, 2026-10-27)

You chose to skip the questions and asked to follow the mockup, so these are the AI's calls. Each was
checked against Ghostty 1.3.1's own docs and on this Mac. Say if any should change.

- **Where the settings come from:** Ghostty's config files only. fuxx gets no config file of its own
  ([[12 - Scope]]: no config DSL). They are read in Ghostty's order, later files winning:
  1. `~/.config/ghostty/config`
  2. `~/.config/ghostty/config.ghostty`
  3. `~/Library/Application Support/com.mitchellh.ghostty/config`
  4. `~/Library/Application Support/com.mitchellh.ghostty/config.ghostty`

  `$XDG_CONFIG_HOME` replaces `~/.config` when it is set. `config-file` includes are followed: a path
  is relative to its file, a leading `?` means optional, and loops are cut. Read **once at startup**:
  restart fuxx to apply a change.
- **No Ghostty config (or no Ghostty):** Ghostty's own defaults:
  - background `#282c34`, foreground `#ffffff`, and Ghostty's 16 colors;
  - font size 13;
  - the font is **JetBrains Mono** if it's installed, else Menlo.

  Ghostty ships JetBrains Mono inside itself; fuxx doesn't bundle it.
- **Font:**
  - `font-family`: the first entry is the font; further entries are fallbacks, as in Ghostty. Families
    that aren't installed are skipped.
  - `font-size`: in points, used as-is.
  - `font-feature`: e.g. `-calt` turns programming ligatures off.
  - Ligatures are **on** by default, as in Ghostty.
  - Bold and italic use the same family's bold/italic faces; `font-family-bold` etc. are ignored.
- **Prompt icons:** fuxx carries the **Symbols Nerd Font** (MIT licence, about 2.6 MB) and gives it to
  macOS as the last fallback. Any icon your font lacks comes from it, whatever font you use.
  (Checked 2026-10-27: a font only loaded in memory is ignored by macOS, so fuxx registers it with
  macOS from a file for its own process.)
- **Colors and theme:**
  - The keys used are `background`, `foreground`, `palette = N=#rrggbb` (colors 0–15),
    `cursor-color`, `cursor-text`, `selection-background` and `selection-foreground`.
  - `theme = Name` loads that Ghostty theme from your `themes` folder, else from Ghostty.app's 463
    built-in themes. A full path works too.
  - `light:A,dark:B` picks by macOS appearance at startup.
  - Values in your own config override the theme's, as in Ghostty.
- **The rest of the window follows the theme:** the sidebar is a slightly darker shade of the
  background, the active and hovered rows are tints of the foreground, and the rings use the
  theme's yellow / red / green.
- **Bad or unknown settings** are skipped silently, like Ghostty, so fuxx always starts. A line
  saying what was skipped goes to the log.
- **Colors programs set themselves** (OSC 4/10/11) still win over the theme, as today.

## Rough scope / checklist

- [ ] Ghostty config files found and read in Ghostty's order, includes followed.
- [ ] Theme: defaults → theme file → your values; light/dark pick.
- [ ] Terminal colors, cursor and selection come from the theme.
- [ ] Font family (with fallbacks), size and features from the config.
- [ ] Prompt icons via the bundled Symbols Nerd Font.
- [ ] Sidebar shades and rings derived from the theme.

## Not in M6

- **No fuxx-only settings and no settings screen** ([[12 - Scope]]).
- **No live reload.**
- **No font zoom** (cmd-+ / cmd--).
- **No window padding or opacity, no `adjust-*` metrics.**
- **No bold-font keys.**
- **X11 color names** (`background = navy`) are skipped; hex colors work.
- **No `bold-color` / `minimum-contrast`.**
- **No `palette-generate`.**
- **Other UI polish beyond what the theme drives:** e.g. new sidebar controls. M7 has the session
  ergonomics; anything else needs a plan change first.

## Success check

With your config (MesloLGS NF, Ghostty default colors), fuxx's prompt shows its icons and the same
colors as Ghostty. Setting `theme = Catppuccin Mocha` in Ghostty's config and restarting fuxx turns
fuxx Catppuccin, sidebar included.

## Related

- [[40 - Roadmap]] · [[11 - Features]] · [[12 - Scope]] · [[00 - Plan Tracker (Start Here)]] · [[45 - Milestone 5 - Terminal UX]]
