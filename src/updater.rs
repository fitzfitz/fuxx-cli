// The self-updater's effects: asking GitHub for the latest release (at launch, daily, and from
// fuxx → Check for Updates…), what the app knows about it (`Updates`), and installing it
// (download, check, swap and reopen).
//
// The fetch and the download block (curl, shasum, ditto, plutil, codesign), so they run on the
// background executor; their outcomes are handled on the main thread. What to do with an answer
// is decided by the pure functions here and in `update_release`; `fetch_latest` and
// `download_and_check` are the only parts that touch the network.

use std::ffi::{CString, OsStr, OsString, c_char, c_int};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use gpui::{App, Global, PromptLevel, Window};

use crate::update_release::{self, CheckOutcome, Release, Unpacked, Version};
use crate::workspace::{Session, Workspace};
use crate::workspace_daemon::Service;

unsafe extern "C" {
    fn access(path: *const c_char, mode: c_int) -> c_int;
}

/// `access`'s "may write" mode.
const W_OK: c_int = 2;

/// GitHub's latest release of fuxx, as JSON. Public repo: no token.
pub const API_URL: &str = "https://api.github.com/repos/fitzfitz/fuxx-cli/releases/latest";
/// The latest release's page, for when the JSON gives none (or one that isn't fuxx's).
pub const RELEASES_PAGE: &str = "https://github.com/fitzfitz/fuxx-cli/releases/latest";
/// The first automatic check, after launch.
pub const FIRST_CHECK: Duration = Duration::from_secs(10);
/// Then one a day while fuxx runs.
pub const EVERY: Duration = Duration::from_secs(24 * 60 * 60);

/// What the app knows about updates.
#[derive(Clone, Debug, PartialEq)]
pub enum UpdateState {
    Idle,
    /// A check found this newer release.
    Available(Release),
    /// Downloading and checking it (`install`), until fuxx quits for the swap or it fails.
    Downloading,
}

/// The sidebar's button for `state`: its words, and whether a click opens the offer. None
/// while nothing newer is known.
pub fn button(state: &UpdateState) -> Option<(&'static str, bool)> {
    match state {
        UpdateState::Idle => None,
        UpdateState::Available(_) => Some(("Update Available", true)),
        UpdateState::Downloading => Some(("Downloading update\u{2026}", false)),
    }
}

/// Where a release link fuxx opens goes: the release's own page only when it is one of fuxx's
/// on GitHub, else the latest release's (the JSON doesn't get to pick what the browser opens).
pub fn release_link(release: &Release) -> &str {
    if release
        .page_url
        .starts_with("https://github.com/fitzfitz/fuxx-cli/")
    {
        &release.page_url
    } else {
        RELEASES_PAGE
    }
}

/// The `fuxx.app` to swap, for the running binary `exe`: the app three levels up
/// (`fuxx.app/Contents/MacOS/fuxx`), unless it is on a disk image (`/Volumes/`), run from where
/// macOS moved it (`/AppTranslocation/`), or in a folder that isn't `writable` (where the swap
/// renames it). None means fuxx can't update itself.
pub fn self_update_target(exe: &Path, writable: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let app = contents.parent()?;
    let named = |p: &Path, name: &str| p.file_name().is_some_and(|n| n == name);
    if !(named(macos, "MacOS") && named(contents, "Contents") && named(app, "fuxx.app")) {
        return None;
    }
    let text = app.to_string_lossy();
    if text.starts_with("/Volumes/") || text.contains("/AppTranslocation/") {
        return None;
    }
    writable(app.parent()?).then(|| app.to_path_buf())
}

/// Whether `release` can be installed in place: it has the `fuxx-{version}.zip` with a digest
/// to check it by. Otherwise the offer sends you to GitHub.
pub fn installable(release: &Release) -> bool {
    release.zip.as_ref().is_some_and(|zip| zip.sha256.is_some())
}

/// Whether this process may write in `dir` (`access(2)`; one quick syscall).
fn writable(dir: &Path) -> bool {
    let Ok(path) = CString::new(dir.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: `path` is a valid NUL-terminated string that outlives the call.
    unsafe { access(path.as_ptr(), W_OK) == 0 }
}

/// The app an update would swap, for the running binary `exe`: `self_update_target`, and only
/// when its path is UTF-8 (the swap script is handed it as text). The one condition both the
/// offer (`can_self_update`) and the install (`install`) use, so they can't disagree.
fn update_app_for(exe: &Path, writable: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    self_update_target(exe, writable).filter(|app| app.to_str().is_some())
}

/// `update_app_for` the running binary, by the real `writable`.
fn running_update_app() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|exe| update_app_for(&exe, writable))
}

