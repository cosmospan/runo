//! Git, where the work is talked about: the lines and cards a job shows in its
//! thread about its branch. Where it stands (its branch, commits on it, how
//! far its base moved), the seatbelt holding it before a push or a merge, and
//! Peek's heads-up when another job changes the same lines.

use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::store::{Job, Status};
use super::ui::{self, c, ca};
use super::Shell;

/// "squad/apple-pay · 3 commits · 2 behind main", for a job with a branch.
pub fn branch_words(j: &Job) -> Option<(String, String, Option<String>)> {
    if j.branch.is_empty() {
        return None;
    }
    let commits = match j.ahead {
        0 => String::new(),
        1 => "1 commit".to_string(),
        n => format!("{n} commits"),
    };
    let behind = (j.behind > 0).then(|| {
        let base = if j.base.is_empty() {
            "its base"
        } else {
            &j.base
        };
        format!("{} behind {base}", j.behind)
    });
    Some((j.branch.clone(), commits, behind))
}

impl Shell {
    /// The branch a job works on, how many commits it has made there and how
    /// far its base moved since it started.
    pub(super) fn branch_line(&self, j: &Job) -> Option<Div> {
        if !matches!(
            j.status,
            Status::Running | Status::Done | Status::Review | Status::Shipping
        ) {
            return None;
        }
        let (branch, commits, behind) = branch_words(j)?;
        let dot = || div().text_color(c(ui::faint())).child("\u{b7}");
        Some(
            div()
                .mx(px(14.))
                .mb(px(8.))
                .px(px(10.))
                .py(px(6.))
                .rounded(px(8.))
                .bg(c(ui::well()))
                .border_1()
                .border_color(c(ui::line2()))
                .flex()
                .items_center()
                .gap(px(8.))
                .text_size(px(12.))
                .text_color(c(ui::muted()))
                .child(
                    Icon::new(IconName::GitBranch)
                        .size(px(12.))
                        .text_color(c(ui::muted())),
                )
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .font_family(ui::MONO)
                        .text_color(c(ui::body()))
                        .child(branch),
                )
                .when(!commits.is_empty(), |d| d.child(dot()).child(commits))
                .when_some(behind, |d, b| {
                    d.child(dot())
                        .child(div().text_color(c(ui::t_amber())).child(b))
                }),
        )
    }

    /// The seatbelt held Approve: what it found, and the three ways on.
    pub(super) fn belt_card(&mut self, j: &Job, cx: &mut Context<Self>) -> Option<Div> {
        if j.held.is_empty() || j.status != Status::Review {
            return None;
        }
        let id = j.id;
        let tone = ui::bad_tone();
        let mut rows = div().flex().flex_col();
        for (n, line) in j.held.iter().enumerate() {
            let (place, what) = line
                .split_once(" \u{b7} ")
                .map(|(a, b)| (a.to_string(), b.to_string()))
                .unwrap_or_else(|| (String::new(), line.clone()));
            rows = rows.child(
                div()
                    .px(px(12.))
                    .py(px(6.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .text_size(px(12.5))
                    .when(n > 0, |d| d.border_t_1().border_color(c(tone.2)))
                    .child(div().text_color(c(ui::body())).child(what))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .font_family(ui::MONO)
                            .text_size(px(11.5))
                            .text_color(c(ui::muted()))
                            .child(place),
                    ),
            );
        }
        let anyway = match self.crew.way(j) {
            super::ship::Way::PullRequest => "Push anyway",
            super::ship::Way::Merge => "Merge anyway",
        };
        Some(
            div()
                .mx(px(14.))
                .mb(px(10.))
                .rounded(px(12.))
                .bg(c(tone.1))
                .border_1()
                .border_color(c(tone.2))
                .flex()
                .flex_col()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .px(px(12.))
                        .pt(px(10.))
                        .pb(px(6.))
                        .text_size(px(13.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(c(ui::ink()))
                        .child(
                            Icon::new(IconName::ShieldAlert)
                                .size(px(14.))
                                .text_color(c(tone.0)),
                        )
                        .child("Seatbelt: Shipper stopped before it left your computer"),
                )
                .child(rows)
                .child(
                    div()
                        .px(px(12.))
                        .py(px(10.))
                        .flex()
                        .flex_wrap()
                        .gap(px(8.))
                        .child(
                            ui::solid(
                                ("belt-take-out", id as usize),
                                format!("Ask {} to take them out", j.bot.name()),
                            )
                            .h(px(28.))
                            .on_click(cx.listener(move |this, _, _, cx| this.take_out(id, cx))),
                        )
                        .child(
                            ui::ghost(("belt-anyway", id as usize), anyway)
                                .h(px(28.))
                                .on_click(
                                    cx.listener(move |this, _, _, cx| this.approve_anyway(id, cx)),
                                ),
                        ),
                ),
        )
    }

    /// Peek's heads-up: other jobs in the project change the same lines.
    pub(super) fn clash_card(&mut self, j: &Job, cx: &mut Context<Self>) -> Option<Div> {
        if j.clash_with.is_empty()
            || !matches!(j.status, Status::Running | Status::Done | Status::Review)
        {
            return None;
        }
        let violet = ui::t_violet();
        let mut card = div()
            .mx(px(14.))
            .mb(px(10.))
            .px(px(12.))
            .py(px(9.))
            .rounded(px(12.))
            .bg(ca(ui::VIOLET, 0.07))
            .border_1()
            .border_color(ca(ui::VIOLET, 0.28))
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(c(ui::ink()))
                    .child(Icon::new(IconName::Radar).size(px(14.)).text_color(c(violet)))
                    .child("Clash ahead"),
            )
            .child(
                div()
                    .text_size(px(12.5))
                    .text_color(c(ui::muted()))
                    .child("Whichever is approved second will need its branch brought up to date. Nothing in either folder was touched."),
            );
        for (other, words) in &j.clash_with {
            let other = *other;
            let title = self
                .crew
                .store
                .job(other)
                .map(|o| o.label().to_string())
                .unwrap_or_default();
            card = card.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_size(px(12.5))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(c(ui::body()))
                            .child(format!("#{other} {title} \u{b7} {words}")),
                    )
                    .child(
                        ui::ghost(
                            ("clash-open", (j.id as usize) * 4096 + other as usize),
                            format!("Open #{other}"),
                        )
                        .h(px(26.))
                        .on_click(cx.listener(move |this, _, _, cx| this.open_job(other, cx))),
                    ),
            );
        }
        Some(card)
    }
}

