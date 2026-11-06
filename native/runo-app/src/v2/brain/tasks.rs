//! Peek's jobs besides routing, when Smart Peek is on:
//!
//! - [`title`]: a short title for a job's card, from what the person typed.
//! - [`recap`]: what a bot did, in a sentence or two, when its job ends.
//! - [`clarify`]: whether a job is too vague to start, and what to ask back.
//!
//! Each is asked on its own thread, so nothing waits for the model, and each
//! answer is checked before use. With Smart Peek off, or when the model
//! doesn't answer well, nothing changes: the card keeps the person's words and
//! the feed keeps the bot's own last message.

use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

use serde_json::{json, Value};

use crate::settings::AppSettings;

use super::super::store::{Item, Job, Status};
use super::model::Local;

/// How long a job's first ask may wait for Squad's own server to start.
const START_WAIT: Duration = Duration::from_secs(30);

fn head() -> &'static str {
    "You are Peek, the guide in Squad Code, where a crew of coding bots works on a person's project. "
}

/// The instructions for each job. They must match `system` in
/// training/tasks.py word for word; the tests check both against
/// training/eval/<task>_system.txt.
pub fn title_system() -> String {
    format!(
        "{}The person typed a job for the crew. Write the title for the job's card: \
         2 to 6 words, sentence case, starting with a verb, naming the specific thing. \
         Keep the person's names for things. No period, no quotes.\n\
         Answer with JSON only: {{\"title\": text}}.",
        head()
    )
}

pub fn recap_system() -> String {
    format!(
        "{}A bot's job just ended. From the job, what it changed and its last message, \
         tell the person what happened in one or two short plain sentences, under 40 words: \
         what changed or what it found, and what they need to do, if anything. \
         Say it like a colleague would, with no greeting and no markdown. \
         needs_you is true when the person has to decide, answer or fix something before the work can go on.\n\
         Answer with JSON only: {{\"recap\": text, \"needs_you\": true or false}}.",
        head()
    )
}

pub fn clarify_system() -> String {
    format!(
        "{}The person typed a job for the crew. The bots can read the whole project, so a job is clear \
         when a careful developer who reads the code could start on it, even if it is short or has typos. \
         It is unclear only when they would have to guess what the person wants: no thing named, \
         two very different readings, or it points at something only the person knows.\n\
         Answer with JSON only: {{\"clear\": true or false, \"question\": one short question to ask back, or \"\"}}.",
        head()
    )
}

/// What the recap is shown about a job, as `recap_input` in training/tasks.py
/// writes it.
pub fn recap_input(job: &Job) -> String {
    let changed: Vec<String> = if job.changes.is_empty() {
        job.files.clone()
    } else {
        job.changes
            .iter()
            .map(|c| format!("{} (+{} -{})", c.path, c.added, c.removed))
            .collect()
    };
    let changed = if changed.is_empty() {
        "nothing".to_string()
    } else {
        changed.join(", ")
    };
    let mut last = job
        .feed
        .iter()
        .rev()
        .find_map(|i| match i {
            Item::Text(t) if !t.trim().is_empty() => Some(t.trim().to_string()),
            Item::Error(t) => Some(t.trim().to_string()),
            _ => None,
        })
        .unwrap_or_default();
    if last.chars().count() > 1500 {
        last = last.chars().take(1500).collect::<String>() + "…";
    }
    let ended = match job.status {
        Status::Review => "waiting for your approval",
        Status::Failed => "failed",
        Status::Stopped | Status::Interrupted => "stopped",
        _ => "done",
    };
    format!(
        "Job: {}\nBot: {}\nEnded: {ended}\nChanged: {changed}\nLast message:\n{last}",
        job.title.trim(),
        job.bot.name()
    )
}

fn object(reply: &str) -> Option<Value> {
    let start = reply.find('{')?;
    let end = reply.rfind('}')?;
    serde_json::from_str(reply.get(start..=end)?).ok()
}

/// A title the card can use: 1 to 8 words, one line, no quotes or markdown.
pub fn parse_title(reply: &str) -> Option<String> {
    let v = object(reply)?;
    let t = v["title"]
        .as_str()?
        .trim()
        .trim_matches(|c| c == '"' || c == '\'' || c == '`')
        .trim_end_matches('.')
        .trim()
        .to_string();
    let n = t.split_whitespace().count();
    ((1..=8).contains(&n) && !t.contains('\n') && !t.contains(['*', '#', '`'])).then_some(t)
}

/// A recap the feed can use, and whether the job needs the person.
pub fn parse_recap(reply: &str) -> Option<(String, bool)> {
    let v = object(reply)?;
    let r = v["recap"].as_str()?.trim().to_string();
    let n = r.split_whitespace().count();
    ((1..=60).contains(&n)).then_some((r, v["needs_you"].as_bool().unwrap_or(false)))
}

/// The question to ask back, when the job is too vague to start.
pub fn parse_clarify(reply: &str) -> Option<String> {
    let v = object(reply)?;
    if v["clear"].as_bool()? {
        return None;
    }
    let q = v["question"].as_str()?.trim().to_string();
    (!q.is_empty() && q.split_whitespace().count() <= 30).then_some(q)
}

