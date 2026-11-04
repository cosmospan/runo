//! Settings: a list of panes on the left and the open pane on the right, in the
//! same dark cards, pills and hairlines as the rest of v2.
//!
//! - **General**: how the app behaves while the crew works, and the limits.
//! - **Mascot**: the crew's look. Fin or the bubble crew, Fin's material, and
//!   each bot's colour and props, with a live preview.
//! - **Engines**: which coding agent and model each bot runs on.
//!
//! Every choice is saved the moment it's made. Anything not here is still in
//! the classic app's settings.

use gpui_kit::assets::IconName;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::Icon;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use runo_holo::fin::{Badge, FinPose, Hat, Held, Material, Outfit, State, Tool, ROLES};
use runo_holo::{Figure, Framing, Mood, Tint};

use crate::settings::AppSettings;

use super::data::Member;
use super::engines::{choice, section};
use super::mascot;
use super::ui::{self, c, ca, space};
use super::{Place, Shell};

/// A pane of Settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Pane {
    General,
    Mascot,
    Engines,
}

impl Pane {
    pub const ALL: [Pane; 3] = [Pane::General, Pane::Mascot, Pane::Engines];

    pub fn name(self) -> &'static str {
        match self {
            Pane::General => "General",
            Pane::Mascot => "Mascot",
            Pane::Engines => "Engines",
        }
    }

    fn line(self) -> &'static str {
        match self {
            Pane::General => "Alerts, motion and limits",
            Pane::Mascot => "Fin, colours and props",
            Pane::Engines => "Agents and models",
        }
    }

    fn icon(self) -> IconName {
        match self {
            Pane::General => IconName::Settings,
            Pane::Mascot => IconName::Bot,
            Pane::Engines => IconName::Cpu,
        }
    }
}

fn save(cx: &mut App, f: impl FnOnce(&mut AppSettings)) {
    cx.update_global::<AppSettings, _>(|st, _| {
        f(st);
        st.save();
    });
}

/// A small on/off switch.
fn switch(on: bool) -> Div {
    div()
        .w(px(34.))
        .h(px(20.))
        .flex_none()
        .rounded_full()
        .bg(c(if on { ui::VIOLET } else { 0x2A2A33 }))
        .border_1()
        .border_color(c(if on { 0x8E66FF } else { ui::LINE_HI }))
        .relative()
        .child(
            div()
                .absolute()
                .top(px(2.))
                .left(px(if on { 16. } else { 2. }))
                .size(px(14.))
                .rounded_full()
                .bg(c(0xFFFFFF))
                .shadow(ui::lift()),
        )
}

/// One setting: what it is and what it does on the left, its control on the right.
fn row(title: &'static str, line: impl Into<SharedString>, control: impl IntoElement) -> Div {
    div()
        .px(px(18.))
        .py(px(14.))
        .flex()
        .items_center()
        .gap(px(space::M))
        .border_t_1()
        .border_color(c(ui::LINE2))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(3.))
                .child(
                    div()
                        .text_size(px(13.5))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(c(ui::INK))
                        .child(title),
                )
                .child(
                    div()
                        .text_size(px(12.5))
                        .text_color(c(ui::MUTED))
                        .child(line.into()),
                ),
        )
        .child(control)
}

/// A card of settings rows under a small heading.
fn group(title: &'static str, rows: Vec<Div>) -> Div {
    let mut card = ui::card().overflow_hidden().flex().flex_col();
    card = card.child(
        div()
            .px(px(18.))
            .pt(px(14.))
            .pb(px(10.))
            .text_size(px(11.5))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(c(ui::MUTED))
            .child(title),
    );
    for r in rows {
        card = card.child(r);
    }
    card
}

/// A row of options with one picked, on a sunken track.
fn segmented(items: Vec<Stateful<Div>>) -> Div {
    let mut d = div()
        .flex()
        .flex_none()
        .gap(px(3.))
        .p(px(3.))
        .rounded(px(9.))
        .bg(c(ui::BG))
        .border_1()
        .border_color(c(ui::LINE));
    for i in items {
        d = d.child(i);
    }
    d
}

