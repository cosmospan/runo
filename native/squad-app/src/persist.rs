//! How the old window kept projects and sessions on disk: an index in `state.json` and
//! one file per session at `sessions/<id>/session.json`. Squad Code only reads them now.

use crate::store::{Session, State};
use std::path::{Path, PathBuf};

/// The folder holding a session's file and the output of its running turn.
pub fn session_dir(root: &Path, id: u64) -> PathBuf {
    root.join("sessions").join(id.to_string())
}

fn session_file(root: &Path, id: u64) -> PathBuf {
    session_dir(root, id).join("session.json")
}

/// Read the state at `index` (and the session files next to it) without changing
/// anything on disk, with why the index could be read only in part, when it could.
pub fn read(index: &Path) -> Result<(State, Option<String>), String> {
    let Ok(text) = std::fs::read_to_string(index) else {
        return Ok((State::default(), None));
    };
    let mut value: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let listed: Vec<u64> = value
        .as_object_mut()
        .and_then(|o| o.remove("session_files"))
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();
    // An index that doesn't read as a whole keeps what does (see `store::salvage`), and
    // says why, so the loader can keep a copy of the file before it is saved over.
    let (mut state, index_error) = match serde_json::from_value::<State>(value.clone()) {
        Ok(state) => (state, None),
        Err(error) => match crate::store::salvage::<State>(value) {
            Some(state) => (state, Some(error.to_string())),
            None => return Err(error.to_string()),
        },
    };
    let root = index.parent().unwrap_or(Path::new("."));
    // A session written just before Runo stopped may be missing from the index. Ids up to
    // `next_id` that aren't listed were deleted.
    let mut ids = listed.clone();
    if let Ok(entries) = std::fs::read_dir(root.join("sessions")) {
        let mut newer: Vec<u64> = entries
            .flatten()
            .filter_map(|e| e.file_name().to_str()?.parse().ok())
            .filter(|id| *id > state.next_id && !listed.contains(id))
            .filter(|id| session_file(root, *id).is_file())
            .collect();
        newer.sort_unstable();
        state.next_id = newer.last().copied().unwrap_or(state.next_id);
        ids.extend(newer);
    }
    for id in ids {
        let path = session_file(root, id);
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        match serde_json::from_str::<Session>(&text) {
            Ok(session) => state.sessions.push(session),
            Err(error) => {
                // The file is kept beside it as it was, and what can still be read of the
                // session stays in the sidebar instead of disappearing.
                crate::store::move_aside(&path, &error.to_string());
                let salvaged = serde_json::from_str(&text)
                    .ok()
                    .and_then(crate::store::salvage::<Session>);
                if let Some(session) = salvaged.filter(|s| s.id == id) {
                    state.sessions.push(session);
                }
            }
        }
    }
    Ok((state, index_error))
}