/// Whether this fuxx can update itself to `release`: run from a `fuxx.app` it may swap
/// (`self_update_target`), and the release has a checked zip.
fn can_self_update(release: &Release) -> bool {
    crate::native_notify::is_bundled() && installable(release) && running_update_app().is_some()
}

/// How many sessions restarting ends: none while the background service holds them (the new
/// fuxx reattaches), else every terminal session running here, `local_sessions`. That covers
/// persistence off, a service that couldn't start or is still starting, and a lost one.
fn ending_sessions(service_on: bool, local_sessions: usize) -> usize {
    if service_on { 0 } else { local_sessions }
}

/// `ending_sessions` for `workspace`, by its live background service (not the persistence
/// asked for at startup). Its running sessions are counted as the Quit item counts them
/// (`sync_quit_title`): not lost, and not a held SSH session that has ended.
fn sessions_restart_ends(workspace: &Workspace) -> usize {
    let service_on = matches!(workspace.service, Service::On(_));
    let running = |s: &&Session| !s.lost && s.disconnected.is_none();
    let local = workspace.sessions.items().iter().filter(running).count();
    ending_sessions(service_on, local)
}

/// The app's update state (a global, set at launch).
pub struct Updates {
    pub state: UpdateState,
}

impl Global for Updates {}

/// The state after a check: a newer release becomes available, unless an update is already
/// under way. Up to date or failed changes nothing.
fn next_state(current: &UpdateState, outcome: &CheckOutcome) -> UpdateState {
    match (current, outcome) {
        (UpdateState::Downloading, _) => UpdateState::Downloading,
        (_, CheckOutcome::Newer(release)) => UpdateState::Available(release.clone()),
        (state, _) => state.clone(),
    }
}

/// curl's arguments: fail on an HTTP error, quiet but for errors, follow redirects, 20 s at
/// most, GitHub's JSON, and the status code on a last line of its own.
fn curl_args() -> [&'static str; 8] {
    [
        "-fsSL",
        "--max-time",
        "20",
        "-H",
        "Accept: application/vnd.github+json",
        "-w",
        "\n%{http_code}",
        API_URL,
    ]
}

/// The body from curl's output (`body`, a newline, the status code), given whether curl
/// succeeded: GitHub's non-2xx status is an error naming it; no status (`000`) or a transfer
/// curl gave up on is "offline or unreachable".
fn curl_answer(succeeded: bool, stdout: &str) -> Result<String, String> {
    let offline = || "offline or unreachable".to_string();
    let (body, code) = stdout.rsplit_once('\n').ok_or_else(offline)?;
    let code: u16 = code.trim().parse().map_err(|_| offline())?;
    match code {
        200..=299 if succeeded => Ok(body.to_string()),
        100..=599 if !(200..=299).contains(&code) => Err(format!("GitHub answered {code}")),
        _ => Err(offline()),
    }
}

/// Ask GitHub for the latest release's JSON. Blocks for up to 20 s: call it off the UI thread.
pub fn fetch_latest() -> Result<String, String> {
    let out = std::process::Command::new("/usr/bin/curl")
        .args(curl_args())
        .output()
        .map_err(|_| "offline or unreachable".to_string())?;
    curl_answer(out.status.success(), &String::from_utf8_lossy(&out.stdout))
}

/// A sheet that only informs: its title, its text, and its buttons in order.
#[derive(Clone, Debug, PartialEq)]
struct Sheet {
    title: String,
    detail: Option<String>,
    buttons: Vec<&'static str>,
}

/// The manual check's sheet for what it found; None for a newer release, which gets the
/// update offer (`show_offer`) the button opens.
fn manual_sheet(outcome: &CheckOutcome) -> Option<Sheet> {
    match outcome {
        CheckOutcome::Newer(_) => None,
        CheckOutcome::UpToDate(v) => Some(Sheet {
            title: update_release::up_to_date_title(*v),
            detail: None,
            buttons: vec!["OK"],
        }),
        CheckOutcome::Failed(reason) => Some(Sheet {
            title: update_release::CHECK_FAILED.to_string(),
            detail: Some(reason.clone()),
            buttons: vec!["OK"],
        }),
    }
}

