# M11au Plugin Settings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans (this repo's build mode: the session
> builds every task itself; per-task gate `cargo test --bin fuxx`, both clippy runs, `cargo fmt --check`; screenshots
> at the end). Steps use checkbox (`- [ ]`) syntax.

**Goal:** a plugin declares settings in `fuxx-plugin.conf`; Settings → Plugins shows a control per setting; fuxx
keeps the values in its settings file and exports the changed ones while it loads the plugin's zsh scripts.

**Architecture:** a new pure module `plugin_settings` (the `setting.*` line, the stored `plugin.<id>.<name>` lines,
which values count as changed, the env names, removing a plugin's lines). The manifest and the plugin record carry
the declared settings; `plugin_shell::script` takes the stored values and adds the exports to a plugin's block;
`Appearance` keeps the `plugin.*` entries for the page; the page draws the rows.

**Tech Stack:** Rust, GPUI (pinned rev), fuxx's `settings` parser. No new crate.

**Spec:** [[94 - Milestone 11au - Plugin Settings]] (approved 2026-11-22 via fuxx-completes' design §4.4, §6).

## Global Constraints

- A malformed setting line is ignored, never the plugin; a plugin without settings is unchanged (byte-identical
  `plugins.zsh` block, same card).
- Only values that differ from the default are exported; every exported name is unset in the block's `always` part.
- Values are zsh-quoted with `zsh_quote` like every other word in `plugins.zsh`.
- Uninstall edits only the main settings file (like the `theme` line, note 76 §8).

## Tasks

- [x] **Task 1 — Declared settings in the manifest and the record.** `plugin_settings::{Setting, Kind, parse}`;
  `Manifest.settings` (from `setting.*` keys, in file order, last duplicate wins, malformed ignored);
  `Record.settings` (`serde(default)`, copied at install and update). Tests: the four example lines, each malformed
  form, duplicates, a manifest with settings stays valid, an old `plugins.json` without the field loads.
- [x] **Task 2 — Values in the settings file.** `config_key`, `current` (stored-or-default for the page),
  `changed` (valid, declared, ≠ default), `values_for` (what to write: nothing for the default),
  `config_without_plugin` (drops `plugin.<id>.*`); `Appearance.plugin_values`; uninstall removes the lines.
  Tests: each rule, numbers compared as numbers, another plugin's lines untouched.
- [x] **Task 3 — Export while loading.** `plugin_shell::script(f, records, values)`; `write`/`write_from` carry
  the values read from the settings file inside the write's turn. Tests: the exact block with two changed values,
  unchanged values not exported, a plugin without settings byte-identical, a real zsh sees the values while loading
  and not after.
- [x] **Task 4 — Settings → Plugins rows, docs.** A row per declared setting on the card: the label, a switch or a
  number field (a `NameInput` per number setting, kept in step with the records; Enter or blur saves; a bad number
  goes back). Saving writes the settings file (`preferences::apply`) and regenerates `plugins.zsh`. The
  `preferences-plugins-installed` scene gains a helper with settings and asserts a changed value changes the page.
  README's "Making a plugin" documents `setting.*`.
