// The self-updater's effects: asking GitHub for the latest release (at launch, daily, and from
// fuxx → Check for Updates…), and what the app knows about it (`Updates`).
//
// The fetch blocks (curl), so it runs on the background executor; the outcome is handled on the
// main thread. What to do with an answer is decided by the pure functions here and in
// `update_release`; `fetch_latest` is the only part that touches the network.

use std::ffi::{CString, c_char, c_int};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use gpui::{App, Global, PromptLevel, Window};

use crate::update_release::{self, CheckOutcome, Release};
use crate::workspace::Workspace;

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
    /// Updating to it (Task 5 sets this).
    #[allow(dead_code)] // set by the download from M11e Task 5
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

/// Whether this fuxx can update itself to `release`: run from a `fuxx.app` it may swap
/// (`self_update_target`), and the release has a checked zip.
fn can_self_update(release: &Release) -> bool {
    crate::native_notify::is_bundled()
        && installable(release)
        && std::env::current_exe()
            .ok()
            .and_then(|exe| self_update_target(&exe, writable))
            .is_some()
}

/// How many sessions restarting ends: every terminal session, unless they run in the
/// background service (the new fuxx reattaches to those).
fn ending_sessions(workspace: &Workspace) -> usize {
    if workspace.persistent {
        0
    } else {
        workspace.sessions.items().len()
    }
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
        ending_sessions(workspace),
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
                install(&release, cx);
            } else {
                cx.open_url(release_link(&release));
            }
        });
    })
    .detach();
}

/// Download `release`, check it, swap `fuxx.app` and reopen. For now (until M11e Task 5) it
/// opens the release's page.
pub fn install(release: &Release, cx: &mut App) {
    cx.open_url(release_link(release));
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
