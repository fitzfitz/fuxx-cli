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
/// Whether plugins that run a program may be installed.
pub const CODE_ALLOWED: bool = true;

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
    NoProgramForThisMac,
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
            Self::NoProgramForThisMac => "This plugin has no program for this Mac.".to_string(),
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

/// True when `file` is something this Mac can run: a script with `#!`, or a Mach-O with a
/// slice for this chip. The first bytes say which; `lipo` lists a Mach-O's chips.
fn runs_on_this_mac(file: &Path) -> bool {
    use std::io::Read;
    let mut head = [0u8; 4];
    let read = std::fs::File::open(file).and_then(|mut f| f.read_exact(&mut head));
    if read.is_err() {
        return false;
    }
    if head.starts_with(b"#!") {
        return true;
    }
    // Thin 64-bit Mach-O and fat (universal) headers, in either byte order.
    let mach_o = matches!(
        u32::from_be_bytes(head),
        0xfeedfacf | 0xcffaedfe | 0xcafebabe | 0xbebafeca
    );
    if !mach_o {
        return false;
    }
    std::process::Command::new("/usr/bin/lipo")
        .arg("-archs")
        .arg(file)
        .output()
        .map(|o| {
            o.status.success()
                && String::from_utf8_lossy(&o.stdout)
                    .split_whitespace()
                    .any(|a| a == crate::plugin_source::chip())
        })
        .unwrap_or(false)
}

/// At least one file directly in `bin/` must be executable on this Mac. Links and folders
/// are skipped, so a link can't borrow a program from outside the plugin.
fn has_program_for_this_mac(dir: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|e| {
        let Ok(meta) = std::fs::symlink_metadata(e.path()) else {
            return false;
        };
        meta.is_file() && meta.permissions().mode() & 0o111 != 0 && runs_on_this_mac(&e.path())
    })
}

/// Check that every file the manifest names exists inside `root` as a plain file (`bin` as a
/// folder), and that a `bin` holds a program this Mac can run.
pub fn check_files(root: &Path, m: &Manifest) -> Result<(), ManifestError> {
    for t in &m.themes {
        check_path(root, t, Kind::File)?;
    }
    if let Some(b) = &m.bin {
        check_path(root, b, Kind::Dir)?;
        if !has_program_for_this_mac(&root.join(b)) {
            return Err(ManifestError::NoProgramForThisMac);
        }
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
    fn code_plugins_are_accepted_now() {
        let m = parse("format = 1\nid = ab\nname = n\nversion = 1\nzsh = a.zsh").unwrap();
        assert!(m.runs_code());
        assert_eq!(accept(&m, CODE_ALLOWED), Ok(()));
        // The refusal still works for a build that turns code off.
        assert_eq!(accept(&m, false), Err(ManifestError::RunsCode));
    }

    fn put_executable(path: &Path, bytes: &[u8]) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(path, bytes).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn a_bin_needs_a_program_for_this_mac() {
        let root = std::env::temp_dir().join(format!("fuxx-manifest-chip-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let check = |name: &str, fill: &dyn Fn(&Path)| {
            let dir = root.join(name);
            std::fs::create_dir_all(dir.join("bin")).unwrap();
            fill(&dir.join("bin"));
            check_files(&dir, &manifest_with("bin = bin\n"))
        };
        let none = Err(ManifestError::NoProgramForThisMac);

        // Only a file without an execute bit.
        assert_eq!(
            check("plain", &|b| std::fs::write(b.join("tool"), "#!/bin/sh\n")
                .unwrap()),
            none
        );
        // An empty folder.
        assert_eq!(check("empty", &|_| {}), none);
        // An executable script.
        assert_eq!(
            check("script", &|b| put_executable(
                &b.join("tool"),
                b"#!/bin/sh\necho hi\n"
            )),
            Ok(())
        );
        // An executable that is neither a script nor a Mach-O.
        assert_eq!(
            check("junk", &|b| put_executable(&b.join("tool"), b"hello world")),
            none
        );
        // A link to a script does not count.
        assert_eq!(
            check("link", &|b| {
                put_executable(&root.join("link/real"), b"#!/bin/sh\n");
                std::os::unix::fs::symlink(root.join("link/real"), b.join("tool")).unwrap();
            }),
            none
        );
        // A Mach-O built for this chip: the test program itself.
        let me = std::env::current_exe().unwrap();
        assert_eq!(
            check("this-chip", &|b| {
                std::fs::copy(&me, b.join("tool")).unwrap();
            }),
            Ok(())
        );
        // /usr/bin/true holds an x86_64 slice, which is "the other chip" on an arm64 Mac.
        let other = if crate::plugin_source::chip() == "arm64" {
            "x86_64"
        } else {
            "arm64"
        };
        let thinned = root.join("thin-other");
        let made = std::process::Command::new("/usr/bin/lipo")
            .args(["/usr/bin/true", "-thin", other, "-output"])
            .arg(&thinned)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        // If lipo can't cut that slice out here, there is nothing to assert.
        if made {
            assert_eq!(
                check("other-chip", &|b| {
                    put_executable(&b.join("tool"), &std::fs::read(&thinned).unwrap());
                }),
                none
            );
            // A universal binary holding this chip and the other one.
            let universal = root.join("universal-bin");
            let both = std::process::Command::new("/usr/bin/lipo")
                .args(["-create"])
                .arg(&me)
                .arg(&thinned)
                .arg("-output")
                .arg(&universal)
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if both {
                assert_eq!(
                    check("universal", &|b| {
                        put_executable(&b.join("tool"), &std::fs::read(&universal).unwrap());
                    }),
                    Ok(())
                );
            }
        }
        // One good program among bad ones is enough.
        assert_eq!(
            check("mixed", &|b| {
                put_executable(&b.join("a"), b"junk");
                put_executable(&b.join("b"), b"#!/bin/sh\n");
            }),
            Ok(())
        );

        let _ = std::fs::remove_dir_all(&root);
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
        assert_eq!(
            ManifestError::NoProgramForThisMac.message(),
            "This plugin has no program for this Mac."
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
        put_executable(&root.join("tools/run"), b"#!/bin/sh\n");
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

        // A bin folder with nothing this Mac can run is refused.
        std::fs::create_dir_all(root.join("empty-bin")).unwrap();
        assert_eq!(
            check("bin = empty-bin\n"),
            Err(ManifestError::NoProgramForThisMac)
        );

        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_file(&outside);
        let _ = std::fs::remove_dir_all(&outside_dir);
    }
}
