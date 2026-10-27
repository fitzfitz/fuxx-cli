//! Font settings from fuxx's settings file, and the bundled fonts (JetBrains Mono, Symbols Nerd Font).
//!
//! `from_entries` and `parse_feature` are pure. `gpui_font` turns the settings into a GPUI font
//! (skipping families that aren't installed). `register_bundled_fonts` makes the bundled fonts
//! visible to CoreText: JetBrains Mono becomes the default family (ligatures work with nothing
//! installed) and the icon font is the fallback for private-use glyphs (Nerd Font icons).
//!
//! The icon font is licensed under `assets/fonts/LICENSE-NerdFonts.txt`; the icon sets inside it
//! carry their own licences, listed in `assets/fonts/NOTICE-NerdFonts-IconSets.md`. JetBrains Mono
//! (v2.304) is under the SIL OFL, in `assets/fonts/LICENSE-JetBrainsMono-OFL.txt`.

use std::sync::{Arc, OnceLock};

use gpui::{Font, FontFallbacks, FontFeatures, SharedString};

use crate::settings::Entry;

pub const DEFAULT_SIZE: f32 = 13.0;
pub const SYMBOLS_FAMILY: &str = "Symbols Nerd Font Mono";

const JETBRAINS_FAMILY: &str = "JetBrains Mono";

/// The fonts shipped inside the binary: (file name in the cache folder, contents).
const BUNDLED: [(&str, &[u8]); 5] = [
    (
        "SymbolsNerdFontMono-Regular.ttf",
        include_bytes!("../assets/fonts/SymbolsNerdFontMono-Regular.ttf"),
    ),
    (
        "JetBrainsMono-Regular.ttf",
        include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"),
    ),
    (
        "JetBrainsMono-Bold.ttf",
        include_bytes!("../assets/fonts/JetBrainsMono-Bold.ttf"),
    ),
    (
        "JetBrainsMono-Italic.ttf",
        include_bytes!("../assets/fonts/JetBrainsMono-Italic.ttf"),
    ),
    (
        "JetBrainsMono-BoldItalic.ttf",
        include_bytes!("../assets/fonts/JetBrainsMono-BoldItalic.ttf"),
    ),
];

/// The font choices from the config: families in priority order, size in points, OpenType features.
#[derive(Clone, Debug, PartialEq)]
pub struct FontSettings {
    pub families: Vec<String>,
    pub size: f32,
    pub features: Vec<(String, u32)>,
}

/// Sizes outside this range (in points) are ignored: tiny ones give a zero-height line and huge
/// ones exhaust memory when rasterising.
const MIN_SIZE: f32 = 4.0;
const MAX_SIZE: f32 = 200.0;

/// Fold `font-family`, `font-size` and `font-feature` entries, in order. Families and features
/// accumulate and an empty value clears them; the last valid size wins and an empty one resets it.
pub fn from_entries(entries: &[Entry]) -> FontSettings {
    let mut s = FontSettings {
        families: vec![],
        size: DEFAULT_SIZE,
        features: vec![],
    };
    for e in entries {
        match e.key.as_str() {
            "font-family" if e.value.is_empty() => s.families.clear(),
            "font-family" => s.families.push(e.value.clone()),
            "font-size" if e.value.is_empty() => s.size = DEFAULT_SIZE,
            "font-size" => match e.value.parse::<f32>() {
                Ok(v) if (MIN_SIZE..=MAX_SIZE).contains(&v) => s.size = v,
                _ => eprintln!(
                    "[fuxx] config: ignoring bad font-size {:?} (use {MIN_SIZE}-{MAX_SIZE})",
                    e.value
                ),
            },
            "font-feature" if e.value.is_empty() => s.features.clear(),
            "font-feature" => {
                for item in e.value.split(',') {
                    match parse_feature(item) {
                        Some(f) => s.features.push(f),
                        None => {
                            eprintln!("[fuxx] config: ignoring bad font-feature {:?}", item.trim())
                        }
                    }
                }
            }
            _ => {}
        }
    }
    s
}

