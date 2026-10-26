//! Terminal colors from the user's Ghostty config: the `Theme` struct, color parsing, and theme
//! file resolution.
//!
//! Pure: no filesystem access here. `resolve` takes a `read_theme` closure and `theme_dirs` only
//! builds paths. The rules come from Ghostty 1.3.1: theme files are plain config files (their
//! `theme` and `config-file` keys are ignored), the user's own values override the theme's, an
//! empty value resets a key, and `palette = N=COLOR` takes a decimal or `0x`/`0o`/`0b` index.

use crate::ghostty_config::{Entry, parse};
use alacritty_terminal::vte::ansi::Rgb;
use std::path::{Path, PathBuf};

const fn hex(v: u32) -> Rgb {
    Rgb {
        r: (v >> 16) as u8,
        g: (v >> 8) as u8,
        b: v as u8,
    }
}

const DEFAULT_BACKGROUND: Rgb = hex(0x282c34);
const DEFAULT_FOREGROUND: Rgb = hex(0xffffff);
const DEFAULT_PALETTE: [Rgb; 16] = [
    hex(0x1d1f21),
    hex(0xcc6666),
    hex(0xb5bd68),
    hex(0xf0c674),
    hex(0x81a2be),
    hex(0xb294bb),
    hex(0x8abeb7),
    hex(0xc5c8c6),
    hex(0x666666),
    hex(0xd54e53),
    hex(0xb9ca4a),
    hex(0xe7c547),
    hex(0x7aa6da),
    hex(0xc397d8),
    hex(0x70c0b1),
    hex(0xeaeaea),
];

/// The colors fuxx draws the terminal with. `None` means "derive it from the other colors".
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    pub background: Rgb,
    pub foreground: Rgb,
    pub cursor: Option<Rgb>,
    pub cursor_text: Option<Rgb>,
    pub selection_background: Option<Rgb>,
    pub selection_foreground: Option<Rgb>,
    pub palette: [Rgb; 16],
}

impl Default for Theme {
    fn default() -> Self {
        Theme {
            background: DEFAULT_BACKGROUND,
            foreground: DEFAULT_FOREGROUND,
            cursor: None,
            cursor_text: None,
            selection_background: None,
            selection_foreground: None,
            palette: DEFAULT_PALETTE,
        }
    }
}

/// `#rrggbb` or `rrggbb`, any case. Everything else (names, 3-digit hex) is `None`.
pub fn parse_color(s: &str) -> Option<Rgb> {
    let s = s.trim();
    let s = s.strip_prefix('#').unwrap_or(s);
    if s.len() != 6 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(s, 16).ok().map(hex)
}

/// A palette index: decimal, or with a `0x`/`0o`/`0b` prefix.
fn parse_index(s: &str) -> Option<usize> {
    let s = s.trim();
    let (digits, radix) = if let Some(r) = s.strip_prefix("0x") {
        (r, 16)
    } else if let Some(r) = s.strip_prefix("0o") {
        (r, 8)
    } else if let Some(r) = s.strip_prefix("0b") {
        (r, 2)
    } else {
        (s, 10)
    };
    usize::from_str_radix(digits, radix).ok()
}

fn skip(key: &str, value: &str) {
    eprintln!("[fuxx] config: ignoring bad value for `{key}`: {value:?}");
}

/// Set a required color: empty resets to `default`, a bad value is skipped.
fn set_color(slot: &mut Rgb, e: &Entry, default: Rgb) {
    if e.value.is_empty() {
        *slot = default;
    } else if let Some(c) = parse_color(&e.value) {
        *slot = c;
    } else {
        skip(&e.key, &e.value);
    }
}

/// Set an optional color: empty or `cell-*` clears it, a bad value is skipped.
fn set_optional(slot: &mut Option<Rgb>, e: &Entry) {
    if e.value.is_empty() || e.value == "cell-foreground" || e.value == "cell-background" {
        *slot = None;
    } else if let Some(c) = parse_color(&e.value) {
        *slot = Some(c);
    } else {
        skip(&e.key, &e.value);
    }
}

/// Apply the color keys of `entries` to `theme`, in order. Other keys are ignored.
pub fn apply(theme: &mut Theme, entries: &[Entry]) {
    for e in entries {
        match e.key.as_str() {
            "background" => set_color(&mut theme.background, e, DEFAULT_BACKGROUND),
            "foreground" => set_color(&mut theme.foreground, e, DEFAULT_FOREGROUND),
            "cursor-color" => set_optional(&mut theme.cursor, e),
            "cursor-text" => set_optional(&mut theme.cursor_text, e),
            "selection-background" => set_optional(&mut theme.selection_background, e),
            "selection-foreground" => set_optional(&mut theme.selection_foreground, e),
            "palette" => {
                if e.value.is_empty() {
                    theme.palette = DEFAULT_PALETTE;
                    continue;
                }
                let parsed = e
                    .value
                    .split_once('=')
                    .and_then(|(n, c)| Some((parse_index(n)?, parse_color(c)?)));
                match parsed {
                    Some((n, c)) if n < 16 => theme.palette[n] = c,
                    Some(_) => {} // 16..=255 are not stored
                    None => skip(&e.key, &e.value),
                }
            }
            _ => {}
        }
    }
}

