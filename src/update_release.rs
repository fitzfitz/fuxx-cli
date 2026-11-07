// Releases and versions for the self-updater: the version order, and GitHub's "latest release" JSON.
// Pure: no network, no UI. The check that fetches the JSON is elsewhere.

use std::fmt;

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

    #[test]
    fn check_copy() {
        assert_eq!(
            up_to_date_title(Version(0, 3, 0)),
            "fuxx 0.3.0 is the newest version."
        );
        assert_eq!(CHECK_FAILED, "Couldn't check for updates");
    }
}
