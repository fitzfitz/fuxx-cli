//! Where plugins live on disk, the record of what is installed, and the only code that
//! deletes plugin files. Every delete is built from `Folders` plus a validated id, and
//! never follows a symlink, so a plugin can't point an uninstall at someone else's files.

use crate::plugin_manifest::{valid_id, Manifest};
use crate::plugin_source::{Installed, Repo};
use std::path::{Path, PathBuf};

/// The two folders fuxx owns for plugins.
#[derive(Clone, Debug, PartialEq)]
pub struct Folders {
    pub plugins: PathBuf,
    pub data: PathBuf,
}

impl Folders {
    pub fn under(home: &Path) -> Folders {
        let base = home.join("Library/Application Support/fuxx");
        Folders {
            plugins: base.join("plugins"),
            data: base.join("plugin-data"),
        }
    }
    /// Where downloads are unpacked. Inside `plugins/` so a rename into place stays on one volume.
    pub fn staging(&self) -> PathBuf {
        self.plugins.join(".staging")
    }
    pub fn plugin(&self, id: &str) -> PathBuf {
        self.plugins.join(id)
    }
    pub fn data_of(&self, id: &str) -> PathBuf {
        self.data.join(id)
    }
    fn records(&self) -> PathBuf {
        self.plugins.join("plugins.json")
    }
}

/// One installed plugin, as remembered in `plugins.json`.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Record {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub repo: String,
    pub installed: Installed,
    pub themes: Vec<String>,
    pub installed_at: u64,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Table {
    plugins: Vec<Record>,
}

/// The installed plugins. A missing or damaged file means none.
pub fn load(f: &Folders) -> Vec<Record> {
    std::fs::read_to_string(f.records())
        .ok()
        .and_then(|t| serde_json::from_str::<Table>(&t).ok())
        .map(|t| t.plugins)
        .unwrap_or_default()
}

pub fn save(f: &Folders, records: &[Record]) -> std::io::Result<()> {
    let table = Table {
        plugins: records.to_vec(),
    };
    let text = serde_json::to_string_pretty(&table).map_err(std::io::Error::other)?;
    crate::settings::write_atomic(&f.records(), &text)
}

/// Delete one path we built ourselves. A symlink is unlinked, never followed.
fn remove_path(path: &Path) {
    match std::fs::symlink_metadata(path) {
        Ok(m) if m.is_dir() => {
            let _ = std::fs::remove_dir_all(path);
        }
        Ok(_) => {
            let _ = std::fs::remove_file(path);
        }
        Err(_) => {}
    }
}

