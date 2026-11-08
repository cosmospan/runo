//! Agent turns: queued prompts, per-turn checkpoints with Undo / Keep, image attachments,
//! @file mentions, plans, questions and permission requests.

pub(crate) fn is_image(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            matches!(
                e.to_ascii_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp"
            )
        })
        .unwrap_or(false)
}

/// Add `/.runo/` to the repository's `info/exclude`, once.
pub(crate) fn exclude_runo_folder(dir: &str) {
    let Ok(out) = std::process::Command::new("git")
        .args(["-C", dir, "rev-parse", "--git-path", "info/exclude"])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
    else {
        return;
    };
    if !out.status.success() {
        return;
    }
    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let path = std::path::Path::new(dir).join(path);
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    if text.lines().any(|l| l.trim() == "/.runo/") {
        return;
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let sep = if text.is_empty() || text.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    let _ = std::fs::write(&path, format!("{text}{sep}/.runo/\n"));
}