/// Check now: fetch and read the latest release off the UI thread, then on the main thread a
/// newer one becomes `Available` (and the windows redraw). A `manual` check always answers in a
/// sheet on the main fuxx window; an automatic one is silent but for a log line when it fails.
pub fn check(manual: bool, cx: &mut App) {
    let fetch = cx.background_executor().spawn(async {
        let result = fetch_latest().and_then(|json| update_release::parse_latest(&json));
        update_release::check_outcome(result, update_release::current())
    });
    cx.spawn(async move |cx| {
        let outcome = fetch.await;
        cx.update(|cx| found(outcome, manual, cx));
    })
    .detach();
}

/// Act on a check's outcome (main thread).
fn found(outcome: CheckOutcome, manual: bool, cx: &mut App) {
    match &outcome {
        CheckOutcome::Newer(r) => eprintln!("[fuxx] update check: {} is out", r.version),
        CheckOutcome::UpToDate(v) => eprintln!("[fuxx] update check: {v} is the newest"),
        CheckOutcome::Failed(reason) => eprintln!("[fuxx] update check failed: {reason}"),
    }
    let current = cx
        .try_global::<Updates>()
        .map_or(UpdateState::Idle, |u| u.state.clone());
    let next = next_state(&current, &outcome);
    if next != current {
        cx.set_global(Updates { state: next });
        cx.refresh_windows();
    }
    if manual {
        match manual_sheet(&outcome) {
            Some(sheet) => show_sheet(sheet, cx),
            None => crate::with_help_window(cx, |workspace, _, window, cx| {
                show_offer(workspace, window, cx)
            }),
        }
    }
}

/// `sheet` on the main fuxx window.
fn show_sheet(sheet: Sheet, cx: &mut App) {
    crate::with_help_window(cx, move |_, _, window, cx| {
        let answer = window.prompt(
            PromptLevel::Info,
            &sheet.title,
            sheet.detail.as_deref(),
            sheet.buttons.as_slice(),
            cx,
        );
        // Its only button closes it; the answer is waited for and dropped.
        cx.spawn(async move |_, _| {
            answer.await.ok();
        })
        .detach();
    });
}

/// The update offer (`update_release::offer`) as a sheet on `window`, for the release now
/// `Available`; nothing in any other state (so a click while downloading does nothing). Update
/// and Restart installs it; Download from GitHub… opens its page; Later just closes it. Both
/// the Update Available button and fuxx → Check for Updates… show it.
pub fn show_offer(workspace: &Workspace, window: &mut Window, cx: &mut App) {
    let UpdateState::Available(release) = cx
        .try_global::<Updates>()
        .map_or(UpdateState::Idle, |u| u.state.clone())
    else {
        return;
    };
    let offer = update_release::offer(
        &release,
        update_release::current(),
        sessions_restart_ends(workspace),
        can_self_update(&release),
    );
    let answer = window.prompt(
        PromptLevel::Info,
        &offer.title,
        Some(&offer.detail),
        offer.buttons.as_slice(),
        cx,
    );
    cx.spawn(async move |cx| {
        if answer.await.ok() != Some(0) {
            return; // Later, or the sheet went away
        }
        cx.update(|cx| {
            if offer.self_update {
                install(release, cx);
            } else {
                cx.open_url(release_link(&release));
            }
        });
    })
    .detach();
}

/// Whether the sheet's Update and Restart for `release` still applies: it is the release on offer
/// now (`state`). An answer can arrive after a second offer, or once an update is under way.
fn should_install(state: &UpdateState, release: &Release) -> bool {
    matches!(state, UpdateState::Available(offered) if offered.version == release.version)
}

/// The sheet for an update that couldn't be installed: the reason, Open Release Page and OK.
fn install_failed_sheet(reason: String) -> Sheet {
    Sheet {
        title: update_release::INSTALL_FAILED.to_string(),
        detail: Some(reason),
        buttons: vec!["Open Release Page", "OK"],
    }
}

/// The download folder, `~/Library/Caches/fuxx/update/` under `home`.
fn cache_dir(home: &Path) -> PathBuf {
    home.join("Library/Caches/fuxx/update")
}

