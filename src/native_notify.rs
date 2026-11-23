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
use objc2_foundation::{NSArray, NSBundle, NSError, NSString};
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
    #[allow(dead_code)] // read only through `Debug`, in `notifier::fire`'s log line
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

/// Show a banner: `summary` as the title, `subtitle` (if any) and `body` under it, no sound.
/// `about` (the terminal it is about, and the run that numbered it) rides in the request
/// identifier, which is the same for every banner of that terminal (part 5: a new one replaces
/// the one before), and groups its banners in Notification Center. Blocks
/// up to 2 s for the system to take it; if it doesn't answer in time, the request is withdrawn
/// (so a late one rarely shows next to the caller's fallback banner).
pub fn post(
    summary: &str,
    subtitle: &str,
    body: &str,
    about: Option<About>,
) -> Result<(), PostError> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    if !is_bundled() {
        return Err(PostError::Unbundled);
    }
    let center = UNUserNotificationCenter::currentNotificationCenter();
    let content = UNMutableNotificationContent::new();
    content.setTitle(&NSString::from_str(summary));
    content.setBody(&NSString::from_str(body));
    if !subtitle.is_empty() {
        content.setSubtitle(&NSString::from_str(subtitle));
    }
    let thread = match about {
        Some(About::Terminal(Terminal { run, id })) => Some(format!("fuxx-s{id}-r{run}")),
        Some(About::Tab { run, id }) => Some(format!("fuxx-t{id}-r{run}")),
        None => None,
    };
    if let Some(thread) = thread {
        content.setThreadIdentifier(&NSString::from_str(&thread));
    }
    let id = NSString::from_str(&request_id(
        about,
        this_run(),
        banner_number(about, COUNTER.fetch_add(1, Ordering::Relaxed)),
    ));
    let request = UNNotificationRequest::requestWithIdentifier_content_trigger(&id, &content, None);
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
        Err(_) => {
            let ids = NSArray::from_retained_slice(&[id]);
            center.removePendingNotificationRequestsWithIdentifiers(&ids);
            center.removeDeliveredNotificationsWithIdentifiers(&ids);
            Err(PostError::TimedOut)
        }
    }
}

/// A terminal a banner is about: session `sN` (`id`) as numbered by run `run`. Numbers start
/// again at `s1` in each run, so the run tells today's `s3` from an earlier run's. The run is
/// the background service's random instance id for a session it runs, or `this_run()` for one
/// a window runs itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Terminal {
    pub run: u64,
    pub id: u64,
}

impl Terminal {
    /// The session to show for a click, if it was numbered by `current` (the run numbering the
    /// sessions now; `None` when that isn't known). A banner from another run shows nothing.
    pub fn in_run(self, current: Option<u64>) -> Option<u64> {
        (current == Some(self.run)).then_some(self.id)
    }
}

/// What a banner is about, for its click: a terminal, or an agent tab (the opens fixed,
/// 2026-11-23: before, an agent's banner named nothing and its click went to the oldest
/// waiting question).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum About {
    Terminal(Terminal),
    /// Agent tab `id` (its `TabId`) of window run `run` (`this_run`): tab ids start again in
    /// each run, so another run's tab 3 is not today's.
    Tab {
        run: u64,
        id: u64,
    },
}

impl About {
    /// The terminal it is about, if it is one.
    pub fn terminal(self) -> Option<Terminal> {
        match self {
            About::Terminal(t) => Some(t),
            About::Tab { .. } => None,
        }
    }
}

/// Where a banner's click lands (`landing`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Landing {
    /// Terminal `sN` of the run numbering the sessions now.
    Terminal(u64),
    /// Agent tab `id`, still open: shown, its waiting card with the keys.
    Tab(u64),
    /// The oldest waiting question (a banner about nothing, or its tab gone).
    OldestQuestion,
    /// Only fuxx comes forward (a terminal of another run).
    Nothing,
}

