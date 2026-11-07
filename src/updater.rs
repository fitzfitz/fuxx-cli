// The self-updater's effects: asking GitHub for the latest release (at launch, daily, and from
// fuxx → Check for Updates…), and what the app knows about it (`Updates`).
//
// The fetch blocks (curl), so it runs on the background executor; the outcome is handled on the
// main thread. What to do with an answer is decided by the pure functions here and in
// `update_release`; `fetch_latest` is the only part that touches the network.

use std::time::Duration;

use gpui::{App, Global, PromptLevel};

use crate::update_release::{self, CheckOutcome, Release};

/// GitHub's latest release of fuxx, as JSON. Public repo: no token.
pub const API_URL: &str = "https://api.github.com/repos/fitzfitz/fuxx-cli/releases/latest";
/// The latest release's page, for when the JSON gives none.
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

/// A sheet: its title, its text, its buttons in order, and the button that opens a page.
#[derive(Clone, Debug, PartialEq)]
struct Sheet {
    title: String,
    detail: Option<String>,
    buttons: Vec<&'static str>,
    open: Option<(usize, String)>,
}

/// The manual check's sheet for what it found. (A newer release's is a stand-in until the
/// Update Available offer, M11e Task 4.)
fn manual_sheet(outcome: &CheckOutcome) -> Sheet {
    match outcome {
        CheckOutcome::Newer(release) => {
            let page = if release.page_url.is_empty() {
                RELEASES_PAGE.to_string()
            } else {
                release.page_url.clone()
            };
            Sheet {
                title: format!("fuxx {} is available", release.version),
                detail: None,
                buttons: vec!["Download from GitHub\u{2026}", "Later"],
                open: Some((0, page)),
            }
        }
        CheckOutcome::UpToDate(v) => Sheet {
            title: update_release::up_to_date_title(*v),
            detail: None,
            buttons: vec!["OK"],
            open: None,
        },
        CheckOutcome::Failed(reason) => Sheet {
            title: update_release::CHECK_FAILED.to_string(),
            detail: Some(reason.clone()),
            buttons: vec!["OK"],
            open: None,
        },
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
        show_sheet(manual_sheet(&outcome), cx);
    }
}

/// `sheet` on the main fuxx window; its page button opens the page.
fn show_sheet(sheet: Sheet, cx: &mut App) {
    crate::with_help_window(cx, move |_, _, window, cx| {
        let answer = window.prompt(
            PromptLevel::Info,
            &sheet.title,
            sheet.detail.as_deref(),
            sheet.buttons.as_slice(),
            cx,
        );
        let open = sheet.open;
        cx.spawn(async move |_, cx| {
            if let Some((button, url)) = open
                && answer.await.ok() == Some(button)
            {
                cx.update(|cx| cx.open_url(&url));
            }
        })
        .detach();
    });
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
        let page = "https://github.com/fitzfitz/fuxx-cli/releases/tag/v0.3.1";
        let newer = manual_sheet(&CheckOutcome::Newer(release(Version(0, 3, 1), page)));
        assert_eq!(
            newer,
            Sheet {
                title: "fuxx 0.3.1 is available".into(),
                detail: None,
                buttons: vec!["Download from GitHub\u{2026}", "Later"],
                open: Some((0, page.into())),
            }
        );
        // No page in the JSON: the latest release's page.
        let no_page = manual_sheet(&CheckOutcome::Newer(release(Version(0, 3, 1), "")));
        assert_eq!(
            no_page.open,
            Some((
                0,
                "https://github.com/fitzfitz/fuxx-cli/releases/latest".into()
            ))
        );
        assert_eq!(
            manual_sheet(&CheckOutcome::UpToDate(Version(0, 3, 0))),
            Sheet {
                title: "fuxx 0.3.0 is the newest version.".into(),
                detail: None,
                buttons: vec!["OK"],
                open: None,
            }
        );
        assert_eq!(
            manual_sheet(&CheckOutcome::Failed("GitHub answered 403".into())),
            Sheet {
                title: "Couldn't check for updates".into(),
                detail: Some("GitHub answered 403".into()),
                buttons: vec!["OK"],
                open: None,
            }
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
