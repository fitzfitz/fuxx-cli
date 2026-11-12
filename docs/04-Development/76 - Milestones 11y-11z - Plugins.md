---
tags: [development, milestone, design]
status: built-m11y
---

# 76 - Milestones 11y-11z - Plugins (design)

**Status:: M11y 🟨 built 2026-11-13 on branch `m11y-plugins`, waiting for your on-screen check; M11z not planned yet.** Comes **before M11p** (your call). Plugins moved **into scope** on 2026-11-12 by you
("yes, plugins in scope, start the design"). You handed the details over ("Do the thinking"), so everything below
except "Decided by you" is **the AI's call** (revised after a builder check: §8's theme pairs, §9's loading after
your `.zshrc` and independence from `command-blocks`, §10's double-load guard and M11z's test helper).
**Phase 4: an M11 extension.** Two milestones: **M11y — Plugins and theme plugins**, then **M11z — Helper-program
plugins** (fuxx-completes first).

## Goal

Anyone can extend fuxx by pasting a GitHub link into **Settings → Plugins**. The plugin installs, works, and can be
uninstalled from the same page, after which **nothing of it is left**. Every plugin follows one format, so fuxx can
treat any repo that follows it as a plugin.

This round has two kinds of plugin:

1. **Themes** (M11y): colour themes a designer shares with the team. Data only; nothing runs.
2. **Helper programs** (M11z): a program plus a zsh hook that run in fuxx's terminals. The first one is
   **fuxx-completes** (ghost-text suggestions now, a menu later).

## Why (your words, 2026-11-12)

- "improve the terminal shell to be human friendly, not just developer friendly. Designer can use it with enjoy,
  product team aswell can utilize it with happiness"
- "we make it as plugins. we give fuxx-cli the github repo on settings > plugins and its installed. We remove it just
  simply uninstall it from settings > pluigns. Make sure this plugins have format so we can treat it as a plugins.
  When removed, make sure its remove completely."
- On sharing a theme: "they push it to github, PM paste the repo link in plugins"
- "Do the thinking. For now I want themes and the helper programs (fuxx-completes)"

## Decided by you

1. **Plugins are in scope** (2026-11-12). This reverses "A plugin ecosystem" in [[12 - Scope]]'s out-of-scope list.
2. **Install = paste a GitHub repo link in Settings → Plugins. Uninstall = one button on the same page.**
3. **Plugins have a fixed format** fuxx can recognise.
4. **Uninstalling removes it completely.**
5. **Sharing a theme = push it to GitHub; the other person pastes the link.**
6. **This round: themes and helper programs (fuxx-completes).** Plugins that show screens (sidebar widgets, panels,
   tabs: your pet game, world clock, stocks, a GBA emulator in a tab) are **later**, not in M11y or M11z.
7. **Order:** the plugin core proved with themes first, then helper programs.
8. **Both come before M11p** (2026-11-13).

## What exists today

- **Themes** (`theme.rs`): six built-ins compiled in; your own theme files in `~/.config/fuxx/themes/` (Ghostty's
  key names, `key = value`). Preferences → Appearance shows `Default`, then your themes and the built-ins **merged
  in one list sorted ignoring case** (`prefs::theme_choices`, `prefs.rs:27`). The chosen theme is the `theme =`
  line in `~/.config/fuxx/config`: a plain name, or a pair `light:A,dark:B` (`theme.rs:650`); the config may also
  pull in other files with `config-file` (`settings.rs:141`).
- **Downloads without new libraries** (`updater.rs`, `sounds.rs`): fuxx already calls `/usr/bin/curl`, `ditto`,
  `shasum` on the background executor and asks GitHub's API for a repo's latest release (no token, public repos).
  `fetch_latest` calls curl directly; only the parsing of answers is tested (`curl_answer`, `parse_latest`).
- **The zsh integration** (`shell_integration.rs`, M11l): fuxx writes its scripts to
  `~/Library/Application Support/fuxx/shell-integration/zsh/` and points `ZDOTDIR` there, so a zsh started by fuxx
  runs fuxx's hooks without touching your dotfiles. Only zsh is injected (bash and fish aren't), and **only while
  `command-blocks` is on** (`env_for`, `shell_integration.rs:113`). fuxx's `.zshenv` puts your `ZDOTDIR` back
  and sources `fuxx-integration.zsh` right there, **before your `.zshrc`**; fuxx has no step after it today.
- **Preferences** (`preferences.rs`, M11x): a panel of pages (General, Appearance, …, Browser, Sound), each a page
  of grouped cards.
