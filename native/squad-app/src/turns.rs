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

/// Squad Code's folder inside a project: design references and the record of what it
/// added to agents' project files. Kept out of git.
pub(crate) const PROJECT_FOLDER: &str = ".squadcode";
/// The folder's name before the app was renamed.
const OLD_PROJECT_FOLDER: &str = ".runo";

/// `root`'s Squad Code folder. A `.runo` folder an older build left is moved into place
/// the first time, unless `root` is the home folder, where `.runo` was the app's data.
pub(crate) fn project_folder(root: &std::path::Path) -> std::path::PathBuf {
    let new = root.join(PROJECT_FOLDER);
    let old = root.join(OLD_PROJECT_FOLDER);
    let is_home = crate::store::home_dir().is_some_and(|home| home == root);
    let old_is_dir = std::fs::symlink_metadata(&old).is_ok_and(|m| m.is_dir());
    if !is_home && old_is_dir && std::fs::symlink_metadata(&new).is_err() {
        let _ = std::fs::rename(&old, &new);
    }
    new
}

/// Add `/.squadcode/` to the repository's `info/exclude`, once.
pub(crate) fn exclude_own_folder(dir: &str) {
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
    if text.lines().any(|l| l.trim() == "/.squadcode/") {
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
    let _ = std::fs::write(&path, format!("{text}{sep}/.squadcode/\n"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_projects_old_folder_moves_into_place_once() {
        let root =
            std::env::temp_dir().join(format!("squadcode-own-folder-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join(".runo/design")).unwrap();
        std::fs::write(root.join(".runo/design/a.png"), "x").unwrap();
        let folder = project_folder(&root);
        assert_eq!(folder, root.join(".squadcode"));
        assert!(folder.join("design/a.png").is_file());
        assert!(!root.join(".runo").exists());
        // With both there, neither is touched.
        std::fs::create_dir_all(root.join(".runo")).unwrap();
        project_folder(&root);
        assert!(root.join(".runo").is_dir());
        assert!(folder.join("design/a.png").is_file());
        let _ = std::fs::remove_dir_all(&root);
    }
}
