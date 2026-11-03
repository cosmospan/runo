//! The crew dock: the bots float in a glass bar at the bottom of the window.
//! Each grows as the pointer passes, shows what it's doing, and opens its chat
//! (or the job it's on) when clicked.

use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use runo_holo::Framing;

use super::data::Member;
use super::ui::{self, c, ca};
use super::{Place, Shell};

/// A bot's size in the dock at rest, under the pointer, and beside it.
const REST: f32 = 54.;
const NEAR: f32 = 64.;
const UNDER: f32 = 78.;

impl Shell {
    /// Eases a dock slot toward its size; returns the size to draw now.
    fn dock_size(&mut self, key: u64, to: f32) -> f32 {
        let v = self.dock_sizes.entry(key).or_insert(to);
        *v += (to - *v) * if self.motion { 0.28 } else { 1.0 };
        if (to - *v).abs() < 0.3 {
            *v = to;
        } else {
            self.dock_moving = true;
        }
        // Whole pixels, so a frame drawn at one size is reused, not redrawn.
        v.round()
    }

    pub(super) fn dock(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        self.dock_moving = false;
        let place = self.place;
        let crew = Member::all();
        let hovered = self
            .rail_hover
            .and_then(|h| crew.iter().position(|m| *m == h));
        let mut bar = div()
            .flex()
            .items_end()
            .gap(px(6.))
            .px(px(12.))
            .pt(px(8.))
            .pb(px(10.))
            .rounded(px(30.))
            .bg(ca(0xFFFFFF, 0.92))
            .border_1()
            .border_color(ca(0xFFFFFF, 0.95))
            .shadow(vec![
                BoxShadow {
                    color: ca(0x28285A, 0.16),
                    offset: point(px(0.), px(18.)),
                    blur_radius: px(44.),
                    spread_radius: px(-10.),
                    inset: false,
                },
                BoxShadow {
                    color: ca(0x28285A, 0.06),
                    offset: point(px(0.), px(1.)),
                    blur_radius: px(3.),
                    spread_radius: px(0.),
                    inset: false,
                },
            ]);
        for (i, who) in crew.iter().copied().enumerate() {
            let target = match hovered {
                Some(h) if h == i => UNDER,
                Some(h) if h.abs_diff(i) == 1 => NEAR,
                _ => REST,
            };
            let size = self.dock_size(who.key(), target);
            let mood = self.mood(who);
            let face = self.bot(who, mood, size, Framing::Full);
            let (line, dot) = self.status_of(who);
            let busy = self.store.busy(who).map(|j| (j.id, j.progress()));
            let here = place == Place::Chat(who)
                || matches!(place, Place::Live(id) if self.store.job(id).is_some_and(|j| j.bot == who));
            let progress = busy.map(|(id, p)| {
                let to = p
                    .map(|(d, n)| (d as f32 + 0.5) / n.max(1) as f32)
                    .unwrap_or(0.0);
                (p.is_some(), self.glide_bar(id, to))
            });
            let asking = dot == ui::AMBER;
            let k = runo_holo::anim::spring(self.hover_at.elapsed().as_secs_f64(), 0.2) as f32;
            bar = bar.child(
                div()
                    .id(("dock-bot", who.key() as usize))
                    .relative()
                    .flex_none()
                    .w(px(size))
                    .flex()
                    .flex_col()
                    .items_center()
                    .cursor_pointer()
                    .on_hover(cx.listener(move |this, on: &bool, _, cx| {
                        if *on {
                            this.rail_hover = Some(who);
                        } else if this.rail_hover == Some(who) {
                            this.rail_hover = None;
                        }
                        this.hover_bot(who, *on, cx);
                    }))
                    // The name and what it's doing, floating above while hovered.
                    .when(hovered == Some(i), |d| {
                        d.child(
                            deferred(
                                div()
                                    .absolute()
                                    .bottom(px(size + 14. + 6. * (1. - k)))
                                    .left(px(size / 2. - 90.))
                                    .w(px(180.))
                                    .flex()
                                    .justify_center()
                                    .opacity(k)
                                    .child(
                                        div()
                                            .px(px(12.))
                                            .py(px(7.))
                                            .rounded(px(12.))
                                            .bg(c(ui::INK))
                                            .text_color(c(0xFFFFFF))
                                            .flex()
                                            .flex_col()
                                            .items_center()
                                            .child(
                                                div()
                                                    .text_size(px(13.))
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .child(who.name()),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(11.))
                                                    .text_color(ca(0xFFFFFF, 0.7))
                                                    .child(line.clone()),
                                            ),
                                    ),
                            )
                            .with_priority(2),
                        )
                    })
                    // A bot that needs you raises a small flag.
                    .when(asking && hovered != Some(i), |d| {
                        let bob = if self.motion {
                            runo_holo::anim::alternate(self.now(), 0.8, runo_holo::anim::SOFT)
                                as f32
                                * 3.
                        } else {
                            0.
                        };
                        d.child(
                            div()
                                .absolute()
                                .top(px(-6. - bob))
                                .right(px(-2.))
                                .size(px(16.))
                                .rounded_full()
                                .bg(c(ui::AMBER))
                                .border_2()
                                .border_color(c(0xFFFFFF))
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(9.))
                                .font_weight(FontWeight::BOLD)
                                .text_color(c(0xFFFFFF))
                                .child("!"),
                        )
                    })
                    .child(face)
                    // Under the bot: its progress while it works, a dot for where you are.
                    .child(
                        div()
                            .h(px(5.))
                            .mt(px(2.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .map(|d| match progress {
                                Some((true, f)) => d.child(
                                    div()
                                        .w(px(size * 0.62))
                                        .child(ui::bar(f, who.accent()).h(px(3.))),
                                ),
                                Some((false, _)) => d.child(
                                    div()
                                        .w(px(size * 0.62))
                                        .child(ui::sweep(who.accent(), 0.).h(px(3.))),
                                ),
                                None if here => {
                                    d.child(div().size(px(5.)).rounded_full().bg(c(ui::INK)))
                                }
                                None => d,
                            }),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let to = busy
                            .map(|(id, _)| Place::Live(id))
                            .unwrap_or(Place::Chat(who));
                        this.go(to, cx);
                    })),
            );
        }
        // After the crew: add a bot, and find anything.
        let tool = |id: &'static str, icon: IconName, tip: &'static str| {
            div()
                .id(id)
                .flex_none()
                .mb(px(7.))
                .size(px(40.))
                .rounded_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(ca(0xFFFFFF, 0.9))
                .border_1()
                .border_color(c(ui::LINE))
                .cursor_pointer()
                .hover(|s| s.bg(c(0xFFFFFF)).border_color(c(0xCFD2E0)))
                .active(|s| s.bg(c(0xF0F1F6)))
                .tooltip(move |window, cx| {
                    gpui_kit::component::tooltip::Tooltip::new(tip).build(window, cx)
                })
                .child(Icon::new(icon).size(px(16.)).text_color(c(ui::MUTED)))
        };
        bar = bar
            .child(
                div()
                    .flex_none()
                    .mb(px(12.))
                    .mx(px(4.))
                    .w(px(1.))
                    .h(px(30.))
                    .bg(c(ui::LINE)),
            )
            .child(
                tool("dock-add", IconName::Plus, "Add or make a bot")
                    .on_click(cx.listener(|this, _, _, cx| this.go(Place::Crew, cx))),
            );
        bar
    }
}
