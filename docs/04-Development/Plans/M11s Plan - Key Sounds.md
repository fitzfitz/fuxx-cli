# M11s Plan — Key Sounds

> Executed natively on master (as M11r). Design: [[70 - Milestone 11s - Key Sounds]].

**Goal:** Settings → Sound with Enter and Ctrl + C, each a link (file or web) that plays when that key is pressed in
a terminal.

**Architecture:** `src/sounds.rs`: pure parts (which key a press is, what a link is, the cache file's name, the
settings lines) and a `Sounds` global that loads each link into an `NSSound` (downloading a web one first with curl,
off the UI thread) and plays it. `appearance.rs` reads `sound-enter` / `sound-ctrl-c`; a reload re-syncs `Sounds`.
`terminal_view.rs` plays on a key press (not a repeat). `preferences.rs` gets a Sound section: a `NameInput` per key,
▶, and the problem under it.

## Global constraints

- No new crate: `objc2-app-kit`'s `NSSound` feature only.
- No work on a keypress but `play` (loaded once per link); downloads off the UI thread.
- Settings written through `preferences::apply` (one key's lines rewritten).

## Review focus

1. A link with spaces or `~`: the path is expanded and trimmed.
2. A web link that fails or isn't a sound: the message shows, the key is silent, the UI never blocks.
3. Changing a link while it downloads: the old download's result is dropped.
4. A held key: one sound.
5. Empty field: the line is removed from the settings file.

## Tasks

1. `sounds.rs` pure parts, test first: `SoundKey::{ALL, key, label}`, `key_for_bytes`, `Link::parse`, `cache_file`.
2. `appearance.rs`: `sounds: [String; 2]` from the entries, test first.
3. `Sounds` global: sync on load and reload, download, `NSSound` load, `play`; the press in `terminal_view.rs`.
4. Preferences → Sound: rows, field, ▶, status; ui-shot scene `preferences-sound`.
5. Verify (tests, fmt, clippy, ui-shots), docs (note 70 "As built", Roadmap, Scope, tracker, Progress Log).
