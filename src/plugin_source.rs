//! Where a plugin comes from: GitHub links, GitHub's JSON answers, and what to download.
//! No network here; the install code fetches and passes the text in.

/// Shown when a pasted link is not one fuxx understands.
pub const LINK_HINT: &str = "Paste a GitHub repo link, like github.com/owner/repo.";

/// A GitHub repository.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Repo {
    pub owner: String,
    pub name: String,
}

impl Repo {
    /// `owner/name`.
    pub fn slug(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }
    /// The repo's web page.
    pub fn page(&self) -> String {
        format!("https://github.com/{}", self.slug())
    }
    /// GitHub's "latest release" answer for this repo.
    pub fn release_api(&self) -> String {
        format!(
            "https://api.github.com/repos/{}/releases/latest",
            self.slug()
        )
    }
    /// GitHub's answer for the newest commit, used when there is no release.
    pub fn head_api(&self) -> String {
        format!("https://api.github.com/repos/{}/commits/HEAD", self.slug())
    }
}

/// True for a non-empty segment of letters, digits, `.`, `_` and `-` that is not `.` or `..`.
fn valid_segment(s: &str) -> bool {
    !s.is_empty()
        && s != "."
        && s != ".."
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// Reads a pasted GitHub repo link: `https://github.com/o/r`, `http://…`, `github.com/o/r`,
/// `www.github.com/o/r` or `o/r`. A trailing `/` or `.git` and surrounding spaces are ignored.
/// Anything else (other hosts, deeper paths, ssh links) is None.
pub fn parse_link(text: &str) -> Option<Repo> {
    let t = text.trim();
    let t = t
        .strip_prefix("https://")
        .or_else(|| t.strip_prefix("http://"))
        .unwrap_or(t);
    let t = t.strip_prefix("www.").unwrap_or(t);
    let rest = t.strip_prefix("github.com/").unwrap_or(t);
    let rest = rest.strip_suffix('/').unwrap_or(rest);
    let rest = rest.strip_suffix(".git").unwrap_or(rest);
    let (owner, name) = rest.split_once('/')?;
    (valid_segment(owner) && valid_segment(name)).then(|| Repo {
        owner: owner.to_string(),
        name: name.to_string(),
    })
}

/// What version of a plugin is on disk: a release tag, or a commit when the repo has no release.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Installed {
    Tag(String),
    Commit(String),
}

impl Installed {
    /// The tag, or the first 7 characters of the commit.
    pub fn label(&self) -> String {
        match self {
            Installed::Tag(t) => t.clone(),
            Installed::Commit(c) => c.chars().take(7).collect(),
        }
    }
}

/// How a download is packed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ArchiveKind {
    Zip,
    Tarball,
}

/// One thing to download, and what it will count as once installed.
#[derive(Clone, Debug, PartialEq)]
pub struct Download {
    pub url: String,
    pub kind: ArchiveKind,
    /// The checksum GitHub published for it, when it did.
    pub sha256: Option<String>,
    pub installed: Installed,
}

/// One file attached to a release.
#[derive(Clone, Debug, PartialEq)]
pub struct ReleaseAsset {
    pub name: String,
    pub url: String,
    pub sha256: Option<String>,
}

/// GitHub's latest release, reduced to what the installer needs.
#[derive(Clone, Debug, PartialEq)]
pub struct LatestRelease {
    pub tag: String,
    pub tarball_url: String,
    pub assets: Vec<ReleaseAsset>,
}

/// The hex of a `sha256:` digest, lowercase, when it is exactly 64 hex digits; else None.
fn sha256_of(digest: &str) -> Option<String> {
    let hex = digest.strip_prefix("sha256:")?;
    (hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| hex.to_ascii_lowercase())
}

/// The message of a GitHub error body (`{"message": …}`), or `fallback`.
fn error_of(v: &serde_json::Value, fallback: &str) -> String {
    match v.get("message").and_then(|m| m.as_str()) {
        Some(m) => m.to_string(),
        None => fallback.to_string(),
    }
}

