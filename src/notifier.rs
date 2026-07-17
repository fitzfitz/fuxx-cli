use std::process::Command;

/// Fire a native macOS desktop notification via `osascript`.
///
/// Best-effort: the command's result is intentionally discarded so a notification failure can
/// never crash or interrupt the wrapper.
///
/// Why `osascript` and not a notifications crate: on modern macOS (tested on 26.x), the
/// `NSUserNotification` API that `notify-rust`/`mac-notification-sys` rely on is deprecated and
/// does not display notifications for an unbundled CLI binary — `show()` succeeds but nothing
/// appears. `osascript`'s `display notification` goes through an entitled system component and
/// works without shipping fuxx as a signed `.app` bundle. macOS-only, matching v1's scope.
pub fn fire(summary: &str, body: &str) {
    // Escape for embedding in an AppleScript double-quoted string literal. Content is currently
    // fixed (no external input), but escape defensively so this stays safe if that changes.
    let escape = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!(
        "display notification \"{}\" with title \"{}\"",
        escape(body),
        escape(summary),
    );
    let _ = Command::new("osascript").arg("-e").arg(script).status();
}