/// Where a click on a banner `about` lands: a terminal of the run numbering the sessions now
/// (`numbering`); an agent tab of this window's run (`window_run`) while it is `open`; else, for
/// a banner about no terminal, the oldest waiting question. Pure.
pub fn landing(
    about: Option<About>,
    numbering: Option<u64>,
    window_run: u64,
    open: impl Fn(u64) -> bool,
) -> Landing {
    match about {
        Some(About::Terminal(t)) => t
            .in_run(numbering)
            .map_or(Landing::Nothing, Landing::Terminal),
        Some(About::Tab { run, id }) if run == window_run && open(id) => Landing::Tab(id),
        Some(About::Tab { .. }) | None => Landing::OldestQuestion,
    }
}

/// A random id for this process, made once: the run that numbers a window's own sessions, and
/// part of every banner id it posts, so ids never repeat across processes and runs (a banner
/// with the id of an earlier one would replace it in Notification Center).
pub fn this_run() -> u64 {
    use std::hash::BuildHasher;
    static RUN: OnceLock<u64> = OnceLock::new();
    *RUN.get_or_init(|| {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        // std's `RandomState` is keyed from the OS's randomness.
        std::collections::hash_map::RandomState::new().hash_one((now, std::process::id()))
    })
}

/// The `n` in a banner's request identifier (part 5): 0 for a terminal's or a tab's banners, so
/// each one replaces its last; else `count`, so a script's never replace each other.
pub fn banner_number(about: Option<About>, count: u64) -> u64 {
    if about.is_some() { 0 } else { count }
}

/// A banner's request identifier: `fuxx-s{N}-r{run}-{process}-{n}` for terminal `about` (so a
/// click can say which one, and from which run), `fuxx-t{N}-r{run}-{process}-{n}` for agent tab
/// `N`, else `fuxx-{process}-{n}`. `process` (the poster's `this_run`) and `n` (its count) make
/// each one unique.
pub fn request_id(about: Option<About>, process: u64, n: u64) -> String {
    match about {
        Some(About::Terminal(Terminal { run, id })) => format!("fuxx-s{id}-r{run}-{process}-{n}"),
        Some(About::Tab { run, id }) => format!("fuxx-t{id}-r{run}-{process}-{n}"),
        None => format!("fuxx-{process}-{n}"),
    }
}

/// What a request identifier is about (a terminal or an agent tab), if it names one in the
/// current form.
pub fn about_of(request_id: &str) -> Option<About> {
    let (tab, rest) = match request_id.strip_prefix("fuxx-s") {
        Some(rest) => (false, rest),
        None => (true, request_id.strip_prefix("fuxx-t")?),
    };
    let mut parts = rest.split('-');
    let id = parts.next()?.parse().ok()?;
    let run = parts.next()?.strip_prefix('r')?.parse().ok()?;
    let _process: u64 = parts.next()?.parse().ok()?;
    let _n: u64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(if tab {
        About::Tab { run, id }
    } else {
        About::Terminal(Terminal { run, id })
    })
}

