// Releases and versions for the self-updater: the version order, GitHub's "latest release" JSON,
// the checks a download must pass, and the swap script's text.
// Pure: no network, no UI. The check that fetches the JSON and the install are in `updater`.

use std::fmt;
use std::path::{Path, PathBuf};

/// A release number: major, minor, patch. Compares field by field, so 0.10.0 is newer than 0.9.0.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Version(pub u64, pub u64, pub u64);

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

/// Reads `0.3.0` or `v0.3.0`. Anything else (two parts, a suffix like `-beta`, junk) is None.
pub fn parse_version(s: &str) -> Option<Version> {
    let s = s.strip_prefix('v').unwrap_or(s);
    let mut parts = s.split('.');
    let mut next = || {
        let p = parts.next()?;
        if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        p.parse::<u64>().ok()
    };
    let v = Version(next()?, next()?, next()?);
    parts.next().is_none().then_some(v)
}

/// The version of the running app, from Cargo.toml.
pub fn current() -> Version {
    parse_version(env!("CARGO_PKG_VERSION")).expect("Cargo.toml version is MAJOR.MINOR.PATCH")
}

/// True when `release` is strictly newer than `running`.
pub fn is_newer(release: Version, running: Version) -> bool {
    release > running
}

/// A release asset we can download: its URL and, when GitHub gave one, its SHA-256 (lowercase hex).
#[derive(Clone, Debug, PartialEq)]
pub struct Asset {
    pub url: String,
    pub sha256: Option<String>,
}

/// GitHub's latest release, reduced to what the updater needs.
#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub version: Version,
    pub page_url: String,
    pub notes: String,
    /// The asset named exactly `fuxx-{version}.zip`, if the release has one.
    pub zip: Option<Asset>,
}

/// Reads GitHub's "latest release" JSON. A GitHub error answer (`{"message": …}`) becomes Err
/// carrying that message; so do invalid JSON and a tag that is not a version.
pub fn parse_latest(json: &str) -> Result<Release, String> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("invalid release JSON: {e}"))?;
    let Some(tag) = v.get("tag_name").and_then(|t| t.as_str()) else {
        return Err(match v.get("message").and_then(|m| m.as_str()) {
            Some(m) => m.to_string(),
            None => "release JSON has no tag_name".to_string(),
        });
    };
    let version = parse_version(tag).ok_or_else(|| format!("unrecognised version {tag:?}"))?;
    let text = |key: &str| {
        v.get(key)
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string()
    };
    let wanted = format!("fuxx-{version}.zip");
    let zip = v
        .get("assets")
        .and_then(|a| a.as_array())
        .into_iter()
        .flatten()
        .find(|a| a.get("name").and_then(|n| n.as_str()) == Some(wanted.as_str()))
        .and_then(|a| {
            let url = a.get("browser_download_url")?.as_str()?.to_string();
            let sha256 = a.get("digest").and_then(|d| d.as_str()).and_then(sha256_of);
            Some(Asset { url, sha256 })
        });
    Ok(Release {
        version,
        page_url: text("html_url"),
        notes: text("body"),
        zip,
    })
}

/// The hex of a `sha256:` digest, lowercase, when it is exactly 64 hex digits; else None.
fn sha256_of(digest: &str) -> Option<String> {
    let hex = digest.strip_prefix("sha256:")?;
    (hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| hex.to_ascii_lowercase())
}

/// What a check found, for the running version.
#[derive(Clone, Debug, PartialEq)]
pub enum CheckOutcome {
    /// GitHub's latest release is newer than the running app.
    Newer(Release),
    /// The latest release is the running version or older; carries the running version.
    UpToDate(Version),
    /// Offline, GitHub's error, or JSON we can't read: the reason.
    Failed(String),
}

/// What a check found: the fetched-and-parsed release (or why there is none) against `running`.
pub fn check_outcome(result: Result<Release, String>, running: Version) -> CheckOutcome {
    match result {
        Ok(release) if is_newer(release.version, running) => CheckOutcome::Newer(release),
        Ok(_) => CheckOutcome::UpToDate(running),
        Err(reason) => CheckOutcome::Failed(reason),
    }
}

