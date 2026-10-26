//! The look fuxx starts with: theme, font and size from the user's Ghostty config.
//!
//! `load` is the only place that touches the real filesystem and environment for the config; all
//! the decisions live in the tested pure functions of `ghostty_config`, `theme` and `fonts`. It
//! runs once at startup (no live reload) and the result is an app global that views read.

use std::path::{Path, PathBuf};

use gpui::{App, Font, Global, WindowAppearance};

use crate::fonts;
use crate::ghostty_config::{self, Entry};
use crate::theme::{self, Theme};

pub struct Appearance {
    pub theme: Theme,
    pub font: Font,
    /// In points, used as pixels as-is (as in Ghostty).
    pub font_size: f32,
}

impl Global for Appearance {}

/// An environment variable as a path; unset or empty counts as absent.
fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// Read the Ghostty config and build the appearance. Never fails: anything missing or unreadable
/// falls back to Ghostty's defaults.
pub fn load(cx: &App) -> Appearance {
    let home = env_path("HOME");
    let xdg = env_path("XDG_CONFIG_HOME");
    let resources = env_path("GHOSTTY_RESOURCES_DIR");

    let read_file = |p: &Path| std::fs::read_to_string(p).ok();
    let entries: Vec<Entry> = match &home {
        Some(home) => ghostty_config::load(
            &ghostty_config::config_paths(home, xdg.as_deref()),
            &read_file,
        ),
        None => {
            eprintln!("[fuxx] config: $HOME is unset; using the defaults");
            vec![]
        }
    };

    // Without $HOME there are no user theme folders, but Ghostty.app's own themes still resolve.
    let dirs = theme::theme_dirs(
        home.as_deref().unwrap_or(Path::new("/")),
        xdg.as_deref(),
        resources.as_deref(),
    );
    let read_theme = |name: &str| {
        let path = Path::new(name);
        if path.is_absolute() {
            read_file(path)
        } else {
            dirs.iter().find_map(|dir| read_file(&dir.join(name)))
        }
    };
    let dark = !matches!(
        cx.window_appearance(),
        WindowAppearance::Light | WindowAppearance::VibrantLight
    );
    let theme = theme::resolve(&entries, dark, &read_theme);

    // Register first, so the symbols font is known to CoreText before any text is shaped. It logs
    // its own failure; the font still names it as the last fallback either way.
    fonts::register_symbols_font();
    let settings = fonts::from_entries(&entries);
    let font = fonts::gpui_font(&settings, &cx.text_system().all_font_names());

    let theme_name = entries
        .iter()
        .rev()
        .find(|e| e.key == "theme")
        .map(|e| e.value.as_str())
        .filter(|v| !v.is_empty())
        .unwrap_or("default");
    eprintln!(
        "[fuxx] config: font {} {}pt, theme {theme_name}",
        font.family, settings.size
    );

    Appearance {
        theme,
        font,
        font_size: settings.size,
    }
}
