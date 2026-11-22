---
tags: [milestone, plugins, settings]
updated: 2026-11-22
---

# 94 - Milestone 11au - Plugin Settings

**Status:** 🟨 built 2026-11-22 on branch `plugin-settings` (a worktree, by an agent, while M11aq's branch is worked
on in the main checkout; all 4 plan tasks, 2,419 tests, both clippy runs at baseline, `preferences` ui-shots passed
with a new `preferences-plugins-settings-<theme>` shot). Not merged, not pushed. Waiting for your on-screen check
(below). *The number and letter were the next free ones on master when it was written (M11ar is Subagents; M11as
and M11at are named on the Roadmap); if they collide at merge they are renumbered.*

## What was built

- `src/plugin_settings.rs` (new, pure): the `setting.*` line, the `plugin.<id>.<name>` values, what counts as
  changed, the env names, removing a plugin's lines.
- `Manifest.settings` and `Record.settings` (old `plugins.json` files still read); install and update copy them.
- `plugin_shell::script` adds `FUXX_PLUGIN_SETTING_<NAME>=…` to the plugin's `export` line and the same names to its
  `unset` line; the values are read from the settings file inside the write's turn. A plugin without settings, or
  with none changed, gets the same block as before.
- `Appearance.plugin_values` (the `plugin.*` lines), `plugins::set_setting` (writes the line, rewrites
  `plugins.zsh`), uninstall removes the plugin's lines with its `theme` line.
- Settings → Plugins: the rows under the card (switches; number fields that save on Enter or leaving and go back on
  Escape or a non-number), with "New terminals get these settings." under them. README "Making a plugin" →
  "Settings".
- **Not checked by eye in the real app** (an agent can't see the window): the off-screen shot shows the four rows
  under the plugin's card (a switch off, one on, fields 1.5 and 5, the note), but typing in the field, leaving it
  and a new tab's environment need your check.

## Why

You approved fuxx-completes' suggestion-list design (2026-11-22,
`fuxx-completes/docs/superpowers/specs/2026-11-22-suggestion-list-design.md` §4.4 and §6): the list has four
settings (on/off, on an empty line too, the pause before it shows, how many items), and inside fuxx they should be
set in **Settings → Plugins → fuxx-completes**, not only as shell variables in `.zshrc`. fuxx's plugin format had
no way to say "I have settings" (note 76: "a plugin keeps its own settings"). This milestone adds that, generic
for any plugin. It doesn't wait for fuxx-completes' M1c, and M1c doesn't wait for it.

## Goals

- A manifest may **declare settings**, one line each:
  ```ini
  setting.list       = bool on    Show a suggestion list when you pause
  setting.list_empty = bool on    Show the list on an empty line too
  setting.list_delay = number 3   Seconds of pause before the list shows
  setting.list_size  = number 5   Items in the list (1–5)
  ```
  `setting.<name> = <type> <default> <label>`; types `bool` (default `on` or `off`) and `number`.
