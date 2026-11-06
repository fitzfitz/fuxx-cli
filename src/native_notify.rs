//! Native macOS banners (M11c): Apple's UserNotifications framework, named "fuxx" and shown from
//! the app bundle. Only usable when the running binary is inside `fuxx.app` — unbundled, the
//! framework throws, so nothing here touches it unless `is_bundled()`.

use std::sync::mpsc;
use std::time::Duration;

use block2::RcBlock;
use objc2::runtime::Bool;
use objc2_foundation::{NSBundle, NSError, NSString};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNAuthorizationStatus, UNMutableNotificationContent,
    UNNotificationRequest, UNNotificationSettings, UNUserNotificationCenter,
};

/// The app's identity, as `packaging/macos/Info.plist` has it.
pub const BUNDLE_ID: &str = "io.github.fitzfitz.fuxx";

/// How long a permission read or a post may wait for the system before the caller falls back.
const WAIT: Duration = Duration::from_secs(2);

/// Whether a main-bundle identifier is fuxx's own.
pub fn is_bundled_id(id: Option<&str>) -> bool {
    id == Some(BUNDLE_ID)
}

/// Whether this process runs from the `fuxx.app` bundle (not a plain `cargo run` binary, and not
/// some other app's process).
pub fn is_bundled() -> bool {
    let id = NSBundle::mainBundle()
        .bundleIdentifier()
        .map(|s| s.to_string());
    is_bundled_id(id.as_deref())
}

/// What the user has told macOS about fuxx's banners.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Permission {
    Allowed,
    NotDetermined,
    Denied,
}

/// Why a banner was not posted; the caller falls back to `osascript`.
#[derive(Debug)]
pub enum PostError {
    /// Not running from the app bundle.
    Unbundled,
    /// The system rejected the request.
    #[allow(dead_code)] // the reason is for `Debug` output; `fire` just falls back
    Rejected(String),
    /// No answer within the wait.
    TimedOut,
}

/// The banner permission, waiting up to 2 s for the system. `None` when unbundled or when it did
/// not answer in time. Blocks: call off the UI thread.
pub fn permission() -> Option<Permission> {
    if !is_bundled() {
        return None;
    }
    let (tx, rx) = mpsc::channel();
    let center = UNUserNotificationCenter::currentNotificationCenter();
    let done = RcBlock::new(move |settings: std::ptr::NonNull<UNNotificationSettings>| {
        // SAFETY: the system passes a live settings object for the duration of the call.
        let status = unsafe { settings.as_ref() }.authorizationStatus();
        let _ = tx.send(status);
    });
    center.getNotificationSettingsWithCompletionHandler(&done);
    let status = rx.recv_timeout(WAIT).ok()?;
    Some(match status {
        UNAuthorizationStatus::Denied => Permission::Denied,
        UNAuthorizationStatus::NotDetermined => Permission::NotDetermined,
        // Authorized, Provisional and Ephemeral all let a banner through.
        _ => Permission::Allowed,
    })
}

/// Ask macOS for permission to show alerts (no sound). Used by Task 3's first-run flow; returns
/// whether it was granted, `None` on timeout or unbundled. Blocks while the system prompt is up
/// only as long as the 2 s wait.
#[allow(dead_code)] // used by the permission prompt (M11c Task 3)
pub fn request_permission() -> Option<bool> {
    if !is_bundled() {
        return None;
    }
    let (tx, rx) = mpsc::channel();
    let center = UNUserNotificationCenter::currentNotificationCenter();
    let done = RcBlock::new(move |granted: Bool, _error: *mut NSError| {
        let _ = tx.send(granted.as_bool());
    });
    center.requestAuthorizationWithOptions_completionHandler(UNAuthorizationOptions::Alert, &done);
    rx.recv_timeout(WAIT).ok()
}

/// Show a banner: `summary` as the title, `body` under it, no subtitle, no sound. `terminal` (the
/// `sN` it is about) rides in the request identifier. Blocks up to 2 s for the system to take it.
pub fn post(summary: &str, body: &str, terminal: Option<u64>) -> Result<(), PostError> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    if !is_bundled() {
        return Err(PostError::Unbundled);
    }
    let center = UNUserNotificationCenter::currentNotificationCenter();
    let content = UNMutableNotificationContent::new();
    content.setTitle(&NSString::from_str(summary));
    content.setBody(&NSString::from_str(body));
    let id = request_id(terminal, COUNTER.fetch_add(1, Ordering::Relaxed));
    let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
        &NSString::from_str(&id),
        &content,
        None,
    );
    let (tx, rx) = mpsc::channel();
    let done = RcBlock::new(move |error: *mut NSError| {
        // SAFETY: when non-null, the system passes a live NSError for the duration of the call.
        let result = match unsafe { error.as_ref() } {
            None => Ok(()),
            Some(e) => Err(e.localizedDescription().to_string()),
        };
        let _ = tx.send(result);
    });
    center.addNotificationRequest_withCompletionHandler(&request, Some(&done));
    match rx.recv_timeout(WAIT) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(why)) => Err(PostError::Rejected(why)),
        Err(_) => Err(PostError::TimedOut),
    }
}

/// A banner's request identifier: `fuxx-s{N}-{n}` for terminal `sN` (so a click can say which
/// one), else `fuxx-{n}`; `n` makes each one unique.
pub fn request_id(terminal: Option<u64>, n: u64) -> String {
    match terminal {
        Some(t) => format!("fuxx-s{t}-{n}"),
        None => format!("fuxx-{n}"),
    }
}

/// The terminal a request identifier is about, if it names one.
#[allow(dead_code)] // used by the click handler (M11c Task 2)
pub fn terminal_of(request_id: &str) -> Option<u64> {
    let rest = request_id.strip_prefix("fuxx-s")?;
    let (terminal, _) = rest.split_once('-')?;
    terminal.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_only_for_our_own_identifier() {
        assert!(is_bundled_id(Some(BUNDLE_ID)));
        assert!(!is_bundled_id(None));
        assert!(!is_bundled_id(Some("com.apple.ScriptEditor2")));
        assert!(!is_bundled_id(Some("")));
    }

    #[test]
    fn request_ids_carry_the_terminal() {
        assert_eq!(request_id(Some(3), 7), "fuxx-s3-7");
        assert_eq!(request_id(None, 7), "fuxx-7");
        assert_eq!(terminal_of("fuxx-s3-7"), Some(3));
        assert_eq!(terminal_of("fuxx-7"), None);
        assert_eq!(terminal_of("fuxx-sx-1"), None);
        assert_eq!(terminal_of("other"), None);
        assert_eq!(terminal_of(""), None);
    }
}