/// Parse one OpenType feature: `tag`, `+tag`, `-tag`, `tag on|off`, `tag=N`; the tag may be
/// quoted and must be exactly 4 ASCII alphanumerics.
pub fn parse_feature(s: &str) -> Option<(String, u32)> {
    let s = s.trim();
    let (sign, rest) = match s.as_bytes().first()? {
        b'+' => (Some(1), &s[1..]),
        b'-' => (Some(0), &s[1..]),
        _ => (None, s),
    };
    let (tag, value) = if let Some((tag, v)) = rest.split_once('=') {
        (tag, Some(v.trim().parse::<u32>().ok()?))
    } else if let Some((tag, word)) = rest.split_once(char::is_whitespace) {
        let v = match word.trim() {
            "on" => 1,
            "off" => 0,
            _ => return None,
        };
        (tag, Some(v))
    } else {
        (rest, None)
    };
    // A sign prefix and an explicit value together are ambiguous: reject.
    let value = match (sign, value) {
        (Some(_), Some(_)) => return None,
        (Some(v), None) | (None, Some(v)) => v,
        (None, None) => 1,
    };
    let tag = tag.trim().trim_matches('"');
    (tag.len() == 4 && tag.bytes().all(|b| b.is_ascii_alphanumeric()))
        .then(|| (tag.to_string(), value))
}

/// Build the GPUI font: the first configured family that is installed (else JetBrains Mono, else
/// Menlo), the other installed ones as fallbacks, then the bundled symbols font.
pub fn gpui_font(s: &FontSettings, installed: &[String]) -> Font {
    for f in s.families.iter().filter(|f| !installed.contains(f)) {
        eprintln!("[fuxx] config: font-family {f:?} is not installed, skipping");
    }
    let mut kept = s.families.iter().filter(|f| installed.contains(f)).cloned();
    let primary = kept.next().unwrap_or_else(|| {
        let jb = "JetBrains Mono";
        if installed.iter().any(|f| f == jb) {
            jb
        } else {
            "Menlo"
        }
        .to_string()
    });
    let mut fallbacks: Vec<String> = kept.collect();
    fallbacks.push(SYMBOLS_FAMILY.to_string());
    Font {
        family: SharedString::from(primary),
        features: FontFeatures(Arc::new(s.features.clone())),
        fallbacks: Some(FontFallbacks::from_fonts(fallbacks)),
        ..gpui::font("Menlo")
    }
}

/// Write the bundled fonts to `~/Library/Caches/fuxx/` and register them with CoreText for this
/// process. CoreText ignores fonts that only live in GPUI's memory when it picks a fallback for
/// missing glyphs, so they have to be registered files. `installed` is the font list from before
/// registering: when it already has "JetBrains Mono" the user's own copy wins and the bundled
/// JetBrains Mono files are skipped (CoreText would report a duplicate). Returns whether every
/// file it tried is registered.
pub fn register_bundled_fonts(installed: &[String]) -> bool {
    // CoreText reports a second registration of the same file as a failure (error 105, "already
    // registered"). Caching the first result makes later calls return it without re-registering,
    // and without having to decode the CFError. The first call's `installed` decides.
    static RESULT: OnceLock<bool> = OnceLock::new();
    *RESULT.get_or_init(|| {
        // An empty or relative $HOME would write the cache into the current folder.
        let home = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .filter(|h| h.is_absolute());
        let Some(home) = home else {
            eprintln!(
                "[fuxx] font: $HOME is unset or not absolute; the bundled fonts are not registered"
            );
            return false;
        };
        let dir = home.join("Library/Caches/fuxx");
        let have_jetbrains = installed.iter().any(|f| f == JETBRAINS_FAMILY);
        let mut all_ok = true;
        for (file, bytes) in BUNDLED {
            if have_jetbrains && file.starts_with("JetBrainsMono-") {
                continue;
            }
            all_ok &= install_and_register(&dir, file, bytes);
        }
        all_ok
    })
}

