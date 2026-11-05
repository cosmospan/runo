//! Settings: a list of panes on the left and the open pane on the right, in the
//! same dark cards, pills and hairlines as the rest of v2.
//!
//! - **General**: how the app behaves while the crew works, and the limits.
//! - **Appearance**: the theme, solid or frosted glass, with a preview of each.
//! - **Mascot**: the crew's look. Fin's material, and each bot's colour and
//!   props, with a live preview.
//! - **Engines**: which coding agent and model each bot runs on.
//!
//! Every choice is saved the moment it's made.

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
use super::theme::Theme;
use super::ui::{self, c, ca, space};
use super::{Place, Shell};

/// A pane of Settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Pane {
    General,
    Appearance,
    Mascot,
    Engines,
}

impl Pane {
    pub const ALL: [Pane; 4] = [Pane::General, Pane::Appearance, Pane::Mascot, Pane::Engines];

    pub fn name(self) -> &'static str {
        match self {
            Pane::General => "General",
            Pane::Appearance => "Appearance",
            Pane::Mascot => "Mascot",
            Pane::Engines => "Engines",
        }
    }

    fn line(self) -> &'static str {
        match self {
            Pane::General => "Alerts, motion and limits",
            Pane::Appearance => "Themes and frosted glass",
            Pane::Mascot => "Fin, colours and props",
            Pane::Engines => "Agents and models",
        }
    }

    fn icon(self) -> IconName {
        match self {
            Pane::General => IconName::Settings,
            Pane::Appearance => IconName::Palette,
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
        .bg(c(if on { ui::VIOLET } else { ui::off() }))
        .border_1()
        .border_color(c(if on { 0x8E66FF } else { ui::line_hi() }))
        .relative()
        .child(
            div()
                .absolute()
                .top(px(2.))
                .left(px(if on { 16. } else { 2. }))
                .size(px(14.))
                .rounded_full()
                .bg(c(0xFFFFFF)),
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
        .border_color(c(ui::line2()))
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
                        .text_color(c(ui::ink()))
                        .child(title),
                )
                .child(
                    div()
                        .text_size(px(12.5))
                        .text_color(c(ui::muted()))
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
            .text_color(c(ui::muted()))
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
        .bg(c(ui::bg()))
        .border_1()
        .border_color(c(ui::line()));
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
                d.bg(c(ui::raised()))
                    .border_1()
                    .border_color(c(ui::line_hi()))
                    .text_color(c(ui::ink()))
            } else {
                d.text_color(c(ui::muted()))
                    .hover(|s| s.text_color(c(ui::ink())))
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
        .border_color(c(ui::line()))
        .bg(c(ui::raised()))
        .child(down)
        .child(
            div()
                .min_w(px(78.))
                .px(px(6.))
                .flex()
                .justify_center()
                .text_size(px(12.5))
                .font_weight(FontWeight::MEDIUM)
                .text_color(c(ui::ink()))
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
        .text_color(c(ui::body()))
        .cursor_pointer()
        .hover(|s| s.bg(c(ui::hover())).text_color(c(ui::ink())))
        .active(|s| s.bg(c(ui::press())))
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
                .text_color(c(ui::faint()))
                .child(label),
        )
        .child(items.flex_1().flex().flex_wrap().gap(px(6.)))
}

/// A small picture of a theme: a sidebar and a card on its page, with text lines, a
/// button and the status colours. Frosted themes sit over a colourful backdrop, to
/// show the desktop coming through.
fn swatch(t: &'static Theme) -> Div {
    let line = |w: f32, color: u32| div().h(px(4.)).w(px(w)).rounded_full().bg(c(color));
    let dot = |color: u32| div().size(px(6.)).rounded_full().bg(c(color));
    let page = div()
        .size_full()
        .flex()
        .bg(c(t.bg))
        .child(
            div()
                .w(px(48.))
                .h_full()
                .flex()
                .flex_col()
                .gap(px(6.))
                .p(px(8.))
                .border_r_1()
                .border_color(c(t.line_c))
                .child(line(28., t.ink))
                .child(div().h(px(10.)).w_full().rounded(px(3.)).bg(c(t.hover)))
                .child(line(22., t.muted))
                .child(line(26., t.faint)),
        )
        .child(
            div().flex_1().p(px(8.)).child(
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .p(px(8.))
                    .rounded(px(6.))
                    .bg(c(t.panel))
                    .border_1()
                    .border_color(c(t.line_c))
                    .child(line(64., t.ink))
                    .child(line(84., t.body))
                    .child(
                        div()
                            .flex()
                            .gap(px(4.))
                            .child(dot(t.t_blue))
                            .child(dot(t.t_amber))
                            .child(dot(t.t_green))
                            .child(dot(t.t_red)),
                    )
                    .child(
                        div()
                            .mt_auto()
                            .flex()
                            .justify_end()
                            .child(div().h(px(12.)).w(px(34.)).rounded(px(3.)).bg(c(t.fill))),
                    ),
            ),
        );
    div()
        .h(px(112.))
        .w_full()
        .rounded(px(8.))
        .overflow_hidden()
        .relative()
        .bg(c(if t.light { 0xDCE3F2 } else { 0x0B0B10 }))
        .when(t.frosted, |d| {
            d.bg(linear_gradient(
                135.,
                linear_color_stop(c(0xF76B8A), 0.),
                linear_color_stop(c(0x5B7CFA), 1.),
            ))
            .child(
                div()
                    .absolute()
                    .top(px(-18.))
                    .left(px(70.))
                    .size(px(80.))
                    .rounded_full()
                    .bg(c(0xFFC24B)),
            )
            .child(
                div()
                    .absolute()
                    .bottom(px(-24.))
                    .left(px(-10.))
                    .size(px(90.))
                    .rounded_full()
                    .bg(c(0x2EC5A1)),
            )
        })
        .child(div().absolute().inset_0().child(page))
}

impl Shell {
    pub(super) fn settings(&mut self, pane: Pane, cx: &mut Context<Self>) -> AnyElement {
        // The panes as a row of tabs across the top, the one you're on
        // underlined.
        let mut tabs = div().flex().items_end().gap(px(22.));
        for p in Pane::ALL {
            let on = p == pane;
            tabs = tabs.child(
                div()
                    .id(("settings-pane", p as usize))
                    .relative()
                    .h(px(40.))
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .text_size(px(13.))
                    .font_weight(FontWeight::MEDIUM)
                    .cursor_pointer()
                    .map(|d| {
                        if on {
                            d.text_color(c(ui::ink())).child(
                                div()
                                    .absolute()
                                    .left_0()
                                    .right_0()
                                    .bottom(px(-1.))
                                    .h(px(2.))
                                    .rounded_t(px(2.))
                                    .bg(c(ui::ink())),
                            )
                        } else {
                            d.text_color(c(ui::muted()))
                                .hover(|s| s.text_color(c(ui::ink())))
                        }
                    })
                    .child(Icon::new(p.icon()).size(px(14.)))
                    .child(p.name())
                    .tooltip(move |window, cx| {
                        gpui_kit::component::tooltip::Tooltip::new(p.line()).build(window, cx)
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        // Switching panes stays inside Settings: no page fade, no
                        // extra step on the back trail.
                        this.settings_pane = p;
                        this.place = Place::Settings(p);
                        cx.notify();
                    })),
            );
        }
        let nav = div()
            .flex_none()
            .px(px(28.))
            .pt(px(6.))
            .border_b_1()
            .border_color(c(ui::line2()))
            .child(tabs);
        let body = match pane {
            Pane::General => self.settings_general(cx),
            Pane::Appearance => self.settings_appearance(cx),
            Pane::Mascot => self.settings_mascot(cx),
            Pane::Engines => self.engines(cx),
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(nav)
            .child(div().flex_1().min_h_0().w_full().child(body))
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
                                    .text_size(px(24.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(title),
                            )
                            .child(
                                div()
                                    .mt(px(6.))
                                    .text_size(px(13.5))
                                    .text_color(c(ui::muted()))
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
            vec![toggle(
                "set-motion",
                "Animations",
                "The crew moves and screens ease in. Off, everything holds still.",
                st.animations,
                |s, v| s.animations = v,
                cx,
            )],
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

    fn settings_appearance(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let current = ui::theme().id;
        let card_for = |t: &'static Theme, cx: &mut Context<Self>| {
            let on = t.id == current;
            div()
                .id(SharedString::from(format!("theme-{}", t.id)))
                .w(px(206.))
                .flex()
                .flex_col()
                .gap(px(10.))
                .p(px(10.))
                .rounded(px(12.))
                .border_1()
                .cursor_pointer()
                .map(|d| {
                    if on {
                        d.border_color(c(ui::VIOLET)).bg(c(ui::panel()))
                    } else {
                        d.border_color(c(ui::line()))
                            .bg(c(ui::panel()))
                            .hover(|s| s.border_color(c(ui::line_hi())))
                    }
                })
                .child(swatch(t))
                .child(
                    div()
                        .px(px(4.))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .gap(px(2.))
                                .child(
                                    div()
                                        .text_size(px(13.))
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(c(ui::ink()))
                                        .child(t.name),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.5))
                                        .text_color(c(ui::muted()))
                                        .truncate()
                                        .child(t.line),
                                ),
                        )
                        .when(on, |d| {
                            d.child(
                                div()
                                    .size(px(20.))
                                    .flex_none()
                                    .rounded_full()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .bg(c(ui::VIOLET))
                                    .text_color(c(0xFFFFFF))
                                    .child(Icon::new(IconName::Check).size(px(12.))),
                            )
                        }),
                )
                .on_click(cx.listener(move |_, _, _, cx| {
                    ui::set_theme(t.id);
                    save(cx, |st| st.v2_theme = t.id.to_string());
                    cx.notify();
                }))
        };
        let grid = |frosted: bool, cx: &mut Context<Self>| {
            let mut d = div().flex().flex_wrap().gap(px(space::SM));
            for t in ui::THEMES.iter().filter(|t| t.frosted == frosted) {
                d = d.child(card_for(t, cx));
            }
            d
        };
        let heading = |title: &'static str, line: &'static str| {
            div()
                .flex()
                .flex_col()
                .gap(px(3.))
                .child(
                    div()
                        .text_size(px(13.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(c(ui::ink()))
                        .child(title),
                )
                .child(
                    div()
                        .text_size(px(12.5))
                        .text_color(c(ui::muted()))
                        .child(line),
                )
        };
        let solid = grid(false, cx);
        let frosted = grid(true, cx);
        Self::page(
            "settings-appearance",
            "Appearance",
            "How Squad Code looks. A theme applies the moment you pick it.",
            div()
                .flex()
                .flex_col()
                .gap(px(space::L))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(space::SM))
                        .child(heading("Solid", "An opaque window, quickest to draw."))
                        .child(solid),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(space::SM))
                        .child(heading(
                            "Frosted",
                            "A see-through window over your desktop, blurred like frosted glass.",
                        ))
                        .child(frosted),
                ),
        )
    }

    fn settings_mascot(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let st = cx.global::<AppSettings>().clone();
        let crew = Member::all();
        if !crew.contains(&self.fin_sel) {
            self.fin_sel = crew.first().copied().unwrap_or(Member::Builder);
        }
        let who = self.fin_sel;
        let fin = mascot::fin_in(&st, who);
        let (tint, outfit) = (fin.tint, fin.outfit);

        // The preview plays through every state unless one is held.
        let state = self.fin_state.unwrap_or_else(|| {
            let i = (self.now() / 3.2) as usize % State::ALL.len();
            State::ALL[i]
        });
        let figure = Figure::Fin(FinPose {
            fin,
            state,
            shine: false,
            gaze: (0, 0),
            offset: 0,
        });
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
                    .border_color(c(ui::line2()))
                    .bg(linear_gradient(
                        180.,
                        linear_color_stop(ca(accent, 0.14), 0.),
                        linear_color_stop(c(ui::bg()), 1.),
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
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(c(ui::muted()))
                                    .child(format!("{role_name} · {}", state.name())),
                            ),
                    ),
            )
            .child(states);

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
                            d.bg(c(ui::hover())).border_color(c(ui::line_hi()))
                        } else {
                            d.border_color(c(ui::line()))
                                .hover(|s| s.bg(c(ui::hover())))
                        }
                    })
                    .child(face)
                    .child(
                        div()
                            .pr(px(4.))
                            .text_size(px(12.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(c(if on { ui::ink() } else { ui::body() }))
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
                    .border_color(if on { c(ui::ink()) } else { ca(0, 0.) })
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
                "What Fin is made of, for every bot, everywhere in the app.",
            )))
            .child(row(
                "Material",
                "Liquid glass, a slowly shifting rainbow glow, or solid colour.",
                segmented(mats),
            ));

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
            .child(div().h(px(1.)).my(px(4.)).bg(c(ui::line2())))
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
                            .text_color(c(if m == who { ui::ink() } else { ui::muted() }))
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
            .child(wardrobe);

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
