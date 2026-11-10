//! The first-run walkthrough: check which coding agents are installed and pick
//! a folder. Two short steps, both skippable, shown once. The crew starts with
//! just Fin, who leads it: Fin answers questions and offers the other bots as
//! jobs need them, and any bot can be added or made on Crew. What to do first
//! is in #general, which opens next.

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
        // The steps as a row of chips, each with where it stands.
        let mut steps = div().flex().items_center().justify_center().gap(px(8.));
        for (i, name) in STEPS.into_iter().enumerate() {
            let done = i < step;
            let on = i == step;
            if i > 0 {
                steps = steps.child(
                    Icon::new(IconName::ChevronRight)
                        .size(px(13.))
                        .text_color(c(ui::faint())),
                );
            }
            steps = steps.child(
                div()
                    .id(("setup-dot", i))
                    .h(px(26.))
                    .px(px(11.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .rounded_full()
                    .cursor_pointer()
                    .text_size(px(12.5))
                    .font_weight(FontWeight::MEDIUM)
                    .map(|d| {
                        if on {
                            d.bg(c(ui::fill())).text_color(c(ui::on_fill()))
                        } else if done {
                            d.bg(ca(ui::GREEN, 0.14)).text_color(c(ui::t_green()))
                        } else {
                            d.bg(c(ui::well())).text_color(c(ui::muted()))
                        }
                    })
                    .tooltip({
                        let note = notes[i].clone();
                        move |window, cx| {
                            gpui_kit::component::tooltip::Tooltip::new(note.clone())
                                .build(window, cx)
                        }
                    })
                    .child(if done {
                        format!("\u{2713} {name}")
                    } else {
                        format!("{} {name}", i + 1)
                    })
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
        let skip = div()
            .id("setup-skip")
            .px(px(8.))
            .text_size(px(13.))
            .text_color(c(ui::muted()))
            .cursor_pointer()
            .hover(|s| s.text_color(c(ui::ink())))
            .child("Skip")
            .tooltip(|window, cx| {
                gpui_kit::component::tooltip::Tooltip::new("Skip setup and go to #general")
                    .build(window, cx)
            })
            .on_click(cx.listener(|this, _, _, cx| {
                this.finish_setup(cx);
                this.go(Place::Today, cx);
            }));
        let mark = self.bot(Member::Fin, Mood::Signature, 84., Framing::Bust);
        let head = div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(10.))
            .child(mark)
            .child(
                div()
                    .mt(px(6.))
                    .text_size(px(32.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(c(ui::ink()))
                    .child("Welcome to Squad Code"),
            )
            .child(
                div()
                    .text_size(px(15.))
                    .text_color(c(ui::muted()))
                    .child("Your coding agents, as a crew you talk to in threads."),
            )
            .child(div().mt(px(10.)).child(steps));
        div()
            .id("welcome")
            .size_full()
            .overflow_y_scroll()
            .child(
                div()
                    .min_h_full()
                    .flex()
                    .flex_col()
                    .justify_center()
                    .items_center()
                    .gap(px(26.))
                    .px(px(40.))
                    .py(px(48.))
                    .child(head)
                    .child(ui::rise(
                        div().w_full().max_w(px(680.)).child(body),
                        self.stagger(0),
                    ))
                    .child(
                        div()
                            .w_full()
                            .max_w(px(680.))
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .child(skip)
                            .children(back)
                            .child(div().flex_1())
                            .child(next),
                    ),
            )
            .into_any_element()
    }

    fn welcome_engines(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let st = cx.global::<crate::settings::AppSettings>().clone();
        let ready: Vec<usize> = (0..crate::agent::PROVIDERS.len())
            .filter(|i| super::engine::ready(&st, *i))
            .collect();
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
                        // Missing: one click runs its install inside Squad.
                        match super::engines::install_cmd(p.id) {
                            Some(cmd) => d
                                .cursor_pointer()
                                .hover(|s| s.border_color(c(ui::faint())).text_color(c(ui::ink())))
                                .child(div().text_size(px(11.)).child("not found \u{b7} install"))
                                .on_click(
                                    cx.listener(move |this, _, _, cx| this.run_setup(&cmd, cx)),
                                ),
                            None => d.child(div().text_size(px(11.)).child("not found")),
                        }
                    }),
            );
        }
        let words = if ready.is_empty() {
            "The crew works through a coding agent on this computer, and none was found. Install Claude Code, Codex or Gemini CLI and sign in, then check again."
        } else {
            "The crew works through the coding agents on this computer. These are ready; you can pick which one each bot uses later, in Engines."
        };
        card_with(
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
            "The crew works in one project folder at a time. Pick one here, or add another. You can add or switch projects any time from the rail on the left."
        };
        card_with(
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

fn card_with(title: &'static str, words: &'static str, body: impl IntoElement) -> Div {
    ui::card()
        .p(px(26.))
        .flex()
        .flex_col()
        .gap(px(10.))
        .child(
            div()
                .text_size(px(18.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(c(ui::ink()))
                .child(title),
        )
        .child(
            div()
                .text_size(px(13.5))
                .line_height(px(20.))
                .text_color(c(ui::body()))
                .child(words),
        )
        .child(div().mt(px(8.)).child(body))
}
