use std::process::Command;
use std::time::Duration;

use crate::native_notify::{self, Permission, PostError, Terminal};
use crate::status::{Status, age_label};

/// Which way a banner goes out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    Native,
    Script,
}

/// Native only from the app bundle with banners allowed; anything else (unbundled, not answered,
/// denied, no reply in time) is the `osascript` banner.
pub fn route(bundled: bool, permission: Option<Permission>) -> Route {
    if bundled && permission == Some(Permission::Allowed) {
        Route::Native
    } else {
        Route::Script
    }
}

/// Show a desktop notification: a native banner when fuxx runs from its app bundle and macOS
/// allows it, else the `osascript` one. `about` is the terminal it is about (with the run that
/// numbered it), if any.
///
/// Best-effort: failures are swallowed so a notification can never crash or interrupt the app.
/// It blocks (up to ~2 s reading the permission plus ~2 s for the post, so ~4 s, then ~100 ms
/// for `osascript`), so call it off the UI thread. A native post that fails is logged and falls
/// back to `osascript`: never both, never neither.
///
/// Why `osascript` stays as the fallback: on modern macOS (tested on 26.x), the
/// `NSUserNotification` API that `notify-rust`/`mac-notification-sys` rely on is deprecated and
/// does not display notifications for an unbundled binary — `show()` succeeds but nothing
/// appears. `osascript`'s `display notification` goes through an entitled system component and
/// works without shipping fuxx as a signed `.app` bundle.
pub fn fire(summary: &str, subtitle: &str, body: &str, about: Option<Terminal>) {
    let bundled = native_notify::is_bundled();
    let permission = if bundled {
        native_notify::permission()
    } else {
        None
    };
    if route(bundled, permission) == Route::Native {
        match native_notify::post(summary, subtitle, body, about) {
            Ok(()) => return,
            Err(PostError::Unbundled) => {}
            Err(why) => eprintln!(
                "[fuxx] notifications: the native banner failed ({why:?}); using osascript"
            ),
        }
    }
    fire_script(summary, subtitle, body);
}

/// The `osascript` banner (with its subtitle, when there is one).
fn fire_script(summary: &str, subtitle: &str, body: &str) {
    let mut script = format!(
        "display notification \"{}\" with title \"{}\"",
        applescript_escape(body),
        applescript_escape(summary),
    );
    if !subtitle.is_empty() {
        script.push_str(&format!(" subtitle \"{}\"", applescript_escape(subtitle)));
    }
    let _ = Command::new("osascript").arg("-e").arg(script).status();
}

/// Make text safe inside an AppleScript double-quoted string. Session titles are set by the
/// programs running in them (OSC 0/2), so this is untrusted input: escape `\` and `"`, and drop
/// control characters.
pub fn applescript_escape(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

/// Focus-aware rule: notify when a session *changes* to Done or Waiting, unless you are looking
/// at it (it's the active session AND the fuxx window is focused).
pub fn should_notify(
    old: Status,
    new: Status,
    is_active_session: bool,
    window_active: bool,
) -> bool {
    let noteworthy = matches!(new, Status::Done | Status::Waiting);
    let looking_at_it = is_active_session && window_active;
    noteworthy && old != new && !looking_at_it
}

/// Notification body for a status, if that status is worth one.
pub fn message(status: Status) -> Option<&'static str> {
    match status {
        Status::Done => Some("Finished"),
        Status::Waiting => Some("Needs your input"),
        Status::Idle | Status::Working => None,
    }
}

/// A notification's text under its title: what a waiting session asks (`question`), or, for a
/// finished one, what it was doing (`doing`: its terminal's title, part 5) and how long it
/// worked (`worked`: the Working stretch that just ended). Without those, the plain message.
pub fn body(
    status: Status,
    question: Option<&str>,
    worked: Option<Duration>,
    doing: Option<&str>,
) -> Option<String> {
    let doing = doing.map(str::trim).filter(|d| !d.is_empty());
    let shown = match (status, question, worked, doing) {
        (Status::Waiting, Some(question), _, _) => question.to_string(),
        (Status::Done, _, Some(worked), Some(doing)) => {
            format!("{doing} \u{b7} worked {}", age_label(worked))
        }
        (Status::Done, _, Some(worked), None) => format!("Worked for {}", age_label(worked)),
        _ => message(status)?.to_string(),
    };
    Some(shown)
}

/// A banner's title (M11v part 5): the workspace's name, as the sidebar and the 🔔 list say it,
/// and what happened: `Chores list — Finished`.
pub fn banner_title(workspace: &str, message: &str) -> String {
    format!("{workspace} \u{2014} {message}")
}

/// A banner's subtitle: `#3 · proj · claude`: its ⌘ number, its folder group (left out when it is
/// the workspace's own name, or none) and the agent running there (if one is).
pub fn banner_subtitle(
    number: usize,
    group: Option<&str>,
    workspace: &str,
    agent: Option<&str>,
) -> String {
    let mut parts = vec![format!("#{number}")];
    parts.extend(group.filter(|g| *g != workspace).map(str::to_string));
    parts.extend(agent.filter(|a| !a.is_empty()).map(str::to_string));
    parts.join(" \u{b7} ")
}

