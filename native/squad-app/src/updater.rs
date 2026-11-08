//! How to update the exact agent CLI Squad Code runs, and what to tell the person when an
//! update can't or shouldn't run: another app owns the copy, npm needs admin rights,
//! a file is in the way, or the network is down.

use std::path::{Path, PathBuf};

/// The file Squad Code starts for an agent: its Settings path, or the first match on PATH.
pub(crate) fn resolve(bin: &str) -> Option<PathBuf> {
    if bin.contains('/') || bin.contains('\\') {
        let path = PathBuf::from(bin);
        return path.is_file().then_some(path);
    }
    let suffixes: &[&str] = if cfg!(windows) {
        &["", ".exe", ".cmd", ".bat"]
    } else {
        &[""]
    };
    std::env::split_paths(&std::env::var_os("PATH")?).find_map(|dir| {
        suffixes
            .iter()
            .map(|s| dir.join(format!("{bin}{s}")))
            .find(|p| p.is_file())
    })
}

fn slashes(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// The app that ships this copy of a CLI inside its own folder, like Orca.
pub(crate) fn owner_app(real: &Path) -> Option<String> {
    let s = slashes(real);
    let after = |marker: &str| {
        let (_, rest) = s.split_once(marker)?;
        rest.split('/').next().map(str::to_string)
    };
    let app = after("/Application Support/")
        .or_else(|| {
            s.split('/')
                .find(|seg| seg.ends_with(".app"))
                .map(|seg| seg.trim_end_matches(".app").to_string())
        })
        .or_else(|| after("/AppData/Local/Programs/"))?;
    // Squad Code itself and generic tool folders aren't "another app".
    let lower = app.to_lowercase();
    (!app.is_empty()
        && ![
            "squad code",
            "squadcode",
            "runo",
            "npm",
            "node",
            "nvm",
            "fnm",
            "volta",
        ]
        .contains(&lower.as_str()))
    .then(|| {
        let mut chars = app.chars();
        chars
            .next()
            .map(|c| c.to_uppercase().collect::<String>() + chars.as_str())
            .unwrap_or_default()
    })
}

/// The version a CLI reports, or "" when it doesn't run.
pub(crate) fn version_of(path: &Path) -> String {
    std::process::Command::new(path)
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}
