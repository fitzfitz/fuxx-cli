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
    /// `check-updates = on|off`: whether a bundled fuxx asks GitHub for a newer release at launch
    /// and once a day (default on). Fuxx → Check for Updates… works either way.
    pub check_updates: bool,
    /// `cursor-style = bar|underline|hollow|block`: the terminal cursor's shape when no program
    /// has chosen one (default bar).
    pub cursor_style: CursorShapeSetting,
    /// `cursor-glide = on|off`: the cursor slides to where it goes (default on).
    pub cursor_glide: bool,
    /// `cursor-glow = on|off`: the cursor breathes instead of blinking (default on).
    pub cursor_glow: bool,
    /// `cursor-color = status|theme`: the cursor takes its terminal's status colour (`status`,
    /// the default, true) or always the theme's cursor colour (`theme`, false).
    pub cursor_follows_status: bool,
    /// `cursor-fun = off|trail|sparks`: an optional effect on the cursor (default off).
    pub cursor_fun: CursorFun,
    /// `command-blocks = on|off`: new terminals' zsh gets fuxx's integration, and the window
    /// draws command blocks (default on, M11l).
    pub command_blocks: bool,
    /// `sound-enter` / `sound-ctrl-c` (M11s): each key's sound link, as typed (`SoundKey::ALL`
    /// order); empty: none.
    pub sound_links: [String; 2],
    /// `sound-enter-on` / `sound-ctrl-c-on`: each key's sound switched on (the default) or off.
    pub sound_on: [bool; 2],
}

impl Global for Appearance {}

/// The `cursor-style` setting: the shape fuxx draws when no program has chosen one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CursorShapeSetting {
    #[default]
    Bar,
    Underline,
    Hollow,
    Block,
}

impl CursorShapeSetting {
    /// In Preferences' order.
    pub const ALL: [CursorShapeSetting; 4] = [
        CursorShapeSetting::Bar,
        CursorShapeSetting::Underline,
        CursorShapeSetting::Hollow,
        CursorShapeSetting::Block,
    ];

    /// As written in the settings file.
    pub fn value(self) -> &'static str {
        match self {
            CursorShapeSetting::Bar => "bar",
            CursorShapeSetting::Underline => "underline",
            CursorShapeSetting::Hollow => "hollow",
            CursorShapeSetting::Block => "block",
        }
    }

    /// As Preferences shows it.
    pub fn label(self) -> &'static str {
        match self {
            CursorShapeSetting::Bar => "Bar",
            CursorShapeSetting::Underline => "Underline",
            CursorShapeSetting::Hollow => "Hollow",
            CursorShapeSetting::Block => "Block",
        }
    }
}

/// The `cursor-fun` setting: an optional effect on the cursor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CursorFun {
    #[default]
    Off,
    Trail,
    Sparks,
}

impl CursorFun {
    /// In Preferences' order.
    pub const ALL: [CursorFun; 3] = [CursorFun::Off, CursorFun::Trail, CursorFun::Sparks];

    /// As written in the settings file.
    pub fn value(self) -> &'static str {
        match self {
            CursorFun::Off => "off",
            CursorFun::Trail => "trail",
            CursorFun::Sparks => "sparks",
        }
    }

    /// As Preferences shows it.
    pub fn label(self) -> &'static str {
        match self {
            CursorFun::Off => "Off",
            CursorFun::Trail => "Trail",
            CursorFun::Sparks => "Sparks",
        }
    }
}

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

/// `sound-enter` and `sound-ctrl-c` (M11s), in `SoundKey::ALL` order: each key's last line (a
/// blank one clears it); empty when it has none.
fn sound_links_from_entries(entries: &[Entry]) -> [String; 2] {
    crate::sounds::SoundKey::ALL.map(|key| {
        entries
            .iter()
            .rev()
            .find(|e| e.key == key.key())
            .map_or_else(String::new, |e| e.value.trim().to_string())
    })
}

/// `sound-enter-on` and `sound-ctrl-c-on` (M11s): each key's sound is on unless switched off
/// (see `switch_from_entries`), in `SoundKey::ALL` order.
fn sound_on_from_entries(entries: &[Entry]) -> ([bool; 2], Vec<String>) {
    let mut warnings = vec![];
    let on = crate::sounds::SoundKey::ALL.map(|key| {
        let (on, w) = switch_from_entries(entries, key.switch_key(), true);
        warnings.extend(w);
        on
    });
    (on, warnings)
}

