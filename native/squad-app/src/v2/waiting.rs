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
}
