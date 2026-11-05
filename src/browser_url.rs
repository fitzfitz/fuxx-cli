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
    if let Some((scheme, rest)) = input.split_once("://") {
        let scheme = scheme.to_ascii_lowercase();
        return if (scheme == "http" || scheme == "https") && !rest.is_empty() {
            Ok(format!("{scheme}://{rest}"))
        } else {
            Err(ONLY_HTTP.to_string())
        };
    }
    // No `://`: a scheme-like prefix (`javascript:`, `about:`, `data:`) is refused, but
    // `host:port` is a bare host. A scheme is letters, digits, `+`, `-`, `.` and not a port.
    let authority = input.split(['/', '?', '#']).next().unwrap_or("");
    if let Some((before, after)) = authority.split_once(':')
        && !before.is_empty()
        && !before.starts_with('[')
        && !after.chars().all(|c| c.is_ascii_digit())
    {
        return Err(ONLY_HTTP.to_string());
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
    fn blank_and_spaces_are_refused() {
        assert!(normalize_url("").is_err());
        assert!(normalize_url("   ").is_err());
        assert!(normalize_url("exa mple.com").is_err());
        assert!(normalize_url("http://a.b/x y").is_err());
        assert!(normalize_url("a.b\u{7}").is_err());
    }
}
