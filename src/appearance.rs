//! The look fuxx starts with: theme, font and size from fuxx's own settings file.
//!
//! `load` is the only place that touches the real filesystem and environment for the config; all
//! the decisions live in the tested pure functions of `settings`, `theme` and `fonts`. It
//! runs once at startup (no live reload) and the result is an app global that views read.

use std::path::{Path, PathBuf};

use gpui::{App, Font, Global, WindowAppearance};

use crate::fonts;
use crate::presets::{self, Preset};
use crate::restore::RestoreMode;
use crate::settings::{self, Entry};
use crate::theme::{self, Theme};

pub struct Appearance {
    pub theme: Theme,
    pub font: Font,
    /// In points, used as pixels as-is (as Ghostty does).
    pub font_size: f32,
    /// The last `theme` entry's value, if any.
    pub theme_name: Option<String>,
    pub font_settings: fonts::FontSettings,
    pub settings_path: Option<PathBuf>,
    pub user_theme_dir: Option<PathBuf>,
    /// The agent presets: built-ins plus `agent-preset` lines.
    pub presets: Vec<Preset>,
    pub restore: RestoreMode,
    /// `scripting = on|off`: whether the scripting socket starts (default on).
    pub scripting: bool,
    /// `persistent-sessions = on|off`: sessions run in the background service and survive
    /// quitting (default off). Read at startup; changing it takes effect the next launch.
    pub persistent: bool,
    /// `animations = on|off`: the status marks' motion (default on). macOS Reduce motion stops
    /// it too (see `reduce_motion`).
    pub animations: bool,
    /// `browser-address-bar = on|off`: the browser windows' address field (default on).
    pub browser_address_bar: bool,
    /// `browser-keep-data = on|off`: browser windows opened from now on keep cookies, logins and
    /// site data on disk between runs (default off: each run starts from nothing).
    pub browser_keep_data: bool,
}

impl Global for Appearance {}

/// An environment variable as a path; unset or empty counts as absent.
fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// The last valid `scripting` entry wins (`on` / `off`, any case); an empty value resets to on.
/// Returns the setting and a warning per invalid value (the previous value stays).
fn scripting_from_entries(entries: &[Entry]) -> (bool, Vec<String>) {
    let mut on = true;
    let mut warnings = Vec::new();
    for entry in entries.iter().filter(|e| e.key == "scripting") {
        match entry.value.trim().to_lowercase().as_str() {
            "" | "on" => on = true,
            "off" => on = false,
            _ => warnings.push(format!("scripting \"{}\": expected on or off", entry.value)),
        }
    }
    (on, warnings)
}

/// The last valid `persistent-sessions` entry wins (`on` / `off`, any case); an empty value
/// resets to off. Returns the setting and a warning per invalid value (the previous value stays).
fn persistent_from_entries(entries: &[Entry]) -> (bool, Vec<String>) {
    let mut on = false;
    let mut warnings = Vec::new();
    for entry in entries.iter().filter(|e| e.key == "persistent-sessions") {
        match entry.value.trim().to_lowercase().as_str() {
            "" | "off" => on = false,
            "on" => on = true,
            _ => warnings.push(format!(
                "persistent-sessions \"{}\": expected on or off",
                entry.value
            )),
        }
    }
    (on, warnings)
}

/// The last valid `animations` entry wins (`on` / `off`, any case); an empty value resets to on.
/// Returns the setting and a warning per invalid value (the previous value stays).
fn animations_from_entries(entries: &[Entry]) -> (bool, Vec<String>) {
    let mut on = true;
    let mut warnings = Vec::new();
    for entry in entries.iter().filter(|e| e.key == "animations") {
        match entry.value.trim().to_lowercase().as_str() {
            "" | "on" => on = true,
            "off" => on = false,
            _ => warnings.push(format!(
                "animations \"{}\": expected on or off",
                entry.value
            )),
        }
    }
    (on, warnings)
}

/// The last valid `key` entry wins (`on` / `off`, any case); an empty value resets to `default`.
/// Returns the setting and a warning per invalid value (the previous value stays).
fn switch_from_entries(entries: &[Entry], key: &str, default: bool) -> (bool, Vec<String>) {
    let mut on = default;
    let mut warnings = Vec::new();
    for entry in entries.iter().filter(|e| e.key == key) {
        match entry.value.trim().to_lowercase().as_str() {
            "" => on = default,
            "on" => on = true,
            "off" => on = false,
            _ => warnings.push(format!("{key} \"{}\": expected on or off", entry.value)),
        }
    }
    (on, warnings)
}

