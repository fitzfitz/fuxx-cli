//! Download a plugin, unpack it and check it. This is the only plugin code that touches the
//! network or runs `curl`, `tar`, `ditto` and `shasum`; everything else is pure. The network is
//! behind `Net` so tests can serve archives from disk.

use crate::plugin_manifest::{self, MANIFEST, Manifest};
use crate::plugin_source::{self, ArchiveKind, Download, Installed, Repo};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The most a plugin download may weigh: 100 MB.
pub const MAX_BYTES: u64 = 104_857_600;

/// The most a plugin may unpack to: 500 MB (your call, 2026-11-13). A small archive can expand
/// enormously, and fuxx unpacks before it asks, so the size is read from the archive's own
/// listing first and nothing is written when it's over.
pub const MAX_UNPACKED: u64 = 524_288_000;

/// What a plugin over `MAX_UNPACKED` says.
pub const UNPACKS_TOO_BIG: &str = "This plugin unpacks to over 500 MB, so fuxx won't install it.";

const CANT_UNPACK: &str = "fuxx couldn't unpack this plugin.";

#[derive(Clone, Debug, PartialEq)]
pub enum FetchError {
    Offline,
    Status(u16),
    TooBig,
}

/// What fuxx needs from the network. A trait so tests never go online.
pub trait Net {
    fn get(&self, url: &str) -> Result<String, FetchError>;
    fn download(&self, url: &str, to: &Path) -> Result<(), FetchError>;
}

/// The real network: `curl`, which ships with macOS.
pub struct Curl;

/// Reads curl's output (body, a newline, then `%{http_code}`). The code matters here, since a
/// 404 means "no release" and a 403 means "rate limited", so it is kept rather than flattened.
fn answer(succeeded: bool, stdout: &str) -> Result<String, FetchError> {
    let Some((body, code)) = stdout.rsplit_once('\n') else {
        return Err(FetchError::Offline);
    };
    let Ok(code) = code.trim().parse::<u16>() else {
        return Err(FetchError::Offline);
    };
    match code {
        200..=299 if succeeded => Ok(body.to_string()),
        100..=199 | 300..=599 => Err(FetchError::Status(code)),
        _ => Err(FetchError::Offline),
    }
}

impl Net for Curl {
    fn get(&self, url: &str) -> Result<String, FetchError> {
        let out = Command::new("/usr/bin/curl")
            .args([
                "-fsSL",
                "--max-time",
                "60",
                "-H",
                "Accept: application/vnd.github+json",
                "-w",
                "\n%{http_code}",
                "--url",
                url,
            ])
            .output()
            .map_err(|_| FetchError::Offline)?;
        answer(out.status.success(), &String::from_utf8_lossy(&out.stdout))
    }

    fn download(&self, url: &str, to: &Path) -> Result<(), FetchError> {
        let out = Command::new("/usr/bin/curl")
            .args([
                "-fsSL",
                "--max-time",
                "60",
                "--max-filesize",
                "104857600",
                "-w",
                "%{http_code}",
                "-o",
            ])
            .arg(to)
            .args(["--url", url])
            .output()
            .map_err(|_| FetchError::Offline)?;
        if out.status.success() {
            return Ok(());
        }
        // Exit 63 is curl's "maximum file size exceeded".
        if out.status.code() == Some(63) {
            return Err(FetchError::TooBig);
        }
        match String::from_utf8_lossy(&out.stdout).trim().parse::<u16>() {
            Ok(code @ 400..=599) => Err(FetchError::Status(code)),
            _ => Err(FetchError::Offline),
        }
    }
}

/// The sentence shown for a network failure.
pub fn error_text(e: &FetchError) -> String {
    match e {
        FetchError::Offline => {
            "fuxx couldn't reach GitHub. Check your connection and try again.".to_string()
        }
        FetchError::Status(403) | FetchError::Status(429) => {
            "GitHub is limiting requests from this Mac for now. Try again in an hour.".to_string()
        }
        FetchError::Status(code) => format!("GitHub answered {code}. Try again later."),
        FetchError::TooBig => "This plugin is over 100 MB, so fuxx won't download it.".to_string(),
    }
}