/// `check-updates`, on by default (see `switch_from_entries`).
fn check_updates_from_entries(entries: &[Entry]) -> (bool, Vec<String>) {
    switch_from_entries(entries, "check-updates", true)
}

/// The last valid `key` entry wins: one of `choices`, named by `value` (any case); an empty value
/// resets to `default`. Returns the setting and a warning per invalid value (the previous value
/// stays), naming the choices.
fn choice_from_entries<T: Copy>(
    entries: &[Entry],
    key: &str,
    default: T,
    choices: &[T],
    value: fn(T) -> &'static str,
) -> (T, Vec<String>) {
    let mut chosen = default;
    let mut warnings = Vec::new();
    for entry in entries.iter().filter(|e| e.key == key) {
        let given = entry.value.trim().to_lowercase();
        if given.is_empty() {
            chosen = default;
        } else if let Some(&c) = choices.iter().find(|&&c| value(c) == given) {
            chosen = c;
        } else {
            let names: Vec<&str> = choices.iter().map(|&c| value(c)).collect();
            let (last, rest) = names.split_last().expect("at least one choice");
            warnings.push(format!(
                "{key} \"{}\": expected {} or {last}",
                entry.value,
                rest.join(", ")
            ));
        }
    }
    (chosen, warnings)
}

/// `cursor-style`, bar by default (see `choice_from_entries`).
fn cursor_style_from_entries(entries: &[Entry]) -> (CursorShapeSetting, Vec<String>) {
    choice_from_entries(
        entries,
        "cursor-style",
        CursorShapeSetting::default(),
        &CursorShapeSetting::ALL,
        CursorShapeSetting::value,
    )
}

/// `command-blocks`, on by default (see `switch_from_entries`).
fn command_blocks_from_entries(entries: &[Entry]) -> (bool, Vec<String>) {
    switch_from_entries(entries, "command-blocks", true)
}

/// `cursor-glide`, on by default (see `switch_from_entries`).
fn cursor_glide_from_entries(entries: &[Entry]) -> (bool, Vec<String>) {
    switch_from_entries(entries, "cursor-glide", true)
}

/// `cursor-glow`, on by default (see `switch_from_entries`).
fn cursor_glow_from_entries(entries: &[Entry]) -> (bool, Vec<String>) {
    switch_from_entries(entries, "cursor-glow", true)
}

/// `cursor-color = status|theme` as "follows the status": `status` (the default) is true.
fn cursor_color_from_entries(entries: &[Entry]) -> (bool, Vec<String>) {
    choice_from_entries(entries, "cursor-color", true, &[true, false], |follows| {
        if follows { "status" } else { "theme" }
    })
}

/// `cursor-fun`, off by default (see `choice_from_entries`).
fn cursor_fun_from_entries(entries: &[Entry]) -> (CursorFun, Vec<String>) {
    choice_from_entries(
        entries,
        "cursor-fun",
        CursorFun::default(),
        &CursorFun::ALL,
        CursorFun::value,
    )
}

/// Read fuxx's settings file and build the appearance. Never fails: anything missing or
/// unreadable falls back to the defaults.
pub fn load(cx: &App) -> Appearance {
    load_from(cx, env_path("HOME"), env_path("XDG_CONFIG_HOME"))
}

/// `load`, with `home` and `xdg` for `$HOME` and `$XDG_CONFIG_HOME`.
fn load_from(cx: &App, home: Option<PathBuf>, xdg: Option<PathBuf>) -> Appearance {
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
    let (check_updates, check_updates_warnings) = check_updates_from_entries(&entries);
    let (cursor_style, cursor_style_warnings) = cursor_style_from_entries(&entries);
    let (cursor_glide, cursor_glide_warnings) = cursor_glide_from_entries(&entries);
    let (cursor_glow, cursor_glow_warnings) = cursor_glow_from_entries(&entries);
    let (cursor_follows_status, cursor_color_warnings) = cursor_color_from_entries(&entries);
    let (cursor_fun, cursor_fun_warnings) = cursor_fun_from_entries(&entries);
    let (command_blocks, command_blocks_warnings) = command_blocks_from_entries(&entries);
    let (sound_on, sound_on_warnings) = sound_on_from_entries(&entries);
    for warning in preset_warnings
        .iter()
        .chain(&restore_warnings)
        .chain(&scripting_warnings)
        .chain(&persistent_warnings)
        .chain(&animations_warnings)
        .chain(&address_bar_warnings)
        .chain(&keep_data_warnings)
        .chain(&check_updates_warnings)
        .chain(&cursor_style_warnings)
        .chain(&cursor_glide_warnings)
        .chain(&cursor_glow_warnings)
        .chain(&cursor_color_warnings)
        .chain(&cursor_fun_warnings)
        .chain(&command_blocks_warnings)
        .chain(&sound_on_warnings)
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
        check_updates,
        cursor_style,
        cursor_glide,
        cursor_glow,
        cursor_follows_status,
        cursor_fun,
        command_blocks,
        sound_links: sound_links_from_entries(&entries),
        sound_on,
    }
}

