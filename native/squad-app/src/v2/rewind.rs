//! Rewind a job to any message. Before each turn of a job that works on its
//! own branch, Squad notes where its files stood (a git checkpoint, kept
//! under `refs/runo/checkpoints`, so nothing on the branch changes). Rewinding
//! puts the files back as they were before that message, drops the turns
//! after it from the feed, and starts the bot's next turn fresh, told what is
//! still part of the job.

use serde::{Deserialize, Serialize};

use super::store::{now, Item, Job, Status};

/// Where a job's files stood before one of its turns.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Mark {
    /// Where the turn starts in the feed: the message that began it.
    pub feed_at: usize,
    /// The checkpoint commit.
    pub sha: String,
    /// What began the turn, for the button and for telling the bot.
    pub said: String,
    pub at: u64,
}

/// Notes where the files stand before a turn of `job` begins. `said` is what
/// the turn was asked. Does nothing for a job that works in your own folder
/// (rewinding it could undo your own changes) or runs on your server.
pub fn take(job: &mut Job, said: &str) {
    if job.worktree.is_empty() || !job.remote_on.is_empty() {
        return;
    }
    let Ok(sha) = crate::git::checkpoint(&job.worktree) else {
        return;
    };
    // The person's message (or Squad's note) for this turn is already in
    // the feed; rewinding to here takes it away too.
    let feed_at = match job.feed.last() {
        Some(Item::You(_)) | Some(Item::Note(_)) if !job.marks.is_empty() => job.feed.len() - 1,
        _ => job.feed.len(),
    };
    job.marks.push(Mark {
        feed_at,
        sha,
        said: said
            .lines()
            .next()
            .unwrap_or("")
            .trim()
            .chars()
            .take(140)
            .collect(),
        at: now(),
    });
}

/// The marks a job can be rewound to: every turn after its first (rewinding
/// to before the first is Discard), by index into `marks`.
pub fn points(job: &Job) -> Vec<usize> {
    if !can_rewind(job) {
        return Vec::new();
    }
    (1..job.marks.len()).collect()
}

/// Whether a job can be rewound now.
pub fn can_rewind(job: &Job) -> bool {
    !job.worktree.is_empty()
        && job.remote_on.is_empty()
        && !job.shipping
        && matches!(
            job.status,
            Status::Done | Status::Review | Status::Failed | Status::Stopped | Status::Interrupted
        )
}

/// The mark whose turn begins at feed item `at`, when it can be rewound to.
pub fn mark_at(job: &Job, at: usize) -> Option<usize> {
    points(job)
        .into_iter()
        .find(|k| job.marks[*k].feed_at == at)
}

/// Rewinds `job` to before mark `k`: the files go back, later turns leave the
/// feed, and the engine's conversation is dropped so the bot doesn't
/// remember undone work. Returns what to say in the feed.
pub fn rewind(job: &mut Job, k: usize) -> Result<String, String> {
    if !can_rewind(job) {
        return Err("This job can't be rewound now.".into());
    }
    let mark = job.marks.get(k).cloned().ok_or("That point is gone.")?;
    let done = crate::git::restore(&job.worktree, &mark.sha, None)?;
    job.feed.truncate(mark.feed_at.min(job.feed.len()));
    job.marks.truncate(k);
    job.turn_from = job.turn_from.min(job.feed.len());
    job.conversation.clear();
    job.rewound = true;
    job.recap.clear();
    job.conflict.clear();
    job.checks.clear();
    job.status = Status::Done;
    let files = done.files.len();
    Ok(format!(
        "Rewound to before \u{201c}{}\u{201d}. {} put back as {} were then; what came after is gone from here and from {}'s memory.",
        mark.said,
        if files == 1 {
            "1 file is".to_string()
        } else {
            format!("{files} files are")
        },
        if files == 1 { "it" } else { "they" },
        job.bot.name()
    ))
}

/// What the bot's next turn is told after a rewind: the job, what it was
/// asked since that's still standing, and the new ask.
pub fn context(job: &Job, text: &str) -> String {
    let since: Vec<String> = job
        .feed
        .iter()
        .filter_map(|i| match i {
            Item::You(t) => Some(format!("- {}", t.lines().next().unwrap_or("").trim())),
            _ => None,
        })
        .collect();
    let mut out = format!(
        "{}\n\nYou already worked on this, and the files hold that work: check them before redoing anything. Some later turns were undone, so the files may not match what you remember.",
        job.title.trim()
    );
    if !since.is_empty() {
        out.push_str("\n\nSince then you were also asked:\n");
        out.push_str(&since.join("\n"));
    }
    out.push_str(&format!("\n\nNow: {text}"));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::process::Command;

    fn run(dir: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .status()
            .unwrap()
            .success();
        assert!(ok, "git {args:?}");
    }

    #[test]
    fn rewinding_puts_the_files_and_the_feed_back() {
        let dir = std::env::temp_dir().join(format!("squad-rewind-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        run(&dir, &["init", "-q"]);
        std::fs::write(dir.join("a.txt"), "one\n").unwrap();
        run(&dir, &["add", "."]);
        run(&dir, &["commit", "-qm", "base"]);
        let mut job = Job {
            title: "Make a".into(),
            worktree: dir.to_string_lossy().into_owned(),
            status: Status::Running,
            ..Job::default()
        };
        // Turn one changes a.txt.
        take(&mut job, "Make a");
        job.feed.push(Item::Text("Changed a".into()));
        std::fs::write(dir.join("a.txt"), "two\n").unwrap();
        // Turn two, asked by the person, changes it again and adds b.txt.
        job.feed.push(Item::You("Make it three".into()));
        take(&mut job, "Make it three");
        job.feed.push(Item::Text("Now three".into()));
        std::fs::write(dir.join("a.txt"), "three\n").unwrap();
        std::fs::write(dir.join("b.txt"), "new\n").unwrap();
        job.status = Status::Review;
        assert_eq!(points(&job), vec![1]);
        assert_eq!(mark_at(&job, 1), Some(1));
        let note = rewind(&mut job, 1).unwrap();
        assert!(note.contains("Make it three"), "{note}");
        assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "two\n");
        assert!(!dir.join("b.txt").exists());
        assert_eq!(job.feed, vec![Item::Text("Changed a".into())]);
        assert_eq!(job.marks.len(), 1);
        assert!(job.rewound && job.conversation.is_empty());
        let ctx = context(&job, "Make it four");
        assert!(ctx.starts_with("Make a") && ctx.ends_with("Now: Make it four"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_job_in_your_folder_keeps_no_marks() {
        let mut job = Job::default();
        take(&mut job, "x");
        assert!(job.marks.is_empty());
        assert!(points(&job).is_empty());
    }
}
