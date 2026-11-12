//! A plugin's `fuxx-plugin.conf`: parse it, then check the files it names. The format is the
//! same `key = value` text as the settings file, so `settings::parse` does the reading.
//!
//! The parse and the file checks are separate on purpose: `parse` is pure text, `check_files`
//! touches the disk. Neither runs anything; `accept` decides whether a plugin that would run a
//! program may be installed at all.

use crate::settings;
use std::path::{Component, Path};

/// The manifest's file name, at the root of a plugin folder.
pub const MANIFEST: &str = "fuxx-plugin.conf";
/// The manifest format this fuxx understands.
pub const FORMAT: u32 = 1;
/// Whether plugins that run a program may be installed. M11z turns this on.
pub const CODE_ALLOWED: bool = false;

#[derive(Clone, Debug, PartialEq)]
pub struct Manifest {
    pub format: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    /// Paths of theme files, relative to the plugin folder, in file order.
    pub themes: Vec<String>,
    /// A folder of programs to put on `PATH`.
    pub bin: Option<String>,
    /// Shell scripts to load.
    pub zsh: Vec<String>,
}

impl Manifest {
    /// True when the plugin brings a program or shell script, not only data.
    pub fn runs_code(&self) -> bool {
        self.bin.is_some() || !self.zsh.is_empty()
    }

