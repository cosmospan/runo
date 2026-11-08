//! The first-run walkthrough: check which coding agents are installed and pick
//! a folder. Two short steps, both skippable, shown once. The crew starts with
//! just Fin, who leads it: Fin answers questions and offers the other bots as
//! jobs need them, and any bot can be added or made on Crew. What to do first
//! is on the Jobs page, which opens next.

use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use squad_holo::{Framing, Mood};

use super::data::{self, Member};
use super::ui::{self, c, ca};
use super::{Place, Shell};

const STEPS: [&str; 2] = ["Engines", "Folder"];

impl Shell {
    /// Ends the walkthrough with the crew picked so far.
    pub(super) fn finish_setup(&mut self, _cx: &mut Context<Self>) {
        self.crew.store.removed = Member::CREW
            .into_iter()
            .filter(|m| !self.setup_crew.contains(m))
            .collect();
        data::set_removed(&self.crew.store.removed);
        self.crew.store.set_up = true;
        self.crew.store.save();
        self.trail.clear();
    }

    fn setup_step(&mut self, step: usize, cx: &mut Context<Self>) {
        self.setup_step = step.min(STEPS.len() - 1);
        self.entered = std::time::Instant::now();
        cx.notify();
    }

    pub(super) fn welcome(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let step = self.setup_step;
        let body = match step {
            0 => self.welcome_engines(cx).into_any_element(),
            _ => self.welcome_folder(cx).into_any_element(),
        };
        // The steps down the left, each with where it stands.
        let ready = {
            let st = cx.global::<crate::settings::AppSettings>();
            (0..crate::agent::PROVIDERS.len())
                .filter(|&i| super::engine::ready(st, i))
                .count()
        };
        let notes = [
            match ready {
                0 => "None found yet".to_string(),
                1 => "1 agent ready".to_string(),
                n => format!("{n} agents ready"),
            },
            self.crew
                .store
                .folder()
                .map(|f| f.name.clone())
                .unwrap_or_else(|| "Not picked yet".into()),
        ];
        let mut steps = div().flex().flex_col().gap(px(4.));
        for (i, name) in STEPS.into_iter().enumerate() {
            let done = i < step;
            let on = i == step;
            steps = steps.child(
                div()
                    .id(("setup-dot", i))
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .px(px(12.))
                    .py(px(10.))
                    .rounded(px(10.))
                    .cursor_pointer()
                    .map(|d| {
                        if on {
                            d.bg(c(ui::hover()))
                        } else {
                            d.hover(|s| s.bg(c(ui::hover())))
                        }
                    })
                    .child(
                        div()
                            .size(px(24.))
                            .flex_none()
                            .rounded_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(11.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .map(|d| {
                                if on {
                                    d.bg(c(ui::fill())).text_color(c(ui::on_fill()))
                                } else if done {
                                    d.bg(ca(ui::GREEN, 0.16)).text_color(c(ui::t_green()))
                                } else {
                                    d.border_1()
                                        .border_color(c(ui::line_hi()))
                                        .text_color(c(ui::muted()))
                                }
                            })
                            .child(if done {
                                "\u{2713}".to_string()
                            } else {
                                format!("{}", i + 1)
                            }),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(13.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(c(if on || done { ui::ink() } else { ui::body() }))
                                    .child(name),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(c(ui::muted()))
                                    .truncate()
                                    .child(notes[i].clone()),
                            ),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.setup_step(i, cx))),
            );
        }
        let back = (step > 0).then(|| {
            ui::ghost("setup-back", "Back")
                .on_click(cx.listener(move |this, _, _, cx| this.setup_step(step - 1, cx)))
        });
        let next: Stateful<Div> = if step < STEPS.len() - 1 {
            ui::solid("setup-next", "Continue")
                .on_click(cx.listener(move |this, _, _, cx| this.setup_step(step + 1, cx)))
        } else {
            ui::solid("setup-done", "Start using Squad Code").on_click(cx.listener(
                |this, _, _, cx| {
                    this.finish_setup(cx);
                    this.go(Place::Today, cx);
                },
            ))
        };
        let mark = self.bot(Member::Builder, Mood::Signature, 56., Framing::Bust);
        let left = div()
            .w(px(320.))
            .flex_none()
            .h_full()
            .bg(c(ui::panel()))
            .border_r_1()
            .border_color(c(ui::line()))
            .px(px(24.))
            .pt(px(if cfg!(target_os = "macos") { 48. } else { 28. }))
            .pb(px(24.))
            .flex()
            .flex_col()
            .gap(px(22.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(mark)
                    .child(
                        div()
                            .text_size(px(20.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(c(ui::ink()))
                            .child("Welcome to Squad Code"),
                    )
                    .child(
                        div()
                            .text_size(px(13.))
                            .line_height(px(19.))
                            .text_color(c(ui::muted()))
                            .child("Two quick checks, then you're on Jobs with a few ideas for a first job. Both can be changed later."),
                    ),
            )
            .child(steps)
            .child(div().flex_1())
            .child(
                div()
                    .id("setup-skip")
                    .text_size(px(12.5))
                    .text_color(c(ui::muted()))
                    .cursor_pointer()
                    .hover(|s| s.text_color(c(ui::ink())))
                    .child("Skip setup and go to Jobs")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.finish_setup(cx);
                        this.go(Place::Today, cx);
                    })),
            );
        let right = div()
            .id("welcome")
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_y_scroll()
            .child(
                div()
                    .min_h_full()
                    .flex()
                    .flex_col()
                    .justify_center()
                    .items_center()
                    .px(px(40.))
                    .py(px(40.))
                    .child(ui::rise(
                        div().w_full().max_w(px(760.)).child(body),
                        self.stagger(0),
                    ))
                    .child(
                        div()
                            .w_full()
                            .max_w(px(760.))
                            .mt(px(22.))
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .children(back)
                            .child(div().flex_1())
                            .child(next),
                    ),
            );
        div()
            .size_full()
            .flex()
            .child(left)
            .child(right)
            .into_any_element()
    }