- **fuxx-completes** (`~/Kerja/proj/fuxx-completes`, `docs/design.md`): designed 2026-11-09, not built (its
  roadmap is at M0). Its design today says fuxx **links its crates directly**; this note changes that (see "Changes
  to fuxx-completes").

## AI's calls

### 1. What a plugin is

- A plugin is **a folder with a `fuxx-plugin.conf` at its top**, in a public GitHub repo.
- fuxx never loads a plugin's code into itself (Rust can't safely load code at runtime). A theme is read as data;
  a helper program runs as its own program, started by its zsh hook.
- **No new library.** The manifest uses fuxx's own `key = value` syntax (the settings file's parser, repeated keys
  allowed), downloads use `curl`, unpacking uses `tar` / `ditto`, the record of what's installed is JSON
  (`serde_json`, already in).

### 2. The format: `fuxx-plugin.conf`

```
# A theme plugin
format      = 1
id          = sunset-themes
name        = Sunset Themes
version     = 1.0.0
description = Warm themes from the design team.
theme       = themes/Sunset
theme       = themes/Sunset Light
```

```
# A helper-program plugin
format      = 1
id          = fuxx-completes
name        = fuxx-completes
version     = 0.1.0
description = Suggestions from your history as you type.
bin         = bin
zsh         = shell/fuxx-completes.plugin.zsh
```

| Key | Required | Meaning |
| --- | --- | --- |
| `format` | yes | The plugin format version. This design is `1`. A higher number → "Needs a newer fuxx". |
| `id` | yes | `a-z`, `0-9`, `-`; 2–40 characters. The folder name and the key in fuxx's records. |
| `name` | yes | Shown in Settings → Plugins. |
| `version` | yes | Shown; free text (a release tag also identifies what's installed, see §3). |
| `description` | no | One line, shown under the name. |
| `theme` | no, repeatable | A theme file in fuxx's theme format, as a path inside the plugin. Its file name is the theme's name. |
| `bin` | no | A folder inside the plugin added to `PATH` in fuxx's zsh terminals. |
| `zsh` | no, repeatable | A zsh script inside the plugin, sourced in fuxx's zsh terminals. |

- **A plugin may have themes, a helper program, or both.** There is no `kind` key: a plugin **runs code** if and
  only if it has `bin` or `zsh`. That one fact decides the install warning (§4) and the on/off switch (§6).
- **Every path must stay inside the plugin's folder**: relative, no `..`, no symlink pointing outside. A plugin that
  breaks this is refused at install.
- Unknown keys are ignored (room for later formats, e.g. screens), so a format-1 fuxx still installs a plugin
  that adds optional keys it doesn't know.
- Each new `format` number is a promise: fuxx keeps reading every older format.

### 3. Where a plugin comes from

- **The link.** Accepted: `https://github.com/owner/repo`, `github.com/owner/repo`, `owner/repo` (trailing `.git`
  or `/` ignored). Anything else: "Paste a GitHub repo link, like github.com/owner/repo." Public repos only.
- **What gets downloaded:**
  1. If the repo has **releases**, the latest one. From its assets fuxx takes `fuxx-plugin-macos-arm64.zip` or
     `fuxx-plugin-macos-x86_64.zip` for this Mac's chip, else `fuxx-plugin-macos.zip` (universal), else the
     release's source tarball.
  2. If it has **no releases** (a designer who just pushes), the default branch's tarball at its newest commit.
- **What identifies the installed version:** the release tag, or the commit hash. That's what update checks compare.
- **A helper program needs a release** with a prebuilt zip: fuxx never builds code (your designers and PMs won't
  have Rust). A source tarball that declares `bin`/`zsh` installs only if every file it names is there (scripts-only
  helpers, already-built binaries in the repo); a `bin` whose files aren't executable for this Mac is refused:
  "This plugin has no program for this Mac."
- **Limits:** a download over **100 MB** is refused (curl `--max-filesize`); 60 s per request; offline or a GitHub
  error says so in plain words and installs nothing.
- **Unpacked size: at most 500 MB** (your call, 2026-11-13: "add the 500 MB cap", after the final review found
  that a small archive could expand enough to fill the disk before the confirmation). Before unpacking, fuxx adds
  up the archive's own listing (`tar -tvzf`, `zipinfo -t`); over 500 MB → "This plugin unpacks to over 500 MB, so
  fuxx won't install it." and nothing is unpacked. An archive that can't be listed → "fuxx couldn't unpack this
  plugin."

### 4. Installing

1. Settings → Plugins → paste the link → **Install**.
2. fuxx downloads into a private staging folder, unpacks, strips GitHub's top folder (`owner-repo-<sha>/`), reads
   `fuxx-plugin.conf` and checks it (§2). Any problem → one plain message, the staging folder deleted.
3. **A confirmation**, every time, showing the name, version, `github.com/owner/repo`, the description, and what it
   adds: "Adds 2 themes: Sunset, Sunset Light", and for a plugin that runs code, in the warning colour:
   **"Runs a program on your Mac in every new terminal. fuxx can't limit what it does, so only install plugins from
   people you trust."** No fake permission list: fuxx can't enforce one on a native program, so it doesn't pretend.
4. **Install** → the folder is moved into place in one rename and recorded (§7). Cancel → nothing kept.
5. An `id` that's already installed **from the same repo** is an update (§5); **from a different repo** is refused:
   "A plugin called <id> is already installed from <other repo>."
6. Themes appear in Appearance at once; a helper program applies to **terminals opened from now on**.

### 5. Updates

- fuxx checks each installed plugin **at launch (in the background) and when Settings → Plugins opens**. No timer,
  no automatic install.
- A newer release tag (or a newer commit on the default branch, for release-less plugins) shows an **Update**
  button on that plugin's row with the new version. Clicking it is the consent; it runs §4's steps 2 and 4.
- If the update newly **runs code** (an update that adds `bin` or `zsh` to a theme plugin), the full confirmation
  with the warning shows again.
- A failed update leaves the installed version untouched.

### 6. Settings → Plugins

A new page in the Preferences panel, after Sound, in M11x's card style:

- **Top card:** a link box ("github.com/owner/repo") and an **Install** button; a line under it while downloading
  ("Downloading…", "Checking…") and for errors.
- **One card per installed plugin:** name and version, `github.com/owner/repo` (click opens it in the browser), the
  description, what it adds ("2 themes", "Runs a program"), and on the right: **Update** (when there is one), an
  **On/Off switch** (only for plugins that run code), **Uninstall**.
- **Off** = the plugin's `bin` and `zsh` aren't loaded into new terminals; its files and data stay. Themes have no
  switch: a theme that's installed is simply in the picker.
- **Empty state:** "No plugins yet. Paste a GitHub link above." and a "How to make a plugin" link (§11).
- **Theme picker** (Appearance): plugin themes join today's one sorted list (no new sections; your themes don't
  move), each tile with its plugin's name in small text under the theme's name. A plugin theme whose name matches
  one you already have (ignoring case, as the list already does) is listed as `<name> (<plugin name>)`; that's
  also the name the `theme =` line stores.

