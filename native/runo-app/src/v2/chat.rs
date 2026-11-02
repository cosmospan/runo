//! Chat: asking the crew in plain words, and handing work between bots.

use gpui_kit::assets::IconName;
use gpui_kit::component::input::Input;
use gpui_kit::component::Icon;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use runo_holo::{Framing, Mood};

use super::data::{self, Member};
use super::ui::{self, c, ca};
use super::Shell;

const PURPLE: u32 = 0x7B4DF5;

impl Shell {
    pub(super) fn chat(&mut self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .size_full()
            .flex()
            .child(self.chat_list(cx))
            .child(self.chat_thread(cx))
            .into_any_element()
    }

    fn chat_list(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut list = div().mt(px(16.)).flex().flex_col().gap(px(4.));
        for (i, item) in data::CHATS.iter().enumerate() {
            let on = self.chat_selected == i;
            let face: AnyElement = if item.group {
                let a = self.avatar(Member::Builder, 30.);
                let b = self.avatar(Member::Checker, 30.);
                div()
                    .relative()
                    .size(px(44.))
                    .child(div().absolute().top_0().left_0().child(a))
                    .child(div().absolute().bottom_0().right_0().child(b))
                    .into_any_element()
            } else {
                self.bot(item.who, item.who.mood(), 44., Framing::Bust)
                    .into_any_element()
            };
            list = list.child(
                div()
                    .id(("chat", i))
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .px(px(12.))
                    .py(px(10.))
                    .rounded(px(16.))
                    .cursor_pointer()
                    .map(|d| {
                        if on {
                            d.bg(c(0xFFFFFF)).shadow(ui::lift())
                        } else {
                            d.hover(|s| s.bg(ca(0xFFFFFF, 0.6)))
                        }
                    })
                    .child(face)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(
                                div()
                                    .flex()
                                    .justify_between()
                                    .child(
                                        div()
                                            .text_size(px(14.))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(item.title),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.5))
                                            .text_color(c(ui::MUTED))
                                            .child(item.when),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .text_color(c(ui::MUTED))
                                    .truncate()
                                    .child(item.last),
                            ),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.chat_selected = i;
                        cx.notify();
                    })),
            );
        }
        div()
            .w(px(300.))
            .flex_none()
            .h_full()
            .border_r_1()
            .border_color(c(ui::LINE))
            .bg(ca(0xFFFFFF, 0.3))
            .px(px(14.))
            .pt(px(24.))
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .px(px(8.))
                    .child(
                        div()
                            .text_size(px(22.))
                            .font_weight(FontWeight::BOLD)
                            .child("Chats"),
                    )
                    .child(
                        ui::ghost("new-chat", "New")
                            .h(px(32.))
                            .px(px(12.))
                            .gap(px(6.))
                            .flex_row_reverse()
                            .child(
                                Icon::new(IconName::Plus)
                                    .size(px(13.))
                                    .text_color(c(ui::INK)),
                            ),
                    ),
            )
            .child(list)
    }

    fn chat_thread(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let item = &data::CHATS[self.chat_selected.min(data::CHATS.len() - 1)];
        let (who, group) = (item.who, item.group);
        let hero: AnyElement = if group {
            let row = [Member::Builder, Member::Checker, Member::Guard]
                .map(|m| self.bot(m, Mood::Idle, 64., Framing::Bust));
            div().flex().gap(px(4.)).children(row).into_any_element()
        } else {
            self.bot(who, Mood::Signature, 118., Framing::Full)
                .into_any_element()
        };
        let (name, about) = if group {
            (
                "#checkout crew",
                "Builder, Checker and Guard · with Priya and you",
            )
        } else {
            match who {
                Member::Explainer => (
                    "Explainer",
                    "Answers in plain words · reads code, never changes it",
                ),
                other => (other.name(), other.role()),
            }
        };
        let header = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(14.))
            .px(px(24.))
            .py(px(4.))
            .h(px(126.))
            .border_b_1()
            .border_color(c(ui::LINE))
            .bg(linear_gradient(
                180.,
                linear_color_stop(c(0xEAF4FF), 0.0),
                linear_color_stop(ca(0xFFFFFF, 0.0), 1.0),
            ))
            .child(hero)
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(
                        div()
                            .text_size(px(22.))
                            .font_weight(FontWeight::BOLD)
                            .child(name),
                    )
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(c(ui::MUTED))
                            .child(about),
                    ),
            )
            .when(who == Member::Explainer && !group, |d| {
                d.child(
                    div()
                        .px(px(12.))
                        .py(px(6.))
                        .rounded_full()
                        .border_1()
                        .border_color(c(ui::LINE))
                        .bg(c(0xFFFFFF))
                        .text_size(px(12.5))
                        .child("Priya is asking"),
                )
            });

        let mut msgs = div().flex().flex_col().gap(px(12.)).px(px(28.)).py(px(22.));
        if who == Member::Explainer && !group {
            msgs = msgs.child(person(
                "P",
                PURPLE,
                "Why is checkout slower this week? Customers are complaining.",
            ));
            let face = self.avatar(Member::Explainer, 32.);
            msgs = msgs.child(
                div()
                    .flex()
                    .gap(px(14.))
                    .child(face)
                    .child(
                        div()
                            .w(px(600.))
                            .flex()
                            .flex_col()
                            .gap(px(10.))
                            .child(answer().child("Product photos got much bigger on Tuesday. Each one went from about 120 KB to 2 MB, so the checkout page now loads 18 MB instead of 1.6 MB. On phones that's about 4 extra seconds."))
                            .child(size_chart(self.now()))
                            .child(answer().child("The fix is to shrink photos when they're uploaded. Builder can do it in about 10 minutes, and nothing changes for your team."))
                            .child(self.handoff_buttons(cx)),
                    ),
            );
            if self.handed_to_builder {
                let face = self.avatar(Member::Builder, 32.);
                msgs = msgs.child(
                    div().flex().gap(px(14.)).child(face).child(
                        div()
                            .w(px(600.))
                            .px(px(18.))
                            .py(px(13.))
                            .rounded(px(16.))
                            .border_1()
                            .border_color(c(0xFFD4BF))
                            .bg(c(0xFFF4EE))
                            .text_size(px(14.))
                            .line_height(px(22.))
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap(px(4.))
                                    .child(div().font_weight(FontWeight::BOLD).child("Builder"))
                                    .child("joined. Got it, shrinking photos on upload. I'll send Priya a before and after when it's done."),
                            ),
                    ),
                );
            }
        } else {
            let face = if group {
                self.avatar(Member::Builder, 32.)
            } else {
                self.avatar(who, 32.)
            };
            msgs = msgs.child(
                div()
                    .flex()
                    .gap(px(14.))
                    .child(face)
                    .child(answer().max_w(px(600.)).child(item.last)),
            );
        }
        for (mine, text) in self.chat_extra.clone() {
            if mine {
                msgs = msgs.child(person("A", ui::INK, text));
            } else {
                let face = self.avatar(who, 32.);
                msgs = msgs.child(
                    div()
                        .flex()
                        .gap(px(14.))
                        .child(face)
                        .child(answer().max_w(px(600.)).child(text)),
                );
            }
        }

        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .child(header)
            .child(
                div()
                    .id("chat-msgs")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(msgs),
            )
            .child(
                div().flex_none().px(px(24.)).pb(px(22.)).child(
                    ui::card()
                        .h(px(58.))
                        .pl(px(18.))
                        .pr(px(10.))
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .child(
                            div()
                                .flex_1()
                                .child(Input::new(&self.chat_input).appearance(false)),
                        )
                        .child(
                            div()
                                .size(px(36.))
                                .rounded_full()
                                .border_1()
                                .border_color(c(ui::LINE))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    Icon::new(IconName::Mic)
                                        .size(px(14.))
                                        .text_color(c(ui::INK)),
                                ),
                        )
                        .child(
                            div()
                                .size(px(36.))
                                .rounded_full()
                                .bg(c(ui::INK))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    Icon::new(IconName::ArrowUp)
                                        .size(px(15.))
                                        .text_color(c(0xFFFFFF)),
                                ),
                        ),
                ),
            )
    }

    fn handoff_buttons(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let face = self.avatar(Member::Builder, 22.);
        let handed = self.handed_to_builder;
        div()
            .flex()
            .gap(px(8.))
            .child(
                ui::solid(
                    "hand-to-builder",
                    if handed {
                        "With Builder"
                    } else {
                        "Hand to Builder"
                    },
                )
                .px(px(16.))
                .flex_row_reverse()
                .child(face)
                .on_click(cx.listener(|this, _, _, cx| {
                    if !this.handed_to_builder {
                        this.handed_to_builder = true;
                        let now = this.now();
                        this.jobs.insert(
                            0,
                            data::Job {
                                title: "Shrink photos on upload".into(),
                                bot: Member::Builder,
                                color: ui::ORANGE,
                                start: 0.05,
                                rate: 0.01,
                                note: None,
                                since: now,
                            },
                        );
                        this.say("Builder has it. It's on Today under Working now.");
                    }
                    cx.notify();
                })),
            )
            .child(ui::ghost("simpler", "Explain it simpler"))
            .child(ui::ghost("show-code", "Show me the code"))
    }
}