/// The manual check's sheet when there is nothing newer.
pub fn up_to_date_title(v: Version) -> String {
    format!("fuxx {v} is the newest version.")
}

/// The manual check's sheet when it couldn't ask (the reason is its text).
pub const CHECK_FAILED: &str = "Couldn't check for updates";

/// The update offer's sheet: its title, its text, and its buttons in order (the first updates,
/// or downloads from GitHub when `self_update` is false; the last is Later).
#[derive(Clone, Debug, PartialEq)]
pub struct Offer {
    pub title: String,
    pub detail: String,
    pub buttons: Vec<&'static str>,
    pub self_update: bool,
}

/// How many lines of the release notes the offer shows.
pub const OFFER_NOTE_LINES: usize = 6;

/// The offer of `release` to the `running` app: the version you have, the notes' first lines,
/// and how many sessions restarting ends (said only when some do). Update and Restart when fuxx
/// can update itself, else Download from GitHub….
pub fn offer(
    release: &Release,
    running: Version,
    ending_sessions: usize,
    can_self_update: bool,
) -> Offer {
    let mut parts = vec![format!("You have {running}.")];
    let notes = notes_excerpt(&release.notes, OFFER_NOTE_LINES);
    if !notes.is_empty() {
        parts.push(notes);
    }
    if ending_sessions > 0 {
        let s = if ending_sessions == 1 { "" } else { "s" };
        parts.push(format!("Restarting ends {ending_sessions} session{s}."));
    }
    let first = if can_self_update {
        "Update and Restart"
    } else {
        "Download from GitHub\u{2026}"
    };
    Offer {
        title: format!("fuxx {} is available", release.version),
        detail: parts.join("\n\n"),
        buttons: vec![first, "Later"],
        self_update: can_self_update,
    }
}

/// The first `lines` non-empty lines of the release notes, joined by newlines.
pub fn notes_excerpt(body: &str, lines: usize) -> String {
    body.lines()
        .map(str::trim_end)
        .filter(|l| !l.trim().is_empty())
        .take(lines)
        .collect::<Vec<_>>()
        .join("\n")
}

/// The error sheet's title when a download or a check fails (its text is the reason); its
/// buttons are Open Release Page and OK.
pub const INSTALL_FAILED: &str = "The update couldn't be installed";
/// Why: fuxx no longer runs from a `fuxx.app` it may swap (or has no home folder to download to).
pub const CANT_UPDATE_HERE: &str = "fuxx can't update itself here";
/// Why: the download folder couldn't be emptied or made.
pub const CACHE_FAILED: &str = "the download folder couldn't be emptied";
/// Why: curl failed (offline, an HTTP error, or the 10 minutes ran out).
pub const DOWNLOAD_FAILED: &str = "the download failed";
/// Why: the swap script couldn't be written or started.
pub const SWAP_FAILED: &str = "the swap couldn't be started";

/// What the downloaded zip unpacked to: how many `*.app` it held, the one app's folder name, its
/// bundle identifier and version (from its Info.plist; empty when unreadable), and whether
/// `codesign --verify --deep --strict` passed.
#[derive(Clone, Debug, PartialEq)]
pub struct Unpacked {
    pub bundle_id: String,
    pub version: String,
    pub signature_ok: bool,
    pub apps: usize,
    pub app_name: String,
}

/// Whether the download's digest, `actual` (from shasum), is the release's, `expected`: both
/// exactly 64 hex digits, and the same.
pub fn checksum_matches(expected: &str, actual: &str) -> bool {
    let hex = |s: &str| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit());
    hex(expected) && hex(actual) && expected.eq_ignore_ascii_case(actual)
}

/// Whether the download may replace the installed fuxx: its checksum, then exactly one app,
/// named `fuxx.app`, that is fuxx (`native_notify::BUNDLE_ID`), at the `expected` version, with
/// an intact signature. Err is the first check that failed, worded for the error sheet.
pub fn verify(
    expected: Version,
    sha256_expected: &str,
    sha256_actual: &str,
    u: &Unpacked,
) -> Result<(), String> {
    if !checksum_matches(sha256_expected, sha256_actual) {
        return Err("the download is damaged (checksum)".into());
    }
    if u.apps != 1 {
        return Err(format!("the zip holds {} apps", u.apps));
    }
    if u.app_name != "fuxx.app" {
        return Err("the zip holds no fuxx.app".into());
    }
    if u.bundle_id.is_empty() {
        return Err("not fuxx (no bundle id)".into());
    }
    if u.bundle_id != crate::native_notify::BUNDLE_ID {
        return Err(format!("not fuxx ({})", u.bundle_id));
    }
    if parse_version(&u.version) != Some(expected) {
        return Err(format!("version {} instead of {expected}", u.version));
    }
    if !u.signature_ok {
        return Err("the app's signature is broken".into());
    }
    Ok(())
}

