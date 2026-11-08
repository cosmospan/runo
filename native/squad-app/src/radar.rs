//! Clash radar: every agent session's work, the project folder's own and the main
//! branch, merged pair by pair in memory to find where two lines of work change the same
//! lines, long before anyone merges. Nothing in any folder is touched; uncommitted work
//! counts too.

use super::*;

/// "around line 40", "around lines 40 and 88", "around lines 4, 40 and 88".
pub(crate) fn around(lines: &[usize]) -> String {
    let list: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
    match list.as_slice() {
        [] => String::new(),
        [one] => format!("around line {one}"),
        [rest @ .., last] => format!("around lines {} and {last}", rest.join(", ")),
    }
}

/// "webhook.rs", "webhook.rs and retry.rs", "webhook.rs and 2 more files".
pub(crate) fn file_names(files: &[git::Overlap]) -> String {
    let name = |f: &git::Overlap| f.path.rsplit('/').next().unwrap_or(&f.path).to_string();
    match files {
        [] => String::new(),
        [one] => name(one),
        [a, b] => format!("{} and {}", name(a), name(b)),
        [a, rest @ ..] => format!("{} and {} more files", name(a), rest.len()),
    }
}