/// Pick the theme name from a `theme` value: plain, or `light:A,dark:B` in either order.
fn pick_theme_name(value: &str, dark: bool) -> Option<String> {
    let want = if dark { "dark" } else { "light" };
    if value.contains("light:") || value.contains("dark:") {
        value.split(',').find_map(|part| {
            let (k, v) = part.split_once(':')?;
            (k.trim() == want).then(|| v.trim().to_string())
        })
    } else {
        Some(value.trim().to_string())
    }
}

/// Defaults, then the named theme (if any), then the user's own entries on top.
pub fn resolve(
    entries: &[Entry],
    dark: bool,
    read_theme: &dyn Fn(&str) -> Option<String>,
) -> Theme {
    let mut theme = Theme::default();
    let name = entries
        .iter()
        .rev()
        .find(|e| e.key == "theme")
        .and_then(|e| pick_theme_name(&e.value, dark))
        .filter(|n| !n.is_empty());
    if let Some(name) = name {
        let path_like = name.contains('/') && !Path::new(&name).is_absolute();
        let text = if path_like || name == "." || name == ".." {
            None
        } else {
            read_theme(&name)
        };
        match text {
            Some(text) => {
                let inner: Vec<Entry> = parse(&text)
                    .into_iter()
                    .filter(|e| e.key != "theme" && e.key != "config-file")
                    .collect();
                apply(&mut theme, &inner);
            }
            None => eprintln!("[fuxx] config: theme {name:?} not found, using defaults"),
        }
    }
    apply(&mut theme, entries);
    theme
}

/// Directories to look for a named theme in, in lookup order.
pub fn theme_dirs(
    home: &Path,
    xdg_config_home: Option<&Path>,
    resources_dir: Option<&Path>,
) -> Vec<PathBuf> {
    let config = xdg_config_home
        .map(Path::to_path_buf)
        .unwrap_or_else(|| home.join(".config"));
    let mut dirs = vec![config.join("ghostty/themes")];
    if let Some(r) = resources_dir {
        dirs.push(r.join("themes"));
    }
    dirs.push(PathBuf::from(
        "/Applications/Ghostty.app/Contents/Resources/ghostty/themes",
    ));
    dirs.push(home.join("Applications/Ghostty.app/Contents/Resources/ghostty/themes"));
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ghostty_config::Entry;
    use std::path::{Path, PathBuf};

    const MOCHA: &str = "palette = 1=#f38ba8\nbackground = #1e1e2e\nforeground = #cdd6f4\ncursor-color = #f5e0dc\nselection-background = #585b70\ntheme = Nested\nconfig-file = evil";
    fn entries(text: &str) -> Vec<Entry> {
        crate::ghostty_config::parse(text)
    }
    fn read(name: &str) -> Option<String> {
        (name == "Catppuccin Mocha").then(|| MOCHA.to_string())
    }

    #[test]
    fn defaults_are_ghosttys() {
        let t = Theme::default();
        assert_eq!(
            (t.background, t.foreground),
            (
                parse_color("#282c34").unwrap(),
                parse_color("ffffff").unwrap()
            )
        );
        assert_eq!(t.palette[1], parse_color("cc6666").unwrap());
        assert_eq!(t.palette[15], parse_color("eaeaea").unwrap());
    }

    #[test]
    fn a_named_theme_loads_and_user_values_override_it() {
        let t = resolve(
            &entries("theme = Catppuccin Mocha\nbackground = #000000"),
            true,
            &read,
        );
        assert_eq!(t.background, parse_color("000000").unwrap()); // the user's own value wins
        assert_eq!(t.foreground, parse_color("cdd6f4").unwrap()); // the theme fills in the rest
        assert_eq!(t.palette[1], parse_color("f38ba8").unwrap());
        assert_eq!(t.cursor, parse_color("f5e0dc"));
    }

    #[test]
    fn light_dark_pair_picks_by_appearance() {
        let e = entries("theme = light:Nope,dark:Catppuccin Mocha");
        assert_eq!(
            resolve(&e, true, &read).background,
            parse_color("1e1e2e").unwrap()
        );
        assert_eq!(resolve(&e, false, &read), Theme::default()); // "Nope" missing → defaults
    }

    #[test]
    fn bad_values_are_skipped() {
        let t = resolve(
            &entries(
                "background = navy\nforeground = #12345\npalette = 300=#ffffff\npalette = x=#ffffff\npalette = 0x2=#010203\ncursor-color = cell-foreground",
            ),
            true,
            &read,
        );
        assert_eq!(
            (t.background, t.foreground),
            (Theme::default().background, Theme::default().foreground)
        );
        assert_eq!(t.palette[2], parse_color("010203").unwrap());
        assert_eq!(t.cursor, None);
    }

    #[test]
    fn empty_value_resets_and_missing_theme_keeps_defaults() {
        let t = resolve(
            &entries("background = #000000\nbackground =\ntheme = Nope"),
            true,
            &read,
        );
        assert_eq!(t, Theme::default());
    }

    #[test]
    fn theme_dirs_search_user_then_ghostty_app() {
        let d = theme_dirs(Path::new("/h"), None, Some(Path::new("/r")));
        assert_eq!(d[0], PathBuf::from("/h/.config/ghostty/themes"));
        assert_eq!(d[1], PathBuf::from("/r/themes"));
        assert_eq!(
            d[2],
            PathBuf::from("/Applications/Ghostty.app/Contents/Resources/ghostty/themes")
        );
    }
}
