# Milestone 11s — Key Sounds

**Status:** 🟨 built on master 2026-11-10; waiting for your on-screen (and on-ear) check.
**Asked for:** 2026-11-10, by you: "disetting tambah 1 tab namanya sound. didalam ada daftar key seperti Enter dan
ctrl + c. dan tiap key ada field text disampingnya. text field akan disimpan dan berupa link sound. fungsinya jika
user tekan enter, akan bunyi sound sesuai link. sekarang 2 key dulu dalam daftar." Plan: "New milestone, next"
(before M11p). Link: "Either one" (a file or a web link). Design: "ok setuju, lanjut kerjakan sound tab".
**Plan:** [[M11s Plan - Key Sounds]].

## What it does

- **Settings → Sound**, a new section after Browser: a list of keys, **Enter** and **Ctrl + C** for now, each with a
  text field and a ▶ button that plays it. An empty field: no sound for that key.
- **When it plays:** a press of that key in a terminal (what goes to the shell or the agent), not typing in
  Settings, a rename box or the ⌘K/⌘P list. A held key plays once (no repeats); a new press restarts the sound.
- **The link** is either a **file** (`~/Sounds/click.wav`, `/System/Library/Sounds/Tink.aiff`) or a **web link**
  (`https://…`). A web link is downloaded once in the background into `~/Library/Caches/fuxx/sounds/` and the copy is
  played from then on. A problem (no such file, the download failed, not a sound) shows under the field, and that key
  stays silent.
- **Saved** in the settings file as `sound-enter = …` and `sound-ctrl-c = …`; a change works at once.

## AI's calls

1. The field is saved on Enter or when it loses focus; Escape puts back what was saved.
2. Played with macOS's `NSSound` (AppKit, already a dependency: one more feature of `objc2-app-kit`, no new crate),
   loaded into memory once per link, so a keypress only plays it.
3. Downloads use `/usr/bin/curl` as the updater does (20 s at most); the cached file is named from the link.
4. The key is read from the bytes sent (`\r` is Enter, `0x03` is Ctrl + C), so it doesn't matter which Enter key.

## Not in it

Other keys, volume, a sound per agent or per status, sounds for the app's own shortcuts.

## As built (2026-11-10)

- `src/sounds.rs`: `SoundKey` (Enter, Ctrl + C), `key_for_bytes`, `Link::parse`, `cache_file`, `cache_dir`,
  `download_args`, `values`; the `Sounds` global (`sync` at launch and on every settings change, `load` with
  `NSSound initWithContentsOfFile:byReference:NO`, `fetch` with curl off the UI thread into a `.part` then renamed,
  dropped if the link changed meanwhile, `play` stops and restarts, `press`, `problem`).
- `src/appearance.rs`: `sound_links` from `sound-enter` / `sound-ctrl-c` (the last line; blank clears).
- `src/terminal_view.rs`: a key press (not a held key's repeat) plays its sound before its bytes go to the terminal.
- `src/preferences.rs`: Section **Sound** after Browser; `Control::Sound(key)`: a `NameInput` field (saved on Enter
  or losing focus; Escape puts it back), ▶, and under it the problem, "Downloading…", or a hint while empty. Listed in
  ⌘K as "Sound for Enter" / "Sound for Ctrl + C".
- `Cargo.toml`: `objc2-app-kit`'s `NSSound` feature (no new crate).
- Tests: 1351 pass (new: keys from bytes, links, the cache file's name, curl's arguments, blank lines, a macOS
  sound loads and a missing or non-sound file says why, the settings lines). ui-shots: `preferences-sound`.
- Not checked by machine: hearing it, and a real web download (you check those).

## Added after the design (2026-11-11, your calls)

- **An On / Off switch per key** ("di sound buatkan juga switchernya per action untuk on/off"): Off silences that key
  and keeps its link (▶ still plays it, to try it). Saved as `sound-enter-on = off` / `sound-ctrl-c-on = off` (On, the
  default, writes no line). The row is wider: switch, field, ▶.
- **⌘V pastes into the fields** ("textfield di settings > sound, tidak bisa paste"): ⌘V for a box was bound only in
  the palette, branch switcher and address bars; Settings now has its own key context (`Preferences > NameInput`).

## On-screen checklist (for you)

1. Settings has a Sound section with Enter and Ctrl + C, each with a field and ▶.
2. A file path plays on ▶, and on Enter in a terminal.
3. A web link (an mp3 or wav) plays after a moment, and from then on at once (also after a restart).
4. A wrong path or link says so under the field; the key stays silent.
5. Holding Enter plays once; typing in Settings or the rename box plays nothing.
6. Clearing the field stops the sound for that key.
7. Off silences the key (▶ still plays); On brings it back without retyping.
8. ⌘V pastes a link into the field.
