//! A running summary of each thread, so every bot knows what the thread is
//! about, not only its last few messages.
//!
//! Peek's engine writes it in the background, on its smallest model: after
//! [`EVERY`] new messages, or when a job in the thread ends, it gets the
//! summary so far and what was said since, and writes the summary again.
//! Every [`REBUILD`] updates it starts afresh from the thread itself, so small
//! mistakes don't pile up. The person's own notes (`/note …`) are kept as
//! they wrote them and win over anything the summary says. The thread's jobs
//! come from Squad Code, never from the model, so none is made up or lost.

use serde::{Deserialize, Serialize};

/// New messages in a thread before its summary is written again.
pub const EVERY: usize = 8;
/// Updates before a summary is written afresh from the whole thread.
pub const REBUILD: u32 = 6;
/// The most a summary may be, in characters.
const MOST: usize = 1400;
/// The messages bots read word for word, beside the summary.
pub const RECENT: usize = 6;

/// A thread's summary, kept per project and thread.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Summary {
    /// The project's folder path.
    pub folder: String,
    /// The thread; 0 is #general.
    pub thread: u64,
    /// What Peek's engine wrote; empty until it first has.
    pub text: String,
    /// The person's own notes, word for word.
    pub notes: Vec<String>,
    /// When the last message it covers was said.
    pub upto: u64,
    /// How many messages it covers.
    pub covers: usize,
    /// Updates since it was last written afresh.
    pub updates: u32,
    /// When it was last written.
    pub at: u64,
}

