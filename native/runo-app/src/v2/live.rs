//! Live: watching a bot work. Its feed on the left in plain sentences, a live
//! preview and the plan on the right, and a box to steer it without stopping.

use gpui_kit::assets::IconName;
use gpui_kit::component::input::Input;
use gpui_kit::component::Icon;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use runo_holo::{Framing, Mood};

use super::data::{self, Feed, Member, Span};
use super::ui::{self, c, ca};
use super::Shell;

impl Shell {
    pub(super) fn live(&mut self, who: Member, cx: &mut Context<Self>) -> AnyElement {
        div()
            .size_full()
            .flex()
            .child(self.live_main(who, cx))
            .child(self.live_side())
            .into_any_element()
    }

    fn live_main(&mut self, who: Member, cx: &mut Context<Self>) -> impl IntoElement {
        let now = self.now();
        let paused = self.live_paused;
        let hero = self.bot(
            who,
            if paused { Mood::Idle } else { Mood::Working },
            150.,
            Framing::Full,
        );
        let tabs = ["Live", "Plan · 5 steps", "Changes · 4 files", "Tests"];
        let mut tab_row = div().flex().gap(px(8.)).mt(px(18.));
        for (i, t) in tabs.into_iter().enumerate() {
            tab_row = tab_row.child(ui::pill(("live-tab", i), t, self.live_tab == i).on_click(
                cx.listener(move |this, _, _, cx| {
                    this.live_tab = i;
                    cx.notify();
                }),
            ));
        }
        let progress = (0.56 + 0.0015 * now as f32).min(0.7);
        let header =
            div()
                .flex_none()
                .flex()
                .gap(px(14.))
                .px(px(30.))
                .pt(px(16.))
                .pb(px(18.))
                .border_b_1()
                .border_color(c(ui::LINE))
                .bg(linear_gradient(
                    180.,
                    linear_color_stop(c(0xFFF1E8), 0.0),
                    linear_color_stop(ca(0xFFFFFF, 0.0), 1.0),
                ))
                .child(div().mt(px(-6.)).child(hero))
                .child(
                    div()
                        .flex_1()
                        .pt(px(16.))
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.))
                                .text_size(px(13.))
                                .child(div().font_weight(FontWeight::SEMIBOLD).child(who.name()))
                                .child(div().text_color(c(ui::MUTED)).child(if paused {
                                    "is paused"
                                } else {
                                    "is building"
                                }))
                                .child(
                                    div()
                                        .ml(px(4.))
                                        .px(px(9.))
                                        .py(px(3.))
                                        .rounded_full()
                                        .border_1()
                                        .border_color(c(ui::LINE))
                                        .bg(c(0xFFFFFF))
                                        .text_size(px(11.5))
                                        .child("Claude Code"),
                                ),
                        )
                        .child(
                            div()
                                .mt(px(6.))
                                .text_size(px(34.))
                                .font_weight(FontWeight::BOLD)
                                .child("Partial refunds"),
                        )
                        .child(
                            div()
                                .mt(px(10.))
                                .flex()
                                .items_center()
                                .gap(px(12.))
                                .child(div().w(px(240.)).child(ui::bar(progress, ui::ORANGE)))
                                .child(div().text_size(px(12.5)).text_color(c(ui::MUTED)).child(
                                    format!(
                                        "step 3 of 5 · {} min · ${:.2}",
                                        18 + (now / 60.0) as u32,
                                        0.41 + 0.0003 * now
                                    ),
                                )),
                        )
                        .child(tab_row),
                )
                .child(
                    div()
                        .pt(px(20.))
                        .flex()
                        .gap(px(8.))
                        .child(
                            ui::ghost("pause", if paused { "Resume" } else { "Pause" }).on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.live_paused = !this.live_paused;
                                    cx.notify();
                                }),
                            ),
                        )
                        .child(ui::ghost("take-over", "Take over").on_click(cx.listener(
                            |this, _, _, cx| {
                                this.say(
                                "v1 opens with Builder's session; v2's own terminal comes next.",
                            );
                                cx.defer(crate::open_main_window);
                            },
                        )))
                        .child(ui::ghost("stop", "Stop").text_color(c(ui::RED)).on_click(
                            cx.listener(|this, _, _, cx| {
                                this.say("Builder stopped. Nothing was merged.");
                                this.live_paused = true;
                                cx.notify();
                            }),
                        )),
                );

        // The feed.
        let mut feed = div().flex().flex_col().gap(px(12.)).px(px(30.)).py(px(20.));
        let rows = data::live_feed();
        let extra = self.live_extra.clone();
        for row in rows {
            feed = feed.child(self.feed_row(row, now));
        }
        for (i, text) in extra.iter().enumerate() {
            feed = feed.child(you_row(text.clone().into()));
            let face = self.avatar(Member::Builder, 30.);
            feed = feed.child(div().flex().gap(px(14.)).child(face).child(bubble().child(
                if i % 2 == 0 {
                    "Got it. I'll fold that in before Checker starts."
                } else {
                    "Done. Added it to the plan."
                },
            )));
        }

        let face = self.avatar(Member::Builder, 22.);
        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .child(header)
            .child(
                div()
                    .id("live-feed")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(feed),
            )
            .child(
                div().flex_none().px(px(30.)).pb(px(22.)).pt(px(6.)).child(
                    ui::card()
                        .h(px(58.))
                        .pl(px(16.))
                        .pr(px(10.))
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .child(face)
                        .child(
                            div()
                                .flex_1()
                                .child(Input::new(&self.live_input).appearance(false)),
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

    fn feed_row(&mut self, row: Feed, now: f64) -> AnyElement {
        match row {
            Feed::Bot(who, spans) => {
                let face = self.avatar(who, 30.);
                let mut line = div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(5.))
                    .text_size(px(14.))
                    .line_height(px(22.));
                for s in spans {
                    line = match s {
                        Span::Text(t) => line.child(t),
                        Span::Code(t) => line.child(ui::code(t)),
                    };
                }
                div()
                    .flex()
                    .gap(px(14.))
                    .child(face)
                    .child(bubble().child(line))
                    .into_any_element()
            }
            Feed::Terminal(title, lines) => {
                let face = self.avatar(Member::Builder, 30.);
                let mut term = div()
                    .flex_1()
                    .rounded(px(16.))
                    .bg(c(0x1A1924))
                    .px(px(18.))
                    .py(px(16.))
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .font_family(ui::MONO)
                    .text_size(px(12.5))
                    .text_color(c(0xE9E9F2))
                    .child(
                        div()
                            .font_family("Geist")
                            .text_size(px(12.))
                            .text_color(c(0x9A9CB0))
                            .child(title),
                    );
                // The last test finishes a few seconds after the screen opens.
                let done_at = 6.0;
                for (text, ok) in lines {
                    let (status, color) = match ok {
                        Some(s) => (s.to_string(), 0x5BE8A6),
                        None if now > done_at => ("ok".into(), 0x5BE8A6),
                        None => (".".repeat(1 + ((now * 3.0) as usize % 4)), 0x9A9CB0),
                    };
                    term = term.child(
                        div()
                            .flex()
                            .gap(px(8.))
                            .child(text)
                            .child(div().text_color(c(color)).child(status)),
                    );
                }
                if now > done_at {
                    term = term.child(
                        div()
                            .mt(px(4.))
                            .text_color(c(0x9A9CB0))
                            .child("10 passed in 2.4s"),
                    );
                }
                div()
                    .flex()
                    .gap(px(14.))
                    .child(face)
                    .child(term)
                    .into_any_element()
            }
            Feed::You(text) => you_row(text.into()).into_any_element(),
            Feed::Next(who, text) => {
                let face = self.avatar(who, 30.);
                div()
                    .flex()
                    .gap(px(14.))
                    .opacity(0.55)
                    .child(face)
                    .child(bubble().text_color(c(ui::MUTED)).child(text))
                    .into_any_element()
            }
        }
    }

    fn live_side(&mut self) -> impl IntoElement {
        let mut plan = div().flex().flex_col().gap(px(14.)).mt(px(14.));
        for (text, who, state) in data::PLAN {
            let mark: AnyElement = match (state, who) {
                (2, _) => div()
                    .size(px(20.))
                    .rounded_full()
                    .bg(ca(ui::GREEN, 0.15))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        Icon::new(IconName::Check)
                            .size(px(12.))
                            .text_color(c(ui::GREEN)),
                    )
                    .into_any_element(),
                (_, Some(w)) => self.avatar(w, 22.).into_any_element(),
                _ => div().size(px(20.)).into_any_element(),
            };
            plan = plan.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .text_size(px(13.5))
                    .when(state == 0, |d| d.text_color(c(ui::FAINT)))
                    .when(state == 1, |d| d.font_weight(FontWeight::SEMIBOLD))
                    .child(mark)
                    .child(text),
            );
        }
        let trio = [Member::Builder, Member::Checker, Member::Guard].map(|m| self.avatar(m, 40.));
        let mut stack = div().flex().w(px(110.));
        for (i, f) in trio.into_iter().enumerate() {
            stack = stack.child(div().when(i > 0, |d| d.ml(px(-10.))).child(f));
        }
        div()
            .w(px(400.))
            .flex_none()
            .h_full()
            .border_l_1()
            .border_color(c(ui::LINE))
            .bg(ca(0xFFFFFF, 0.35))
            .px(px(22.))
            .py(px(24.))
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_baseline()
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Live preview"),
                    )
                    .child(
                        div()
                            .font_family(ui::MONO)
                            .text_size(px(11.))
                            .text_color(c(ui::MUTED))
                            .child("admin.acme.shop/refunds"),
                    ),
            )
            .child(
                ui::card()
                    .mt(px(14.))
                    .p(px(20.))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(c(ui::MUTED))
                            .child("Order 48213 · Léa Martin"),
                    )
                    .child(
                        div()
                            .mt(px(4.))
                            .text_size(px(20.))
                            .font_weight(FontWeight::BOLD)
                            .child("Refund items"),
                    )
                    .child(preview_line("☑ Linen shirt", "$42.00", false).mt(px(14.)))
                    .child(preview_line("☐ Shipping", "$6.00", true).mt(px(10.)))
                    .child(div().my(px(14.)).h(px(1.)).bg(c(ui::LINE)))
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .text_size(px(15.))
                            .font_weight(FontWeight::BOLD)
                            .child("Refund")
                            .child("$42.00"),
                    )
                    .child(
                        div()
                            .relative()
                            .mt(px(10.))
                            .h(px(40.))
                            .rounded(px(10.))
                            .bg(c(ui::INK))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(c(0xFFFFFF))
                            .text_size(px(13.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Refund $42.00")
                            .child(
                                div()
                                    .absolute()
                                    .top(px(-9.))
                                    .right(px(-8.))
                                    .px(px(8.))
                                    .py(px(2.))
                                    .rounded_full()
                                    .bg(c(ui::ORANGE))
                                    .border_2()
                                    .border_color(c(0xFFFFFF))
                                    .text_size(px(10.5))
                                    .font_weight(FontWeight::BOLD)
                                    .child("new"),
                            ),
                    ),
            )
            .child(
                div()
                    .mt(px(26.))
                    .text_size(px(14.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Plan"),
            )
            .child(plan)
            .child(div().flex_1())
            .child(
                ui::card()
                    .p(px(18.))
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(stack)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(3.))
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_weight(FontWeight::BOLD)
                                    .child("Ready for you in about 12 min"),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(c(ui::MUTED))
                                    .child("You'll get one question at the end, not ten."),
                            ),
                    ),
            )
    }
}

fn bubble() -> Div {
    ui::card()
        .flex_1()
        .px(px(18.))
        .py(px(13.))
        .rounded(px(16.))
        .text_size(px(14.))
        .line_height(px(22.))
}

fn you_row(text: SharedString) -> Div {
    div()
        .flex()
        .gap(px(14.))
        .items_center()
        .child(
            div()
                .size(px(30.))
                .flex_none()
                .rounded_full()
                .bg(c(ui::INK))
                .text_color(c(0xFFFFFF))
                .text_size(px(12.))
                .font_weight(FontWeight::BOLD)
                .flex()
                .items_center()
                .justify_center()
                .child("A"),
        )
        .child(
            div()
                .px(px(16.))
                .py(px(10.))
                .rounded(px(14.))
                .bg(c(ui::INK))
                .text_color(c(0xFFFFFF))
                .text_size(px(14.))
                .font_weight(FontWeight::MEDIUM)
                .child(text),
        )
}

fn preview_line(label: &'static str, price: &'static str, faint: bool) -> Div {
    div()
        .flex()
        .justify_between()
        .text_size(px(13.5))
        .when(faint, |d| d.text_color(c(ui::MUTED)))
        .child(label)
        .child(price)
}