    /// The theme names a user sees: each theme path's last component.
    pub fn theme_names(&self) -> Vec<String> {
        self.themes
            .iter()
            .map(|t| t.rsplit('/').next().unwrap_or(t).to_string())
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ManifestError {
    Missing(&'static str),
    BadId(String),
    BadFormat(String),
    NewerFormat(u32),
    PathOutside(String),
    FileMissing(String),
    RunsCode,
}

impl ManifestError {
    /// The sentence shown to the user.
    pub fn message(&self) -> String {
        match self {
            Self::Missing(k) => format!("This plugin's fuxx-plugin.conf has no `{k}`."),
            Self::BadId(id) => format!(
                "This plugin's id \"{id}\" isn't valid (use a-z, 0-9 and -, 2 to 40 characters)."
            ),
            Self::BadFormat(s) => format!("This plugin's format \"{s}\" isn't a number."),
            Self::NewerFormat(n) => {
                format!("This plugin needs a newer fuxx (plugin format {n}).")
            }
            Self::PathOutside(p) => format!("This plugin names a file outside its folder: {p}"),
            Self::FileMissing(p) => format!("This plugin names a file it doesn't have: {p}"),
            Self::RunsCode => {
                "This plugin runs a program. fuxx can install those from a later version."
                    .to_string()
            }
        }
    }
}

/// An id is 2 to 40 characters of `a-z`, `0-9` and `-`. It becomes a folder name, so nothing
/// that could climb out of the plugins folder is allowed.
pub fn valid_id(id: &str) -> bool {
    (2..=40).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Read a manifest. Unknown keys are ignored; single keys take their last value, `theme` and
/// `zsh` collect every value. Checks run in the order format, id, name, version.
pub fn parse(text: &str) -> Result<Manifest, ManifestError> {
    let entries = settings::parse(text);
    let last = |key: &str| {
        entries
            .iter()
            .rev()
            .find(|e| e.key == key)
            .map(|e| e.value.clone())
    };
    let all = |key: &str| -> Vec<String> {
        entries
            .iter()
            .filter(|e| e.key == key)
            .map(|e| e.value.clone())
            .collect()
    };

    let format_text = last("format").ok_or(ManifestError::Missing("format"))?;
    let format: u32 = format_text
        .parse()
        .map_err(|_| ManifestError::BadFormat(format_text.clone()))?;
    if format > FORMAT {
        return Err(ManifestError::NewerFormat(format));
    }
    let id = last("id").ok_or(ManifestError::Missing("id"))?;
    if !valid_id(&id) {
        return Err(ManifestError::BadId(id));
    }
    let name = last("name")
        .filter(|v| !v.is_empty())
        .ok_or(ManifestError::Missing("name"))?;
    let version = last("version")
        .filter(|v| !v.is_empty())
        .ok_or(ManifestError::Missing("version"))?;
    Ok(Manifest {
        format,
        id,
        name,
        version,
        description: last("description"),
        themes: all("theme"),
        bin: last("bin"),
        zsh: all("zsh"),
    })
}

/// What a named path must be on disk.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    File,
    Dir,
}

fn check_path(root: &Path, rel: &str, kind: Kind) -> Result<(), ManifestError> {
    let p = Path::new(rel);
    let escapes = p.is_absolute()
        || p.components()
            .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir));
    if escapes {
        return Err(ManifestError::PathOutside(rel.to_string()));
    }
    // symlink_metadata on every prefix, so a link is seen as a link and a linked folder in the
    // middle of the path can't lead out of the plugin either
    let mut walked = root.to_path_buf();
    let mut meta = None;
    for c in p.components() {
        if let Component::Normal(part) = c {
            walked.push(part);
            let m = std::fs::symlink_metadata(&walked)
                .map_err(|_| ManifestError::FileMissing(rel.to_string()))?;
            if m.file_type().is_symlink() {
                return Err(ManifestError::PathOutside(rel.to_string()));
            }
            meta = Some(m);
        }
    }
    let meta = meta.ok_or_else(|| ManifestError::FileMissing(rel.to_string()))?;
    let right_kind = match kind {
        Kind::File => meta.is_file(),
        Kind::Dir => meta.is_dir(),
    };
    if right_kind {
        Ok(())
    } else {
        Err(ManifestError::FileMissing(rel.to_string()))
    }
}

/// Check that every file the manifest names exists inside `root` as a plain file (`bin` as a
/// folder). Checked even for code entries, so the later program-plugin work reuses it.
pub fn check_files(root: &Path, m: &Manifest) -> Result<(), ManifestError> {
    for t in &m.themes {
        check_path(root, t, Kind::File)?;
    }
    if let Some(b) = &m.bin {
        check_path(root, b, Kind::Dir)?;
    }
    for z in &m.zsh {
        check_path(root, z, Kind::File)?;
    }
    Ok(())
}

/// Refuse a plugin that runs code unless code is allowed.
pub fn accept(m: &Manifest, code_allowed: bool) -> Result<(), ManifestError> {
    if m.runs_code() && !code_allowed {
        Err(ManifestError::RunsCode)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_theme_example() {
        let m = parse(
            "format = 1\nid = sunset-themes\nname = Sunset Themes\nversion = 1.0.0\n\
             description = Warm themes.\ntheme = themes/Sunset\ntheme = themes/Sunset Light\n",
        )
        .unwrap();
        assert_eq!(m.id, "sunset-themes");
        assert_eq!(m.themes, ["themes/Sunset", "themes/Sunset Light"]);
        assert_eq!(m.theme_names(), ["Sunset", "Sunset Light"]);
        assert!(!m.runs_code());
    }

    #[test]
    fn refusals() {
        assert_eq!(
            parse("id = a1\nname = n\nversion = 1"),
            Err(ManifestError::Missing("format"))
        );
        assert_eq!(
            parse("format = 2\nid = ab\nname = n\nversion = 1"),
            Err(ManifestError::NewerFormat(2))
        );
        assert_eq!(
            parse("format = one\nid = ab\nname = n\nversion = 1"),
            Err(ManifestError::BadFormat("one".into()))
        );
        assert_eq!(
            parse("format = 1\nid = A_b\nname = n\nversion = 1"),
            Err(ManifestError::BadId("A_b".into()))
        );
    }

    #[test]
    fn an_empty_name_or_version_is_missing() {
        assert_eq!(
            parse("format = 1\nid = ab\nname =\nversion = 1"),
            Err(ManifestError::Missing("name"))
        );
        assert_eq!(
            parse("format = 1\nid = ab\nname = n\nversion ="),
            Err(ManifestError::Missing("version"))
        );
    }

    #[test]
    fn ids() {
        assert!(valid_id("ab"));
        assert!(valid_id("fuxx-completes"));
        assert!(valid_id(&"a".repeat(40)));
        assert!(!valid_id("a"));
        assert!(!valid_id(&"a".repeat(41)));
        assert!(!valid_id("Ab"));
        assert!(!valid_id("a/b"));
        assert!(!valid_id(".."));
    }

    #[test]
    fn unknown_keys_are_ignored() {
        assert!(parse("format = 1\nid = ab\nname = n\nversion = 1\nscreen = x").is_ok());
    }

    #[test]
    fn code_is_refused_in_m11y() {
        let m = parse("format = 1\nid = ab\nname = n\nversion = 1\nzsh = a.zsh").unwrap();
        assert!(m.runs_code());
        assert_eq!(accept(&m, CODE_ALLOWED), Err(ManifestError::RunsCode));
        assert_eq!(accept(&m, true), Ok(()));
    }

    #[test]
    fn messages_are_the_fixed_copy() {
        assert_eq!(
            ManifestError::Missing("id").message(),
            "This plugin's fuxx-plugin.conf has no `id`."
        );
        assert_eq!(
            ManifestError::RunsCode.message(),
            "This plugin runs a program. fuxx can install those from a later version."
        );
    }

    fn manifest_with(extra: &str) -> Manifest {
        parse(&format!(
            "format = 1\nid = ab\nname = n\nversion = 1\n{extra}"
        ))
        .unwrap()
    }

    #[test]
    fn files_must_exist_inside_the_folder() {
        let root = std::env::temp_dir().join(format!("fuxx-manifest-files-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("themes")).unwrap();
        std::fs::create_dir_all(root.join("tools")).unwrap();
        std::fs::write(root.join("themes/Sunset"), "background = 000000\n").unwrap();
        std::fs::write(root.join("a.zsh"), "").unwrap();
        let outside =
            std::env::temp_dir().join(format!("fuxx-manifest-out-{}", std::process::id()));
        std::fs::write(&outside, "x").unwrap();
        std::os::unix::fs::symlink(&outside, root.join("themes/Link")).unwrap();
        let outside_dir =
            std::env::temp_dir().join(format!("fuxx-manifest-outdir-{}", std::process::id()));
        std::fs::create_dir_all(&outside_dir).unwrap();
        std::fs::write(outside_dir.join("Sunset"), "x").unwrap();
        std::os::unix::fs::symlink(&outside_dir, root.join("linked")).unwrap();

        let check = |extra: &str| check_files(&root, &manifest_with(extra));
        assert_eq!(check("theme = themes/Sunset\n"), Ok(()));
        assert_eq!(
            check("theme = ../x\n"),
            Err(ManifestError::PathOutside("../x".into()))
        );
        assert_eq!(
            check("theme = /etc/hosts\n"),
            Err(ManifestError::PathOutside("/etc/hosts".into()))
        );
        assert_eq!(
            check("theme = themes/Nope\n"),
            Err(ManifestError::FileMissing("themes/Nope".into()))
        );
        assert_eq!(
            check("theme = themes/Link\n"),
            Err(ManifestError::PathOutside("themes/Link".into()))
        );
        assert_eq!(
            check("theme = linked/Sunset\n"),
            Err(ManifestError::PathOutside("linked/Sunset".into()))
        );
        assert_eq!(check("bin = tools\nzsh = a.zsh\n"), Ok(()));
        assert_eq!(
            check("bin = a.zsh\n"),
            Err(ManifestError::FileMissing("a.zsh".into()))
        );
        assert_eq!(
            check("zsh = ../a.zsh\n"),
            Err(ManifestError::PathOutside("../a.zsh".into()))
        );
        assert_eq!(
            check("zsh = tools\n"),
            Err(ManifestError::FileMissing("tools".into()))
        );

        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_file(&outside);
        let _ = std::fs::remove_dir_all(&outside_dir);
    }
}
