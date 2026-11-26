//! M10c: the one place an address typed for the agent browser becomes a URL. Only `http` and
//! `https` get through. No GPUI, no WebKit.

/// The refusal for every other scheme.
const ONLY_HTTP: &str = "only http and https addresses can be opened";

/// Turn what was typed into an `http(s)` URL, or say why not. A bare host gets `http://` when
/// it is `localhost`, `*.localhost`, an IPv4 or a bracketed IPv6 address, else `https://`.
/// Only the scheme is lowercased; a path, query and fragment stay as given. `about:blank` is
/// the one internal address that is let through.
pub fn normalize_url(input: &str) -> Result<String, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("empty address".to_string());
    }
    if input.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("an address can't contain spaces or control characters".to_string());
    }
    if input.eq_ignore_ascii_case("about:blank") {
        return Ok("about:blank".to_string());
    }
    // Only a leading `scheme://` is one: a `://` after a `/`, `?` or `#` is in the path.
    if let Some((scheme, rest)) = input.split_once("://")
        && !scheme.contains(['/', '?', '#'])
    {
        let scheme = scheme.to_ascii_lowercase();
        return if (scheme == "http" || scheme == "https") && !rest.is_empty() {
            Ok(format!("{scheme}://{rest}"))
        } else {
            Err(ONLY_HTTP.to_string())
        };
    }
    // No `://`: a scheme-like prefix (`javascript:`, `about:`, `data:`) is refused, but
    // `host:port` is a bare host. A port is digits, at least one: `javascript:` alone is no
    // host with an empty port.
    let authority = input.split(['/', '?', '#']).next().unwrap_or("");
    if let Some((before, after)) = authority.split_once(':')
        && !before.is_empty()
        && !before.starts_with('[')
        && (after.is_empty() || !after.chars().all(|c| c.is_ascii_digit()))
    {
        return Err(ONLY_HTTP.to_string());
    }
    if authority.is_empty() {
        return Err("an address needs a host".to_string());
    }
    let scheme = if is_local(host_of(authority)) {
        "http"
    } else {
        "https"
    };
    Ok(format!("{scheme}://{input}"))
}

/// The host part of `host[:port]`, brackets kept for IPv6, any `user@` dropped.
fn host_of(authority: &str) -> &str {
    let authority = authority.rsplit('@').next().unwrap_or(authority);
    if authority.starts_with('[') {
        return match authority.find(']') {
            Some(end) => &authority[..=end],
            None => authority,
        };
    }
    authority.split(':').next().unwrap_or(authority)
}

/// `localhost`, `*.localhost`, an IPv4 address or a bracketed IPv6 address.
fn is_local(host: &str) -> bool {
    let lower = host.to_ascii_lowercase();
    if lower == "localhost" || lower.ends_with(".localhost") {
        return true;
    }
    if lower.starts_with('[') && lower.ends_with(']') {
        return true;
    }
    let parts: Vec<&str> = lower.split('.').collect();
    parts.len() == 4
        && parts.iter().all(|p| {
            !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()) && p.parse::<u8>().is_ok()
        })
}

/// `text` with every byte but ASCII letters, digits, `-_.~` and `keep` written `%XX`.
pub fn percent_encode(text: &str, keep: &[u8]) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') || keep.contains(&b)
        {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Whether `name` ends in `.html` or `.htm` (any case): a page fuxx's browser shows (chore 35).
pub fn is_page_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".html") || lower.ends_with(".htm")
}

/// Chore 35: the `file://` address of the page at absolute `path`; none for a relative path or
/// a file that isn't a page (`is_page_name`).
pub fn page_url(path: &std::path::Path) -> Option<String> {
    let text = path.to_str()?;
    (path.is_absolute() && is_page_name(text))
        .then(|| format!("file://{}", percent_encode(text, b"/")))
}

/// Chore 35: whether `url` is a local page's address, `file://` and a path (before any `?` or
/// `#`) that `is_page_name`. The one `file://` a program's own opening may show in fuxx.
pub fn is_local_page(url: &str) -> bool {
    let Some(scheme) = url.get(..7) else {
        return false;
    };
    let path = url[7..].split(['?', '#']).next().unwrap_or("");
    scheme.eq_ignore_ascii_case("file://") && is_page_name(path)
}

