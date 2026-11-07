//! What Peek learns from the person's engine, kept small.
//!
//! When the engine reads a message ([`super::think`]), its decision is a
//! good example of what Peek should do with words like those. Peek keeps
//! those examples, and only those, in one small file on this computer. When
//! the engine can't run (offline, a plan's limit, a slow reply), the closest
//! example it kept decides, before the keyword rules guess. The same file is
//! what a later on-device training run learns from (`squad brain export`).
//!
//! It never grows without bound: the same message, however typed, is one
//! example that counts its uses; long pastes and anything that looks like a
//! secret are never kept; and past [`MAX`] examples the least useful go.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::front::Front;
use super::route::{self, Route};
use super::words;
use crate::v2::data::{Member, Mode};

/// The most examples kept. About 300 bytes each: under 1 MB on disk.
pub const MAX: usize = 3000;
/// The longest message worth keeping, in characters.
const LONGEST: usize = 400;
/// How alike two messages must be, by shared words, for one to stand in for
/// the other.
const ALIKE: f32 = 0.7;

/// One decision the engine made.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Example {
    /// The message, trimmed.
    pub text: String,
    /// reply, ask, stop, answer or hand.
    pub r#do: String,
    /// Peek's words: the reply, the question, or the hand-off line.
    pub say: String,
    /// The bot it went to, and the kind of work, for a hand-off.
    pub bot: String,
    pub kind: String,
    /// How many times this message came up.
    pub hits: u32,
    /// When it last came up, in seconds since 1970.
    pub at: u64,
}

fn path() -> PathBuf {
    crate::store::data_dir().join("peek-learned.jsonl")
}

/// A message's words in order, as one key: "Who are you?" and "who are
/// you" are the same example.
fn key(text: &str) -> String {
    words(text).join(" ")
}

/// Whether a message is worth keeping: short, and nothing in it that looks
/// like a key, a token or a password.
pub fn keepable(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() || t.chars().count() > LONGEST {
        return false;
    }
    let lower = t.to_lowercase();
    let secretish = [
        "sk-",
        "ghp_",
        "gho_",
        "xoxb-",
        "xoxp-",
        "akia",
        "-----begin",
        "password",
        "passwd",
        "token=",
        "api_key",
        "apikey",
    ]
    .iter()
    .any(|s| lower.contains(s));
    // A long unbroken run of letters and digits is likely a key.
    let blob = t.split_whitespace().any(|w| {
        w.len() >= 32
            && w.chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_=+/".contains(c))
    });
    !secretish && !blob
}

/// The examples kept so far.
pub fn load() -> Vec<Example> {
    load_from(&path())
}

fn load_from(p: &std::path::Path) -> Vec<Example> {
    std::fs::read_to_string(p)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

fn save_to(p: &std::path::Path, all: &[Example]) {
    let body: String = all
        .iter()
        .filter_map(|e| serde_json::to_string(e).ok())
        .map(|l| l + "\n")
        .collect();
    let tmp = p.with_extension("tmp");
    if std::fs::write(&tmp, body).is_ok() {
        let _ = std::fs::rename(&tmp, p);
    }
}

/// Adds `e` to `all`: the same message updates its example and counts the
/// use; past `MAX`, the least used and oldest go.
pub fn merge(all: &mut Vec<Example>, mut e: Example) {
    let k = key(&e.text);
    if let Some(old) = all.iter_mut().find(|o| key(&o.text) == k) {
        e.hits = old.hits + 1;
        *old = e;
    } else {
        e.hits = 1;
        all.push(e);
    }
    if all.len() > MAX {
        // Keep the most used, then the most recent.
        all.sort_by(|a, b| b.hits.cmp(&a.hits).then(b.at.cmp(&a.at)));
        all.truncate(MAX);
    }
}

/// Keeps what the engine decided about `text`. Quietly does nothing for a
/// message not worth keeping, or when learning is off.
pub fn keep(st: &crate::settings::AppSettings, text: &str, front: &Front, say: &str) {
    if !on(st) || !keepable(text) {
        return;
    }
    let Some(e) = example(text, front, say) else {
        return;
    };
    let p = path();
    let mut all = load_from(&p);
    merge(&mut all, e);
    save_to(&p, &all);
}

/// The example for a decision, or None for one not worth repeating.
pub fn example(text: &str, front: &Front, say: &str) -> Option<Example> {
    let (d, bot, kind) = match front {
        Front::Chat { .. } => ("reply", String::new(), String::new()),
        Front::AskBack { .. } => ("ask", String::new(), String::new()),
        Front::Stop => ("stop", String::new(), String::new()),
        Front::Answer { .. } => ("answer", String::new(), String::new()),
        Front::Hand(r) | Front::Confirm { route: r, .. } => (
            "hand",
            r.bot.name().to_string(),
            r.mode.label().to_lowercase(),
        ),
    };
    Some(Example {
        text: text.trim().to_string(),
        r#do: d.into(),
        say: say.trim().chars().take(300).collect(),
        bot,
        kind,
        hits: 1,
        at: crate::v2::store::now(),
    })
}

/// Shared words over all words: 1 for the same words, 0 for none alike.
fn alike(a: &[String], b: &[String]) -> f32 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let shared = a.iter().filter(|w| b.contains(w)).count() as f32;
    let all = (a.len() + b.len()) as f32 - shared;
    shared / all
}

