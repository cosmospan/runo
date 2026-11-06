//! Which bot a typed sentence goes to, and what kind of job it is.
//!
//! The rules score each kind of job by the words in the sentence ("crash",
//! "stack trace" and "doesn't work" lean to Fix; "why" and a question mark lean
//! to Ask) and pick the highest, with Build when nothing stands out. A bot the
//! person made wins over the built-in one when its job description shares
//! words with the sentence, so a "Docs" bot gets "update the README".

use super::super::data::{Member, Mode};
use super::{stem_match, words};

/// Where a sentence goes, how sure the guess is, and why.
#[derive(Clone, Debug, PartialEq)]
pub struct Route {
    pub mode: Mode,
    pub bot: Member,
    /// 0 to 1. Under [`Route::SURE`] a client should offer the pick rather
    /// than just make it.
    pub sure: f32,
    /// "\u{201c}crash\u{201d} and \u{201c}error\u{201d} sound like a bug", for a tooltip.
    pub why: String,
    /// Where the answer came from: "rules" or the local model's name.
    pub by: String,
}

impl Route {
    pub const SURE: f32 = 0.6;

    /// Peek's line as it hands the job on: "Sounds like a bug, so I'm
    /// bringing in Fixer to hunt it down."
    pub fn handoff(&self) -> String {
        let name = self.bot.name();
        if self.bot.is_made() {
            return format!("This one's right up {name}'s street, so I'm handing it over.");
        }
        match self.mode {
            Mode::Build => format!("This is a build job, so I'm bringing in {name}."),
            Mode::Fix => format!("Sounds like a bug, so I'm bringing in {name} to hunt it down."),
            Mode::Test => format!("Let's try it for real. {name}, you're up."),
            Mode::Review => format!("This wants a careful second look, so I'm bringing in {name}."),
            Mode::Tidy => format!("Cleanup time. {name} is taking this one."),
            Mode::Ask => format!("Good question. {name} will dig into the code and answer."),
        }
    }
}

/// Words and phrases that lean a sentence to each kind of job, with weights.
/// Single words match by stem ("crash" matches "crashed"); phrases match
/// anywhere in the lowercased sentence.
const CUES: [(Mode, &[(&str, f32)]); 6] = [
    (
        Mode::Fix,
        &[
            ("fix", 3.0),
            ("bug", 3.0),
            ("crash", 3.0),
            ("error", 2.5),
            ("broken", 3.0),
            ("break", 1.5),
            ("fail", 2.0),
            ("exception", 3.0),
            ("panic", 2.5),
            ("traceback", 3.0),
            ("stack trace", 3.0),
            ("doesn't work", 3.0),
            ("does not work", 3.0),
            ("not working", 3.0),
            ("regression", 3.0),
            ("wrong", 1.5),
            ("undefined", 2.0),
            ("null", 1.5),
            ("500", 1.5),
            ("hang", 1.5),
            ("leak", 2.0),
            ("slow", 1.5),
        ],
    ),
    (
        Mode::Test,
        &[
            ("test", 2.5),
            ("tests", 2.5),
            ("coverage", 3.0),
            ("e2e", 3.0),
            ("verify", 2.0),
            ("try the", 1.5),
            ("run the checks", 3.0),
            ("qa", 2.5),
            ("smoke", 2.0),
            ("check that", 2.0),
            ("make sure", 1.5),
        ],
    ),
    (
        Mode::Review,
        &[
            ("review", 3.0),
            ("audit", 3.0),
            ("security", 2.5),
            ("risk", 2.5),
            ("vulnerab", 3.0),
            ("secret", 2.0),
            ("look over", 2.5),
            ("before i merge", 3.0),
            ("before merging", 3.0),
            ("safe to", 2.0),
            ("uncommitted", 1.5),
        ],
    ),
    (
        Mode::Tidy,
        &[
            ("refactor", 3.0),
            ("tidy", 3.0),
            ("clean", 2.5),
            ("rename", 2.5),
            ("dead code", 3.0),
            ("unused", 2.5),
            ("dependenc", 2.5),
            ("deps", 2.5),
            ("upgrade", 2.0),
            ("bump", 2.5),
            ("lint", 2.5),
            ("format", 1.5),
            ("flaky", 2.5),
            ("duplicat", 2.0),
            ("simplify", 2.0),
            ("chore", 2.5),
        ],
    ),
    (
        Mode::Ask,
        &[
            ("what", 1.5),
            ("why", 2.0),
            ("how does", 2.5),
            ("how do", 2.0),
            ("where", 1.5),
            ("which", 1.0),
            ("explain", 3.0),
            ("walk me through", 3.0),
            ("tell me", 2.0),
            ("understand", 2.0),
            ("is there", 1.5),
            ("are there", 1.5),
            ("can you show", 1.5),
        ],
    ),
    (
        Mode::Build,
        &[
            ("add", 2.0),
            ("build", 2.5),
            ("create", 2.0),
            ("implement", 3.0),
            ("make", 1.5),
            ("new", 1.0),
            ("feature", 2.5),
            ("support", 1.5),
            ("page", 1.0),
            ("endpoint", 1.5),
            ("screen", 1.0),
            ("button", 1.0),
            ("write", 1.0),
            ("integrate", 2.0),
        ],
    ),
];

