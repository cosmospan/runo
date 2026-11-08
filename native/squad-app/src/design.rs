//! The Design page: each project's design canvas, and the pictures to design from
//! (mockups, screenshots and sketches) and design notes its AI asks use.

use super::*;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use workspace::unix_now;

/// Where a project keeps its references: beside the session attachments, out of git.
pub(crate) const FOLDER: &str = "design";
const INDEX: &str = "references.json";

/// One design reference: an image in the project's design folder and what it is for.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct Reference {
    /// The file name inside the design folder.
    pub file: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
    #[serde(default)]
    pub added: u64,
}

pub(crate) fn folder(root: &Path) -> PathBuf {
    turns::project_folder(root).join(FOLDER)
}

/// The project's references, in the order they were added. Images put in the folder by
/// hand show up too, and entries whose file is gone are dropped.
pub(crate) fn load(root: &Path) -> Vec<Reference> {
    let dir = folder(root);
    let mut refs: Vec<Reference> = std::fs::read_to_string(dir.join(INDEX))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    refs.retain(|r| dir.join(&r.file).is_file());
    let mut extra: Vec<String> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && turns::is_image(p))
        .filter_map(|p| Some(p.file_name()?.to_string_lossy().into_owned()))
        .filter(|name| !refs.iter().any(|r| &r.file == name))
        .collect();
    extra.sort();
    refs.extend(extra.into_iter().map(|file| Reference {
        file,
        ..Default::default()
    }));
    refs
}

pub(crate) fn save(root: &Path, refs: &[Reference]) -> std::io::Result<()> {
    let dir = folder(root);
    std::fs::create_dir_all(&dir)?;
    let text = serde_json::to_string_pretty(refs).map_err(std::io::Error::other)?;
    std::fs::write(dir.join(INDEX), text)
}

/// A file name for a new reference: a timestamp, then the original name made safe.
pub(crate) fn file_name(stamp: &str, name: &str) -> String {
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '-'
            }
        })
        .collect();
    format!("{stamp}-{}", safe.trim_start_matches('.'))
}

fn stamp() -> String {
    chrono::Local::now().format("%Y%m%d-%H%M%S%3f").to_string()
}

/// The design folder, made and kept out of git.
fn ready_folder(root: &Path) -> Option<PathBuf> {
    let dir = folder(root);
    std::fs::create_dir_all(&dir).ok()?;
    turns::exclude_own_folder(&root.to_string_lossy());
    Some(dir)
}

/// Copy an image into the design folder as a new reference.
pub(crate) fn add_file(root: &Path, image: &Path) -> Option<Reference> {
    let dir = ready_folder(root)?;
    let file = file_name(&stamp(), &image.file_name()?.to_string_lossy());
    std::fs::copy(image, dir.join(&file)).ok()?;
    Some(Reference {
        file,
        note: String::new(),
        added: unix_now(),
    })
}

/// Save image bytes (a paste or a capture) as a new reference.
pub(crate) fn add_bytes(root: &Path, bytes: &[u8], name: &str, note: &str) -> Option<Reference> {
    let dir = ready_folder(root)?;
    let file = file_name(&stamp(), name);
    std::fs::write(dir.join(&file), bytes).ok()?;
    Some(Reference {
        file,
        note: note.to_string(),
        added: unix_now(),
    })
}

/// Where the project keeps its design notes: an existing `DESIGN.md` or
/// `docs/design.md`, else `docs/design.md` once something is written.
pub(crate) fn notes_path(root: &Path) -> PathBuf {
    for name in ["DESIGN.md", "design.md", "docs/DESIGN.md", "docs/design.md"] {
        let path = root.join(name);
        if path.is_file() {
            return path;
        }
    }
    root.join("docs").join("design.md")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::prelude::v1::test;

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("squadcode-design-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn references_are_saved_and_found() {
        let root = temp("refs");
        let a = add_bytes(&root, b"png", "home screen.png", "The landing page").unwrap();
        assert!(a.file.ends_with("-home-screen.png"));
        save(&root, std::slice::from_ref(&a)).unwrap();
        // An image dropped into the folder by hand shows up after the saved ones.
        std::fs::write(folder(&root).join("zz.jpg"), b"jpg").unwrap();
        std::fs::write(folder(&root).join("notes.txt"), b"not an image").unwrap();
        let refs = load(&root);
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0], a);
        assert_eq!(refs[1].file, "zz.jpg");
        // A reference whose file is gone is dropped.
        std::fs::remove_file(folder(&root).join(&a.file)).unwrap();
        assert_eq!(load(&root).len(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn names_are_safe() {
        assert_eq!(file_name("1", "My Shot (2).png"), "1-My-Shot--2-.png");
        assert_eq!(file_name("1", ".hidden.png"), "1-hidden.png");
    }

    #[test]
    fn notes_file_prefers_an_existing_one() {
        let root = temp("notes");
        assert_eq!(notes_path(&root), root.join("docs/design.md"));
        std::fs::write(root.join("DESIGN.md"), "x").unwrap();
        assert_eq!(notes_path(&root), root.join("DESIGN.md"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