/// The closest kept example to `text`, if one is alike enough.
pub fn closest<'a>(text: &str, all: &'a [Example]) -> Option<&'a Example> {
    let ws = words(text);
    all.iter()
        .map(|e| (e, alike(&ws, &words(&e.text))))
        .filter(|(_, s)| *s >= ALIKE)
        .max_by(|a, b| a.1.total_cmp(&b.1).then(a.0.hits.cmp(&b.0.hits)))
        .map(|(e, _)| e)
}

/// What Peek does with `text` from what the engine decided before about
/// words like it, with the guard rails on top. None when nothing kept is
/// close enough; the rules decide then.
pub fn recall(text: &str, all: &[Example], crew: &[Member]) -> Option<(Front, String)> {
    let e = closest(text, all)?;
    let front = match e.r#do.as_str() {
        "reply" if !e.say.is_empty() => Front::Chat {
            reply: e.say.clone(),
        },
        "ask" if !e.say.is_empty() => Front::AskBack {
            question: e.say.clone(),
            choices: Vec::new(),
        },
        "stop" => Front::Stop,
        "answer" => Front::Answer {
            question: text.to_string(),
            code: true,
        },
        "hand" => {
            let mode = match e.kind.as_str() {
                "fix" => Mode::Fix,
                "test" => Mode::Test,
                "review" => Mode::Review,
                "tidy" => Mode::Tidy,
                "ask" => Mode::Ask,
                _ => Mode::Build,
            };
            let bot = crew
                .iter()
                .copied()
                .find(|m| *m != Member::Fin && m.name().eq_ignore_ascii_case(&e.bot))
                .unwrap_or_else(|| route::doer(mode, crew));
            let r = Route {
                mode,
                bot,
                sure: 0.8,
                why: e.say.clone(),
                by: "learned".into(),
            };
            match super::guard::risk(text) {
                Some(why) => Front::Confirm { why, route: r },
                None => Front::Hand(r),
            }
        }
        _ => return None,
    };
    Some((front, e.say.clone()))
}

/// Whether Peek keeps what the engine decides: on unless the person turned
/// it off (`peek_learns: false` in settings).
pub fn on(st: &crate::settings::AppSettings) -> bool {
    st.extra
        .get("peek_learns")
        .and_then(|v| v.as_bool())
        .unwrap_or(true)
}

/// Forgets everything Peek learned.
pub fn forget() -> std::io::Result<()> {
    match std::fs::remove_file(path()) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ex(text: &str, d: &str, say: &str) -> Example {
        Example {
            text: text.into(),
            r#do: d.into(),
            say: say.into(),
            at: 1,
            ..Default::default()
        }
    }

    #[test]
    fn the_same_message_is_one_example() {
        let mut all = Vec::new();
        merge(&mut all, ex("Who are you?", "reply", "I'm Peek."));
        merge(
            &mut all,
            ex("who are you", "reply", "I'm Peek, your crew lead."),
        );
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].hits, 2);
        assert_eq!(all[0].say, "I'm Peek, your crew lead.");
    }

    #[test]
    fn it_never_grows_past_its_cap() {
        let mut all = Vec::new();
        merge(&mut all, ex("ship it", "hand", "Shipping."));
        merge(&mut all, ex("ship it", "hand", "Shipping."));
        for i in 0..MAX + 50 {
            merge(&mut all, ex(&format!("message number {i}"), "reply", "ok"));
        }
        assert_eq!(all.len(), MAX);
        // The one used twice stays.
        assert!(all.iter().any(|e| e.text == "ship it"));
    }

    #[test]
    fn secrets_and_long_pastes_are_never_kept() {
        assert!(keepable("whoa re you"));
        assert!(!keepable("use sk-ant-api03-abcdef for the key"));
        assert!(!keepable("my password is hunter2"));
        assert!(!keepable(&"x".repeat(500)));
        assert!(!keepable("set it to AbCdEf0123456789AbCdEf0123456789xyz"));
    }

    #[test]
    fn a_close_message_is_decided_like_before() {
        let crew = Member::CREW.to_vec();
        let all = vec![
            ex("whoa re you", "reply", "I'm Peek, I run your crew."),
            Example {
                text: "the checkout page crashes on safari".into(),
                r#do: "hand".into(),
                say: "A bug, so Fixer's on it.".into(),
                bot: "Fixer".into(),
                kind: "fix".into(),
                ..Default::default()
            },
        ];
        match recall("whoa re you", &all, &crew) {
            Some((Front::Chat { reply }, _)) => assert!(reply.contains("Peek")),
            other => panic!("{other:?}"),
        }
        match recall("the checkout page crashes on safari again", &all, &crew) {
            Some((Front::Hand(r), _)) => {
                assert_eq!(r.bot, Member::Fixer);
                assert_eq!(r.by, "learned");
            }
            other => panic!("{other:?}"),
        }
        assert!(recall("add a pricing page", &all, &crew).is_none());
    }
}
