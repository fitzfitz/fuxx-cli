//! Where plugins live on disk, the record of what is installed, and the only code that
//! deletes plugin files. Every delete is built from `Folders` plus a validated id, and
//! never follows a symlink, so a plugin can't point an uninstall at someone else's files.

use crate::plugin_manifest::{Manifest, valid_id};
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
    /// Off keeps a helper plugin installed but not loaded. Plugins from before M11z are on.
    #[serde(default = "on")]
    pub enabled: bool,
    /// A folder of programs to put on `PATH`, from the manifest.
    #[serde(default)]
    pub bin: Option<String>,
    /// Shell scripts to load, from the manifest.
    #[serde(default)]
    pub zsh: Vec<String>,
}

fn on() -> bool {
    true
}

impl Record {
    /// True when the plugin brings a program or shell script, not only data.
    pub fn runs_code(&self) -> bool {
        self.bin.is_some() || !self.zsh.is_empty()
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Table {
    plugins: Vec<Record>,
}

/// `plugins.json` exists but can't be read as a list.
struct Damaged;

const DAMAGED: &str =
    "fuxx couldn't read its list of installed plugins, so it didn't change anything.";

/// Like `load`, but tells a missing file (no plugins yet) from a damaged one. Code that
/// deletes or rewrites must use this, or one bad edit would look like "nothing installed".
fn load_checked(f: &Folders) -> Result<Vec<Record>, Damaged> {
    match std::fs::read_to_string(f.records()) {
        Ok(t) => serde_json::from_str::<Table>(&t)
            .map(|t| t.plugins)
            .map_err(|_| Damaged),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(_) => Err(Damaged),
    }
}

/// The installed plugins. A missing or damaged file means none.
pub fn load(f: &Folders) -> Vec<Record> {
    load_checked(f).unwrap_or_default()
}

pub fn save(f: &Folders, records: &[Record]) -> std::io::Result<()> {
    let table = Table {
        plugins: records.to_vec(),
    };
    let text = serde_json::to_string_pretty(&table).map_err(std::io::Error::other)?;
    crate::settings::write_atomic(&f.records(), &text)
}

/// Make every folder under `dir` owner-writable so `remove_dir_all` can empty it. An archive
/// can hold read-only folders. Links are never followed.
pub(crate) fn open_up(dir: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let Ok(meta) = std::fs::symlink_metadata(dir) else {
        return;
    };
    if !meta.is_dir() {
        return;
    }
    let mode = meta.permissions().mode() | 0o700;
    let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(mode));
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.filter_map(|e| e.ok()) {
            open_up(&e.path());
        }
    }
}

