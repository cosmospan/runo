//! The review: what a finished writing job changed, Checker's run of the
//! checks, and the decision (approve, send back or discard).

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::data::Member;
use super::ship;
use super::store::{Job, Status};
use super::ui::{self, c, ca};
use super::work::{self, Verdict};
use super::Shell;

/// Most diff lines shown for one file; the rest are a click away in the folder.
const DIFF_LINES: usize = 400;

impl Shell {
    /// The review card at the end of a job's feed, once it has changes.
    pub(super) fn review_card(&mut self, job: &Job, cx: &mut Context<Self>) -> Option<Div> {
        if job.status == Status::Shipped && !job.shipped.is_empty() {
            return Some(
                div()
                    .ml(px(40.))
                    .px(px(16.))
                    .py(px(12.))
                    .rounded(px(12.))
                    .bg(c(0x0F251F))
                    .border_1()
                    .border_color(c(0x134733))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .text_size(px(13.))
                    .text_color(c(ui::T_GREEN))
                    .child(div().size(px(7.)).rounded_full().bg(c(ui::GREEN)))
                    .child(job.shipped.clone()),
            );
        }
        if job.status == Status::Shipping {
            return Some(self.pr_card(job, cx));
        }
        if job.status != Status::Review || job.changes.is_empty() {
            return None;
        }
        let id = job.id;
        let (files, added, removed) = (job.changes.len(), job.lines().0, job.lines().1);
        let (said, verdict) = work::verdict(&job.checks, job.checking);
        let verdict_color = match verdict {
            Verdict::Pass => ui::T_GREEN,
            Verdict::Fail => ui::T_RED,
            Verdict::Couldnt => ui::T_AMBER,
            Verdict::Running => ui::T_BLUE,
            Verdict::None => ui::MUTED,
        };
        let checker = self.avatar(Member::Checker, 22.);

        // Head: where it stands, the size of the change, and the branch.
        let head = div()
            .px(px(16.))
            .py(px(12.))
            .flex()
            .items_center()
            .gap(px(10.))
            .border_b_1()
            .border_color(c(ui::LINE))
            .child(super::home::pill(
                "Ready for review",
                (ui::T_AMBER, 0x2B2010, 0x573B0F),
            ))
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(format!(
                        "{files} {}",
                        if files == 1 { "file" } else { "files" }
                    )),
            )
            .child(
                div()
                    .font_family(ui::MONO)
                    .text_size(px(12.))
                    .flex()
                    .gap(px(6.))
                    .child(div().text_color(c(ui::T_GREEN)).child(format!("+{added}")))
                    .child(
                        div()
                            .text_color(c(ui::T_RED))
                            .child(format!("\u{2212}{removed}")),
                    ),
            )
            .child(div().flex_1())
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(c(ui::MUTED))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(branch_chip(&job.branch))
                    .child("into")
                    .child(branch_chip(&job.base)),
            );

        // Checker's run of the checks.
        let mut proof = div()
            .px(px(16.))
            .py(px(12.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .border_b_1()
            .border_color(c(ui::LINE))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(checker)
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(13.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(c(verdict_color))
                            .child(said),
                    )
                    .when(!job.checking, |d| {
                        d.child(
                            ui::ghost(("recheck", id as usize), "Run again")
                                .h(px(26.))
                                .px(px(9.))
                                .text_size(px(12.))
                                .on_click(cx.listener(move |this, _, _, cx| this.recheck(id, cx))),
                        )
                    }),
            );
        if job.checking {
            proof = proof.child(ui::sweep(ui::BLUE, 0.).h(px(3.)));
        }
        for check in &job.checks {
            let (mark, col) = if check.ok {
                ("pass", ui::T_GREEN)
            } else if !check.infra.is_empty() {
                ("couldn't run", ui::T_AMBER)
            } else {
                ("fail", ui::T_RED)
            };
            proof = proof.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .text_size(px(12.5))
                    .child(
                        div()
                            .flex_1()
                            .font_family(ui::MONO)
                            .text_size(px(12.))
                            .text_color(c(ui::BODY))
                            .child(check.command.clone()),
                    )
                    .child(
                        div()
                            .font_family(ui::MONO)
                            .text_size(px(11.5))
                            .text_color(c(ui::FAINT))
                            .child(format!("{}s", check.secs)),
                    )
                    .child(div().w(px(84.)).text_color(c(col)).child(mark)),
            );
            if !check.ok && !check.tail.trim().is_empty() {
                let tail: Vec<&str> = check.tail.lines().rev().take(14).collect();
                let tail: Vec<&str> = tail.into_iter().rev().collect();
                proof = proof.child(
                    div()
                        .px(px(12.))
                        .py(px(8.))
                        .rounded(px(8.))
                        .bg(c(0x0A0A0D))
                        .border_1()
                        .border_color(c(ui::LINE))
                        .font_family(ui::MONO)
                        .text_size(px(11.5))
                        .line_height(px(17.))
                        .text_color(c(ui::BODY))
                        .child(tail.join("\n")),
                );
            }
        }

        // The files, each one opening its diff.
        let mut list = div().flex().flex_col();
        for (i, f) in job.changes.iter().enumerate() {
            let open = self
                .diff_open
                .as_ref()
                .is_some_and(|(j, p)| *j == id && *p == f.path);
            let path = f.path.clone();
            list = list.child(
                div()
                    .id(("change", i))
                    .h(px(32.))
                    .px(px(16.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .cursor_pointer()
                    .when(i > 0, |d| d.border_t_1().border_color(c(ui::LINE2)))
                    .when(open, |d| d.bg(c(ui::HOVER)))
                    .hover(|s| s.bg(c(ui::HOVER)))
                    .child(
                        div()
                            .w(px(14.))
                            .font_family(ui::MONO)
                            .text_size(px(11.))
                            .text_color(c(match f.status.chars().next() {
                                Some('A') => ui::T_GREEN,
                                Some('D') => ui::T_RED,
                                _ => ui::T_AMBER,
                            }))
                            .child(f.status.chars().next().unwrap_or('M').to_string()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font_family(ui::MONO)
                            .text_size(px(12.))
                            .text_color(c(ui::BODY))
                            .child(f.path.clone()),
                    )
                    .child(
                        div()
                            .font_family(ui::MONO)
                            .text_size(px(11.5))
                            .flex()
                            .gap(px(6.))
                            .when(f.added > 0, |d| {
                                d.child(
                                    div()
                                        .text_color(c(ui::T_GREEN))
                                        .child(format!("+{}", f.added)),
                                )
                            })
                            .when(f.removed > 0, |d| {
                                d.child(
                                    div()
                                        .text_color(c(ui::T_RED))
                                        .child(format!("\u{2212}{}", f.removed)),
                                )
                            }),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let key = (id, path.clone());
                        if this.diff_open.as_ref() == Some(&key) {
                            this.diff_open = None;
                        } else {
                            // Read once on opening, not on every frame.
                            this.diff_text = this
                                .store
                                .job(id)
                                .map(|j| work::diff(j, &key.1))
                                .unwrap_or_default();
                            this.diff_open = Some(key);
                        }
                        cx.notify();
                    })),
            );
            if open {
                list = list.child(diff_view(&self.diff_text));
            }
        }

        // The decision. GitHub projects can switch to merging here, and back.
        let label = self.approve_label(job);
        let way = self.way(job);
        let github = ship::remote_url(&job.folder.path).is_some_and(|u| ship::is_github(&u));
        let switch = match way {
            ship::Way::PullRequest => Some(("Merge here instead", ship::Way::Merge)),
            ship::Way::Merge if github => Some(("Open a PR instead", ship::Way::PullRequest)),
            ship::Way::Merge => None,
        };
        let foot = div()
            .px(px(16.))
            .py(px(12.))
            .flex()
            .items_center()
            .gap(px(8.))
            .border_t_1()
            .border_color(c(ui::LINE))
            .child(
                ui::solid(("approve", id as usize), label)
                    .on_click(cx.listener(move |this, _, _, cx| this.approve(id, cx))),
            )
            .when_some(switch, |d, (words, to)| {
                d.child(
                    div()
                        .id(("way", id as usize))
                        .text_size(px(12.))
                        .text_color(c(ui::MUTED))
                        .cursor_pointer()
                        .hover(|s| s.text_color(c(ui::INK)))
                        .child(words)
                        .on_click(cx.listener(move |this, _, _, cx| this.set_way(id, to, cx))),
                )
            })
            .child(
                div()
                    .flex_1()
                    .text_size(px(12.))
                    .text_color(c(ui::FAINT))
                    .child("Or write below to send it back with changes."),
            )
            .child(
                ui::ghost(("discard", id as usize), "Discard")
                    .text_color(c(ui::T_RED))
                    .on_click(cx.listener(move |this, _, _, cx| this.discard_job(id, cx))),
            );

        Some(
            div()
                .ml(px(40.))
                .rounded(px(12.))
                .bg(c(ui::PANEL))
                .border_1()
                .border_color(ca(ui::AMBER, 0.35))
                .overflow_hidden()
                .flex()
                .flex_col()
                .child(head)
                .child(proof)
                .child(list)
                .child(foot),
        )
    }
}

