//! Today: the crew up top, one box to drop a job in, and the day in three
//! cards (the question waiting for you, what's being worked on, what landed).

use gpui_kit::assets::IconName;
use gpui_kit::component::input::Input;
use gpui_kit::component::Icon;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use runo_holo::{Framing, Mood};

use super::data::{self, Member, Mode};
use super::ui::{self, c, ca};
use super::{Place, Shell};

fn greeting() -> &'static str {
    use chrono::Timelike;
    match chrono::Local::now().hour() {
        5..=11 => "Good morning, Anish.",
        12..=17 => "Good afternoon, Anish.",
        _ => "Good evening, Anish.",
    }
}

impl Shell {
    pub(super) fn today(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let answered = self.guard_answer.is_some();
        // The crew, Guard in the middle and bigger while it waits on you.
        let mut row = div()
            .flex_none()
            .h(px(196.))
            .flex()
            .items_end()
            .justify_center()
            .gap(px(14.));
        for who in [
            Member::Builder,
            Member::Checker,
            Member::Guard,
            Member::Fixer,
            Member::Explainer,
            Member::Keeper,
            Member::Mochi,
        ] {
            let asking = who == Member::Guard && !answered;
            let mood = if who == Member::Guard && answered {
                Mood::Done
            } else {
                who.mood()
            };
            let size = if asking { 150. } else { 104. };
            let face = self.bot(who, mood, size, Framing::Full);
            let (line, color) = if who == Member::Guard && answered {
                ("all set", Some(ui::GREEN))
            } else {
                who.short_status()
            };
            row = row.child(
                div()
                    .id(("crew-top", who.key() as usize))
                    .relative()
                    .w(px(if asking { 150. } else { 104. }))
                    .flex()
                    .flex_col()
                    .items_center()
                    .cursor_pointer()
                    .when(asking, |d| {
                        d.child(
                            div()
                                .absolute()
                                .top(px(-6.))
                                .px(px(10.))
                                .py(px(5.))
                                .rounded(px(10.))
                                .bg(c(ui::INK))
                                .text_color(c(0xFFFFFF))
                                .text_size(px(12.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("Got a sec?"),
                        )
                    })
                    .child(face)
                    .child(
                        div()
                            .mt(px(-6.))
                            .text_size(px(14.))
                            .font_weight(FontWeight::BOLD)
                            .child(who.name()),
                    )
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(c(color.unwrap_or(ui::MUTED)))
                            .when(color.is_some(), |d| d.font_weight(FontWeight::MEDIUM))
                            .child(line),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let to = if who == Member::Builder {
                            Place::Live(who)
                        } else {
                            Place::Crew
                        };
                        this.go(to, cx);
                    })),
            );
        }
        row = row.child(
            div()
                .id("make-one")
                .w(px(96.))
                .pb(px(2.))
                .flex()
                .flex_col()
                .items_center()
                .gap(px(4.))
                .cursor_pointer()
                .child(
                    div()
                        .size(px(60.))
                        .mb(px(22.))
                        .rounded_full()
                        .border_1()
                        .border_dashed()
                        .border_color(c(0xC9CBD8))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            Icon::new(IconName::Plus)
                                .size(px(18.))
                                .text_color(c(ui::MUTED)),
                        ),
                )
                .child(
                    div()
                        .text_size(px(14.))
                        .font_weight(FontWeight::BOLD)
                        .child("Make one"),
                )
                .child(
                    div()
                        .text_size(px(11.5))
                        .text_color(c(ui::MUTED))
                        .child("your own bot"),
                )
                .on_click(cx.listener(|this, _, _, cx| this.go(Place::Crew, cx))),
        );

        let shipped = 6 + usize::from(self.guard_answer == Some(true));
        let sub = if answered {
            format!(
                "The crew shipped {shipped} things while you were away. Nothing is waiting on you."
            )
        } else {
            format!(
                "The crew shipped {shipped} things while you were away. One question is waiting."
            )
        };

        div()
            .id("today")
            .size_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .items_center()
            .px(px(40.))
            .pt(px(30.))
            .pb(px(30.))
            .child(row)
            .child(
                div()
                    .mt(px(18.))
                    .text_size(px(52.))
                    .font_weight(FontWeight::BOLD)
                    .line_height(px(58.))
                    .child(greeting()),
            )
            .child(
                div()
                    .mt(px(8.))
                    .text_size(px(15.))
                    .text_color(c(ui::MUTED))
                    .child(sub),
            )
            .child(self.composer_card(cx))
            .child(self.suggestions(cx))
            .child(
                div()
                    .mt(px(40.))
                    .w_full()
                    .max_w(px(1280.))
                    .flex()
                    .gap(px(18.))
                    .child(self.question_card(cx))
                    .child(self.working_card(cx))
                    .child(self.landed_card()),
            )
            .into_any_element()
    }

    fn composer_card(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let target = Member::for_mode(self.mode);
        let face = self.avatar(target, 22.);
        let mut modes = div()
            .flex()
            .items_center()
            .p(px(3.))
            .gap(px(2.))
            .rounded(px(12.))
            .bg(c(0xF1F2F7));
        for (i, m) in Mode::ALL.into_iter().enumerate() {
            let on = self.mode == m;
            modes = modes.child(
                div()
                    .id(("mode", i))
                    .px(px(12.))
                    .h(px(30.))
                    .flex()
                    .items_center()
                    .rounded(px(9.))
                    .text_size(px(13.))
                    .cursor_pointer()
                    .map(|d| {
                        if on {
                            d.bg(c(0xFFFFFF))
                                .shadow(ui::lift())
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(c(ui::INK))
                        } else {
                            d.text_color(c(ui::MUTED))
                                .hover(|s| s.text_color(c(ui::INK)))
                        }
                    })
                    .child(m.label())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.mode = m;
                        cx.notify();
                    })),
            );
        }
        ui::card()
            .mt(px(26.))
            .w(px(820.))
            .px(px(20.))
            .pt(px(16.))
            .pb(px(14.))
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(
                div().text_size(px(16.)).child(
                    Input::new(&self.composer)
                        .appearance(false)
                        .text_size(px(16.)),
                ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .size(px(34.))
                            .rounded_full()
                            .border_1()
                            .border_color(c(ui::LINE))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                Icon::new(IconName::Plus)
                                    .size(px(15.))
                                    .text_color(c(ui::INK)),
                            ),
                    )
                    .child(modes)
                    .child(ui::pill("think", "Think hard", false))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .pl(px(6.))
                            .pr(px(12.))
                            .h(px(32.))
                            .rounded_full()
                            .border_1()
                            .border_color(c(ui::LINE))
                            .text_size(px(13.))
                            .child(face)
                            .child(format!("Goes to {}", target.name())),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .size(px(38.))
                            .rounded(px(12.))
                            .border_1()
                            .border_color(c(ui::LINE))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                Icon::new(IconName::Mic)
                                    .size(px(15.))
                                    .text_color(c(ui::INK)),
                            ),
                    )
                    .child(
                        div()
                            .id("send")
                            .size(px(38.))
                            .rounded_full()
                            .bg(c(ui::INK))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .child(
                                Icon::new(IconName::ArrowUp)
                                    .size(px(16.))
                                    .text_color(c(0xFFFFFF)),
                            )
                            .on_click(cx.listener(|this, _, window, cx| {
                                let text = this.composer.read(cx).value().trim().to_string();
                                if !text.is_empty() {
                                    this.composer
                                        .update(cx, |i, cx| i.set_value("", window, cx));
                                    this.drop_job(text, cx);
                                }
                            })),
                    ),
            )
    }

    fn suggestions(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut row = div().mt(px(16.)).flex().gap(px(8.)).justify_center();
        for (i, (who, text, mode)) in data::SUGGESTIONS.into_iter().enumerate() {
            let face = self.avatar(who, 20.);
            row = row.child(
                div()
                    .id(("suggest", i))
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .pl(px(8.))
                    .pr(px(13.))
                    .h(px(34.))
                    .rounded_full()
                    .bg(ca(0xFFFFFF, 0.7))
                    .border_1()
                    .border_color(c(ui::LINE))
                    .text_size(px(13.))
                    .text_color(c(ui::BODY))
                    .cursor_pointer()
                    .hover(|s| s.bg(c(0xFFFFFF)).text_color(c(ui::INK)))
                    .child(face)
                    .child(text)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.mode = mode;
                        this.composer
                            .update(cx, |i, cx| i.set_value(text, window, cx));
                        cx.notify();
                    })),
            );
        }
        row
    }

    fn question_card(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let answer = self.guard_answer;
        let mood = match answer {
            Some(true) => Mood::Done,
            Some(false) => Mood::Thinking,
            None => Mood::NeedsYou,
        };
        let guard = self.bot(Member::Guard, mood, 150., Framing::Full);
        let (head, quote) = match answer {
            None => (
                "4 min ago",
                "\u{201C}Apple Pay works, but line 40 touches login. Use my safe fix?\u{201D}",
            ),
            Some(true) => (
                "just now",
                "Landed the safe fix. Login is untouched, and Checker re-ran Apple Pay: it passes.",
            ),
            Some(false) => (
                "just now",
                "Opened the change for you. I'll wait here until you decide.",
            ),
        };
        div()
            .flex_1()
            .flex_basis(px(0.))
            .min_w_0()
            .h(px(268.))
            .relative()
            .rounded(px(20.))
            .border_1()
            .border_color(c(0xF6DFA8))
            .bg(linear_gradient(
                135.,
                linear_color_stop(c(0xFFF6DE), 0.0),
                linear_color_stop(c(0xFFFBF2), 1.0),
            ))
            .shadow(ui::lift())
            .flex()
            .p(px(18.))
            .gap(px(8.))
            .child(div().mt(px(-4.)).child(guard))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .flex()
                            .gap(px(6.))
                            .items_baseline()
                            .text_size(px(13.))
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Guard · Apple Pay"),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(c(ui::MUTED))
                                    .child(head),
                            ),
                    )
                    .child(
                        div()
                            .mt(px(10.))
                            .text_size(px(18.))
                            .line_height(px(25.))
                            .font_weight(FontWeight::MEDIUM)
                            .child(quote),
                    )
                    .child(div().flex_1())
                    .child(match answer {
                        None => div()
                            .flex()
                            .gap(px(8.))
                            .child(ui::solid("use-fix", "Use the fix").on_click(cx.listener(
                                |this, _, _, cx| {
                                    this.guard_answer = Some(true);
                                    this.say("Guard landed the safe fix.");
                                    cx.notify();
                                },
                            )))
                            .child(ui::ghost("look-first", "Look first").on_click(cx.listener(
                                |this, _, _, cx| {
                                    this.guard_answer = Some(false);
                                    cx.notify();
                                },
                            ))),
                        Some(_) => {
                            div()
                                .flex()
                                .gap(px(8.))
                                .child(ui::ghost("undo-fix", "Undo").on_click(cx.listener(
                                    |this, _, _, cx| {
                                        this.guard_answer = None;
                                        cx.notify();
                                    },
                                )))
                        }
                    }),
            )
    }

    fn working_card(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let now = self.now();
        let jobs = self.jobs.clone();
        let mut list = div().flex().flex_col().gap(px(14.)).mt(px(14.));
        for (i, job) in jobs.iter().take(4).enumerate() {
            let face = self.bot(job.bot, Mood::Working, 30., Framing::Bust);
            let p = job.progress(now);
            let note: SharedString = match job.note {
                Some("starting") if p > 0.1 => "working".into(),
                Some(n) => n.into(),
                None => format!("{}%", (p * 100.) as u32).into(),
            };
            list = list.child(
                div()
                    .id(("job", i))
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .cursor_pointer()
                    .child(face)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(7.))
                            .child(
                                div()
                                    .flex()
                                    .justify_between()
                                    .gap(px(8.))
                                    .child(
                                        div()
                                            .text_size(px(13.5))
                                            .font_weight(FontWeight::MEDIUM)
                                            .truncate()
                                            .child(job.title.clone()),
                                    )
                                    .child(
                                        div()
                                            .flex_none()
                                            .text_size(px(11.5))
                                            .text_color(c(ui::MUTED))
                                            .child(note),
                                    ),
                            )
                            .child(ui::bar(p, job.color)),
                    )
                    .on_click(
                        cx.listener(|this, _, _, cx| this.go(Place::Live(Member::Builder), cx)),
                    ),
            );
        }
        let cost = 0.84 + 0.0004 * now;
        ui::card()
            .flex_1()
            .flex_basis(px(0.))
            .min_w_0()
            .h(px(268.))
            .p(px(20.))
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Working now"),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(c(ui::MUTED))
                            .child(format!("{} jobs · ${cost:.2}", self.jobs.len())),
                    ),
            )
            .child(list)
    }

    fn landed_card(&mut self) -> impl IntoElement {
        let mut list = div().flex().flex_col().gap(px(13.)).mt(px(14.));
        for (who, text, note) in data::LANDED {
            let face = self.bot(who, Mood::Done, 28., Framing::Bust);
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(face)
                    .child(div().flex_1().text_size(px(13.5)).child(text))
                    .children(
                        note.map(|n| div().text_size(px(11.5)).text_color(c(ui::MUTED)).child(n)),
                    ),
            );
        }
        ui::card()
            .flex_1()
            .flex_basis(px(0.))
            .min_w_0()
            .h(px(268.))
            .p(px(20.))
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Landed overnight"),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(c(ui::GREEN))
                            .child("all green"),
                    ),
            )
            .child(list)
            .child(div().flex_1())
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(c(ui::ORANGE))
                    .child("Watch the 2 minute recap →"),
            )
    }
}
