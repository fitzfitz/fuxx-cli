//! Translate a GPUI keystroke into the bytes a terminal program expects.
//!
//! Pure function, no GPUI types, so it is unit-testable. The caller skips cmd (platform)
//! shortcuts, which belong to the app, and passes `alt` only for named keys (arrows, enter, ...);
//! for character keys macOS already composed the Option character into `key_char`.
//! Byte table follows Zed's `crates/terminal/src/mappings/keys.rs` (xterm conventions).

pub fn key_to_bytes(
    key: &str,
    key_char: Option<&str>,
    ctrl: bool,
    alt: bool,
    shift: bool,
    app_cursor: bool,
) -> Option<Vec<u8>> {
    let none = !ctrl && !alt && !shift;
    let only_ctrl = ctrl && !alt && !shift;
    let only_alt = alt && !ctrl && !shift;
    let only_shift = shift && !ctrl && !alt;
    let s = |b: &[u8]| Some(b.to_vec());

    // 1. Keys with fixed meanings per exact modifier combination.
    match key {
        "tab" if none => return s(b"\t"),
        "tab" if only_shift => return s(b"\x1b[Z"),
        "escape" if none => return s(b"\x1b"),
        "enter" if none => return s(b"\r"),
        "enter" if only_shift => return s(b"\n"),
        "enter" if only_alt => return s(b"\x1b\r"),
        "backspace" if none || only_shift => return s(b"\x7f"),
        "backspace" if only_ctrl => return s(b"\x08"),
        "backspace" if only_alt => return s(b"\x1b\x7f"),
        "space" if only_ctrl => return s(b"\x00"),
        _ => {}
    }

    // 2. Unmodified navigation / function keys. Arrows + home/end switch to SS3 form in
    //    application-cursor mode (vim, less, Claude Code's TUI set it).
    if none {
        let ss3 = app_cursor;
        let seq: Option<&[u8]> = match key {
            "up" => Some(if ss3 { b"\x1bOA" } else { b"\x1b[A" }),
            "down" => Some(if ss3 { b"\x1bOB" } else { b"\x1b[B" }),
            "right" => Some(if ss3 { b"\x1bOC" } else { b"\x1b[C" }),
            "left" => Some(if ss3 { b"\x1bOD" } else { b"\x1b[D" }),
            "home" => Some(if ss3 { b"\x1bOH" } else { b"\x1b[H" }),
            "end" => Some(if ss3 { b"\x1bOF" } else { b"\x1b[F" }),
            "insert" => Some(b"\x1b[2~"),
            "delete" => Some(b"\x1b[3~"),
            "pageup" => Some(b"\x1b[5~"),
            "pagedown" => Some(b"\x1b[6~"),
            "f1" => Some(b"\x1bOP"),
            "f2" => Some(b"\x1bOQ"),
            "f3" => Some(b"\x1bOR"),
            "f4" => Some(b"\x1bOS"),
            "f5" => Some(b"\x1b[15~"),
            "f6" => Some(b"\x1b[17~"),
            "f7" => Some(b"\x1b[18~"),
            "f8" => Some(b"\x1b[19~"),
            "f9" => Some(b"\x1b[20~"),
            "f10" => Some(b"\x1b[21~"),
            "f11" => Some(b"\x1b[23~"),
            "f12" => Some(b"\x1b[24~"),
            _ => None,
        };
        if let Some(seq) = seq {
            return Some(seq.to_vec());
        }
    }

    // 3. Ctrl + single character -> C0 control byte (ctrl-c = 0x03). Case-insensitive because
    //    macOS reports ctrl-shift-a as key "a" + shift.
    if ctrl && !alt && key.chars().count() == 1 {
        let c = key.chars().next().unwrap();
        let b = match c {
            'a'..='z' => Some(c as u8 - b'a' + 1),
            'A'..='Z' => Some(c as u8 - b'A' + 1),
            '@' => Some(0x00),
            '[' => Some(0x1b),
            '\\' => Some(0x1c),
            ']' => Some(0x1d),
            '^' => Some(0x1e),
            '_' => Some(0x1f),
            '?' => Some(0x7f),
            _ => None,
        };
        if let Some(b) = b {
            return Some(vec![b]);
        }
    }

    // 4. Modified navigation / function keys: CSI with modifier code m = 1 + shift + 2*alt + 4*ctrl.
    if ctrl || alt || shift {
        let m = 1 + (shift as u8) + ((alt as u8) << 1) + ((ctrl as u8) << 2);
        let seq = match key {
            "up" => Some(format!("\x1b[1;{m}A")),
            "down" => Some(format!("\x1b[1;{m}B")),
            "right" => Some(format!("\x1b[1;{m}C")),
            "left" => Some(format!("\x1b[1;{m}D")),
            "home" => Some(format!("\x1b[1;{m}H")),
            "end" => Some(format!("\x1b[1;{m}F")),
            "f1" => Some(format!("\x1b[1;{m}P")),
            "f2" => Some(format!("\x1b[1;{m}Q")),
            "f3" => Some(format!("\x1b[1;{m}R")),
            "f4" => Some(format!("\x1b[1;{m}S")),
            "f5" => Some(format!("\x1b[15;{m}~")),
            "f6" => Some(format!("\x1b[17;{m}~")),
            "f7" => Some(format!("\x1b[18;{m}~")),
            "f8" => Some(format!("\x1b[19;{m}~")),
            "f9" => Some(format!("\x1b[20;{m}~")),
            "f10" => Some(format!("\x1b[21;{m}~")),
            "f11" => Some(format!("\x1b[23;{m}~")),
            "f12" => Some(format!("\x1b[24;{m}~")),
            "insert" => Some(format!("\x1b[2;{m}~")),
            "delete" => Some(format!("\x1b[3;{m}~")),
            "pageup" => Some(format!("\x1b[5;{m}~")),
            "pagedown" => Some(format!("\x1b[6;{m}~")),
            _ => None,
        };
        if let Some(seq) = seq {
            return Some(seq.into_bytes());
        }
    }

    // 5. Alt as Meta: ESC prefix (only reached for named keys, see module docs).
    if alt && !ctrl {
        let base = if key == "space" { " " } else { key };
        if base.len() == 1 && base.is_ascii() {
            let ch = if shift {
                base.to_ascii_uppercase()
            } else {
                base.to_string()
            };
            return Some(format!("\x1b{ch}").into_bytes());
        }
    }

    // 6. Plain text: whatever character macOS produced (handles shift, symbols, Option chars).
    if !ctrl {
        if let Some(t) = key_char {
            if !t.is_empty() {
                return Some(t.as_bytes().to_vec());
            }
        }
    }
    None
}

