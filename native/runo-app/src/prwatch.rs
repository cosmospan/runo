//! The pull request watcher. A Crew thread that pushed and opened a pull request ends its
//! turn instead of waiting on CI; Runo reads the pull request every couple of minutes
//! (`load_pr_status`) and wakes the thread only when there is something to do: a check
//! failed, someone else reviewed or commented, or the branch conflicts with its base.
//! Only comments from the repository's owner, members and collaborators wake it: anyone
//! can comment on a public repository, so another person's comment only shows on the
//! card ("New comment from @x") for the person to read.
//! Checks passing only turn the thread's card green, without a wake: a turn just to
//! answer "done" would reload the thread's whole context for nothing.
//!
//! A watched thread isn't running, so it holds no slot. What the watcher decides is in
//! `decide` and `read_failed`, pure functions over the saved `Watch` and one read of the
//! pull request; the rest of this file reads, wakes and draws.

use serde::{Deserialize, Serialize};

/// Why a thread was last woken, so its card can say what it is doing.
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Wake {
    Ci,
    Review,
    Conflict,
}

/// A thread's watched pull request, saved with the thread.
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Debug)]
pub struct Watch {
    pub pr: u64,
    #[serde(default)]
    pub url: String,
    /// The pull request's head commit at the last read.
    #[serde(default)]
    pub head: String,
    /// The head commit CI wakes were last counted for, and how many it got.
    #[serde(default)]
    pub ci_head: String,
    #[serde(default)]
    pub ci_wakes: u32,
    /// CI wakes since checks last passed.
    #[serde(default)]
    pub ci_streak: u32,
    #[serde(default)]
    pub comment_wakes: u32,
    #[serde(default)]
    pub failed_reads: u32,
    /// The time of the latest review or comment from someone else already sent.
    #[serde(default)]
    pub seen: String,
    /// The head commit the thread was told conflicts with its base.
    #[serde(default)]
    pub conflict_head: String,
    /// The head commit whose checks all passed.
    #[serde(default)]
    pub green_head: String,
    /// The wake the thread is working on, until its turn ends.
    #[serde(default)]
    pub waking: Option<Wake>,
    /// Why the watcher stopped waking the thread and wants the person.
    #[serde(default)]
    pub needs: Option<String>,
    /// Who last commented without being one of the repository's people, shown on the
    /// card instead of waking the thread, until its next turn ends.
    #[serde(default)]
    pub stranger: Option<String>,
    /// The time of that latest comment already shown.
    #[serde(default)]
    pub stranger_seen: String,
    /// Reads of this pull request where no CI run or check had appeared yet. A push
    /// that never triggers CI stops waking the thread after `MAX_PROBE_READS`.
    #[serde(default)]
    pub probe_reads: u32,
    /// The push produced no CI at all; the card says so instead of pretending to wait.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub no_ci_detected: bool,
}