/// Scores for each kind of job, and the cues that matched, by rule.
fn scores(text: &str) -> Vec<(Mode, f32, Vec<&'static str>)> {
    let lower = text.to_lowercase();
    let ws = words(text);
    let mut out: Vec<(Mode, f32, Vec<&'static str>)> = CUES
        .iter()
        .map(|(mode, cues)| {
            let mut score = 0.0;
            let mut hit = Vec::new();
            for (cue, weight) in cues.iter() {
                let found = if cue.contains(' ') || cue.contains('\'') {
                    lower.contains(cue)
                } else {
                    ws.iter().any(|w| stem_match(w, cue))
                };
                if found {
                    score += weight;
                    hit.push(*cue);
                }
            }
            (*mode, score, hit)
        })
        .collect();
    // A question is an Ask unless it's clearly asking for work ("can you add…?").
    if lower.trim_end().ends_with('?') {
        if let Some(ask) = out.iter_mut().find(|s| s.0 == Mode::Ask) {
            ask.1 += 2.0;
            ask.2.push("?");
        }
    }
    // A pasted error or stack trace leans hard to Fix.
    let pasted = lower.contains("error:")
        || lower.contains("at line")
        || lower.contains("    at ")
        || lower.contains(".rs:")
        || lower.contains(".py\", line")
        || lower.contains(".ts:");
    if pasted {
        if let Some(fix) = out.iter_mut().find(|s| s.0 == Mode::Fix) {
            fix.1 += 3.0;
            fix.2.push("a pasted error");
        }
    }
    out
}

/// Plain words for why this kind of job won.
fn because(mode: Mode, hit: &[&str]) -> String {
    let kind = match mode {
        Mode::Ask => "a question about the code",
        Mode::Build => "something new to build",
        Mode::Fix => "a bug to fix",
        Mode::Review => "a review",
        Mode::Test => "something to test",
        Mode::Tidy => "a tidy-up",
    };
    let shown: Vec<String> = hit
        .iter()
        .take(2)
        .map(|c| match *c {
            "?" => "the question mark".to_string(),
            "a pasted error" => "the pasted error".to_string(),
            c => format!("\u{201c}{c}\u{201d}"),
        })
        .collect();
    match shown.len() {
        0 => format!("Nothing stood out, so it's treated as {kind}."),
        1 => format!("{} sounds like {kind}.", cap(&shown[0])),
        _ => format!("{} and {} sound like {kind}.", cap(&shown[0]), shown[1]),
    }
}

fn cap(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// Words too common to tie a sentence to a made bot: glue words, and the
/// tools and kinds of work nearly every job names.
pub const COMMON: &[&str] = &[
    "the",
    "a",
    "an",
    "and",
    "or",
    "for",
    "to",
    "of",
    "in",
    "on",
    "it",
    "is",
    "my",
    "our",
    "your",
    "this",
    "that",
    "with",
    "bot",
    "job",
    "jobs",
    "code",
    "all",
    "any",
    "app",
    "api",
    "npm",
    "javascript",
    "typescript",
    "node",
    "python",
    "rust",
    "project",
    "file",
    "files",
    "repo",
    "test",
    "tests",
    "build",
    "builds",
    "fix",
    "fixes",
    "bug",
    "bugs",
    "run",
    "runs",
    "add",
    "make",
    "update",
    "change",
    "changes",
    "version",
    "bump",
    "new",
    "feature",
    "error",
    "errors",
];

/// Whether a made bot of kind `theirs` should take a job of kind `job`:
/// the same kind, or making and fixing, which are close.
fn same_kind(theirs: Mode, job: Mode) -> bool {
    theirs == job
        || matches!(
            (theirs, job),
            (Mode::Build, Mode::Fix) | (Mode::Fix, Mode::Build)
        )
}

fn plain_words(text: &str) -> Vec<String> {
    words(text)
        .into_iter()
        .filter(|w| w.len() > 2 && !COMMON.contains(&w.as_str()))
        .collect()
}

/// The same word, give or take an ending: "docs" and "doc", "pricing" and
/// "price", but not "read" and "readme".
fn same_word(a: &str, b: &str) -> bool {
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    if short == long {
        return true;
    }
    let short = short.strip_suffix('e').unwrap_or(short);
    short.len() >= 3
        && long.strip_prefix(short).is_some_and(|rest| {
            [
                "s", "es", "ed", "d", "ing", "er", "ers", "ation", "ations", "ment", "ments",
            ]
            .contains(&rest)
        })
}

/// How well a sentence's words `ws` fit a made bot, and the word that tied
/// them. Its name or what it's for counts double, a skill once; one skill
/// alone isn't enough, since skills often name the whole project's subject.
fn fit(ws: &[String], name_role: &str, skills: &str) -> Option<(usize, String)> {
    let shares = |theirs: &[String], w: &String| theirs.iter().any(|t| same_word(w, t));
    let named = plain_words(name_role);
    let skilled = plain_words(skills);
    let mut score = 0;
    let mut word: Option<String> = None;
    for w in ws {
        let n = if shares(&named, w) {
            2
        } else if shares(&skilled, w) {
            1
        } else {
            0
        };
        if n > 0 {
            score += n;
            word.get_or_insert_with(|| w.clone());
        }
    }
    (score >= 2).then(|| (score, word.unwrap_or_default()))
}

/// A made bot of a fitting kind whose name or job, or two of its skills,
/// share words with the sentence, if any.
fn made_bot_for(text: &str, crew: &[Member], mode: Mode) -> Option<(Member, String)> {
    let ws = plain_words(text);
    let mut best: Option<(Member, usize, String)> = None;
    for m in crew
        .iter()
        .copied()
        .filter(|m| m.is_made() && same_kind(m.mode(), mode))
    {
        let name_role = format!("{} {}", m.name(), m.role());
        if let Some((score, word)) = fit(&ws, &name_role, m.skills()) {
            if best.as_ref().is_none_or(|b| score > b.1) {
                best = Some((m, score, word));
            }
        }
    }
    best.map(|(m, _, w)| (m, w))
}

/// The rules' pick for `text` among `crew` (the bots on the person's crew).
pub fn by_rules(text: &str, crew: &[Member]) -> Route {
    let mut s = scores(text);
    s.sort_by(|a, b| b.1.total_cmp(&a.1));
    let (mut mode, top, hit) = s[0].clone();
    let second = s.get(1).map(|x| x.1).unwrap_or(0.0);
    if top <= 0.0 {
        mode = Mode::Build;
    }
    // How far ahead the winner is: alone at 3 points is sure, a tie is a coin flip.
    let sure = if top <= 0.0 {
        0.3
    } else {
        let lead = (top - second) / top;
        (0.35 + 0.45 * lead + 0.05 * top.min(4.0)).min(0.97)
    };
    let mut why = because(mode, &hit);
    let bot = match made_bot_for(text, crew, mode) {
        Some((m, word)) => {
            why = format!("{} is your bot for \u{201c}{word}\u{201d}. {why}", m.name());
            m
        }
        None => doer(mode, crew),
    };
    Route {
        mode,
        bot,
        sure,
        why,
        by: "rules".into(),
    }
}

/// The bot on `crew` that takes jobs of this kind: the built-in one if it's
/// there, else a made bot that does that work, else the built-in one anyway.
pub fn doer(mode: Mode, crew: &[Member]) -> Member {
    let own = Member::for_mode(mode);
    if crew.contains(&own) {
        return own;
    }
    crew.iter()
        .copied()
        .find(|m| *m != Member::Shipper && m.mode() == mode)
        .unwrap_or(own)
}

/// The built-in tiny classifier's pick, as a route. Made bots still win:
/// the rules match those by name.
pub fn by_tiny(text: &str, crew: &[Member]) -> Route {
    let (mode, sure) = super::tiny::classify(text);
    let why = format!("It reads like a {} job.", mode.label().to_lowercase());
    // A bot the person made for the subject takes it, when it does this kind of work.
    let (bot, why) = match made_bot_for(text, crew, mode) {
        Some((m, word)) => (
            m,
            format!("{} is your bot for \u{201c}{word}\u{201d}. {why}", m.name()),
        ),
        None => (doer(mode, crew), why),
    };
    Route {
        mode,
        bot,
        sure,
        why,
        by: "peek-tiny".into(),
    }
}

/// Without a model: the tiny classifier when it's sure, since it reads the
/// whole sentence; else the keyword rules when they're sure; else the tiny
/// classifier's best guess. 117 of the 120 messy test sentences and all 53
/// plain ones, against 70 and 51 for the rules alone.
pub fn built_in(text: &str, crew: &[Member]) -> Route {
    let tiny = by_tiny(text, crew);
    if tiny.sure >= Route::SURE {
        return tiny;
    }
    let rules = by_rules(text, crew);
    if rules.sure >= Route::SURE {
        return rules;
    }
    tiny
}

/// Where `text` goes: the built-in pick when it's sure; else, when Smart Peek
/// is on, the local model, if its answer makes sense.
pub fn route(st: &crate::settings::AppSettings, text: &str) -> Route {
    let crew = Member::all();
    let mine = built_in(text, &crew);
    // The model is only worth its CPU when the built-in pick isn't sure.
    if mine.sure >= Route::SURE {
        return mine;
    }
    if let Some(m) = super::model::Local::from_settings(st) {
        // Squad's own server starts on the first unsure job; while it loads,
        // the built-in pick stands.
        if super::helper::applies(st) && !super::helper::ensure(st, std::time::Duration::ZERO) {
            return mine;
        }
        if let Some(mut r) = m.route(text, &crew) {
            // The model knows only the built-in crew; a bot the person made
            // for the subject still takes it.
            if let Some((made, word)) = made_bot_for(text, &crew, r.mode) {
                r.why = format!(
                    "{} is your bot for \u{201c}{word}\u{201d}. {}",
                    made.name(),
                    r.why
                );
                r.bot = made;
            }
            return r;
        }
    }
    mine
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crew() -> Vec<Member> {
        Member::CREW.to_vec()
    }

    fn mode_of(text: &str) -> Mode {
        by_rules(text, &crew()).mode
    }

    #[test]
    fn one_project_word_in_the_skills_is_not_enough() {
        let docs = ("Docs", "Keeps the readme and docs current");
        let skills = "readme, docs, cart, subtotal, pricing";
        let fits = |t: &str| fit(&plain_words(t), &format!("{} {}", docs.0, docs.1), skills);
        assert!(fits("refactor subtotal so it's easier to read").is_none());
        assert!(fits("clean up the pricing helpers").is_none());
        assert!(fits("update the readme for the new install steps").is_some());
        assert!(fits("document cart pricing").is_some());
        assert!(same_word("docs", "doc") && same_word("price", "pricing"));
        assert!(!same_word("read", "readme"));
    }

    #[test]
    fn the_handoff_names_the_bot_and_why() {
        let r = by_rules("the export button crashes on safari", &crew());
        let h = r.handoff();
        assert!(h.contains(r.bot.name()) && h.contains("bug"), "{h}");
    }

    #[test]
    fn a_made_bot_takes_only_its_kind_of_work() {
        // A docs writer (a Build bot) can fix docs, but not run tests or tidy.
        assert!(same_kind(Mode::Build, Mode::Fix));
        assert!(same_kind(Mode::Test, Mode::Test));
        assert!(!same_kind(Mode::Build, Mode::Test));
        assert!(!same_kind(Mode::Build, Mode::Tidy));
        assert!(!same_kind(Mode::Ask, Mode::Build));
        for w in ["npm", "javascript", "api", "code", "app", "bump"] {
            assert!(COMMON.contains(&w), "{w}");
        }
    }

    #[test]
    fn plain_sentences_go_to_the_right_kind_of_job() {
        assert_eq!(mode_of("The app crashes when I log out"), Mode::Fix);
        assert_eq!(mode_of("add a dark mode toggle to settings"), Mode::Build);
        assert_eq!(mode_of("How does the login flow work?"), Mode::Ask);
        assert_eq!(mode_of("review my changes before I merge"), Mode::Review);
        assert_eq!(
            mode_of("bump the dependencies and remove dead code"),
            Mode::Tidy
        );
        assert_eq!(mode_of("increase test coverage for the parser"), Mode::Test);
    }

    #[test]
    fn a_pasted_error_is_a_fix() {
        let r = by_rules(
            "TypeError: cannot read property 'id' of undefined\n    at render (app.ts:42)",
            &crew(),
        );
        assert_eq!(r.mode, Mode::Fix);
        assert_eq!(r.bot, Member::Fixer);
        assert!(r.sure >= Route::SURE, "sure = {}", r.sure);
    }

    #[test]
    fn nothing_to_go_on_is_an_unsure_build() {
        let r = by_rules("the onboarding", &crew());
        assert_eq!(r.mode, Mode::Build);
        assert!(r.sure < Route::SURE);
    }

    #[test]
    fn a_missing_bot_hands_over_to_one_that_does_that_work() {
        // A crew without Fixer still gets fixes done by the Builder default.
        let crew = vec![Member::Builder, Member::Explainer];
        let r = by_rules("fix the crash on save", &crew);
        assert_eq!(r.mode, Mode::Fix);
        assert_eq!(r.bot, Member::Fixer, "falls back to the built-in kind");
    }

    #[test]
    fn says_why_in_plain_words() {
        let r = by_rules("fix the crash on save", &crew());
        assert!(r.why.contains("bug to fix"), "{}", r.why);
    }
}
