//! Preferences window choices: what the lists show and which settings lines get written.
//!
//! Pure functions, no GPUI types, so they are unit-tested. The window feeds its state in.

use alacritty_terminal::vte::ansi::Rgb;

use crate::appearance::{CursorFun, CursorShapeSetting};
use crate::fonts;
use crate::settings::Entry;
use crate::theme::{self, BUILTIN_THEMES, Theme};

/// First list entry: leave the setting out of the file and use the built-in default.
pub const DEFAULT_CHOICE: &str = "Default";

/// Sort case-insensitively (stable), drop case-variants (first spelling wins) and any `Default`,
/// then put `DEFAULT_CHOICE` first.
fn finish(mut names: Vec<String>) -> Vec<String> {
    names.retain(|n| !n.eq_ignore_ascii_case(DEFAULT_CHOICE));
    names.sort_by_key(|n| n.to_lowercase());
    names.dedup_by_key(|n| n.to_lowercase());
    names.insert(0, DEFAULT_CHOICE.to_string());
    names
}

/// `Default`, then user, plugin and built-in theme names, sorted case-insensitively.
/// Case-variants collapse to one row and the user's spelling wins (user themes are listed first,
/// then plugin themes, which `plugin_themes::list` already told apart from the others).
pub fn theme_choices(user_themes: &[String], plugin_themes: &[String]) -> Vec<String> {
    finish(
        user_themes
            .iter()
            .chain(plugin_themes)
            .cloned()
            .chain(BUILTIN_THEMES.iter().map(|s| s.to_string()))
            .collect(),
    )
}

/// `Default`, then installed monospace families (system-private `.` names skipped) and `current`.
/// `current` matches an installed family regardless of case; the installed spelling wins.
pub fn font_choices(
    installed: &[String],
    is_mono: &dyn Fn(&str) -> bool,
    current: Option<&str>,
) -> Vec<String> {
    let mut names: Vec<String> = installed
        .iter()
        .filter(|n| !n.starts_with('.') && is_mono(n))
        .cloned()
        .collect();
    names.extend(current.map(str::to_string));
    finish(names)
}

/// A font is monospace when a narrow glyph ("i") and a wide one ("m") advance equally.
pub fn is_monospace(i_advance: f32, m_advance: f32) -> bool {
    (i_advance - m_advance).abs() < 0.01 && m_advance > 0.0
}

/// Change the font size by `delta` points from the rounded size, within 4 to 200.
pub fn step_size(size: f32, delta: i32) -> f32 {
    (size.round() + delta as f32).clamp(4.0, 200.0)
}

/// The View menu's font size items.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontStep {
    /// One point bigger (`step_size`).
    Bigger,
    /// One point smaller.
    Smaller,
    /// Back to the default size.
    ActualSize,
}

/// The font size after `step` from `size`.
pub fn font_size_for(size: f32, step: FontStep) -> f32 {
    match step {
        FontStep::Bigger => step_size(size, 1),
        FontStep::Smaller => step_size(size, -1),
        FontStep::ActualSize => fonts::DEFAULT_SIZE,
    }
}

/// Ligatures are on unless the last `calt` entry is 0.
pub fn ligatures_on(features: &[(String, u32)]) -> bool {
    features
        .iter()
        .rev()
        .find(|(tag, _)| tag == "calt")
        .is_none_or(|(_, v)| *v != 0)
}

/// `font-feature` values to write: every non-`calt` feature, then `-calt` when ligatures are off.
pub fn feature_lines(features: &[(String, u32)], ligatures: bool) -> Vec<String> {
    let mut lines: Vec<String> = features
        .iter()
        .filter(|(tag, _)| tag != "calt")
        .map(|(tag, v)| format!("{tag}={v}"))
        .collect();
    if !ligatures {
        lines.push("-calt".to_string());
    }
    lines
}

/// `persistent-sessions` values to write: `on`, or nothing (off is the default, so no line).
pub fn persistence_values(on: bool) -> Vec<String> {
    if on { vec!["on".to_string()] } else { vec![] }
}

/// `animations` values to write: nothing when on (the default, so no line), `off` otherwise.
pub fn animations_values(on: bool) -> Vec<String> {
    if on { vec![] } else { vec!["off".to_string()] }
}

/// `browser-address-bar` values to write: nothing when on (the default, so no line), `off`
/// otherwise.
pub fn browser_address_bar_values(on: bool) -> Vec<String> {
    if on { vec![] } else { vec!["off".to_string()] }
}

/// `status-bar` values to write: nothing when on (the default, so no line), `off` otherwise.
pub fn status_bar_values(on: bool) -> Vec<String> {
    if on { vec![] } else { vec!["off".to_string()] }
}

/// `browser-keep-data` values to write: `on`, or nothing (off is the default, so no line).
pub fn browser_keep_data_values(on: bool) -> Vec<String> {
    if on { vec!["on".to_string()] } else { vec![] }
}