### 7. Where things live

```
~/Library/Application Support/fuxx/
  plugins/
    plugins.json            ← the record: id, repo, installed tag or commit, version, on/off, installed at
    <id>/                   ← the plugin's files, exactly as unpacked
    .staging/               ← downloads in progress; emptied at launch
  plugin-data/
    <id>/                   ← the plugin's own data folder (§8)
  shell-integration/zsh/
    plugins.zsh             ← generated: what fuxx's zsh loads (§9)
```

- `plugins/` is fuxx's, not yours: no hand editing expected (that's what Settings → Plugins is for).
- `plugins.json` is the truth for what's installed. A folder with no record (a crash mid-install) is deleted at
  launch; a record with no folder is dropped.

### 8. Uninstalling, completely

**Uninstall** → a confirmation ("Uninstall Sunset Themes? Its files and its data are deleted.") → fuxx deletes:

1. `plugins/<id>/`
2. `plugin-data/<id>/`
3. its record in `plugins.json`
4. its lines in `plugins.zsh` (regenerated)
5. **settings that point at it:** if the `theme =` line in `~/.config/fuxx/config` names one of its themes (plain,
   or either half of a `light:A,dark:B` pair), the **whole line** is removed and fuxx goes back to its default
   theme, live. (Keeping the other half would silently change your light or dark look; the default is the
   predictable result.) fuxx edits only its main config file: a `theme` line in a file you pulled in with
   `config-file` is yours and is left alone; the missing theme then falls back to the default as any missing
   theme does today.

**The hard rule:** uninstall deletes **only** inside `plugins/<id>/` and `plugin-data/<id>/`, two folders fuxx
created. A plugin can't name any other path to delete, so a bad or hostile plugin can't make uninstall remove your
files. A plugin that writes elsewhere (its own cache in `~/.cache`, say) breaks the format's one rule, "keep your
data in `$FUXX_PLUGIN_DATA`"; fuxx can't see those files, so it can't promise to remove them, and the "How to make
a plugin" page says so.

