//! Inbox (finished work you haven't looked at, and jobs that stopped with a
//! problem) and History (every job, newest first).

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::store::{self, Job, Status};
use super::ui::{self, c};
use super::{Place, Shell};

impl Shell {
    /// Inbox: what needs you on the left, the picked item on the right with what
    /// the bot said, what it changed, and what you can do about it.
    pub(super) fn inbox(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let jobs: Vec<Job> = self
            .store
            .jobs
            .iter()
            .rev()
            .filter(|j| j.needs_you())
            .cloned()
            .collect();
        if jobs.is_empty() {
            return self.empty_place(
                "You're all caught up",
                "Finished work you haven't looked at, and anything that needs you, shows up here.",
                cx,
            );
        }
        let pick = self
            .inbox_pick
            .filter(|id| jobs.iter().any(|j| j.id == *id))
            .unwrap_or(jobs[0].id);
        let count = jobs.len();
        let problems = jobs.iter().filter(|j| j.status != Status::Done).count();

        let mut list = div()
            .id("inbox-list")
            .flex_1()
            .overflow_y_scroll()
            .p(px(8.))
            .flex()
            .flex_col()
            .gap(px(4.));
        for j in &jobs {
            let id = j.id;
            let on = id == pick;
            let face = self.avatar(j.bot, 30.);
            let (word, tone) = super::home::status_of(j);
            list = list.child(
                div()
                    .id(("inbox", id as usize))
                    .flex()
                    .gap(px(12.))
                    .p(px(12.))
                    .rounded(px(10.))
                    .border_1()
                    .cursor_pointer()
                    .map(|d| {
                        if on {
                            d.bg(c(ui::PANEL))
                                .border_color(c(ui::LINE))
                                .shadow(ui::lift())
                        } else {
                            d.border_color(gpui_kit::transparent_black())
                                .hover(|s| s.bg(c(ui::HOVER)))
                        }
                    })
                    .child(div().flex_none().child(face))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(3.))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .truncate()
                                            .text_size(px(13.))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(j.title.clone()),
                                    )
                                    .child(
                                        div()
                                            .flex_none()
                                            .text_size(px(11.))
                                            .text_color(c(ui::FAINT))
                                            .child(store::ago(j.ended.unwrap_or(j.started))),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(c(ui::MUTED))
                                    .truncate()
                                    .child(format!(
                                        "{} · #{} · {}",
                                        j.bot.name(),
                                        id,
                                        j.folder.name
                                    )),
                            )
                            .child(
                                div()
                                    .mt(px(4.))
                                    .flex()
                                    .child(super::home::pill(&word, tone)),
                            ),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.inbox_pick = Some(id);
                        cx.notify();
                    })),
            );
        }

        let left = div()
            .w(px(380.))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(c(ui::LINE))
            .child(
                div()
                    .px(px(20.))
                    .pt(px(20.))
                    .pb(px(14.))
                    .flex()
                    .items_end()
                    .gap(px(10.))
                    .border_b_1()
                    .border_color(c(ui::LINE))
                    .child(
                        div()
                            .flex_1()
                            .child(
                                div()
                                    .text_size(px(20.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Needs you"),
                            )
                            .child(
                                div()
                                    .mt(px(2.))
                                    .text_size(px(12.5))
                                    .text_color(c(ui::MUTED))
                                    .child(match problems {
                                        0 => format!("{count} to review"),
                                        p => format!("{count} to review · {p} stopped early"),
                                    }),
                            ),
                    )
                    .child(
                        ui::ghost("mark-all", "Mark all seen")
                            .h(px(28.))
                            .text_size(px(12.5))
                            .on_click(cx.listener(|this, _, _, cx| {
                                for j in &mut this.store.jobs {
                                    if j.status != Status::Running {
                                        j.seen = true;
                                    }
                                }
                                this.store.save();
                                cx.notify();
                            })),
                    ),
            )
            .child(list);

        let j = jobs
            .iter()
            .find(|j| j.id == pick)
            .cloned()
            .unwrap_or_else(|| jobs[0].clone());
        let right = self.inbox_detail(&j, cx);
        div()
            .size_full()
            .flex()
            .child(left)
            .child(right)
            .into_any_element()
    }

    fn inbox_detail(&mut self, j: &Job, cx: &mut Context<Self>) -> impl IntoElement {
        let id = j.id;
        let face = self.bot(j.bot, runo_holo::Mood::Idle, 56., runo_holo::Framing::Bust);
        let headline = match j.status {
            Status::Review => format!("{} has changes for you to review", j.bot.name()),
            Status::Done if j.files.is_empty() => format!("{} finished", j.bot.name()),
            Status::Done => format!(
                "{} changed {} {}",
                j.bot.name(),
                j.files.len(),
                if j.files.len() == 1 { "file" } else { "files" }
            ),
            Status::Failed => format!("{} hit a problem", j.bot.name()),
            _ => format!("{} stopped before the end", j.bot.name()),
        };
        let said = j
            .last_words()
            .map(super::live::plain)
            .unwrap_or_else(|| "No reply was kept for this job.".into());
        let mut said_lines = div().flex().flex_col().gap(px(8.));
        for para in said.split("\n\n").filter(|p| !p.trim().is_empty()).take(6) {
            said_lines = said_lines.child(div().child(para.trim().to_string()));
        }
        let mut review = self.review_card(j, cx);
        let mut files = ui::card().overflow_hidden().child(
            div()
                .h(px(40.))
                .px(px(16.))
                .flex()
                .items_center()
                .border_b_1()
                .border_color(c(ui::LINE))
                .text_size(px(12.5))
                .font_weight(FontWeight::SEMIBOLD)
                .child("Files changed")
                .child(div().flex_1())
                .child(
                    div()
                        .font_weight(FontWeight::NORMAL)
                        .text_color(c(ui::FAINT))
                        .child(j.files.len().to_string()),
                ),
        );
        if j.files.is_empty() {
            files = files.child(
                div()
                    .px(px(16.))
                    .py(px(14.))
                    .text_size(px(12.5))
                    .text_color(c(ui::FAINT))
                    .child("Nothing was changed."),
            );
        }
        for f in j.files.iter().take(12) {
            files = files.child(
                div()
                    .h(px(34.))
                    .px(px(16.))
                    .flex()
                    .items_center()
                    .border_b_1()
                    .border_color(c(ui::LINE2))
                    .font_family(ui::MONO)
                    .text_size(px(12.))
                    .child(f.clone()),
            );
        }
        let mut plan = div().flex().flex_col().gap(px(7.));
        for (s, state) in &j.plan {
            plan = plan.child(
                div()
                    .flex()
                    .gap(px(8.))
                    .text_size(px(12.5))
                    .text_color(c(if *state == 2 { ui::BODY } else { ui::FAINT }))
                    .child(if *state == 2 { "✓" } else { "·" })
                    .child(s.clone()),
            );
        }
        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .id("inbox-detail")
                    .flex_1()
                    .overflow_y_scroll()
                    .px(px(28.))
                    .pt(px(24.))
                    .pb(px(24.))
                    .flex()
                    .flex_col()
                    .gap(px(16.))
                    .child(
                        div().flex().items_center().gap(px(16.)).child(face).child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .child(
                                    div()
                                        .text_size(px(18.))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(headline),
                                )
                                .child(
                                    div()
                                        .mt(px(3.))
                                        .text_size(px(12.5))
                                        .text_color(c(ui::MUTED))
                                        .truncate()
                                        .child(format!(
                                            "{} · Job #{id} · {}{}",
                                            j.title,
                                            if j.engine.is_empty() {
                                                "—"
                                            } else {
                                                &j.engine
                                            },
                                            if j.summary.is_empty() {
                                                String::new()
                                            } else {
                                                format!(" · {}", j.summary)
                                            }
                                        )),
                                ),
                        ),
                    )
                    .child(
                        ui::card()
                            .px(px(18.))
                            .py(px(14.))
                            .text_size(px(13.5))
                            .line_height(px(21.))
                            .text_color(c(ui::BODY))
                            .child(said_lines),
                    )
                    .map(|d| match review.take() {
                        Some(card) => d.child(card.ml(px(0.))),
                        None => d.child(files),
                    })
                    .when(!j.plan.is_empty(), |d| {
                        d.child(
                            ui::card()
                                .px(px(18.))
                                .py(px(14.))
                                .flex()
                                .flex_col()
                                .gap(px(10.))
                                .child(
                                    div()
                                        .text_size(px(11.))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(c(ui::FAINT))
                                        .child("PLAN"),
                                )
                                .child(plan),
                        )
                    }),
            )
            .child(
                div()
                    .h(px(60.))
                    .px(px(28.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .bg(c(ui::PANEL))
                    .border_t_1()
                    .border_color(c(ui::LINE))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(12.5))
                            .text_color(c(ui::MUTED))
                            .child(match j.status {
                                Status::Review => {
                                    "Approve to merge it, or open it to send it back."
                                }
                                Status::Done => "Look it over, then mark it seen or ask for more.",
                                _ => "Open the job to see where it stopped, or ask it to carry on.",
                            }),
                    )
                    .child(
                        ui::ghost(
                            "inbox-open",
                            if matches!(j.status, Status::Done | Status::Review) {
                                "Send back"
                            } else {
                                "Open job"
                            },
                        )
                        .on_click(cx.listener(move |this, _, _, cx| this.go(Place::Live(id), cx))),
                    )
                    .when(j.status == Status::Review, |d| {
                        let label = format!("Approve & merge into {}", j.base);
                        d.child(
                            ui::solid("inbox-approve", label)
                                .on_click(cx.listener(move |this, _, _, cx| this.approve(id, cx))),
                        )
                    })
                    .when(j.status != Status::Review, |d| {
                        d.child(
                            ui::solid("inbox-seen", "Mark as seen").on_click(cx.listener(
                                move |this, _, _, cx| {
                                    if let Some(j) = this.store.jobs.iter_mut().find(|j| j.id == id)
                                    {
                                        j.seen = true;
                                    }
                                    this.inbox_pick = None;
                                    this.store.save();
                                    cx.notify();
                                },
                            )),
                        )
                    }),
            )
    }

    pub(super) fn history(&mut self, cx: &mut Context<Self>) -> AnyElement {
        if self.store.jobs.is_empty() {
            return self.empty_place(
                "No jobs yet",
                "Every job the crew takes on is kept here, with what it said and changed.",
                cx,
            );
        }
        let filters = ["All", "Running", "Done", "Problems"];
        let mut tabs = div().flex().gap(px(8.));
        for (i, f) in filters.into_iter().enumerate() {
            tabs = tabs.child(
                ui::pill(("hist-filter", i), f, self.history_filter == i).on_click(cx.listener(
                    move |this, _, _, cx| {
                        this.history_filter = i;
                        cx.notify();
                    },
                )),
            );
        }
        let filter = self.history_filter;
        let query = self.history_input.read(cx).value().to_lowercase();
        let jobs: Vec<Job> = self
            .store
            .jobs
            .iter()
            .rev()
            .filter(|j| {
                let hay = format!(
                    "{} {} {} {}",
                    j.title,
                    j.bot.name(),
                    j.folder.name,
                    j.mode.label()
                )
                .to_lowercase();
                query.split_whitespace().all(|w| hay.contains(w))
            })
            .filter(|j| match filter {
                1 => j.status == Status::Running,
                2 => matches!(j.status, Status::Done | Status::Shipped),
                3 => matches!(
                    j.status,
                    Status::Failed | Status::Interrupted | Status::Stopped
                ),
                _ => true,
            })
            .cloned()
            .collect();
        let total = self.store.jobs.len();
        let list = if jobs.is_empty() {
            div()
                .py(px(20.))
                .text_size(px(13.))
                .text_color(c(ui::FAINT))
                .child(if query.trim().is_empty() {
                    "Nothing here.".to_string()
                } else {
                    format!("No jobs match \u{201c}{}\u{201d}.", query.trim())
                })
                .into_any_element()
        } else {
            ui::card()
                .overflow_hidden()
                .child(self.job_table(&jobs, true, 0, cx))
                .into_any_element()
        };
        page(
            "Jobs",
            format!(
                "{total} {} so far.",
                if total == 1 { "job" } else { "jobs" }
            ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(14.))
                .child(tabs)
                .child(div().flex_1())
                .child(
                    div()
                        .w(px(300.))
                        .h(px(36.))
                        .px(px(12.))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .rounded(px(12.))
                        .bg(c(ui::PANEL))
                        .border_1()
                        .border_color(c(ui::LINE))
                        .child(
                            gpui_kit::component::Icon::new(gpui_kit::assets::IconName::Search)
                                .size(px(14.))
                                .text_color(c(ui::FAINT)),
                        )
                        .child(
                            div().flex_1().text_size(px(13.)).child(
                                gpui_kit::component::input::Input::new(&self.history_input)
                                    .appearance(false)
                                    .bordered(false),
                            ),
                        ),
                ),
        )
        .child(list)
        .into_any_element()
    }
}

fn page(title: &'static str, line: String) -> Stateful<Div> {
    div()
        .id(title)
        .size_full()
        .overflow_y_scroll()
        .px(px(28.))
        .pt(px(24.))
        .pb(px(40. + super::DOCK_ROOM))
        .flex()
        .flex_col()
        .gap(px(18.))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(
                    div()
                        .text_size(px(24.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title),
                )
                .child(
                    div()
                        .text_size(px(14.))
                        .text_color(c(ui::MUTED))
                        .child(line),
                ),
        )
}