fn segment(id: impl Into<ElementId>, label: impl Into<SharedString>, on: bool) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(26.))
        .px(px(12.))
        .flex()
        .items_center()
        .rounded(px(6.))
        .text_size(px(12.5))
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .map(|d| {
            if on {
                d.bg(c(ui::RAISED))
                    .border_1()
                    .border_color(c(ui::LINE_HI))
                    .text_color(c(ui::INK))
                    .shadow(ui::lift())
            } else {
                d.text_color(c(ui::MUTED))
                    .hover(|s| s.text_color(c(ui::INK)))
            }
        })
        .child(label.into())
}

/// − value + for a number setting.
fn stepper(value: impl Into<SharedString>, down: Stateful<Div>, up: Stateful<Div>) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .rounded(px(8.))
        .border_1()
        .border_color(c(ui::LINE))
        .bg(c(ui::RAISED))
        .child(down)
        .child(
            div()
                .min_w(px(78.))
                .px(px(6.))
                .flex()
                .justify_center()
                .text_size(px(12.5))
                .font_weight(FontWeight::MEDIUM)
                .text_color(c(ui::INK))
                .child(value.into()),
        )
        .child(up)
}

fn step_button(id: impl Into<ElementId>, icon: IconName) -> Stateful<Div> {
    div()
        .id(id)
        .size(px(28.))
        .flex()
        .items_center()
        .justify_center()
        .text_color(c(ui::BODY))
        .cursor_pointer()
        .hover(|s| s.bg(c(ui::HOVER)).text_color(c(ui::INK)))
        .active(|s| s.bg(c(ui::PRESS)))
        .child(Icon::new(icon).size(px(13.)))
}

/// The label in front of a row of choices in the Mascot customizer.
fn slot(label: &'static str, items: Div) -> Div {
    div()
        .flex()
        .items_start()
        .gap(px(space::SM))
        .child(
            div()
                .w(px(74.))
                .flex_none()
                .pt(px(6.))
                .font_family(ui::MONO)
                .text_size(px(10.5))
                .text_color(c(ui::FAINT))
                .child(label),
        )
        .child(items.flex_1().flex().flex_wrap().gap(px(6.)))
}

impl Shell {
    pub(super) fn settings(&mut self, pane: Pane, cx: &mut Context<Self>) -> AnyElement {
        let mut nav = div()
            .id("settings-nav")
            .w(px(232.))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(2.))
            .px(px(12.))
            .pt(px(24.))
            .pb(px(16.))
            .border_r_1()
            .border_color(c(ui::LINE))
            .bg(c(ui::BG))
            .child(
                div()
                    .px(px(10.))
                    .pb(px(14.))
                    .text_size(px(20.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Settings"),
            );
        for p in Pane::ALL {
            let on = p == pane;
            nav = nav.child(
                div()
                    .id(("settings-pane", p as usize))
                    .px(px(10.))
                    .py(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .rounded(px(9.))
                    .cursor_pointer()
                    .map(|d| {
                        if on {
                            d.bg(c(ui::PANEL)).border_1().border_color(c(ui::LINE))
                        } else {
                            d.hover(|s| s.bg(c(ui::HOVER)))
                        }
                    })
                    .child(
                        div()
                            .size(px(28.))
                            .flex_none()
                            .rounded(px(8.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(if on {
                                ca(ui::RUNO, 0.22)
                            } else {
                                c(ui::RAISED)
                            })
                            .text_color(c(if on { ui::T_VIOLET } else { ui::MUTED }))
                            .child(Icon::new(p.icon()).size(px(15.))),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(c(if on { ui::INK } else { ui::BODY }))
                                    .child(p.name()),
                            )
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .text_color(c(ui::FAINT))
                                    .truncate()
                                    .child(p.line()),
                            ),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        // Switching panes stays inside Settings: no page fade, no
                        // extra step on the back trail.
                        this.settings_pane = p;
                        this.place = Place::Settings(p);
                        cx.notify();
                    })),
            );
        }
        nav = nav.child(
            div()
                .mt_auto()
                .px(px(10.))
                .flex()
                .flex_col()
                .gap(px(8.))
                .child(
                    div()
                        .text_size(px(11.5))
                        .text_color(c(ui::FAINT))
                        .child("Themes, terminal, keys and connections are in the classic app."),
                )
                .child(
                    ui::ghost("settings-classic", "Open the classic app")
                        .on_click(cx.listener(|_, _, _, cx| cx.defer(crate::open_main_window))),
                ),
        );

        let body = match pane {
            Pane::General => self.settings_general(cx),
            Pane::Mascot => self.settings_mascot(cx),
            Pane::Engines => self.engines(cx),
        };
        div()
            .size_full()
            .flex()
            .child(nav)
            .child(div().flex_1().min_w_0().h_full().child(body))
            .into_any_element()
    }

