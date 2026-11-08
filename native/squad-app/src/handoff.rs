//! Switching a session to another agent: a confirm dialog that says what carries over
//! and what it costs, and a compact handoff brief so the new agent picks up the work.

/// Where Runo keeps a session's memory, inside its folder.
pub(crate) const MEMORY_FILE: &str = ".runo/memory.md";
/// Notes agents saved through the memory server.
pub(crate) const NOTES_FILE: &str = ".runo/notes.md";