/// Re-read the settings and replace the global; observers (every view) then redraw.
pub fn reload(cx: &mut App) {
    #[cfg(feature = "ui-shots")]
    if let Some(dir) = cx.try_global::<ShotsHome>().map(|h| h.0.clone()) {
        let appearance = load_from(cx, Some(dir.clone()), Some(dir));
        cx.set_global(appearance);
        return;
    }
    let appearance = load(cx);
    cx.set_global(appearance);
}

/// The screenshots: while set, a reload reads the settings and themes from this folder (as
/// both `HOME` and `XDG_CONFIG_HOME`), never your own.
#[cfg(feature = "ui-shots")]
pub struct ShotsHome(pub PathBuf);

#[cfg(feature = "ui-shots")]
impl Global for ShotsHome {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_key_s_sound_is_on_unless_switched_off() {
        let e = |key: &str, value: &str| Entry {
            key: key.into(),
            value: value.into(),
        };
        assert_eq!(sound_on_from_entries(&[]), ([true, true], vec![]));
        let (on, warnings) =
            sound_on_from_entries(&[e("sound-enter-on", "off"), e("sound-ctrl-c-on", "loud")]);
        assert_eq!(on, [false, true]);
        assert_eq!(warnings.len(), 1, "a junk value warns");
    }

    #[test]
    fn each_key_s_sound_is_its_last_line_and_a_blank_one_clears_it() {
        let e = |key: &str, value: &str| Entry {
            key: key.into(),
            value: value.into(),
        };
        assert_eq!(
            sound_links_from_entries(&[]),
            [String::new(), String::new()]
        );
        assert_eq!(
            sound_links_from_entries(&[
                e("sound-enter", "~/a.wav"),
                e("sound-ctrl-c", "https://x.io/b.mp3"),
                e("sound-enter", "/System/Library/Sounds/Tink.aiff"),
            ]),
            [
                "/System/Library/Sounds/Tink.aiff".to_string(),
                "https://x.io/b.mp3".to_string()
            ]
        );
        assert_eq!(
            sound_links_from_entries(&[e("sound-enter", "~/a.wav"), e("sound-enter", "")]),
            [String::new(), String::new()]
        );
    }

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

    #[test]
    fn check_updates_setting_defaults_on_and_warns_on_junk() {
        let check = |v: &str| entry("check-updates", v);
        assert_eq!(check_updates_from_entries(&[]), (true, vec![]));
        assert_eq!(check_updates_from_entries(&[check("off")]), (false, vec![]));
        let (on, warnings) = check_updates_from_entries(&[check("daily")]);
        assert!(on); // the bad value leaves the default in place
        assert_eq!(
            warnings,
            vec!["check-updates \"daily\": expected on or off".to_string()]
        );
        // An empty value resets to the default (on).
        assert_eq!(
            check_updates_from_entries(&[check("off"), check("")]),
            (true, vec![])
        );
        // Other keys are ignored.
        assert_eq!(
            check_updates_from_entries(&[entry("animations", "off")]),
            (true, vec![])
        );
    }

    #[test]
    fn cursor_settings_default_to_a_breathing_gliding_bar_in_the_status_colour() {
        assert_eq!(
            cursor_style_from_entries(&[]),
            (CursorShapeSetting::Bar, vec![])
        );
        assert_eq!(cursor_glide_from_entries(&[]), (true, vec![]));
        assert_eq!(cursor_glow_from_entries(&[]), (true, vec![]));
        assert_eq!(cursor_color_from_entries(&[]), (true, vec![]));
        assert_eq!(cursor_fun_from_entries(&[]), (CursorFun::Off, vec![]));
    }