- **Settings → Plugins** shows, on the plugin's card, a row per declared setting: its label and a switch (bool)
  or a number field (number, saved on Enter or when it loses focus; anything that isn't a number goes back).
- The value you choose is kept in **fuxx's own settings file** (`~/.config/fuxx/config`) as
  `plugin.<id>.<name> = <value>`. Choosing the default again removes the line.
- While fuxx loads the plugin's zsh scripts it exports **only the values you changed** as
  `FUXX_PLUGIN_SETTING_<NAME>` (the name upper-cased), next to `FUXX_PLUGIN_DIR` / `FUXX_PLUGIN_DATA`, and unsets
  them afterwards. A value you didn't change isn't exported, so the plugin's own default or your `.zshrc` variable
  decides (the spec's "which wins").
- A change applies to **new tabs** (no live reload).
- **Uninstall** removes the plugin's `plugin.<id>.*` lines from the settings file.

## Rules (the builder's calls, 2026-11-22)

- **Names:** `a-z`, `0-9`, `_`, starting with a letter, up to 40 characters, so `FUXX_PLUGIN_SETTING_<NAME>` is
  always a valid shell name.
- **A malformed setting line is ignored**, never the plugin: an unknown type, a bad name, a `bool` default other
  than `on`/`off`, a `number` default that isn't a plain decimal (`3`, `0.5`, `-1`). The label may be empty (the
  name is shown then). A name declared twice: the last line wins.
- **A stored value counts only if it fits the type** (a hand-edited `plugin.x.list = maybe` is ignored) and only for
  a setting the installed version still declares. "Changed" means a stored value that differs from the default
  (numbers compared as numbers: `3.0` is `3`).
- **Storage in the settings file, not `plugins.json`:** the spec says "stores the values in its config", and note 76
  already treats `theme =` lines that point at a plugin as the plugin's to remove at uninstall. Only the main file
  is edited at uninstall; a line you put in a `config-file` include is yours.
- **An update keeps your values** (the lines are untouched); a setting the new version drops stops being exported.
- **No range checks:** the format has no min/max; the label says the range and the plugin clamps.
- A plugin without settings looks and loads exactly as before (its `plugins.zsh` block is byte-identical).

## Not in it

- `text`, choice or range types, min/max, a live reload of open tabs, settings for theme-only content, a page of
  its own per plugin, bash and fish.

## Approaches

- **A. Rows on the plugin's card (chosen):** the settings sit under the plugin they belong to, in the card that
  already holds its switch and buttons; nothing to navigate.
- B. A "Settings…" button opening a sub-page per plugin: one more click for two to four settings.
- C. Only document the shell variables: works in any terminal but nothing to see in fuxx — the spec asks for B/A.

## Plan

[[M11au Plan - Plugin Settings]] (4 tasks).

## Review and fixes (2026-11-22)

A whole-branch review found three things worth fixing; each got a test that failed first:

- **A stale number field saved a value you never chose.** A field got its text once; after a plugin update moved
  a default (3 → 4) with the page open, clicking into the field and out again wrote `3`. Now the page remembers
  the text it last put in each field, refreshes a field you aren't in when the kept value changes, and leaving a
  field whose text you didn't change saves nothing. The field's save also reads the setting as installed now, not
  as it was when the field was made. Tests `leaving_an_untouched_field_saves_nothing`,
  `a_field_takes_a_changed_value_unless_it_has_focus` (`preferences_plugins`).
- **A setting kind from a newer fuxx made every plugin vanish.** `plugins.json` with, say, `"kind":"choice"` failed
  to read as a whole, so fuxx treated the list as damaged. Unknown kinds now read as `Unknown` and are dropped when
  the file is read: not shown, not exported; the plugin and its other settings work. Test
  `a_setting_kind_from_a_newer_fuxx_is_skipped_not_fatal` (`plugin_store`).
- **Setting names from `plugins.json` reached the generated zsh unchecked** (the file can be edited by hand).
  `changed` now skips any name the manifest reader would refuse (a `;` or a space, upper case, empty), so such a
  name exports nothing. Test `a_name_the_manifest_would_refuse_is_never_exported` (`plugin_settings`).

### Deferred (the review's other minors, left as they are)

- A number field can still be edited while a plugin is busy (install/update running); the save then races it.
- A plugin with settings but no zsh scripts still shows its settings, which nothing reads.
- A `FUXX_PLUGIN_SETTING_<NAME>` already in the environment (from `.zshrc`) is seen by the plugin when you didn't
  change that setting, and is unset after loading.
- The README doesn't say that hand edits to `plugin.<id>.<name>` lines are honoured only when they fit the type.

## Your on-screen check

On branch `plugin-settings` (`cargo run`), with a plugin that declares settings (fuxx-completes once its M1c
declares them, or a test repo with the four lines above):

1. Settings → Plugins: the plugin's card shows four rows under it: two switches (on) and two number fields (3, 5).
2. Turn "Show a suggestion list when you pause" off: `~/.config/fuxx/config` gains `plugin.<id>.list = off`.
3. Type 1.5 in the pause field, Enter: the file gains `plugin.<id>.list_delay = 1.5`. Type `abc`: it goes back.
4. A new tab: the plugin sees `FUXX_PLUGIN_SETTING_LIST=off` and `FUXX_PLUGIN_SETTING_LIST_DELAY=1.5` while it
   loads; `echo $FUXX_PLUGIN_SETTING_LIST` at the prompt prints nothing (unset after loading).
5. Turn the switch on again (its default): the line leaves the file.
6. Uninstall the plugin: its `plugin.<id>.*` lines are gone.
7. A plugin without settings looks exactly as before.