fn exists(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

/// A trash name in `plugins/` that nothing is using yet.
fn trash_name(f: &Folders, id: &str, tag: u64) -> PathBuf {
    let mut n = 0u32;
    loop {
        let p = f.plugins.join(format!(".trash-{id}-{tag}-{n}"));
        if !exists(&p) {
            return p;
        }
        n += 1;
    }
}

/// Move a downloaded, checked folder into place and remember it. A second install of the
/// same id from the same repo replaces the first; from another repo it is refused.
pub fn commit(
    f: &Folders,
    plugin_dir: &Path,
    m: &Manifest,
    repo: &Repo,
    installed: &Installed,
    now: u64,
) -> Result<Record, String> {
    if !valid_id(&m.id) {
        return Err(format!("{} is not a valid plugin id.", m.id));
    }
    let mut records = load(f);
    if let Some(old) = records
        .iter()
        .find(|r| r.id == m.id && r.repo != repo.slug())
    {
        return Err(format!(
            "A plugin called {} is already installed from github.com/{}.",
            m.id, old.repo
        ));
    }
    std::fs::create_dir_all(&f.plugins)
        .map_err(|e| format!("Couldn't make the plugins folder: {e}"))?;
    let dest = f.plugin(&m.id);
    let trash = if exists(&dest) {
        let t = trash_name(f, &m.id, now);
        std::fs::rename(&dest, &t).map_err(|e| format!("Couldn't replace {}: {e}", m.id))?;
        Some(t)
    } else {
        None
    };
    if let Err(e) = std::fs::rename(plugin_dir, &dest) {
        // Put the old copy back so a failed update leaves what was there.
        if let Some(t) = &trash {
            let _ = std::fs::rename(t, &dest);
        }
        return Err(format!("Couldn't install {}: {e}", m.id));
    }
    let record = Record {
        id: m.id.clone(),
        name: m.name.clone(),
        version: m.version.clone(),
        description: m.description.clone(),
        repo: repo.slug(),
        installed: installed.clone(),
        themes: m.themes.clone(),
        installed_at: now,
    };
    records.retain(|r| r.id != m.id);
    records.push(record.clone());
    save(f, &records).map_err(|e| format!("Couldn't save the plugin list: {e}"))?;
    if let Some(t) = trash {
        remove_path(&t);
    }
    Ok(record)
}

/// Remove a plugin and its data. Deletes only `plugins/<id>/` and `plugin-data/<id>/`.
pub fn uninstall(f: &Folders, id: &str) -> Result<Record, String> {
    let unknown = || format!("No plugin called {id} is installed.");
    if !valid_id(id) {
        return Err(unknown());
    }
    let mut records = load(f);
    let Some(pos) = records.iter().position(|r| r.id == id) else {
        return Err(unknown());
    };
    let dir = f.plugin(id);
    let trash = if exists(&dir) {
        let t = trash_name(f, id, 0);
        std::fs::rename(&dir, &t).map_err(|e| format!("Couldn't remove {id}: {e}"))?;
        Some(t)
    } else {
        None
    };
    let record = records.remove(pos);
    if let Err(e) = save(f, &records) {
        if let Some(t) = &trash {
            let _ = std::fs::rename(t, &dir);
        }
        return Err(format!("Couldn't save the plugin list: {e}"));
    }
    if let Some(t) = trash {
        remove_path(&t);
    }
    remove_path(&f.data_of(id));
    Ok(record)
}

/// Mend what a crash can leave behind: half-unpacked downloads, trash, records without a
/// folder, and folders without a record. Anything not named like a plugin id is left alone.
pub fn launch_cleanup(f: &Folders) {
    if let Ok(entries) = std::fs::read_dir(f.staging()) {
        for e in entries.flatten() {
            remove_path(&e.path());
        }
    }
    if let Ok(entries) = std::fs::read_dir(&f.plugins) {
        for e in entries.flatten() {
            if e.file_name().to_string_lossy().starts_with(".trash-") {
                remove_path(&e.path());
            }
        }
    }
    let records = load(f);
    let kept: Vec<Record> = records
        .iter()
        .filter(|r| exists(&f.plugin(&r.id)))
        .cloned()
        .collect();
    if kept.len() != records.len() {
        let _ = save(f, &kept);
    }
    let has = |name: &str| kept.iter().any(|r| r.id == name);
    for dir in [&f.plugins, &f.data] {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if valid_id(&name) && !has(&name) {
                remove_path(&e.path());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    fn tmp(tag: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("fuxx-plugin-store-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }
    fn folders(tag: &str) -> (PathBuf, Folders) {
        let home = tmp(tag);
        let f = Folders::under(&home);
        std::fs::create_dir_all(f.staging()).unwrap();
        std::fs::create_dir_all(&f.data).unwrap();
        (home, f)
    }
    fn manifest(id: &str) -> Manifest {
        Manifest {
            format: 1,
            id: id.into(),
            name: "Sunset".into(),
            version: "1.0".into(),
            description: Some("d".into()),
            themes: vec!["themes/Sunset".into()],
            bin: None,
            zsh: vec![],
        }
    }
    fn repo(o: &str, n: &str) -> Repo {
        Repo {
            owner: o.into(),
            name: n.into(),
        }
    }
    /// A staged folder as the downloader would leave it.
    fn stage(f: &Folders, name: &str, file: &str) -> PathBuf {
        let d = f.staging().join(name);
        std::fs::create_dir_all(d.join("themes")).unwrap();
        std::fs::write(d.join("themes/Sunset"), "x").unwrap();
        std::fs::write(d.join(file), "x").unwrap();
        d
    }
    fn tag(t: &str) -> Installed {
        Installed::Tag(t.into())
    }

    #[test]
    fn folders_live_in_application_support() {
        let f = Folders::under(Path::new("/h"));
        assert_eq!(
            f.plugins,
            Path::new("/h/Library/Application Support/fuxx/plugins")
        );
        assert_eq!(
            f.data,
            Path::new("/h/Library/Application Support/fuxx/plugin-data")
        );
        assert_eq!(f.staging(), f.plugins.join(".staging"));
    }

    #[test]
    fn load_of_missing_or_broken_file_is_empty() {
        let (_h, f) = folders("load-empty");
        assert!(load(&f).is_empty());
        std::fs::write(f.records(), "not json").unwrap();
        assert!(load(&f).is_empty());
    }

    #[test]
    fn commit_creates_the_plugins_folder() {
        let home = tmp("commit-creates");
        let f = Folders::under(&home);
        std::fs::create_dir_all(home.join("stage")).unwrap();
        let r = commit(
            &f,
            &home.join("stage"),
            &manifest("sunset"),
            &repo("o", "a"),
            &tag("v1"),
            5,
        )
        .unwrap();
        assert_eq!(r.repo, "o/a");
        assert!(f.plugin("sunset").is_dir());
        assert_eq!(load(&f), vec![r]);
    }

    #[test]
    fn install_then_uninstall_leaves_nothing() {
        let (_h, f) = folders("roundtrip");
        let staged = stage(&f, "123-x", "a.txt");
        let r = commit(
            &f,
            &staged,
            &manifest("sunset"),
            &repo("o", "a"),
            &tag("v1"),
            7,
        )
        .unwrap();
        assert_eq!(r.themes, vec!["themes/Sunset".to_string()]);
        std::fs::create_dir_all(f.data_of("sunset")).unwrap();
        std::fs::write(f.data_of("sunset").join("state"), "s").unwrap();
        assert_eq!(load(&f).len(), 1);
        uninstall(&f, "sunset").unwrap();
        assert!(!f.plugin("sunset").exists());
        assert!(!f.data_of("sunset").exists());
        assert!(load(&f).is_empty());
        let mut left: Vec<String> = std::fs::read_dir(&f.plugins)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n != ".staging")
            .collect();
        left.sort();
        assert_eq!(left, vec!["plugins.json".to_string()]);
    }

    #[test]
    fn uninstall_never_follows_a_symlink_out() {
        let (h, f) = folders("symlink");
        let outside = h.join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("keep.txt"), "k").unwrap();
        let staged = stage(&f, "s", "a.txt");
        symlink(&outside, staged.join("escape")).unwrap();
        commit(
            &f,
            &staged,
            &manifest("sunset"),
            &repo("o", "a"),
            &tag("v1"),
            1,
        )
        .unwrap();
        uninstall(&f, "sunset").unwrap();
        assert!(outside.join("keep.txt").exists());
        assert!(!f.plugin("sunset").exists());
    }

    #[test]
    fn uninstall_removes_a_symlinked_data_folder_but_not_its_target() {
        let (h, f) = folders("data-link");
        let outside = h.join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("keep.txt"), "k").unwrap();
        let staged = stage(&f, "s", "a.txt");
        commit(
            &f,
            &staged,
            &manifest("sunset"),
            &repo("o", "a"),
            &tag("v1"),
            1,
        )
        .unwrap();
        symlink(&outside, f.data_of("sunset")).unwrap();
        uninstall(&f, "sunset").unwrap();
        assert!(outside.join("keep.txt").exists());
        assert!(!exists(&f.data_of("sunset")));
    }

    #[test]
    fn same_id_from_another_repo_is_refused() {
        let (_h, f) = folders("other-repo");
        let a = stage(&f, "a", "a.txt");
        commit(&f, &a, &manifest("sunset"), &repo("o", "a"), &tag("v1"), 1).unwrap();
        let b = stage(&f, "b", "b.txt");
        let err = commit(&f, &b, &manifest("sunset"), &repo("o", "b"), &tag("v1"), 2).unwrap_err();
        assert_eq!(
            err,
            "A plugin called sunset is already installed from github.com/o/a."
        );
        assert!(f.plugin("sunset").join("a.txt").exists());
        assert!(b.exists());
    }

    #[test]
    fn same_id_same_repo_replaces() {
        let (_h, f) = folders("replace");
        let a = stage(&f, "a", "old.txt");
        commit(&f, &a, &manifest("sunset"), &repo("o", "a"), &tag("v1"), 1).unwrap();
        let b = stage(&f, "b", "new.txt");
        let mut m = manifest("sunset");
        m.version = "2.0".into();
        commit(&f, &b, &m, &repo("o", "a"), &tag("v2"), 2).unwrap();
        let rs = load(&f);
        assert_eq!(rs.len(), 1);
        assert_eq!(rs[0].version, "2.0");
        assert_eq!(rs[0].installed, tag("v2"));
        assert!(f.plugin("sunset").join("new.txt").exists());
        assert!(!f.plugin("sunset").join("old.txt").exists());
        assert!(std::fs::read_dir(&f.plugins).unwrap().all(|e| !e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".trash-")));
    }

    #[test]
    fn uninstall_refuses_a_bad_or_unknown_id() {
        let (h, f) = folders("bad-id");
        std::fs::write(h.join("x"), "precious").unwrap();
        std::fs::create_dir_all(f.plugins.join("nope")).unwrap();
        assert_eq!(
            uninstall(&f, "../x").unwrap_err(),
            "No plugin called ../x is installed."
        );
        assert_eq!(
            uninstall(&f, "nope").unwrap_err(),
            "No plugin called nope is installed."
        );
        assert!(h.join("x").exists());
        assert!(f.plugins.join("nope").exists());
    }

    #[test]
    fn launch_cleanup_mends_a_crash() {
        let (_h, f) = folders("cleanup");
        let kept = stage(&f, "k", "a.txt");
        commit(
            &f,
            &kept,
            &manifest("sunset"),
            &repo("o", "a"),
            &tag("v1"),
            1,
        )
        .unwrap();
        let mut rs = load(&f);
        let mut gone = rs[0].clone();
        gone.id = "gone".into();
        rs.push(gone);
        save(&f, &rs).unwrap();
        stage(&f, "123-x", "half.txt");
        std::fs::create_dir_all(f.plugins.join(".trash-sunset-1/inner")).unwrap();
        std::fs::create_dir_all(f.plugins.join("orphan")).unwrap();
        std::fs::create_dir_all(f.plugins.join("Notes")).unwrap();
        std::fs::create_dir_all(f.data_of("orphan")).unwrap();
        std::fs::create_dir_all(f.data_of("sunset")).unwrap();
        std::fs::create_dir_all(f.data.join("Mine")).unwrap();
        launch_cleanup(&f);
        assert!(f.staging().is_dir());
        assert_eq!(std::fs::read_dir(f.staging()).unwrap().count(), 0);
        assert!(!f.plugins.join(".trash-sunset-1").exists());
        assert!(!f.plugin("orphan").exists());
        assert!(!f.data_of("orphan").exists());
        assert!(f.plugins.join("Notes").exists());
        assert!(f.data.join("Mine").exists());
        assert!(f.plugin("sunset").exists());
        assert!(f.data_of("sunset").exists());
        let ids: Vec<String> = load(&f).into_iter().map(|r| r.id).collect();
        assert_eq!(ids, vec!["sunset".to_string()]);
    }
}