/// Where the swap keeps the app it replaced: `<app>.previous`, beside it.
pub fn previous_app(app: &Path) -> PathBuf {
    let mut name = app.as_os_str().to_os_string();
    name.push(".previous");
    PathBuf::from(name)
}

/// `path` as one shell word: in single quotes, each `'` inside written `'\''`.
pub fn sh_quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', r"'\''"))
}

/// How many times the swap script looks for the old fuxx, 0.2 s apart: 60 s in all.
pub const SWAP_POLLS: u32 = 300;

/// The swap script (POSIX sh, run detached by `/bin/sh` once every check has passed). It waits
/// until fuxx's process `pid` has ended (at most 60 s; still running, it changes nothing), then
/// renames `app` to `<app>.previous` (removing a leftover one first, so the rename can't land
/// inside it) and moves `new_app` into its place, opening it with `opener` (a command:
/// `/usr/bin/open`, or `true` in tests). Something already at `app` before that move counts as
/// the move failing (it would land inside). When the move fails, whatever is at `app` is moved
/// aside and the previous app put back and opened; if it can't be put back, nothing is opened
/// (never a partial copy, whose launch would delete the previous app) and the previous app stays
/// beside it. Every path is quoted, every tool is called by its absolute path (PATH is
/// `/bin:/usr/bin` too), and the installed app is only ever renamed, never deleted.
pub fn swap_script(pid: u32, app: &Path, new_app: &Path, opener: &str) -> String {
    let mut incomplete = app.as_os_str().to_os_string();
    incomplete.push(".incomplete");
    format!(
        r#"#!/bin/sh
# fuxx's update: once fuxx (pid {pid}) has quit, swap the new fuxx.app in and open it.
PATH=/bin:/usr/bin
export PATH
app={app}
new={new}
previous={previous}
incomplete={incomplete}
polls=0
while kill -0 {pid} 2>/dev/null; do
  if [ "$polls" -ge {SWAP_POLLS} ]; then
    exit 1 # fuxx is still running after 60 s: change nothing
  fi
  /bin/sleep 0.2
  polls=$((polls + 1))
done
# A leftover previous app goes first: renaming onto a folder would land inside it.
/bin/rm -rf "$previous"
if [ -e "$previous" ] || [ -L "$previous" ] || ! /bin/mv "$app" "$previous"; then
  {opener} "$app"
  exit 1
fi
# Move the new app in. Something at its place already would take it inside: that's a failure.
if [ ! -e "$app" ] && [ ! -L "$app" ] && /bin/mv "$new" "$app"; then
  /usr/bin/xattr -dr com.apple.quarantine "$app" 2>/dev/null
  {opener} "$app"
  exit 0
fi
# It failed: move aside whatever is at the app's place (a half-moved copy), put the previous
# app back and open it. Not put back: open nothing, and the previous app stays beside it.
if [ -e "$app" ] || [ -L "$app" ]; then
  /bin/rm -rf "$incomplete"
  /bin/mv "$app" "$incomplete"
fi
if [ ! -e "$app" ] && [ ! -L "$app" ] && /bin/mv "$previous" "$app"; then
  {opener} "$app"
fi
exit 1
"#,
        app = sh_quote(app),
        new = sh_quote(new_app),
        previous = sh_quote(&previous_app(app)),
        incomplete = sh_quote(Path::new(&incomplete)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "tag_name": "v0.3.1",
        "html_url": "https://github.com/x/fuxx/releases/tag/v0.3.1",
        "body": "One\n\nTwo",
        "assets": [
            {"name": "fuxx-0.3.1.dmg", "browser_download_url": "https://x/fuxx-0.3.1.dmg", "digest": "sha256:FFFF"},
            {"name": "fuxx-0.3.1.zip", "browser_download_url": "https://x/fuxx-0.3.1.zip", "digest": "sha256:0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF"}
        ]
    }"#;

    /// A valid SHA-256: 64 hex digits.
    const HEX64: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn parses_versions() {
        assert_eq!(parse_version("0.3.0"), Some(Version(0, 3, 0)));
        assert_eq!(parse_version("v0.3.0"), Some(Version(0, 3, 0)));
        for bad in [
            "v1.2",
            "1.2.3-beta",
            "",
            "x.y.z",
            "+1.2.3",
            " 1.2.3",
            "99999999999999999999.0.0",
            "1.2.3.4",
        ] {
            assert_eq!(parse_version(bad), None, "{bad}");
        }
        assert_eq!(Version(0, 3, 0).to_string(), "0.3.0");
    }

    #[test]
    fn current_matches_cargo_version() {
        assert_eq!(Some(current()), parse_version(env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn orders_versions_numerically() {
        assert!(is_newer(Version(0, 10, 0), Version(0, 9, 0)));
        assert!(!is_newer(Version(0, 3, 0), Version(0, 3, 0)));
        assert!(!is_newer(Version(0, 2, 9), Version(0, 3, 0)));
    }

    #[test]
    fn parses_the_latest_release() {
        let r = parse_latest(SAMPLE).unwrap();
        assert_eq!(r.version, Version(0, 3, 1));
        assert_eq!(r.page_url, "https://github.com/x/fuxx/releases/tag/v0.3.1");
        assert_eq!(r.notes, "One\n\nTwo");
        let zip = r.zip.unwrap();
        assert_eq!(zip.url, "https://x/fuxx-0.3.1.zip");
        assert_eq!(zip.sha256, Some(HEX64.to_string()));
    }

    #[test]
    fn no_matching_asset_or_digest() {
        let no_zip = SAMPLE.replace("fuxx-0.3.1.zip", "other.zip");
        assert_eq!(parse_latest(&no_zip).unwrap().zip, None);
        let no_digest = SAMPLE.replace(
            &format!(r#", "digest": "sha256:{}""#, HEX64.to_uppercase()),
            "",
        );
        assert_eq!(parse_latest(&no_digest).unwrap().zip.unwrap().sha256, None);
        let null_digest = SAMPLE.replace(&format!(r#""sha256:{}""#, HEX64.to_uppercase()), "null");
        assert_eq!(
            parse_latest(&null_digest).unwrap().zip.unwrap().sha256,
            None
        );
    }

    #[test]
    fn null_body_is_empty_notes() {
        let j = r#"{"tag_name":"v0.3.1","html_url":"u","body":null,"assets":[]}"#;
        assert_eq!(parse_latest(j).unwrap().notes, "");
    }

    #[test]
    fn errors() {
        let e = parse_latest(r#"{"message":"API rate limit exceeded","documentation_url":"d"}"#)
            .unwrap_err();
        assert!(e.contains("API rate limit exceeded"), "{e}");
        assert!(parse_latest("not json").is_err());
        let bad = SAMPLE.replace("v0.3.1\"", "banana\"");
        let e = parse_latest(&bad).unwrap_err();
        assert!(e.starts_with("unrecognised version"), "{e}");
    }

    #[test]
    fn excerpt_takes_first_non_empty_lines() {
        let body = (1..=10)
            .map(|i| format!("line {i}\n\n"))
            .collect::<String>();
        let ex = notes_excerpt(&body, 6);
        assert_eq!(ex.lines().count(), 6);
        assert_eq!(ex.lines().last(), Some("line 6"));
    }

    /// The zip's digest as read from `SAMPLE` with its digest replaced by `digest`.
    fn digest_of(digest: &str) -> Option<String> {
        let j = SAMPLE.replace(&format!("sha256:{}", HEX64.to_uppercase()), digest);
        parse_latest(&j).unwrap().zip.unwrap().sha256
    }

    #[test]
    fn accepts_only_a_sha256_of_64_hex_digits() {
        assert_eq!(
            digest_of(&format!("sha256:{HEX64}")),
            Some(HEX64.to_string())
        );
        let short = &HEX64[..63];
        let long = format!("{HEX64}0");
        let not_hex = format!("{}g", &HEX64[..63]);
        for bad in [
            String::new(),
            "sha256:".to_string(),
            format!("sha256:{short}"),
            format!("sha256:{long}"),
            format!("sha256:{not_hex}"),
            format!("sha512:{HEX64}"),
            HEX64.to_string(),
            format!("sha256: {}", &HEX64[1..]),
        ] {
            assert_eq!(digest_of(&bad), None, "{bad:?}");
        }
    }

    fn release(v: Version) -> Release {
        Release {
            version: v,
            page_url: "https://github.com/fitzfitz/fuxx-cli/releases/tag/v0.3.1".into(),
            notes: String::new(),
            zip: None,
        }
    }

    #[test]
    fn check_outcomes() {
        let running = Version(0, 3, 0);
        let newer = release(Version(0, 3, 1));
        assert_eq!(
            check_outcome(Ok(newer.clone()), running),
            CheckOutcome::Newer(newer)
        );
        assert_eq!(
            check_outcome(Ok(release(running)), running),
            CheckOutcome::UpToDate(running)
        );
        // An older release (a pulled one): still up to date, at the running version.
        assert_eq!(
            check_outcome(Ok(release(Version(0, 2, 9))), running),
            CheckOutcome::UpToDate(running)
        );
        assert_eq!(
            check_outcome(Err("offline or unreachable".into()), running),
            CheckOutcome::Failed("offline or unreachable".into())
        );
    }

    fn offered(notes: &str, zip: bool) -> Release {
        Release {
            version: Version(0, 3, 1),
            page_url: "https://github.com/fitzfitz/fuxx-cli/releases/tag/v0.3.1".into(),
            notes: notes.into(),
            zip: zip.then(|| Asset {
                url: "https://x/fuxx-0.3.1.zip".into(),
                sha256: Some(HEX64.into()),
            }),
        }
    }

    #[test]
    fn the_offer_with_no_sessions_ending() {
        let o = offer(&offered("Faster\n\nFixes", true), Version(0, 3, 0), 0, true);
        assert_eq!(
            o,
            Offer {
                title: "fuxx 0.3.1 is available".into(),
                detail: "You have 0.3.0.\n\nFaster\nFixes".into(),
                buttons: vec!["Update and Restart", "Later"],
                self_update: true,
            }
        );
    }

    #[test]
    fn the_offer_says_how_many_sessions_restarting_ends() {
        let o = offer(&offered("Faster", true), Version(0, 3, 0), 4, true);
        assert_eq!(
            o.detail,
            "You have 0.3.0.\n\nFaster\n\nRestarting ends 4 sessions."
        );
        let one = offer(&offered("Faster", true), Version(0, 3, 0), 1, true);
        assert!(
            one.detail.ends_with("\n\nRestarting ends 1 session."),
            "{}",
            one.detail
        );
        // No notes: no empty paragraph between.
        let bare = offer(&offered("", true), Version(0, 3, 0), 2, true);
        assert_eq!(
            bare.detail,
            "You have 0.3.0.\n\nRestarting ends 2 sessions."
        );
    }

    #[test]
    fn the_offer_downloads_from_github_when_it_cant_update_itself() {
        let o = offer(&offered("Faster", true), Version(0, 3, 0), 3, false);
        assert_eq!(o.buttons, vec!["Download from GitHub\u{2026}", "Later"]);
        assert!(!o.self_update);
        assert_eq!(o.title, "fuxx 0.3.1 is available");
    }

    #[test]
    fn the_offer_cuts_the_notes_to_six_lines() {
        let notes = (1..=9)
            .map(|i| format!("- change {i}\n"))
            .collect::<String>();
        let o = offer(&offered(&notes, true), Version(0, 3, 0), 0, true);
        assert_eq!(OFFER_NOTE_LINES, 6);
        let lines: Vec<&str> = o.detail.lines().collect();
        assert_eq!(lines.len(), 2 + 6, "{lines:?}");
        assert_eq!(lines.last(), Some(&"- change 6"));
    }

    /// What a good download unpacks to, for 0.3.1.
    fn unpacked() -> Unpacked {
        Unpacked {
            bundle_id: "io.github.fitzfitz.fuxx".into(),
            version: "0.3.1".into(),
            signature_ok: true,
            apps: 1,
            app_name: "fuxx.app".into(),
        }
    }

    #[test]
    fn a_good_download_passes_every_check() {
        assert_eq!(verify(Version(0, 3, 1), HEX64, HEX64, &unpacked()), Ok(()));
    }

    #[test]
    fn each_failed_check_says_why() {
        let v = Version(0, 3, 1);
        let other = "f".repeat(64);
        let damaged = Err("the download is damaged (checksum)".to_string());
        assert_eq!(verify(v, HEX64, &other, &unpacked()), damaged);
        // No digest read at all (shasum failed) never matches, not even an empty one.
        assert_eq!(verify(v, HEX64, "", &unpacked()), damaged);
        assert_eq!(verify(v, "", "", &unpacked()), damaged);
        for apps in [0, 2] {
            let u = Unpacked { apps, ..unpacked() };
            assert_eq!(
                verify(v, HEX64, HEX64, &u),
                Err(format!("the zip holds {apps} apps"))
            );
        }
        let u = Unpacked {
            bundle_id: "com.example.other".into(),
            ..unpacked()
        };
        assert_eq!(
            verify(v, HEX64, HEX64, &u),
            Err("not fuxx (com.example.other)".into())
        );
        for version in ["0.3.0", "", "0.3.1-beta"] {
            let u = Unpacked {
                version: version.into(),
                ..unpacked()
            };
            assert_eq!(
                verify(v, HEX64, HEX64, &u),
                Err(format!("version {version} instead of 0.3.1"))
            );
        }
        let u = Unpacked {
            signature_ok: false,
            ..unpacked()
        };
        assert_eq!(
            verify(v, HEX64, HEX64, &u),
            Err("the app's signature is broken".into())
        );
        // The checksum is checked first: a damaged download says so, whatever else is wrong.
        let all_wrong = Unpacked {
            bundle_id: String::new(),
            version: String::new(),
            signature_ok: false,
            apps: 0,
            app_name: String::new(),
        };
        assert_eq!(verify(v, HEX64, &other, &all_wrong), damaged);
    }

    #[test]
    fn the_one_app_must_be_fuxx_app_with_a_readable_bundle_id() {
        let v = Version(0, 3, 1);
        let u = Unpacked {
            app_name: "Other.app".into(),
            ..unpacked()
        };
        assert_eq!(
            verify(v, HEX64, HEX64, &u),
            Err("the zip holds no fuxx.app".into())
        );
        // A name that only differs in case is not fuxx.app either.
        let u = Unpacked {
            app_name: "Fuxx.app".into(),
            ..unpacked()
        };
        assert_eq!(
            verify(v, HEX64, HEX64, &u),
            Err("the zip holds no fuxx.app".into())
        );
        // Its Info.plist couldn't be read.
        let u = Unpacked {
            bundle_id: String::new(),
            ..unpacked()
        };
        assert_eq!(
            verify(v, HEX64, HEX64, &u),
            Err("not fuxx (no bundle id)".into())
        );
    }

    #[test]
    fn the_swap_script_runs_its_tools_by_absolute_path() {
        use std::path::Path;
        let text = swap_script(
            1,
            Path::new("/A/fuxx.app"),
            Path::new("/C/fuxx.app"),
            "/usr/bin/open",
        );
        assert!(
            text.contains("\nPATH=/bin:/usr/bin\nexport PATH\n"),
            "{text}"
        );
        for line in text.lines() {
            // The code only: a comment starts a line or follows a space.
            let code = if line.trim_start().starts_with('#') {
                ""
            } else {
                line.split(" #").next().unwrap_or("")
            };
            for word in code.split_whitespace() {
                assert!(
                    !["mv", "rm", "sleep", "xattr", "open", "chmod", "cp"].contains(&word),
                    "bare {word:?} in {line:?}"
                );
            }
        }
        for tool in ["/bin/mv ", "/bin/rm -rf ", "/bin/sleep ", "/usr/bin/xattr "] {
            assert!(text.contains(tool), "{tool}");
        }
    }

    /// A fresh folder for one swap test, with a space and a quote in its name (as `~/My Apps`).
    fn swap_dir(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("fuxx swap 'test' {name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A directory standing in for an app, holding `which` = `content`.
    fn fake_app(path: &std::path::Path, content: &str) {
        std::fs::create_dir_all(path).unwrap();
        std::fs::write(path.join("which"), content).unwrap();
    }

    fn which(app: &std::path::Path) -> String {
        std::fs::read_to_string(app.join("which")).unwrap()
    }

    /// The pid of a process that has already exited (and been reaped).
    fn gone_pid() -> u32 {
        let mut child = std::process::Command::new("true").spawn().unwrap();
        let pid = child.id();
        child.wait().unwrap();
        pid
    }

    /// Run `swap_script` for real, as fuxx does: written to a file, run by `/bin/sh`. True when
    /// it says the new app went in (exit 0).
    fn run_swap(
        dir: &std::path::Path,
        app: &std::path::Path,
        new: &std::path::Path,
        opener: &str,
    ) -> bool {
        let script = dir.join("swap.sh");
        std::fs::write(&script, swap_script(gone_pid(), app, new, opener)).unwrap();
        std::process::Command::new("/bin/sh")
            .arg(&script)
            .status()
            .unwrap()
            .success()
    }

    #[test]
    fn the_swap_moves_the_new_app_in_and_keeps_the_old_beside_it() {
        let dir = swap_dir("ok");
        let app = dir.join("My Apps").join("fuxx.app");
        let new = dir.join("cache it").join("fuxx.app");
        fake_app(&app, "old");
        fake_app(&new, "new");
        assert!(run_swap(&dir, &app, &new, "true"));
        assert_eq!(which(&app), "new");
        assert_eq!(which(&dir.join("My Apps").join("fuxx.app.previous")), "old");
        assert!(!new.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_swap_replaces_a_leftover_previous_app() {
        let dir = swap_dir("leftover");
        let app = dir.join("fuxx.app");
        let previous = dir.join("fuxx.app.previous");
        let new = dir.join("new").join("fuxx.app");
        fake_app(&app, "old");
        fake_app(&previous, "older");
        fake_app(&new, "new");
        assert!(run_swap(&dir, &app, &new, "true"));
        assert_eq!(which(&app), "new");
        // The running app took its place (not moved inside it).
        assert_eq!(which(&previous), "old");
        assert!(!previous.join("fuxx.app").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_failed_move_puts_the_previous_app_back() {
        let dir = swap_dir("fails");
        let app = dir.join("My Apps").join("fuxx.app");
        let new = dir.join("cache it").join("fuxx.app"); // never made: the move fails
        fake_app(&app, "old");
        assert!(!run_swap(&dir, &app, &new, "true"));
        assert_eq!(which(&app), "old");
        assert!(!dir.join("My Apps").join("fuxx.app.previous").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_swap_opens_the_app_either_way() {
        use std::os::unix::fs::PermissionsExt;
        for (name, make_new) in [("opens new", true), ("opens old", false)] {
            let dir = swap_dir(name);
            let app = dir.join("fuxx.app");
            let new = dir.join("new").join("fuxx.app");
            fake_app(&app, "old");
            if make_new {
                fake_app(&new, "new");
            }
            // An opener that writes down what it was asked to open.
            let opened = dir.join("opened");
            let opener = dir.join("open it.sh");
            std::fs::write(
                &opener,
                format!("#!/bin/sh\nprintf %s \"$1\" > {}\n", sh_quote(&opened)),
            )
            .unwrap();
            std::fs::set_permissions(&opener, std::fs::Permissions::from_mode(0o755)).unwrap();
            assert_eq!(run_swap(&dir, &app, &new, &sh_quote(&opener)), make_new);
            assert_eq!(
                std::fs::read_to_string(&opened).unwrap(),
                app.to_string_lossy(),
                "{name}"
            );
            assert_eq!(which(&app), if make_new { "new" } else { "old" });
            std::fs::remove_dir_all(&dir).unwrap();
        }
    }

    /// Run only the part of the real swap script after fuxx has quit and the old app was renamed
    /// to `.previous`: its variables, then from the "Move the new app in" step on. For states the
    /// full script can't reach on its own (something at `app` again before the move).
    fn run_move_in(app: &std::path::Path, new: &std::path::Path, opener: &str) -> bool {
        let text = swap_script(gone_pid(), app, new, opener);
        let (head, _) = text.split_once("\npolls=0\n").unwrap();
        let at = text.find("# Move the new app in").unwrap();
        let script = format!("{head}\n{}", &text[at..]);
        std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(script)
            .status()
            .unwrap()
            .success()
    }

    /// An opener script in `dir` that writes down what it was asked to open (to `opened`).
    fn recording_opener(dir: &std::path::Path) -> String {
        use std::os::unix::fs::PermissionsExt;
        let opener = dir.join("open it.sh");
        std::fs::write(
            &opener,
            format!(
                "#!/bin/sh\nprintf %s \"$1\" > {}\n",
                sh_quote(&dir.join("opened"))
            ),
        )
        .unwrap();
        std::fs::set_permissions(&opener, std::fs::Permissions::from_mode(0o755)).unwrap();
        sh_quote(&opener)
    }

    #[test]
    fn something_at_the_apps_place_before_the_move_is_a_failed_move() {
        let dir = swap_dir("reappeared");
        let app = dir.join("My Apps").join("fuxx.app");
        let new = dir.join("cache it").join("fuxx.app");
        fake_app(&dir.join("My Apps").join("fuxx.app.previous"), "old");
        fake_app(&app, "intruder");
        fake_app(&new, "new");
        let opener = recording_opener(&dir);
        assert!(!run_move_in(&app, &new, &opener));
        // The new app wasn't moved inside it: the previous app is back, the intruder aside.
        assert_eq!(which(&app), "old");
        assert!(!app.join("fuxx.app").exists());
        assert_eq!(which(&new), "new");
        assert_eq!(
            which(&dir.join("My Apps").join("fuxx.app.incomplete")),
            "intruder"
        );
        assert!(!dir.join("My Apps").join("fuxx.app.previous").exists());
        assert_eq!(
            std::fs::read_to_string(dir.join("opened")).unwrap(),
            app.to_string_lossy()
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_partial_app_that_cant_be_moved_aside_is_never_opened() {
        let dir = swap_dir("stuck");
        let apps = dir.join("My Apps");
        let app = apps.join("fuxx.app");
        let new = dir.join("cache it").join("fuxx.app");
        let incomplete = apps.join("fuxx.app.incomplete");
        fake_app(&apps.join("fuxx.app.previous"), "old");
        fake_app(&app, "partial");
        fake_app(&new, "new");
        // An immutable file where the partial copy would go: it can't be removed or replaced.
        std::fs::write(&incomplete, "locked").unwrap();
        /// Sets `uchg` on its file, and clears it when dropped (even if the test panics), so the
        /// folder can always be removed.
        struct Locked(std::path::PathBuf);
        fn chflags(flag: &str, path: &std::path::Path) -> bool {
            std::process::Command::new("/usr/bin/chflags")
                .arg(flag)
                .arg(path)
                .status()
                .is_ok_and(|s| s.success())
        }
        impl Drop for Locked {
            fn drop(&mut self) {
                chflags("nouchg", &self.0);
            }
        }
        assert!(chflags("uchg", &incomplete));
        let locked = Locked(incomplete);
        let opener = recording_opener(&dir);
        let swapped = run_move_in(&app, &new, &opener);
        drop(locked);
        assert!(!swapped);
        // Nothing opened (not the partial copy), and the good previous app stays beside it.
        assert!(!dir.join("opened").exists());
        assert_eq!(which(&apps.join("fuxx.app.previous")), "old");
        assert_eq!(which(&app), "partial");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn quotes_paths_for_the_shell() {
        use std::path::Path;
        assert_eq!(sh_quote(Path::new("/a/b")), "'/a/b'");
        assert_eq!(sh_quote(Path::new("/My Apps/it's")), "'/My Apps/it'\\''s'");
    }

    #[test]
    fn check_copy() {
        assert_eq!(
            up_to_date_title(Version(0, 3, 0)),
            "fuxx 0.3.0 is the newest version."
        );
        assert_eq!(CHECK_FAILED, "Couldn't check for updates");
    }
}
