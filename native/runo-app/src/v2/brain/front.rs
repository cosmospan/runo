//! Peek's front door: every message the person sends is read here first.
//!
//! Each one ends in one of these:
//! - [`Front::Chat`]: small talk ("hi", "thanks", "what can you do"). Peek
//!   replies itself, at once; no engine, no job.
//! - [`Front::Answer`]: a question. An engine answers it read-only; no branch
//!   and no job card.
//! - [`Front::AskBack`]: too vague to start ("fix it"). Peek asks one question.
//! - [`Front::Hand`]: real work. It goes to the bot the routing picks, as a job.
//!
//! The built-in rules and the tiny classifier decide in microseconds, with
//! nothing downloaded. With Smart Peek on, its model is asked whether a job
//! is too vague; everything else stays built in, so sending never waits on it.
//! A client skips this when the person picked a bot or typed a command.

use crate::settings::AppSettings;

use super::super::data::{Member, Mode};
use super::route::{self, Route};
use super::{tasks, words};

/// What Peek does with a message.
#[derive(Clone, Debug, PartialEq)]
pub enum Front {
    /// Peek answers itself, with this.
    Chat { reply: String },
    /// A question for an engine to answer, read-only. `code` when it is about
    /// the project, so the engine should read the folder.
    Answer { question: String, code: bool },
    /// Too vague to start: ask this, offering `choices` when there are some.
    AskBack {
        question: String,
        choices: Vec<String>,
    },
    /// Real work, for this bot.
    Hand(Route),
    /// Real work that can't easily be undone (production, deleting, secrets,
    /// sending): show `why` and start it as `Hand` only on the person's yes.
    Confirm { why: String, route: Route },
}

/// Reads `text` and decides. `recent` is the conversation so far, oldest
/// first, as (who, words); a short follow-up ("yes", "do it") after Peek asked
/// something is read as an answer to it, not as small talk.
pub fn read(st: &AppSettings, text: &str, recent: &[(String, String)]) -> Front {
    let t = text.trim();
    let follow_up = recent.last().is_some_and(|(who, said)| {
        who.eq_ignore_ascii_case("peek") && said.trim_end().ends_with('?')
    });
    if !follow_up {
        if let Some(reply) = small_talk(t) {
            return Front::Chat { reply };
        }
        if let Some((question, choices)) = vague(t) {
            return Front::AskBack { question, choices };
        }
    }
    let r = route::route(st, t);
    if r.mode == Mode::Ask || (r.sure < Route::SURE && general_question(t)) {
        return Front::Answer {
            question: t.to_string(),
            code: about_code(t),
        };
    }
    // Smart Peek, when it's on and up, catches vague work the rules let by.
    if !follow_up {
        if let Some(question) = tasks::clarify(st, t) {
            return Front::AskBack {
                question,
                choices: Vec::new(),
            };
        }
    }
    match super::guard::risk(t) {
        Some(why) => Front::Confirm { why, route: r },
        None => Front::Hand(r),
    }
}

/// Words that only make small talk.
const GREETINGS: [&str; 17] = [
    "hi",
    "hii",
    "hello",
    "hey",
    "heya",
    "yo",
    "hiya",
    "sup",
    "morning",
    "evening",
    "afternoon",
    "good",
    "day",
    "there",
    "peek",
    "all",
    "everyone",
];
const THANKS: [&str; 12] = [
    "thanks", "thank", "you", "ty", "thx", "cheers", "great", "nice", "cool", "awesome", "perfect",
    "lovely",
];
const BYE: [&str; 6] = ["bye", "goodbye", "night", "later", "see", "ya"];
const OK: [&str; 6] = ["ok", "okay", "k", "sure", "alright", "got"];

fn all_in(ws: &[String], set: &[&str]) -> bool {
    !ws.is_empty() && ws.iter().all(|w| set.contains(&w.as_str()))
}

/// Peek's reply to small talk, or None when the message is more than that.
pub fn small_talk(text: &str) -> Option<String> {
    let ws = words(text);
    if ws.is_empty() || ws.len() > 6 {
        return None;
    }
    let lower = text.to_lowercase();
    let has = |p: &str| lower.contains(p);
    if has("what can you do")
        || has("who are you")
        || has("what are you")
        || has("how do you work")
        || ws == ["help"]
    {
        return Some(
            "I'm Peek. Ask me anything, and I'll answer or look it up. Tell me what you want done, and I'll hand it to the right bot on your crew. If something's unclear, I'll ask first."
                .into(),
        );
    }
    if has("how are you") || has("how's it going") || has("hows it going") {
        return Some("All good here, and the crew's ready. What can I do for you?".into());
    }
    if all_in(&ws, &GREETINGS) {
        return Some(
            "Hi! What can I do for you? Ask me something, or tell me what you need done.".into(),
        );
    }
    if all_in(
        &ws,
        &[THANKS.as_slice(), &["so", "much", "a", "lot", "peek"]].concat(),
    ) {
        return Some("Any time.".into());
    }
    if all_in(&ws, &BYE) {
        return Some("See you. The crew keeps going while you're away.".into());
    }
    if all_in(&ws, &[OK.as_slice(), &["it", "thanks"]].concat()) {
        return Some("Okay. Anything else?".into());
    }
    None
}