fn schema(task: &str) -> Value {
    match task {
        "title" => {
            json!({"type": "object", "properties": {"title": {"type": "string"}}, "required": ["title"]})
        }
        "recap" => {
            json!({"type": "object", "properties": {"recap": {"type": "string"}, "needs_you": {"type": "boolean"}}, "required": ["recap", "needs_you"]})
        }
        _ => {
            json!({"type": "object", "properties": {"clear": {"type": "boolean"}, "question": {"type": "string"}}, "required": ["clear", "question"]})
        }
    }
}

/// Peek's model, when Smart Peek is on and it's up (starting Squad's own
/// server and waiting up to `wait` for it when that's the one in use).
fn model(st: &AppSettings, wait: Duration) -> Option<Local> {
    let m = Local::from_settings(st)?;
    if super::helper::applies(st) && !super::helper::ensure(st, wait) {
        return None;
    }
    Some(m)
}

/// What Peek said about a job, from [`ask`].
#[derive(Clone, Debug, PartialEq)]
pub enum Said {
    Title(String),
    Recap(String, bool),
}

/// Asks Peek for `job`'s title (`recap` false) or recap, on another thread.
/// None when Smart Peek is off, so nothing is waited for.
pub fn ask(st: &AppSettings, job: &Job, recap: bool) -> Option<Receiver<Said>> {
    Local::from_settings(st)?;
    let st = st.clone();
    let (system, user) = if recap {
        (recap_system(), recap_input(job))
    } else {
        (title_system(), job.title.trim().to_string())
    };
    // A short ask is already its own title.
    if !recap && user.split_whitespace().count() <= 4 && !user.contains('\n') {
        return None;
    }
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let Some(m) = model(&st, START_WAIT) else {
            return;
        };
        let task = if recap { "recap" } else { "title" };
        let Some(reply) = m.chat(&system, &user, Some(schema(task))) else {
            return;
        };
        let said = if recap {
            parse_recap(&reply).map(|(r, n)| Said::Recap(r, n))
        } else {
            parse_title(&reply).map(Said::Title)
        };
        if let Some(s) = said {
            let _ = tx.send(s);
        }
    });
    Some(rx)
}

/// Peek's title for `text`, waiting for the answer. None when Smart Peek is
/// off or not up yet.
pub fn title(st: &AppSettings, text: &str) -> Option<String> {
    let m = model(st, Duration::ZERO)?;
    parse_title(&m.chat(&title_system(), text.trim(), Some(schema("title")))?)
}

/// The question to ask back about `text`, or None when it's clear enough to
/// start (or Smart Peek is off or not up yet). Waits for the answer.
pub fn clarify(st: &AppSettings, text: &str) -> Option<String> {
    let m = model(st, Duration::ZERO)?;
    let reply = m.chat(&clarify_system(), text.trim(), Some(schema("clarify")))?;
    parse_clarify(&reply)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v2::data::Member;
    use crate::v2::store::Changed;

    #[test]
    fn the_prompts_match_the_ones_training_uses() {
        assert_eq!(
            title_system(),
            include_str!("../../../../../training/eval/title_system.txt")
        );
        assert_eq!(
            recap_system(),
            include_str!("../../../../../training/eval/recap_system.txt")
        );
        assert_eq!(
            clarify_system(),
            include_str!("../../../../../training/eval/clarify_system.txt")
        );
    }

    #[test]
    fn a_good_title_is_used_and_a_rambling_one_is_not() {
        assert_eq!(
            parse_title("{\"title\": \"Fix Safari export button.\"}").as_deref(),
            Some("Fix Safari export button")
        );
        assert!(parse_title("{\"title\": \"\"}").is_none());
        assert!(parse_title(
            "{\"title\": \"This job is about fixing the export button that does nothing on Safari\"}"
        )
        .is_none());
        assert!(parse_title("Fix the button").is_none());
    }

    #[test]
    fn a_recap_says_whether_it_needs_you() {
        assert_eq!(
            parse_recap(
                "{\"recap\": \"Tests pass. Pick a color for the badge.\", \"needs_you\": true}"
            ),
            Some(("Tests pass. Pick a color for the badge.".into(), true))
        );
        assert!(parse_recap("{\"recap\": \"\"}").is_none());
    }

    #[test]
    fn only_an_unclear_job_gets_a_question() {
        assert_eq!(
            parse_clarify("{\"clear\": false, \"question\": \"Which page?\"}").as_deref(),
            Some("Which page?")
        );
        assert!(parse_clarify("{\"clear\": true, \"question\": \"\"}").is_none());
    }

    #[test]
    fn the_recap_input_reads_like_training() {
        let job = Job {
            bot: Member::Fixer,
            title: "fix the csv export".into(),
            status: Status::Review,
            changes: vec![Changed {
                path: "src/export.ts".into(),
                status: "M".into(),
                added: 12,
                removed: 3,
            }],
            feed: vec![
                Item::Text("Fixed the header row.".into()),
                Item::Note("x".into()),
            ],
            ..Default::default()
        };
        assert_eq!(
            recap_input(&job),
            "Job: fix the csv export\nBot: Fixer\nEnded: waiting for your approval\n\
             Changed: src/export.ts (+12 -3)\nLast message:\nFixed the header row."
        );
    }

    #[test]
    fn nothing_is_asked_with_smart_peek_off() {
        let job = Job {
            title: "make the settings page load faster on old phones".into(),
            ..Default::default()
        };
        assert!(ask(&AppSettings::default(), &job, false).is_none());
    }
}