impl Shell {
    /// The pull request Shipper opened: its checks, auto-merge, and the buttons
    /// to merge now or close it.
    fn pr_card(&mut self, job: &Job, cx: &mut Context<Self>) -> Div {
        let id = job.id;
        let shipper = self.avatar(Member::Shipper, 22.);
        let opening = job.pr.is_empty();
        let failed = !job.pr_failed.is_empty() && job.pr_line.contains("failed");
        let line_color = if opening {
            ui::T_BLUE
        } else if failed {
            ui::T_RED
        } else if job.pr_line.starts_with("All") {
            ui::T_GREEN
        } else {
            ui::T_BLUE
        };
        let title = if opening {
            "Opening the pull request".to_string()
        } else if job.pr_number > 0 {
            format!("Pull request #{}", job.pr_number)
        } else {
            "Pull request".to_string()
        };
        let url = job.pr.clone();
        let auto = job.auto_merge;
        let mut card = div()
            .ml(px(40.))
            .rounded(px(12.))
            .bg(c(ui::PANEL))
            .border_1()
            .border_color(ca(0x14B8A6, 0.40))
            .overflow_hidden()
            .flex()
            .flex_col()
            .child(
                div()
                    .px(px(16.))
                    .py(px(12.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .border_b_1()
                    .border_color(c(ui::LINE))
                    .child(shipper)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(13.5))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(title),
                            )
                            .child(div().text_size(px(12.)).text_color(c(line_color)).child(
                                if opening {
                                    "Squashing, catching up with the base and pushing".to_string()
                                } else if job.pr_line.is_empty() {
                                    "Looking at its checks".to_string()
                                } else {
                                    job.pr_line.clone()
                                },
                            )),
                    )
                    .when(!opening, |d| {
                        d.child(
                            ui::ghost(("pr-open", id as usize), "View on GitHub")
                                .h(px(28.))
                                .text_size(px(12.5))
                                .on_click(move |_, _, cx| cx.open_url(&url)),
                        )
                    }),
            );
        if opening || (!failed && !job.pr_line.starts_with("All")) {
            card = card.child(ui::sweep(0x14B8A6, 0.).h(px(2.)).rounded_none());
        }
        if !opening {
            card = card.child(
                div()
                    .px(px(16.))
                    .py(px(12.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(
                        div()
                            .id(("auto", id as usize))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .cursor_pointer()
                            .child(toggle(auto))
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .text_color(c(ui::BODY))
                                    .child("Merge when checks pass"),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.set_auto_merge(id, !auto, cx)
                            })),
                    )
                    .child(div().flex_1())
                    .child(
                        ui::ghost(("pr-close", id as usize), "Close PR")
                            .h(px(28.))
                            .text_size(px(12.5))
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.land_pr(id, false, cx)),
                            ),
                    )
                    .child(
                        ui::solid(("pr-merge", id as usize), "Merge now")
                            .h(px(28.))
                            .text_size(px(12.5))
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.land_pr(id, true, cx)),
                            ),
                    ),
            );
        }
        card
    }
}