/// Words that name no thing to work on.
const EMPTY: [&str; 22] = [
    "fix",
    "it",
    "this",
    "that",
    "make",
    "better",
    "improve",
    "do",
    "the",
    "thing",
    "stuff",
    "update",
    "change",
    "please",
    "pls",
    "plz",
    "something",
    "now",
    "again",
    "all",
    "work",
    "on",
];

/// The question to ask when `text` names nothing to work on: "fix it", "make
/// it better".
pub fn vague(text: &str) -> Option<(String, Vec<String>)> {
    let ws = words(text);
    if ws.is_empty() || ws.len() > 5 || !all_in(&ws, &EMPTY) {
        return None;
    }
    let lead = if ws.iter().any(|w| w == "fix") {
        "Happy to fix it. What's going wrong, and where? A page, a file or an error message helps."
    } else if ws.iter().any(|w| w == "better" || w == "improve") {
        "What should get better: how it looks, how fast it is, or how it works?"
    } else {
        "What would you like done? Name the thing, and I'll take it from there."
    };
    let choices = if ws.iter().any(|w| w == "better" || w == "improve") {
        vec!["How it looks".into(), "Speed".into(), "How it works".into()]
    } else {
        Vec::new()
    };
    Some((lead.to_string(), choices))
}

/// Words that mark a question as about the project, not the world.
const CODE: [&str; 40] = [
    "code",
    "app",
    "repo",
    "project",
    "function",
    "file",
    "files",
    "class",
    "method",
    "module",
    "bug",
    "test",
    "tests",
    "build",
    "error",
    "api",
    "endpoint",
    "page",
    "button",
    "screen",
    "component",
    "database",
    "schema",
    "query",
    "branch",
    "commit",
    "pr",
    "deploy",
    "ci",
    "config",
    "server",
    "route",
    "auth",
    "login",
    "css",
    "html",
    "script",
    "crate",
    "package",
    "dependency",
];

/// Whether a question is about the project (read the folder) or the world.
pub fn about_code(text: &str) -> bool {
    let lower = text.to_lowercase();
    words(text).iter().any(|w| CODE.contains(&w.as_str()))
        || lower.contains('/')
        || lower.contains("()")
        || lower.contains('`')
        || [
            ".rs", ".ts", ".js", ".py", ".go", ".swift", ".kt", ".json", ".md",
        ]
        .iter()
        .any(|e| lower.contains(e))
}

/// A question to answer rather than work to do: it ends with a question mark
/// or starts like one, and asks for no change.
pub fn general_question(text: &str) -> bool {
    let ws = words(text);
    let Some(first) = ws.first() else {
        return false;
    };
    const STARTS: [&str; 18] = [
        "what",
        "how",
        "why",
        "who",
        "when",
        "where",
        "which",
        "can",
        "could",
        "is",
        "are",
        "should",
        "does",
        "do",
        "explain",
        "summarise",
        "summarize",
        "tell",
    ];
    text.trim_end().ends_with('?') || STARTS.contains(&first.as_str())
}

/// Who answers a question when nobody was picked: the crew's explainer.
pub fn answerer(crew: &[Member]) -> Member {
    route::doer(Mode::Ask, crew)
}

/// The outcome's name, as the front door's test set writes it.
pub fn outcome(f: &Front) -> &'static str {
    match f {
        Front::Chat { .. } => "chat",
        Front::Answer { .. } => "answer",
        Front::AskBack { .. } => "ask_back",
        Front::Confirm { .. } => "confirm",
        Front::Hand(_) => "hand",
    }
}

/// How the front door did on a test set.
#[derive(Default)]
pub struct Score {
    pub total: usize,
    pub right: usize,
    /// Of the work it handed on, how many had the right kind of job.
    pub mode_total: usize,
    pub mode_right: usize,
    /// (wanted, got) → count.
    pub confusion: std::collections::BTreeMap<(String, String), usize>,
    pub misses: Vec<(String, String, String)>,
}

