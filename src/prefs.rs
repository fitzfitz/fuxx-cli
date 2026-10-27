//! Preferences window choices: what the lists show and which settings lines get written.
//!
//! Pure functions, no GPUI types, so they are unit-tested. The window feeds its state in.

use crate::theme::BUILTIN_THEMES;

/// First list entry: leave the setting out of the file and use the built-in default.
pub const DEFAULT_CHOICE: &str = "Default";

fn sort_dedupe(names: &mut Vec<String>) {
    names.sort_by_key(|n| n.to_lowercase());
    names.dedup();
}

/// `Default`, then built-in and user theme names, sorted case-insensitively without duplicates.
pub fn theme_choices(user_themes: &[String]) -> Vec<String> {
    let mut names: Vec<String> = BUILTIN_THEMES
        .iter()
        .map(|s| s.to_string())
        .chain(user_themes.iter().cloned())
        .collect();
    sort_dedupe(&mut names);
    names.insert(0, DEFAULT_CHOICE.to_string());
    names
}

/// `Default`, then installed monospace families (system-private `.` names skipped) and `current`.
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
    if let Some(c) = current {
        names.push(c.to_string());
    }
    sort_dedupe(&mut names);
    names.insert(0, DEFAULT_CHOICE.to_string());
    names
}

/// A font is monospace when a narrow glyph ("i") and a wide one ("m") advance equally.
pub fn is_monospace(i_advance: f32, m_advance: f32) -> bool {
    (i_advance - m_advance).abs() < 0.01 && m_advance > 0.0
}

/// Change the font size by `delta` points from the rounded size, within 4 to 200.
pub fn step_size(size: f32, delta: i32) -> f32 {
    (size.round() + delta as f32).clamp(4.0, 200.0)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_choices_merge_sort_and_dedupe() {
        let c = theme_choices(&["Rose Pine".into(), "Nord".into()]);
        assert_eq!(c[0], DEFAULT_CHOICE);
        assert_eq!(c.iter().filter(|n| n.as_str() == "Nord").count(), 1);
        assert!(c.contains(&"Rose Pine".to_string()) && c.contains(&"Dracula".to_string()));
        assert_eq!(c.len(), 1 + 6 + 1);
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
}
