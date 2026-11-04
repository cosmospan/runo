//! When a job doesn't end well: what went wrong, in plain words, and the one
//! or two things that get it going again. An engine out of its plan or not
//! installed offers another engine; a stopped job can carry on; a branch that
//! clashes with its base can be handed back to the bot to bring up to date.

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::store::{Item, Job, Status};
use super::ui::{self, c};
use super::Pane;
use super::{Place, Shell};

/// What went wrong with a job.
#[derive(Clone, Debug, PartialEq)]
pub enum Trouble {
    /// The engine's plan ran out; when it resets, if it said.
    Limit(String),
    /// The engine isn't installed here.
    Missing,
    /// The engine isn't signed in.
    SignIn,
    /// Its base moved and these files now clash.
    Conflict(Vec<String>),
    Stopped,
    Interrupted,
    /// It stopped with a problem, in the engine's words.
    Failed(String),
}

/// The last error the job reported, else the bot's last words.
fn last_problem(job: &Job) -> String {
    job.feed
        .iter()
        .rev()
        .find_map(|i| match i {
            Item::Error(e) => Some(e.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

/// What went wrong, or `None` when nothing did.
pub fn trouble(job: &Job) -> Option<Trouble> {
    if !job.conflict.is_empty() && job.status == Status::Review {
        return Some(Trouble::Conflict(job.conflict.clone()));
    }
    let err = last_problem(job);
    let said = job.last_words().unwrap_or("");
    // A plan that ran out can end a run that otherwise looks fine.
    if matches!(job.status, Status::Failed | Status::Done) {
        for text in [err.as_str(), said] {
            if let Some(limit) = crate::planlimit::from_text(text, chrono::Local::now()) {
                let when = limit
                    .resets_at
                    .map(|at| crate::planlimit::reset_clock(at, super::store::now() as i64))
                    .unwrap_or_default();
                return Some(Trouble::Limit(when));
            }
        }
    }
    match job.status {
        Status::Stopped => Some(Trouble::Stopped),
        Status::Interrupted => Some(Trouble::Interrupted),
        Status::Failed => {
            let lower = err.to_lowercase();
            Some(if lower.contains("isn't installed") {
                Trouble::Missing
            } else if lower.contains("sign in")
                || lower.contains("/login")
                || lower.contains(" login")
                || lower.contains("api key")
            {
                Trouble::SignIn
            } else if lower.contains("rate limit")
                || lower.contains("quota")
                || lower.contains("429")
                || lower.contains("credit balance")
            {
                Trouble::Limit(String::new())
            } else {
                Trouble::Failed(err)
            })
        }
        _ => None,
    }
}

/// The files a local merge stopped on, from git's message.
pub fn conflict_files(err: &str) -> Vec<String> {
    err.split_once("conflicts in ")
        .map(|(_, rest)| rest.split(". ").next().unwrap_or(rest))
        .map(|list| {
            list.trim_end_matches('.')
                .split(", ")
                .map(|f| f.trim().to_string())
                .filter(|f| !f.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// What the bot is told when its branch clashes with its base.
pub fn resolve_prompt(job: &Job) -> String {
    format!(
        "{base} moved on since you started, and these files now clash with it: {files}. Bring this branch up to date: fetch if the project has a remote, merge the latest {base} into this branch, resolve each conflict so both changes still work, run the project's checks, and commit.",
        base = if job.base.is_empty() { "The base branch" } else { &job.base },
        files = job.conflict.join(", ")
    )
}

impl Shell {
    /// The card at the end of a job that didn't end well.
    pub(super) fn trouble_card(&mut self, job: &Job, cx: &mut Context<Self>) -> Option<Div> {
        let t = trouble(job)?;
        let id = job.id;
        let engine = job
            .engine
            .split(" \u{b7} ")
            .next()
            .unwrap_or("The engine")
            .to_string();
        let engine = if engine.is_empty() {
            "The engine".to_string()
        } else {
            engine
        };
        let other = {
            let st = cx.global::<crate::settings::AppSettings>();
            super::engine::fallback(st, job)
        };
        let other_name = other.map(|i| crate::agent::PROVIDERS[i].name);
        let (tone, title, line): (u32, String, String) = match &t {
            Trouble::Limit(when) => (
                ui::AMBER,
                format!("{engine} hit its usage limit"),
                if when.is_empty() {
                    "Run it on another engine now, or try again once the plan resets.".into()
                } else {
                    format!("It resets at {when}. Run it on another engine now, or try again then.")
                },
            ),
            Trouble::Missing => (
                ui::AMBER,
                format!("{engine} isn't installed on this computer"),
                "Install it, or run this job on an engine you have.".into(),
            ),
            Trouble::SignIn => (
                ui::AMBER,
                format!("{engine} isn't signed in"),
                last_problem(job),
            ),
            Trouble::Conflict(files) => (
                ui::AMBER,
                format!(
                    "{} moved on, and {} {} now clash",
                    if job.base.is_empty() { "The base branch" } else { &job.base },
                    files.len(),
                    if files.len() == 1 { "file" } else { "files" }
                ),
                format!(
                    "{}. {} can bring the branch up to date and resolve them, then it comes back for review.",
                    files.join(", "),
                    job.bot.name()
                ),
            ),
            Trouble::Stopped => (
                ui::MUTED,
                "You stopped it".into(),
                if job.worktree.is_empty() {
                    "It can carry on from where it was.".into()
                } else {
                    "What it did so far is still on its branch. It can carry on from there.".into()
                },
            ),
            Trouble::Interrupted => (
                ui::MUTED,
                "Squad Code closed while this was running".into(),
                "It can carry on from where it was, or start over.".into(),
            ),
            Trouble::Failed(why) => (
                ui::RED,
                "It stopped with a problem".into(),
                if why.is_empty() {
                    "The engine stopped without saying why.".into()
                } else {
                    why.clone()
                },
            ),
        };
        let mut buttons = div().flex().flex_wrap().gap(px(8.));
        match &t {
            Trouble::Conflict(_) => {
                buttons = buttons
                    .child(
                        ui::solid(
                            "trouble-resolve",
                            format!("Ask {} to fix it", job.bot.name()),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| this.resolve(id, cx))),
                    )
                    .child(
                        ui::ghost("trouble-discard", "Discard")
                            .on_click(cx.listener(move |this, _, _, cx| this.discard_job(id, cx))),
                    );
            }
            Trouble::Stopped | Trouble::Interrupted => {
                buttons = buttons
                    .child(
                        ui::solid("trouble-carry", "Carry on")
                            .on_click(cx.listener(move |this, _, _, cx| this.carry_on(id, cx))),
                    )
                    .child(
                        ui::ghost("trouble-over", "Start over")
                            .on_click(cx.listener(move |this, _, _, cx| this.rerun(id, None, cx))),
                    );
                if !job.worktree.is_empty() {
                    buttons = buttons.child(
                        ui::ghost("trouble-discard", "Discard")
                            .on_click(cx.listener(move |this, _, _, cx| this.discard_job(id, cx))),
                    );
                }
            }
            _ => {
                if let (Some(i), Some(name)) = (other, other_name) {
                    buttons = buttons.child(
                        ui::solid("trouble-other", format!("Run on {name} instead")).on_click(
                            cx.listener(move |this, _, _, cx| this.rerun(id, Some(i), cx)),
                        ),
                    );
                }
                if matches!(t, Trouble::Failed(_) | Trouble::Limit(_)) {
                    let try_again = ui::ghost("trouble-again", "Try again")
                        .on_click(cx.listener(move |this, _, _, cx| this.rerun(id, None, cx)));
                    buttons = buttons.child(try_again);
                }
                buttons = buttons.child(ui::ghost("trouble-engines", "Engines").on_click(
                    cx.listener(|this, _, _, cx| this.go(Place::Settings(Pane::Engines), cx)),
                ));
            }
        }
        Some(
            ui::card()
                .ml(px(40.))
                .p(px(16.))
                .flex()
                .flex_col()
                .gap(px(10.))
                .border_color(c(match tone {
                    ui::AMBER => 0x573B0F,
                    ui::RED => 0x561A25,
                    _ => ui::LINE,
                }))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(div().size(px(8.)).rounded_full().bg(c(tone)))
                        .child(
                            div()
                                .text_size(px(14.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(c(ui::INK))
                                .child(title),
                        ),
                )
                .when(!line.is_empty(), |d| {
                    d.child(
                        div()
                            .text_size(px(13.))
                            .line_height(px(19.))
                            .text_color(c(ui::BODY))
                            .child(line),
                    )
                })
                .when(other.is_none() && !matches!(t, Trouble::Conflict(_) | Trouble::Stopped | Trouble::Interrupted), |d| {
                    d.child(
                        div()
                            .text_size(px(12.))
                            .text_color(c(ui::FAINT))
                            .child("No other engine is ready on this computer. Add one in Engines to switch."),
                    )
                })
                .child(buttons),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::super::store::{Item, Job, Status};
    use super::{conflict_files, trouble, Trouble};

    fn failed(err: &str) -> Job {
        Job {
            status: Status::Failed,
            feed: vec![Item::Error(err.into())],
            ..Default::default()
        }
    }

    #[test]
    fn troubles_are_told_apart() {
        assert_eq!(
            trouble(&failed("Claude Code isn't installed on this computer. Pick another engine for Builder in Engines.")),
            Some(Trouble::Missing)
        );
        assert!(matches!(
            trouble(&failed("You've hit your usage limit for Claude Code.")),
            Some(Trouble::Limit(_))
        ));
        assert_eq!(
            trouble(&failed(
                "Codex isn't signed in. Open a terminal and run `codex login`."
            )),
            Some(Trouble::SignIn)
        );
        assert!(matches!(trouble(&failed("boom")), Some(Trouble::Failed(_))));
        let ok = Job {
            status: Status::Done,
            ..Default::default()
        };
        assert_eq!(trouble(&ok), None);
    }

    #[test]
    fn a_limit_in_the_last_words_counts_too() {
        let j = Job {
            status: Status::Done,
            feed: vec![Item::Text("Claude usage limit reached.".into())],
            ..Default::default()
        };
        assert!(matches!(trouble(&j), Some(Trouble::Limit(_))));
    }

    #[test]
    fn conflicts_name_their_files() {
        assert_eq!(
            conflict_files("Merging stopped on conflicts in src/a.rs, src/b.rs. Update the branch from main first so they can be resolved there."),
            vec!["src/a.rs".to_string(), "src/b.rs".to_string()]
        );
        assert!(conflict_files("other error").is_empty());
    }
}
