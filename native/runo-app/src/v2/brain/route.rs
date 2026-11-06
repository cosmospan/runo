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

/// Words too common to tie a sentence to a made bot.
const COMMON: [&str; 24] = [
    "the", "a", "an", "and", "or", "for", "to", "of", "in", "on", "it", "is", "my", "our", "your",
    "this", "that", "with", "bot", "job", "jobs", "code", "all", "any",
];

/// A made bot whose name or job shares a word with the sentence, if any.
fn made_bot_for(text: &str, crew: &[Member]) -> Option<(Member, String)> {
    let ws: Vec<String> = words(text)
        .into_iter()
        .filter(|w| w.len() > 2 && !COMMON.contains(&w.as_str()))
        .collect();
    let mut best: Option<(Member, usize, String)> = None;
    for m in crew.iter().copied().filter(|m| m.is_made()) {
        let theirs: Vec<String> = words(&format!("{} {}", m.name(), m.role()))
            .into_iter()
            .filter(|w| w.len() > 2 && !COMMON.contains(&w.as_str()))
            .collect();
        let shared: Vec<&String> = ws
            .iter()
            .filter(|w| theirs.iter().any(|t| stem_match(w, t) || stem_match(t, w)))
            .collect();
        if !shared.is_empty() && best.as_ref().is_none_or(|b| shared.len() > b.1) {
            best = Some((m, shared.len(), shared[0].clone()));
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
    let bot = match made_bot_for(text, crew) {
        Some((m, word)) => {
            mode = m.mode();
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
    let rules = by_rules(text, crew);
    if made_bot_for(text, crew).is_some() {
        return rules;
    }
    let (mode, sure) = super::tiny::classify(text);
    Route {
        mode,
        bot: doer(mode, crew),
        sure,
        why: format!("It reads like a {} job.", mode.label().to_lowercase()),
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
        if let Some(r) = m.route(text, &crew) {
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