    #[test]
    fn cursor_settings_read_each_value() {
        let style = |v: &str| cursor_style_from_entries(&[entry("cursor-style", v)]).0;
        assert_eq!(style("bar"), CursorShapeSetting::Bar);
        assert_eq!(style("underline"), CursorShapeSetting::Underline);
        assert_eq!(style(" Hollow "), CursorShapeSetting::Hollow);
        assert_eq!(style("BLOCK"), CursorShapeSetting::Block);
        assert_eq!(
            cursor_glide_from_entries(&[entry("cursor-glide", "off")]),
            (false, vec![])
        );
        assert_eq!(
            cursor_glow_from_entries(&[entry("cursor-glow", "off")]),
            (false, vec![])
        );
        assert_eq!(
            cursor_color_from_entries(&[entry("cursor-color", "theme")]),
            (false, vec![])
        );
        assert_eq!(
            cursor_color_from_entries(&[
                entry("cursor-color", "theme"),
                entry("cursor-color", "status")
            ]),
            (true, vec![])
        );
        let fun = |v: &str| cursor_fun_from_entries(&[entry("cursor-fun", v)]).0;
        assert_eq!(fun("off"), CursorFun::Off);
        assert_eq!(fun("trail"), CursorFun::Trail);
        assert_eq!(fun("Sparks"), CursorFun::Sparks);
        // An empty value resets to the default; the last valid line wins.
        assert_eq!(
            cursor_style_from_entries(&[entry("cursor-style", "block"), entry("cursor-style", "")]),
            (CursorShapeSetting::Bar, vec![])
        );
        // Other keys are ignored.
        assert_eq!(
            cursor_fun_from_entries(&[entry("cursor-style", "trail")]),
            (CursorFun::Off, vec![])
        );
    }

    #[test]
    fn cursor_settings_warn_on_junk_and_keep_the_previous_value() {
        assert_eq!(
            cursor_style_from_entries(&[entry("cursor-style", "beam")]),
            (
                CursorShapeSetting::Bar,
                vec!["cursor-style \"beam\": expected bar, underline, hollow or block".to_string()]
            )
        );
        assert_eq!(
            cursor_style_from_entries(&[
                entry("cursor-style", "block"),
                entry("cursor-style", "x")
            ])
            .0,
            CursorShapeSetting::Block
        );
        assert_eq!(
            cursor_glide_from_entries(&[entry("cursor-glide", "fast")]),
            (
                true,
                vec!["cursor-glide \"fast\": expected on or off".to_string()]
            )
        );
        assert_eq!(
            cursor_glow_from_entries(&[entry("cursor-glow", "dim")]),
            (
                true,
                vec!["cursor-glow \"dim\": expected on or off".to_string()]
            )
        );
        assert_eq!(
            cursor_color_from_entries(&[entry("cursor-color", "red")]),
            (
                true,
                vec!["cursor-color \"red\": expected status or theme".to_string()]
            )
        );
        assert_eq!(
            cursor_fun_from_entries(&[entry("cursor-fun", "confetti")]),
            (
                CursorFun::Off,
                vec!["cursor-fun \"confetti\": expected off, trail or sparks".to_string()]
            )
        );
    }

    #[test]
    fn cursor_choices_name_their_settings_values() {
        assert_eq!(
            CursorShapeSetting::ALL.map(CursorShapeSetting::value),
            ["bar", "underline", "hollow", "block"]
        );
        assert_eq!(
            CursorShapeSetting::ALL.map(CursorShapeSetting::label),
            ["Bar", "Underline", "Hollow", "Block"]
        );
        assert_eq!(
            CursorFun::ALL.map(CursorFun::value),
            ["off", "trail", "sparks"]
        );
        assert_eq!(
            CursorFun::ALL.map(CursorFun::label),
            ["Off", "Trail", "Sparks"]
        );
        assert_eq!(CursorShapeSetting::default(), CursorShapeSetting::Bar);
        assert_eq!(CursorFun::default(), CursorFun::Off);
    }

    #[test]
    fn command_blocks_setting() {
        assert_eq!(command_blocks_from_entries(&[]), (true, vec![]));
        assert_eq!(
            command_blocks_from_entries(&[entry("command-blocks", "off")]),
            (false, vec![])
        );
        assert_eq!(
            command_blocks_from_entries(&[entry("command-blocks", "maybe")]),
            (
                true,
                vec!["command-blocks \"maybe\": expected on or off".to_string()]
            )
        );
    }
}
