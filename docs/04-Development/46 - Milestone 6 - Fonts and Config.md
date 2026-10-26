---
tags: [development, milestone]
status: in-progress
---

# 46 - Milestone 6 - Fonts and Config

**Status:: 🟨 In progress (plan written 2026-10-27; changed the same day to fuxx's own settings file).**  ·  **Phase 2: fuxx looks the way you want.**
**Plan:** [[M6 Plan - Fonts and Config]] (step-by-step, with status per task)

## Goal

fuxx reads **its own settings file**, which sets the **font, size, ligatures and colors**, so you can
make it look the way you want, as in Screen 2 of the mockups (`mockups/2-daily-driver.html`, items #5
and #6). **Prompt icons** (Nerd Font symbols, `?` boxes today) draw correctly for everyone. The
sidebar takes its shades from the same theme. ([[40 - Roadmap]] M6 line; [[11 - Features]] A.)

> [!note] Changed 2026-10-27, on request
> The first version of this plan read **Ghostty's** config so fuxx would match Ghostty. You asked
> for fuxx to have its own settings instead, without depending on Ghostty (option 1). fuxx now
> ignores Ghostty's files. It keeps Ghostty's simple `key = value` format and key names, so a
> Ghostty setting or theme file can be copied over as-is. [[12 - Scope]] was updated first.
> The mockup's "theme from your Ghostty config" label is out of date.

## How it behaves (the calls made for this plan, 2026-10-27)

Calls by the AI unless marked as yours. Each was checked on this Mac. Say if any should change.

- **The settings file (your choice).** It lives at `~/.config/fuxx/config`, or
  `$XDG_CONFIG_HOME/fuxx/config` when that is set.
  - It uses plain `key = value` lines. `#` starts a comment line, quotes are optional, and an empty
    value resets a key to its default.
  - `config-file = path` includes another file. The path is relative to the including file, a
    leading `?` makes it optional, and include loops are cut.
  - The file is read **once at startup**; restart fuxx to apply a change.
  - fuxx never creates or edits the file. If it doesn't exist, the defaults are used.
- **Defaults** (Ghostty's, which look good): background `#282c34`, foreground `#ffffff`, a 16-color
  palette, and font size 13. The font is **JetBrains Mono** if it's installed, otherwise Menlo.
- **Font settings.**
  - `font-family`: the first entry is the font. Further entries are fallbacks. Families that aren't
    installed are skipped.
  - `font-size`: in points.
  - `font-feature`: for example `-calt` turns programming ligatures off. Ligatures are **on** by
    default.
  - Bold and italic use the same family's bold and italic faces.
- **Prompt icons.** fuxx carries the **Symbols Nerd Font** (Nerd Fonts 3.5.1; MIT, with icon sets
  under CC BY 4.0, Apache 2.0 and OFL, see `assets/fonts/`). macOS gets it as the last fallback, so
  any icon your font lacks comes from it, whatever font you use. A font only loaded in fuxx's memory
  is ignored by macOS, so fuxx registers it with macOS from a file for its own process.
- **Colors.**
  - Keys: `background`, `foreground`, `palette = N=#rrggbb` (colors 0–15), `cursor-color`,
    `cursor-text`, `selection-background`, `selection-foreground`.
  - Values in your settings file override the theme's.
  - Colors that programs set themselves (OSC 4/10/11) still win over both.
- **Themes (your choice: both sources).**
  - `theme = Name` looks first in your folder `~/.config/fuxx/themes/Name`. Any Ghostty or
    iTerm2-Color-Schemes theme file can be copied there.
  - Next it checks fuxx's **six built-in themes**: Catppuccin Mocha, Catppuccin Latte, Dracula,
    Nord, Gruvbox Dark and TokyoNight. They come from iTerm2-Color-Schemes, MIT licence, see
    `assets/themes/`.
  - A full path also works.
  - `light:A,dark:B` picks a theme by the macOS appearance at startup.
- **The rest of the window follows the theme.** The sidebar is a slightly darker shade of the
  background, the active and hovered rows are tints of the foreground, and the rings use the theme's
  yellow, red and green.
- **Bad or unknown settings** are skipped, so fuxx always starts. Each skipped setting gets one log line.

## Rough scope / checklist

- [ ] fuxx's own settings file read at startup, with includes followed.
- [ ] Theme: defaults → theme (your folder, then built-in) → your values; light/dark pick.
- [ ] Terminal colors, cursor and selection come from the theme.
- [ ] Font family (with fallbacks), size and features from the settings file.
- [ ] Prompt icons via the bundled Symbols Nerd Font.
- [ ] Sidebar shades and rings derived from the theme.

## Not in M6

- **No reading Ghostty's config.** You can copy settings over by hand.
- **No settings screen.**
- **No starter file written by fuxx.**
- **No live reload.**
- **No font zoom** (cmd-+ / cmd--).
- **Not supported:** window padding or opacity, `adjust-*` metrics, bold-font keys, X11 color
  names, `bold-color`, `minimum-contrast`, `palette-generate`.
- **No other UI polish** beyond what the theme drives. M7 covers session ergonomics; anything else
  needs a plan change first.

## Success check

With no settings file, fuxx starts with the default colors, and your prompt shows its icons. A
`~/.config/fuxx/config` with `font-family = MesloLGS NF` and `theme = Catppuccin Mocha` turns fuxx
Catppuccin with that font, sidebar included. Ghostty's config no longer has any effect on fuxx.

## Related

- [[40 - Roadmap]] · [[11 - Features]] · [[12 - Scope]] · [[00 - Plan Tracker (Start Here)]] · [[45 - Milestone 5 - Terminal UX]]
