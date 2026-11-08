//! App-wide popups: permission requests, questions and plans from any session, and
//! agent CLI updates. They stack in the corner so the person can answer a session
//! without opening it.

/// The npm package each agent CLI ships as, for looking up its latest release.
pub(crate) fn npm_package(provider: &str) -> Option<&'static str> {
    crate::registry::spec(provider)?.package.as_deref()
}

/// The numeric parts of a version such as "v2.1.296 (Claude Code)" or "codex-cli 0.50.1".
pub(crate) fn version_parts(v: &str) -> Vec<u64> {
    let start = v.find(|c: char| c.is_ascii_digit()).unwrap_or(v.len());
    v[start..]
        .split(|c: char| !c.is_ascii_digit() && c != '.')
        .next()
        .unwrap_or("")
        .split('.')
        .filter_map(|p| p.parse().ok())
        .collect()
}

/// Whether `latest` is a newer release than `current`.
pub(crate) fn is_newer(latest: &str, current: &str) -> bool {
    let (l, c) = (version_parts(latest), version_parts(current));
    !l.is_empty() && !c.is_empty() && l > c
}