/// Finds what to download for a repo: its latest release, else (no release) its newest commit.
pub fn latest(net: &dyn Net, repo: &Repo, chip: &str) -> Result<Download, String> {
    match net.get(&repo.release_api()) {
        Ok(body) => {
            return plugin_source::parse_release(&body)
                .map(|r| plugin_source::from_release(&r, chip));
        }
        Err(FetchError::Status(404)) => {}
        Err(e) => return Err(error_text(&e)),
    }
    match net.get(&repo.head_api()) {
        Ok(body) => {
            let sha = plugin_source::parse_head(&body)?;
            Ok(plugin_source::from_head(repo, &sha))
        }
        Err(FetchError::Status(404)) => Err(format!(
            "There's no public repo at github.com/{}.",
            repo.slug()
        )),
        Err(e) => Err(error_text(&e)),
    }
}

/// A downloaded, unpacked and checked plugin, waiting to be installed.
#[derive(Debug)]
pub struct Staged {
    /// The folder that holds `fuxx-plugin.conf`.
    pub plugin_dir: PathBuf,
    /// The scratch folder holding everything; the caller removes it when done.
    pub work: PathBuf,
    pub manifest: Manifest,
    pub installed: Installed,
}

/// The plugin's root: the folder itself if it holds the manifest, else its only visible
/// subfolder if that does (GitHub tarballs wrap everything in one folder).
pub fn find_plugin_root(dir: &Path) -> Option<PathBuf> {
    if has_manifest(dir) {
        return Some(dir.to_path_buf());
    }
    // Never follow links: an archive could point one at a folder outside itself.
    let mut subs = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| e.path());
    let only = subs.next()?;
    if subs.next().is_some() {
        return None;
    }
    has_manifest(&only).then_some(only)
}

/// True when the folder holds a manifest that is a regular file, not a link to somewhere else.
fn has_manifest(dir: &Path) -> bool {
    std::fs::symlink_metadata(dir.join(MANIFEST))
        .map(|m| m.is_file())
        .unwrap_or(false)
}

/// Deletes a scratch folder. An archive can hold read-only folders, which `remove_dir_all`
/// cannot empty, so every folder is made owner-writable first (links are never followed).
pub(crate) fn remove_work(work: &Path) {
    crate::plugin_store::open_up(work);
    let _ = std::fs::remove_dir_all(work);
}

/// Downloads into a fresh folder under `staging`, checks size and checksum, unpacks and reads
/// the manifest. On any error the scratch folder is removed before returning.
pub fn stage(net: &dyn Net, d: &Download, staging: &Path) -> Result<Staged, String> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|t| t.as_nanos())
        .unwrap_or(0);
    let work = staging.join(format!("{}-{nanos}", std::process::id()));
    let unpacked = work.join("unpacked");
    std::fs::create_dir_all(&unpacked)
        .map_err(|_| "fuxx couldn't unpack this plugin.".to_string())?;
    match stage_in(net, d, &work, &unpacked) {
        Ok((plugin_dir, manifest)) => Ok(Staged {
            plugin_dir,
            work,
            manifest,
            installed: d.installed.clone(),
        }),
        Err(e) => {
            remove_work(&work);
            Err(e)
        }
    }
}

