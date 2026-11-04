//! Training examples for Squad's own model, from the person's own jobs.
//!
//! A job that went well (it was approved, shipped or answered) is an example
//! of where a sentence should go: the words the person typed, and the kind of
//! job and bot that did it. Each example is written in the chat format
//! fine-tuning tools read, with exactly the instructions the model is asked
//! with ([`super::model::route_system`]).
//!
//! Nothing leaves the computer: `squad brain export` writes a file, and the
//! person decides whether it goes into training.

use serde_json::json;

use super::super::data::Member;
use super::super::store::{Job, Status, Store};
use super::{model, route};

/// Whether a job is a trustworthy example of where its words should go.
fn good(job: &Job) -> bool {
    matches!(job.status, Status::Done | Status::Review | Status::Shipping | Status::Shipped)
        && !job.title.trim().is_empty()
        // A try in a race was routed by its race, not by its words.
        && (job.race == 0 || job.race == job.id)
        // A goal's steps were worded by Squad, not the person.
        && job.goal == 0
}

/// One training example per good job, as JSON lines.
pub fn examples(store: &Store, crew: &[Member]) -> Vec<String> {
    let system = model::route_system(crew);
    store
        .jobs
        .iter()
        .filter(|j| good(j))
        .map(|j| {
            let rules = route::by_rules(&j.title, crew);
            let why = if rules.mode == j.mode {
                rules.why
            } else {
                format!("It reads as a {} job.", j.mode.label().to_lowercase())
            };
            let answer = json!({
                "mode": j.mode.label().to_lowercase(),
                "bot": j.bot.name(),
                "sure": 0.95,
                "why": why,
            });
            json!({
                "messages": [
                    {"role": "system", "content": system},
                    {"role": "user", "content": j.title},
                    {"role": "assistant", "content": answer.to_string()},
                ]
            })
            .to_string()
        })
        .collect()
}

/// The fixed set of sentences every model is scored on before it ships.
pub const CASES: &str = include_str!("../../../../../training/eval/route.jsonl");

/// How a router did on a set of cases: right, out of, and the ones it missed
/// as (sentence, expected, got).
pub struct Score {
    pub right: usize,
    pub total: usize,
    pub missed: Vec<(String, String, String)>,
}

impl Score {
    pub fn percent(&self) -> f32 {
        if self.total == 0 {
            return 0.0;
        }
        100.0 * self.right as f32 / self.total as f32
    }
}

/// Scores `pick` (a sentence to a kind of job) on JSON-lines `cases`, each
/// `{"text": ..., "mode": "fix"}`.
pub fn score(cases: &str, pick: impl Fn(&str) -> super::super::data::Mode) -> Score {
    let mut s = Score {
        right: 0,
        total: 0,
        missed: Vec::new(),
    };
    for line in cases.lines().filter(|l| !l.trim().is_empty()) {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let (Some(text), Some(want)) = (v["text"].as_str(), v["mode"].as_str()) else {
            continue;
        };
        s.total += 1;
        let got = pick(text).label().to_lowercase();
        if got == want {
            s.right += 1;
        } else {
            s.missed.push((text.to_string(), want.to_string(), got));
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::super::super::data::Mode;
    use super::*;

    #[test]
    fn only_jobs_that_went_well_become_examples() {
        let mk = |id, title: &str, mode, status| Job {
            id,
            title: title.into(),
            mode,
            bot: Member::for_mode(mode),
            status,
            ..Default::default()
        };
        let store = Store {
            jobs: vec![
                mk(1, "fix the crash on save", Mode::Fix, Status::Shipped),
                mk(2, "add a page", Mode::Build, Status::Discarded),
                mk(3, "what does main do?", Mode::Ask, Status::Done),
            ],
            ..Default::default()
        };
        let ex = examples(&store, &Member::CREW);
        assert_eq!(ex.len(), 2);
        let v: serde_json::Value = serde_json::from_str(&ex[0]).unwrap();
        assert_eq!(v["messages"][1]["content"], "fix the crash on save");
        let answer: serde_json::Value =
            serde_json::from_str(v["messages"][2]["content"].as_str().unwrap()).unwrap();
        assert_eq!(answer["bot"], "Fixer");
    }

    /// The rules are the floor every trained model has to beat.
    #[test]
    fn the_rules_get_most_of_the_fixed_cases_right() {
        let s = score(CASES, |t| route::by_rules(t, &Member::CREW).mode);
        assert!(s.total >= 50);
        assert!(
            s.percent() >= 85.0,
            "rules scored {:.0}%, missed {:#?}",
            s.percent(),
            s.missed
        );
    }
}