/// curl's arguments for the download: fail on an HTTP error, follow redirects (GitHub's assets
/// redirect), 10 minutes at most, into `zip`. The URL comes after `--url`, so it is never read as
/// an option.
fn download_args(url: &str, zip: &Path) -> [OsString; 7] {
    [
        "-fL".into(),
        "--max-time".into(),
        "600".into(),
        "-o".into(),
        zip.into(),
        "--url".into(),
        url.into(),
    ]
}

/// The digest from `shasum -a 256`'s output (`<hex>  <path>`); empty when it printed none.
fn digest_from_shasum(out: &str) -> String {
    out.split_whitespace().next().unwrap_or("").to_string()
}

/// The `*.app` folders directly in `dir` (not files, not links), sorted; none when `dir` can't
/// be read.
fn apps_in(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut apps: Vec<PathBuf> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "app"))
        .collect();
    apps.sort();
    apps
}

/// Run `program args…` and give its standard output, or None when it couldn't run or failed.
fn output_of(program: &str, args: &[&OsStr]) -> Option<String> {
    let out = Command::new(program).args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// `key` from `app`'s Info.plist (`plutil -extract <key> raw`), trimmed; empty when unreadable.
fn plist_value(app: &Path, key: &str) -> String {
    let plist = app.join("Contents/Info.plist");
    let args = [
        OsStr::new("-extract"),
        OsStr::new(key),
        OsStr::new("raw"),
        OsStr::new("-o"),
        OsStr::new("-"),
        plist.as_os_str(),
    ];
    output_of("/usr/bin/plutil", &args).map_or_else(String::new, |v| v.trim().to_string())
}

/// What the unpacked zip in `dir` holds, for `update_release::verify`, and its one app if it
/// holds exactly one. The app is read (Info.plist, `codesign --verify --deep --strict`) only then.
fn read_unpacked(dir: &Path) -> (Unpacked, Option<PathBuf>) {
    match apps_in(dir).as_slice() {
        [app] => {
            let verify = ["--verify", "--deep", "--strict"].map(OsStr::new);
            let signature_ok = output_of(
                "/usr/bin/codesign",
                &[&verify[..], &[app.as_os_str()]].concat(),
            )
            .is_some();
            let unpacked = Unpacked {
                bundle_id: plist_value(app, "CFBundleIdentifier"),
                version: plist_value(app, "CFBundleShortVersionString"),
                signature_ok,
                apps: 1,
                app_name: app
                    .file_name()
                    .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
            };
            (unpacked, Some(app.clone()))
        }
        apps => {
            let unpacked = Unpacked {
                bundle_id: String::new(),
                version: String::new(),
                signature_ok: false,
                apps: apps.len(),
                app_name: String::new(),
            };
            (unpacked, None)
        }
    }
}

/// Download the release `version`'s zip from `url` into `cache` (emptied first), and check it
/// against its digest `sha256` and `update_release::verify`. Ok is the unpacked app, ready to
/// swap in; Err is the reason for the error sheet. Nothing outside `cache` is touched. Blocks
/// (the download may take up to 10 minutes): call it off the UI thread.
fn download_and_check(
    version: Version,
    url: &str,
    sha256: &str,
    cache: &Path,
) -> Result<PathBuf, String> {
    // Emptied first, so nothing from an earlier try is checked or swapped in.
    if cache.exists() {
        std::fs::remove_dir_all(cache).map_err(|_| update_release::CACHE_FAILED)?;
    }
    std::fs::create_dir_all(cache).map_err(|_| update_release::CACHE_FAILED)?;
    let zip = cache.join(format!("fuxx-{version}.zip"));
    let args = download_args(url, &zip);
    let args: Vec<&OsStr> = args.iter().map(OsString::as_os_str).collect();
    output_of("/usr/bin/curl", &args).ok_or(update_release::DOWNLOAD_FAILED)?;
    let actual = output_of(
        "/usr/bin/shasum",
        &[OsStr::new("-a"), OsStr::new("256"), zip.as_os_str()],
    )
    .map_or_else(String::new, |out| digest_from_shasum(&out));
    // A damaged download isn't even unpacked.
    if !update_release::checksum_matches(sha256, &actual) {
        return Err("the download is damaged (checksum)".into());
    }
    let unpacked = cache.join("unpacked");
    let ditto = [
        OsStr::new("-x"),
        OsStr::new("-k"),
        zip.as_os_str(),
        unpacked.as_os_str(),
    ];
    let (facts, app) = match output_of("/usr/bin/ditto", &ditto) {
        Some(_) => read_unpacked(&unpacked),
        // It didn't unpack: count it as holding no app.
        None => (
            Unpacked {
                bundle_id: String::new(),
                version: String::new(),
                signature_ok: false,
                apps: 0,
                app_name: String::new(),
            },
            None,
        ),
    };
    update_release::verify(version, sha256, &actual, &facts)?;
    app.ok_or_else(|| format!("the zip holds {} apps", facts.apps))
}

/// Write the swap script (`update_release::swap_script`, for this process) into `cache`, and
/// start it detached with `/bin/sh`: its own session, stdin and output on `/dev/null`, no other
/// file descriptors. It does the swap once fuxx has quit.
fn start_swap(cache: &Path, app: &Path, new_app: &Path) -> Result<(), String> {
    let script = cache.join("swap.sh");
    let text = update_release::swap_script(std::process::id(), app, new_app, "/usr/bin/open");
    std::fs::write(&script, text).map_err(|_| update_release::SWAP_FAILED)?;
    let script = script.to_str().ok_or(update_release::SWAP_FAILED)?;
    crate::daemon_client::spawn_detached(
        Path::new("/bin/sh"),
        &[script],
        Path::new("/dev/null"),
        &[],
    )
    .map_err(|_| update_release::SWAP_FAILED)?;
    Ok(())
}

/// Update and Restart: install `release` if it is still the one on offer (`should_install`;
/// else nothing). The button reads "Downloading update…" while, off the UI thread, the zip is
/// downloaded and checked (`download_and_check`) and the swap script started; then fuxx quits
/// and the script reopens it. When anything fails the installed fuxx is untouched: the release
/// is on offer again and a sheet says why (`install_failed`).
pub fn install(release: Release, cx: &mut App) {
    let state = cx
        .try_global::<Updates>()
        .map_or(UpdateState::Idle, |u| u.state.clone());
    if !should_install(&state, &release) {
        return;
    }
    let app = running_update_app();
    let zip = release
        .zip
        .clone()
        .and_then(|zip| Some((zip.url, zip.sha256?)));
    let home = crate::workspace::home_dir();
    let (Some(app), Some((url, sha256)), Some(home)) = (app, zip, home) else {
        install_failed(release, update_release::CANT_UPDATE_HERE.into(), cx);
        return;
    };
    eprintln!("[fuxx] updating to {}", release.version);
    cx.set_global(Updates {
        state: UpdateState::Downloading,
    });
    cx.refresh_windows();
    let version = release.version;
    let work = cx.background_executor().spawn(async move {
        let cache = cache_dir(&home);
        let new_app = download_and_check(version, &url, &sha256, &cache)?;
        start_swap(&cache, &app, &new_app)
    });
    cx.spawn(async move |cx| {
        let result = work.await;
        cx.update(|cx| match result {
            Ok(()) => {
                eprintln!("[fuxx] update to {version} checked: quitting for the swap");
                cx.quit();
            }
            Err(reason) => install_failed(release, reason, cx),
        });
    })
    .detach();
}

/// The update didn't install (`reason`): `release` is on offer again (the button back to Update
/// Available), and the error sheet goes on the main fuxx window. Open Release Page opens its
/// page (`release_link`); OK closes it.
fn install_failed(release: Release, reason: String, cx: &mut App) {
    eprintln!("[fuxx] the update couldn't be installed: {reason}");
    cx.set_global(Updates {
        state: UpdateState::Available(release.clone()),
    });
    cx.refresh_windows();
    let sheet = install_failed_sheet(reason);
    crate::with_help_window(cx, move |_, _, window, cx| {
        let answer = window.prompt(
            PromptLevel::Warning,
            &sheet.title,
            sheet.detail.as_deref(),
            sheet.buttons.as_slice(),
            cx,
        );
        cx.spawn(async move |_, cx| {
            if answer.await.ok() == Some(0) {
                cx.update(|cx| cx.open_url(release_link(&release)));
            }
        })
        .detach();
    });
}

/// Once the window has opened, delete the app the last update replaced (`<app>.previous`, kept
/// beside `app` until the new fuxx has shown it starts). Nothing when there is none; a failure is only logged.
pub fn clean_previous(app: &Path) {
    let previous = update_release::previous_app(app);
    let Ok(meta) = previous.symlink_metadata() else {
        return;
    };
    let removed = if meta.is_dir() {
        std::fs::remove_dir_all(&previous)
    } else {
        std::fs::remove_file(&previous)
    };
    match removed {
        Ok(()) => eprintln!("[fuxx] removed {}", previous.display()),
        Err(err) => eprintln!("[fuxx] couldn't remove {}: {err}", previous.display()),
    }
}

/// The automatic checks, for a bundled fuxx only (never `cargo run`, the ui-shots or tests):
/// the first after `FIRST_CHECK`, then every `EVERY`, each only while `check-updates` is on
/// (read again each time, so turning it off in Preferences stops them).
pub fn start_schedule(cx: &mut App) {
    if !crate::native_notify::is_bundled() {
        return;
    }
    let executor = cx.background_executor().clone();
    cx.spawn(async move |cx| {
        executor.timer(FIRST_CHECK).await;
        loop {
            cx.update(|cx| {
                let on = cx
                    .try_global::<crate::appearance::Appearance>()
                    .is_some_and(|a| a.check_updates);
                if on {
                    check(false, cx);
                }
            });
            executor.timer(EVERY).await;
        }
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::update_release::{CheckOutcome, Release, Version};

    fn release(v: Version, page: &str) -> Release {
        Release {
            version: v,
            page_url: page.into(),
            notes: String::new(),
            zip: None,
        }
    }

    #[test]
    fn reads_curls_answer_by_its_status() {
        assert_eq!(curl_answer(true, "{\"a\":1}\n200"), Ok("{\"a\":1}".into()));
        assert_eq!(
            curl_answer(false, "\n403"),
            Err("GitHub answered 403".into())
        );
        assert_eq!(
            curl_answer(false, "\n404"),
            Err("GitHub answered 404".into())
        );
        // No answer at all: curl couldn't connect (or timed out first).
        for out in ["\n000", "", "junk"] {
            assert_eq!(
                curl_answer(false, out),
                Err("offline or unreachable".into()),
                "{out:?}"
            );
        }
        // A 200 that curl gave up on part-way (the 20 s limit) isn't an answer.
        assert_eq!(
            curl_answer(false, "{\"a\"\n200"),
            Err("offline or unreachable".into())
        );
    }

    #[test]
    fn a_newer_release_becomes_available_unless_downloading() {
        let r = release(Version(9, 0, 0), "p");
        let newer = CheckOutcome::Newer(r.clone());
        assert_eq!(
            next_state(&UpdateState::Idle, &newer),
            UpdateState::Available(r.clone())
        );
        let older = release(Version(8, 0, 0), "p");
        assert_eq!(
            next_state(&UpdateState::Available(older), &newer),
            UpdateState::Available(r.clone())
        );
        assert_eq!(
            next_state(&UpdateState::Downloading, &newer),
            UpdateState::Downloading
        );
        // Up to date or failed: what was known stays.
        for outcome in [
            CheckOutcome::UpToDate(Version(0, 3, 0)),
            CheckOutcome::Failed("offline or unreachable".into()),
        ] {
            assert_eq!(next_state(&UpdateState::Idle, &outcome), UpdateState::Idle);
            assert_eq!(
                next_state(&UpdateState::Available(r.clone()), &outcome),
                UpdateState::Available(r.clone())
            );
        }
    }

    #[test]
    fn the_manual_checks_sheets() {
        // A newer release answers with the update offer (`show_offer`), not a sheet of its own.
        let page = "https://github.com/fitzfitz/fuxx-cli/releases/tag/v0.3.1";
        assert_eq!(
            manual_sheet(&CheckOutcome::Newer(release(Version(0, 3, 1), page))),
            None
        );
        assert_eq!(
            manual_sheet(&CheckOutcome::UpToDate(Version(0, 3, 0))),
            Some(Sheet {
                title: "fuxx 0.3.0 is the newest version.".into(),
                detail: None,
                buttons: vec!["OK"],
            })
        );
        assert_eq!(
            manual_sheet(&CheckOutcome::Failed("GitHub answered 403".into())),
            Some(Sheet {
                title: "Couldn't check for updates".into(),
                detail: Some("GitHub answered 403".into()),
                buttons: vec!["OK"],
            })
        );
    }

    #[test]
    fn opens_only_fuxxs_own_release_pages() {
        let good = "https://github.com/fitzfitz/fuxx-cli/releases/tag/v0.3.1";
        assert_eq!(release_link(&release(Version(0, 3, 1), good)), good);
        for bad in [
            "",
            "javascript:alert(1)",
            "https://evil.example/fitzfitz/fuxx-cli/releases",
            "http://github.com/fitzfitz/fuxx-cli/releases/tag/v0.3.1",
            "https://github.com/fitzfitz/fuxx-cli-evil/releases",
            "https://github.com/other/fuxx-cli/releases/tag/v0.3.1",
        ] {
            assert_eq!(
                release_link(&release(Version(0, 3, 1), bad)),
                RELEASES_PAGE,
                "{bad:?}"
            );
        }
    }

    #[test]
    fn updates_itself_only_in_a_writable_fuxx_app() {
        use std::path::{Path, PathBuf};
        let yes = |_: &Path| true;
        let no = |_: &Path| false;
        let exe = Path::new("/Users/me/Applications/fuxx.app/Contents/MacOS/fuxx");
        assert_eq!(
            self_update_target(exe, yes),
            Some(PathBuf::from("/Users/me/Applications/fuxx.app"))
        );
        // It is the app's folder (where `fuxx.app.previous` goes) that must be writable.
        let asked = std::cell::RefCell::new(Vec::new());
        self_update_target(exe, |p: &Path| {
            asked.borrow_mut().push(p.to_path_buf());
            true
        });
        assert_eq!(*asked.borrow(), [PathBuf::from("/Users/me/Applications")]);
        assert_eq!(self_update_target(exe, no), None);
        for exe in [
            "/Volumes/fuxx 0.3.0/fuxx.app/Contents/MacOS/fuxx",
            "/private/var/folders/x1/T/AppTranslocation/ABCD-1234/d/fuxx.app/Contents/MacOS/fuxx",
            "/Users/me/code/fuxx-cli/target/debug/fuxx",
            "/Applications/Other.app/Contents/MacOS/fuxx",
            "/Applications/fuxx.app/Contents/Resources/fuxx",
            "fuxx",
        ] {
            assert_eq!(self_update_target(Path::new(exe), yes), None, "{exe}");
        }
    }

    #[test]
    fn offer_and_install_share_the_utf8_app_condition() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;
        use std::path::{Path, PathBuf};
        let yes = |_: &Path| true;
        let ok = Path::new("/Users/me/Applications/fuxx.app/Contents/MacOS/fuxx");
        assert_eq!(
            update_app_for(ok, yes),
            Some(PathBuf::from("/Users/me/Applications/fuxx.app"))
        );
        let mut bytes = b"/Users/m\xFFe/fuxx.app/Contents/MacOS/fuxx".to_vec();
        bytes.shrink_to_fit();
        let bad = Path::new(OsStr::from_bytes(&bytes));
        assert!(self_update_target(bad, yes).is_some());
        assert_eq!(update_app_for(bad, yes), None);
    }

    #[test]
    fn installs_only_a_release_with_a_checked_zip() {
        let mut r = release(Version(0, 3, 1), "p");
        assert!(!installable(&r), "no zip");
        r.zip = Some(update_release::Asset {
            url: "https://x/fuxx-0.3.1.zip".into(),
            sha256: None,
        });
        assert!(!installable(&r), "no digest");
        r.zip = Some(update_release::Asset {
            url: "https://x/fuxx-0.3.1.zip".into(),
            sha256: Some("ab".repeat(32)),
        });
        assert!(installable(&r));
    }

    #[test]
    fn restarting_ends_the_sessions_the_background_service_doesnt_hold() {
        // The service holds them: the new fuxx reattaches.
        assert_eq!(ending_sessions(true, 4), 0);
        // Off (persistence off, or the service failed to start), still starting, or lost: the
        // sessions running here end.
        assert_eq!(ending_sessions(false, 4), 4);
        assert_eq!(ending_sessions(false, 0), 0);
    }

    #[test]
    fn installs_only_the_release_still_on_offer() {
        let r = release(Version(0, 3, 1), "p");
        assert!(should_install(&UpdateState::Available(r.clone()), &r));
        // A second offer of another version came first: this answer is stale.
        let other = release(Version(0, 3, 2), "p");
        assert!(!should_install(&UpdateState::Available(other), &r));
        // Already downloading (a second sheet answered), or nothing on offer.
        assert!(!should_install(&UpdateState::Downloading, &r));
        assert!(!should_install(&UpdateState::Idle, &r));
    }

    #[test]
    fn the_install_failed_sheet() {
        assert_eq!(
            install_failed_sheet("the zip holds 2 apps".into()),
            Sheet {
                title: "The update couldn't be installed".into(),
                detail: Some("the zip holds 2 apps".into()),
                buttons: vec!["Open Release Page", "OK"],
            }
        );
    }

    #[test]
    fn downloads_into_the_cache_with_curl() {
        use std::path::Path;
        assert_eq!(
            cache_dir(Path::new("/Users/me")),
            Path::new("/Users/me/Library/Caches/fuxx/update")
        );
        let zip = Path::new("/c/fuxx-0.3.1.zip");
        assert_eq!(
            download_args("https://x/fuxx-0.3.1.zip", zip),
            [
                "-fL",
                "--max-time",
                "600",
                "-o",
                "/c/fuxx-0.3.1.zip",
                "--url",
                "https://x/fuxx-0.3.1.zip",
            ]
            .map(std::ffi::OsString::from)
        );
    }

    #[test]
    fn reads_the_digest_shasum_prints() {
        let hex = "ab".repeat(32);
        assert_eq!(digest_from_shasum(&format!("{hex}  /c/My Zip.zip\n")), hex);
        assert_eq!(digest_from_shasum(""), "");
    }

    /// A fresh folder for one test.
    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("fuxx-updater-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn counts_the_apps_the_zip_held() {
        let dir = temp("count");
        assert_eq!(apps_in(&dir), Vec::<PathBuf>::new());
        std::fs::create_dir(dir.join("fuxx.app")).unwrap();
        std::fs::write(dir.join("readme.txt"), "x").unwrap();
        std::fs::create_dir(dir.join("__MACOSX")).unwrap();
        assert_eq!(apps_in(&dir), vec![dir.join("fuxx.app")]);
        std::fs::create_dir(dir.join("Other.app")).unwrap();
        assert_eq!(apps_in(&dir).len(), 2);
        // A missing folder (the zip didn't unpack) holds none.
        assert!(apps_in(&dir.join("nope")).is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn reads_the_one_apps_name() {
        let dir = temp("unpacked");
        let (u, app) = read_unpacked(&dir);
        assert_eq!((u.apps, u.app_name.as_str(), app), (0, "", None));
        // Not a real app: no Info.plist to read, no signature.
        std::fs::create_dir(dir.join("Other.app")).unwrap();
        let (u, app) = read_unpacked(&dir);
        assert_eq!(u.apps, 1);
        assert_eq!(u.app_name, "Other.app");
        assert_eq!(u.bundle_id, "");
        assert!(!u.signature_ok);
        assert_eq!(app, Some(dir.join("Other.app")));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn launch_removes_the_previous_app_only() {
        let dir = temp("previous");
        let app = dir.join("x.app");
        std::fs::create_dir_all(app.join("Contents")).unwrap();
        std::fs::create_dir_all(dir.join("x.app.previous").join("Contents")).unwrap();
        clean_previous(&app);
        assert!(app.join("Contents").is_dir());
        assert!(!dir.join("x.app.previous").exists());
        // Nothing to remove: nothing happens.
        clean_previous(&app);
        assert!(app.join("Contents").is_dir());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_button_reads_by_the_state() {
        let r = release(Version(0, 3, 1), "p");
        assert_eq!(button(&UpdateState::Idle), None);
        assert_eq!(
            button(&UpdateState::Available(r)),
            Some(("Update Available", true))
        );
        assert_eq!(
            button(&UpdateState::Downloading),
            Some(("Downloading update\u{2026}", false))
        );
    }

    #[test]
    fn asks_githubs_api_for_the_latest_release() {
        assert_eq!(
            API_URL,
            "https://api.github.com/repos/fitzfitz/fuxx-cli/releases/latest"
        );
        assert_eq!(
            RELEASES_PAGE,
            "https://github.com/fitzfitz/fuxx-cli/releases/latest"
        );
        assert_eq!(FIRST_CHECK, std::time::Duration::from_secs(10));
        assert_eq!(EVERY, std::time::Duration::from_secs(24 * 60 * 60));
        assert_eq!(
            curl_args(),
            [
                "-fsSL",
                "--max-time",
                "20",
                "-H",
                "Accept: application/vnd.github+json",
                "-w",
                "\n%{http_code}",
                API_URL,
            ]
        );
    }
}