/// `browser-address-bar`, on by default (see `switch_from_entries`).
fn browser_address_bar_from_entries(entries: &[Entry]) -> (bool, Vec<String>) {
    switch_from_entries(entries, "browser-address-bar", true)
}

/// `browser-keep-data`, off by default (see `switch_from_entries`).
fn browser_keep_data_from_entries(entries: &[Entry]) -> (bool, Vec<String>) {
    switch_from_entries(entries, "browser-keep-data", false)
}

/// Read fuxx's settings file and build the appearance. Never fails: anything missing or
/// unreadable falls back to the defaults.
pub fn load(cx: &App) -> Appearance {
    let home = env_path("HOME");
    let xdg = env_path("XDG_CONFIG_HOME");

    let read_file = |p: &Path| std::fs::read_to_string(p).ok();
    let settings_path = home
        .as_deref()
        .map(|h| settings::settings_path(h, xdg.as_deref()));
    let (entries, config_note): (Vec<Entry>, String) = match (&home, &settings_path) {
        (Some(_), Some(path)) => {
            let text = read_file(path);
            let note = format!(
                "{} ({})",
                path.display(),
                if text.is_some() { "found" } else { "not found" }
            );
            (settings::load(std::slice::from_ref(path), &read_file), note)
        }
        _ => {
            eprintln!("[fuxx] config: $HOME is unset; using the defaults");
            (vec![], "none ($HOME unset)".to_string())
        }
    };

    // A theme name is an absolute file, then a file in the user's theme folder, then a built-in.
    // Without $HOME there is no user folder, only the built-ins.
    let user_dir = home
        .as_deref()
        .map(|h| theme::user_theme_dir(h, xdg.as_deref()));
    let read_theme = |name: &str| {
        let path = Path::new(name);
        if path.is_absolute() {
            return read_file(path);
        }
        user_dir
            .as_ref()
            .and_then(|dir| read_file(&dir.join(name)))
            .or_else(|| theme::builtin_theme(name).map(str::to_string))
    };
    let dark = !matches!(
        cx.window_appearance(),
        WindowAppearance::Light | WindowAppearance::VibrantLight
    );
    let theme = theme::resolve(&entries, dark, &read_theme);

    // Register first, so the bundled fonts are known to CoreText before any text is shaped. The
    // list from before registering lets it skip JetBrains Mono when the user has their own. It
    // logs its own failure; the font still names the symbols font as last fallback either way.
    fonts::register_bundled_fonts(&cx.text_system().all_font_names());
    let settings = fonts::from_entries(&entries);
    // Listed again: the bundled JetBrains Mono only shows up after registering.
    let font = fonts::gpui_font(&settings, &cx.text_system().all_font_names());

    let theme_name = entries
        .iter()
        .rev()
        .find(|e| e.key == "theme")
        .map(|e| e.value.as_str())
        .filter(|v| !v.is_empty());
    let theme_label = theme_name.unwrap_or("default");
    eprintln!(
        "[fuxx] config: {config_note}, font {} {}pt, theme {theme_label}",
        font.family, settings.size
    );

    let (presets, preset_warnings) = presets::from_entries(&entries);
    let (restore, restore_warnings) = RestoreMode::from_entries(&entries);
    let (scripting, scripting_warnings) = scripting_from_entries(&entries);
    let (persistent, persistent_warnings) = persistent_from_entries(&entries);
    let (animations, animations_warnings) = animations_from_entries(&entries);
    let (browser_address_bar, address_bar_warnings) = browser_address_bar_from_entries(&entries);
    let (browser_keep_data, keep_data_warnings) = browser_keep_data_from_entries(&entries);
    for warning in preset_warnings
        .iter()
        .chain(&restore_warnings)
        .chain(&scripting_warnings)
        .chain(&persistent_warnings)
        .chain(&animations_warnings)
        .chain(&address_bar_warnings)
        .chain(&keep_data_warnings)
    {
        eprintln!("[fuxx] config: {warning}");
    }

    Appearance {
        theme,
        font,
        font_size: settings.size,
        theme_name: theme_name.map(str::to_string),
        font_settings: settings,
        settings_path,
        user_theme_dir: user_dir,
        presets,
        restore,
        scripting,
        persistent,
        animations,
        browser_address_bar,
        browser_keep_data,
    }
}

