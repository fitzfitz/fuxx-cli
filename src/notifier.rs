use std::process::Command;
use std::time::Duration;

use crate::command_name::Finished;
use crate::native_notify::{self, About, Permission, PostError};
use crate::status::{self, Status, age_label};

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
pub fn fire(summary: &str, subtitle: &str, body: &str, about: Option<About>) {
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

/// The title of a question the agent `who` asked without waiting on it (agy): the tab isn't
/// Waiting, so not Waiting's "Needs your input".
pub fn asked_title(who: &str) -> String {
    format!("{who} asked a question")
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

/// A notification's words (M11q).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Words {
    /// What follows `—` in the banner's title (`banner_title`).
    pub message: String,
    /// The banner's text under its title.
    pub body: String,
    /// The 🔔 entry's words.
    pub listed: String,
    /// A command that failed.
    pub failed: bool,
    /// Its banner goes out whatever the length of the run (no `QUIET_UNDER`).
    pub always: bool,
}

/// The words of a terminal's notification, if `status` is worth one. A command that `ended`
/// (its end mark) is told by its short form and how it ended, always. Else the status's own
/// words (`body`: `question`, `worked`, `doing`), under the short form of the command `running`
/// there when there is one (an agent started at the prompt that went quiet or asks), else
/// under `message`.
pub fn words(
    status: Status,
    question: Option<&str>,
    worked: Option<Duration>,
    doing: Option<&str>,
    running: Option<&str>,
    ended: Option<&Finished>,
) -> Option<Words> {
    if let Some(fin) = ended {
        return Some(Words {
            message: fin.name().to_string(),
            body: fin.words(),
            listed: fin.listed(),
            failed: fin.ending.failed(),
            always: true,
        });
    }
    let body = body(status, question, worked, doing)?;
    Some(Words {
        message: running.or(message(status))?.to_string(),
        listed: body.clone(),
        body,
        failed: false,
        always: false,
    })
}

/// What a command's end mark tells (plan-time call 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EndTell {
    /// The status settles on Done (`status::settles_on_end`), and that change is told by the
    /// M4 rules, with the ending's words.
    pub settle: bool,
    /// The ending goes to the 🔔 list (and to scripts).
    pub list: bool,
    /// A banner goes out for it.
    pub banner: bool,
}

