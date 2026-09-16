//! Rewrites Content-Security-Policy so the injected page bridge is allowed to run.
//!
//! The preview only works if the page can execute our script and POST capture
//! data back to the proxy origin. A strict CSP on the target app would block
//! both, so we relax exactly the directives the bridge needs and leave the rest
//! of the policy alone.

/// The path our bridge is served from, on the proxy origin.
pub const BRIDGE_PATH: &str = "/cascade-browser-integration.js";

/// Minimum additions the bridge needs.
const NEEDED: &[(&str, &str)] = &[
    ("script-src", "'self' 'unsafe-inline' 'unsafe-eval'"),
    ("connect-src", "'self'"),
];

/// Rewrite a single CSP header value.
///
/// For each directive we need: if it is absent, append it; if present, append
/// our sources to the existing list rather than replacing it, so the target
/// app's own policy keeps working.
pub fn rewrite_policy(value: &str) -> String {
    let mut parts: Vec<String> = value.split(';').map(|s| s.trim().to_string()).collect();
    parts.retain(|p| !p.is_empty());

    for (directive, additions) in NEEDED {
        let existing = parts.iter_mut().find(|p| {
            p.split_whitespace()
                .next()
                .map(|d| d.eq_ignore_ascii_case(directive))
                .unwrap_or(false)
        });

        match existing {
            Some(p) => {
                // skip if already permissive enough
                if additions.split_whitespace().all(|a| p.contains(a)) {
                    continue;
                }
                let extra = additions
                    .split_whitespace()
                    .filter(|a| !p.contains(a))
                    .collect::<Vec<_>>()
                    .join(" ");
                if !extra.is_empty() {
                    p.push(' ');
                    p.push_str(&extra);
                }
            }
            None => parts.push(format!("{directive} {additions}")),
        }
    }

    parts.join("; ")
}

/// Insert the bridge script as the first thing in `<head>` so it runs before
/// app code, and expose the CSRF token for it to read.
#[allow(clippy::string_slice)] // Indices come from find() on ASCII "<head" / '>'; byte slicing is safe.
pub fn inject_bridge(html: &str, csrf: &str) -> String {
    let tag = format!(
        r#"<meta name="codeium-csrf-token" content="{csrf}"><script src="{BRIDGE_PATH}"></script>"#
    );
    // case-insensitive search for <head ...>
    let lower = html.to_ascii_lowercase();
    if let Some(start) = lower.find("<head") {
        if let Some(rel_end) = html[start..].find('>') {
            let at = start + rel_end + 1;
            let mut out = String::with_capacity(html.len() + tag.len());
            out.push_str(&html[..at]);
            out.push_str(&tag);
            out.push_str(&html[at..]);
            return out;
        }
    }
    // no parseable <head>: prepend so it still runs first
    format!("{tag}{html}")
}

/// Headers that must never be forwarded in either direction.
pub fn is_hop_by_hop(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailers"
            | "transfer-encoding"
            | "upgrade"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_missing_directives() {
        let out = rewrite_policy("default-src 'self'");
        assert!(out.contains("default-src 'self'"));
        assert!(out.contains("script-src 'self' 'unsafe-inline' 'unsafe-eval'"));
        assert!(out.contains("connect-src 'self'"));
    }

    #[test]
    fn preserves_existing_directive_sources() {
        let out = rewrite_policy("script-src 'self' https://cdn.example.com");
        // must keep the app's own CDN
        assert!(out.contains("https://cdn.example.com"));
        // and add what we need
        assert!(out.contains("'unsafe-eval'"));
    }

    #[test]
    fn does_not_duplicate_already_permissive_tokens() {
        let out = rewrite_policy("script-src 'self' 'unsafe-inline' 'unsafe-eval'");
        assert_eq!(out.matches("'unsafe-eval'").count(), 1);
    }

    #[test]
    fn hop_by_hop_detection_is_case_insensitive() {
        assert!(is_hop_by_hop("Transfer-Encoding"));
        assert!(!is_hop_by_hop("content-type"));
    }

    #[test]
    fn injects_after_head_open_tag() {
        let html = "<html><head><title>t</title></head><body></body></html>";
        let out = inject_bridge(html, "tok123");
        assert!(out.contains("codeium-csrf-token"));
        assert!(out.contains("tok123"));
        let head = out.find("<head").unwrap();
        let script = out.find("cascade-browser-integration.js").unwrap();
        let title = out.find("<title>").unwrap();
        assert!(head < script && script < title);
    }

    #[test]
    fn injects_without_head_tag() {
        let out = inject_bridge("<body>hi</body>", "tok");
        assert!(out.starts_with("<meta"));
        assert!(out.ends_with("<body>hi</body>"));
    }

    #[test]
    fn handles_uppercase_head() {
        let out = inject_bridge("<HTML><HEAD></HEAD></HTML>", "t");
        let script = out.find("cascade-browser-integration.js").unwrap();
        let head_end = out.to_ascii_lowercase().find("</head>").unwrap();
        assert!(script < head_end);
    }
}
