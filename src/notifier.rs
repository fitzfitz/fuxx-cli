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
pub fn fire(summary: &str, body: &str, about: Option<Terminal>) {
    let bundled = native_notify::is_bundled();
    let permission = if bundled {
        native_notify::permission()
    } else {
        None
    };
    if route(bundled, permission) == Route::Native {
        match native_notify::post(summary, body, about) {
            Ok(()) => return,
            Err(PostError::Unbundled) => {}
            Err(why) => eprintln!(
                "[fuxx] notifications: the native banner failed ({why:?}); using osascript"
            ),
        }
    }
    fire_script(summary, body);
}

/// The `osascript` banner.
fn fire_script(summary: &str, body: &str) {
    let script = format!(
        "display notification \"{}\" with title \"{}\"",
        applescript_escape(body),
        applescript_escape(summary),
    );
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

/// A notification's text under its title: what a waiting session asks (`question`), or how long
/// a finished one worked (`worked`: the Working stretch that just ended). Without those, the
/// plain message.
pub fn body(status: Status, question: Option<&str>, worked: Option<Duration>) -> Option<String> {
    let shown = match (status, question, worked) {
        (Status::Waiting, Some(question), _) => question.to_string(),
        (Status::Done, _, Some(worked)) => format!("Worked for {}", age_label(worked)),
        _ => message(status)?.to_string(),
    };
    Some(shown)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(body(Status::Waiting, Some(q), None).as_deref(), Some(q));
        assert_eq!(
            body(Status::Waiting, None, Some(Duration::from_secs(9))).as_deref(),
            Some("Needs your input")
        );
        assert_eq!(
            body(Status::Done, None, Some(Duration::from_secs(4 * 60 + 12))).as_deref(),
            Some("Worked for 4m")
        );
        assert_eq!(
            body(Status::Done, Some(q), Some(Duration::from_secs(12))).as_deref(),
            Some("Worked for 12s")
        );
        assert_eq!(body(Status::Done, None, None).as_deref(), Some("Finished"));
        assert_eq!(body(Status::Working, None, None), None);
        assert_eq!(body(Status::Idle, Some(q), None), None);
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