/// `check-updates` values to write: nothing when on (the default, so no line), `off` otherwise.
pub fn check_updates_values(on: bool) -> Vec<String> {
    if on { vec![] } else { vec!["off".to_string()] }
}

/// `cursor-style` values to write: nothing for bar (the default, so no line), else its name.
pub fn cursor_style_values(style: CursorShapeSetting) -> Vec<String> {
    if style == CursorShapeSetting::default() {
        vec![]
    } else {
        vec![style.value().to_string()]
    }
}

/// `cursor-glide` values to write: nothing when on (the default, so no line), `off` otherwise.
pub fn cursor_glide_values(on: bool) -> Vec<String> {
    if on { vec![] } else { vec!["off".to_string()] }
}

/// `command-blocks` values to write: nothing when on (the default, so no line), `off`
/// otherwise.
pub fn command_blocks_values(on: bool) -> Vec<String> {
    if on { vec![] } else { vec!["off".to_string()] }
}

/// `cursor-glow` values to write: nothing when on (the default, so no line), `off` otherwise.
pub fn cursor_glow_values(on: bool) -> Vec<String> {
    if on { vec![] } else { vec!["off".to_string()] }
}

/// `cursor-color` values to write: nothing when following the status (the default, so no
/// line), `theme` otherwise.
pub fn cursor_color_values(follows_status: bool) -> Vec<String> {
    if follows_status {
        vec![]
    } else {
        vec!["theme".to_string()]
    }
}

/// `cursor-fun` values to write: nothing for off (the default, so no line), else its name.
pub fn cursor_fun_values(fun: CursorFun) -> Vec<String> {
    if fun == CursorFun::default() {
        vec![]
    } else {
        vec![fun.value().to_string()]
    }
}

/// The theme a list choice stands for, on its own (without the user's other colour lines):
/// the defaults for `Default`, otherwise the named theme read with `read_theme`.
pub fn choice_theme(name: &str, read_theme: &dyn Fn(&str) -> Option<String>) -> Theme {
    if name == DEFAULT_CHOICE {
        return Theme::default();
    }
    let entry = Entry {
        key: "theme".into(),
        value: name.into(),
    };
    // A plain name reads the same in dark and light mode.
    theme::resolve(&[entry], true, read_theme)
}

