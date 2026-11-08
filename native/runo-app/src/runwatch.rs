//! The watcher for CI after a push to the default branch, a merge or a tag push. The
//! pull request watcher (`prwatch.rs`) stops once a pull request is merged, but a thread
//! that merged or pushed to main, or pushed a release tag, often has more to do once the
//! runs on that commit or tag finish ("I'll push the release once main's CI is green").
//! Such a thread ends its turn; Runo reads the GitHub Actions runs for what it pushed,
//! on the pull request watcher's pace, and wakes it when they have all finished: "Main CI
//! passed for 1a2b3c4" or "Release workflow failed: build (macos)".
//!
//! What to watch comes from the turn's own commands (`detect`): `git push` to the default
//! branch or of a tag, `gh pr merge`, `gh release create`, or a line the thread writes,
//! such as `runo-watch: main`, `runo-watch: tag v1.2.0` or `runo-watch: run 123`.
//! A watched thread isn't running, so it holds no slot. What the watcher decides is in
//! `decide` and `read_failed`, pure functions over the saved `RunWatch` and one read.

use serde::{Deserialize, Serialize};

/// What a thread waits on.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    /// The runs on the latest commit of a branch, the default branch in practice. Empty
    /// for the folder's current branch, watched only when it turns out to be the default.
    Branch(String),
    /// The runs for a pushed tag, such as a Release workflow. Empty for the newest tag.
    Tag(String),
    /// One run, by its id.
    Run(u64),
}

/// A thread's watched runs, saved with the thread.
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct RunWatch {
    pub target: Target,
    /// For a branch: the commit whose runs are watched, found on the first read.
    #[serde(default)]
    pub sha: String,
    /// When the watch started, in seconds since the epoch.
    #[serde(default)]
    pub since: u64,
    #[serde(default)]
    pub failed_reads: u32,
    #[serde(default)]
    pub empty_reads: u32,
    /// A run to open from the card.
    #[serde(default)]
    pub url: String,
}

impl RunWatch {}

// ---- Reading GitHub ----
