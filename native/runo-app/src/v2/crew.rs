//! Crew: every bot, what it is for and what it has done, plus a playground for
//! making your own bubble.

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use runo_holo::{Bubble, Eyes, Framing, Layout, Mood, Sway, Tint};

use super::data::{Member, Mode};
use super::store::Status;
use super::ui::{self, c, ca};
use super::{Place, Shell};

fn layout_name(l: Layout) -> &'static str {
    match l {
        Layout::Solo => "Solo",
        Layout::Pair => "Pair",
        Layout::Trio => "Trio",
        Layout::Orbit => "Orbit",
        Layout::Rising => "Rising",
        Layout::Shield => "Shield",
        _ => "",
    }
}

fn eyes_name(e: Eyes) -> &'static str {
    match e {
        Eyes::Dots => "Dots",
        Eyes::Smile => "Smiles",
        Eyes::Lens => "Lens",
        Eyes::Visor => "Visor",
        Eyes::Sleepy => "Sleepy",
        Eyes::Squint => "Squint",
        _ => "",
    }
}

fn sway_name(s: Sway) -> &'static str {
    match s {
        Sway::Float => "Float",
        Sway::Bounce => "Bounce",
        Sway::Hop => "Hop",
        Sway::Wobble => "Wobble",
        Sway::Drift => "Drift",
        _ => "",
    }
}

/// A maker option: its name, whether it's picked, and what picking it does.
type Choice = (String, bool, Box<dyn Fn(&mut Shell)>);

