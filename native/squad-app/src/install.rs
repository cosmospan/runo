//! Installing agent CLIs from inside Squad Code, with each vendor's own installer or npm package.
//! The exact command is shown before anything runs.

use std::path::PathBuf;

/// Folders installers commonly use that a desktop app's PATH may not include.
pub(crate) fn extra_dirs() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_default();
    let mut dirs = vec![
        home.join(".local/bin"),
        home.join(".npm-global/bin"),
        home.join(".bun/bin"),
        home.join(".opencode/bin"),
        home.join(".factory/bin"),
        home.join(".grok/bin"),
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
    ];
    if let Some(appdata) = std::env::var_os("APPDATA") {
        dirs.push(PathBuf::from(appdata).join("npm"));
    }
    dirs
}

/// Where a CLI ended up: on PATH, or in a folder installers use.
pub(crate) fn locate(bin: &str) -> Option<PathBuf> {
    let suffixes: &[&str] = if cfg!(windows) {
        &["", ".exe", ".cmd", ".bat"]
    } else {
        &[""]
    };
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .chain(extra_dirs())
        .flat_map(|dir| suffixes.iter().map(move |s| dir.join(format!("{bin}{s}"))))
        .find(|p| p.is_file())
}

static NODE: std::sync::RwLock<Option<(String, PathBuf)>> = std::sync::RwLock::new(None);

/// Look for Node.js again; called with agent detection, off the window thread.
pub(crate) fn refresh_node() {
    let found = locate("node").and_then(|path| {
        let out = std::process::Command::new(&path)
            .arg("--version")
            .output()
            .ok()?;
        let version = String::from_utf8_lossy(&out.stdout).trim().to_string();
        Some((version, path))
    });
    if let Ok(mut node) = NODE.write() {
        *node = found;
    }
}