**Terminals already open** keep a helper program's hook until they're closed (a running shell can't unload a
script). The Plugins page says "Terminals already open keep it until you close them." Hooks must cope with their
files vanishing (fuxx-completes' design already goes quiet when its program is missing).

### 9. How a helper program runs (M11z)

- fuxx writes `plugins.zsh` beside its zsh integration whenever a plugin is installed, updated, uninstalled or
  switched on/off. For each helper that's **on**, in install order:
  - `path=(<plugin>/<bin> $path)` when it has `bin`;
  - `FUXX_PLUGIN_DIR=<plugin folder>` and `FUXX_PLUGIN_DATA=<plugin-data/<id>>` exported while its `zsh` scripts
    are sourced. A script that needs them later keeps its own copy.
- **Loading after your `.zshrc` (new; changes M11l's handoff).** Today fuxx's `.zshenv` hands `ZDOTDIR` back
  before your `.zshrc`, so fuxx has no later step. When a helper plugin is on, fuxx's `.zshenv` instead **keeps
  `ZDOTDIR` on fuxx's folder**, and fuxx adds two pass-through files there:
  - `.zprofile`: sources yours (from your `ZDOTDIR` or `HOME`, only if readable), nothing else;
  - `.zshrc`: puts your `ZDOTDIR` back exactly as it was, sources your `.zshrc`, then sources `plugins.zsh` at top
    level. zsh then reads your `.zlogin` from your own folder as always.
  So plugins load **after your own setup (and after `compinit`) and at top level**, the place zsh plugins expect;
  their plain `typeset`s stay global. (Sourcing them from the first `precmd` was considered and rejected: inside a
  function a plain `typeset` becomes local and vanishes.) With no helper plugin on, the handoff stays exactly as
  M11l built it. M11l's real-zsh tests are extended to cover both paths and your `.zprofile` / `.zlogin` order.
- **Plugins don't depend on `command-blocks`.** fuxx injects its integration when `command-blocks` is on **or** any
  helper plugin is on; the command marks follow `command-blocks` alone. Turning command blocks off no longer turns
  plugins off without a word.
- A script that errors prints its error and the shell goes on; fuxx's own hooks (command blocks) keep working.
- **zsh only**, because zsh is the shell fuxx injects into. bash and fish terminals in fuxx don't get helper
  programs.
- fuxx starts no plugin process itself. The plugin's hook starts what it needs (fuxx-completes' adapter starts its
  `serve` process) and the program lives and dies with the shell.

### 10. Changes to fuxx-completes (its own repo)

This replaces its design's "fuxx links the crates directly" (§2 goal 6, §4, §8):

- fuxx **installs fuxx-completes as a plugin** like anyone else's. Inside fuxx the zsh adapter draws the ghost text
  itself, as it does in any terminal. fuxx draws nothing and links no crate.
- fuxx-completes gains: a `fuxx-plugin.conf` (above); release assets `fuxx-plugin-macos-arm64.zip` and
  `fuxx-plugin-macos-x86_64.zip` (binary in `bin/`, adapter in `shell/`) from its release workflow; and **when
  `FUXX_PLUGIN_DATA` is set, its context file lives there** instead of `~/.local/share/fuxx-completes/`, so
  uninstalling from fuxx removes it. A copy installed for other terminals keeps its own data.
- **A double-load guard is added to fuxx-completes' design** (it has none today; its §9 table covers other plugins,
  not a second copy of itself): if you also load fuxx-completes from your `.zshrc`, the copy that loads second
  sees the first and stays off, so one copy runs.
- **fuxx drawing a nicer menu** (the host channel, OSC 7701) is **not in M11z**. It needs fuxx-completes' M2 and
  M6, and a plugin API for drawing that format 1 doesn't have. It comes back with the later "screens" round.
- **M11z doesn't wait for fuxx-completes.** M11z is finished and checked with a small **test helper plugin** (a
  real public repo with a release, a tiny program and a zsh hook). fuxx-completes' M0 and its plugin packaging are
  built in its own repo, on its own roadmap; installing it in fuxx is then a check, not new fuxx work.

### 11. Making a plugin (for your designers)

- A short **"How to make a plugin"** page in fuxx's GitHub README section (linked from the empty state): a theme
  plugin is a repo with `fuxx-plugin.conf` and a `themes/` folder of fuxx theme files; push it, share the link.
  The two examples above, copy-paste ready.
