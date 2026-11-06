//! Crew: every bot, what it is for and what it has done, plus a playground for
//! making your own Fin.

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use runo_holo::fin::{Badge, Fin, Hat, Held, Material, Outfit, Tool, ROLES};
use runo_holo::{Framing, Mood, Tint};

use super::data::{Member, Mode};
use super::mascot;
use super::store::Status;
use super::ui::{self, c, ca};
use super::Pane;
use super::{Place, Shell};
use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;

/// A look picked by `seed`, for "Surprise me": any colour, a role kit or a
/// mix of props.
fn surprise(seed: u64) -> (Tint, Outfit) {
    let mut s = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03;
    let mut next = |n: usize| {
        s ^= s >> 31;
        s = s.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        s ^= s >> 29;
        (s % n as u64) as usize
    };
    let tint = mascot::COLOURS[next(mascot::COLOURS.len())].2;
    let outfit = if next(2) == 0 {
        ROLES[next(ROLES.len())].outfit
    } else {
        Outfit {
            hat: Hat::ALL[next(Hat::ALL.len())],
            left: Held::ALL[next(Held::ALL.len())],
            right: Tool::ALL[next(Tool::ALL.len())],
            badge: Badge::ALL[next(Badge::ALL.len())],
        }
    };
    (tint, outfit)
}

type Choice = (String, bool, Box<dyn Fn(&mut Shell)>);

