//! Read the user's Ghostty config files into a flat list of `key = value` entries.
//!
//! Pure: no filesystem access here, `load` takes a `read` closure. The conventions come from
//! Ghostty 1.3.1: comments only on their own lines, values may be quoted, an empty value resets
//! the key, a repeated key accumulates, and `config-file` includes are relative to the including
//! file, `?`-prefixed when optional, and loaded after the including file's own lines.

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

/// One `key = value` line. An empty `value` means "reset to the default".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub key: String,
    pub value: String,
}

/// Parse config text into entries, in order. Blank lines, `#` lines and lines without `=` are
/// skipped; there are no inline comments.
pub fn parse(text: &str) -> Vec<Entry> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (key, value) = line.split_once('=')?;
            let value = value.trim();
            let value = value
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .unwrap_or(value);
            Some(Entry {
                key: key.trim().to_string(),
                value: value.to_string(),
            })
        })
        .collect()
}

/// Ghostty's config file locations, lowest precedence first.
pub fn config_paths(home: &Path, xdg_config_home: Option<&Path>) -> Vec<PathBuf> {
    let xdg = xdg_config_home
        .map(Path::to_path_buf)
        .unwrap_or_else(|| home.join(".config"))
        .join("ghostty");
    let support = home.join("Library/Application Support/com.mitchellh.ghostty");
    vec![
        xdg.join("config"),
        xdg.join("config.ghostty"),
        support.join("config"),
        support.join("config.ghostty"),
    ]
}

/// Entries of every existing file in `paths`, in order. Each file's `config-file` includes are
/// expanded after its own entries; the `config-file` entries themselves are dropped.
pub fn load(paths: &[PathBuf], read: &dyn Fn(&Path) -> Option<String>) -> Vec<Entry> {
    let mut seen: HashSet<PathBuf> = paths.iter().map(|p| normalize(p)).collect();
    let mut out = Vec::new();
    for path in paths {
        if let Some(text) = read(path) {
            load_file(path, &text, read, &mut seen, &mut out);
        }
    }
    out
}

/// Lexically drop `.` components and resolve `..` against the previous one (no filesystem access).
fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn load_file(
    path: &Path,
    text: &str,
    read: &dyn Fn(&Path) -> Option<String>,
    seen: &mut HashSet<PathBuf>,
    out: &mut Vec<Entry>,
) {
    let mut includes = Vec::new();
    for entry in parse(text) {
        if entry.key == "config-file" {
            includes.push(entry.value);
        } else {
            out.push(entry);
        }
    }
    let dir = path.parent().unwrap_or(Path::new(""));
    for include in includes {
        let (optional, name) = match include.strip_prefix('?') {
            Some(rest) => (true, rest),
            None => (false, include.as_str()),
        };
        if name.is_empty() {
            continue;
        }
        let target = normalize(&dir.join(name));
        if !seen.insert(target.clone()) {
            continue;
        }
        match read(&target) {
            Some(text) => load_file(&target, &text, read, seen, out),
            None if !optional => {
                eprintln!(
                    "[fuxx] config: included file not found: {}",
                    target.display()
                );
            }
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn e(k: &str, v: &str) -> Entry {
        Entry {
            key: k.into(),
            value: v.into(),
        }
    }

    #[test]
    fn parse_handles_comments_quotes_spacing_and_resets() {
        let text = "# comment\n\nfont-family = \"MesloLGS NF\"\nfont-size=14\n  background =  #112233  \nfont-family =\nnot a setting\nforeground = abc # not a comment\n";
        assert_eq!(
            parse(text),
            vec![
                e("font-family", "MesloLGS NF"),
                e("font-size", "14"),
                e("background", "#112233"),
                e("font-family", ""),
                e("foreground", "abc # not a comment"),
            ]
        );
    }

    #[test]
    fn config_paths_follow_ghostty_order() {
        let home = Path::new("/h");
        assert_eq!(
            config_paths(home, None),
            vec![
                PathBuf::from("/h/.config/ghostty/config"),
                PathBuf::from("/h/.config/ghostty/config.ghostty"),
                PathBuf::from("/h/Library/Application Support/com.mitchellh.ghostty/config"),
                PathBuf::from(
                    "/h/Library/Application Support/com.mitchellh.ghostty/config.ghostty"
                ),
            ]
        );
        assert_eq!(
            config_paths(home, Some(Path::new("/x")))[1],
            PathBuf::from("/x/ghostty/config.ghostty")
        );
    }

    #[test]
    fn later_files_win_and_includes_follow_their_file() {
        let files: HashMap<&str, &str> = HashMap::from([
            ("/c/a", "font-size = 10\nconfig-file = inc\nfont-size = 11"),
            ("/c/inc", "font-size = 12"),
            ("/c/b", "font-size = 13\nconfig-file = ?missing"),
        ]);
        let read = |p: &Path| files.get(p.to_str().unwrap()).map(|s| s.to_string());
        let got = load(
            &[
                PathBuf::from("/c/a"),
                PathBuf::from("/nope"),
                PathBuf::from("/c/b"),
            ],
            &read,
        );
        let sizes: Vec<&str> = got
            .iter()
            .filter(|x| x.key == "font-size")
            .map(|x| x.value.as_str())
            .collect();
        assert_eq!(sizes, ["10", "11", "12", "13"]); // the include comes after its file's own lines
        assert!(got.iter().all(|x| x.key != "config-file"));
    }

    #[test]
    fn include_cycles_stop() {
        let files: HashMap<&str, &str> = HashMap::from([
            ("/c/a", "x = 1\nconfig-file = b"),
            ("/c/b", "y = 2\nconfig-file = a"),
        ]);
        let read = |p: &Path| files.get(p.to_str().unwrap()).map(|s| s.to_string());
        assert_eq!(
            load(&[PathBuf::from("/c/a")], &read),
            vec![e("x", "1"), e("y", "2")]
        );
    }

    #[test]
    fn self_include_via_dot_paths_stops() {
        let files: HashMap<&str, &str> = HashMap::from([
            ("/c/a", "x = 1\nconfig-file = ./a"),
            ("/c/b", "y = 2\nconfig-file = ../c/b"),
        ]);
        // like a real filesystem, the read resolves `.` and `..` itself
        let read = |p: &Path| {
            files
                .get(normalize(p).to_str().unwrap())
                .map(|s| s.to_string())
        };
        assert_eq!(
            load(&[PathBuf::from("/c/a"), PathBuf::from("/c/b")], &read),
            vec![e("x", "1"), e("y", "2")]
        );
    }
}
