//! Crew: one chat per project that answers the person and runs threads for them. A thread
//! is an ordinary session with a `ThreadInfo` beside it: the brief Crew gave it, the
//! checklist and reports its agent keeps, and whether it needs the person.
//!
//! Threads talk back to the app through Squad Code's MCP server (`memory_mcp.rs`), which leaves
//! one small event file per call in `events_dir()`, written under a temporary name and
//! renamed so the app never reads half a file. Agents with no MCP write the same things as
//! fenced `runo-…` blocks in their reply, which `TagFilter` takes out of the visible text.
//!
//! Crew's own chat is a session too (`CANVAS`), held read-only; it acts by ending a reply
//! with a `runo-actions` block, which `parse_actions` and `within_limits` check before
//! anything runs. This file holds the data and the rules; the app side is `crew_ui.rs`.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Steps a checklist keeps; more are cut off.
pub const MAX_STEPS: usize = 20;
/// Characters kept of one step, a decision option or a link.
pub const MAX_LINE: usize = 200;
/// Characters kept of a report, a question or a message to Crew.
pub const MAX_TEXT: usize = 4000;

/// What a session that is a Crew thread keeps besides the session itself.
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Debug)]
pub struct ThreadInfo {
    #[serde(default)]
    pub origin: Origin,
    /// What Crew asked the thread to do, shown at the top of the thread.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub brief: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checklist: Vec<Step>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reports: Vec<Report>,
    /// The decision the thread is waiting on, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision: Option<Decision>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attention: Option<Attention>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget: Option<Budget>,
    /// Hidden from the open threads; the session itself stays.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub resolved: bool,
    /// The pull request Squad Code watches for the thread after it opened one and ended its
    /// turn (`prwatch.rs`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub watch: Option<crate::prwatch::Watch>,
    /// CI runs Squad Code watches for the thread after it pushed or merged to the default
    /// branch, or pushed a tag (`runwatch.rs`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runs: Vec<crate::runwatch::RunWatch>,
    /// The last decision Crew answered for the person, shown on the thread's card.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crew_answer: Option<CrewAnswer>,
}

/// A thread's decision that Crew answered itself instead of passing it to the person.
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Debug)]
pub struct CrewAnswer {
    pub question: String,
    pub answer: String,
    /// Seconds since the epoch.
    #[serde(default)]
    pub at: u64,
}

/// Who started a thread.
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Debug)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Origin {
    /// A session the person started that Crew now routes to.
    #[default]
    Person,
    /// Crew, answering the person's message with this id.
    Crew { message: u64 },
}

#[derive(Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Mark {
    #[default]
    Todo,
    Doing,
    Done,
}

#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Debug)]
pub struct Step {
    pub text: String,
    #[serde(default)]
    pub mark: Mark,
}

#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Debug)]
pub struct Report {
    pub summary: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<String>,
    /// Seconds since the epoch.
    #[serde(default)]
    pub at: u64,
}

#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Hash, Debug)]
pub struct Choice {
    pub label: String,
    /// What happens if the person picks it.
    #[serde(default)]
    pub consequence: String,
}