/// Write `bytes` to `dir/file` (unless a file of the same size is already there) and register it
/// for this process. Logs one `[fuxx] font: …` line on failure.
fn install_and_register(dir: &std::path::Path, file: &str, bytes: &[u8]) -> bool {
    let path = dir.join(file);
    let current = std::fs::metadata(&path).map(|m| m.len()).ok();
    if current != Some(bytes.len() as u64) {
        // Write a temp file and rename it over the target. Rename gives the path a new inode,
        // so another fuxx that has the old file registered or mapped keeps its copy intact,
        // and two instances starting together never see a half-written font.
        let tmp = dir.join(format!("{file}.{}.tmp", std::process::id()));
        let written = std::fs::create_dir_all(dir)
            .and_then(|_| std::fs::write(&tmp, bytes))
            .and_then(|_| std::fs::rename(&tmp, &path));
        if let Err(err) = written {
            let _ = std::fs::remove_file(&tmp);
            eprintln!("[fuxx] font: could not write {}: {err}", path.display());
            return false;
        }
    }
    let ok = register_file(path.as_os_str().as_encoded_bytes());
    if !ok {
        eprintln!("[fuxx] font: CoreText refused {}", path.display());
    }
    ok
}

#[link(name = "CoreFoundation", kind = "framework")]
#[link(name = "CoreText", kind = "framework")]
unsafe extern "C" {
    fn CFURLCreateFromFileSystemRepresentation(
        alloc: *const std::ffi::c_void,
        buf: *const u8,
        len: isize,
        is_dir: u8,
    ) -> *const std::ffi::c_void;
    fn CTFontManagerRegisterFontsForURL(
        url: *const std::ffi::c_void,
        scope: u32,
        error: *mut *const std::ffi::c_void,
    ) -> u8;
    fn CFRelease(cf: *const std::ffi::c_void);
}