/// Re-read the settings and replace the global; observers (every view) then redraw.
pub fn reload(cx: &mut App) {
    let appearance = load(cx);
    cx.set_global(appearance);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(value: &str) -> Entry {
        Entry {
            key: "scripting".into(),
            value: value.into(),
        }
    }

    #[test]
    fn scripting_setting_defaults_on_and_warns_on_junk() {
        assert_eq!(scripting_from_entries(&[]), (true, vec![]));
        assert_eq!(scripting_from_entries(&[s(" OFF ")]), (false, vec![]));
        let (on, warnings) = scripting_from_entries(&[s("off"), s("maybe")]);
        assert!(!on); // the bad value leaves the previous one in place
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("maybe"));
        assert_eq!(scripting_from_entries(&[s("off"), s("")]), (true, vec![]));
    }

    fn p(value: &str) -> Entry {
        Entry {
            key: "persistent-sessions".into(),
            value: value.into(),
        }
    }

    #[test]
    fn persistent_setting_defaults_off_and_warns_on_junk() {
        assert_eq!(persistent_from_entries(&[]), (false, vec![]));
        assert_eq!(persistent_from_entries(&[p(" ON ")]), (true, vec![]));
        assert_eq!(
            persistent_from_entries(&[p("on"), p("off")]),
            (false, vec![])
        );
        let (on, warnings) = persistent_from_entries(&[p("on"), p("yes")]);
        assert!(on); // the bad value leaves the previous one in place
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("yes") && warnings[0].contains("persistent-sessions"));
        assert_eq!(persistent_from_entries(&[p("on"), p("")]), (false, vec![]));
        // Other keys are ignored.
        assert_eq!(persistent_from_entries(&[s("on")]), (false, vec![]));
    }

    fn a(value: &str) -> Entry {
        Entry {
            key: "animations".into(),
            value: value.into(),
        }
    }

    #[test]
    fn animations_setting_parses() {
        assert_eq!(animations_from_entries(&[]), (true, vec![]));
        assert_eq!(animations_from_entries(&[a(" OFF ")]), (false, vec![]));
        assert_eq!(
            animations_from_entries(&[a("off"), a("On")]),
            (true, vec![])
        );
        let (on, warnings) = animations_from_entries(&[a("off"), a("sometimes")]);
        assert!(!on); // the bad value leaves the previous one in place
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("sometimes") && warnings[0].contains("animations"));
        // An empty value resets to the default (on).
        assert_eq!(animations_from_entries(&[a("off"), a("")]), (true, vec![]));
        // Other keys are ignored.
        assert_eq!(animations_from_entries(&[s("off")]), (true, vec![]));
    }

    fn entry(key: &str, value: &str) -> Entry {
        Entry {
            key: key.into(),
            value: value.into(),
        }
    }

    #[test]
    fn browser_address_bar_setting_defaults_on_and_warns_on_junk() {
        let bar = |v: &str| entry("browser-address-bar", v);
        assert_eq!(browser_address_bar_from_entries(&[]), (true, vec![]));
        assert_eq!(
            browser_address_bar_from_entries(&[bar(" OFF ")]),
            (false, vec![])
        );
        assert_eq!(
            browser_address_bar_from_entries(&[bar("off"), bar("On")]),
            (true, vec![])
        );
        let (on, warnings) = browser_address_bar_from_entries(&[bar("off"), bar("hidden")]);
        assert!(!on); // the bad value leaves the previous one in place
        assert_eq!(
            warnings,
            vec!["browser-address-bar \"hidden\": expected on or off".to_string()]
        );
        // An empty value resets to the default (on).
        assert_eq!(
            browser_address_bar_from_entries(&[bar("off"), bar("")]),
            (true, vec![])
        );
        // Other keys are ignored.
        assert_eq!(
            browser_address_bar_from_entries(&[entry("browser-keep-data", "off")]),
            (true, vec![])
        );
    }

    #[test]
    fn browser_keep_data_setting_defaults_off_and_warns_on_junk() {
        let keep = |v: &str| entry("browser-keep-data", v);
        assert_eq!(browser_keep_data_from_entries(&[]), (false, vec![]));
        assert_eq!(
            browser_keep_data_from_entries(&[keep(" ON ")]),
            (true, vec![])
        );
        assert_eq!(
            browser_keep_data_from_entries(&[keep("on"), keep("Off")]),
            (false, vec![])
        );
        let (on, warnings) = browser_keep_data_from_entries(&[keep("on"), keep("forever")]);
        assert!(on); // the bad value leaves the previous one in place
        assert_eq!(
            warnings,
            vec!["browser-keep-data \"forever\": expected on or off".to_string()]
        );
        // An empty value resets to the default (off).
        assert_eq!(
            browser_keep_data_from_entries(&[keep("on"), keep("")]),
            (false, vec![])
        );
        // Other keys are ignored.
        assert_eq!(
            browser_keep_data_from_entries(&[entry("browser-address-bar", "on")]),
            (false, vec![])
        );
    }
}
