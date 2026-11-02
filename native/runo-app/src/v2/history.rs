//! Inbox (finished work you haven't looked at, and jobs that stopped with a
//! problem) and History (every job, newest first).

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::live::status_color;
use super::store::{self, Job, Status};
use super::ui::{self, c};
use super::{Place, Shell};

impl Shell {
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
        let count = jobs.len();
        let list = self.job_list(&jobs, cx);
        page(
            "Inbox",
            format!(
                "{count} {} for you to look at.",
                if count == 1 { "thing" } else { "things" }
            ),
        )
        .child(div().flex().justify_end().child(
            ui::ghost("mark-all", "Mark all as seen").on_click(cx.listener(|this, _, _, cx| {
                for j in &mut this.store.jobs {
                    if j.status != Status::Running {
                        j.seen = true;
                    }
                }
                this.store.save();
                cx.notify();
            })),
        ))
        .child(list)
        .into_any_element()
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
                2 => j.status == Status::Done,
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
            self.job_list(&jobs, cx).into_any_element()
        };
        page(
            "History",
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
                        .bg(c(0xFFFFFF))
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

    fn job_list(&mut self, jobs: &[Job], cx: &mut Context<Self>) -> impl IntoElement {
        let mut list = ui::card().p(px(8.)).flex().flex_col();
        for (n, j) in jobs.iter().enumerate() {
            let face = self.avatar(j.bot, 34.);
            let id = j.id;
            let color = status_color(j.status);
            let when = match store::ago(j.started).as_str() {
                "just now" => "just now".to_string(),
                a => format!("{a} ago"),
            };
            let detail = match j.status {
                Status::Running => j.doing(),
                _ => j
                    .last_words()
                    .map(|t| {
                        super::live::plain(t)
                            .lines()
                            .last()
                            .unwrap_or("")
                            .to_string()
                    })
                    .unwrap_or_default(),
            };
            let k = self.stagger(n + 1);
            list = list
                .when(n > 0, |d| {
                    d.child(div().mx(px(14.)).h(px(1.)).bg(c(ui::LINE)))
                })
                .child(
                    div()
                        .id(("hist", id as usize))
                        .flex()
                        .items_center()
                        .gap(px(14.))
                        .px(px(14.))
                        .py(px(12.))
                        .rounded(px(14.))
                        .cursor_pointer()
                        .hover(|s| s.bg(c(0xF6F7FB)))
                        .child(face)
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
                                                .text_size(px(14.))
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .truncate()
                                                .child(j.title.clone()),
                                        )
                                        .when(!j.seen && j.status != Status::Running, |d| {
                                            d.child(
                                                div()
                                                    .flex_none()
                                                    .size(px(7.))
                                                    .rounded_full()
                                                    .bg(c(ui::ORANGE)),
                                            )
                                        }),
                                )
                                .child(
                                    div()
                                        .text_size(px(12.))
                                        .text_color(c(ui::MUTED))
                                        .truncate()
                                        .child(if detail.is_empty() {
                                            j.mode.label().to_string()
                                        } else {
                                            detail
                                        }),
                                ),
                        )
                        .child(
                            div()
                                .flex_none()
                                .w(px(220.))
                                .flex()
                                .flex_col()
                                .items_end()
                                .gap(px(3.))
                                .child(ui::status(color, j.status.label()))
                                .child(div().text_size(px(11.5)).text_color(c(ui::FAINT)).child(
                                    format!("{} · {} · {when}", j.bot.name(), j.folder.name),
                                )),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| this.go(Place::Live(id), cx)))
                        .opacity(k)
                        .relative()
                        .top(px(8. * (1. - k))),
                );
        }
        list
    }
}

fn page(title: &'static str, line: String) -> Stateful<Div> {
    div()
        .id(title)
        .size_full()
        .overflow_y_scroll()
        .px(px(40.))
        .pt(px(if cfg!(target_os = "macos") { 40. } else { 30. }))
        .pb(px(40.))
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
                        .text_size(px(34.))
                        .font_weight(FontWeight::BOLD)
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