/// A small on/off switch.
fn toggle(on: bool) -> Div {
    div()
        .w(px(30.))
        .h(px(18.))
        .flex_none()
        .rounded_full()
        .bg(c(if on { ui::GREEN } else { 0x2A2A33 }))
        .relative()
        .child(
            div()
                .absolute()
                .top(px(2.))
                .left(px(if on { 14. } else { 2. }))
                .size(px(14.))
                .rounded_full()
                .bg(c(0xFFFFFF)),
        )
}

fn branch_chip(name: &str) -> Div {
    div()
        .px(px(6.))
        .py(px(1.))
        .rounded(px(5.))
        .bg(c(ui::RAISED))
        .border_1()
        .border_color(c(ui::LINE))
        .font_family(ui::MONO)
        .text_size(px(11.5))
        .text_color(c(ui::BODY))
        .child(name.to_string())
}

/// A unified diff, coloured line by line.
fn diff_view(diff: &str) -> Div {
    let mut body = div()
        .py(px(6.))
        .bg(c(0x0A0A0D))
        .border_t_1()
        .border_color(c(ui::LINE2))
        .font_family(ui::MONO)
        .text_size(px(11.5))
        .line_height(px(18.));
    let lines: Vec<&str> = diff.lines().skip_while(|l| !l.starts_with("@@")).collect();
    if lines.is_empty() {
        return body.child(
            div()
                .px(px(16.))
                .text_color(c(ui::FAINT))
                .child("No text changes to show (a binary file, or only its mode changed)."),
        );
    }
    for l in lines.iter().take(DIFF_LINES) {
        let (fg, bg) = if l.starts_with('+') {
            (0x7EE2B0, Some(ca(ui::GREEN, 0.08)))
        } else if l.starts_with('-') {
            (0xFF9AA6, Some(ca(ui::RED, 0.08)))
        } else if l.starts_with("@@") {
            (ui::FAINT, None)
        } else {
            (ui::BODY, None)
        };
        body = body.child(
            div()
                .px(px(16.))
                .whitespace_nowrap()
                .overflow_hidden()
                .text_color(c(fg))
                .when_some(bg, |d, bg| d.bg(bg))
                .child(if l.is_empty() {
                    " ".to_string()
                } else {
                    l.to_string()
                }),
        );
    }
    if lines.len() > DIFF_LINES {
        body = body.child(
            div()
                .px(px(16.))
                .text_color(c(ui::FAINT))
                .child(format!("{} more lines", lines.len() - DIFF_LINES)),
        );
    }
    body
}
