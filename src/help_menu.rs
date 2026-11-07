// The Help menu (M11d): its items, its marking as the app's Help menu (so macOS adds its search
// box), which window a Help item acts on, and what Notifications Status… says.

use gpui::{Menu, MenuItem};
use objc2::MainThreadMarker;
use objc2_app_kit::NSApplication;
use objc2_foundation::NSString;

use crate::native_notify::Permission;
use crate::{
    ReportIssue, ShowCommands, ShowLogs, ShowNotificationStatus, ShowShortcuts, ShowWelcomeTips,
};

/// The Help menu: the two panels (Keyboard Shortcuts shows ⌘/, its binding), the notification
/// status and the tips card, then reporting an issue and the logs.
pub fn menu() -> Menu {
    Menu::new("Help").items([
        MenuItem::action("Keyboard Shortcuts", ShowShortcuts),
        MenuItem::action("fuxx Command Reference", ShowCommands),
        MenuItem::separator(),
        MenuItem::action("Notifications Status\u{2026}", ShowNotificationStatus),
        MenuItem::action("Show Welcome Tips", ShowWelcomeTips),
        MenuItem::separator(),
        MenuItem::action("Report an Issue\u{2026}", ReportIssue),
        MenuItem::action("Show Logs", ShowLogs),
    ])
}

/// Mark the menu bar's Help menu as the app's (`NSApplication.helpMenu`), so macOS puts its
/// search box at the top. GPUI marks only the Window menu, and rebuilds the whole bar on every
/// `set_menus`, so call this right after each one (GPUI sets the bar at once). Nothing off the
/// main thread or with no Help menu in the bar.
pub fn mark_help_menu() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    let help = app
        .mainMenu()
        .and_then(|bar| bar.itemWithTitle(&NSString::from_str("Help")))
        .and_then(|item| item.submenu());
    if let Some(help) = help {
        app.setHelpMenu(Some(&help));
    }
}

/// Which window a Help item that needs one goes to, from each open window's facts: whether it
/// is a main fuxx window (its root is a `Workspace`) and whether it is the key window. The key
/// window if it is a main one; else the first main one (with a popped-out browser window or
/// Preferences in front); else none.
pub fn help_target(windows: &[(bool, bool)]) -> Option<usize> {
    windows
        .iter()
        .position(|&(main, key)| main && key)
        .or_else(|| windows.iter().position(|&(main, _)| main))
}

/// fuxx's own page in System Settings → Notifications (macOS may open only the list).
pub const NOTIFICATION_SETTINGS_URL: &str = "x-apple.systempreferences:com.apple.Notifications-Settings.extension?id=io.github.fitzfitz.fuxx";

/// What Notifications Status… says: the sheet's title, its text, its buttons in order, and
/// which of them is Open System Settings (none when there is no such button).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StatusMessage {
    pub title: &'static str,
    pub detail: &'static str,
    pub buttons: &'static [&'static str],
    pub settings_button: Option<usize>,
}

/// Open System Settings first, the button for when something is off.
const SETTINGS_FIRST: &[&str] = &["Open System Settings", "OK"];

/// Notifications Status…'s sheet for what macOS said (note 56's table): `bundled` (running as
/// fuxx.app) and the banner permission (`None`: no answer in time). Unbundled is always "Not
/// running as fuxx.app", whatever the permission.
pub fn status_message(bundled: bool, permission: Option<Permission>) -> StatusMessage {
    if !bundled {
        return StatusMessage {
            title: "Not running as fuxx.app",
            detail: "This fuxx was started outside its app, so macOS can't give it notifications \
                     of its own. It sends Script Editor banners instead.",
            buttons: &["OK"],
            settings_button: None,
        };
    }
    match permission {
        Some(Permission::Allowed) => StatusMessage {
            title: "fuxx sends its own notifications",
            detail: "Banners come from fuxx, and clicking one shows the terminal that raised it.",
            buttons: &["OK", "Open System Settings"],
            settings_button: Some(1),
        },
        Some(Permission::Denied) => StatusMessage {
            title: "fuxx's own notifications are off",
            detail: "macOS doesn't allow fuxx's banners, so fuxx sends Script Editor banners \
                     instead, without click-through. Turn fuxx on in System Settings \u{2192} \
                     Notifications.",
            buttons: SETTINGS_FIRST,
            settings_button: Some(0),
        },
        Some(Permission::NotDetermined) => StatusMessage {
            title: "Notifications not answered yet",
            detail: "Until you allow fuxx, it sends Script Editor banners instead.",
            buttons: SETTINGS_FIRST,
            settings_button: Some(0),
        },
        None => StatusMessage {
            title: "Couldn't read the notification setting",
            detail: "macOS didn't answer in time. Try again, or look in System Settings \u{2192} \
                     Notifications \u{2192} fuxx.",
            buttons: SETTINGS_FIRST,
            settings_button: Some(0),
        },
    }
}

/// The new-issue page for fuxx's public repo; the issue text follows `body=`.
const ISSUE_URL_BASE: &str = "https://github.com/fitzfitz/fuxx-cli/issues/new?body=";

/// The issue text filled in in advance. `os` is `sw_vers -productVersion`, or `None` if it failed.
pub fn issue_body(version: &str, os: Option<&str>) -> String {
    format!(
        "**What happened**\n\n\n**What you expected**\n\n\n---\nfuxx {version} \u{b7} macOS {}",
        os.unwrap_or("unknown")
    )
}

