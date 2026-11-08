//! Keeping an eye on what the person isn't watching: local turns picked up after a
//! restart, agents gone quiet, spending, turn and token limits, the cap on agents running at once, crash
//! reports and newer Runo releases. Also saves the state a moment after it changes.

/// A token count the way people read it: 950, 12k, 1.5M.
pub(crate) fn token_count(n: u64) -> String {
    match n {
        0..1_000 => n.to_string(),
        1_000..1_000_000 => format!("{}k", n / 1_000),
        _ => {
            let m = format!("{:.1}", n as f64 / 1e6);
            format!("{}M", m.strip_suffix(".0").unwrap_or(&m))
        }
    }
}
