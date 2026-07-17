use notify_rust::Notification;

/// Fire a desktop notification (macOS banner). Best-effort: a failure to display must never
/// crash the wrapper, so the `show()` Result is intentionally discarded. On macOS, only
/// `summary` and `body` are meaningful (`icon`/`appname`/`hint` are no-ops there).
pub fn fire(summary: &str, body: &str) {
    let _ = Notification::new().summary(summary).body(body).show();
}
