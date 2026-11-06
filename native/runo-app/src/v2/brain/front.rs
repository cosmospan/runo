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
use super::words;

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
    /// "stop app", "cancel that", "kill the server": stop what's running
    /// here, with no new job.
    Stop,
}

/// Reads `text` and decides. `recent` is the conversation so far, oldest
/// first, as (who, words); a short follow-up ("yes", "do it") after Peek asked
/// something is read as an answer to it, not as small talk.
pub fn read(st: &AppSettings, text: &str, recent: &[(String, String)]) -> Front {
    let t = text.trim();
    if wants_stop(t) {
        return Front::Stop;
    }
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
    if r.mode == Mode::Ask || (general_question(t) && !asks_for_work(t)) {
        return Front::Answer {
            question: t.to_string(),
            code: about_code(t),
        };
    }
    match super::guard::risk(t) {
        Some(why) => Front::Confirm { why, route: r },
        None => Front::Hand(r),
    }
}

/// Verbs that stop what's running.
const STOP: &[&str] = &[
    "stop",
    "kill",
    "cancel",
    "halt",
    "abort",
    "end",
    "quit",
    "terminate",
    "shut",
    "close",
];

/// Words that may follow a stop verb in a control command: what to stop,
/// and fillers.
const STOP_WHAT: &[&str] = &[
    "the",
    "app",
    "apps",
    "server",
    "servers",
    "it",
    "that",
    "this",
    "these",
    "those",
    "job",
    "jobs",
    "run",
    "running",
    "everything",
    "all",
    "now",
    "please",
    "pls",
    "dev",
    "process",
    "processes",
    "down",
    "bot",
    "bots",
    "work",
    "working",
    "task",
    "tasks",
    "build",
    "test",
    "tests",
    "them",
    "for",
    "right",
    "away",
    "asap",
    "already",
    "just",
    "website",
    "site",
    "preview",
    "peek",
    "hey",
    "ok",
    "okay",
    "thanks",
    "thx",
];

/// Whether `text` is a short command to stop what's running ("stop app",
/// "pls kill the server", "cancel that"), not work about stopping ("stop
/// the app from crashing", "add a stop button").
pub fn wants_stop(text: &str) -> bool {
    let ws = words(text);
    if ws.is_empty() || ws.len() > 6 {
        return false;
    }
    let lead = ["please", "pls", "hey", "peek", "ok", "okay", "just", "now"];
    let start = ws.iter().position(|w| !lead.contains(&w.as_str()));
    let Some(i) = start else {
        return false;
    };
    STOP.contains(&ws.get(i).map(String::as_str).unwrap_or(""))
        && ws.iter().skip(i + 1).all(|w| STOP_WHAT.contains(&w.as_str()))
        // "end" and "close" alone are too open: "close" needs a thing to close.
        && !(matches!(ws.get(i).map(String::as_str), Some("close") | Some("end")) && ws.len() == i + 1)
}

fn all_in(ws: &[String], set: &[&str]) -> bool {
    !ws.is_empty() && ws.iter().all(|w| set.contains(&w.as_str()))
}

/// Words that ask for work or name something to work on. A message with
/// none of these and a social word in it is small talk.
const WORK: &[&str] = &[
    "fix",
    "add",
    "build",
    "make",
    "change",
    "update",
    "write",
    "run",
    "test",
    "tests",
    "review",
    "delete",
    "remove",
    "deploy",
    "push",
    "merge",
    "explain",
    "show",
    "create",
    "refactor",
    "bump",
    "check",
    "rename",
    "move",
    "clean",
    "upgrade",
    "install",
    "set",
    "setup",
    "find",
    "look",
    "debug",
    "improve",
    "speed",
    "sort",
    "help",
    "why",
    "where",
    "when",
    "which",
    "code",
    "bug",
    "error",
    "crash",
    "page",
    "file",
    "files",
    "app",
    "api",
    "script",
    "query",
    "table",
    "test",
    "pr",
    "branch",
    "build",
    "issue",
    "broken",
    "wrong",
    "slow",
    "failing",
    "summarise",
    "summarize",
    "draft",
];