/// What the end mark of a command that ran for `ran` tells about a terminal in `status`, so
/// that each ending has at most one banner. Nothing is told while you are `looking` at it (the
/// active terminal of the focused window); it settles all the same. A terminal that settles
/// here is told with a banner, however short the run. One already Done (the silence guess was
/// first) has the ending listed once (`end_told`: this Done's ending was), with a banner only
/// if this Done had none (`done_bannered`: its run was under `QUIET_UNDER`, or you were
/// looking then).
pub fn end_tells(
    status: Status,
    ran: Duration,
    looking: bool,
    done_bannered: bool,
    end_told: bool,
) -> EndTell {
    let settle = status::settles_on_end(status, ran);
    let list = !looking && (settle || (status == Status::Done && !end_told));
    EndTell {
        settle,
        list,
        banner: list && (settle || !done_bannered),
    }
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
    use crate::command_name::Ending;

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

    fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    /// A command that ended with `exit` after `ran` seconds.
    fn ended(short: Option<&str>, exit: Option<i32>, ran: u64) -> Finished {
        Finished {
            short: short.map(String::from),
            ending: Ending::of(exit),
            ran: secs(ran),
        }
    }

    /// The words of a command's end (its status is Done by then).
    fn end_words(fin: &Finished) -> Words {
        words(Status::Done, None, None, None, None, Some(fin)).expect("an ending has words")
    }

    /// M11q: the spec's four rows, in today's shape (plan-time call 1): the short form is the
    /// title's message, the ending its body.
    #[test]
    fn the_banner_table() {
        let rows = [
            ("pnpm build", Some(0), 192, "Done in 3m 12s", false),
            ("pnpm dev", Some(130), 3840, "Stopped after 1h 4m", false),
            (
                "cargo test",
                Some(101),
                40,
                "Failed (exit 101) after 40s",
                true,
            ),
            ("make", None, 120, "Finished after 2m", false),
        ];
        for (short, exit, ran, body, failed) in rows {
            let w = end_words(&ended(Some(short), exit, ran));
            assert_eq!(
                banner_title("fuxx-cli", &w.message),
                format!("fuxx-cli \u{2014} {short}")
            );
            assert_eq!(w.body, body, "{short}");
            assert_eq!(w.listed, format!("{short} \u{b7} {body}"), "{short}");
            assert_eq!(w.failed, failed, "{short}");
            assert!(w.always, "{short}: an ending is told whatever its length");
        }
        assert_eq!(banner_subtitle(2, None, "fuxx-cli", None), "#2");
        // What the terminal was doing, or what still runs there, says nothing about an ending.
        let fin = ended(Some("pnpm build"), Some(0), 192);
        let busy = words(
            Status::Done,
            Some("Proceed? (y/n)"),
            Some(secs(5)),
            Some("\u{2733} a title"),
            Some("cargo test"),
            Some(&fin),
        );
        assert_eq!(busy, Some(end_words(&fin)));
    }

    #[test]
    fn a_command_with_no_short_form_banners_command() {
        let w = end_words(&ended(None, Some(0), 5));
        assert_eq!(
            banner_title("fuxx-cli", &w.message),
            "fuxx-cli \u{2014} command"
        );
        assert_eq!(w.body, "Done in 5s");
        assert_eq!(w.listed, "command \u{b7} Done in 5s");
    }

    /// A command still running when its terminal goes quiet or asks (an agent started at the
    /// prompt): the silence guess's notification names it, and keeps its own rules.
    #[test]
    fn an_agent_started_from_the_prompt_is_named() {
        let done = words(
            Status::Done,
            None,
            Some(secs(240)),
            None,
            Some("claude"),
            None,
        )
        .expect("Done has words");
        assert_eq!(done.message, "claude");
        assert_eq!(done.body, "Worked for 4m");
        assert_eq!(done.listed, done.body);
        assert!(!done.always, "the silence guess keeps QUIET_UNDER");
        assert!(!done.failed);
        let asks = words(
            Status::Waiting,
            Some("Proceed? (y/n)"),
            None,
            None,
            Some("claude"),
            None,
        )
        .expect("Waiting has words");
        assert_eq!(asks.message, "claude");
        assert_eq!(asks.body, "Proceed? (y/n)");
        assert!(!asks.always && !asks.failed);
    }

    #[test]
    fn without_a_command_the_words_are_todays() {
        let title = Some("\u{2733} a title");
        let done = words(Status::Done, None, Some(secs(34)), title, None, None).expect("Done");
        assert_eq!(done.message, "Finished");
        assert_eq!(
            Some(&done.body),
            body(Status::Done, None, Some(secs(34)), title).as_ref()
        );
        assert_eq!(done.listed, done.body);
        assert!(!done.always && !done.failed);
        let asks = words(Status::Waiting, None, None, None, None, None).expect("Waiting");
        assert_eq!(asks.message, "Needs your input");
        assert_eq!(asks.body, "Needs your input");
        for quiet in [Status::Idle, Status::Working] {
            assert_eq!(words(quiet, None, None, None, None, None), None);
            assert_eq!(
                words(quiet, None, None, None, Some("claude"), None),
                None,
                "a running command alone is no news"
            );
        }
    }

    /// Plan-time call 2: what an end mark tells, by the status it finds.
    #[test]
    fn end_tells_table() {
        let tell = |settle, list, banner| EndTell {
            settle,
            list,
            banner,
        };
        let all = tell(true, true, true);
        let nothing = tell(false, false, false);
        for (bannered, told) in [(false, false), (true, false), (false, true), (true, true)] {
            for busy in [Status::Working, Status::Waiting] {
                assert_eq!(
                    end_tells(busy, secs(1), false, bannered, told),
                    all,
                    "{busy:?}: it settles, and that is news"
                );
                assert_eq!(
                    end_tells(busy, secs(1), true, bannered, told),
                    tell(true, false, false),
                    "{busy:?}: not while you look at it"
                );
            }
            for looking in [false, true] {
                assert_eq!(
                    end_tells(Status::Idle, secs(2), looking, bannered, told),
                    nothing,
                    "a quick silent command"
                );
                assert_eq!(
                    end_tells(Status::Done, secs(60), looking, bannered, true),
                    nothing,
                    "this Done's ending was told"
                );
            }
            assert_eq!(
                end_tells(Status::Done, secs(60), true, bannered, told),
                nothing,
                "Done, and you look at it"
            );
        }
        assert_eq!(end_tells(Status::Idle, secs(3), false, false, false), all);
        assert_eq!(
            end_tells(Status::Idle, secs(3), true, false, false),
            tell(true, false, false)
        );
        assert_eq!(
            end_tells(Status::Done, secs(60), false, false, false),
            tell(false, true, true),
            "this Done had no banner (a short run, or you were looking)"
        );
        assert_eq!(
            end_tells(Status::Done, secs(60), false, true, false),
            tell(false, true, false),
            "this Done had its banner: the ending is only listed"
        );
    }

    /// Review Focus 2: mark text is untrusted. Whatever a short form holds, its title is safe
    /// inside the AppleScript string `fire_script` builds.
    #[test]
    fn a_command_name_is_escaped_like_a_title() {
        let w = end_words(&ended(Some("e\"vil\\ \u{7}\n\"; beep"), Some(0), 1));
        let escaped = applescript_escape(&banner_title("fuxx-cli", &w.message));
        assert!(!escaped.chars().any(char::is_control), "{escaped:?}");
        let mut chars = escaped.chars();
        while let Some(c) = chars.next() {
            assert_ne!(c, '"', "a bare quote ends the string: {escaped:?}");
            if c == '\\' {
                let next = chars.next();
                assert!(
                    matches!(next, Some('\\' | '"')),
                    "a backslash escapes only a quote or itself: {escaped:?}"
                );
            }
        }
        assert_eq!(escaped, "fuxx-cli \u{2014} e\\\"vil\\\\ \\\"; beep");
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

    /// The lead's nit (the lanes review): agy's question (no Waiting follows) has its own
    /// title, not Waiting's "Needs your input".
    #[test]
    fn a_question_the_agent_didn_t_wait_on_has_its_own_title() {
        assert_eq!(asked_title("agy"), "agy asked a question");
        assert_eq!(
            banner_title("my-app", &asked_title("agy")),
            "my-app \u{2014} agy asked a question"
        );
    }

    #[test]
    fn messages() {
        assert_eq!(message(Status::Done), Some("Finished"));
        assert_eq!(message(Status::Waiting), Some("Needs your input"));
        assert_eq!(message(Status::Working), None);
    }
}
