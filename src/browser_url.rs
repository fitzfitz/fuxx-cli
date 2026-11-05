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

#[cfg(test)]
mod tests {
    use super::*;

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
