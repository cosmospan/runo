//! The connections the person keeps, per project, in `~/.runo/data/connections.json`.
//! No secret is written there: a connection found in the project's files is found again
//! each time it opens, and a password typed or pasted goes to the macOS Keychain (on
//! other systems, to a file only the person can read).

use super::{detect, Env, Kind};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "from", rename_all = "lowercase")]
pub(crate) enum Source {
    /// Found in the project's files, by `detect::Found::key`.
    Found { key: String },
    /// Added by URL; the password, if any, is kept apart.
    Url { url: String, password: bool },
    /// A SQLite file picked by path.
    File { path: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Saved {
    pub id: String,
    pub project: u64,
    pub name: String,
    pub kind: Kind,
    #[serde(default)]
    pub env: Env,
    pub source: Source,
    /// The address without secrets, as shown in the list.
    #[serde(default)]
    pub shown: String,
}

#[cfg(test)]
thread_local! {
    /// Tests keep their files apart from each other and from the person's.
    pub(crate) static TEST_DIR: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

fn folder() -> PathBuf {
    #[cfg(test)]
    if let Some(dir) = TEST_DIR.with_borrow(|d| d.clone()) {
        return dir;
    }
    crate::store::data_dir().join("data")
}

/// Make Runo's data folder, readable only by the person.
fn make_folder() -> std::io::Result<PathBuf> {
    let dir = folder();
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&dir)?;
    Ok(dir)
}

/// Write `text` to `path` so that only the person can ever read it: a new file is
/// created with that mode (not loosened then tightened) and moved into place, so a reader
/// never sees it half written or open to others.
pub(crate) fn write_private(path: &Path, text: &str) -> std::io::Result<()> {
    use std::io::Write as _;
    make_folder()?;
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    let _ = std::fs::remove_file(&tmp);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&tmp)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&tmp, path)
}

fn file() -> PathBuf {
    folder().join("connections.json")
}

pub(crate) fn load() -> Vec<Saved> {
    std::fs::read_to_string(file())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// One thing an agent ran on a database, kept so the person can see it in the Data place.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Activity {
    /// Seconds since 1970.
    pub at: u64,
    pub conn: String,
    pub sql: String,
    #[serde(default)]
    pub rows: usize,
    #[serde(default)]
    pub millis: u128,
    /// Why it didn't run, when it didn't.
    #[serde(default)]
    pub refused: Option<String>,
}

fn activity_file() -> PathBuf {
    folder().join("activity.jsonl")
}

/// The log keeps about this much; older lines are dropped.
const ACTIVITY_MAX: u64 = 1 << 20;

/// Add a line to the activity log, which only the person can read.
pub(crate) fn log_activity(entry: &Activity) {
    let path = activity_file();
    let _ = make_folder();
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > ACTIVITY_MAX) {
        if let Ok(text) = std::fs::read_to_string(&path) {
            let keep: Vec<&str> = text.lines().collect();
            let half = keep[keep.len() / 2..].join("\n");
            let _ = write_private(&path, &(half + "\n"));
        }
    }
    let mut options = std::fs::OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    if let (Ok(mut file), Ok(line)) = (options.open(&path), serde_json::to_string(entry)) {
        use std::io::Write as _;
        let _ = writeln!(file, "{line}");
    }
}

/// What to connect to for `saved`, secrets filled in, for the project at `root`.
pub(crate) fn target(saved: &Saved, root: &Path) -> Result<String, String> {
    match &saved.source {
        Source::File { path } => Ok(path.clone()),
        Source::Url { url, password } => {
            let mut url = url::Url::parse(url).map_err(|e| e.to_string())?;
            if *password {
                let pw = get_secret(&saved.id).ok_or_else(|| {
                    "Its password isn't in the Keychain any more. Remove the connection and add it again.".to_string()
                })?;
                let _ = url.set_password(Some(&pw));
            }
            Ok(url.to_string())
        }
        Source::Found { key } => {
            let mut found = detect::find(root);
            found.extend(detect::open_ports(&found));
            let found = found.into_iter().find(|f| &f.key == key);
            if let Some(f) = found.as_ref().filter(|f| f.kind == Kind::Sqlite) {
                let path = f
                    .target
                    .trim_start_matches("sqlite:")
                    .trim_start_matches("file:");
                return detect::inside(root, Path::new(path))
                    .map(|p| p.to_string_lossy().to_string());
            }
            found.map(|f| f.target).ok_or_else(|| {
                format!(
                    "It isn't in the project's files any more (it came from {}).",
                    key.split(':').nth(1).unwrap_or(key)
                )
            })
        }
    }
}

#[cfg(target_os = "macos")]
const SERVICE: &str = "Runo database";

#[cfg(target_os = "macos")]
fn set_secret(id: &str, secret: &str) -> Result<(), String> {
    security_framework::passwords::set_generic_password(SERVICE, id, secret.as_bytes())
        .map_err(|e| format!("The Keychain refused the password: {e}"))
}

#[cfg(target_os = "macos")]
fn get_secret(id: &str) -> Option<String> {
    security_framework::passwords::get_generic_password(SERVICE, id)
        .ok()
        .map(|b| String::from_utf8_lossy(&b).to_string())
}

#[cfg(target_os = "macos")]
fn delete_secret(id: &str) {
    let _ = security_framework::passwords::delete_generic_password(SERVICE, id);
}

/// Elsewhere, a JSON file of secrets readable only by the person.
#[cfg(not(target_os = "macos"))]
fn secrets_file() -> PathBuf {
    folder().join("secrets.json")
}

#[cfg(not(target_os = "macos"))]
fn secrets() -> std::collections::BTreeMap<String, String> {
    std::fs::read_to_string(secrets_file())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

#[cfg(not(target_os = "macos"))]
fn get_secret(id: &str) -> Option<String> {
    secrets().get(id).cloned()
}