impl Shell {
    /// The Git side of the Jobs panel, for one project and thread: where the
    /// folder stands against its remote, the branches the crew works on, and
    /// what to watch out for before approving.
    pub(super) fn git_side(
        &mut self,
        folder: &str,
        jobs: &[Job],
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let mut body = div()
            .id("git-side")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(px(16.))
            .pb(px(16.))
            .flex()
            .flex_col()
            .gap(px(12.));
        let panel = || {
            div()
                .p(px(12.))
                .rounded(px(12.))
                .bg(c(ui::panel()))
                .border_1()
                .border_color(c(ui::line()))
                .flex()
                .flex_col()
                .gap(px(8.))
        };

        // Where the folder stands.
        let repo = self.crew.repos.get(folder).cloned();
        let path = folder.to_string();
        let mut top = panel().child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(
                    Icon::new(IconName::GitBranch)
                        .size(px(14.))
                        .text_color(c(ui::muted())),
                )
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .font_family(ui::MONO)
                        .text_size(px(13.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(c(ui::ink()))
                        .child(match &repo {
                            Some(r) if !r.state.branch.is_empty() => r.state.branch.clone(),
                            Some(_) => "no branch".to_string(),
                            None => "\u{2026}".to_string(),
                        }),
                )
                .child(div().flex_1())
                .when(
                    repo.as_ref().is_some_and(|r| !r.state.remote.is_empty()),
                    |d| {
                        d.child(
                            ui::ghost_i("git-fetch", IconName::RotateCw, "Refresh")
                                .h(px(26.))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.crew.fetch(&path);
                                    cx.notify();
                                })),
                        )
                    },
                ),
        );
        let words = match &repo {
            None => "Looking at git\u{2026}".to_string(),
            Some(r) => {
                let mut parts = Vec::new();
                if r.state.remote.is_empty() {
                    parts.push("No remote: approved jobs merge here".to_string());
                } else {
                    let host = if r.state.host.is_empty() {
                        r.state.remote.clone()
                    } else {
                        r.state.host.clone()
                    };
                    parts.push(match r.state.behind {
                        0 => format!("Up to date with {host}"),
                        n => format!("\u{2193}{n} new on {host}"),
                    });
                    if r.state.ahead > 0 {
                        parts.push(format!("\u{2191}{} not sent", r.state.ahead));
                    }
                }
                parts.push(if r.dirty {
                    "your folder has changes".to_string()
                } else {
                    "your folder is clean".to_string()
                });
                parts.join(" \u{b7} ")
            }
        };
        top = top.child(
            div()
                .text_size(px(12.))
                .text_color(c(if repo.as_ref().is_some_and(|r| r.state.behind > 0) {
                    ui::t_amber()
                } else {
                    ui::muted()
                }))
                .child(words),
        );
        body = body.child(top);

        // The crew's branches.
        let branched: Vec<&Job> = jobs
            .iter()
            .filter(|j| {
                !j.branch.is_empty()
                    && matches!(
                        j.status,
                        Status::Running | Status::Done | Status::Review | Status::Shipping
                    )
            })
            .collect();
        let mut list = panel().child(ui::heading("Branches", None, Some(branched.len())));
        if branched.is_empty() {
            list = list.child(
                div()
                    .text_size(px(12.5))
                    .text_color(c(ui::faint()))
                    .child("No job is working on a branch right now."),
            );
        }
        for j in &branched {
            let id = j.id;
            let dot = match j.status {
                Status::Running => ui::t_blue(),
                Status::Shipping => ui::t_green(),
                _ => ui::t_amber(),
            };
            let mut right = vec![format!("#{id}")];
            if j.ahead > 0 {
                right.push(match j.ahead {
                    1 => "1 commit".to_string(),
                    n => format!("{n} commits"),
                });
            }
            if j.pr_number > 0 {
                right.push(format!("PR #{}", j.pr_number));
            }
            let mut notes: Vec<(u32, String)> = Vec::new();
            if !j.held.is_empty() {
                notes.push((ui::t_red(), "held by the seatbelt".into()));
            }
            for (other, _) in &j.clash_with {
                notes.push((ui::t_violet(), format!("clashes with #{other}")));
            }
            if j.behind > 0 {
                let base = if j.base.is_empty() {
                    "its base"
                } else {
                    &j.base
                };
                notes.push((ui::t_amber(), format!("{} behind {base}", j.behind)));
            }
            list = list.child(
                div()
                    .id(("git-branch", id as usize))
                    .px(px(6.))
                    .py(px(5.))
                    .mx(px(-6.))
                    .rounded(px(7.))
                    .cursor_pointer()
                    .hover(|s| s.bg(c(ui::hover())))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .text_size(px(12.5))
                            .child(div().size(px(6.)).flex_none().rounded_full().bg(c(dot)))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .font_family(ui::MONO)
                                    .text_size(px(12.))
                                    .text_color(c(ui::body()))
                                    .child(j.branch.clone()),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .text_size(px(11.5))
                                    .text_color(c(ui::faint()))
                                    .child(right.join(" \u{b7} ")),
                            ),
                    )
                    .when(!notes.is_empty(), |d| {
                        d.child(
                            div()
                                .pl(px(14.))
                                .flex()
                                .flex_wrap()
                                .gap(px(8.))
                                .text_size(px(11.5))
                                .children(
                                    notes
                                        .into_iter()
                                        .map(|(col, n)| div().text_color(c(col)).child(n)),
                                ),
                        )
                    })
                    .on_click(cx.listener(move |this, _, _, cx| this.open_job(id, cx))),
            );
        }
        body = body.child(list);

        // What to watch out for before approving.
        let mut watch: Vec<(u32, String, u64)> = Vec::new();
        for j in &branched {
            if !j.held.is_empty() {
                watch.push((
                    ui::t_red(),
                    format!("Seatbelt held #{} before it left", j.id),
                    j.id,
                ));
            }
        }
        let mut told: Vec<(u64, u64)> = Vec::new();
        for j in &branched {
            for (other, words) in &j.clash_with {
                let pair = (j.id.min(*other), j.id.max(*other));
                if !told.contains(&pair) {
                    told.push(pair);
                    watch.push((
                        ui::t_violet(),
                        format!("#{} and #{other} clash in {words}", j.id),
                        j.id,
                    ));
                }
            }
        }
        let behind: Vec<&&Job> = branched.iter().filter(|j| j.behind > 0).collect();
        if let Some(first) = behind.first() {
            watch.push((
                ui::t_amber(),
                match behind.len() {
                    1 => format!("#{}'s base moved since it started", first.id),
                    n => format!("{n} branches are behind their base"),
                },
                first.id,
            ));
        }
        let mut outs = panel().child(ui::heading("Watch-outs", None, None));
        if watch.is_empty() {
            outs = outs.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_size(px(12.5))
                    .text_color(c(ui::t_green()))
                    .child(Icon::new(IconName::ShieldCheck).size(px(13.)))
                    .child("Nothing to watch out for."),
            );
        }
        for (n, (col, words, id)) in watch.into_iter().enumerate() {
            outs = outs.child(
                div()
                    .id(("git-watch", n))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .px(px(6.))
                    .py(px(4.))
                    .mx(px(-6.))
                    .rounded(px(7.))
                    .cursor_pointer()
                    .hover(|s| s.bg(c(ui::hover())))
                    .text_size(px(12.5))
                    .text_color(c(ui::body()))
                    .child(div().size(px(6.)).flex_none().rounded_full().bg(c(col)))
                    .child(div().min_w_0().truncate().child(words))
                    .on_click(cx.listener(move |this, _, _, cx| this.open_job(id, cx))),
            );
        }
        body.child(outs)
    }
}

#[cfg(test)]
mod tests {
    use super::super::store::Job;
    use super::branch_words;

    #[test]
    fn branch_words_read_plainly() {
        let mut j = Job::default();
        assert!(branch_words(&j).is_none());
        j.branch = "squad/apple-pay".into();
        j.base = "main".into();
        assert_eq!(
            branch_words(&j),
            Some(("squad/apple-pay".into(), String::new(), None))
        );
        j.ahead = 3;
        j.behind = 2;
        assert_eq!(
            branch_words(&j),
            Some((
                "squad/apple-pay".into(),
                "3 commits".into(),
                Some("2 behind main".into())
            ))
        );
        j.ahead = 1;
        assert_eq!(branch_words(&j).unwrap().1, "1 commit");
    }
}