impl Shell {
    pub(super) fn crew(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let all = Member::all();
        let n = all.len();
        let busy = self.crew.store.running().count();
        // The crew as a grid of cards, three across on a wide window.
        let wide = self.view_w - super::sidebar::WIDTH > 1100.;
        let mut list = div()
            .w_full()
            .grid()
            .grid_cols(if wide { 3 } else { 2 })
            .gap(px(14.));
        for who in all {
            list = list.child(self.crew_row(who, cx));
        }
        let page = div()
            .w_full()
            .max_w(px(1160.))
            .px(px(28.))
            .pt(px(24.))
            .pb(px(40. + super::DOCK_ROOM))
            .flex()
            .flex_col()
            .gap(px(22.))
            .child(
                div()
                    .flex()
                    .items_end()
                    .gap(px(12.))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .child(
                                div()
                                    .text_size(px(24.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Crew"),
                            )
                            .child(
                                div()
                                    .text_size(px(13.5))
                                    .text_color(c(ui::muted()))
                                    .child(format!(
                                        "{n} {} on your crew, {busy} working now. Each one runs on the engine you pick for it, and you can add as many as you like.",
                                        if n == 1 { "bot" } else { "bots" }
                                    )),
                            ),
                    )
                    .child(
                        ui::solid_i("crew-make", IconName::Plus, "Make a bot").on_click(cx.listener(|this, _, _, cx| {
                            this.maker_open = !this.maker_open;
                            cx.notify();
                        })),
                    ),
            )
            .when(self.maker_open, |d| {
                d.child(ui::rise(div().child(self.maker_card(cx)), self.stagger(1)))
            })
            .child(list)
            .children(self.bench(cx));
        div()
            .id("crew")
            .size_full()
            .overflow_y_scroll()
            .child(page)
            .into_any_element()
    }

    fn crew_row(&mut self, who: Member, cx: &mut Context<Self>) -> Stateful<Div> {
        let mood = self.mood(who);
        let face = self.bot(who, mood, 52., Framing::Bust);
        let (status, color) = self.status_of(who);
        let jobs: Vec<_> = self
            .crew
            .store
            .jobs
            .iter()
            .filter(|j| j.bot == who)
            .collect();
        let done = jobs
            .iter()
            .filter(|j| matches!(j.status, Status::Done | Status::Shipped))
            .count();
        let ended = jobs
            .iter()
            .filter(|j| {
                !matches!(
                    j.status,
                    Status::Running | Status::Review | Status::Shipping
                )
            })
            .count();
        let spend: f64 = jobs
            .iter()
            .filter_map(|j| super::home::cost(&j.summary))
            .sum::<f64>();
        let engine = super::engine::label(cx.global::<crate::settings::AppSettings>(), who);
        let mode = who.mode();
        let pinned = self.crew.store.pinned.contains(&who);
        let small = |id: (&'static str, usize), label: &'static str| {
            div()
                .id(id)
                .px(px(10.))
                .flex_none()
                .h(px(28.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(7.))
                .text_size(px(12.))
                .text_color(c(ui::muted()))
                .cursor_pointer()
                .hover(|s| s.bg(c(ui::hover())).text_color(c(ui::ink())))
                .child(label)
        };
        let key = who.key() as usize;
        // One figure under the bot: a small label over a mono value.
        let figure = |label: &'static str, v: String| {
            div()
                .flex_1()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(c(ui::faint()))
                        .child(label),
                )
                .child(
                    div()
                        .font_family(ui::MONO)
                        .text_size(px(14.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(c(if v == "\u{2014}" {
                            ui::faint()
                        } else {
                            ui::ink()
                        }))
                        .child(v),
                )
        };
        div()
            .id(("crew-row", key))
            .on_hover(cx.listener(move |this, on: &bool, _, cx| {
                this.hover_bot(who, *on, cx);
            }))
            .relative()
            .overflow_hidden()
            .flex()
            .flex_col()
            .gap(px(14.))
            .p(px(16.))
            .bg(c(ui::panel()))
            .border_1()
            .border_color(c(ui::line()))
            .rounded(px(ui::CARD_R + 2.))
            .hover(|s| s.border_color(c(ui::line_hi())))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
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
                                    .gap(px(6.))
                                    .text_size(px(15.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(c(ui::ink()))
                                    .child(who.name())
                                    .when(pinned, |d| {
                                        d.child(
                                            Icon::new(IconName::Pin)
                                                .size(px(12.))
                                                .text_color(c(ui::faint())),
                                        )
                                    }),
                            )
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .text_color(c(ui::muted()))
                                    .truncate()
                                    .child(who.role()),
                            )
                            .child(div().mt(px(4.)).child(if color == ui::faint() {
                                ui::status(ui::faint(), "Idle")
                            } else {
                                ui::status(color, status)
                            })),
                    ),
            )
            .child(
                div()
                    .id(("crew-engine", key))
                    .h(px(28.))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .rounded(px(7.))
                    .bg(c(ui::well()))
                    .border_1()
                    .border_color(c(ui::line2()))
                    .text_size(px(12.))
                    .text_color(c(ui::body()))
                    .cursor_pointer()
                    .hover(|s| s.text_color(c(ui::ink())).border_color(c(ui::line_hi())))
                    .child(
                        Icon::new(IconName::Cpu)
                            .size(px(13.))
                            .text_color(c(ui::faint())),
                    )
                    .child(div().flex_1().min_w_0().truncate().child(engine))
                    .child(
                        Icon::new(IconName::ChevronRight)
                            .size(px(12.))
                            .text_color(c(ui::faint())),
                    )
                    .on_click(
                        cx.listener(|this, _, _, cx| this.go(Place::Settings(Pane::Engines), cx)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .px(px(4.))
                    .child(figure(
                        "Jobs",
                        if jobs.is_empty() {
                            "\u{2014}".into()
                        } else {
                            jobs.len().to_string()
                        },
                    ))
                    .child(figure(
                        "Success",
                        match (done * 100).checked_div(ended) {
                            Some(pct) => format!("{pct}%"),
                            None => "\u{2014}".into(),
                        },
                    ))
                    .child(figure(
                        "Spend",
                        if spend == 0. {
                            "\u{2014}".into()
                        } else {
                            format!("${spend:.2}")
                        },
                    )),
            )
            .child(
                div()
                    .pt(px(12.))
                    .border_t_1()
                    .border_color(c(ui::line2()))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        small(("crew-pin", key), if pinned { "Unpin" } else { "Pin" }).on_click(
                            cx.listener(move |this, _, _, cx| {
                                if let Some(i) =
                                    this.crew.store.pinned.iter().position(|m| *m == who)
                                {
                                    this.crew.store.pinned.remove(i);
                                } else {
                                    this.crew.store.pinned.push(who);
                                }
                                this.crew.store.save();
                                cx.notify();
                            }),
                        ),
                    )
                    // Fin leads every crew, so it stays.
                    .when(who != Member::Fin, |d| {
                        d.child(small(("crew-remove", key), "Remove").on_click(cx.listener(
                            move |this, _, _, cx| {
                                if who.is_made() {
                                    this.remove_made(who, cx)
                                } else {
                                    this.set_on_crew(who, false, cx)
                                }
                            },
                        )))
                    })
                    .child(div().flex_1())
                    .child(
                        ui::ghost_i(("crew-chat", key), IconName::MessageCircle, "Chat")
                            .h(px(30.))
                            .text_size(px(12.5))
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.go(Place::Chat(who), cx)),
                            ),
                    )
                    .when(mode != Mode::Ask, |d| {
                        d.child(
                            ui::solid_i(("crew-job", key), IconName::Plus, "Give a job")
                                .h(px(30.))
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
                    .bg(ca(ui::panel(), 0.7))
                    .border_1()
                    .border_color(c(ui::line()))
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
                                    .text_color(c(ui::muted()))
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
        let (tint, outfit) = self.maker;
        let material = Material::from_id(&cx.global::<crate::settings::AppSettings>().fin_material);
        let preview = self.pose_img(
            runo_holo::Figure::Fin(
                Fin {
                    tint,
                    material,
                    outfit,
                }
                .pose(Mood::Signature),
            ),
            220.,
            Framing::Full,
        );
        let row = |label: &'static str,
                   items: Vec<Choice>,
                   key: &'static str,
                   cx: &mut Context<Shell>| {
            let mut opts = div().flex_1().min_w_0().flex().flex_wrap().gap(px(8.));
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
                        .text_color(c(ui::faint()))
                        .child(label.to_uppercase()),
                )
                .child(opts)
        };
        let mut swatches = div().flex().gap(px(10.));
        for (i, (_, _, t)) in mascot::COLOURS.into_iter().enumerate() {
            let on = tint == t;
            let [l, s, _] = t.0;
            swatches = swatches.child(
                div()
                    .id(("swatch", i))
                    .size(px(28.))
                    .rounded_full()
                    .border_2()
                    .border_color(c(if on { ui::ink() } else { ui::panel() }))
                    .bg(linear_gradient(
                        135.,
                        linear_color_stop(c(l.0), 0.0),
                        linear_color_stop(c(s.0), 1.0),
                    ))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.maker.0 = t;
                        cx.notify();
                    })),
            );
        }
        let pick = |name: &str, on: bool, f: Box<dyn Fn(&mut Shell)>| -> Choice {
            (name.to_string(), on, f)
        };
        let mut kits = vec![pick(
            "Plain",
            outfit == Outfit::PLAIN,
            Box::new(|s: &mut Shell| s.maker.1 = Outfit::PLAIN),
        )];
        kits.extend(ROLES.iter().map(|r| {
            let o = r.outfit;
            pick(
                r.name,
                outfit == o,
                Box::new(move |s: &mut Shell| s.maker.1 = o),
            )
        }));

        let rights = Tool::ALL
            .iter()
            .copied()
            .map(|t| {
                pick(
                    t.name(),
                    outfit.right == t,
                    Box::new(move |s: &mut Shell| s.maker.1.right = t),
                )
            })
            .collect();

        let likes = Member::KINDS
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
        let kits = row("Kit", kits, "mk-kit", cx);
        let rights = row("Prop", rights, "mk-right", cx);
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
                    .bg(ca(ui::panel(), 0.6))
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
                                    .min_w_0()
                                    .text_size(px(12.5))
                                    .text_color(c(ui::muted()))
                                    .child("Dress a Peek, give it a name and a job, and it joins the crew with its own chat, jobs and engine."),
                            )
                            .child(ui::ghost_i("surprise", IconName::Sparkles, "Surprise me").on_click(cx.listener(|this, _, _, cx| {
                                this.maker = surprise(this.clock.elapsed().as_nanos() as u64);
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
                                    .text_color(c(ui::faint()))
                                    .child("COLOUR"),
                            )
                            .child(swatches),
                    )
                    .child(kits)
                    .child(rights)
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
        .bg(c(ui::panel()))
        .border_1()
        .border_color(c(ui::line()))
        .child(
            div()
                .flex_none()
                .font_family(ui::MONO)
                .text_size(px(9.5))
                .text_color(c(ui::faint()))
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
