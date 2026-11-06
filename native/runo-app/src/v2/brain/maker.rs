//! Setting up a new bot: what it's for, the kind of work it does, what it knows
//! best and its rules.
//!
//! Made by hand, the person can set any of that, and what they leave out is
//! filled in here. Two passes:
//! 1. [`draft`], at once and built in: the kind of work from what the bot is
//!    for, its skills from the words that name its subject, and a safe set of
//!    rules for that kind.
//! 2. [`design_prompt`], asked of the crew's engine in the project's folder:
//!    it reads the project and writes rules and skills that fit it (the test
//!    command, the package manager, folders to leave alone). [`parse_design`]
//!    checks the answer before it replaces the draft.
//!
//! The skills are what routing matches a new job against, so a job about
//! Stripe goes to the bot that knows Stripe.

use std::sync::mpsc::{channel, Receiver};

use serde_json::Value;

use crate::agent::{self, Event};
use crate::settings::AppSettings;

use super::super::data::{Member, Mode};
use super::{route, words};

/// A bot's setup, before it joins the crew.
#[derive(Clone, Debug, PartialEq)]
pub struct Design {
    pub role: String,
    pub like: Member,
    pub skills: Vec<String>,
    pub rules: Vec<String>,
}

/// Words that say nothing about a bot's subject.
const PLAIN: [&str; 48] = [
    "the", "a", "an", "and", "or", "for", "to", "of", "in", "on", "it", "is", "my", "our", "your",
    "this", "that", "with", "bot", "job", "jobs", "all", "any", "who", "what", "when", "make",
    "makes", "keep", "keeps", "handle", "handles", "look", "looks", "after", "every", "things",
    "stuff", "work", "works", "fix", "fixes", "write", "writes", "code", "tests", "test", "up",
];

/// The words that name what a bot is about: "Stripe Pro: payments, Stripe
/// webhooks and billing bugs" → stripe, pro, payments, webhooks, billing, bugs.
pub fn skill_words(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for w in words(text) {
        if w.len() < 3 || PLAIN.contains(&w.as_str()) || w.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        if !out.contains(&w) {
            out.push(w);
        }
    }
    out.truncate(10);
    out
}

/// A safe starting set of rules for each kind of work.
pub fn rules_for(like: Member) -> Vec<String> {
    let r: &[&str] = match like.mode() {
        Mode::Fix => &[
            "Find the cause before changing code, and show the evidence.",
            "Add a test that fails without the fix.",
            "Keep the change to what the bug needs.",
        ],
        Mode::Build => &[
            "Read how the project already does similar things, and match it.",
            "Add tests for what you build.",
            "Ask before adding a new dependency.",
        ],
        Mode::Test => &[
            "Run the project's own test command, and report what passed and what broke.",
            "Never skip, disable or loosen a test to make it pass.",
        ],
        Mode::Review => &[
            "Change nothing: read and report.",
            "Lead with the riskiest thing you found, with the file and line.",
        ],
        Mode::Tidy => &[
            "Change how the code reads, never what it does.",
            "Run the tests before and after.",
        ],
        Mode::Ask => &[
            "Change nothing: answer from the code, with the files you read.",
            "Use plain words for people who may not code.",
        ],
    };
    r.iter().map(|s| s.to_string()).collect()
}

/// The built-in setup for a bot named `name` that's for `role`. `like` is the
/// kind of work the person picked, or None to work it out from the role.
pub fn draft(name: &str, role: &str, like: Option<Member>) -> Design {
    let about = if role.trim().is_empty() { name } else { role };
    let like = like.unwrap_or_else(|| {
        // A bot's job reads like a description ("fixes billing bugs"), where
        // the keyword rules do better than the classifier trained on requests.
        let about = base_forms(about);
        let rules = route::by_rules(&about, &[]);
        let mode = if rules.sure >= route::Route::SURE {
            rules.mode
        } else {
            route::built_in(&about, &[]).mode
        };
        Member::for_mode(mode)
    });
    let role = if role.trim().is_empty() {
        like.role().to_string()
    } else {
        role.trim().to_string()
    };
    Design {
        skills: skill_words(&format!("{name} {role}")),
        rules: rules_for(like),
        role,
        like,
    }
}