    fn welcome_engines(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let st = cx.global::<crate::settings::AppSettings>().clone();
        let ready: Vec<usize> = (0..crate::agent::PROVIDERS.len())
            .filter(|i| super::engine::ready(&st, *i))
            .collect();
        let hero = self.bot(Member::Builder, Mood::Idle, 150., Framing::Full);
        let mut list = div().flex().flex_wrap().gap(px(8.));
        // The installed agents first, then the three most people use, greyed.
        let common = ["claude", "codex", "gemini"];
        let mut shown: Vec<(usize, bool)> = ready.iter().map(|i| (*i, true)).collect();
        for id in common {
            if let Some(i) = crate::agent::PROVIDERS.iter().position(|p| p.id == id) {
                if !ready.contains(&i) {
                    shown.push((i, false));
                }
            }
        }
        for (n, (i, ok)) in shown.into_iter().enumerate() {
            let p = &crate::agent::PROVIDERS[i];
            list = list.child(
                div()
                    .id(("setup-agent", n))
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .px(px(12.))
                    .h(px(34.))
                    .rounded_full()
                    .border_1()
                    .border_color(c(if ok { ui::ok_tone().2 } else { ui::line() }))
                    .bg(c(if ok { ui::ok_tone().1 } else { ui::panel() }))
                    .text_size(px(13.))
                    .text_color(c(if ok { ui::ink() } else { ui::faint() }))
                    .child(if ok { "✓" } else { "○" })
                    .child(p.name)
                    .when(!ok, |d| {
                        d.child(div().text_size(px(11.)).child("not found"))
                    }),
            );
        }
        let words = if ready.is_empty() {
            "The crew works through a coding agent on this computer, and none was found. Install Claude Code, Codex or Gemini CLI and sign in, then check again."
        } else {
            "The crew works through the coding agents on this computer. These are ready; you can pick which one each bot uses later, in Engines."
        };
        card_with(
            hero,
            "Is a coding agent installed?",
            words,
            div()
                .flex()
                .flex_col()
                .gap(px(14.))
                .child(list)
                .child(
                    div()
                        .flex()
                        .child(
                            ui::ghost("setup-recheck", "Check again").on_click(cx.listener(
                                |this, _, _, cx| {
                                    crate::caps::refresh();
                                    this.say("Looking for coding agents…");
                                    cx.notify();
                                },
                            )),
                        ),
                ),
        )
    }

    fn welcome_folder(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let hero = self.bot(Member::Keeper, Mood::Idle, 150., Framing::Full);
        let mut list = div().flex().flex_col().gap(px(6.));
        let current = self.crew.store.current;
        for (i, f) in self.crew.store.folders.iter().enumerate().take(5) {
            let on = i == current;
            list = list.child(
                div()
                    .id(("setup-folder", i))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .px(px(12.))
                    .py(px(9.))
                    .rounded(px(12.))
                    .border_1()
                    .border_color(c(if on { ui::ink() } else { ui::line() }))
                    .bg(c(ui::panel()))
                    .cursor_pointer()
                    .child(
                        Icon::new(IconName::Folder)
                            .size(px(15.))
                            .text_color(c(ui::muted())),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(13.5))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(f.name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(c(ui::faint()))
                                    .truncate()
                                    .child(f.path.clone()),
                            ),
                    )
                    .when(on, |d| d.child(div().text_color(c(ui::GREEN)).child("✓")))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.crew.store.current = i;
                        this.crew.store.save();
                        cx.notify();
                    })),
            );
        }
        let words = if self.crew.store.folders.is_empty() {
            "Pick the project the crew should work in. You can add more folders later, or drop one onto the window."
        } else {
            "The crew works in one project folder at a time. Pick one here, or add another. You can switch any time from Jobs."
        };
        card_with(
            hero,
            "Where's your code?",
            words,
            div().flex().flex_col().gap(px(12.)).child(list).child(
                div().flex().child(
                    ui::ghost("setup-add-folder", "Choose a folder…")
                        .gap(px(6.))
                        .on_click(cx.listener(|this, _, _, cx| this.pick_folder(cx))),
                ),
            ),
        )
    }
}

fn card_with(hero: Img, title: &'static str, words: &'static str, body: impl IntoElement) -> Div {
    ui::card()
        .p(px(28.))
        .flex()
        .gap(px(26.))
        .child(div().flex_none().child(hero))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .pt(px(10.))
                .flex()
                .flex_col()
                .gap(px(10.))
                .child(
                    div()
                        .text_size(px(30.))
                        .font_weight(FontWeight::BOLD)
                        .child(title),
                )
                .child(
                    div()
                        .text_size(px(14.))
                        .line_height(px(21.))
                        .text_color(c(ui::body()))
                        .child(words),
                )
                .child(div().mt(px(8.)).child(body)),
        )
}
