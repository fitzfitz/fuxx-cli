use std::process::Command;
use std::time::Duration;

use crate::status::{Status, age_label};

/// Fire a native macOS desktop notification via `osascript`.
///
/// Best-effort: the command's result is intentionally discarded so a notification failure can
/// never crash or interrupt the app. It blocks for ~100 ms, so call it off the UI thread.
///
/// Why `osascript` and not a notifications crate: on modern macOS (tested on 26.x), the
/// `NSUserNotification` API that `notify-rust`/`mac-notification-sys` rely on is deprecated and
/// does not display notifications for an unbundled binary — `show()` succeeds but nothing
/// appears. `osascript`'s `display notification` goes through an entitled system component and
/// works without shipping fuxx as a signed `.app` bundle.
pub fn fire(summary: &str, body: &str) {
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
    fn messages() {
        assert_eq!(message(Status::Done), Some("Finished"));
        assert_eq!(message(Status::Waiting), Some("Needs your input"));
        assert_eq!(message(Status::Working), None);
    }
}