/// Register the font file at `path` for this process only (`kCTFontManagerScopeProcess` = 1).
fn register_file(path: &[u8]) -> bool {
    // SAFETY: `path` outlives the call and its length is passed explicitly. The URL is checked
    // for null before use, the error out-pointer may be null, and the URL we created (Create
    // rule) is released exactly once.
    unsafe {
        let url = CFURLCreateFromFileSystemRepresentation(
            std::ptr::null(),
            path.as_ptr(),
            path.len() as isize,
            0,
        );
        if url.is_null() {
            return false;
        }
        let ok = CTFontManagerRegisterFontsForURL(url, 1, std::ptr::null_mut()) != 0;
        CFRelease(url);
        ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Entry;

    /// One headless text system for every test: creating several macOS platforms at once (tests run
    /// in parallel) aborts the process.
    fn text_system() -> Arc<dyn gpui::PlatformTextSystem> {
        static TS: OnceLock<Arc<dyn gpui::PlatformTextSystem>> = OnceLock::new();
        TS.get_or_init(|| gpui_platform::current_platform(true).text_system())
            .clone()
    }

    fn e(k: &str, v: &str) -> Entry {
        Entry {
            key: k.into(),
            value: v.into(),
        }
    }

    #[test]
    fn settings_accumulate_reset_and_default() {
        let s = from_entries(&[
            e("font-family", "A"),
            e("font-family", "B"),
            e("font-size", "x"),
            e("font-size", "14.5"),
            e("font-feature", "-calt, liga=0"),
        ]);
        assert_eq!(
            s,
            FontSettings {
                families: vec!["A".into(), "B".into()],
                size: 14.5,
                features: vec![("calt".into(), 0), ("liga".into(), 0)]
            }
        );
        let r = from_entries(&[
            e("font-family", "A"),
            e("font-family", ""),
            e("font-size", ""),
        ]);
        assert_eq!((r.families.len(), r.size), (0, DEFAULT_SIZE));
    }

    #[test]
    fn font_size_out_of_range_is_skipped() {
        for bad in ["0.3", "0", "100000"] {
            assert_eq!(
                from_entries(&[e("font-size", bad)]).size,
                DEFAULT_SIZE,
                "{bad}"
            );
        }
        assert_eq!(from_entries(&[e("font-size", "4")]).size, 4.0);
        assert_eq!(from_entries(&[e("font-size", "200")]).size, 200.0);
    }

    #[test]
    fn parse_feature_forms() {
        for (s, want) in [
            ("calt", Some(1)),
            ("+calt", Some(1)),
            ("calt on", Some(1)),
            ("calt=1", Some(1)),
            ("-calt", Some(0)),
            ("calt off", Some(0)),
            ("calt=0", Some(0)),
            ("\"ss01\"=2", Some(2)),
            ("toolong", None),
            ("ca", None),
            ("calt=x", None),
        ] {
            assert_eq!(parse_feature(s).map(|(_, v)| v), want, "{s}");
        }
    }

    #[test]
    fn gpui_font_skips_missing_families() {
        let installed = vec!["Menlo".to_string(), "MesloLGS NF".to_string()];
        let f = gpui_font(
            &FontSettings {
                families: vec!["Nope".into(), "MesloLGS NF".into(), "Menlo".into()],
                size: 13.0,
                features: vec![],
            },
            &installed,
        );
        assert_eq!(f.family.as_ref(), "MesloLGS NF");
        assert_eq!(
            f.fallbacks.unwrap().fallback_list(),
            ["Menlo".to_string(), SYMBOLS_FAMILY.to_string()]
        );
        let d = gpui_font(
            &FontSettings {
                families: vec![],
                size: 13.0,
                features: vec![],
            },
            &installed,
        );
        assert_eq!(d.family.as_ref(), "Menlo"); // JetBrains Mono not installed here
    }

    #[test]
    fn bundled_symbols_draw_the_apple_icon() {
        // Headless macOS text system (no window). Proves the icon comes from a real font, not
        // CoreText's last-resort "?" box, once the bundled font is registered.
        use gpui::{FontRun, px};
        let ts = text_system();
        assert!(register_bundled_fonts(&ts.all_font_names()));
        let font = gpui_font(
            &FontSettings {
                families: vec!["Menlo".into()],
                size: 13.0,
                features: vec![],
            },
            &ts.all_font_names(),
        );
        let id = ts.font_id(&font).unwrap();
        let text = "a\u{f179}b";
        let layout = ts.layout_line(
            text,
            px(13.0),
            &[FontRun {
                len: text.len(),
                font_id: id,
            }],
        );
        let icon_run = layout
            .runs
            .iter()
            .find(|r| r.font_id != id)
            .expect("icon drawn by a fallback font");
        let plain = ts.font_id(&gpui::font("Menlo")).unwrap();
        let last_resort = ts.layout_line(
            text,
            px(13.0),
            &[FontRun {
                len: text.len(),
                font_id: plain,
            }],
        );
        let lr = last_resort
            .runs
            .iter()
            .find(|r| r.font_id != plain)
            .map(|r| r.font_id);
        assert_ne!(
            Some(icon_run.font_id),
            lr,
            "icon must not come from the last-resort font"
        );
    }

    #[test]
    fn bundled_jetbrains_mono_is_listed_and_the_default() {
        let ts = text_system();
        assert!(register_bundled_fonts(&ts.all_font_names()));
        let installed = ts.all_font_names();
        assert!(installed.iter().any(|f| f == "JetBrains Mono"));
        let d = gpui_font(
            &FontSettings {
                families: vec![],
                size: 13.0,
                features: vec![],
            },
            &installed,
        );
        assert_eq!(d.family.as_ref(), "JetBrains Mono");
    }

    #[test]
    fn jetbrains_mono_ligatures_follow_calt() {
        // "->" is drawn with different glyphs when contextual alternates are on.
        use gpui::{FontRun, px};
        let ts = text_system();
        assert!(register_bundled_fonts(&ts.all_font_names()));
        let glyphs = |features: Vec<(String, u32)>| {
            let font = gpui_font(
                &FontSettings {
                    families: vec!["JetBrains Mono".into()],
                    size: 13.0,
                    features,
                },
                &ts.all_font_names(),
            );
            let id = ts.font_id(&font).unwrap();
            let layout = ts.layout_line(
                "->",
                px(13.0),
                &[FontRun {
                    len: 2,
                    font_id: id,
                }],
            );
            layout
                .runs
                .iter()
                .flat_map(|r| r.glyphs.iter().map(|g| g.id))
                .collect::<Vec<_>>()
        };
        assert_ne!(glyphs(vec![]), glyphs(vec![("calt".into(), 0)]));
    }
}
