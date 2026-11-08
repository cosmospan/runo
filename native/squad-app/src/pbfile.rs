//! Playbook files: a playbook as one Markdown file people share through Git, a link or
//! a chat, and Runo installs from the Playbooks page.
//!
//! ```text
//! +++
//! name = "Release notes"
//! description = "Draft notes for the next release"
//! author = "anish"
//! version = "1.0.0"
//! agent = "claude"
//! permissions = ["read", "shell"]
//!
//! [[inputs]]
//! key = "since"
//! label = "Compare from"
//! default = "the last tag"
//! +++
//!
//! Read the commits since {{since}} and write release notes…
//! ```
//!
//! The TOML header between the `+++` lines describes it; everything below is the
//! instructions, with `{{key}}` where an input goes.

use serde::{Deserialize, Serialize};

/// A value the person fills in when the playbook runs.
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug, Default)]
pub struct Input {
    pub key: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub default: String,
}

impl Input {}