fn stage_in(
    net: &dyn Net,
    d: &Download,
    work: &Path,
    unpacked: &Path,
) -> Result<(PathBuf, Manifest), String> {
    let archive = work.join("archive");
    net.download(&d.url, &archive).map_err(|e| error_text(&e))?;
    // A chunked download has no size header, so the size on disk is what counts.
    let len = std::fs::metadata(&archive)
        .map_err(|_| error_text(&FetchError::Offline))?
        .len();
    if len > MAX_BYTES {
        return Err(error_text(&FetchError::TooBig));
    }
    if let Some(want) = &d.sha256 {
        let got = sha256_of(&archive).unwrap_or_default();
        if !got.eq_ignore_ascii_case(want.trim()) {
            return Err(
                "The download didn't match its checksum, so fuxx didn't install it.".into(),
            );
        }
    }
    too_big_unpacked(&archive, d.kind, MAX_UNPACKED)?;
    let ok = match d.kind {
        ArchiveKind::Tarball => Command::new("/usr/bin/tar")
            .arg("-xzf")
            .arg(&archive)
            .arg("-C")
            .arg(unpacked)
            .status(),
        ArchiveKind::Zip => Command::new("/usr/bin/ditto")
            .args(["-x", "-k"])
            .arg(&archive)
            .arg(unpacked)
            .status(),
    };
    if !ok.map(|s| s.success()).unwrap_or(false) {
        return Err(CANT_UNPACK.into());
    }
    let root = find_plugin_root(unpacked)
        .ok_or_else(|| format!("This repo isn't a fuxx plugin (it has no {MANIFEST})."))?;
    let text = std::fs::read_to_string(root.join(MANIFEST))
        .map_err(|_| format!("This repo isn't a fuxx plugin (it has no {MANIFEST})."))?;
    let manifest = plugin_manifest::parse(&text).map_err(|e| e.message())?;
    plugin_manifest::check_files(&root, &manifest).map_err(|e| e.message())?;
    Ok((root, manifest))
}

/// Refuse an archive whose listed contents add up to more than `cap` bytes; one that can't be
/// listed can't be unpacked either.
fn too_big_unpacked(archive: &Path, kind: ArchiveKind, cap: u64) -> Result<(), String> {
    match unpacked_size(archive, kind) {
        Some(size) if size <= cap => Ok(()),
        Some(_) => Err(UNPACKS_TOO_BIG.into()),
        None => Err(CANT_UNPACK.into()),
    }
}

/// What `archive` would unpack to, from its listing (`tar -tvzf`, `zipinfo -t`), without
/// unpacking it.
fn unpacked_size(archive: &Path, kind: ArchiveKind) -> Option<u64> {
    let out = match kind {
        ArchiveKind::Tarball => Command::new("/usr/bin/tar")
            .arg("-tvzf")
            .arg(archive)
            .output(),
        ArchiveKind::Zip => Command::new("/usr/bin/zipinfo")
            .arg("-t")
            .arg(archive)
            .output(),
    }
    .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    match kind {
        ArchiveKind::Tarball => tar_listing_size(&text),
        ArchiveKind::Zip => zipinfo_total(&text),
    }
}

/// The sum of the sizes in a `tar -tv` listing (the fifth column); None if a line isn't one.
fn tar_listing_size(listing: &str) -> Option<u64> {
    listing
        .lines()
        .filter(|l| !l.trim().is_empty())
        .try_fold(0u64, |sum, line| {
            let size: u64 = line.split_whitespace().nth(4)?.parse().ok()?;
            sum.checked_add(size)
        })
}