/// Whether a browser page may go to `url` (the navigation policy, after the chores 33–35
/// review): anywhere but a `file://` that isn't a page (`is_local_page`), so a local page's tab
/// can't be steered (a link, a script, `browser.eval`) to show another file on disk.
pub fn may_navigate(url: &str) -> bool {
    let file = url
        .get(..5)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("file:"));
    !file || is_local_page(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Chore 35: a page on disk, made a `file://` address (as a person or a program's `open`
    /// names it): every byte but the safe ones and `/` written `%XX`.
    #[test]
    fn a_page_on_disk_becomes_its_file_address() {
        use std::path::Path;
        assert_eq!(percent_encode("a b/c~é", b"/"), "a%20b/c~%C3%A9");
        assert_eq!(
            page_url(Path::new("/tmp/my app/caf\u{e9} #1.html")).as_deref(),
            Some("file:///tmp/my%20app/caf%C3%A9%20%231.html")
        );
        assert_eq!(
            page_url(Path::new("/x/my-app/INDEX.HTM")).as_deref(),
            Some("file:///x/my-app/INDEX.HTM")
        );
        for no in [
            "my-app/index.html",
            "/x/report.pdf",
            "/x/my-app",
            "/x/page.html.txt",
        ] {
            assert_eq!(page_url(Path::new(no)), None, "{no}");
        }
    }

    /// The review's finding (chores 33–35): a page may go anywhere but to a file that isn't a
    /// page, so a local page's tab can't be steered to read other files.
    #[test]
    fn a_page_may_go_to_no_file_but_a_page() {
        for yes in [
            "https://example.com/a",
            "about:blank",
            "file:///x/my-app/index.html",
            "FILE:///x/my-app/about.HTM#team",
        ] {
            assert!(may_navigate(yes), "{yes}");
        }
        for no in [
            "file:///Users/me/.ssh/id_ed25519",
            "FILE:///etc/hosts",
            "file:///x/my-app/",
            "file:///x/report.pdf?a=.html",
        ] {
            assert!(!may_navigate(no), "{no}");
        }
    }

    #[test]
    fn a_local_page_is_a_file_address_to_an_html_file() {
        for yes in [
            "file:///x/page.html",
            "FILE:///x/Page.HTM",
            "file:///x/page.html?a=1#b",
            "file:///x/my%20app/page.htm#top",
        ] {
            assert!(is_local_page(yes), "{yes}");
        }
        for no in [
            "file:///etc/hosts",
            "file:///x/report.pdf",
            "file:///x/",
            "file://",
            "http://x/page.html",
            "/x/page.html",
            "file:///x/page.html.pdf",
            "file:///x/report.pdf?x=.html",
        ] {
            assert!(!is_local_page(no), "{no}");
        }
    }

    #[test]
    fn localhost_and_ips_get_http() {
        assert_eq!(
            normalize_url("localhost:3000").unwrap(),
            "http://localhost:3000"
        );
        assert_eq!(normalize_url("LocalHost").unwrap(), "http://LocalHost");
        assert_eq!(
            normalize_url("127.0.0.1:8080/x").unwrap(),
            "http://127.0.0.1:8080/x"
        );
        assert_eq!(normalize_url("[::1]:5173").unwrap(), "http://[::1]:5173");
        assert_eq!(
            normalize_url("app.localhost").unwrap(),
            "http://app.localhost"
        );
    }

    #[test]
    fn other_hosts_get_https() {
        assert_eq!(normalize_url("example.com").unwrap(), "https://example.com");
        assert_eq!(
            normalize_url("docs.rs/objc2").unwrap(),
            "https://docs.rs/objc2"
        );
        // Not an IPv4 address: a number over 255, or too few parts.
        assert_eq!(normalize_url("256.1.1.1").unwrap(), "https://256.1.1.1");
        assert_eq!(normalize_url("1.2.3").unwrap(), "https://1.2.3");
        assert_eq!(
            normalize_url("notlocalhost.com").unwrap(),
            "https://notlocalhost.com"
        );
    }

    #[test]
    fn explicit_schemes_are_kept() {
        assert_eq!(
            normalize_url("http://example.com/A?b=C#D").unwrap(),
            "http://example.com/A?b=C#D"
        );
        assert_eq!(
            normalize_url("HTTPS://Example.com").unwrap(),
            "https://Example.com"
        );
        assert_eq!(normalize_url("  https://a.b  ").unwrap(), "https://a.b");
        assert_eq!(normalize_url("about:blank").unwrap(), "about:blank");
    }

    #[test]
    fn other_schemes_are_refused() {
        for bad in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "data:text/html,x",
            "ftp://h",
            "about:config",
        ] {
            assert_eq!(
                normalize_url(bad).unwrap_err(),
                "only http and https addresses can be opened",
                "{bad}"
            );
        }
    }

    #[test]
    fn a_scheme_with_nothing_after_it_is_refused() {
        // Read as a bare host, these would be `https://javascript:` and so on.
        for bad in ["javascript:", "about:", "data:", "http:/x", "localhost:"] {
            assert_eq!(
                normalize_url(bad).unwrap_err(),
                "only http and https addresses can be opened",
                "{bad}"
            );
        }
    }

    #[test]
    fn only_a_leading_scheme_counts_as_one() {
        // A `://` in the path or query isn't the address's scheme.
        assert_eq!(
            normalize_url("example.com/go?to=http://x").unwrap(),
            "https://example.com/go?to=http://x"
        );
        assert_eq!(
            normalize_url("localhost:3000/#/http://x").unwrap(),
            "http://localhost:3000/#/http://x"
        );
        // Nor does it smuggle another scheme through.
        assert_eq!(
            normalize_url("javascript:/x?http://y").unwrap_err(),
            "only http and https addresses can be opened"
        );
    }

    #[test]
    fn the_named_risks() {
        for bad in [
            "JaVaScRiPt:alert(1)",
            "JAVASCRIPT://x",
            "https:\\\\x",
            "http:/x",
        ] {
            assert_eq!(
                normalize_url(bad).unwrap_err(),
                "only http and https addresses can be opened",
                "{bad}"
            );
        }
        // No host at all.
        assert_eq!(
            normalize_url("//host").unwrap_err(),
            "an address needs a host"
        );
        assert_eq!(normalize_url("/x").unwrap_err(), "an address needs a host");
        // A leading tab is trimmed like a space: what follows decides.
        assert_eq!(normalize_url("\thttps://a.b").unwrap(), "https://a.b");
        assert_eq!(
            normalize_url("\tjavascript:alert(1)").unwrap_err(),
            "only http and https addresses can be opened"
        );
        // Credentials stay (the spec keeps them); WebKit shows the real host, `evil`.
        assert_eq!(normalize_url("http://a@evil").unwrap(), "http://a@evil");
    }

    #[test]
    fn blank_and_spaces_are_refused() {
        assert!(normalize_url("").is_err());
        assert!(normalize_url("   ").is_err());
        assert!(normalize_url("exa mple.com").is_err());
        assert!(normalize_url("http://a.b/x y").is_err());
        assert!(normalize_url("a.b\u{7}").is_err());
    }
}
