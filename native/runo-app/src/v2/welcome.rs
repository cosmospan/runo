//! The first-run walkthrough: check which coding agents are installed, pick a
//! folder, choose the crew, and ask a first question. Four short steps, any of
//! them skippable, shown once.

use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use runo_holo::{Framing, Mood};

use super::data::{self, Member};
use super::ui::{self, c, ca};
use super::{Place, Shell};

const STEPS: [&str; 4] = ["Engines", "Folder", "Crew", "First question"];
const FIRST_QUESTION: &str = "What does this project do, in plain words?";

impl Shell {
    /// Ends the walkthrough with the crew picked so far.
    pub(super) fn finish_setup(&mut self, _cx: &mut Context<Self>) {
        self.store.removed = Member::CREW
            .into_iter()
            .filter(|m| !self.setup_crew.contains(m))
            .collect();
        data::set_removed(&self.store.removed);
        self.store.set_up = true;
        self.store.save();
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
            1 => self.welcome_folder(cx).into_any_element(),
            2 => self.welcome_crew(cx).into_any_element(),
            _ => self.welcome_first(cx).into_any_element(),
        };
        // Step dots.
        let mut dots = div().flex().items_center().gap(px(8.));
        for (i, name) in STEPS.into_iter().enumerate() {
            let done = i < step;
            let on = i == step;
            dots = dots.child(
                div()
                    .id(("setup-dot", i))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .px(px(10.))
                    .h(px(28.))
                    .rounded_full()
                    .text_size(px(12.))
                    .font_weight(FontWeight::MEDIUM)
                    .cursor_pointer()
                    .map(|d| {
                        if on {
                            d.bg(c(ui::FILL)).text_color(c(ui::ON_FILL))
                        } else if done {
                            d.bg(ca(ui::GREEN, 0.12)).text_color(c(ui::T_GREEN))
                        } else {
                            d.bg(ca(ui::PANEL, 0.7)).text_color(c(ui::MUTED))
                        }
                    })
                    .child(if done {
                        "✓".to_string()
                    } else {
                        format!("{}", i + 1)
                    })
                    .child(name)
                    .on_click(cx.listener(move |this, _, _, cx| this.setup_step(i, cx))),
            );
        }
        let back = (step > 0).then(|| {
            ui::ghost("setup-back", "Back")
                .on_click(cx.listener(move |this, _, _, cx| this.setup_step(step - 1, cx)))
        });
        let next_ok = step != 2 || !self.setup_crew.is_empty();
        let next: Option<Stateful<Div>> = (step < STEPS.len() - 1).then(|| {
            ui::solid("setup-next", "Continue")
                .when(!next_ok, |d| d.opacity(0.4))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if step == 2 && this.setup_crew.is_empty() {
                        this.say("Pick at least one bot.");
                        cx.notify();
                        return;
                    }
                    this.setup_step(step + 1, cx)
                }))
        });
        div()
            .id("welcome")
            .size_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .items_center()
            .px(px(40.))
            .pt(px(if cfg!(target_os = "macos") { 48. } else { 32. }))
            .pb(px(40.))
            .child(
                div()
                    .w_full()
                    .max_w(px(860.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(dots)
                    .child(
                        div()
                            .id("setup-skip")
                            .text_size(px(12.5))
                            .text_color(c(ui::MUTED))
                            .cursor_pointer()
                            .hover(|s| s.text_color(c(ui::INK)))
                            .child("Skip setup")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.finish_setup(cx);
                                this.go(Place::Today, cx);
                            })),
                    ),
            )
            .child(ui::rise(
                div().w_full().max_w(px(860.)).mt(px(28.)).child(body),
                self.stagger(0),
            ))
            .child(
                div()
                    .w_full()
                    .max_w(px(860.))
                    .mt(px(26.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .children(back)
                    .child(div().flex_1())
                    .children(next),
            )
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
                    .border_color(c(if ok { 0x134733 } else { ui::LINE }))
                    .bg(c(if ok { 0x0F251F } else { ui::PANEL }))
                    .text_size(px(13.))
                    .text_color(c(if ok { ui::INK } else { ui::FAINT }))
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
            "Welcome to Runo",
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
        let current = self.store.current;
        for (i, f) in self.store.folders.iter().enumerate().take(5) {
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
                    .border_color(c(if on { ui::INK } else { ui::LINE }))
                    .bg(c(ui::PANEL))
                    .cursor_pointer()
                    .child(
                        Icon::new(IconName::Folder)
                            .size(px(15.))
                            .text_color(c(ui::MUTED)),
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
                                    .text_color(c(ui::FAINT))
                                    .truncate()
                                    .child(f.path.clone()),
                            ),
                    )
                    .when(on, |d| d.child(div().text_color(c(ui::GREEN)).child("✓")))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.store.current = i;
                        this.store.save();
                        cx.notify();
                    })),
            );
        }
        let words = if self.store.folders.is_empty() {
            "Pick the project the crew should work in. You can add more folders later, or drop one onto the window."
        } else {
            "The crew works in one project folder at a time. Pick one here, or add another. You can switch any time from Today."
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

    fn welcome_crew(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut grid = div().flex().flex_wrap().gap(px(12.));
        for who in Member::CREW {
            let on = self.setup_crew.contains(&who);
            let face = self.bot(
                who,
                if on { Mood::Idle } else { Mood::Asleep },
                76.,
                Framing::Full,
            );
            grid = grid.child(
                div()
                    .id(("setup-bot", who.key() as usize))
                    .w(px(270.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .pl(px(6.))
                    .pr(px(14.))
                    .py(px(8.))
                    .rounded(px(18.))
                    .border_2()
                    .border_color(c(if on { who.accent() } else { ui::LINE }))
                    .bg(if on {
                        ca(who.accent(), 0.07)
                    } else {
                        ca(ui::PANEL, 0.8)
                    })
                    .cursor_pointer()
                    .on_hover(cx.listener(move |this, h: &bool, _, cx| this.hover_bot(who, *h, cx)))
                    .child(face)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(15.))
                                    .font_weight(FontWeight::BOLD)
                                    .child(who.name()),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .line_height(px(16.))
                                    .text_color(c(ui::MUTED))
                                    .child(who.pitch()),
                            ),
                    )
                    .child(
                        div()
                            .flex_none()
                            .size(px(22.))
                            .rounded_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(12.))
                            .text_color(c(ui::ON_FILL))
                            .map(|d| {
                                if on {
                                    d.bg(c(who.accent())).child("✓")
                                } else {
                                    d.border_1().border_color(c(ui::LINE))
                                }
                            }),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(i) = this.setup_crew.iter().position(|m| *m == who) {
                            this.setup_crew.remove(i);
                        } else {
                            this.setup_crew.push(who);
                        }
                        cx.notify();
                    })),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap(px(18.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(
                        div()
                            .text_size(px(30.))
                            .font_weight(FontWeight::BOLD)
                            .child("Choose your crew"),
                    )
                    .child(
                        div()
                            .text_size(px(14.))
                            .text_color(c(ui::MUTED))
                            .child(format!(
                                "Start with the bots you need. {} picked; add or remove any of them later on Crew, or make your own.",
                                self.setup_crew.len()
                            )),
                    ),
            )
            .child(grid)
    }

    fn welcome_first(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        // The first question goes to a bot that answers questions, if one is on the crew.
        let asker = if self.setup_crew.contains(&Member::Explainer) {
            Member::Explainer
        } else {
            self.setup_crew
                .first()
                .copied()
                .unwrap_or(Member::Explainer)
        };
        let hero = self.bot(asker, Mood::Idle, 150., Framing::Full);
        let has_folder = self.store.folder().is_some();
        card_with(
            hero,
            "You're set",
            if has_folder {
                "Try one safe question first. It only reads the code, changes nothing, and shows you how the crew works."
            } else {
                "Add a folder whenever you're ready. Until then you can look around."
            },
            div()
                .flex()
                .gap(px(10.))
                .when(has_folder, |d| {
                    d.child(
                        ui::solid(
                            "setup-ask",
                            format!("Ask {}: \u{201c}{FIRST_QUESTION}\u{201d}", asker.name()),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.finish_setup(cx);
                            this.go(Place::Chat(asker), cx);
                            this.ask(asker, FIRST_QUESTION.to_string(), cx);
                        })),
                    )
                })
                .child(ui::ghost("setup-done", "Go to Today").on_click(cx.listener(
                    |this, _, _, cx| {
                        this.finish_setup(cx);
                        this.go(Place::Today, cx);
                    },
                ))),
        )
    }
}

/// A step: a bot, a title, a line of words, and what to do.
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
                        .text_color(c(ui::BODY))
                        .child(words),
                )
                .child(div().mt(px(8.)).child(body)),
        )
}