/// Words that only make small talk: greetings, thanks, praise, goodbyes.
const SOCIAL: &[&str] = &[
    "hi",
    "hii",
    "hello",
    "hey",
    "heya",
    "hiya",
    "yo",
    "sup",
    "gm",
    "morning",
    "evening",
    "afternoon",
    "night",
    "thanks",
    "thank",
    "thx",
    "ty",
    "cheers",
    "appreciate",
    "great",
    "nice",
    "cool",
    "awesome",
    "perfect",
    "lovely",
    "amazing",
    "lol",
    "wow",
    "bye",
    "goodbye",
    "cya",
    "later",
    "tomorrow",
    "tmrw",
    "logging",
    "signing",
    "off",
    "welcome",
    "worked",
    "fixed",
    "done",
    "ok",
    "okay",
    "alright",
    "sweet",
    "lifesaver",
    "legend",
    "genius",
    "star",
    "gn",
    "ttyl",
    "gtg",
    "brb",
    "apprecated",
    "appreciated",
    "thnx",
    "tysm",
];

const ABOUT_ME: &str = "I'm Peek. Ask me anything, and I'll answer or look it up. Tell me what you want done, and I'll hand it to the right bot on your crew. If something's unclear, I'll ask first.";

/// Whether the message asks what Peek and the crew can do.
fn asks_what_we_do(lower: &str, ws: &[String]) -> bool {
    let has = |w: &str| ws.iter().any(|x| x == w);
    ((has("what") || has("wat") || has("wht"))
        && (has("can") || has("could"))
        && (has("do") || has("help")))
        && ws.len() <= 12
        || lower.contains("who are you")
        || lower.contains("what are you")
        || lower.contains("how do you work")
        || ws == ["help"]
}

/// Whether the message asks how Peek is.
fn asks_how_are_you(ws: &[String]) -> bool {
    let has = |w: &str| ws.iter().any(|x| x == w);
    let how = has("how") || has("hows") || has("how's");
    how && (has("you")
        || has("u")
        || has("going")
        || has("goin")
        || has("day")
        || has("doing")
        || has("doin"))
        && !ws.iter().any(|w| WORK.contains(&w.as_str()) && w != "help")
        && ws.len() <= 8
}

/// Peek's reply to small talk, or None when the message is more than that.
pub fn small_talk(text: &str) -> Option<String> {
    let ws = words(text);
    if ws.is_empty() || ws.len() > 16 {
        return None;
    }
    let lower = text.to_lowercase();
    if asks_what_we_do(&lower, &ws) {
        return Some(ABOUT_ME.into());
    }
    if asks_how_are_you(&ws) {
        return Some("All good here, and the crew's ready. What can I do for you?".into());
    }
    // A question that isn't about Peek is a question, not small talk.
    if text.trim_end().ends_with('?') || starts_like_a_question(&ws) {
        return None;
    }
    let has = |w: &str| ws.iter().any(|x| x == w);
    let social = ws.iter().any(|w| SOCIAL.contains(&w.as_str()));
    // Thanks and goodbyes may name the work they're about ("thanks, the
    // query runs faster now"); only a new request makes them work.
    let thanks_or_bye = has("thanks")
        || has("thank")
        || has("thx")
        || has("thnx")
        || has("ty")
        || has("tysm")
        || has("cheers")
        || has("appreciate")
        || has("apprecated")
        || has("appreciated")
        || has("lifesaver")
        || has("legend")
        || has("bye")
        || has("gn")
        || has("ttyl")
        || has("cya");
    let work = if thanks_or_bye {
        asks_for_work(text)
            || lower.contains("but ")
            || lower.contains("can you")
            || lower.contains("could you")
    } else {
        ws.iter().any(|w| WORK.contains(&w.as_str()))
    };
    // "nothing needed", "just checking in", "first time here", "new here".
    let idle = lower.contains("nothing for")
        || lower.contains("nothing needed")
        || lower.contains("nothing else")
        || lower.contains("nothing much")
        || lower.contains("checking in")
        || lower.contains("saying hello")
        || lower.contains("new here")
        || lower.contains("first time")
        || lower.contains("that's all")
        || lower.contains("thats all");
    if !(social || idle) || work {
        return None;
    }
    let reply = if has("thanks")
        || has("thank")
        || has("thx")
        || has("ty")
        || has("cheers")
        || has("appreciate")
        || has("worked")
        || has("fixed")
    {
        "Glad it helped. Anything else?"
    } else if has("bye")
        || has("goodbye")
        || has("cya")
        || has("night")
        || has("tomorrow")
        || has("tmrw")
        || has("logging")
        || has("signing")
        || lower.contains("that's all")
        || lower.contains("thats all")
    {
        "See you. The crew keeps going while you're away."
    } else if lower.contains("new here") || lower.contains("first time") {
        "Welcome! Ask me anything, or tell me what you'd like done, in your own words. I'll answer, or hand it to the right bot."
    } else if has("hi")
        || has("hii")
        || has("hello")
        || has("hey")
        || has("heya")
        || has("hiya")
        || has("yo")
        || has("gm")
        || has("morning")
        || has("evening")
        || has("afternoon")
        || has("sup")
    {
        "Hi! What can I do for you? Ask me something, or tell me what you need done."
    } else {
        "Thanks! What's next?"
    };
    Some(reply.into())
}

