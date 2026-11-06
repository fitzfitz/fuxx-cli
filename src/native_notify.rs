//! Native macOS banners (M11c): Apple's UserNotifications framework, named "fuxx" and shown from
//! the app bundle. Only usable when the running binary is inside `fuxx.app` — unbundled, the
//! framework throws, so nothing here touches it unless `is_bundled()`.

use std::cell::RefCell;
use std::sync::{OnceLock, mpsc};
use std::time::Duration;

use block2::{DynBlock, RcBlock};
use objc2::rc::Retained;
use objc2::runtime::{Bool, NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{ClassType, MainThreadMarker, define_class, msg_send};
use objc2_foundation::{NSBundle, NSError, NSString};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNAuthorizationStatus, UNMutableNotificationContent, UNNotification,
    UNNotificationPresentationOptions, UNNotificationRequest, UNNotificationResponse,
    UNNotificationSettings, UNUserNotificationCenter, UNUserNotificationCenterDelegate,
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
pub fn terminal_of(request_id: &str) -> Option<u64> {
    let rest = request_id.strip_prefix("fuxx-s")?;
    let (terminal, _) = rest.split_once('-')?;
    terminal.parse().ok()
}

/// What a click does. Set once, by `hear_clicks`; callable from any thread, so a click is heard
/// whichever thread macOS reports it on (the window's or the background service's).
type OnClick = Box<dyn Fn(Option<u64>) + Send + Sync>;
static ON_CLICK: OnceLock<OnClick> = OnceLock::new();

thread_local! {
    /// The delegate, kept on the main thread where `hear_clicks` made it: macOS holds only a
    /// weak reference to it.
    static DELEGATE: RefCell<Option<Retained<ClickDelegate>>> = const { RefCell::new(None) };
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements; no ivars, no `Drop`.
    #[unsafe(super = NSObject)]
    struct ClickDelegate;

    unsafe impl NSObjectProtocol for ClickDelegate {}

    unsafe impl UNUserNotificationCenterDelegate for ClickDelegate {
        /// A banner arriving while fuxx is the active app is still shown (macOS hides it by
        /// default then), and kept in Notification Center as usual. No sound.
        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn will_present(
            &self,
            _center: &UNUserNotificationCenter,
            _notification: &UNNotification,
            handler: &DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            handler
                .call((UNNotificationPresentationOptions::Banner
                    | UNNotificationPresentationOptions::List,));
        }

        /// A click on one of fuxx's banners: `on_click` with the terminal it is about, if any.
        #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
        fn did_receive(
            &self,
            _center: &UNUserNotificationCenter,
            response: &UNNotificationResponse,
            handler: &DynBlock<dyn Fn()>,
        ) {
            let id = response.notification().request().identifier().to_string();
            match ON_CLICK.get() {
                Some(on_click) => on_click(terminal_of(&id)),
                None => eprintln!("[fuxx] notifications: a click on {id} with nobody to hear it"),
            }
            handler.call(());
        }
    }
);

/// Hear clicks on fuxx's banners (the window's side, at launch, before the app runs so a click
/// that opened fuxx is not missed): `hear_clicks`, then asks macOS, once, for permission to
/// show alerts (no sound), without waiting for the answer. Only from the app bundle (and the
/// main thread); otherwise nothing happens.
pub fn install(on_click: impl Fn(Option<u64>) + Send + Sync + 'static) {
    if hear_clicks(on_click) {
        ask_permission(&UNUserNotificationCenter::currentNotificationCenter());
    }
}

/// Hear clicks on the banners this process posts: a click calls `on_click` with the terminal
/// (`sN`) the banner is about, `None` for one about no terminal. Banners also show while fuxx is
/// the active app. macOS reports clicks on the main thread while its run loop runs (the app's,
/// or the background service's `daemon::Running::wait`). Once per process, only from the app
/// bundle and on the main thread; false (and nothing happens) otherwise.
pub fn hear_clicks(on_click: impl Fn(Option<u64>) + Send + Sync + 'static) -> bool {
    if MainThreadMarker::new().is_none() || !is_bundled() {
        return false;
    }
    if ON_CLICK.set(Box::new(on_click)).is_err() {
        return false;
    }
    // SAFETY: `new` on an NSObject subclass with no ivars returns a fresh, initialised object.
    let delegate: Retained<ClickDelegate> = unsafe { msg_send![ClickDelegate::class(), new] };
    let center = UNUserNotificationCenter::currentNotificationCenter();
    center.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    DELEGATE.set(Some(delegate));
    true
}

/// Ask macOS for permission to show alerts (no sound). Not waited for: the answer only matters
/// to later banners, which read it (`permission`). macOS asks the person only the first time.
fn ask_permission(center: &UNUserNotificationCenter) {
    let done = RcBlock::new(|_granted: Bool, error: *mut NSError| {
        // SAFETY: when non-null, the system passes a live NSError for the duration of the call.
        if let Some(e) = unsafe { error.as_ref() } {
            eprintln!(
                "[fuxx] notifications: asking permission: {}",
                e.localizedDescription()
            );
        }
    });
    center.requestAuthorizationWithOptions_completionHandler(UNAuthorizationOptions::Alert, &done);
}

/// A banner click to show once the window can: at once, or (clicked while the saved sessions
/// are still reopening, as when the click itself opened fuxx) once that has finished. Pure.
#[derive(Debug, Default)]
pub struct PendingShow {
    /// The click held until the restore finishes: the terminal it is about, if any.
    held: Option<Option<u64>>,
}

impl PendingShow {
    /// A click about `terminal`: what to show now, or `None` while `restoring` (it is held; a
    /// later click replaces a held one).
    pub fn click(&mut self, terminal: Option<u64>, restoring: bool) -> Option<Option<u64>> {
        if restoring {
            self.held = Some(terminal);
            None
        } else {
            self.held = None;
            Some(terminal)
        }
    }

    /// The restore has finished: the click held meanwhile, once.
    pub fn restored(&mut self) -> Option<Option<u64>> {
        self.held.take()
    }
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

    #[test]
    fn a_click_is_shown_at_once_when_no_restore_runs() {
        let mut pending = PendingShow::default();
        assert_eq!(pending.click(Some(3), false), Some(Some(3)));
        assert_eq!(pending.click(None, false), Some(None));
        assert_eq!(pending.restored(), None);
    }

    #[test]
    fn a_click_during_a_restore_waits_for_it_once() {
        let mut pending = PendingShow::default();
        assert_eq!(pending.click(Some(3), true), None);
        assert_eq!(pending.restored(), Some(Some(3)));
        assert_eq!(pending.restored(), None);
        // A script's banner (no terminal) is held the same way: fuxx still comes forward.
        assert_eq!(pending.click(None, true), None);
        assert_eq!(pending.restored(), Some(None));
    }

    #[test]
    fn the_later_of_two_clicks_during_a_restore_wins() {
        let mut pending = PendingShow::default();
        assert_eq!(pending.click(Some(3), true), None);
        assert_eq!(pending.click(Some(5), true), None);
        assert_eq!(pending.restored(), Some(Some(5)));
        assert_eq!(pending.restored(), None);
    }
}
