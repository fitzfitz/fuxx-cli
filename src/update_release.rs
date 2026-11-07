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
            let sha256 = a
                .get("digest")
                .and_then(|d| d.as_str())
                .and_then(|d| d.strip_prefix("sha256:"))
                .map(|h| h.to_ascii_lowercase());
            Some(Asset { url, sha256 })
        });
    Ok(Release {
        version,
        page_url: text("html_url"),
        notes: text("body"),
        zip,
    })
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
            {"name": "fuxx-0.3.1.zip", "browser_download_url": "https://x/fuxx-0.3.1.zip", "digest": "sha256:ABC123"}
        ]
    }"#;

    #[test]
    fn parses_versions() {
        assert_eq!(parse_version("0.3.0"), Some(Version(0, 3, 0)));
        assert_eq!(parse_version("v0.3.0"), Some(Version(0, 3, 0)));
        for bad in ["v1.2", "1.2.3-beta", "", "x.y.z"] {
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
        assert_eq!(zip.sha256, Some("abc123".to_string()));
    }

    #[test]
    fn no_matching_asset_or_digest() {
        let no_zip = SAMPLE.replace("fuxx-0.3.1.zip", "other.zip");
        assert_eq!(parse_latest(&no_zip).unwrap().zip, None);
        let no_digest = SAMPLE.replace(r#", "digest": "sha256:ABC123""#, "");
        assert_eq!(parse_latest(&no_digest).unwrap().zip.unwrap().sha256, None);
        let null_digest = SAMPLE.replace(r#""sha256:ABC123""#, "null");
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
}