/// Reads GitHub's "latest release" JSON. A GitHub error answer becomes Err carrying its message.
pub fn parse_release(json: &str) -> Result<LatestRelease, String> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("invalid release JSON: {e}"))?;
    let Some(tag) = v.get("tag_name").and_then(|t| t.as_str()) else {
        return Err(error_of(&v, "release JSON has no tag_name"));
    };
    let tarball_url = v
        .get("tarball_url")
        .and_then(|t| t.as_str())
        .ok_or("release JSON has no tarball_url")?
        .to_string();
    let assets = v
        .get("assets")
        .and_then(|a| a.as_array())
        .into_iter()
        .flatten()
        .filter_map(|a| {
            Some(ReleaseAsset {
                name: a.get("name")?.as_str()?.to_string(),
                url: a.get("browser_download_url")?.as_str()?.to_string(),
                sha256: a.get("digest").and_then(|d| d.as_str()).and_then(sha256_of),
            })
        })
        .collect();
    Ok(LatestRelease {
        tag: tag.to_string(),
        tarball_url,
        assets,
    })
}

/// Reads GitHub's newest-commit JSON and returns its `sha`.
pub fn parse_head(json: &str) -> Result<String, String> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("invalid commit JSON: {e}"))?;
    match v.get("sha").and_then(|s| s.as_str()) {
        Some(sha) => Ok(sha.to_string()),
        None => Err(error_of(&v, "commit JSON has no sha")),
    }
}

/// The name this Mac's chip goes by in asset names.
pub fn chip() -> &'static str {
    if std::env::consts::ARCH == "aarch64" {
        "arm64"
    } else {
        "x86_64"
    }
}

/// Picks what to download from a release: the zip for this chip, else the universal zip,
/// else the release's source tarball.
pub fn from_release(r: &LatestRelease, chip: &str) -> Download {
    let installed = Installed::Tag(r.tag.clone());
    let chip_name = format!("fuxx-plugin-macos-{chip}.zip");
    let find = |name: &str| r.assets.iter().find(|a| a.name == name);
    match find(&chip_name).or_else(|| find("fuxx-plugin-macos.zip")) {
        Some(a) => Download {
            url: a.url.clone(),
            kind: ArchiveKind::Zip,
            sha256: a.sha256.clone(),
            installed,
        },
        None => Download {
            url: r.tarball_url.clone(),
            kind: ArchiveKind::Tarball,
            sha256: None,
            installed,
        },
    }
}

/// The source tarball of one commit, for a repo with no release.
pub fn from_head(repo: &Repo, sha: &str) -> Download {
    Download {
        url: format!("https://api.github.com/repos/{}/tarball/{sha}", repo.slug()),
        kind: ArchiveKind::Tarball,
        sha256: None,
        installed: Installed::Commit(sha.to_string()),
    }
}

