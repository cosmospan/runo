//! The Waiting strip over a thread's box: jobs in line to start later (the
//! night shift), for a bot that's busy, or while their issue is read, with
//! Start now and Cancel.

use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::store;
use super::ui::{self, c};
use super::Shell;

impl Shell {
    /// The jobs waiting to start in this thread, or None when there are none.
    pub(super) fn waiting_strip(
        &mut self,
        folder: &str,
        thread: u64,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let waiting: Vec<super::shift::Later> = self
            .crew
            .store
            .later
            .iter()
            .filter(|l| {
                l.folder.path == folder && self.crew.store.thread_for(folder, l.thread) == thread
            })
            .cloned()
            .collect();
        if waiting.is_empty() {
            return None;
        }
        let now = store::now();
        let mut list = div()
            .w_full()
            .flex()
            .flex_col()
            .rounded(px(10.))
            .bg(c(ui::raised()))
            .border_1()
            .border_color(c(ui::line()))
            .child(
                div()
                    .px(px(12.))
                    .pt(px(8.))
                    .pb(px(4.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_size(px(11.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(c(ui::muted()))
                    .child(Icon::new(IconName::Moon).size(px(12.)))
                    .child(format!("Waiting \u{b7} {}", waiting.len())),
            );
        for l in waiting {
            let id = l.id;
            let busy = self.crew.store.busy(l.bot).is_some();
            let when = if l.reading {
                "reading its issue".to_string()
            } else if l.at == 0 || l.at <= now {
                format!("when {} is free", l.bot.name())
            } else {
                super::shift::when_words(l.at, now)
            };
            let can_start = !l.reading && !busy;
            list = list.child(
                div()
                    .h(px(34.))
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .border_t_1()
                    .border_color(c(ui::line2()))
                    .child(self.avatar(l.bot, 20.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(12.5))
                            .text_color(c(ui::ink()))
                            .child(l.label().to_string()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap(px(4.))
                            .text_size(px(11.5))
                            .text_color(c(ui::faint()))
                            .child(Icon::new(IconName::Clock).size(px(11.)))
                            .child(when),
                    )
                    .when(can_start, |d| {
                        d.child(
                            ui::ghost(("later-now", id as usize), "Start now")
                                .h(px(24.))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    let st = Self::app_settings(cx);
                                    this.crew.start_later_now(&st, id);
                                    cx.notify();
                                })),
                        )
                    })
                    .child(
                        ui::icon_btn(("later-cancel", id as usize), IconName::X)
                            .tooltip(|window, cx| {
                                gpui_kit::component::tooltip::Tooltip::new("Cancel it")
                                    .build(window, cx)
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.crew.cancel_later(id);
                                cx.notify();
                            })),
                    ),
            );
        }
        Some(list)
    }

    /// The night shift on the Jobs page: every job waiting for its time across
    /// your projects, and what the night shift finished in the last day.
    /// Nothing when both are empty.
    pub(super) fn night_shift(&mut self, cx: &mut Context<Self>) -> Option<Div> {
        let now = store::now();
        let mut waiting: Vec<super::shift::Later> = self.crew.store.later.clone();
        waiting.sort_by_key(|l| l.at);
        let done: Vec<(
            u64,
            super::data::Member,
            String,
            String,
            super::store::Status,
        )> = self
            .crew
            .store
            .jobs
            .iter()
            .filter(|j| j.shift && j.ended.is_some_and(|e| now.saturating_sub(e) < 86_400))
            .map(|j| {
                (
                    j.id,
                    j.bot,
                    j.label().to_string(),
                    j.folder.name.clone(),
                    j.status,
                )
            })
            .collect();
        if waiting.is_empty() && done.is_empty() {
            return None;
        }
        let row = || {
            div()
                .h(px(38.))
                .px(px(14.))
                .flex()
                .items_center()
                .gap(px(10.))
                .border_t_1()
                .border_color(c(ui::line2()))
        };
        let quiet = |text: String| {
            div()
                .flex_none()
                .text_size(px(11.5))
                .text_color(c(ui::faint()))
                .child(text)
        };
        let title = |t: &'static str| {
            div()
                .px(px(14.))
                .pt(px(10.))
                .pb(px(6.))
                .flex()
                .items_center()
                .gap(px(6.))
                .text_size(px(11.5))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(c(ui::muted()))
                .child(t)
        };
        let mut card = ui::card().overflow_hidden().flex().flex_col();
        if !waiting.is_empty() {
            card = card.child(title("Waiting for their time"));
            for l in waiting {
                let id = l.id;
                let when = if l.reading {
                    "reading its issue".to_string()
                } else if l.at == 0 || l.at <= now {
                    format!("when {} is free", l.bot.name())
                } else {
                    super::shift::when_words(l.at, now)
                };
                card = card.child(
                    row()
                        .child(self.avatar(l.bot, 20.))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_size(px(13.))
                                .text_color(c(ui::ink()))
                                .child(l.label().to_string()),
                        )
                        .child(quiet(l.folder.name.clone()))
                        .child(
                            div()
                                .w(px(120.))
                                .flex_none()
                                .flex()
                                .justify_end()
                                .items_center()
                                .gap(px(4.))
                                .text_size(px(11.5))
                                .text_color(c(ui::muted()))
                                .child(Icon::new(IconName::Clock).size(px(11.)))
                                .child(when),
                        )
                        .child(
                            ui::ghost(("shift-now", id as usize), "Start now")
                                .h(px(24.))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    let st = Self::app_settings(cx);
                                    this.crew.start_later_now(&st, id);
                                    cx.notify();
                                })),
                        )
                        .child(
                            ui::icon_btn(("shift-cancel", id as usize), IconName::X).on_click(
                                cx.listener(move |this, _, _, cx| {
                                    this.crew.cancel_later(id);
                                    cx.notify();
                                }),
                            ),
                        ),
                );
            }
        }
        if !done.is_empty() {
            card = card.child(title("Done on the night shift"));
            for (id, bot, label, project, status) in done {
                card = card.child(
                    row()
                        .id(("shift-done", id as usize))
                        .cursor_pointer()
                        .hover(|s| s.bg(c(ui::hover())))
                        .child(self.avatar(bot, 20.))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_size(px(13.))
                                .text_color(c(ui::ink()))
                                .child(label),
                        )
                        .child(quiet(project))
                        .child(div().w(px(120.)).flex_none().flex().justify_end().child(
                            ui::status(super::live::status_color(status), status.label()),
                        ))
                        .on_click(cx.listener(move |this, _, _, cx| this.open_job(id, cx))),
                );
            }
        }
        Some(
            div()
                .flex()
                .flex_col()
                .gap(px(12.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .mt(px(8.))
                        .child(
                            Icon::new(IconName::Moon)
                                .size(px(14.))
                                .text_color(c(ui::muted())),
                        )
                        .child(ui::heading("Night shift", None, None)),
                )
                .child(card),
        )
    }
}
