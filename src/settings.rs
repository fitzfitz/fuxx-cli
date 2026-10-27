//! Read fuxx's settings file into a flat list of `key = value` entries. The format is
//! Ghostty-style `key = value`, but fuxx reads only its own file, never Ghostty's.
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

/// Replace every `key` line in `text` with one `key = value` line per value, leaving all other
/// lines (comments, other keys, blanks) exactly as written. The new lines go where the first old
/// one was, or at the end if the key was absent. No values just removes the key.
pub fn set_key(text: &str, key: &str, values: &[String]) -> String {
    let new_lines: String = values.iter().map(|v| format!("{key} = {v}\n")).collect();
    let mut out = String::new();
    let mut placed = false;
    for line in text.split_inclusive('\n') {
        // the same rules `parse` uses to find a line's key
        let trimmed = line.trim();
        let is_key = !trimmed.is_empty()
            && !trimmed.starts_with('#')
            && trimmed
                .split_once('=')
                .is_some_and(|(k, _)| k.trim() == key);
        if !is_key {
            out.push_str(line);
            if !line.ends_with('\n') {
                out.push('\n'); // an unterminated last line gets its newline
            }
        } else if !placed {
            out.push_str(&new_lines);
            placed = true;
        }
    }
    if !placed {
        out.push_str(&new_lines);
    }
    out
}

/// Write `text` to `path` through a temp file in the same directory and a rename, so a crash
/// never leaves a half-written settings file. The temp file is removed if anything fails.
pub fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    // Write through a symlink (dotfile managers link the config) rather than replacing it.
    let path = resolve_link(path);
    let path = path.as_path();
    let dir = path.parent().unwrap_or(Path::new(""));
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let tmp = dir.join(format!("{name}.{}.tmp", std::process::id()));
    let written = std::fs::create_dir_all(dir)
        .and_then(|_| std::fs::write(&tmp, text))
        .and_then(|_| std::fs::rename(&tmp, path));
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    written
}

/// Where a write to `path` should really land: the canonical path if it exists, the link's
/// target if it is a dangling symlink, else `path` itself.
fn resolve_link(path: &Path) -> PathBuf {
    if let Ok(real) = std::fs::canonicalize(path) {
        return real;
    }
    match std::fs::read_link(path) {
        Ok(target) => path.parent().unwrap_or(Path::new("")).join(target),
        Err(_) => path.to_path_buf(),
    }
}

/// fuxx's own settings file: `{XDG_CONFIG_HOME or ~/.config}/fuxx/config`.
pub fn settings_path(home: &Path, xdg_config_home: Option<&Path>) -> PathBuf {
    xdg_config_home
        .map(Path::to_path_buf)
        .unwrap_or_else(|| home.join(".config"))
        .join("fuxx/config")
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
    fn settings_path_is_fuxx_own_file() {
        assert_eq!(
            settings_path(Path::new("/h"), None),
            PathBuf::from("/h/.config/fuxx/config")
        );
        assert_eq!(
            settings_path(Path::new("/h"), Some(Path::new("/x"))),
            PathBuf::from("/x/fuxx/config")
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

    #[test]
    fn set_key_replaces_in_place_and_keeps_everything_else() {
        let text = "# my comment\nbackground = #000000\ntheme = Nord\nfont-size = 14\n";
        assert_eq!(
            set_key(text, "theme", &["Dracula".into()]),
            "# my comment\nbackground = #000000\ntheme = Dracula\nfont-size = 14\n"
        );
        let multi = "font-feature = -calt\nx = 1\nfont-feature = ss01\n";
        assert_eq!(
            set_key(multi, "font-feature", &["ss01=1".into()]),
            "font-feature = ss01=1\nx = 1\n"
        );
    }

    #[test]
    fn set_key_with_no_values_removes_the_key() {
        assert_eq!(set_key("theme = Nord\n# keep\n", "theme", &[]), "# keep\n");
        assert_eq!(set_key("", "theme", &[]), "");
    }

    #[test]
    fn set_key_appends_when_missing() {
        assert_eq!(
            set_key("x = 1", "theme", &["Nord".into()]),
            "x = 1\ntheme = Nord\n"
        );
        assert_eq!(set_key("", "font-size", &["15".into()]), "font-size = 15\n");
    }

    #[test]
    fn write_atomic_replaces_the_file() {
        let dir = std::env::temp_dir().join(format!("fuxx-write-atomic-{}", std::process::id()));
        let path = dir.join("sub/config");
        write_atomic(&path, "a = 1\n").unwrap();
        write_atomic(&path, "a = 2\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "a = 2\n");
        assert_eq!(
            std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
            1
        ); // no tmp left behind
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn write_atomic_follows_a_symlink() {
        let dir = std::env::temp_dir().join(format!("fuxx-write-symlink-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("real")).unwrap();
        std::fs::create_dir_all(dir.join("link")).unwrap();
        std::fs::write(dir.join("real/config"), "a = 1\n").unwrap();
        std::os::unix::fs::symlink("../real/config", dir.join("link/config")).unwrap();
        write_atomic(&dir.join("link/config"), "a = 2\n").unwrap();
        let meta = std::fs::symlink_metadata(dir.join("link/config")).unwrap();
        assert!(meta.file_type().is_symlink());
        assert_eq!(
            std::fs::read_to_string(dir.join("real/config")).unwrap(),
            "a = 2\n"
        );
        for sub in ["real", "link"] {
            let names: Vec<_> = std::fs::read_dir(dir.join(sub))
                .unwrap()
                .map(|e| e.unwrap().file_name())
                .collect();
            assert_eq!(names.len(), 1, "{sub}: {names:?}"); // no tmp left behind
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