/// `issue_body` after `ISSUE_URL_BASE`, everything but `A-Z a-z 0-9 - . _ ~` as UTF-8 `%XX`.
pub fn issue_url(version: &str, os: Option<&str>) -> String {
    let mut url = String::from(ISSUE_URL_BASE);
    for byte in issue_body(version, os).bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            url.push(byte as char);
        } else {
            url.push_str(&format!("%{byte:02X}"));
        }
    }
    url
}

#[cfg(test)]
mod tests {
    use super::{
        ISSUE_URL_BASE, NOTIFICATION_SETTINGS_URL, help_target, issue_body, issue_url,
        status_message,
    };
    use crate::native_notify::Permission;

    #[test]
    fn help_goes_to_the_key_window_if_it_is_a_main_one() {
        // Preferences (or a popped-out page) is key: the main window behind it.
        assert_eq!(help_target(&[(false, true), (true, false)]), Some(1));
        // Two main windows: the key one.
        assert_eq!(help_target(&[(true, false), (true, true)]), Some(1));
        // No main window: nowhere.
        assert_eq!(help_target(&[(false, true)]), None);
        assert_eq!(help_target(&[]), None);
    }

    const SETTINGS_FIRST: &[&str] = &["Open System Settings", "OK"];

    #[test]
    fn allowed_says_fuxx_sends_its_own() {
        let m = status_message(true, Some(Permission::Allowed));
        assert_eq!(m.title, "fuxx sends its own notifications");
        assert_eq!(
            m.detail,
            "Banners come from fuxx, and clicking one shows the terminal that raised it."
        );
        assert_eq!(m.buttons, ["OK", "Open System Settings"]);
        assert_eq!(m.settings_button, Some(1));
    }

    #[test]
    fn denied_says_they_are_off() {
        let m = status_message(true, Some(Permission::Denied));
        assert_eq!(m.title, "fuxx's own notifications are off");
        assert_eq!(
            m.detail,
            "macOS doesn't allow fuxx's banners, so fuxx sends Script Editor banners instead, \
             without click-through. Turn fuxx on in System Settings \u{2192} Notifications."
        );
        assert_eq!(m.buttons, SETTINGS_FIRST);
        assert_eq!(m.settings_button, Some(0));
    }

    #[test]
    fn not_answered_says_so() {
        let m = status_message(true, Some(Permission::NotDetermined));
        assert_eq!(m.title, "Notifications not answered yet");
        assert_eq!(
            m.detail,
            "Until you allow fuxx, it sends Script Editor banners instead."
        );
        assert_eq!(m.buttons, SETTINGS_FIRST);
        assert_eq!(m.settings_button, Some(0));
    }

    #[test]
    fn outside_the_app_there_is_only_ok() {
        let m = status_message(false, None);
        assert_eq!(m.title, "Not running as fuxx.app");
        assert_eq!(
            m.detail,
            "This fuxx was started outside its app, so macOS can't give it notifications of its \
             own. It sends Script Editor banners instead."
        );
        assert_eq!(m.buttons, ["OK"]);
        assert_eq!(m.settings_button, None);
        // Whatever the permission says, unbundled is that message.
        for permission in [
            Some(Permission::Allowed),
            Some(Permission::Denied),
            Some(Permission::NotDetermined),
        ] {
            assert_eq!(status_message(false, permission), m, "{permission:?}");
        }
    }

    #[test]
    fn no_answer_in_time_says_it_couldnt_read_it() {
        let m = status_message(true, None);
        assert_eq!(m.title, "Couldn't read the notification setting");
        assert_eq!(
            m.detail,
            "macOS didn't answer in time. Try again, or look in System Settings \u{2192} \
             Notifications \u{2192} fuxx."
        );
        assert_eq!(m.buttons, SETTINGS_FIRST);
        assert_eq!(m.settings_button, Some(0));
    }

    #[test]
    fn the_settings_button_opens_fuxxs_page() {
        assert_eq!(
            NOTIFICATION_SETTINGS_URL,
            "x-apple.systempreferences:com.apple.Notifications-Settings.extension?id=io.github.fitzfitz.fuxx"
        );
    }

    fn percent_decode(s: &str) -> String {
        let bytes = s.as_bytes();
        let mut out = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'%' {
                out.push(u8::from_str_radix(&s[i + 1..i + 3], 16).unwrap());
                i += 3;
            } else {
                out.push(bytes[i]);
                i += 1;
            }
        }
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn the_issue_body_ends_with_the_versions() {
        assert!(issue_body("0.4.0", Some("26.5")).ends_with("---\nfuxx 0.4.0 \u{b7} macOS 26.5"));
        assert!(issue_body("0.4.0", None).ends_with("---\nfuxx 0.4.0 \u{b7} macOS unknown"));
        assert!(
            issue_body("0.4.0", None).starts_with("**What happened**\n\n\n**What you expected**")
        );
    }

    #[test]
    fn the_issue_url_is_encoded_and_decodes_back() {
        let url = issue_url("0.4.0", Some("26.5"));
        assert!(url.starts_with(ISSUE_URL_BASE));
        let encoded = &url[ISSUE_URL_BASE.len()..];
        for c in [' ', '\n', '&', '#', '\u{b7}'] {
            assert!(!encoded.contains(c), "raw {c:?}");
        }
        assert_eq!(percent_decode(encoded), issue_body("0.4.0", Some("26.5")));
    }
}