- **Not in this round:** an "Export my colours as a plugin" button in Preferences. A good later step for designers,
  but not part of what you asked for now.

### 12. Testing

- Unit tests (temp `HOME`, no network): link parsing; manifest parsing and every refusal (missing key, bad `id`,
  `format = 2`, path escaping with `..` or a symlink); choosing the asset for a chip; release-vs-branch choice;
  install → record → uninstall leaves **nothing** under both folders and touches nothing outside them; the active
  theme falling back on uninstall; theme name clashes; `plugins.zsh` generation for on/off/uninstalled helpers;
  launch cleanup of half-installed folders.
- The network sits behind one small fetch function that's passed in, so tests give it a fake (new: `updater.rs`
  calls curl directly and tests only its parsing; this milestone doesn't change the updater).
- A hidden `fuxx __plugin-probe` installs a fixture plugin from a local tarball and a test helper whose hook sets a
  variable, then checks a fresh zsh sees it, and that uninstall clears both.
- GUI behaviour (the page, the confirmation, the theme appearing live) is your on-screen check, as usual.

## Not in it

- Screens: sidebar widgets, side/bottom panels, plugin tabs (pet game, world clock, stocks, newsletter, a GBA
  emulator), a JavaScript API. Later, its own design.
- fuxx drawing fuxx-completes' menu (host channel).
- Private repos, GitHub sign-in, sources other than GitHub.
- Automatic updates, update timers, a plugin store or search, ratings.
- Sandboxing or a permission system for helper programs.
- Plugin settings pages inside Preferences (a plugin keeps its own settings).
- bash and fish hooks.
- An "Export as plugin" button.

## Milestones

| | Milestone | Done when |
| --- | --- | --- |
| **M11y** | Plugins and theme plugins | The format (§2), install from a link (§3–§4), updates (§5), Settings → Plugins (§6), the record and folders (§7), complete uninstall (§8), theme plugins in the picker; tests (§12) pass; your on-screen check: install a theme repo, use its theme, uninstall, the theme falls back and nothing is left on disk. |
| **M11z** | Helper-program plugins | `bin`/`zsh` loading through `plugins.zsh` after your `.zshrc` (§9), injection no longer tied to `command-blocks`, the On/Off switch, the "runs a program" warning; your on-screen check with the test helper plugin: install, it works in a new terminal, switch it off, uninstall, nothing left. Installing fuxx-completes is a later check, once its repo has a release (§10). |

## Risks

- **Running other people's code.** A helper plugin can do anything you can. The install warning is the only guard,
  stated honestly. Themes carry no such risk.
- **macOS blocking downloaded programs.** curl doesn't add the quarantine flag that Gatekeeper checks, and Rust
  binaries for Apple Silicon are ad-hoc signed by the linker, so they should run; the probe confirms it on your Mac.
- **GitHub's limit:** 60 unauthenticated API requests an hour. Two checks per plugin at launch is fine for a
  handful of plugins; a failed check just shows no Update button.
- **A plugin's zsh script can slow or break the prompt.** fuxx can't prevent it; Off and Uninstall are the fix.
- **fuxx-completes isn't built yet.** M11z closes on a test helper; fuxx-completes in fuxx waits for its own M0
  and packaging.
- **Keeping `ZDOTDIR` through `.zprofile` and `.zshrc` (§9)** touches the most delicate part of M11l: a mistake
  there could skip your own dotfiles. It's off unless a helper plugin is on, and it gets real-zsh tests for login
  and non-login shells, with and without your own `ZDOTDIR`.

## M11y on-screen check

On branch `m11y-plugins`. Run fuxx, open Settings → Plugins, and check each:

1. Settings → Plugins shows the empty state and the "How to make a plugin" link.
2. A bad link shows the hint.
3. Pasting the sample repo (or any theme repo you pick) shows the confirmation with "Adds 2 themes: Sunset, Sunset Light".
4. Install: both themes appear in Appearance with "Sunset Themes" under them.
5. Choose Sunset: it applies live.
6. Uninstall → confirmation → fuxx returns to the default theme live, the `theme` line is gone from `~/.config/fuxx/config`, and `ls ~/Library/Application\ Support/fuxx/plugins` shows only `plugins.json`.
7. After a push to the sample repo, reopening Settings → Plugins shows "Update to …".

The sample plugin lives in `examples/plugins/sunset-themes`; an automated test installs and uninstalls it
against a fake network (`example_plugin_installs_and_uninstalls_cleanly`).