/// Words that ask for work without naming what: verbs, fillers, and nouns
/// too general to point at one thing ("the page", "the numbers").
const VAGUE: &[&str] = &[
    "fix",
    "sort",
    "out",
    "make",
    "do",
    "change",
    "update",
    "clean",
    "cleanup",
    "up",
    "tidy",
    "improve",
    "better",
    "faster",
    "nicer",
    "cleaner",
    "speed",
    "it",
    "its",
    "it's",
    "this",
    "that",
    "these",
    "those",
    "they",
    "them",
    "the",
    "a",
    "an",
    "bit",
    "little",
    "things",
    "thing",
    "stuff",
    "something",
    "issue",
    "problem",
    "bug",
    "page",
    "script",
    "query",
    "job",
    "numbers",
    "number",
    "colors",
    "color",
    "colours",
    "colour",
    "report",
    "app",
    "one",
    "please",
    "pls",
    "plz",
    "can",
    "could",
    "you",
    "u",
    "again",
    "still",
    "now",
    "broken",
    "wrong",
    "work",
    "on",
    "is",
    "are",
    "all",
    "some",
    "hi",
    "hey",
    "hello",
    "peek",
    "just",
    "quick",
    "same",
    "for",
    "other",
    "we",
    "of",
    "something's",
    "somethings",
    "out",
    "pipeline",
    "dashboard",
    "site",
    "website",
    "ui",
    "design",
    "file",
    "screen",
    "code",
    "layout",
    "view",
    "flow",
    "feature",
    "model",
    "data",
    "styling",
    "style",
    "look",
];

