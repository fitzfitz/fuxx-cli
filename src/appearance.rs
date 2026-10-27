//! The look fuxx starts with: theme, font and size from fuxx's own settings file.
//!
//! `load` is the only place that touches the real filesystem and environment for the config; all
//! the decisions live in the tested pure functions of `settings`, `theme` and `fonts`. It
//! runs once at startup (no live reload) and the result is an app global that views read.

use std::path::{Path, PathBuf};

use gpui::{App, Font, Global, WindowAppearance};

use crate::fonts;
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
}

impl Global for Appearance {}

/// An environment variable as a path; unset or empty counts as absent.
fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
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

    Appearance {
        theme,
        font,
        font_size: settings.size,
        theme_name: theme_name.map(str::to_string),
        font_settings: settings,
        settings_path,
        user_theme_dir: user_dir,
    }
}

/// Re-read the settings and replace the global; observers (every view) then redraw.
pub fn reload(cx: &mut App) {
    let appearance = load(cx);
    cx.set_global(appearance);
}