/// Bytes to send for pasted text. Bracketed: wrapped in `ESC[200~ ... ESC[201~` with every ESC
/// removed, so the text can't end the paste early. Plain: line breaks become Enter (`\r`).
/// The typed text with Caps Lock applied. GPUI's macOS key_char only accounts for shift and
/// option, so a letter typed with Caps Lock on arrives lowercase. As on macOS, Caps Lock
/// capitalises letters only (shift + caps lock is still a capital); a letter whose capital isn't
/// a single character (ß) is left as typed.
pub fn with_capslock(text: &str, on: bool) -> String {
    if !on {
        return text.to_string();
    }
    text.chars()
        .map(|c| {
            let mut upper = c.to_uppercase();
            match (upper.next(), upper.next()) {
                (Some(u), None) => u,
                _ => c,
            }
        })
        .collect()
}

pub fn paste_bytes(text: &str, bracketed: bool) -> Vec<u8> {
    if bracketed {
        let mut out = b"\x1b[200~".to_vec();
        out.extend(text.replace('\x1b', "").as_bytes());
        out.extend(b"\x1b[201~");
        out
    } else {
        text.replace("\r\n", "\r").replace('\n', "\r").into_bytes()
    }
}

/// Mac line/word editing keys that readline-style shells understand, sent as Ghostty and Zed do.
/// Matches only when exactly that one modifier is held; everything else is `None`.
pub fn mac_shortcut(
    key: &str,
    cmd: bool,
    alt: bool,
    ctrl: bool,
    shift: bool,
) -> Option<&'static [u8]> {
    if ctrl || shift || (cmd && alt) {
        return None;
    }
    match (key, cmd, alt) {
        ("left", true, false) => Some(b"\x01"), // cmd-left: line start (^A)
        ("right", true, false) => Some(b"\x05"), // cmd-right: line end (^E)
        ("backspace", true, false) => Some(b"\x15"), // cmd-backspace: kill line (^U)
        ("left", false, true) => Some(b"\x1bb"), // option-left: word back (ESC b)
        ("right", false, true) => Some(b"\x1bf"), // option-right: word forward (ESC f)
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(key: &str, key_char: Option<&str>) -> Option<Vec<u8>> {
        key_to_bytes(key, key_char, false, false, false, false)
    }

    #[test]
    fn capslock_capitalises_letters_only() {
        assert_eq!(with_capslock("a", true), "A");
        assert_eq!(with_capslock("A", true), "A"); // shift + caps lock is still a capital on macOS
        assert_eq!(with_capslock("é", true), "É");
        assert_eq!(with_capslock("1", true), "1");
        assert_eq!(with_capslock(";", true), ";");
        assert_eq!(with_capslock("ß", true), "ß"); // no single-letter capital: left as typed
        assert_eq!(with_capslock("a", false), "a");
    }

    #[test]
    fn printable_chars_pass_through() {
        assert_eq!(k("a", Some("a")), Some(b"a".to_vec()));
        assert_eq!(k("a", Some("A")), Some(b"A".to_vec())); // shift-a
        assert_eq!(k("!", Some("!")), Some(b"!".to_vec()));
        assert_eq!(k("s", Some("ß")), Some("ß".as_bytes().to_vec())); // option-s
    }

    #[test]
    fn enter_backspace_tab_escape() {
        assert_eq!(k("enter", Some("\n")), Some(b"\r".to_vec()));
        assert_eq!(k("backspace", None), Some(b"\x7f".to_vec()));
        assert_eq!(k("tab", Some("\t")), Some(b"\t".to_vec()));
        assert_eq!(k("escape", None), Some(b"\x1b".to_vec()));
        assert_eq!(
            key_to_bytes("tab", Some("\t"), false, false, true, false),
            Some(b"\x1b[Z".to_vec())
        );
    }

    #[test]
    fn arrows_respect_app_cursor_mode() {
        assert_eq!(k("up", None), Some(b"\x1b[A".to_vec()));
        assert_eq!(
            key_to_bytes("up", None, false, false, false, true),
            Some(b"\x1bOA".to_vec())
        );
        assert_eq!(
            key_to_bytes("left", None, false, false, false, true),
            Some(b"\x1bOD".to_vec())
        );
    }

    #[test]
    fn ctrl_letters_become_control_bytes() {
        assert_eq!(
            key_to_bytes("c", None, true, false, false, false),
            Some(vec![0x03])
        );
        assert_eq!(
            key_to_bytes("d", None, true, false, false, false),
            Some(vec![0x04])
        );
        assert_eq!(
            key_to_bytes("a", None, true, false, true, false),
            Some(vec![0x01])
        ); // ctrl-shift-a
        assert_eq!(
            key_to_bytes("[", None, true, false, false, false),
            Some(vec![0x1b])
        );
    }

    #[test]
    fn modified_navigation_keys_use_csi_modifier_codes() {
        assert_eq!(
            key_to_bytes("up", None, true, false, false, false),
            Some(b"\x1b[1;5A".to_vec())
        );
        assert_eq!(
            key_to_bytes("left", None, false, true, false, false),
            Some(b"\x1b[1;3D".to_vec())
        );
        assert_eq!(
            key_to_bytes("f5", None, false, false, true, false),
            Some(b"\x1b[15;2~".to_vec())
        );
    }

    #[test]
    fn alt_prefixes_escape_for_named_keys() {
        assert_eq!(
            key_to_bytes("backspace", None, false, true, false, false),
            Some(b"\x1b\x7f".to_vec())
        );
        assert_eq!(
            key_to_bytes("space", Some(" "), false, true, false, false),
            Some(b"\x1b ".to_vec())
        );
    }

    #[test]
    fn unmapped_combinations_send_nothing() {
        assert_eq!(
            key_to_bytes("enter", Some("\n"), true, false, false, false),
            None
        );
        assert_eq!(k("unknown-key", None), None);
    }

    #[test]
    fn bracketed_paste_wraps_and_strips_escape() {
        assert_eq!(
            paste_bytes("a\x1b[201~b\nc", true),
            b"\x1b[200~a[201~b\nc\x1b[201~".to_vec()
        );
    }

    #[test]
    fn plain_paste_turns_newlines_into_enter() {
        assert_eq!(paste_bytes("a\r\nb\nc", false), b"a\rb\rc".to_vec());
    }

    #[test]
    fn mac_line_and_word_shortcuts() {
        assert_eq!(
            mac_shortcut("left", true, false, false, false),
            Some(&b"\x01"[..])
        );
        assert_eq!(
            mac_shortcut("right", true, false, false, false),
            Some(&b"\x05"[..])
        );
        assert_eq!(
            mac_shortcut("backspace", true, false, false, false),
            Some(&b"\x15"[..])
        );
        assert_eq!(
            mac_shortcut("left", false, true, false, false),
            Some(&b"\x1bb"[..])
        );
        assert_eq!(
            mac_shortcut("right", false, true, false, false),
            Some(&b"\x1bf"[..])
        );
        assert_eq!(mac_shortcut("left", true, false, false, true), None); // cmd-shift-left: not ours
        assert_eq!(mac_shortcut("up", true, false, false, false), None); // cmd-up scrolls (binding)
        assert_eq!(mac_shortcut("left", false, false, false, false), None);
    }
}