/// The question to ask when `text` names nothing to work on: "fix it", "make
/// it better", "update the page", "do the thing we talked about".
pub fn vague(text: &str) -> Option<(String, Vec<String>)> {
    let ws = words(text);
    let lower = text.to_lowercase();
    // Something only the person knows about.
    let elsewhere = [
        "we talked about",
        "we discussed",
        "as yesterday",
        "same as before",
        "the other one",
        "you know what",
        "like last time",
        "as last time",
        "like i said",
        "like i told",
        "from earlier",
        "from before",
        "i mentioned",
    ]
    .iter()
    .any(|p| lower.contains(p));
    let empty = !ws.is_empty() && ws.len() <= 12 && all_in(&ws, VAGUE)
        // A message of only fillers ("hi peek") is small talk, not vague work.
        && ws.iter().any(|w| ["fix", "sort", "make", "do", "change", "update", "clean", "cleanup", "tidy", "improve", "speed", "broken", "wrong"].contains(&w.as_str()));
    if !(empty || (elsewhere && ws.len() <= 14)) {
        return None;
    }
    let has = |w: &str| ws.iter().any(|x| x == w);
    let (lead, choices): (&str, Vec<String>) = if elsewhere {
        ("I don't have that conversation. Which one do you mean? Name it, or point me at its thread with #.", Vec::new())
    } else if has("fix")
        || has("broken")
        || has("wrong")
        || has("sort")
        || has("bug")
        || has("issue")
        || has("problem")
    {
        ("Happy to look. What's going wrong, and where? A page, a file or an error message helps.", Vec::new())
    } else if has("better") || has("improve") || has("faster") || has("speed") || has("nicer") {
        (
            "What should get better: how it looks, how fast it is, or how it works?",
            vec!["How it looks".into(), "Speed".into(), "How it works".into()],
        )
    } else {
        (
            "Which one, and what should change? Name the thing, and I'll take it from there.",
            Vec::new(),
        )
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
    if starts_like_a_question(&ws) {
        return true;
    }
    // A long message that ends in a question is judged by that question:
    // "the CSV loader crashes on empty files, can you fix that?" asks for work.
    text.trim_end().ends_with('?') && {
        let last = last_sentence(text);
        let lw = words(last);
        starts_like_a_question(&lw) && !asks_for_work(last)
    }
}

/// The last sentence or clause of a message.
fn last_sentence(text: &str) -> &str {
    let t = text.trim().trim_end_matches(['?', '!', '.', ' ']);
    t.rsplit(['.', '!', '?', '\n', ','])
        .map(str::trim)
        .find(|s| !s.is_empty())
        .unwrap_or(t)
}

/// Whether the words open like a question ("what", "how", "is it", "can you
/// help me word…").
fn starts_like_a_question(ws: &[String]) -> bool {
    const STARTS: &[&str] = &[
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
        "wher",
        "wat",
        "whats",
        "hows",
        "whos",
        "what's",
        "how's",
        "where's",
        "if",
        "would",
        "will",
        "whats",
    ];
    ws.first().is_some_and(|f| STARTS.contains(&f.as_str()))
}

/// Verbs that start a request for work, even when it's put as a question
/// ("can you fix the login?").
const DO: &[&str] = &[
    "fix",
    "add",
    "build",
    "make",
    "change",
    "update",
    "write",
    "run",
    "test",
    "delete",
    "remove",
    "deploy",
    "push",
    "merge",
    "create",
    "refactor",
    "bump",
    "rename",
    "move",
    "clean",
    "upgrade",
    "install",
    "set",
    "migrate",
    "implement",
    "sort",
    "speed",
    "drop",
    "rewrite",
    "convert",
    "port",
    "review",
    "debug",
    "improve",
    "add",
    "wipe",
    "squash",
    "rotate",
    "send",
    "email",
    "ship",
];

/// Whether a message asks for work, after any "hi", "please" or "can you".
pub fn asks_for_work(text: &str) -> bool {
    const LEAD: &[&str] = &[
        "hi", "hey", "hello", "peek", "please", "pls", "plz", "can", "could", "would", "will",
        "you", "u", "quickly", "just", "quick", "one", "ok", "so", "and", "also", "then",
    ];
    // Each sentence or clause, and whatever follows "can you" or "please".
    let lower = text.to_lowercase();
    let mut starts: Vec<&str> = lower.split(['.', '!', '?', '\n', ',', ';']).collect();
    for cue in [
        "can you ",
        "could you ",
        "can u ",
        "would you ",
        "please ",
        "pls ",
    ] {
        starts.extend(lower.match_indices(cue).map(|(i, _)| &lower[i..]));
    }
    starts.iter().any(|part| {
        words(part)
            .iter()
            .find(|w| !LEAD.contains(&w.as_str()))
            .is_some_and(|w| DO.contains(&w.as_str()))
    })
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
        Front::Stop => "stop",
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
    fn stop_app_stops_what_is_running() {
        for t in [
            "stop app",
            "stop",
            "Stop the server",
            "pls kill the app now",
            "cancel that",
            "stop everything",
            "shut it down",
        ] {
            assert_eq!(read0(t), Front::Stop, "{t}");
        }
        for t in [
            "stop the app from crashing on login",
            "add a stop button to the player",
            "the stop button does nothing",
            "why did the server stop?",
            "close the modal when you click outside",
        ] {
            assert_ne!(read0(t), Front::Stop, "{t}");
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