    fn page(id: &'static str, title: &'static str, line: &'static str, body: Div) -> AnyElement {
        div()
            .id(id)
            .size_full()
            .overflow_y_scroll()
            .child(
                div()
                    .w_full()
                    .max_w(px(980.))
                    .px(px(28.))
                    .pt(px(24.))
                    .pb(px(space::XL + super::DOCK_ROOM))
                    .flex()
                    .flex_col()
                    .gap(px(space::L))
                    .child(
                        div()
                            .child(
                                div()
                                    .text_size(px(22.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(title),
                            )
                            .child(
                                div()
                                    .mt(px(6.))
                                    .text_size(px(13.5))
                                    .text_color(c(ui::MUTED))
                                    .child(line),
                            ),
                    )
                    .child(body),
            )
            .into_any_element()
    }

    fn settings_general(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let st = cx.global::<AppSettings>().clone();
        let toggle = |id: &'static str,
                      title: &'static str,
                      line: &'static str,
                      on: bool,
                      set: fn(&mut AppSettings, bool),
                      cx: &mut Context<Self>| {
            row(
                title,
                line,
                div()
                    .id(id)
                    .cursor_pointer()
                    .child(switch(on))
                    .on_click(cx.listener(move |_, _, _, cx| {
                        save(cx, |st| set(st, !on));
                        cx.notify();
                    })),
            )
        };
        let number = |id: &'static str,
                      title: &'static str,
                      line: &'static str,
                      shown: String,
                      get: fn(&AppSettings) -> u32,
                      set: fn(&mut AppSettings, u32),
                      step: u32,
                      cx: &mut Context<Self>| {
            row(
                title,
                line,
                stepper(
                    shown,
                    step_button((id, 0usize), IconName::Minus).on_click(cx.listener(
                        move |_, _, _, cx| {
                            save(cx, |st| {
                                let v = get(st);
                                set(st, v.saturating_sub(step));
                            });
                            cx.notify();
                        },
                    )),
                    step_button((id, 1usize), IconName::Plus).on_click(cx.listener(
                        move |_, _, _, cx| {
                            save(cx, |st| {
                                let v = get(st);
                                set(st, v.saturating_add(step));
                            });
                            cx.notify();
                        },
                    )),
                ),
            )
        };
        let dollars = |v: u32| {
            if v == 0 {
                "No limit".to_string()
            } else {
                format!("${v}")
            }
        };
        let minutes = |v: u32| {
            if v == 0 {
                "Never".to_string()
            } else {
                format!("{v} min")
            }
        };

        let look = group(
            "LOOK AND FEEL",
            vec![
                toggle(
                    "set-motion",
                    "Animations",
                    "The crew moves and screens ease in. Off, everything holds still.",
                    st.animations,
                    |s, v| s.animations = v,
                    cx,
                ),
                row(
                    "Crew look",
                    "Fin, the Squad Code mascot, or the bubble crew. Dress each bot under Mascot.",
                    segmented(vec![
                        segment("set-look-fin", "Fin", st.mascot != "bubble").on_click(
                            cx.listener(|_, _, _, cx| {
                                save(cx, |st| st.mascot = "fin".into());
                                cx.notify();
                            }),
                        ),
                        segment("set-look-bubble", "Bubbles", st.mascot == "bubble").on_click(
                            cx.listener(|_, _, _, cx| {
                                save(cx, |st| st.mascot = "bubble".into());
                                cx.notify();
                            }),
                        ),
                    ]),
                ),
            ],
        );
        let alerts = group(
            "ALERTS",
            vec![
                toggle(
                    "set-notify",
                    "Desktop notifications",
                    "A notification when a job finishes or a bot needs you.",
                    st.notifications,
                    |s, v| s.notifications = v,
                    cx,
                ),
                number(
                    "set-stuck",
                    "Flag a stuck bot after",
                    "A bot quiet this long is marked as maybe stuck, so you can look in.",
                    minutes(st.stuck_after_mins),
                    |s| s.stuck_after_mins,
                    |s, v| s.stuck_after_mins = v,
                    5,
                    cx,
                ),
            ],
        );
        let work = group(
            "WHILE THE CREW WORKS",
            vec![
                toggle(
                    "set-awake",
                    "Keep the computer awake",
                    "Stops the computer sleeping while a job runs.",
                    st.keep_awake,
                    |s, v| s.keep_awake = v,
                    cx,
                ),
                toggle(
                    "set-credit",
                    "Credit the agent in commits",
                    "Adds a line naming the agent to commits made from Squad Code.",
                    st.attribution,
                    |s, v| s.attribution = v,
                    cx,
                ),
                toggle(
                    "set-confirm",
                    "Ask before deleting",
                    "Confirm before a job and its work are thrown away.",
                    st.confirm_delete,
                    |s, v| s.confirm_delete = v,
                    cx,
                ),
                toggle(
                    "set-tidy",
                    "Remove the worktree with the job",
                    "Deleting a job also deletes the copy of the code it worked in.",
                    st.remove_worktree_on_delete,
                    |s, v| s.remove_worktree_on_delete = v,
                    cx,
                ),
            ],
        );
        let limits = group(
            "SPENDING LIMITS",
            vec![
                number(
                    "set-day-limit",
                    "Each day",
                    "Jobs wait once the crew has spent this much today.",
                    dollars(st.daily_limit_usd),
                    |s| s.daily_limit_usd,
                    |s, v| s.daily_limit_usd = v,
                    5,
                    cx,
                ),
                number(
                    "set-job-limit",
                    "Each job",
                    "A job stops and asks you once it has spent this much.",
                    dollars(st.session_limit_usd),
                    |s| s.session_limit_usd,
                    |s, v| s.session_limit_usd = v,
                    1,
                    cx,
                ),
            ],
        );
        Self::page(
            "settings-general",
            "General",
            "How Squad Code behaves while your crew works. Changes save straight away.",
            div()
                .flex()
                .flex_col()
                .gap(px(space::M))
                .child(look)
                .child(alerts)
                .child(work)
                .child(limits),
        )
    }

    fn settings_mascot(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let st = cx.global::<AppSettings>().clone();
        let crew = Member::all();
        if !crew.contains(&self.fin_sel) {
            self.fin_sel = crew.first().copied().unwrap_or(Member::Builder);
        }
        let who = self.fin_sel;
        let is_fin = st.mascot != "bubble";
        let fin = mascot::fin_in(&st, who);
        let (tint, outfit) = (fin.tint, fin.outfit);

        // The preview plays through every state unless one is held.
        let state = self.fin_state.unwrap_or_else(|| {
            let i = (self.now() / 3.2) as usize % State::ALL.len();
            State::ALL[i]
        });
        let figure = if is_fin {
            Figure::Fin(FinPose {
                fin,
                state,
                shine: false,
            })
        } else {
            Figure::Bubble(who.bubble().pose(Mood::Signature))
        };
        let preview = self.pose_img(figure, 240., Framing::Full);
        let accent = who.accent();
        let mut states = div().flex().flex_wrap().gap(px(6.));
        for (i, s) in State::ALL.into_iter().enumerate() {
            let on = self.fin_state == Some(s) || (self.fin_state.is_none() && s == state);
            states = states.child(
                ui::pill(("fin-state", i), s.name(), on).on_click(cx.listener(
                    move |this, _, _, cx| {
                        this.fin_state = if this.fin_state == Some(s) {
                            None
                        } else {
                            Some(s)
                        };
                        cx.notify();
                    },
                )),
            );
        }
        let role_name = match mascot::role_of(outfit) {
            Some(r) => r.name.to_string(),
            None if outfit == Outfit::PLAIN => "Plain".to_string(),
            None => "Custom outfit".to_string(),
        };
        let stage = ui::card()
            .w(px(380.))
            .flex_none()
            .p(px(space::M))
            .flex()
            .flex_col()
            .gap(px(space::SM))
            .child(
                div()
                    .h(px(330.))
                    .rounded(px(10.))
                    .border_1()
                    .border_color(c(ui::LINE2))
                    .bg(linear_gradient(
                        180.,
                        linear_color_stop(ca(accent, 0.14), 0.),
                        linear_color_stop(c(ui::BG), 1.),
                    ))
                    .relative()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(preview)
                    .child(
                        div()
                            .absolute()
                            .left(px(16.))
                            .bottom(px(12.))
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(
                                div()
                                    .text_size(px(18.))
                                    .font_weight(FontWeight::BOLD)
                                    .child(who.name()),
                            )
                            .child(div().text_size(px(12.)).text_color(c(ui::MUTED)).child(
                                if is_fin {
                                    format!("{role_name} · {}", state.name())
                                } else {
                                    who.role().to_string()
                                },
                            )),
                    ),
            )
            .when(is_fin, |d| d.child(states));

        // Whose Fin is being dressed.
        let mut pick = div().flex().flex_wrap().gap(px(8.));
        for m in crew.iter().copied() {
            let on = m == who;
            let face = self.avatar(m, 34.);
            pick = pick.child(
                div()
                    .id(("fin-who", m.key() as usize))
                    .px(px(8.))
                    .py(px(6.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .rounded(px(10.))
                    .cursor_pointer()
                    .border_1()
                    .map(|d| {
                        if on {
                            d.bg(c(ui::HOVER)).border_color(c(ui::LINE_HI))
                        } else {
                            d.border_color(c(ui::LINE)).hover(|s| s.bg(c(ui::HOVER)))
                        }
                    })
                    .child(face)
                    .child(
                        div()
                            .pr(px(4.))
                            .text_size(px(12.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(c(if on { ui::INK } else { ui::BODY }))
                            .child(m.name()),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.fin_sel = m;
                        cx.notify();
                    })),
            );
        }

        let dress = move |tint: Tint, outfit: Outfit| {
            move |_: &mut Shell, _: &ClickEvent, _: &mut Window, cx: &mut Context<Shell>| {
                save(cx, |st| mascot::set_look(st, who, tint, outfit));
                cx.notify();
            }
        };
        let mut roles = div();
        roles = roles.child(
            choice("fin-role-plain", "Plain", outfit == Outfit::PLAIN, true)
                .on_click(cx.listener(dress(tint, Outfit::PLAIN))),
        );
        for (i, r) in ROLES.iter().enumerate() {
            roles = roles.child(
                choice(("fin-role", i), r.name, outfit == r.outfit, true)
                    .on_click(cx.listener(dress(r.tint, r.outfit))),
            );
        }
        let mut hats = div();
        for (i, h) in Hat::ALL.iter().copied().enumerate() {
            hats = hats.child(
                choice(("fin-hat", i), h.name(), outfit.hat == h, true)
                    .on_click(cx.listener(dress(tint, Outfit { hat: h, ..outfit }))),
            );
        }
        let mut lefts = div();
        for (i, h) in Held::ALL.iter().copied().enumerate() {
            lefts = lefts.child(
                choice(("fin-left", i), h.name(), outfit.left == h, true)
                    .on_click(cx.listener(dress(tint, Outfit { left: h, ..outfit }))),
            );
        }
        let mut rights = div();
        for (i, t) in Tool::ALL.iter().copied().enumerate() {
            rights = rights.child(
                choice(("fin-right", i), t.name(), outfit.right == t, true)
                    .on_click(cx.listener(dress(tint, Outfit { right: t, ..outfit }))),
            );
        }
        let mut badges = div();
        for (i, b) in Badge::ALL.iter().copied().enumerate() {
            badges = badges.child(
                choice(("fin-badge", i), b.name(), outfit.badge == b, true)
                    .on_click(cx.listener(dress(tint, Outfit { badge: b, ..outfit }))),
            );
        }
        let mut colours = div().pt(px(2.));
        for (i, (_, name, t)) in mascot::COLOURS.iter().copied().enumerate() {
            let on = t == tint;
            colours = colours.child(
                div()
                    .id(("fin-colour", i))
                    .size(px(24.))
                    .rounded_full()
                    .cursor_pointer()
                    .p(px(2.))
                    .border_2()
                    .border_color(if on { c(ui::INK) } else { ca(0, 0.) })
                    .child(div().size_full().rounded_full().bg(linear_gradient(
                        135.,
                        linear_color_stop(c(t.0[0].0), 0.),
                        linear_color_stop(c(t.0[1].0), 1.),
                    )))
                    .tooltip(move |w, cx| Tooltip::new(name).build(w, cx))
                    .on_click(cx.listener(dress(t, outfit))),
            );
        }
        let is_default = (tint, outfit) == mascot::default_look(who);
        let (dt, dout) = mascot::default_look(who);

        let material = Material::from_id(&st.fin_material);
        let mut mats = Vec::new();
        for m in Material::ALL {
            mats.push(
                segment(("fin-mat", m as usize), m.name(), material == m).on_click(cx.listener(
                    move |_, _, _, cx| {
                        save(cx, |st| st.fin_material = m.id().into());
                        cx.notify();
                    },
                )),
            );
        }
        let looks = ui::card()
            .overflow_hidden()
            .flex()
            .flex_col()
            .child(div().px(px(18.)).pt(px(16.)).pb(px(6.)).child(section(
                "Crew look",
                "How every bot is drawn, everywhere in the app.",
            )))
            .child(row(
                "Mascot",
                "Fin is Squad Code's hex bot. The bubble crew is the earlier look.",
                segmented(vec![
                    segment("fin-on", "Fin", is_fin).on_click(cx.listener(|_, _, _, cx| {
                        save(cx, |st| st.mascot = "fin".into());
                        cx.notify();
                    })),
                    segment("fin-off", "Bubbles", !is_fin).on_click(cx.listener(|_, _, _, cx| {
                        save(cx, |st| st.mascot = "bubble".into());
                        cx.notify();
                    })),
                ]),
            ))
            .when(is_fin, |d| {
                d.child(row(
                    "Material",
                    "Liquid glass, a slowly shifting rainbow glow, or solid colour.",
                    segmented(mats),
                ))
            });

        let wardrobe = ui::card()
            .p(px(18.))
            .flex()
            .flex_col()
            .gap(px(space::SM))
            .child(
                div()
                    .flex()
                    .items_start()
                    .child(div().flex_1().child(section(
                        "Dress a bot",
                        "Start from a role kit, then swap any prop. Each bot keeps its own.",
                    )))
                    .when(!is_default, |d| {
                        d.child(
                            ui::ghost("fin-reset", "Reset").on_click(cx.listener(dress(dt, dout))),
                        )
                    }),
            )
            .child(pick)
            .child(div().h(px(1.)).my(px(4.)).bg(c(ui::LINE2)))
            .child(slot("ROLE", roles))
            .child(slot("HEAD", hats))
            .child(slot("LEFT FIN", lefts))
            .child(slot("RIGHT FIN", rights))
            .child(slot("CHEST", badges))
            .child(slot("COLOUR", colours.flex().gap(px(8.))));

        let mut lineup = div().flex().flex_wrap().gap(px(space::M));
        for m in crew.iter().copied() {
            let img = self.bot(m, Mood::Idle, 76., Framing::Full);
            lineup = lineup.child(
                div()
                    .id(("fin-lineup", m.key() as usize))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(4.))
                    .cursor_pointer()
                    .child(img)
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(c(if m == who { ui::INK } else { ui::MUTED }))
                            .child(m.name()),
                    )
                    .on_hover(cx.listener(move |this, on: &bool, _, cx| this.hover_bot(m, *on, cx)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.fin_sel = m;
                        cx.notify();
                    })),
            );
        }
        let team = ui::card()
            .p(px(18.))
            .flex()
            .flex_col()
            .gap(px(space::M))
            .child(section(
                "Your crew",
                "How the whole crew looks together. Click one to dress it.",
            ))
            .child(lineup);

        let right = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(space::M))
            .child(looks)
            .when(is_fin, |d| d.child(wardrobe));

        Self::page(
            "settings-mascot",
            "Mascot",
            "Meet Fin. It hovers on a ring of light: the fins show effort, the brows show mood and the ring shows status.",
            div()
                .flex()
                .flex_col()
                .gap(px(space::M))
                .child(
                    div()
                        .flex()
                        .items_start()
                        .gap(px(space::M))
                        .child(stage)
                        .child(right),
                )
                .child(team),
        )
    }
}