impl Score {
    pub fn report(&self) -> String {
        let pct = |a: usize, b: usize| {
            if b == 0 {
                0.0
            } else {
                a as f64 * 100.0 / b as f64
            }
        };
        let mut out = format!(
            "Front door: {}/{} right ({:.0}%). Kind of job, when handed on: {}/{} ({:.0}%).",
            self.right,
            self.total,
            pct(self.right, self.total),
            self.mode_right,
            self.mode_total,
            pct(self.mode_right, self.mode_total)
        );
        let mut wrong: Vec<_> = self.confusion.iter().filter(|((w, g), _)| w != g).collect();
        wrong.sort_by(|a, b| b.1.cmp(a.1));
        for ((w, g), n) in wrong.into_iter().take(8) {
            out.push_str(&format!("\n  wanted {w}, got {g}: {n}"));
        }
        for (t, w, g) in self.misses.iter().take(25) {
            out.push_str(&format!("\n  missed: {t:?} wanted {w}, got {g}"));
        }
        out
    }
}

/// Scores [`read`] on JSON lines of {"text", "want", "mode"}.
pub fn score(st: &AppSettings, jsonl: &str) -> Score {
    let mut s = Score::default();
    for line in jsonl.lines().filter(|l| !l.trim().is_empty()) {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let (Some(text), Some(want)) = (v["text"].as_str(), v["want"].as_str()) else {
            continue;
        };
        let f = read(st, text, &[]);
        let got = outcome(&f);
        s.total += 1;
        *s.confusion
            .entry((want.to_string(), got.to_string()))
            .or_default() += 1;
        if got == want {
            s.right += 1;
        } else {
            s.misses
                .push((text.chars().take(90).collect(), want.into(), got.into()));
        }
        let mode = match &f {
            Front::Hand(r) | Front::Confirm { route: r, .. } => Some(r.mode),
            _ => None,
        };
        if let (Some(m), Some(w)) = (mode, v["mode"].as_str().filter(|m| !m.is_empty())) {
            s.mode_total += 1;
            s.mode_right += usize::from(m.label().eq_ignore_ascii_case(w));
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read0(text: &str) -> Front {
        read(&AppSettings::default(), text, &[])
    }

    #[test]
    fn hi_is_small_talk_not_a_job() {
        for t in [
            "hi",
            "Hi!",
            "hello peek",
            "hey there",
            "good morning",
            "thanks!",
            "thank you so much",
            "bye",
        ] {
            assert!(
                matches!(read0(t), Front::Chat { .. }),
                "{t}: {:?}",
                read0(t)
            );
        }
        assert!(matches!(read0("what can you do?"), Front::Chat { .. }));
    }

    #[test]
    fn a_message_that_names_nothing_gets_a_question() {
        for t in ["fix it", "make it better", "do the thing", "pls fix"] {
            assert!(
                matches!(read0(t), Front::AskBack { .. }),
                "{t}: {:?}",
                read0(t)
            );
        }
    }

    #[test]
    fn questions_are_answered_without_a_job() {
        match read0("how does the auth middleware work") {
            Front::Answer { code, .. } => assert!(code),
            other => panic!("{other:?}"),
        }
        match read0("what's a good laptop for video editing under $1000?") {
            Front::Answer { code, .. } => assert!(!code),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn real_work_goes_to_a_bot() {
        match read0("the export button does nothing on safari") {
            Front::Hand(r) => assert_eq!(r.mode, Mode::Fix),
            other => panic!("{other:?}"),
        }
        match read0("add a dark mode toggle to settings") {
            Front::Hand(r) => assert_eq!(r.mode, Mode::Build),
            other => panic!("{other:?}"),
        }
        match read0("deploy the new pricing page to production") {
            Front::Confirm { why, route } => {
                assert!(why.contains("production"), "{why}");
                assert_ne!(route.mode, Mode::Ask);
            }
            other => panic!("{other:?}"),
        }
        // A greeting with work in it is work.
        assert!(matches!(
            read0("hi, please fix the login crash"),
            Front::Hand(_)
        ));
    }

    #[test]
    fn the_score_counts_outcomes_and_kinds() {
        let s = score(
            &AppSettings::default(),
            "{\"text\":\"hi\",\"want\":\"chat\",\"mode\":\"\"}\n{\"text\":\"fix the login crash\",\"want\":\"hand\",\"mode\":\"fix\"}\n{\"text\":\"fix it\",\"want\":\"hand\",\"mode\":\"fix\"}",
        );
        assert_eq!((s.right, s.total, s.mode_right, s.mode_total), (2, 3, 1, 1));
        assert!(s.report().contains("wanted hand, got ask_back: 1"));
    }

    #[test]
    fn a_short_answer_to_peeks_question_is_not_small_talk() {
        let recent = vec![(
            "Peek".to_string(),
            "Which page should it be on?".to_string(),
        )];
        assert!(!matches!(
            read(&AppSettings::default(), "ok the settings page", &recent),
            Front::Chat { .. }
        ));
    }
}