fn answer() -> Div {
    ui::card()
        .px(px(18.))
        .py(px(13.))
        .rounded(px(16.))
        .text_size(px(14.))
        .line_height(px(22.))
}

fn person(initial: &'static str, color: u32, text: impl Into<SharedString>) -> Div {
    div()
        .flex()
        .justify_end()
        .items_center()
        .gap(px(10.))
        .child(
            div()
                .max_w(px(560.))
                .px(px(16.))
                .py(px(10.))
                .rounded(px(16.))
                .bg(c(color))
                .text_color(c(0xFFFFFF))
                .text_size(px(14.))
                .font_weight(FontWeight::MEDIUM)
                .child(text.into()),
        )
        .child(
            div()
                .size(px(30.))
                .flex_none()
                .rounded_full()
                .bg(c(color))
                .text_color(c(0xFFFFFF))
                .text_size(px(12.))
                .font_weight(FontWeight::BOLD)
                .flex()
                .items_center()
                .justify_center()
                .child(initial),
        )
}

/// The checkout size comparison; the bars grow in when the chat opens.
fn size_chart(now: f64) -> impl IntoElement {
    let k = runo_holo::anim::SOFT.at((now / 0.9).min(1.0)) as f32;
    let row = |label: &'static str, frac: f32, color: Hsla, value: &'static str| {
        div()
            .flex()
            .items_center()
            .gap(px(12.))
            .text_size(px(13.))
            .child(div().w(px(72.)).text_color(c(ui::MUTED)).child(label))
            .child(
                div()
                    .w(px(380.) * frac * k)
                    .h(px(12.))
                    .rounded(px(4.))
                    .bg(color),
            )
            .child(div().font_weight(FontWeight::MEDIUM).child(value))
    };
    answer()
        .flex()
        .flex_col()
        .gap(px(10.))
        .child(
            div()
                .text_size(px(14.))
                .font_weight(FontWeight::SEMIBOLD)
                .child("Checkout page size"),
        )
        .child(row(
            "Last week",
            1.6 / 18.0 + 0.02,
            ca(ui::BLUE, 0.45),
            "1.6 MB",
        ))
        .child(row("This week", 1.0, c(ui::BLUE), "18 MB"))
        .child(
            div()
                .text_size(px(11.5))
                .text_color(c(ui::MUTED))
                .child("From the image upload change in PR #471 · checked 120 files"),
        )
}
