//! What a referenced thread says, in a few lines.
//!
//! A message can point at another thread, Slack-style: `#12` (a job's number)
//! or `#fix-safari-export-button` (its title, as a slug). The bot doing the
//! new work, and Peek answering, get a short digest of each one: what was
//! asked, what it changed, how its checks went and how it ended. A digest
//! rather than the whole log keeps the new job cheap and on point.

use super::super::store::{Item, Job, Status};

/// The longest digest one thread gets, in characters.
const MAX: usize = 1200;

/// A title as a reference: lowercase words joined by '-'.
pub fn slug(title: &str) -> String {
    title
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .take(8)
        .collect::<Vec<_>>()
        .join("-")
}

/// The jobs `text` points at with `#`, each once, in the order mentioned.
pub fn referenced<'a>(text: &str, jobs: &'a [Job]) -> Vec<&'a Job> {
    let mut out: Vec<&Job> = Vec::new();
    for part in text.split('#').skip(1) {
        let token: String = part
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        if token.is_empty() {
            continue;
        }
        let token = token.to_lowercase();
        let hit = match token.parse::<u64>() {
            Ok(id) => jobs.iter().find(|j| j.id == id),
            Err(_) => jobs
                .iter()
                .rev()
                .find(|j| slug(j.label()) == token || slug(&j.title) == token),
        };
        if let Some(j) = hit {
            if !out.iter().any(|o| o.id == j.id) {
                out.push(j);
            }
        }
    }
    out
}

/// The threads `text` points at whose changes wait, unmerged, on their own
/// branch: a new job starts from the base without them.
pub fn unmerged<'a>(text: &str, jobs: &'a [Job]) -> Vec<&'a Job> {
    referenced(text, jobs)
        .into_iter()
        .filter(|j| j.status == Status::Review && !j.branch.is_empty())
        .collect()
}

/// What a bot is told about an unmerged thread it builds on, so it can read
/// those changes where they are.
pub fn unmerged_hint(j: &Job) -> String {
    let base = if j.base_sha.is_empty() {
        j.base.as_str()
    } else {
        j.base_sha.as_str()
    };
    format!(
        "#{}'s changes aren't merged yet, so they aren't in this folder. They're on branch {}, in the folder {}, and may not be committed; `git -C {} diff {base}` shows them all. Build on them only if the job needs them, and say so.",
        j.id, j.branch, j.worktree, j.worktree
    )
}

fn clip(s: &str, n: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= n {
        return s.to_string();
    }
    let mut out: String = s.chars().take(n).collect();
    out.push('\u{2026}');
    out
}

/// One thread in a few lines.
pub fn digest(j: &Job) -> String {
    let mut lines = vec![format!(
        "#{} \u{201c}{}\u{201d}, {} by {}.",
        j.id,
        j.label(),
        j.status.label().to_lowercase(),
        j.bot.name()
    )];
    if j.label() != j.title.trim() {
        lines.push(format!("Asked: {}", clip(&j.title, 300)));
    }
    if !j.changes.is_empty() {
        let files: Vec<String> = j
            .changes
            .iter()
            .take(8)
            .map(|c| format!("{} (+{} -{})", c.path, c.added, c.removed))
            .collect();
        lines.push(format!("Changed: {}", files.join(", ")));
    } else if !j.files.is_empty() {
        lines.push(format!(
            "Changed: {}",
            j.files
                .iter()
                .take(8)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !j.checks.is_empty() {
        let failed: Vec<&str> = j
            .checks
            .iter()
            .filter(|c| !c.ok)
            .map(|c| c.command.as_str())
            .collect();
        lines.push(if failed.is_empty() {
            "Checks: all passed.".to_string()
        } else {
            format!("Checks failed: {}", failed.join(", "))
        });
    }
    if !j.shipped.is_empty() {
        lines.push(format!("Shipped: {}", j.shipped));
    }
    // The person's own follow-ups, which often hold the decisions.
    for said in j.feed.iter().filter_map(|i| match i {
        Item::You(t) => Some(t),
        _ => None,
    }) {
        lines.push(format!("The person added: {}", clip(said, 200)));
    }
    let said = j.feed.iter().rev().find_map(|i| match i {
        Item::Text(t) if !t.trim().is_empty() => Some(clip(t, 400)),
        _ => None,
    });
    let ended = if !j.recap.is_empty() {
        Some(j.recap.clone())
    } else if said.is_some() {
        said
    } else if j.status != Status::Running {
        Some(super::tasks::facts(j))
    } else {
        None
    };
    if let Some(e) = ended {
        lines.push(format!("It ended with: {e}"));
    }
    clip(&lines.join("\n"), MAX)
}

/// What a bot is told about the threads `text` points at; empty when none.
pub fn context_for(text: &str, jobs: &[Job]) -> String {
    let refs = referenced(text, jobs);
    if refs.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "The person points at these earlier threads. Use them as background; read the files they name if you need more.\n",
    );
    for j in refs.iter().take(5) {
        out.push('\n');
        out.push_str(&digest(j));
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v2::data::Member;
    use crate::v2::store::Changed;

    fn job() -> Job {
        Job {
            id: 12,
            bot: Member::Fixer,
            title: "the export button does nothing on safari".into(),
            short: "Fix Safari export button".into(),
            status: Status::Shipped,
            changes: vec![Changed {
                path: "src/export.ts".into(),
                status: "M".into(),
                added: 9,
                removed: 2,
            }],
            feed: vec![
                Item::You("keep the old CSV header".into()),
                Item::Text("Safari blocked the download; it now uses a link.".into()),
            ],
            shipped: "Merged into main as 3f2a1c9".into(),
            ..Default::default()
        }
    }

    #[test]
    fn a_thread_is_found_by_number_or_by_title() {
        let jobs = vec![job()];
        assert_eq!(referenced("like #12 but for pdf", &jobs).len(), 1);
        assert_eq!(
            referenced("same as #fix-safari-export-button", &jobs).len(),
            1
        );
        assert!(referenced("#99 and #nothing-here, and C# code", &jobs).is_empty());
        assert_eq!(referenced("#12 and again #12", &jobs).len(), 1);
    }

    #[test]
    fn an_unmerged_thread_is_named_with_its_branch() {
        let mut j = job();
        assert!(unmerged("like #12", &[j.clone()]).is_empty());
        j.status = Status::Review;
        j.branch = "squad/12-safari-export".into();
        j.base = "main".into();
        let jobs = vec![j];
        let hit = unmerged("like #12 but for pdf", &jobs);
        assert_eq!(hit.len(), 1);
        let hint = unmerged_hint(hit[0]);
        assert!(hint.contains("squad/12-safari-export"), "{hint}");
        assert!(hint.contains("git -C"), "{hint}");
    }

    #[test]
    fn the_digest_says_what_was_asked_changed_and_decided() {
        let d = digest(&job());
        for want in [
            "#12",
            "Fix Safari export button",
            "src/export.ts (+9 -2)",
            "keep the old CSV header",
            "Merged into main",
            "it now uses a link",
        ] {
            assert!(d.contains(want), "{want} in {d}");
        }
        assert!(context_for("no refs here", &[job()]).is_empty());
        assert!(context_for("do #12 for pdf", &[job()]).contains("earlier threads"));
    }
}