/// Delete one path we built ourselves. A symlink is unlinked, never followed.
fn remove_path(path: &Path) {
    match std::fs::symlink_metadata(path) {
        Ok(m) if m.is_dir() => {
            open_up(path);
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
    let mut records = load_checked(f).map_err(|_| DAMAGED.to_string())?;
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
    // An update keeps On/Off: turning a helper off must survive getting a new version.
    let enabled = records
        .iter()
        .find(|r| r.id == m.id)
        .map(|r| r.enabled)
        .unwrap_or(true);
    let record = Record {
        id: m.id.clone(),
        name: m.name.clone(),
        version: m.version.clone(),
        description: m.description.clone(),
        repo: repo.slug(),
        installed: installed.clone(),
        themes: m.themes.clone(),
        installed_at: now,
        enabled,
        bin: m.bin.clone(),
        zsh: m.zsh.clone(),
    };
    // An update keeps its place: theme names are suffixed in record order.
    match records.iter().position(|r| r.id == m.id) {
        Some(i) => records[i] = record.clone(),
        None => records.push(record.clone()),
    }
    if let Err(e) = save(f, &records) {
        // Undo both moves so a failed save leaves what was there.
        let _ = std::fs::rename(&dest, plugin_dir);
        if let Some(t) = &trash {
            let _ = std::fs::rename(t, &dest);
        }
        return Err(format!("Couldn't save the plugin list: {e}"));
    }
    if let Some(t) = trash {
        remove_path(&t);
    }
    // A helper keeps its files here. Failing to make it is not worth undoing the install.
    if record.runs_code() {
        let _ = std::fs::create_dir_all(f.data_of(&m.id));
    }
    Ok(record)
}

/// Turn a plugin On or Off. Only the record changes; its files stay.
pub fn set_enabled(f: &Folders, id: &str, on: bool) -> Result<Record, String> {
    let mut records = load_checked(f).map_err(|_| DAMAGED.to_string())?;
    let Some(r) = records.iter_mut().find(|r| r.id == id) else {
        return Err(format!("No plugin called {id} is installed."));
    };
    r.enabled = on;
    let record = r.clone();
    save(f, &records).map_err(|e| format!("Couldn't save the plugin list: {e}"))?;
    Ok(record)
}

/// Remove a plugin and its data. Deletes only `plugins/<id>/` and `plugin-data/<id>/`.
pub fn uninstall(f: &Folders, id: &str) -> Result<Record, String> {
    let unknown = || format!("No plugin called {id} is installed.");
    if !valid_id(id) {
        return Err(unknown());
    }
    let mut records = load_checked(f).map_err(|_| DAMAGED.to_string())?;
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

/// The id in a `.trash-<id>-<tag>-<n>` name. Ids can hold `-`, so only the last two parts go.
fn trash_id(name: &str) -> Option<&str> {
    let rest = name.strip_prefix(".trash-")?;
    let (rest, n) = rest.rsplit_once('-')?;
    let (id, tag) = rest.rsplit_once('-')?;
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    (digits(n) && digits(tag) && valid_id(id)).then_some(id)
}

/// A crash between "old folder to trash" and "new folder into place" leaves a record with no
/// folder. Put the newest trash for it back, so the plugin and its data survive.
fn restore_trashed(f: &Folders) {
    let Ok(records) = load_checked(f) else { return };
    let Ok(entries) = std::fs::read_dir(&f.plugins) else {
        return;
    };
    let trashed: Vec<(String, std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let id = trash_id(&name)?.to_string();
            let m = std::fs::symlink_metadata(e.path()).ok()?;
            Some((id, m.modified().ok()?, e.path()))
        })
        .collect();
    for r in records.iter().filter(|r| !exists(&f.plugin(&r.id))) {
        let newest = trashed
            .iter()
            .filter(|(id, _, _)| *id == r.id)
            .max_by_key(|(_, t, _)| *t);
        if let Some((_, _, path)) = newest {
            let _ = std::fs::rename(path, f.plugin(&r.id));
        }
    }
}

/// Mend what a crash can leave behind: half-unpacked downloads, trash, records without a
/// folder, and folders without a record. Anything not named like a plugin id is left alone.
pub fn launch_cleanup(f: &Folders) {
    let staging = f.staging();
    match std::fs::symlink_metadata(&staging) {
        Ok(m) if m.is_dir() => {
            if let Ok(entries) = std::fs::read_dir(&staging) {
                for e in entries.flatten() {
                    remove_path(&e.path());
                }
            }
        }
        // A link here is not ours: unlink it, never touch what it points at.
        Ok(_) => remove_path(&staging),
        Err(_) => {}
    }
    restore_trashed(f);
    if let Ok(entries) = std::fs::read_dir(&f.plugins) {
        for e in entries.flatten() {
            if e.file_name().to_string_lossy().starts_with(".trash-") {
                remove_path(&e.path());
            }
        }
    }
    // With a damaged record list we can't tell what is an orphan, so delete nothing more.
    let Ok(records) = load_checked(f) else { return };
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
        assert!(std::fs::read_dir(&f.plugins).unwrap().all(|e| {
            !e.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".trash-")
        }));
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

    #[test]
    fn a_damaged_record_file_deletes_nothing() {
        let (_h, f) = folders("damaged");
        std::fs::create_dir_all(f.plugin("sunset")).unwrap();
        std::fs::create_dir_all(f.data_of("sunset")).unwrap();
        std::fs::write(f.records(), "{ not json").unwrap();
        launch_cleanup(&f);
        assert!(f.plugin("sunset").exists());
        assert!(f.data_of("sunset").exists());
        assert_eq!(std::fs::read_to_string(f.records()).unwrap(), "{ not json");
        let staged = stage(&f, "s", "a.txt");
        let err = commit(
            &f,
            &staged,
            &manifest("other"),
            &repo("o", "a"),
            &tag("v1"),
            1,
        )
        .unwrap_err();
        assert_eq!(err, DAMAGED);
        assert!(staged.exists());
        assert!(!f.plugin("other").exists());
        assert_eq!(uninstall(&f, "sunset").unwrap_err(), DAMAGED);
        assert!(f.plugin("sunset").exists());
        assert_eq!(std::fs::read_to_string(f.records()).unwrap(), "{ not json");
    }

    #[test]
    fn a_symlinked_staging_folder_is_unlinked_not_emptied() {
        let (h, f) = folders("staging-link");
        let outside = h.join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("keep.txt"), "k").unwrap();
        std::fs::remove_dir_all(f.staging()).unwrap();
        symlink(&outside, f.staging()).unwrap();
        launch_cleanup(&f);
        assert!(outside.join("keep.txt").exists());
        assert!(!exists(&f.staging()));
    }

    /// A staged plugin with a read-only subfolder, as an archive can leave.
    fn stage_locked(f: &Folders, name: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let d = stage(f, name, "a.txt");
        std::fs::create_dir_all(d.join("locked")).unwrap();
        std::fs::write(d.join("locked/f"), "x").unwrap();
        std::fs::set_permissions(d.join("locked"), std::fs::Permissions::from_mode(0o555)).unwrap();
        d
    }
    fn no_trash(f: &Folders) -> bool {
        std::fs::read_dir(&f.plugins).unwrap().all(|e| {
            !e.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".trash-")
        })
    }

    #[test]
    fn a_read_only_subfolder_does_not_survive_uninstall_or_update() {
        let (_h, f) = folders("readonly");
        let a = stage_locked(&f, "a");
        commit(&f, &a, &manifest("sunset"), &repo("o", "a"), &tag("v1"), 1).unwrap();
        let b = stage_locked(&f, "b");
        commit(&f, &b, &manifest("sunset"), &repo("o", "a"), &tag("v2"), 2).unwrap();
        assert!(no_trash(&f));
        uninstall(&f, "sunset").unwrap();
        assert!(no_trash(&f));
        assert!(!f.plugin("sunset").exists());
        launch_cleanup(&f);
        assert!(no_trash(&f));
    }

    #[test]
    fn an_update_keeps_its_place_in_the_list() {
        let (_h, f) = folders("order");
        for (id, n) in [("aa", "a"), ("bb", "b")] {
            let d = stage(&f, id, "x");
            commit(&f, &d, &manifest(id), &repo("o", n), &tag("v1"), 1).unwrap();
        }
        let d = stage(&f, "again", "x");
        commit(&f, &d, &manifest("aa"), &repo("o", "a"), &tag("v2"), 2).unwrap();
        let rs = load(&f);
        let ids: Vec<&str> = rs.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["aa", "bb"]);
        assert_eq!(rs[0].installed, tag("v2"));
    }

    #[test]
    fn trash_ids_keep_their_dashes() {
        assert_eq!(trash_id(".trash-my-plug-12-0"), Some("my-plug"));
        assert_eq!(trash_id(".trash-sunset-1"), None);
        assert_eq!(trash_id("orphan"), None);
    }

    #[test]
    fn a_crash_mid_update_is_undone_by_launch_cleanup() {
        let (_h, f) = folders("midcrash");
        let a = stage(&f, "a", "old.txt");
        commit(&f, &a, &manifest("my-plug"), &repo("o", "a"), &tag("v1"), 1).unwrap();
        std::fs::create_dir_all(f.data_of("my-plug")).unwrap();
        std::fs::rename(f.plugin("my-plug"), f.plugins.join(".trash-my-plug-5-0")).unwrap();
        launch_cleanup(&f);
        assert!(f.plugin("my-plug").join("old.txt").exists());
        assert!(f.data_of("my-plug").exists());
        assert_eq!(load(&f).len(), 1);
        assert!(no_trash(&f));
    }

    #[test]
    fn a_failed_save_rolls_an_update_back() {
        let (_h, f) = folders("save-fails");
        let a = stage(&f, "a", "old.txt");
        commit(&f, &a, &manifest("sunset"), &repo("o", "a"), &tag("v1"), 1).unwrap();
        std::fs::remove_file(f.records()).unwrap();
        std::fs::create_dir(f.records()).unwrap();
        let b = stage(&f, "b", "new.txt");
        assert!(commit(&f, &b, &manifest("sunset"), &repo("o", "a"), &tag("v2"), 2).is_err());
        assert!(f.plugin("sunset").join("old.txt").exists());
        assert!(b.join("new.txt").exists());
        assert!(no_trash(&f));
    }

    #[test]
    fn an_m11y_record_file_still_reads() {
        let (_h, f) = folders("m11y-record");
        std::fs::write(
            f.records(),
            r#"{"plugins":[{"id":"sunset","name":"Sunset","version":"1.0","description":null,
            "repo":"o/a","installed":{"tag":"v1"},"themes":["themes/Sunset"],"installed_at":5}]}"#,
        )
        .unwrap();
        let records = load(&f);
        assert_eq!(records.len(), 1);
        assert!(records[0].enabled);
        assert_eq!(records[0].bin, None);
        assert!(records[0].zsh.is_empty());
        assert!(!records[0].runs_code());
    }

    #[test]
    fn install_creates_the_data_folder_for_code_only() {
        let (_h, f) = folders("data-folder");
        let staged = stage(&f, "1-a", "a.txt");
        commit(
            &f,
            &staged,
            &manifest("sunset"),
            &repo("o", "a"),
            &tag("v1"),
            1,
        )
        .unwrap();
        assert!(!f.data_of("sunset").exists());

        let staged = stage(&f, "2-b", "a.zsh");
        let mut helper = manifest("helper");
        helper.zsh = vec!["a.zsh".into()];
        let r = commit(&f, &staged, &helper, &repo("o", "b"), &tag("v1"), 2).unwrap();
        assert!(f.data_of("helper").is_dir());
        assert!(r.runs_code());
        assert_eq!(r.zsh, ["a.zsh"]);
        assert!(r.enabled);
    }

    #[test]
    fn set_enabled_flips_and_survives_update() {
        let (_h, f) = folders("set-enabled");
        let mut helper = manifest("helper");
        helper.zsh = vec!["a.zsh".into()];
        let staged = stage(&f, "1-a", "a.zsh");
        commit(&f, &staged, &helper, &repo("o", "b"), &tag("v1"), 1).unwrap();

        let r = set_enabled(&f, "helper", false).unwrap();
        assert!(!r.enabled);
        assert!(!load(&f)[0].enabled);

        let staged = stage(&f, "2-a", "a.zsh");
        let r = commit(&f, &staged, &helper, &repo("o", "b"), &tag("v2"), 2).unwrap();
        assert!(!r.enabled);
        assert!(!load(&f)[0].enabled);

        assert!(set_enabled(&f, "helper", true).unwrap().enabled);
        assert_eq!(
            set_enabled(&f, "nope", true),
            Err("No plugin called nope is installed.".to_string())
        );
        std::fs::write(f.records(), "not json").unwrap();
        assert_eq!(set_enabled(&f, "helper", true), Err(DAMAGED.to_string()));
    }
}