/// True when the latest differs from what is installed. Any difference counts, so a repo that
/// moves from commits to releases is offered the release.
pub fn is_newer(installed: &Installed, latest: &Installed) -> bool {
    installed != latest
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str, sha: Option<&str>) -> ReleaseAsset {
        ReleaseAsset {
            name: name.into(),
            url: format!("https://example.com/{name}"),
            sha256: sha.map(String::from),
        }
    }
    fn release(assets: Vec<ReleaseAsset>) -> LatestRelease {
        LatestRelease {
            tag: "v1.2".into(),
            tarball_url: "https://example.com/tarball".into(),
            assets,
        }
    }

    #[test]
    fn links() {
        let r = Repo {
            owner: "fitzfitz".into(),
            name: "sunset".into(),
        };
        for ok in [
            "https://github.com/fitzfitz/sunset",
            "http://github.com/fitzfitz/sunset/",
            " github.com/fitzfitz/sunset.git ",
            "www.github.com/fitzfitz/sunset",
            "fitzfitz/sunset",
        ] {
            assert_eq!(parse_link(ok), Some(r.clone()), "{ok}");
        }
        for bad in [
            "",
            "sunset",
            "github.com/fitzfitz",
            "github.com/o/r/tree/main",
            "git@github.com:o/r.git",
            "https://gist.github.com/o/abc",
            "https://gitlab.com/o/r",
            "o/r/x",
            "../x/y",
        ] {
            assert_eq!(parse_link(bad), None, "{bad}");
        }
    }

    #[test]
    fn repo_urls() {
        let r = Repo {
            owner: "o".into(),
            name: "r".into(),
        };
        assert_eq!(r.slug(), "o/r");
        assert_eq!(r.page(), "https://github.com/o/r");
        assert_eq!(
            r.release_api(),
            "https://api.github.com/repos/o/r/releases/latest"
        );
        assert_eq!(
            r.head_api(),
            "https://api.github.com/repos/o/r/commits/HEAD"
        );
    }

    #[test]
    fn picks_the_asset_for_this_chip() {
        let hex_a = "a".repeat(64);
        let r = release(vec![
            asset("fuxx-plugin-macos-arm64.zip", Some(&hex_a)),
            asset("fuxx-plugin-macos-x86_64.zip", None),
            asset("fuxx-plugin-macos.zip", None),
        ]);
        let d = from_release(&r, "arm64");
        assert_eq!(d.url, "https://example.com/fuxx-plugin-macos-arm64.zip");
        assert_eq!(d.kind, ArchiveKind::Zip);
        assert_eq!(d.sha256, Some(hex_a));
        assert_eq!(d.installed, Installed::Tag("v1.2".into()));
        let d = from_release(&r, "x86_64");
        assert_eq!(d.url, "https://example.com/fuxx-plugin-macos-x86_64.zip");
        let only_universal = release(vec![
            asset("fuxx-plugin-macos.zip", None),
            asset("other.zip", None),
        ]);
        let d = from_release(&only_universal, "arm64");
        assert_eq!(d.url, "https://example.com/fuxx-plugin-macos.zip");
        assert_eq!(d.kind, ArchiveKind::Zip);
        let none = release(vec![]);
        let d = from_release(&none, "arm64");
        assert_eq!(d.url, "https://example.com/tarball");
        assert_eq!(d.kind, ArchiveKind::Tarball);
        assert_eq!(d.sha256, None);
        assert_eq!(d.installed, Installed::Tag("v1.2".into()));
    }

    #[test]
    fn head_is_a_commit_tarball() {
        let d = from_head(
            &Repo {
                owner: "o".into(),
                name: "r".into(),
            },
            "abc1234def",
        );
        assert_eq!(d.url, "https://api.github.com/repos/o/r/tarball/abc1234def");
        assert_eq!(d.kind, ArchiveKind::Tarball);
        assert_eq!(d.sha256, None);
        assert_eq!(d.installed, Installed::Commit("abc1234def".into()));
        assert_eq!(d.installed.label(), "abc1234");
        assert_eq!(Installed::Tag("v1".into()).label(), "v1");
    }

    #[test]
    fn parses_githubs_json() {
        let hex = "ab".repeat(32);
        let body = format!(
            r#"{{"tag_name":"v2.0","tarball_url":"https://api.github.com/repos/o/r/tarball/v2.0",
            "assets":[{{"name":"fuxx-plugin-macos.zip","browser_download_url":"https://github.com/o/r/releases/download/v2.0/fuxx-plugin-macos.zip","digest":"sha256:{hex}"}},
            {{"name":"notes.txt","browser_download_url":"https://github.com/o/r/x"}}]}}"#
        );
        let r = parse_release(&body).unwrap();
        assert_eq!(r.tag, "v2.0");
        assert_eq!(
            r.tarball_url,
            "https://api.github.com/repos/o/r/tarball/v2.0"
        );
        assert_eq!(r.assets.len(), 2);
        assert_eq!(r.assets[0].name, "fuxx-plugin-macos.zip");
        assert_eq!(r.assets[0].sha256, Some(hex));
        assert_eq!(r.assets[1].sha256, None);
        assert_eq!(
            parse_head(r#"{"sha":"abc123","commit":{}}"#),
            Ok("abc123".to_string())
        );
        assert_eq!(
            parse_release(r#"{"message":"Not Found"}"#),
            Err("Not Found".to_string())
        );
        assert_eq!(
            parse_head(r#"{"message":"Not Found"}"#),
            Err("Not Found".to_string())
        );
        assert!(parse_release("nope").is_err());
        assert!(parse_head("{}").is_err());
    }

    #[test]
    fn newer_means_different() {
        assert!(is_newer(
            &Installed::Tag("v1".into()),
            &Installed::Tag("v2".into())
        ));
        assert!(!is_newer(
            &Installed::Tag("v1".into()),
            &Installed::Tag("v1".into())
        ));
        assert!(is_newer(
            &Installed::Commit("a".into()),
            &Installed::Tag("v1".into())
        ));
    }

    #[test]
    fn installed_round_trips() {
        let t = Installed::Tag("v1".into());
        assert_eq!(serde_json::to_string(&t).unwrap(), r#"{"tag":"v1"}"#);
        let c = Installed::Commit("abc".into());
        assert_eq!(serde_json::to_string(&c).unwrap(), r#"{"commit":"abc"}"#);
        for i in [t, c] {
            let back: Installed =
                serde_json::from_str(&serde_json::to_string(&i).unwrap()).unwrap();
            assert_eq!(back, i);
        }
    }

    #[test]
    fn chip_is_known() {
        assert!(["arm64", "x86_64"].contains(&chip()));
    }
}