/// A theme row's swatch: the background, the foreground, then palette 1 to 6 (red, green,
/// yellow, blue, magenta, cyan).
pub fn swatch(theme: &Theme) -> [Rgb; 8] {
    let p = &theme.palette;
    [
        theme.background,
        theme.foreground,
        p[1],
        p[2],
        p[3],
        p[4],
        p[5],
        p[6],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_choices_merge_sort_and_dedupe() {
        let c = theme_choices(&["Rose Pine".into(), "Nord".into()], &[]);
        assert_eq!(c[0], DEFAULT_CHOICE);
        assert_eq!(c.iter().filter(|n| n.as_str() == "Nord").count(), 1);
        assert!(c.contains(&"Rose Pine".to_string()) && c.contains(&"Dracula".to_string()));
        assert_eq!(c.len(), 1 + 6 + 1);
    }

    #[test]
    fn plugin_themes_join_the_sorted_list() {
        assert_eq!(
            theme_choices(&["mine".into()], &["Sunset".into()])[..3],
            ["Default", "Catppuccin Latte", "Catppuccin Mocha"]
        );
        let c = theme_choices(&[], &["Sunset".into()]);
        assert!(c.contains(&"Sunset".to_string()));
        let sunset = c.iter().position(|n| n == "Sunset").unwrap();
        assert!(c[sunset - 1].to_lowercase().as_str() < "sunset");
    }

    #[test]
    fn font_choices_keep_mono_and_current() {
        let installed: Vec<String> = ["Menlo", "Helvetica", ".SFNS", "MesloLGS NF"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let mono = |n: &str| n == "Menlo" || n == "MesloLGS NF";
        assert_eq!(
            font_choices(&installed, &mono, Some("Helvetica")),
            vec![DEFAULT_CHOICE, "Helvetica", "Menlo", "MesloLGS NF"]
        );
        assert_eq!(
            font_choices(&installed, &mono, None),
            vec![DEFAULT_CHOICE, "Menlo", "MesloLGS NF"]
        );
    }

    #[test]
    fn lists_never_duplicate_default_or_case_variants() {
        let c = theme_choices(&["Default".into(), "nord".into()], &[]);
        assert_eq!(c.iter().filter(|n| n.as_str() == DEFAULT_CHOICE).count(), 1);
        assert_eq!(c[0], DEFAULT_CHOICE);
        let nords: Vec<_> = c.iter().filter(|n| n.to_lowercase() == "nord").collect();
        assert_eq!(nords, vec!["nord"]);
        assert_eq!(c.len(), 1 + 6);

        let installed = vec!["Menlo".to_string()];
        let f = font_choices(&installed, &|_| true, Some("menlo"));
        assert_eq!(f.len(), 2);
        assert_eq!(f[0], DEFAULT_CHOICE);
        assert_eq!(f[1].to_lowercase(), "menlo");
        assert_eq!(
            font_choices(&installed, &|_| true, Some("Default")),
            vec![DEFAULT_CHOICE, "Menlo"]
        );
    }

    #[test]
    fn is_monospace_compares_advances() {
        assert!(is_monospace(8.4, 8.4) && !is_monospace(3.9, 12.0) && !is_monospace(0.0, 0.0));
    }

    #[test]
    fn step_size_clamps_and_rounds() {
        assert_eq!(
            [
                step_size(13.0, 1),
                step_size(13.4, -1),
                step_size(4.0, -1),
                step_size(200.0, 1)
            ],
            [14.0, 12.0, 4.0, 200.0]
        );
    }

    #[test]
    fn feature_lines_toggle_only_calt() {
        let f = vec![("ss01".to_string(), 1), ("calt".to_string(), 0)];
        assert!(!ligatures_on(&f) && ligatures_on(&[]));
        assert_eq!(feature_lines(&f, true), vec!["ss01=1"]);
        assert_eq!(feature_lines(&f, false), vec!["ss01=1", "-calt"]);
    }

    #[test]
    fn animations_values() {
        // On is the default: no line. Off is written out.
        assert!(super::animations_values(true).is_empty());
        assert_eq!(super::animations_values(false), vec!["off".to_string()]);
    }

    #[test]
    fn swatch_colours_come_from_the_theme() {
        let rgb = |hex: u32| Rgb {
            r: (hex >> 16) as u8,
            g: (hex >> 8) as u8,
            b: hex as u8,
        };
        let read = |name: &str| theme::builtin_theme(name).map(str::to_string);
        // Nord's own file: background, foreground, then palette 1-6.
        assert_eq!(
            swatch(&choice_theme("Nord", &read)),
            [
                rgb(0x2e3440),
                rgb(0xd8dee9),
                rgb(0xbf616a),
                rgb(0xa3be8c),
                rgb(0xebcb8b),
                rgb(0x81a1c1),
                rgb(0xb48ead),
                rgb(0x88c0d0),
            ]
        );
        // `Default` is the built-in defaults; a user theme is read like a built-in one.
        let plain = Theme::default();
        let mut expected = [
            plain.background,
            plain.foreground,
            plain.palette[1],
            plain.palette[2],
            plain.palette[3],
            plain.palette[4],
            plain.palette[5],
            plain.palette[6],
        ];
        assert_eq!(swatch(&choice_theme(DEFAULT_CHOICE, &read)), expected);
        let mine = |name: &str| {
            (name == "Mine").then(|| "background = #102030\npalette = 3=#405060\n".to_string())
        };
        expected[0] = rgb(0x102030);
        expected[4] = rgb(0x405060);
        assert_eq!(swatch(&choice_theme("Mine", &mine)), expected);
    }

    #[test]
    fn persistence_line_is_written_only_when_on() {
        assert_eq!(persistence_values(true), vec!["on".to_string()]);
        assert!(persistence_values(false).is_empty());
    }

    #[test]
    fn the_status_bar_line_is_written_only_when_off() {
        assert!(status_bar_values(true).is_empty());
        assert_eq!(status_bar_values(false), vec!["off".to_string()]);
    }

    #[test]
    fn browser_lines_are_written_only_when_not_the_default() {
        // The address bar is on by default: only off is written.
        assert!(browser_address_bar_values(true).is_empty());
        assert_eq!(browser_address_bar_values(false), vec!["off".to_string()]);
        // Keeping data is off by default: only on is written.
        assert_eq!(browser_keep_data_values(true), vec!["on".to_string()]);
        assert!(browser_keep_data_values(false).is_empty());
    }

    #[test]
    fn check_updates_line_is_written_only_when_off() {
        assert!(check_updates_values(true).is_empty());
        assert_eq!(check_updates_values(false), vec!["off".to_string()]);
    }

    #[test]
    fn cursor_lines_are_written_only_when_not_the_default() {
        use crate::appearance::{CursorFun, CursorShapeSetting};
        assert!(cursor_style_values(CursorShapeSetting::Bar).is_empty());
        assert_eq!(
            cursor_style_values(CursorShapeSetting::Underline),
            ["underline"]
        );
        assert_eq!(cursor_style_values(CursorShapeSetting::Hollow), ["hollow"]);
        assert_eq!(cursor_style_values(CursorShapeSetting::Block), ["block"]);
        assert!(cursor_glide_values(true).is_empty());
        assert_eq!(cursor_glide_values(false), ["off"]);
        assert!(cursor_glow_values(true).is_empty());
        assert_eq!(cursor_glow_values(false), ["off"]);
        // Following the status is the default (`status`): only `theme` is written.
        assert!(cursor_color_values(true).is_empty());
        assert_eq!(cursor_color_values(false), ["theme"]);
        assert!(cursor_fun_values(CursorFun::Off).is_empty());
        assert_eq!(cursor_fun_values(CursorFun::Trail), ["trail"]);
        assert_eq!(cursor_fun_values(CursorFun::Sparks), ["sparks"]);
    }
}
