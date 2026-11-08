//! What Peek says, and how Peek looks while saying it.
//!
//! Peek's voice: short, warm, plain words, one sentence, never more cheerful
//! than the news. Each moment has a few lines so Peek doesn't repeat itself;
//! which one is picked follows from the job, so the same job always reads the
//! same way. The facts in a line (names, counts, titles) come from the crew,
//! never from a model.

use squad_holo::Mood;

use super::super::data::Member;

/// Something that happened that Peek can speak to. The crew speaks bot ideas
/// today; the window's Peek bubble takes the rest next.
#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub enum Moment {
    Started {
        bot: Member,
        title: String,
    },
    /// Finished, with how many files changed (0 for an answer).
    Done {
        bot: Member,
        title: String,
        files: usize,
    },
    Failed {
        bot: Member,
        title: String,
    },
    /// Waiting for the person to approve its changes.
    Review {
        bot: Member,
        title: String,
    },
    /// Sent to the person's server.
    Placed {
        bot: Member,
        server: String,
    },
    /// A bot worth making.
    Idea {
        name: String,
        why: String,
    },
    /// Nothing running and nothing waiting.
    Quiet,
}

/// Peek's mood for a moment.
#[allow(dead_code)]
pub fn mood(m: &Moment) -> Mood {
    match m {
        Moment::Started { .. } | Moment::Placed { .. } => Mood::Working,
        Moment::Done { .. } => Mood::Done,
        Moment::Failed { .. } | Moment::Review { .. } | Moment::Idea { .. } => Mood::NeedsYou,
        Moment::Quiet => Mood::Idle,
    }
}

/// A job title short enough to say: its first line, at most 48 characters.
pub fn short(title: &str) -> String {
    let first = title.lines().next().unwrap_or("").trim();
    if first.chars().count() <= 48 {
        return first.to_string();
    }
    let mut out: String = first.chars().take(47).collect();
    if let Some(cut) = out.rfind(' ').filter(|c| *c > 24) {
        out.truncate(cut);
    }
    out.push('\u{2026}');
    out
}

/// A stable pick among `n` lines for the same `seed`.
fn pick(seed: &str, n: usize) -> usize {
    // FNV-1a: small, stable across runs and platforms.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in seed.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    (h % n as u64) as usize
}

/// What Peek says for a moment.
pub fn line(m: &Moment) -> String {
    let q = |s: &str| format!("\u{201c}{}\u{201d}", short(s));
    match m {
        Moment::Started { bot, title } => {
            let b = bot.name();
            let t = q(title);
            let lines = [
                format!("{b} is on {t}."),
                format!("{b} picked up {t}."),
                format!("{b} started {t}. I'll tell you when it's done."),
            ];
            lines[pick(title, lines.len())].clone()
        }
        Moment::Done { bot, title, files } => {
            let b = bot.name();
            let t = q(title);
            let what = match files {
                0 => String::new(),
                1 => " and changed 1 file".into(),
                n => format!(" and changed {n} files"),
            };
            let lines = [
                format!("{b} finished {t}{what}."),
                format!("{t} is done. {b} did it{what}."),
            ];
            lines[pick(title, lines.len())].clone()
        }
        Moment::Failed { bot, title } => {
            let b = bot.name();
            let t = q(title);
            let lines = [
                format!("{b} got stuck on {t}. Have a look?"),
                format!("{t} stopped before it was done. {b} needs you."),
            ];
            lines[pick(title, lines.len())].clone()
        }
        Moment::Review { bot, title } => {
            let b = bot.name();
            let t = q(title);
            let lines = [
                format!("{b}'s changes for {t} are ready for you to look at."),
                format!("{t} is waiting for your OK."),
            ];
            lines[pick(title, lines.len())].clone()
        }
        Moment::Placed { bot, server } => {
            format!(
                "{} is working on {server}, so this computer stays free.",
                bot.name()
            )
        }
        Moment::Idea { name, why } => {
            format!("{why} Want a bot for it? I'd call it {name}.")
        }
        Moment::Quiet => "All quiet. Nothing needs you.".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_are_stable_and_name_the_facts() {
        let m = Moment::Done {
            bot: Member::Builder,
            title: "Add a dark mode toggle".into(),
            files: 3,
        };
        assert_eq!(line(&m), line(&m));
        assert!(line(&m).contains("3 files"));
        assert!(line(&m).contains("Builder"));
        assert_eq!(mood(&m), Mood::Done);
    }

    #[test]
    fn long_titles_are_cut_at_a_word() {
        let s =
            short("Rewrite the whole onboarding flow so that it works on phones and tablets too");
        assert!(s.ends_with('\u{2026}'));
        assert!(s.chars().count() <= 48);
        assert!(!s.contains("  "));
    }
}
