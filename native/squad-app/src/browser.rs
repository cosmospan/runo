//! The Browser tab: a live page from the headless Chromium in `browser_engine`, drawn as
//! an image and driven by forwarding the panel's mouse and keys. Before a page is open it
//! lists the dev servers running on this machine.

/// Turn what was typed into a URL: `localhost:3000` and `:5173` work too.
pub(crate) fn normalize_url(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() || raw.contains(char::is_whitespace) {
        return None;
    }
    if raw.starts_with("http://") || raw.starts_with("https://") {
        return Some(raw.to_string());
    }
    if let Some(port) = raw.strip_prefix(':') {
        return port
            .parse::<u16>()
            .ok()
            .map(|p| format!("http://localhost:{p}"));
    }
    if raw.parse::<u16>().is_ok() {
        return Some(format!("http://localhost:{raw}"));
    }
    let local =
        raw.starts_with("localhost") || raw.starts_with("127.") || raw.starts_with("0.0.0.0");
    if local {
        return Some(format!("http://{raw}"));
    }
    raw.contains('.').then(|| format!("https://{raw}"))
}
