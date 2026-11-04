//! Squad Code's own intelligence layer: the small decisions the crew makes for
//! itself, without asking a coding agent.
//!
//! - [`route`]: which bot, and what kind of job, a sentence the person typed is.
//! - [`place`]: whether a job runs on this computer or on the person's server,
//!   and on which engine, with the reason said in plain words.
//! - [`ideas`]: a bot worth making, when the same kind of job keeps coming up
//!   and no bot on the crew is for it. The person decides; nothing is made for them.
//! - [`fin`]: what Fin says when something happens.
//!
//! Every decision has two tiers. The rules here always work, need no download
//! and are the answer when anything else fails. When the person turns on Smart
//! Fin, a small model running on their own computer ([`model`]) is asked first,
//! and its answer is used only if it parses into something the rules would
//! accept (a mode that exists, a bot on the crew). Facts (what's running, which
//! engines are signed in) always come from Squad Code itself, never the model.
//!
//! [`teach`] turns the person's own finished jobs into training examples for
//! that model, in the same prompt format the model is asked in.

pub mod cli;
pub mod fin;
pub mod ideas;
pub mod model;
pub mod place;
pub mod route;
pub mod teach;

pub use place::{place, JobAsk, Site, Sites};

/// Words in `text`, lowercased, split on anything that isn't a letter, digit,
/// `'` or `-`.
pub(crate) fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !(c.is_alphanumeric() || c == '\'' || c == '-'))
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

/// Whether `w` is `stem` or starts with it (so "crash" matches "crashes").
pub(crate) fn stem_match(w: &str, stem: &str) -> bool {
    w == stem || (stem.len() >= 4 && w.starts_with(stem))
}