/// The "N bytes uncompressed" total from `zipinfo -t`.
fn zipinfo_total(out: &str) -> Option<u64> {
    out.split(',')
        .find(|part| part.trim_end().ends_with("bytes uncompressed"))?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

fn sha256_of(file: &Path) -> Option<String> {
    let out = Command::new("/usr/bin/shasum")
        .args(["-a", "256"])
        .arg(file)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    text.split_whitespace().next().map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin_manifest::ManifestError;
    use std::collections::HashMap;

    #[derive(Default)]
    struct FakeNet {
        gets: HashMap<String, Result<String, FetchError>>,
        files: HashMap<String, PathBuf>,
        sparse: bool,
    }

    impl Net for FakeNet {
        fn get(&self, url: &str) -> Result<String, FetchError> {
            self.gets
                .get(url)
                .cloned()
                .unwrap_or(Err(FetchError::Offline))
        }
        fn download(&self, url: &str, to: &Path) -> Result<(), FetchError> {
            if self.sparse {
                let f = std::fs::File::create(to).unwrap();
                f.set_len(MAX_BYTES + 1).unwrap();
                return Ok(());
            }
            let src = self.files.get(url).ok_or(FetchError::Offline)?;
            std::fs::copy(src, to)
                .map(|_| ())
                .map_err(|_| FetchError::Offline)
        }
    }

    fn tmp(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p =
            std::env::temp_dir().join(format!("fuxx-fetch-{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn conf(extra: &str) -> String {
        format!("format = 1\nid = demo\nname = Demo\nversion = 1.0\n{extra}")
    }

    /// A GitHub-style tarball: one wrapper folder holding the plugin files.
    fn tarball(dir: &Path, files: &[(&str, &str)]) -> PathBuf {
        let wrap = dir.join("src").join("owner-repo-abc");
        for (name, text) in files {
            let p = wrap.join(name);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        }
        let out = dir.join("a.tar.gz");
        let ok = Command::new("/usr/bin/tar")
            .arg("-czf")
            .arg(&out)
            .arg("-C")
            .arg(dir.join("src"))
            .arg("owner-repo-abc")
            .status()
            .unwrap();
        assert!(ok.success());
        out
    }

    fn download(kind: ArchiveKind, sha: Option<&str>) -> Download {
        Download {
            url: "https://x/a".into(),
            kind,
            sha256: sha.map(str::to_string),
            installed: Installed::Tag("v1".into()),
        }
    }

    fn net_with(file: &Path) -> FakeNet {
        let mut n = FakeNet::default();
        n.files.insert("https://x/a".into(), file.to_path_buf());
        n
    }

    fn repo() -> Repo {
        Repo {
            owner: "o".into(),
            name: "r".into(),
        }
    }

    #[test]
    fn answer_reads_curl_output() {
        assert_eq!(answer(true, "{}\n200"), Ok("{}".to_string()));
        assert_eq!(answer(false, "\n404"), Err(FetchError::Status(404)));
        assert_eq!(answer(false, "\n403"), Err(FetchError::Status(403)));
        assert_eq!(answer(false, "\n000"), Err(FetchError::Offline));
        assert_eq!(answer(false, ""), Err(FetchError::Offline));
        assert_eq!(answer(false, "x\n200"), Err(FetchError::Offline));
    }

    #[test]
    fn release_first_then_head_then_no_repo() {
        let r = repo();
        let mut n = FakeNet::default();
        n.gets.insert(
            r.release_api(),
            Ok(r#"{"tag_name":"v2","tarball_url":"https://t/v2","assets":[]}"#.into()),
        );
        let d = latest(&n, &r, "arm64").unwrap();
        assert_eq!(d.url, "https://t/v2");

        let mut n = FakeNet::default();
        n.gets.insert(r.release_api(), Err(FetchError::Status(404)));
        n.gets.insert(r.head_api(), Ok(r#"{"sha":"abc"}"#.into()));
        let d = latest(&n, &r, "arm64").unwrap();
        assert_eq!(d, plugin_source::from_head(&r, "abc"));

        let mut n = FakeNet::default();
        n.gets.insert(r.release_api(), Err(FetchError::Status(404)));
        n.gets.insert(r.head_api(), Err(FetchError::Status(404)));
        assert_eq!(
            latest(&n, &r, "arm64"),
            Err("There's no public repo at github.com/o/r.".to_string())
        );
    }

    #[test]
    fn other_failures_use_friendly_text() {
        let r = repo();
        let mut n = FakeNet::default();
        n.gets.insert(r.release_api(), Err(FetchError::Status(403)));
        assert_eq!(
            latest(&n, &r, "arm64"),
            Err(error_text(&FetchError::Status(403)))
        );
        let mut n = FakeNet::default();
        n.gets
            .insert(r.release_api(), Ok(r#"{"message":"Boom"}"#.into()));
        assert!(latest(&n, &r, "arm64").unwrap_err().contains("Boom"));
    }

    #[test]
    fn friendly_errors() {
        assert_eq!(
            error_text(&FetchError::Offline),
            "fuxx couldn't reach GitHub. Check your connection and try again."
        );
        assert_eq!(
            error_text(&FetchError::Status(403)),
            "GitHub is limiting requests from this Mac for now. Try again in an hour."
        );
        assert_eq!(
            error_text(&FetchError::Status(429)),
            error_text(&FetchError::Status(403))
        );
        assert_eq!(
            error_text(&FetchError::Status(500)),
            "GitHub answered 500. Try again later."
        );
        assert_eq!(
            error_text(&FetchError::TooBig),
            "This plugin is over 100 MB, so fuxx won't download it."
        );
    }

    #[test]
    fn stages_a_real_tarball() {
        let dir = tmp("tar");
        let t = tarball(
            &dir,
            &[
                (MANIFEST, &conf("theme = Sunset\n")),
                ("Sunset", "background = #000000\n"),
            ],
        );
        let staging = dir.join("staging");
        let s = stage(
            &net_with(&t),
            &download(ArchiveKind::Tarball, None),
            &staging,
        )
        .unwrap();
        assert!(s.plugin_dir.ends_with("owner-repo-abc"));
        assert_eq!(s.manifest.id, "demo");
        assert_eq!(s.installed, Installed::Tag("v1".into()));
        assert!(s.work.starts_with(&staging));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stages_a_real_zip_with_files_at_its_root() {
        let dir = tmp("zip");
        let folder = dir.join("plug");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join(MANIFEST), conf("")).unwrap();
        let zip = dir.join("a.zip");
        let ok = Command::new("/usr/bin/ditto")
            .args(["-c", "-k"])
            .arg(&folder)
            .arg(&zip)
            .status()
            .unwrap();
        assert!(ok.success());
        let staging = dir.join("staging");
        let s = stage(&net_with(&zip), &download(ArchiveKind::Zip, None), &staging).unwrap();
        assert_eq!(s.plugin_dir, s.work.join("unpacked"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_repo_without_a_manifest_is_not_a_plugin() {
        let dir = tmp("nomf");
        let t = tarball(&dir, &[("README.md", "hi")]);
        let staging = dir.join("staging");
        let e = stage(
            &net_with(&t),
            &download(ArchiveKind::Tarball, None),
            &staging,
        )
        .unwrap_err();
        assert_eq!(
            e,
            "This repo isn't a fuxx plugin (it has no fuxx-plugin.conf)."
        );
        assert_eq!(std::fs::read_dir(&staging).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_checksum_mismatch_is_refused() {
        let dir = tmp("sum");
        let t = tarball(&dir, &[(MANIFEST, &conf(""))]);
        let staging = dir.join("staging");
        let zeros = "0".repeat(64);
        let e = stage(
            &net_with(&t),
            &download(ArchiveKind::Tarball, Some(&zeros)),
            &staging,
        )
        .unwrap_err();
        assert_eq!(
            e,
            "The download didn't match its checksum, so fuxx didn't install it."
        );
        assert_eq!(std::fs::read_dir(&staging).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_matching_checksum_passes_in_any_case() {
        let dir = tmp("sumok");
        let t = tarball(&dir, &[(MANIFEST, &conf(""))]);
        let sum = sha256_of(&t).unwrap().to_uppercase();
        let staging = dir.join("staging");
        let s = stage(
            &net_with(&t),
            &download(ArchiveKind::Tarball, Some(&sum)),
            &staging,
        );
        assert!(s.is_ok(), "{s:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn too_big_is_refused() {
        let dir = tmp("big");
        let staging = dir.join("staging");
        let n = FakeNet {
            sparse: true,
            ..Default::default()
        };
        let e = stage(&n, &download(ArchiveKind::Tarball, None), &staging).unwrap_err();
        assert_eq!(e, error_text(&FetchError::TooBig));
        assert_eq!(std::fs::read_dir(&staging).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn manifest_errors_come_through() {
        let dir = tmp("fmt");
        let t = tarball(
            &dir,
            &[(
                MANIFEST,
                "format = 2\nid = demo\nname = Demo\nversion = 1\n",
            )],
        );
        let staging = dir.join("staging");
        let e = stage(
            &net_with(&t),
            &download(ArchiveKind::Tarball, None),
            &staging,
        )
        .unwrap_err();
        assert_eq!(e, ManifestError::NewerFormat(2).message());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_broken_archive_cannot_be_unpacked() {
        let dir = tmp("bad");
        let junk = dir.join("junk");
        std::fs::write(&junk, "not an archive").unwrap();
        let staging = dir.join("staging");
        let e = stage(
            &net_with(&junk),
            &download(ArchiveKind::Tarball, None),
            &staging,
        )
        .unwrap_err();
        assert_eq!(e, "fuxx couldn't unpack this plugin.");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tar_listing_sizes_add_up() {
        let listing = "drwxr-xr-x  0 me staff       0 Nov 13 05:44 p/\n\
                       -rw-r--r--  0 me staff       5 Nov 13 05:44 p/a\n\
                       lrwxr-xr-x  0 me staff       0 Nov 13 05:44 p/link -> a\n\
                       -rw-r--r--  0 me staff      10 Nov 13 05:44 p/sub/b c\n";
        assert_eq!(tar_listing_size(listing), Some(15));
        assert_eq!(tar_listing_size(""), Some(0));
        assert_eq!(tar_listing_size("not a listing\n"), None);
    }

    #[test]
    fn zipinfo_total_reads_the_uncompressed_bytes() {
        let out = "8 files, 668 bytes uncompressed, 364 bytes compressed:  45.5%\n";
        assert_eq!(zipinfo_total(out), Some(668));
        assert_eq!(zipinfo_total("nonsense"), None);
    }

    #[test]
    fn the_unpacked_size_is_read_before_unpacking() {
        let dir = tmp("unpacked-size");
        let body = "x".repeat(600);
        let tgz = tarball(
            &dir,
            &[("fuxx-plugin.conf", &conf("")), ("big", body.as_str())],
        );
        let size = unpacked_size(&tgz, ArchiveKind::Tarball).unwrap();
        assert!((600..2000).contains(&size), "{size}");
        assert!(too_big_unpacked(&tgz, ArchiveKind::Tarball, 1000).is_ok());
        assert_eq!(
            too_big_unpacked(&tgz, ArchiveKind::Tarball, 500),
            Err(UNPACKS_TOO_BIG.to_string())
        );
        // A zip is measured too.
        let zip = dir.join("a.zip");
        let ok = Command::new("/usr/bin/ditto")
            .args(["-c", "-k", "--norsrc"])
            .arg(dir.join("src").join("owner-repo-abc"))
            .arg(&zip)
            .status()
            .unwrap();
        assert!(ok.success());
        assert!(unpacked_size(&zip, ArchiveKind::Zip).unwrap() >= 600);
        assert_eq!(
            too_big_unpacked(&zip, ArchiveKind::Zip, 500),
            Err(UNPACKS_TOO_BIG.to_string())
        );
        // Something that can't be listed is refused as unpackable.
        let junk = dir.join("junk");
        std::fs::write(&junk, "not an archive").unwrap();
        assert_eq!(
            too_big_unpacked(&junk, ArchiveKind::Tarball, MAX_UNPACKED),
            Err("fuxx couldn't unpack this plugin.".to_string())
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_unpacked_cap_is_500_mb() {
        assert_eq!(MAX_UNPACKED, 500 * 1024 * 1024);
        assert_eq!(
            UNPACKS_TOO_BIG,
            "This plugin unpacks to over 500 MB, so fuxx won't install it."
        );
    }

    #[test]
    fn a_read_only_folder_is_still_cleaned_up() {
        let dir = tmp("ro");
        let wrap = dir.join("src").join("owner-repo-abc");
        std::fs::create_dir_all(wrap.join("locked")).unwrap();
        std::fs::write(wrap.join("locked/f"), "x").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(wrap.join("locked"), std::fs::Permissions::from_mode(0o555))
            .unwrap();
        let t = dir.join("a.tar.gz");
        let ok = Command::new("/usr/bin/tar")
            .arg("-czf")
            .arg(&t)
            .arg("-C")
            .arg(dir.join("src"))
            .arg("owner-repo-abc")
            .status()
            .unwrap();
        assert!(ok.success());
        std::fs::set_permissions(wrap.join("locked"), std::fs::Permissions::from_mode(0o755))
            .unwrap();
        let staging = dir.join("staging");
        let e = stage(
            &net_with(&t),
            &download(ArchiveKind::Tarball, None),
            &staging,
        )
        .unwrap_err();
        assert!(e.starts_with("This repo isn't a fuxx plugin"));
        assert_eq!(std::fs::read_dir(&staging).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn tar_of(dir: &Path, entry: &str) -> PathBuf {
        let out = dir.join("a.tar.gz");
        let ok = Command::new("/usr/bin/tar")
            .arg("-czf")
            .arg(&out)
            .arg("-C")
            .arg(dir.join("src"))
            .arg(entry)
            .status()
            .unwrap();
        assert!(ok.success());
        out
    }

    #[test]
    fn a_symlinked_plugin_folder_is_refused() {
        let dir = tmp("lnkdir");
        let outside = dir.join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join(MANIFEST), conf("")).unwrap();
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::os::unix::fs::symlink(&outside, dir.join("src/owner-repo-abc")).unwrap();
        let t = tar_of(&dir, "owner-repo-abc");
        let staging = dir.join("staging");
        let e = stage(
            &net_with(&t),
            &download(ArchiveKind::Tarball, None),
            &staging,
        )
        .unwrap_err();
        assert_eq!(
            e,
            "This repo isn't a fuxx plugin (it has no fuxx-plugin.conf)."
        );
        assert_eq!(std::fs::read_dir(&staging).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_symlinked_manifest_is_refused() {
        let dir = tmp("lnkconf");
        let outside = dir.join("real.conf");
        std::fs::write(&outside, conf("")).unwrap();
        let wrap = dir.join("src").join("owner-repo-abc");
        std::fs::create_dir_all(&wrap).unwrap();
        std::os::unix::fs::symlink(&outside, wrap.join(MANIFEST)).unwrap();
        let t = tar_of(&dir, "owner-repo-abc");
        let staging = dir.join("staging");
        let e = stage(
            &net_with(&t),
            &download(ArchiveKind::Tarball, None),
            &staging,
        )
        .unwrap_err();
        assert_eq!(
            e,
            "This repo isn't a fuxx plugin (it has no fuxx-plugin.conf)."
        );
        assert_eq!(std::fs::read_dir(&staging).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn example_plugin_installs_and_uninstalls_cleanly() {
        use crate::plugin_source::Repo;
        use crate::plugin_store::{self, Folders};
        use crate::plugin_themes;

        let dir = tmp("example");
        let wrap = dir.join("src").join("owner-repo-sha");
        let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/plugins/sunset-themes");
        std::fs::create_dir_all(&wrap).unwrap();
        let ok = Command::new("/bin/cp")
            .arg("-R")
            .arg(format!("{}/.", example.display()))
            .arg(&wrap)
            .status()
            .unwrap();
        assert!(ok.success());
        let tar = dir.join("a.tar.gz");
        let ok = Command::new("/usr/bin/tar")
            .arg("-czf")
            .arg(&tar)
            .arg("-C")
            .arg(dir.join("src"))
            .arg("owner-repo-sha")
            .status()
            .unwrap();
        assert!(ok.success());

        let repo = Repo {
            owner: "owner".into(),
            name: "repo".into(),
        };
        let mut net = FakeNet::default();
        net.gets
            .insert(repo.release_api(), Err(FetchError::Status(404)));
        net.gets
            .insert(repo.head_api(), Ok(r#"{"sha":"abc1234def"}"#.to_string()));
        let tarball_url = "https://api.github.com/repos/owner/repo/tarball/abc1234def";
        net.files.insert(tarball_url.into(), tar);

        let download = latest(&net, &repo, "arm64").unwrap();
        assert_eq!(download.url, tarball_url);
        let home = dir.join("home");
        let folders = Folders::under(&home);
        let staged = stage(&net, &download, &folders.staging()).unwrap();
        crate::plugin_manifest::accept(&staged.manifest, false).unwrap();
        assert_eq!(staged.manifest.id, "sunset-themes");
        let record = plugin_store::commit(
            &folders,
            &staged.plugin_dir,
            &staged.manifest,
            &repo,
            &staged.installed,
            1,
        )
        .unwrap();
        remove_work(&staged.work);
        assert_eq!(record.themes.len(), 2);

        let records = plugin_store::load(&folders);
        let themes = plugin_themes::list(&folders, &records, &[]);
        let names: Vec<&str> = themes.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["Sunset", "Sunset Light"]);
        for name in names {
            let text = crate::appearance::read_theme_text(name, None, &themes).unwrap();
            assert!(text.contains("background ="), "{name} reads as a theme");
        }

        plugin_store::uninstall(&folders, "sunset-themes").unwrap();
        assert!(!folders.plugin("sunset-themes").exists());
        assert!(!folders.data_of("sunset-themes").exists());
        assert!(plugin_store::load(&folders).is_empty());
        let left: Vec<String> = std::fs::read_dir(&folders.plugins)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n != "plugins.json" && n != ".staging")
            .collect();
        assert!(left.is_empty(), "left behind: {left:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn helper_plugin_loads_and_uninstalls_cleanly() {
        use crate::plugin_source::Repo;
        use crate::plugin_store::{self, Folders};
        use crate::shell_integration::test_zsh::{run, screen_text, spawn};

        let dir = tmp("helper");
        let wrap = dir.join("src").join("owner-repo-sha");
        let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/plugins/hello-helper");
        std::fs::create_dir_all(&wrap).unwrap();
        // `cp -R` keeps the executable bit, as a GitHub tarball does.
        let ok = Command::new("/bin/cp")
            .arg("-R")
            .arg(format!("{}/.", example.display()))
            .arg(&wrap)
            .status()
            .unwrap();
        assert!(ok.success());
        let tar = dir.join("a.tar.gz");
        let ok = Command::new("/usr/bin/tar")
            .arg("-czf")
            .arg(&tar)
            .arg("-C")
            .arg(dir.join("src"))
            .arg("owner-repo-sha")
            .status()
            .unwrap();
        assert!(ok.success());

        let repo = Repo {
            owner: "owner".into(),
            name: "repo".into(),
        };
        let mut net = FakeNet::default();
        net.gets
            .insert(repo.release_api(), Err(FetchError::Status(404)));
        net.gets
            .insert(repo.head_api(), Ok(r#"{"sha":"abc1234def"}"#.to_string()));
        let tarball_url = "https://api.github.com/repos/owner/repo/tarball/abc1234def";
        net.files.insert(tarball_url.into(), tar);

        let download = latest(&net, &repo, "arm64").unwrap();
        let home = dir.join("home");
        let folders = Folders::under(&home);
        let staged = stage(&net, &download, &folders.staging()).unwrap();
        crate::plugin_manifest::accept(&staged.manifest, true).unwrap();
        assert_eq!(staged.manifest.id, "hello-helper");
        let record = plugin_store::commit(
            &folders,
            &staged.plugin_dir,
            &staged.manifest,
            &repo,
            &staged.installed,
            1,
        )
        .unwrap();
        remove_work(&staged.work);
        assert!(record.runs_code());

        let si = dir.join("si");
        crate::shell_integration::install(&si).unwrap();
        let load = || {
            crate::plugin_shell::write(&si, &folders, &plugin_store::load(&folders)).unwrap();
            let (core, _rx) = spawn(&home, None, &si, true, true);
            // Short output: the screen wraps at 80 columns.
            run(
                &core,
                "print -r -- \"[$(fuxx-hello 2>&1 | head -c 20)][${FUXX_HELLO-none}][${FUXX_HELLO_DATA:h:t}/${FUXX_HELLO_DATA:t}]\"",
            );
            screen_text(&core)
        };
        let screen = load();
        assert!(
            screen.contains("[hello from a fuxx pl][1][plugin-data/hello-helper]"),
            "screen:\n{screen}"
        );

        plugin_store::set_enabled(&folders, "hello-helper", false).unwrap();
        let screen = load();
        assert!(
            screen.contains("[zsh: command not fou][none][./]"),
            "screen:\n{screen}"
        );

        plugin_store::uninstall(&folders, "hello-helper").unwrap();
        crate::plugin_shell::write(&si, &folders, &plugin_store::load(&folders)).unwrap();
        let script = std::fs::read_to_string(si.join("zsh/plugins.zsh")).unwrap();
        assert_eq!(script.lines().count(), 1, "only the header: {script}");
        assert!(!folders.plugin("hello-helper").exists());
        assert!(!folders.data_of("hello-helper").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