/// Whether a Finished banner goes out: not for a run shorter than `QUIET_UNDER` (an agent's
/// pause, not its work); the 🔔 list keeps it anyway. Unknown length: it does.
pub fn worth_a_banner(status: Status, worked: Option<Duration>) -> bool {
    status != Status::Done || worked.is_none_or(|w| w >= QUIET_UNDER)
}

/// Runs shorter than this finish without a banner (part 5, AI's call: 10 s).
pub const QUIET_UNDER: Duration = Duration::from_secs(10);

#[cfg(test)]
mod tests {
    use super::*;

    /// Part 5: the banner names the workspace as fuxx does, says where and who in its subtitle,
    /// and what the agent was doing in its body.
    #[test]
    fn banners_say_which_workspace_where_and_what() {
        assert_eq!(
            banner_title("Chores list", "Finished"),
            "Chores list \u{2014} Finished"
        );
        assert_eq!(
            banner_subtitle(3, Some("proj"), "Chores list", Some("claude")),
            "#3 \u{b7} proj \u{b7} claude"
        );
        assert_eq!(
            banner_subtitle(3, Some("proj"), "proj", None),
            "#3",
            "the group is the name"
        );
        assert_eq!(
            banner_subtitle(4, None, "api", Some("codex")),
            "#4 \u{b7} codex"
        );
        assert_eq!(
            body(
                Status::Done,
                None,
                Some(Duration::from_secs(34)),
                Some("\u{2733} Breadcrumb tiket")
            )
            .as_deref(),
            Some("\u{2733} Breadcrumb tiket \u{b7} worked 34s")
        );
        assert_eq!(
            body(Status::Done, None, Some(Duration::from_secs(34)), None).as_deref(),
            Some("Worked for 34s"),
            "nothing to say what it was doing"
        );
        assert_eq!(
            body(
                Status::Waiting,
                Some("Proceed? (y/n)"),
                None,
                Some("\u{2733} x")
            )
            .as_deref(),
            Some("Proceed? (y/n)"),
            "a question wins"
        );
    }

    #[test]
    fn short_runs_finish_without_a_banner() {
        assert!(!worth_a_banner(Status::Done, Some(Duration::from_secs(3))));
        assert!(worth_a_banner(Status::Done, Some(Duration::from_secs(10))));
        assert!(worth_a_banner(Status::Done, None));
        assert!(
            worth_a_banner(Status::Waiting, Some(Duration::from_secs(1))),
            "a question always"
        );
    }

    #[test]
    fn notifies_for_background_sessions_that_finish_or_wait() {
        assert!(should_notify(Status::Working, Status::Done, false, true));
        assert!(should_notify(Status::Working, Status::Waiting, false, true));
    }

    #[test]
    fn never_nags_about_the_session_you_are_looking_at() {
        assert!(!should_notify(Status::Working, Status::Done, true, true));
    }

    #[test]
    fn active_session_notifies_when_the_window_is_in_the_background() {
        assert!(should_notify(Status::Working, Status::Done, true, false));
    }

    #[test]
    fn ignores_working_idle_and_repeats() {
        assert!(!should_notify(Status::Idle, Status::Working, false, false));
        assert!(!should_notify(Status::Done, Status::Done, false, false));
    }

    #[test]
    fn escape_neutralises_quotes_backslashes_and_control_chars() {
        assert_eq!(applescript_escape(r#"a"b\c"#), r#"a\"b\\c"#);
        assert_eq!(applescript_escape("x\ny\u{7}"), "xy");
    }

    #[test]
    fn notification_bodies() {
        let q = "Do you want to make this edit to auth.rs?";
        assert_eq!(
            body(Status::Waiting, Some(q), None, None).as_deref(),
            Some(q)
        );
        assert_eq!(
            body(Status::Waiting, None, Some(Duration::from_secs(9)), None).as_deref(),
            Some("Needs your input")
        );
        assert_eq!(
            body(
                Status::Done,
                None,
                Some(Duration::from_secs(4 * 60 + 12)),
                None
            )
            .as_deref(),
            Some("Worked for 4m")
        );
        assert_eq!(
            body(Status::Done, Some(q), Some(Duration::from_secs(12)), None).as_deref(),
            Some("Worked for 12s")
        );
        assert_eq!(
            body(Status::Done, None, None, None).as_deref(),
            Some("Finished")
        );
        assert_eq!(body(Status::Working, None, None, None), None);
        assert_eq!(body(Status::Idle, Some(q), None, None), None);
    }

    #[test]
    fn native_only_when_bundled_and_allowed() {
        use crate::native_notify::Permission::{Allowed, Denied, NotDetermined};
        assert_eq!(route(true, Some(Allowed)), Route::Native);
        for p in [Some(NotDetermined), Some(Denied), None] {
            assert_eq!(route(true, p), Route::Script);
        }
        for p in [Some(Allowed), Some(NotDetermined), Some(Denied), None] {
            assert_eq!(route(false, p), Route::Script);
        }
    }

    #[test]
    fn messages() {
        assert_eq!(message(Status::Done), Some("Finished"));
        assert_eq!(message(Status::Waiting), Some("Needs your input"));
        assert_eq!(message(Status::Working), None);
    }
}