/// What a click does. Set once, by `hear_clicks`; callable from any thread, so a click is heard
/// whichever thread macOS reports it on (the window's or the background service's).
type OnClick = Box<dyn Fn(Option<About>) + Send + Sync>;
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

        /// A click on one of fuxx's banners: `on_click` with what it is about, if anything.
        #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
        fn did_receive(
            &self,
            _center: &UNUserNotificationCenter,
            response: &UNNotificationResponse,
            handler: &DynBlock<dyn Fn()>,
        ) {
            let id = response.notification().request().identifier().to_string();
            match ON_CLICK.get() {
                Some(on_click) => on_click(about_of(&id)),
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
pub fn install(on_click: impl Fn(Option<About>) + Send + Sync + 'static) {
    if hear_clicks(on_click) {
        ask_permission(&UNUserNotificationCenter::currentNotificationCenter());
    }
}

/// Hear clicks on the banners this process posts: a click calls `on_click` with what the banner
/// is about (terminal `sN` or an agent tab, and the run that numbered it), `None` for one about
/// nothing
/// (or with an id from before banners named their run). Banners also show while fuxx is
/// the active app. macOS reports clicks on the main thread while its run loop runs (the app's,
/// or the background service's `daemon::Running::wait`). Once per process, only from the app
/// bundle and on the main thread; false (and nothing happens) otherwise.
pub fn hear_clicks(on_click: impl Fn(Option<About>) + Send + Sync + 'static) -> bool {
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
    /// The click held until the restore finishes: what it is about, if anything.
    held: Option<Option<About>>,
}

impl PendingShow {
    /// A click about `terminal`: what to show now, or `None` while `restoring` (it is held; a
    /// later click replaces a held one).
    pub fn click(&mut self, terminal: Option<About>, restoring: bool) -> Option<Option<About>> {
        if restoring {
            self.held = Some(terminal);
            None
        } else {
            self.held = None;
            Some(terminal)
        }
    }

    /// The restore has finished: the click held meanwhile, once.
    pub fn restored(&mut self) -> Option<Option<About>> {
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

    /// Terminal `sN` as numbered by run 41.
    fn t(id: u64) -> Terminal {
        Terminal { run: 41, id }
    }

    /// The terminal a request identifier is about, if it names one.
    fn terminal_of(request_id: &str) -> Option<Terminal> {
        about_of(request_id).and_then(About::terminal)
    }

    /// A banner about terminal `sN` of run 41.
    fn a(id: u64) -> About {
        About::Terminal(t(id))
    }

    #[test]
    fn a_terminals_banners_replace_each_other() {
        let about = Some(About::Terminal(Terminal { run: 7, id: 3 }));
        // Part 5: every banner of terminal 3 in this process has one identifier, so the next
        // replaces the last; a script's banners (no terminal) stay apart.
        let first = request_id(about, 42, banner_number(about, 1));
        let second = request_id(about, 42, banner_number(about, 2));
        assert_eq!(first, second);
        assert_eq!(about_of(&first), about, "a click still finds the terminal");
        assert_ne!(
            request_id(None, 42, banner_number(None, 1)),
            request_id(None, 42, banner_number(None, 2))
        );
    }

    #[test]
    fn request_ids_carry_the_terminal_and_its_run() {
        assert_eq!(request_id(Some(a(3)), 9, 7), "fuxx-s3-r41-9-7");
        assert_eq!(request_id(None, 9, 7), "fuxx-9-7");
        assert_eq!(terminal_of("fuxx-s3-r41-9-7"), Some(t(3)));
        let max = Terminal {
            run: u64::MAX,
            id: u64::MAX,
        };
        assert_eq!(
            terminal_of(&request_id(Some(About::Terminal(max)), u64::MAX, u64::MAX)),
            Some(max)
        );
        assert_eq!(terminal_of("fuxx-9-7"), None);
        // Malformed, and the old form (no run: from before ids named it) name no terminal.
        for bad in [
            "fuxx-s3-7",
            "fuxx-s3-41-9-7",
            "fuxx-s3-r41-9",
            "fuxx-s3-r41-9-7-1",
            "fuxx-sx-r41-9-7",
            "fuxx-s3-rx-9-7",
            "fuxx-s3-r41-x-7",
            "fuxx-s3-r41-9-x",
            "fuxx-s-r41-9-7",
            "fuxx-s3-r-9-7",
            "other",
            "",
        ] {
            assert_eq!(terminal_of(bad), None, "{bad}");
        }
    }

    /// An agent tab's banner (the opens fixed, 2026-11-23): it names the tab and the window's
    /// run, so a click goes to that tab; its banners replace each other as a terminal's do.
    #[test]
    fn request_ids_carry_an_agent_tab_and_its_run() {
        let tab = About::Tab { run: 41, id: 3 };
        assert_eq!(request_id(Some(tab), 9, 7), "fuxx-t3-r41-9-7");
        assert_eq!(about_of("fuxx-t3-r41-9-7"), Some(tab));
        assert_eq!(terminal_of("fuxx-t3-r41-9-7"), None, "not a terminal");
        assert_eq!(about_of("fuxx-s3-r41-9-7"), Some(a(3)));
        assert_eq!(
            request_id(Some(tab), 42, banner_number(Some(tab), 1)),
            request_id(Some(tab), 42, banner_number(Some(tab), 2)),
            "the next replaces the last"
        );
        assert_ne!(
            request_id(Some(tab), 9, 0),
            request_id(Some(a(3)), 9, 0),
            "tab 3 is not terminal s3"
        );
        for bad in [
            "fuxx-t3-41-9-7",
            "fuxx-tx-r41-9-7",
            "fuxx-t3-r41-9",
            "fuxx-t-r41-9-7",
        ] {
            assert_eq!(about_of(bad), None, "{bad}");
        }
    }

    /// Where a click lands: a terminal of the run numbering them now; an agent tab of this
    /// window's run while it is open, its question's or permission's card; else (the tab gone,
    /// or a banner about nothing) the oldest waiting question, as before.
    #[test]
    fn a_banner_click_lands_on_its_own_tab_and_falls_back_when_it_is_gone() {
        let open = |id: u64| id == 3;
        let tab = |run, id| Some(About::Tab { run, id });
        assert_eq!(landing(Some(a(5)), Some(41), 7, open), Landing::Terminal(5));
        assert_eq!(landing(Some(a(5)), Some(40), 7, open), Landing::Nothing);
        assert_eq!(landing(tab(7, 3), Some(41), 7, open), Landing::Tab(3));
        assert_eq!(
            landing(tab(7, 4), Some(41), 7, open),
            Landing::OldestQuestion
        );
        assert_eq!(
            landing(tab(6, 3), Some(41), 7, open),
            Landing::OldestQuestion,
            "another run's tab 3 is not this one"
        );
        assert_eq!(landing(None, Some(41), 7, open), Landing::OldestQuestion);
    }

    #[test]
    fn request_ids_differ_across_processes_and_runs() {
        // The same terminal and count from two processes (a window and the service, or two
        // runs of fuxx), and from two numbering runs.
        assert_ne!(request_id(Some(a(2)), 1, 0), request_id(Some(a(2)), 2, 0));
        assert_ne!(request_id(None, 1, 0), request_id(None, 2, 0));
        let other_run = About::Terminal(Terminal { run: 42, id: 2 });
        assert_ne!(
            request_id(Some(a(2)), 1, 0),
            request_id(Some(other_run), 1, 0)
        );
    }

    #[test]
    fn a_terminal_is_shown_only_in_the_run_that_numbered_it() {
        assert_eq!(t(3).in_run(Some(41)), Some(3));
        // An earlier run's s3 is not today's s3.
        assert_eq!(t(3).in_run(Some(42)), None);
        // The numbering isn't known (yet): nothing to show.
        assert_eq!(t(3).in_run(None), None);
    }

    #[test]
    fn this_process_has_one_run_id() {
        assert_eq!(this_run(), this_run());
    }

    #[test]
    fn a_click_is_shown_at_once_when_no_restore_runs() {
        let mut pending = PendingShow::default();
        assert_eq!(pending.click(Some(a(3)), false), Some(Some(a(3))));
        assert_eq!(pending.click(None, false), Some(None));
        assert_eq!(pending.restored(), None);
    }

    #[test]
    fn a_click_during_a_restore_waits_for_it_once() {
        let mut pending = PendingShow::default();
        assert_eq!(pending.click(Some(a(3)), true), None);
        assert_eq!(pending.restored(), Some(Some(a(3))));
        assert_eq!(pending.restored(), None);
        // A script's banner (no terminal) is held the same way: fuxx still comes forward.
        assert_eq!(pending.click(None, true), None);
        assert_eq!(pending.restored(), Some(None));
    }

    #[test]
    fn the_later_of_two_clicks_during_a_restore_wins() {
        let mut pending = PendingShow::default();
        assert_eq!(pending.click(Some(a(3)), true), None);
        assert_eq!(pending.click(Some(a(5)), true), None);
        assert_eq!(pending.restored(), Some(Some(a(5))));
        assert_eq!(pending.restored(), None);
    }
}