/// What the engine reads to write a thread's summary: the summary so far
/// (empty to start afresh), the person's notes, and what was said since.
pub fn prompt(
    project: &str,
    thread: &str,
    old: &str,
    notes: &[String],
    said: &[(String, String)],
) -> String {
    let lines: Vec<String> = said
        .iter()
        .map(|(who, words)| {
            let words: String = words.chars().take(600).collect();
            format!("{who}: {}", words.replace('\n', " "))
        })
        .collect();
    let notes = if notes.is_empty() {
        "(none)".to_string()
    } else {
        notes
            .iter()
            .map(|n| format!("- {n}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let old = if old.trim().is_empty() {
        "(none yet: write it from the messages)".to_string()
    } else {
        old.trim().to_string()
    };
    format!(
        "You keep the running summary of #{thread}, a thread in the project \"{project}\" in Squad Code, \
where a person works with a crew of AI bots. Bots read this summary before they answer or work in the thread, \
so it must be accurate. Don't use any tools; just reply.\n\n\
The summary so far:\n{old}\n\n\
The person's own notes (true, and they win over anything else):\n{notes}\n\n\
What was said since, oldest first:\n{said}\n\n\
Write the summary again, taking in what was said. Use exactly these four headings, each followed by short lines starting with \"- \":\n\
For: what this thread is for, in one line (a discussion, planning, or building something, and about what).\n\
Decided: decisions made.\n\
Open: questions still open.\n\
Preferences: rules and preferences the person stated here.\n\
Leave a heading with \"- none\" when nothing fits. Only write what the messages or notes say; never guess. \
Drop what was later changed or undone. Don't list jobs; Squad Code adds them. Plain words, no other markdown, under 180 words.",
        said = if lines.is_empty() {
            "(nothing new)".to_string()
        } else {
            lines.join("\n")
        },
    )
}

/// The engine's summary, tidied: from its first heading, without code
/// fences, and no longer than [`MOST`]. Empty when it holds no summary.
pub fn clean(reply: &str) -> String {
    let reply = reply.replace("```", "");
    let Some(start) = reply.find("For:") else {
        return String::new();
    };
    let text = reply[start..].trim();
    if !text.contains("Decided:") {
        return String::new();
    }
    let mut out: String = text.chars().take(MOST).collect();
    if out.len() < text.len() {
        if let Some(cut) = out.rfind('\n') {
            out.truncate(cut);
        }
    }
    out.trim().to_string()
}

/// Whether a summary is due: enough new messages, or a job here ended
/// since it was written (and something new was said).
pub fn due(new: usize, job_ended: bool) -> bool {
    new >= EVERY || (job_ended && new > 0)
}

/// What a bot reads about a thread: its notes, summary and jobs, then the
/// last few messages word for word. Empty when there's nothing to say.
pub fn context(
    thread: &str,
    summary: Option<&Summary>,
    jobs: &[String],
    recent: &[(String, String)],
) -> String {
    let mut out = Vec::new();
    if let Some(s) = summary {
        if !s.notes.is_empty() {
            out.push(format!(
                "The person's notes on #{thread} (these win):\n{}",
                s.notes
                    .iter()
                    .map(|n| format!("- {n}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }
        if !s.text.trim().is_empty() {
            out.push(format!("Summary of #{thread} so far:\n{}", s.text.trim()));
        }
    }
    if !jobs.is_empty() {
        out.push(format!("Jobs in #{thread}:\n{}", jobs.join("\n")));
    }
    let start = recent.len().saturating_sub(RECENT);
    let lines: Vec<String> = recent[start..]
        .iter()
        .map(|(who, words)| {
            let words: String = words.chars().take(600).collect();
            format!("{who}: {words}")
        })
        .collect();
    if !lines.is_empty() {
        out.push(format!(
            "The latest messages in #{thread}, oldest first:\n{}",
            lines.join("\n")
        ));
    }
    out.join("\n\n")
}

/// The summary's first "For:" line, for the strip at the top of a thread.
pub fn headline(text: &str) -> String {
    let mut lines = text.lines().map(str::trim);
    while let Some(l) = lines.next() {
        if let Some(rest) = l.strip_prefix("For:") {
            let rest = rest.trim();
            let first = if rest.is_empty() {
                lines.next().unwrap_or("")
            } else {
                rest
            };
            return first.trim_start_matches("- ").trim().to_string();
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = "For:\n- Discussion: onboarding research teams\nDecided:\n- Teams sign up with .edu emails\nOpen:\n- How many seats each\nPreferences:\n- none";

    #[test]
    fn the_prompt_carries_the_old_summary_notes_and_new_messages() {
        let p = prompt(
            "shop",
            "tech-discussion",
            "For:\n- old",
            &["This is a discussion, not a build".into()],
            &[("you".into(), "lets onboard research teams".into())],
        );
        for want in [
            "#tech-discussion",
            "For:\n- old",
            "- This is a discussion, not a build",
            "you: lets onboard research teams",
            "Don't list jobs",
        ] {
            assert!(p.contains(want), "{want}");
        }
        assert!(prompt("s", "t", "", &[], &[]).contains("write it from the messages"));
    }

    #[test]
    fn clean_keeps_the_summary_and_drops_the_rest() {
        let r = format!("Sure, here it is:\n```\n{GOOD}\n```");
        assert_eq!(clean(&r), GOOD);
        assert!(clean("I can't do that").is_empty());
        assert!(clean("For: something, but no other headings").is_empty());
        let long = format!("{GOOD}\n{}", "- a long line of words\n".repeat(200));
        assert!(clean(&long).chars().count() <= MOST);
    }

    #[test]
    fn due_after_enough_messages_or_a_finished_job() {
        assert!(!due(EVERY - 1, false));
        assert!(due(EVERY, false));
        assert!(due(1, true));
        assert!(!due(0, true));
    }

    #[test]
    fn bots_read_notes_summary_jobs_and_the_latest_words() {
        let s = Summary {
            text: GOOD.into(),
            notes: vec!["Budget is fixed".into()],
            ..Default::default()
        };
        let recent: Vec<(String, String)> = (0..10)
            .map(|i| ("you".into(), format!("msg {i}")))
            .collect();
        let c = context(
            "tech",
            Some(&s),
            &["- #4 Builder: sign-up page (Ready to review)".into()],
            &recent,
        );
        assert!(
            c.contains("Budget is fixed") && c.contains(".edu emails") && c.contains("#4 Builder")
        );
        assert!(c.contains("msg 9") && c.contains("msg 4") && !c.contains("msg 3"));
        assert!(context("t", None, &[], &[]).is_empty());
    }

    #[test]
    fn the_headline_is_what_the_thread_is_for() {
        assert_eq!(headline(GOOD), "Discussion: onboarding research teams");
        assert_eq!(
            headline("For: Building checkout\nDecided:\n- none"),
            "Building checkout"
        );
        assert_eq!(headline(""), "");
    }
}