/// One question for the person, with two to four answers and the one the thread would pick.
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Hash, Debug)]
pub struct Decision {
    pub question: String,
    pub options: Vec<Choice>,
    #[serde(default)]
    pub recommended: usize,
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Attention {
    /// A decision, permission or question is open.
    NeedsYou,
    /// The work is done and waits for review.
    Review,
}

/// What a thread may spend: dollars where its agent reports cost, otherwise turns. Zero
/// means no limit of that kind.
#[derive(Clone, Copy, Default, Serialize, Deserialize, PartialEq, Debug)]
pub struct Budget {
    #[serde(default)]
    pub dollars: f64,
    #[serde(default)]
    pub turns: u32,
}

/// One thing a thread told the app.
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct Event {
    /// Unique, so an event read twice is applied once.
    pub id: String,
    pub thread: u64,
    /// Seconds since the epoch.
    pub at: u64,
    #[serde(flatten)]
    pub kind: EventKind,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventKind {
    /// The whole checklist, replacing the last one, and a short name for the task when
    /// the thread gives one.
    Checklist {
        steps: Vec<Step>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
    },
    Report {
        summary: String,
        #[serde(default)]
        links: Vec<String>,
    },
    Decision(Decision),
    /// A message for Crew, such as a question for another thread.
    Message {
        text: String,
    },
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Where threads leave events for the app.
pub fn events_dir() -> PathBuf {
    crate::store::data_dir().join("crew").join("events")
}

/// A new event from `thread`, stamped now.
pub fn event(thread: u64, kind: EventKind) -> Event {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    Event {
        id: format!("{thread}-{nanos}-{}", std::process::id()),
        thread,
        at: now(),
        kind,
    }
}

/// Leave `event` in `dir`. Written under a temporary name and renamed, so the app never
/// reads half a file; names sort in the order events were sent.
pub fn send_event(dir: &Path, event: &Event) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let name = format!("{nanos:020}-{}", std::process::id());
    let tmp = dir.join(format!("{name}.tmp"));
    std::fs::write(&tmp, serde_json::to_vec(event)?)?;
    std::fs::rename(tmp, dir.join(format!("{name}.json")))
}

/// `text` cut to `max` characters, trimmed.
fn clip(text: &str, max: usize) -> String {
    let text = text.trim();
    match text.char_indices().nth(max) {
        Some((at, _)) => text[..at].trim_end().to_string(),
        None => text.to_string(),
    }
}

/// A checklist as it is kept: steps clipped, empty ones left out, at most `MAX_STEPS`.
pub fn clean_steps(steps: Vec<Step>) -> Vec<Step> {
    steps
        .into_iter()
        .map(|s| Step {
            text: clip(&s.text, MAX_LINE),
            mark: s.mark,
        })
        .filter(|s| !s.text.is_empty())
        .take(MAX_STEPS)
        .collect()
}

/// A decision as it is kept, or why it can't be: it needs a question, two to four options
/// with labels, and a recommended option among them.
pub fn check_decision(decision: Decision) -> Result<Decision, String> {
    let question = clip(&decision.question, MAX_TEXT);
    if question.is_empty() {
        return Err("The decision needs a question.".into());
    }
    let options: Vec<Choice> = decision
        .options
        .into_iter()
        .map(|c| Choice {
            label: clip(&c.label, MAX_LINE),
            consequence: clip(&c.consequence, MAX_LINE),
        })
        .collect();
    if !(2..=4).contains(&options.len()) || options.iter().any(|c| c.label.is_empty()) {
        return Err("Give two to four options, each with a label.".into());
    }
    if decision.recommended >= options.len() {
        return Err("recommended must be the index of one of the options, from 0.".into());
    }
    Ok(Decision {
        question,
        options,
        recommended: decision.recommended,
    })
}

// ---------- Crew's own chat ----------

/// What Crew asks Squad Code to do, from the `runo-actions` block that ends its reply.
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
#[serde(tag = "do", rename_all = "snake_case")]
#[allow(clippy::enum_variant_names)] // the names are the wire format Crew writes
pub enum Action {
    /// Start a thread: a new session that works on `brief`.
    StartThread {
        title: String,
        brief: String,
        /// An agent id such as "codex"; empty for the project's default.
        #[serde(default)]
        agent: String,
        /// Work in its own worktree; unset leaves it to the person's New session default.
        #[serde(default)]
        isolated: Option<bool>,
        /// A model its agent knows, such as "sonnet"; empty for the agent's default.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        model: String,
        /// Reasoning effort, such as "low"; empty for the default.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        effort: String,
        /// What the thread may cost in US dollars before its next turn waits for the
        /// person; 0 for no limit.
        #[serde(default, skip_serializing_if = "is_zero_dollars")]
        budget_usd: f64,
    },
    /// Send a thread a message, as its next prompt.
    MessageThread { thread: u64, text: String },
    /// Read a thread's latest replies, sent back with Crew's next turn.
    ReadThread { thread: u64 },
    /// Mark a thread finished: it leaves Crew's open threads, the session stays.
    ResolveThread { thread: u64 },
}

/// By hand for the dollar budget, which hashes by its bits.
impl std::hash::Hash for Action {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            Action::StartThread {
                title,
                brief,
                agent,
                isolated,
                model,
                effort,
                budget_usd,
            } => {
                (title, brief, agent, isolated, model, effort).hash(state);
                budget_usd.to_bits().hash(state);
            }
            Action::MessageThread { thread, text } => (thread, text).hash(state),
            Action::ReadThread { thread } | Action::ResolveThread { thread } => thread.hash(state),
        }
    }
}

/// A card shown with a message in Crew's chat. Threads are drawn live from the sessions,
/// so a card shows where each one stands now, not when it was posted.
#[derive(Clone, Serialize, Deserialize, PartialEq, Hash, Debug)]
#[serde(tag = "card", rename_all = "snake_case")]
pub enum Card {
    /// Threads started together.
    Started { threads: Vec<u64> },
    /// Threads Crew asked for over its limit, waiting for the person.
    Held {
        actions: Vec<Action>,
        #[serde(default)]
        answered: bool,
    },
    /// News from one thread: it finished, failed, reported or needs the person.
    News { thread: u64 },
}

fn is_zero_dollars(n: &f64) -> bool {
    *n == 0.0
}

impl Action {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thread_info_saves_only_what_is_set() {
        assert_eq!(
            serde_json::to_string(&ThreadInfo::default()).unwrap(),
            r#"{"origin":{"kind":"person"}}"#
        );
        let info = ThreadInfo {
            origin: Origin::Crew { message: 4 },
            brief: "Time launches".into(),
            resolved: true,
            ..Default::default()
        };
        let back: ThreadInfo =
            serde_json::from_str(&serde_json::to_string(&info).unwrap()).unwrap();
        assert_eq!(back, info);
    }

    fn start(title: &str) -> Action {
        Action::StartThread {
            title: title.into(),
            brief: format!("Do {title}"),
            agent: String::new(),
            isolated: None,
            model: String::new(),
            effort: String::new(),
            budget_usd: 0.0,
        }
    }

    #[test]
    fn cards_save_and_load() {
        let card = Card::Held {
            actions: vec![start("Docs")],
            answered: false,
        };
        let text = serde_json::to_string(&card).unwrap();
        assert_eq!(serde_json::from_str::<Card>(&text).unwrap(), card);
        let news: Card = serde_json::from_str(r#"{"card":"news","thread":3}"#).unwrap();
        assert_eq!(news, Card::News { thread: 3 });
    }
}