impl Shell {
    pub(super) fn crew(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let mut grid = div().flex().flex_wrap().gap(px(16.));
        for who in Member::CREW {
            grid = grid.child(self.crew_card(who, cx));
        }
        div()
            .id("crew")
            .size_full()
            .overflow_y_scroll()
            .px(px(40.))
            .pt(px(if cfg!(target_os = "macos") { 40. } else { 30. }))
            .pb(px(40.))
            .flex()
            .flex_col()
            .gap(px(22.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(div().text_size(px(34.)).font_weight(FontWeight::BOLD).child("Your crew"))
                    .child(
                        div()
                            .text_size(px(14.))
                            .text_color(c(ui::MUTED))
                            .child("Six bots, each good at one kind of work. Each runs on the engine you pick for it."),
                    ),
            )
            .child(grid)
            .child(self.maker_card(cx))
            .into_any_element()
    }

    fn crew_card(&mut self, who: Member, cx: &mut Context<Self>) -> impl IntoElement {
        let mood = self.mood(who);
        let face = self.bot(who, mood, 150., Framing::Full);
        let (status, color) = self.status_of(who);
        let jobs: Vec<_> = self.store.jobs.iter().filter(|j| j.bot == who).collect();
        let done = jobs.iter().filter(|j| j.status == Status::Done).count();
        let last = jobs.last().map(|j| (j.id, j.title.clone()));
        let asked = self.store.chat(who).iter().filter(|m| m.mine).count();
        let engine = super::engine::label(cx.global::<crate::settings::AppSettings>(), who);
        let stats = match (done, asked) {
            (0, 0) => "No jobs yet".to_string(),
            (d, 0) => format!("{d} {} done", if d == 1 { "job" } else { "jobs" }),
            (0, a) => format!(
                "{a} {} asked",
                if a == 1 { "question" } else { "questions" }
            ),
            (d, a) => format!("{d} done · {a} asked"),
        };
        let mode = Mode::ALL
            .into_iter()
            .find(|m| Member::for_mode(*m) == who)
            .unwrap_or(Mode::Ask);
        div()
            .id(("crew-card", who.key() as usize))
            .on_hover(cx.listener(move |this, on: &bool, _, cx| {
                this.hover_bot(who, *on, cx);
            }))
            .flex_grow(1.)
            .flex_basis(px(340.))
            .max_w(px(520.))
            .rounded(px(24.))
            .border_1()
            .border_color(c(0xFFFFFF))
            .shadow(ui::lift())
            .bg(linear_gradient(
                180.,
                linear_color_stop(ca(who.accent(), 0.10), 0.0),
                linear_color_stop(ca(0xFFFFFF, 0.75), 0.6),
            ))
            .p(px(18.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(
                div()
                    .flex()
                    .gap(px(10.))
                    .child(div().flex_none().mt(px(-8.)).ml(px(-8.)).child(face))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .pt(px(10.))
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(
                                div()
                                    .text_size(px(20.))
                                    .font_weight(FontWeight::BOLD)
                                    .child(who.name()),
                            )
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .text_color(c(ui::MUTED))
                                    .child(who.role()),
                            )
                            .when(color != ui::FAINT, |d| {
                                d.child(div().mt(px(6.)).child(ui::status(color, status)))
                            })
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .text_color(c(ui::FAINT))
                                    .child(engine),
                            ),
                    ),
            )
            .child(
                div()
                    .text_size(px(13.))
                    .line_height(px(19.))
                    .text_color(c(ui::BODY))
                    .child(who.pitch()),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.))
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(c(ui::MUTED))
                            .truncate()
                            .child(stats),
                    )
                    .when_some(last, |d, (id, title)| {
                        d.child(
                            div()
                                .id(("crew-last", who.key() as usize))
                                .max_w(px(180.))
                                .truncate()
                                .text_size(px(12.))
                                .text_color(c(ui::ORANGE))
                                .cursor_pointer()
                                .hover(|s| s.text_color(c(ui::INK)))
                                .child(format!("Last: {title}"))
                                .on_click(
                                    cx.listener(move |this, _, _, cx| this.go(Place::Live(id), cx)),
                                ),
                        )
                    }),
            )
            .child(
                div()
                    .mt(px(4.))
                    .flex()
                    .gap(px(8.))
                    .child(
                        ui::ghost(("crew-chat", who.key() as usize), "Ask").on_click(
                            cx.listener(move |this, _, _, cx| this.go(Place::Chat(who), cx)),
                        ),
                    )
                    .when(mode != Mode::Ask, |d| {
                        d.child(
                            ui::solid(
                                ("crew-job", who.key() as usize),
                                format!("Give {} a job", who.name()),
                            )
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.set_mode(mode, window, cx);
                                    this.go(Place::Today, cx);
                                    this.say(format!(
                                        "Type the job for {} and press Enter.",
                                        who.name()
                                    ));
                                },
                            )),
                        )
                    }),
            )
    }

    fn maker_card(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let b = self.maker;
        let preview = self.pose_img(b.pose(Mood::Signature), 220., Framing::Full);
        let row = |label: &'static str,
                   items: Vec<Choice>,
                   key: &'static str,
                   cx: &mut Context<Shell>| {
            let mut opts = div().flex().flex_wrap().gap(px(8.));
            for (i, (name, on, f)) in items.into_iter().enumerate() {
                let f = std::rc::Rc::new(f);
                opts = opts.child(ui::pill((key, i), name, on).on_click(cx.listener(
                    move |this, _, _, cx| {
                        f(this);
                        cx.notify();
                    },
                )));
            }
            div()
                .flex()
                .items_center()
                .gap(px(14.))
                .child(
                    div()
                        .w(px(64.))
                        .flex_none()
                        .font_family(ui::MONO)
                        .text_size(px(10.))
                        .text_color(c(ui::FAINT))
                        .child(label.to_uppercase()),
                )
                .child(opts)
        };
        let mut swatches = div().flex().gap(px(10.));
        for (i, t) in Tint::ALL.into_iter().enumerate() {
            let on = b.tint == t;
            let [l, s, _] = t.0;
            swatches = swatches.child(
                div()
                    .id(("swatch", i))
                    .size(px(28.))
                    .rounded_full()
                    .border_2()
                    .border_color(c(if on { ui::INK } else { 0xFFFFFF }))
                    .shadow(ui::lift())
                    .bg(linear_gradient(
                        135.,
                        linear_color_stop(c(l.0), 0.0),
                        linear_color_stop(c(s.0), 1.0),
                    ))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.maker.tint = t;
                        cx.notify();
                    })),
            );
        }
        let layouts = Bubble::LAYOUTS
            .into_iter()
            .map(|l| {
                (
                    layout_name(l).to_string(),
                    b.layout == l,
                    Box::new(move |s: &mut Shell| s.maker.layout = l) as Box<dyn Fn(&mut Shell)>,
                )
            })
            .collect();
        let eyes = Bubble::EYES
            .into_iter()
            .map(|e| {
                (
                    eyes_name(e).to_string(),
                    b.eyes == e,
                    Box::new(move |s: &mut Shell| s.maker.eyes = e) as Box<dyn Fn(&mut Shell)>,
                )
            })
            .collect();
        let sways = Bubble::SWAYS
            .into_iter()
            .map(|w| {
                (
                    sway_name(w).to_string(),
                    b.sway == w,
                    Box::new(move |s: &mut Shell| s.maker.sway = w) as Box<dyn Fn(&mut Shell)>,
                )
            })
            .collect();
        let layouts = row("Bubbles", layouts, "mk-lay", cx);
        let eyes = row("Eyes", eyes, "mk-eye", cx);
        let sways = row("Motion", sways, "mk-sway", cx);
        ui::card()
            .p(px(22.))
            .flex()
            .gap(px(28.))
            .items_center()
            .child(
                div()
                    .w(px(280.))
                    .h(px(280.))
                    .flex_none()
                    .rounded(px(20.))
                    .bg(ca(0xFFFFFF, 0.6))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(preview),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(14.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(14.))
                            .child(div().text_size(px(18.)).font_weight(FontWeight::BOLD).child("Make your own"))
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(px(12.5))
                                    .text_color(c(ui::MUTED))
                                    .child("Four choices, about a thousand combinations, all of them on brand. Saving your own bot comes next."),
                            )
                            .child(ui::ghost("surprise", "Surprise me").on_click(cx.listener(|this, _, _, cx| {
                                this.maker = Bubble::from_seed(this.clock.elapsed().as_nanos() as u64);
                                cx.notify();
                            }))),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(14.))
                            .child(
                                div()
                                    .w(px(64.))
                                    .flex_none()
                                    .font_family(ui::MONO)
                                    .text_size(px(10.))
                                    .text_color(c(ui::FAINT))
                                    .child("COLOUR"),
                            )
                            .child(swatches),
                    )
                    .child(layouts)
                    .child(eyes)
                    .child(sways),
            )
    }
}
