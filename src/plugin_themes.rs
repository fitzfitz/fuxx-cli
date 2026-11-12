//! Themes that installed plugins bring: which names the theme list gets, and the one config
//! line uninstalling a theme plugin must take out.
//!
//! Pure functions. `appearance` feeds in the plugin records and the names already taken.

use std::path::PathBuf;

use crate::plugin_store::{Folders, Record};
use crate::settings;

/// One theme from an installed plugin, as the theme list shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct PluginTheme {
    /// What the list shows and the `theme =` line names. Carries the plugin's name in brackets
    /// when the plain name was already taken.
    pub name: String,
    /// The plugin's display name.
    pub plugin: String,
    pub plugin_id: String,
    /// The theme file.
    pub path: PathBuf,
}

/// The plugin themes, in record order then each record's own order. A name equal (ignoring
/// case) to one in `taken` (the user's themes and the built-ins), to "Default" or to an earlier plugin
/// theme's becomes `<name> (<plugin name>)`, so a plugin can't hide a theme you already have.
pub fn list(f: &Folders, records: &[Record], taken: &[String]) -> Vec<PluginTheme> {
    let mut out: Vec<PluginTheme> = Vec::new();
    for record in records {
        for theme_path in &record.themes {
            let plain = std::path::Path::new(theme_path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| theme_path.clone());
            let clash = taken
                .iter()
                .map(String::as_str)
                .chain(std::iter::once(crate::prefs::DEFAULT_CHOICE))
                .chain(out.iter().map(|t| t.name.as_str()))
                .any(|n| n.eq_ignore_ascii_case(&plain));
            let name = if clash {
                format!("{plain} ({})", record.name)
            } else {
                plain
            };
            out.push(PluginTheme {
                name,
                plugin: record.name.clone(),
                plugin_id: record.id.clone(),
                path: f.plugin(&record.id).join(theme_path),
            });
        }
    }
    out
}

/// `config` without its `theme` line, when the last `theme` entry names one of `removed`
/// (plain, or either half of `light:A,dark:B`, ignoring case). `None` when nothing needs to
/// change. It reads only this file, never a `config-file` include.
pub fn config_without_theme(config: &str, removed: &[String]) -> Option<String> {
    let entries = settings::parse(config);
    let value = &entries.iter().rev().find(|e| e.key == "theme")?.value;
    let names_removed = |n: &str| removed.iter().any(|r| r.eq_ignore_ascii_case(n.trim()));
    let hit = names_removed(value)
        || value.split(',').any(|half| {
            half.trim()
                .strip_prefix("light:")
                .or_else(|| half.trim().strip_prefix("dark:"))
                .is_some_and(names_removed)
        });
    hit.then(|| settings::set_key(config, "theme", &[]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin_source::Installed;

    fn record(id: &str, name: &str, themes: &[&str]) -> Record {
        Record {
            id: id.into(),
            name: name.into(),
            version: "1.0.0".into(),
            description: None,
            repo: "o/r".into(),
            installed: Installed::Tag("v1".into()),
            themes: themes.iter().map(|s| s.to_string()).collect(),
            installed_at: 0,
        }
    }

    fn folders() -> Folders {
        Folders::under(std::path::Path::new("/home/x"))
    }

    #[test]
    fn a_taken_name_gets_its_plugin_name() {
        let f = folders();
        let records = [record(
            "sunset",
            "Sunset Themes",
            &["themes/Sunset", "themes/Dracula"],
        )];
        let taken = ["Dracula".to_string(), "Mine".to_string()];
        let got = list(&f, &records, &taken);
        let names: Vec<&str> = got.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["Sunset", "Dracula (Sunset Themes)"]);
        assert_eq!(got[0].plugin, "Sunset Themes");
        assert_eq!(got[0].plugin_id, "sunset");
        assert_eq!(got[0].path, f.plugin("sunset").join("themes/Sunset"));
        assert_eq!(got[1].path, f.plugin("sunset").join("themes/Dracula"));
    }

    #[test]
    fn a_name_an_earlier_plugin_took_is_told_apart_too() {
        let f = folders();
        let records = [
            record("a", "Alpha", &["themes/Dusk"]),
            record("b", "Beta", &["themes/dusk"]),
        ];
        let got = list(&f, &records, &[]);
        assert_eq!(got[0].name, "Dusk");
        assert_eq!(got[1].name, "dusk (Beta)");
    }

    #[test]
    fn a_plugin_theme_named_default_is_told_apart() {
        let got = list(
            &folders(),
            &[record("a", "Alpha", &["themes/Default"])],
            &[],
        );
        assert_eq!(got[0].name, "Default (Alpha)");
    }

    #[test]
    fn the_theme_line_goes_when_it_names_a_removed_theme() {
        let removed = ["Sunset".to_string()];
        assert_eq!(
            config_without_theme("font-size = 13\ntheme = sunset\n", &removed).as_deref(),
            Some("font-size = 13\n")
        );
        assert_eq!(
            config_without_theme("theme = light:Nord,dark:Sunset\n", &removed).as_deref(),
            Some("")
        );
        assert_eq!(config_without_theme("theme = Nord\n", &removed), None);
        assert_eq!(config_without_theme("config-file = more\n", &removed), None);
    }

    #[test]
    fn only_the_last_theme_entry_counts() {
        let removed = ["Sunset".to_string()];
        assert_eq!(
            config_without_theme("theme = Sunset\ntheme = Nord\n", &removed),
            None
        );
    }
}
