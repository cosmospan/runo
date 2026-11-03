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
        for (i, who) in Member::all().into_iter().enumerate() {
            let k = self.stagger(i + 1);
            grid = grid.child(ui::rise(self.crew_card(who, cx), k));
        }
        div()
            .id("crew")
            .size_full()
            .overflow_y_scroll()
            .px(px(28.))
            .pt(px(24.))
            .pb(px(40. + super::DOCK_ROOM))
            .flex()
            .flex_col()
            .gap(px(22.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(div().text_size(px(24.)).font_weight(FontWeight::SEMIBOLD).child("Crew"))
                    .child(
                        div()
                            .text_size(px(14.))
                            .text_color(c(ui::MUTED))
                            .child("Six bots, each good at one kind of work. Each runs on the engine you pick for it."),
                    ),
            )
            .child(grid)
            .children(self.bench(cx))
            .child(ui::rise(div().child(self.maker_card(cx)), self.stagger(8)))
            .into_any_element()
    }

    fn crew_card(&mut self, who: Member, cx: &mut Context<Self>) -> Stateful<Div> {
        let mood = self.mood(who);
        let face = self.bot(who, mood, 64., Framing::Bust);
        let (status, color) = self.status_of(who);
        let jobs: Vec<_> = self.store.jobs.iter().filter(|j| j.bot == who).collect();
        let done = jobs.iter().filter(|j| j.status == Status::Done).count();
        let ended = jobs.iter().filter(|j| j.status != Status::Running).count();
        let spend: f64 = jobs
            .iter()
            .filter_map(|j| super::home::cost(&j.summary))
            .sum::<f64>()
            + 0.0;
        let engine = super::engine::label(cx.global::<crate::settings::AppSettings>(), who);
        let mode = who.mode();
        let stat = |k: &'static str, v: String| {
            div()
                .flex_1()
                .min_w_0()
                .child(div().text_size(px(11.)).text_color(c(ui::FAINT)).child(k))
                .child(
                    div()
                        .mt(px(2.))
                        .text_size(px(16.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(v),
                )
        };
        div()
            .id(("crew-card", who.key() as usize))
            .on_hover(cx.listener(move |this, on: &bool, _, cx| {
                this.hover_bot(who, *on, cx);
            }))
            .flex_grow(1.)
            .flex_basis(px(300.))
            .max_w(px(440.))
            .rounded(px(14.))
            .border_1()
            .border_color(c(ui::LINE))
            .bg(c(ui::PANEL))
            .overflow_hidden()
            .flex()
            .flex_col()
            .child(
                div()
                    .px(px(18.))
                    .pt(px(16.))
                    .pb(px(14.))
                    .flex()
                    .items_center()
                    .gap(px(14.))
                    .bg(linear_gradient(
                        180.,
                        linear_color_stop(ca(who.accent(), 0.12), 0.0),
                        linear_color_stop(ca(ui::PANEL, 0.0), 1.0),
                    ))
                    .child(div().flex_none().child(face))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(16.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(who.name()),
                            )
                            .child(
                                div()
                                    .mt(px(2.))
                                    .text_size(px(12.5))
                                    .text_color(c(ui::MUTED))
                                    .child(who.role()),
                            ),
                    )
                    .when(color != ui::FAINT, |d| {
                        d.child(div().flex_none().child(ui::status(color, status)))
                    }),
            )
            .child(
                div()
                    .px(px(18.))
                    .text_size(px(12.5))
                    .line_height(px(18.))
                    .text_color(c(ui::BODY))
                    .child(who.pitch()),
            )
            .child(
                div()
                    .mx(px(18.))
                    .mt(px(14.))
                    .py(px(12.))
                    .flex()
                    .border_t_1()
                    .border_b_1()
                    .border_color(c(ui::LINE2))
                    .child(stat("Jobs", jobs.len().to_string()))
                    .child(stat(
                        "Success",
                        if ended == 0 {
                            "—".into()
                        } else {
                            format!("{}%", done * 100 / ended)
                        },
                    ))
                    .child(stat(
                        "Spend",
                        if spend == 0. {
                            "—".into()
                        } else {
                            format!("${spend:.2}")
                        },
                    )),
            )
            .child(
                div()
                    .px(px(18.))
                    .py(px(12.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(11.5))
                            .text_color(c(ui::FAINT))
                            .child(engine),
                    )
                    .child(
                        div()
                            .id(("crew-remove", who.key() as usize))
                            .px(px(6.))
                            .text_size(px(12.))
                            .text_color(c(ui::FAINT))
                            .cursor_pointer()
                            .hover(|s| s.text_color(c(ui::RED)))
                            .child("Remove")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if who.is_made() {
                                    this.remove_made(who, cx)
                                } else {
                                    this.set_on_crew(who, false, cx)
                                }
                            })),
                    )
                    .child(
                        ui::ghost(("crew-chat", who.key() as usize), "Chat")
                            .h(px(28.))
                            .text_size(px(12.5))
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.go(Place::Chat(who), cx)),
                            ),
                    )
                    .when(mode != Mode::Ask, |d| {
                        d.child(
                            ui::solid(("crew-job", who.key() as usize), "Give a job")
                                .h(px(28.))
                                .text_size(px(12.5))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.target = Some(who);
                                    this.set_mode(mode, window, cx);
                                    this.go(Place::Today, cx);
                                    this.say(format!(
                                        "Type the job for {} and press Enter.",
                                        who.name()
                                    ));
                                })),
                        )
                    }),
            )
    }

    /// Built-in bots not on the crew, each one click from joining.
    fn bench(&mut self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let off: Vec<Member> = Member::CREW.into_iter().filter(|m| !m.on_crew()).collect();
        if off.is_empty() {
            return None;
        }
        let mut row = div().flex().flex_wrap().gap(px(12.));
        for who in off {
            let face = self.bot(who, Mood::Asleep, 56., Framing::Bust);
            row = row.child(
                div()
                    .id(("bench", who.key() as usize))
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .pl(px(8.))
                    .pr(px(14.))
                    .py(px(8.))
                    .w(px(330.))
                    .rounded(px(18.))
                    .bg(ca(ui::PANEL, 0.7))
                    .border_1()
                    .border_color(c(ui::LINE))
                    .child(face)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_weight(FontWeight::BOLD)
                                    .child(who.name()),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(c(ui::MUTED))
                                    .child(who.role()),
                            ),
                    )
                    .child(
                        ui::ghost(("bench-add", who.key() as usize), "Add").on_click(
                            cx.listener(move |this, _, _, cx| this.set_on_crew(who, true, cx)),
                        ),
                    ),
            );
        }
        Some(
            div()
                .flex()
                .flex_col()
                .gap(px(10.))
                .child(
                    div()
                        .text_size(px(15.))
                        .font_weight(FontWeight::BOLD)
                        .child("Add a bot"),
                )
                .child(row),
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
                    .border_color(c(if on { ui::INK } else { ui::PANEL }))
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
        let likes = Member::CREW
            .into_iter()
            .map(|m| {
                (
                    m.name().to_string(),
                    self.maker_like == m,
                    Box::new(move |s: &mut Shell| s.maker_like = m) as Box<dyn Fn(&mut Shell)>,
                )
            })
            .collect();
        let like = row("Works like", likes, "mk-like", cx);
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
                    .bg(ca(ui::PANEL, 0.6))
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
                                    .child("Pick a look, give it a name and a job, and it joins the crew with its own chat, jobs and engine."),
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
                    .child(sways)
                    .child(like)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .child(field("NAME", &self.maker_name, 180.))
                            .child(field("ITS JOB", &self.maker_role, 0.))
                            .child(
                                ui::solid("maker-save", "Add to crew")
                                    .on_click(cx.listener(|this, _, window, cx| this.save_made(window, cx))),
                            ),
                    ),
            )
    }
}

/// A labelled text box in the maker.
fn field(
    label: &'static str,
    input: &Entity<gpui_kit::component::input::InputState>,
    w: f32,
) -> Div {
    div()
        .when(w > 0., |d| d.w(px(w)).flex_none())
        .when(w == 0., |d| d.flex_1().min_w_0())
        .h(px(38.))
        .px(px(12.))
        .flex()
        .items_center()
        .gap(px(8.))
        .rounded(px(12.))
        .bg(c(ui::PANEL))
        .border_1()
        .border_color(c(ui::LINE))
        .child(
            div()
                .flex_none()
                .font_family(ui::MONO)
                .text_size(px(9.5))
                .text_color(c(ui::FAINT))
                .child(label),
        )
        .child(
            div().flex_1().min_w_0().text_size(px(13.)).child(
                gpui_kit::component::input::Input::new(input)
                    .appearance(false)
                    .bordered(false),
            ),
        )
}
