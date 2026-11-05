//! A bot worth making: when the same kind of job keeps coming up and no bot
//! on the crew is for it, Fin suggests one, with a name, a job and the kind of
//! work it does. The person makes it or says no; each idea is offered once.

use super::super::data::Member;
use super::super::store::{Job, Store};
use super::{stem_match, words};

/// Jobs on one topic within this many days count toward an idea.
pub const WINDOW_DAYS: u64 = 14;
/// How many such jobs it takes.
pub const ENOUGH: usize = 3;

/// A bot Fin thinks the person could use.
#[derive(Clone, Debug, PartialEq)]
pub struct Idea {
    /// Saved once offered, so it's never offered again: "docs".
    pub key: &'static str,
    pub name: &'static str,
    pub role: &'static str,
    pub like: Member,
    /// "You've handed the crew 4 jobs about docs in the last two weeks."
    pub why: String,
    /// The jobs that led to it.
    pub jobs: Vec<u64>,
}

/// Topics a bot of its own pays for, with the words that mark a job as one.
const TOPICS: [(&str, &str, &str, Member, &[&str]); 10] = [
    (
        "docs",
        "Scribe",
        "Keeps the docs, README and changelog up to date",
        Member::Builder,
        &[
            "docs",
            "documentation",
            "readme",
            "changelog",
            "docstring",
            "comments",
        ],
    ),
    (
        "deps",
        "Updater",
        "Upgrades dependencies and fixes what the upgrade breaks",
        Member::Keeper,
        &[
            "dependency",
            "dependencies",
            "deps",
            "upgrade",
            "bump",
            "renovate",
            "dependabot",
        ],
    ),
    (
        "db",
        "Migrator",
        "Writes and checks database migrations",
        Member::Builder,
        &[
            "migration",
            "schema",
            "database",
            "sql",
            "postgres",
            "table",
            "column",
        ],
    ),
    (
        "ui",
        "Stylist",
        "Polishes screens: layout, styles, dark mode and spacing",
        Member::Builder,
        &[
            "css",
            "style",
            "layout",
            "design",
            "spacing",
            "theme",
            "tailwind",
            "responsive",
        ],
    ),
    (
        "perf",
        "Speedster",
        "Finds what's slow and makes it fast, with numbers",
        Member::Fixer,
        &[
            "slow",
            "performance",
            "perf",
            "latency",
            "speed",
            "faster",
            "memory",
        ],
    ),
    (
        "i18n",
        "Linguist",
        "Translates the app and keeps every language in step",
        Member::Builder,
        &[
            "translate",
            "translation",
            "i18n",
            "locale",
            "localize",
            "language",
        ],
    ),
    (
        "a11y",
        "Includer",
        "Makes screens work with keyboards and screen readers",
        Member::Guard,
        &[
            "accessibility",
            "a11y",
            "aria",
            "screen reader",
            "contrast",
            "keyboard",
        ],
    ),
    (
        "api",
        "Plumber",
        "Builds and documents API endpoints",
        Member::Builder,
        &["endpoint", "api", "route", "graphql", "rest", "webhook"],
    ),
    (
        "release",
        "Releaser",
        "Gets releases out: version, notes and tags",
        Member::Keeper,
        &["release", "version", "tag", "publish", "deploy"],
    ),
    (
        "flaky",
        "Steadier",
        "Hunts flaky tests until they pass every time",
        Member::Checker,
        &["flaky", "intermittent", "flake", "sometimes fails", "retry"],
    ),
];

fn about(job: &Job, cues: &[&str]) -> bool {
    let lower = job.title.to_lowercase();
    let ws = words(&job.title);
    cues.iter().any(|c| {
        if c.contains(' ') {
            lower.contains(c)
        } else {
            ws.iter().any(|w| stem_match(w, c))
        }
    })
}

/// Whether a bot on the crew already covers a topic, by its name or job.
fn covered(cues: &[&str], name: &str, crew: &[Member]) -> bool {
    crew.iter().filter(|m| m.is_made()).any(|m| {
        let theirs = format!("{} {}", m.name(), m.role());
        let ws = words(&theirs);
        m.name().eq_ignore_ascii_case(name)
            || cues.iter().any(|c| ws.iter().any(|w| stem_match(w, c)))
    })
}

/// The best idea for this store at `now` (seconds), if any: the topic with
/// the most recent jobs, at least [`ENOUGH`], that no bot covers and that
/// wasn't offered before.
pub fn next(store: &Store, crew: &[Member], offered: &[String], now: u64) -> Option<Idea> {
    let since = now.saturating_sub(WINDOW_DAYS * 24 * 3600);
    let recent: Vec<&Job> = store.jobs.iter().filter(|j| j.started >= since).collect();
    TOPICS
        .iter()
        .filter(|t| !offered.iter().any(|o| o == t.0))
        .filter(|t| !covered(t.4, t.1, crew))
        .map(|t| {
            let jobs: Vec<u64> = recent
                .iter()
                .filter(|j| about(j, t.4))
                .map(|j| j.id)
                .collect();
            (t, jobs)
        })
        .filter(|(_, jobs)| jobs.len() >= ENOUGH)
        .max_by_key(|(_, jobs)| jobs.len())
        .map(|(t, jobs)| Idea {
            key: t.0,
            name: t.1,
            role: t.2,
            like: t.3,
            why: format!(
                "You've handed the crew {} jobs about {} in the last two weeks.",
                jobs.len(),
                topic_words(t.0)
            ),
            jobs,
        })
}

fn topic_words(key: &str) -> &'static str {
    match key {
        "docs" => "docs",
        "deps" => "dependencies",
        "db" => "the database",
        "ui" => "how screens look",
        "perf" => "speed",
        "i18n" => "translations",
        "a11y" => "accessibility",
        "api" => "the API",
        "release" => "releases",
        "flaky" => "flaky tests",
        _ => "one thing",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(id: u64, title: &str, started: u64) -> Job {
        Job {
            id,
            title: title.into(),
            started,
            ..Default::default()
        }
    }

    const NOW: u64 = 2_000_000_000;

    fn store(titles: &[&str]) -> Store {
        Store {
            jobs: titles
                .iter()
                .enumerate()
                .map(|(i, t)| job(i as u64 + 1, t, NOW - 3600 * (i as u64 + 1)))
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn three_docs_jobs_suggest_a_docs_bot() {
        let s = store(&[
            "Update the README for the new CLI",
            "fix typos in the docs",
            "add a changelog entry for 2.1",
            "add a login page",
        ]);
        let idea = next(&s, &Member::CREW, &[], NOW).expect("an idea");
        assert_eq!(idea.key, "docs");
        assert_eq!(idea.jobs.len(), 3);
        assert!(idea.why.contains("3 jobs about docs"));
    }

    #[test]
    fn two_isnt_enough_and_old_jobs_dont_count() {
        let s = store(&["Update the README", "fix the docs"]);
        assert!(next(&s, &Member::CREW, &[], NOW).is_none());
        let mut s = store(&["Update the README", "fix the docs", "docs for the API"]);
        s.jobs[2].started = NOW - (WINDOW_DAYS + 1) * 24 * 3600;
        assert!(next(&s, &Member::CREW, &[], NOW).is_none());
    }

    #[test]
    fn an_idea_is_offered_once() {
        let s = store(&["Update the README", "fix the docs", "docs for the API"]);
        assert!(next(&s, &Member::CREW, &["docs".into()], NOW).is_none());
    }
}