/// "fixes billing bugs" → "fix billing bug": a description's verbs and plurals
/// in the form the routing rules know.
fn base_forms(text: &str) -> String {
    words(text)
        .into_iter()
        .map(|w| {
            if w.len() > 4
                && ["xes", "ches", "shes", "sses"]
                    .iter()
                    .any(|e| w.ends_with(e))
            {
                w[..w.len() - 2].to_string()
            } else if w.len() > 3 && w.ends_with('s') && !w.ends_with("ss") {
                w[..w.len() - 1].to_string()
            } else {
                w
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// What the crew's engine is asked, in the project's folder, to set a bot up.
pub fn design_prompt(name: &str, d: &Design, project: &str) -> String {
    format!(
        "Squad Code's crew has a new bot for the project \"{project}\" (the current folder). \
         Name: {name}. What it's for: {role}. Kind of work: {kind}.\n\n\
         Read the project first (its package or build files, CI config, tests, contributing notes), \
         then set the bot up to fit it. Change no files.\n\n\
         Reply with JSON only, nothing else:\n\
         {{\"role\": \"what it's for, one short line\", \
         \"skills\": [\"3 to 10 single lowercase words a job about its subject would use\"], \
         \"rules\": [\"3 to 6 short rules for how it works in this project, such as the exact test command, \
         tools to use, folders to leave alone, when to ask first\"]}}",
        role = d.role,
        kind = d.like.mode().label().to_lowercase(),
    )
}

/// The engine's setup, checked: a role, some skills and some rules, each
/// short. None when the reply doesn't hold them, so the draft stands.
pub fn parse_design(reply: &str) -> Option<(String, Vec<String>, Vec<String>)> {
    let start = reply.find('{')?;
    let end = reply.rfind('}')?;
    let v: Value = serde_json::from_str(reply.get(start..=end)?).ok()?;
    let role = v["role"].as_str()?.trim().trim_end_matches('.').to_string();
    let list = |k: &str, max_words: usize| -> Vec<String> {
        v[k].as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str())
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty() && s.split_whitespace().count() <= max_words)
                    .take(10)
                    .collect()
            })
            .unwrap_or_default()
    };
    let skills: Vec<String> = list("skills", 2)
        .into_iter()
        .map(|s| s.to_lowercase())
        .collect();
    let rules = list("rules", 40);
    (!role.is_empty()
        && role.split_whitespace().count() <= 20
        && !skills.is_empty()
        && !rules.is_empty())
    .then_some((role, skills, rules))
}

/// Asks the crew's engine, in `folder`, to set the bot up for the project, on
/// another thread. Err when no engine can run here; the draft stands.
pub fn design(
    st: &AppSettings,
    folder: &super::super::store::Folder,
    name: &str,
    d: &Design,
) -> Result<Receiver<Event>, String> {
    let i = crate::assistant::assistant_provider(st);
    let p = &agent::PROVIDERS[i];
    let configured = st.agent(p.id).binary;
    if !agent::available(p.bin, &configured) {
        return Err(format!("{} isn't installed", p.name));
    }
    let model = crate::assistant::assistant_model(st, p.id);
    let prompt = design_prompt(name, d, &folder.name);
    let command = agent::ask_command_on(p.id, &configured, &prompt, &model);
    let (tx, rx) = channel();
    agent::oneshot("v2-maker".into(), command, &folder.path, tx);
    Ok(rx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bot_for_bugs_works_like_fixer_and_knows_its_subject() {
        let d = draft("Stripe Pro", "fixes Stripe webhook and billing bugs", None);
        assert_eq!(d.like, Member::Fixer);
        assert!(d.skills.contains(&"stripe".to_string()), "{:?}", d.skills);
        assert!(d.skills.contains(&"billing".to_string()));
        assert!(!d.skills.contains(&"fixes".to_string()));
        assert!(!d.rules.is_empty());
    }

    #[test]
    fn the_persons_pick_wins() {
        let d = draft(
            "Scribe",
            "keeps the README up to date",
            Some(Member::Builder),
        );
        assert_eq!(d.like, Member::Builder);
        assert!(d.skills.contains(&"readme".to_string()));
    }

    #[test]
    fn a_review_bot_changes_nothing() {
        let d = draft("Lock", "reviews auth changes for security risks", None);
        assert_eq!(d.like, Member::Guard);
        assert!(d.rules[0].contains("Change nothing"));
    }

    #[test]
    fn the_engines_setup_is_checked() {
        let good = "Here you go:\n{\"role\": \"Fixes payment bugs.\", \"skills\": [\"Stripe\", \"billing\", \"a whole sentence is not a skill\"], \"rules\": [\"Run pnpm vitest before you stop.\"]}";
        let (role, skills, rules) = parse_design(good).unwrap();
        assert_eq!(role, "Fixes payment bugs");
        assert_eq!(skills, vec!["stripe", "billing"]);
        assert_eq!(rules, vec!["Run pnpm vitest before you stop."]);
        assert!(parse_design("{\"role\": \"x\", \"skills\": [], \"rules\": []}").is_none());
        assert!(parse_design("sorry").is_none());
    }
}
