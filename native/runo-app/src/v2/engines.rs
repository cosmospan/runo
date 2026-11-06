//! Engines: which coding agent and model each bot runs on, and the models Squad Code
//! itself thinks with. Choices are saved to Squad Code's settings straight away.

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::agent;
use crate::agent_pick::AgentPick;
use crate::settings::{AppSettings, BotEngine};

use super::data::Member;
use super::engine::{self, vendor};
use super::ui::{self, c, ca, space};
use super::Shell;

fn save(cx: &mut App, f: impl FnOnce(&mut AppSettings)) {
    cx.update_global::<AppSettings, _>(|st, _| {
        f(st);
        st.save();
    });
}

/// A small chip for one choice; `ok` false greys it as not installed.
pub(super) fn choice(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    on: bool,
    ok: bool,
) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(28.))
        .px(px(11.))
        .flex()
        .items_center()
        .gap(px(6.))
        .rounded_full()
        .text_size(px(12.5))
        .cursor_pointer()
        .map(|d| {
            if on {
                d.bg(c(ui::fill()))
                    .text_color(c(ui::on_fill()))
                    .font_weight(FontWeight::MEDIUM)
            } else {
                d.border_1()
                    .border_color(c(ui::line()))
                    .bg(c(ui::panel()))
                    .text_color(c(if ok { ui::ink() } else { ui::faint() }))
                    .hover(|s| s.bg(c(ui::hover())))
            }
        })
        .child(label.into())
}

pub(super) fn section(title: &'static str, line: &'static str) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(4.))
        .child(
            div()
                .text_size(px(17.))
                .font_weight(FontWeight::BOLD)
                .child(title),
        )
        .child(
            div()
                .text_size(px(13.))
                .text_color(c(ui::muted()))
                .child(line),
        )
}

impl Shell {
    /// Smart Peek: off until the person turns it on, which downloads Squad's
    /// own model once. Squad then runs it only when the rules aren't sure.
    fn smart_peek(&mut self, st: &AppSettings, cx: &mut Context<Self>) -> Div {
        use super::brain::helper::{self, Download};
        let have = helper::model_path().is_file();
        let ours = helper::is_ours(&st.brain_model) && st.brain_url.trim().is_empty();
        let other = !st.brain_model.trim().is_empty() && !ours;
        let state = helper::download_state();
        let downloading = matches!(state, Download::Running { .. });
        let mut chips = div()
            .flex()
            .flex_wrap()
            .gap(px(6.))
            .child(
                choice("peek-off", "Off", st.brain_model.trim().is_empty(), true).on_click(
                    cx.listener(|_, _, _, cx| {
                        helper::stop();
                        save(cx, |st| st.brain_model.clear());
                        cx.notify();
                    }),
                ),
            )
            .child(
                choice("peek-on", "On \u{b7} Squad's own model", ours, true).on_click(cx.listener(
                    |_, _, _, cx| {
                        let url = cx.global::<AppSettings>().brain_model_url.clone();
                        helper::download(&url, false);
                        save(cx, |st| {
                            st.brain_model = helper::MODEL.to_string();
                            st.brain_url.clear();
                        });
                        cx.notify();
                    },
                )),
            );
        if other {
            chips = chips.child(choice(
                "peek-other",
                format!("On \u{b7} {}", st.brain_model.trim()),
                true,
                true,
            ));
        }
        if have && ours && !downloading {
            chips = chips.child(
                choice("peek-update", "Check for a newer model", false, true).on_click(
                    cx.listener(|_, _, _, cx| {
                        let url = cx.global::<AppSettings>().brain_model_url.clone();
                        helper::download(&url, true);
                        cx.notify();
                    }),
                ),
            );
        }
        if have && !ours && !downloading {
            chips = chips.child(
                choice("peek-delete", "Delete the model (505 MB)", false, true).on_click(
                    cx.listener(|_, _, _, cx| {
                        helper::remove();
                        cx.notify();
                    }),
                ),
            );
        }
        let mb = |b: u64| b / 1_000_000;
        let (line, warn) = match state {
            Download::Running { got, total } if ours => {
                (format!("Downloading Squad's model: {} of {} MB.", mb(got), mb(total)), false)
            }
            Download::Failed(e) if ours && !have => (format!("The download didn't finish: {e}. Turn it on again to retry."), true),
            Download::Failed(e) if ours => (format!("Couldn't update: {e}. The model you have keeps working."), true),
            _ if other => (format!("Asking {} at {}. You run that server yourself.", st.brain_model.trim(), if st.brain_url.trim().is_empty() { super::brain::model::DEFAULT_URL } else { st.brain_url.trim() }), false),
            _ if ours && have && helper::server_bin().is_none() => (
                "Needs llama-server from llama.cpp (on a Mac: brew install llama.cpp). Until then the rules decide.".to_string(),
                true,
            ),
            _ if ours && have => (format!("On{}. A small model on this computer picks the bot when the rules aren't sure, in about half a second. It stops after {} idle minutes.", helper::installed().map(|r| format!(", Peek v{}", r.version)).unwrap_or_default(), helper::IDLE_MINUTES), false),
            _ => ("Off: the rules pick the bot. Turning it on downloads Squad's own model once (505 MB); it runs on this computer, only when the rules aren't sure.".to_string(), false),
        };
        div()
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(chips)
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(c(if warn { ui::AMBER } else { ui::muted() }))
                    .child(line),
            )
    }

    pub(super) fn engines(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let st = cx.global::<AppSettings>().clone();
        let ready: Vec<usize> = (0..agent::PROVIDERS.len())
            .filter(|&i| engine::ready(&st, i))
            .collect();
        let default_i = engine::index_of(&st, "");

        let sel = self
            .engine_sel
            .filter(|i| *i < agent::PROVIDERS.len())
            .unwrap_or(default_i);
        let list = self.engine_list(&st, &ready, default_i, sel, cx);
        let detail = self.engine_detail(&st, &ready, default_i, sel, cx);

        // Squad Code's own thinking: shaping jobs, and a second opinion.
        let brain_i = crate::assistant::assistant_provider(&st);
        let mut brain_agents = div().flex().flex_wrap().gap(px(6.));
        for &j in &ready {
            let id = agent::PROVIDERS[j].id;
            brain_agents = brain_agents.child(
                choice(("brain", j), agent::PROVIDERS[j].name, j == brain_i, true).on_click(
                    cx.listener(move |_, _, _, cx| {
                        save(cx, |st| {
                            if st.assistant_agent != id {
                                st.assistant_model.clear();
                            }
                            st.assistant_agent = id.to_string();
                        });
                        cx.notify();
                    }),
                ),
            );
        }
        let mut brain_models = div().flex().flex_wrap().gap(px(6.));
        for (k, m) in AgentPick::models(brain_i).into_iter().take(7).enumerate() {
            let value = m.value.clone();
            let label = if m.value.is_empty() && agent::PROVIDERS[brain_i].id == "claude" {
                "Fast default · Haiku".to_string()
            } else {
                m.label.clone()
            };
            brain_models = brain_models.child(
                choice(
                    ("brain-model", k),
                    label,
                    st.assistant_model == m.value,
                    true,
                )
                .on_click(cx.listener(move |_, _, _, cx| {
                    let value = value.clone();
                    save(cx, |st| st.assistant_model = value);
                    cx.notify();
                })),
            );
        }
        let mut challengers = div().flex().flex_wrap().gap(px(6.));
        challengers = challengers.child(
            choice("challenge-off", "Off", st.challenge_agent.is_empty(), true).on_click(
                cx.listener(|_, _, _, cx| {
                    save(cx, |st| st.challenge_agent.clear());
                    cx.notify();
                }),
            ),
        );
        for &j in &ready {
            let id = agent::PROVIDERS[j].id;
            challengers = challengers.child(
                choice(
                    ("challenge", j),
                    agent::PROVIDERS[j].name,
                    st.challenge_agent == id,
                    true,
                )
                .on_click(cx.listener(move |_, _, _, cx| {
                    save(cx, |st| st.challenge_agent = id.to_string());
                    cx.notify();
                })),
            );
        }
        let smart = self.smart_peek(&st, cx);
        let work_vendor = vendor(agent::PROVIDERS[default_i].id);
        let same_vendor =
            !st.challenge_agent.is_empty() && vendor(&st.challenge_agent) == work_vendor;
        let brain = ui::card()
            .p(px(space::L))
            .flex()
            .flex_col()
            .gap(px(space::M))
            .child(section("Squad Code's own thinking", "The model calls Squad Code makes itself, apart from the coding agents doing the work."))
            .child(
                div()
                    .flex()
                    .gap(px(space::M))
                    .child(div().w(px(200.)).flex_none().text_size(px(13.5)).font_weight(FontWeight::MEDIUM).child("Shapes jobs, writes the one question"))
                    .child(div().flex_1().flex().flex_col().gap(px(8.)).child(brain_agents).child(div().flex().items_center().gap(px(8.)).child(ui::caption("MODEL")).child(brain_models))),
            )
            .child(div().h(px(1.)).bg(c(ui::line())))
            .child(
                div()
                    .flex()
                    .gap(px(space::M))
                    .child(div().w(px(200.)).flex_none().text_size(px(13.5)).font_weight(FontWeight::MEDIUM).child("Double-checks decisions"))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .child(challengers)
                            .child(div().text_size(px(12.)).text_color(c(if same_vendor { ui::AMBER } else { ui::muted() })).child(if same_vendor {
                                format!("Same company as the work ({work_vendor}). One from another company catches more.")
                            } else {
                                "One vendor writes, another checks. Pick an agent from a different company than the default.".to_string()
                            })),
                    ),
            )
            .child(div().h(px(1.)).bg(c(ui::line())))
            .child(
                div()
                    .flex()
                    .gap(px(space::M))
                    .child(div().w(px(200.)).flex_none().text_size(px(13.5)).font_weight(FontWeight::MEDIUM).child("Smart Peek"))
                    .child(smart),
            );

        let none_ready = ready.is_empty();
        div()
            .id("engines")
            .size_full()
            .overflow_y_scroll()
            .child(
                div()
                    .w_full()
                    .max_w(px(1160.))
                    .px(px(28.))
                    .pt(px(24.))
                    .pb(px(space::XL + super::DOCK_ROOM))
                    .flex()
                    .flex_col()
                    .gap(px(space::L))
                    .child(
                        div()
                            .child(div().text_size(px(24.)).font_weight(FontWeight::SEMIBOLD).child("Engines"))
                            .child(
                                div()
                                    .mt(px(6.))
                                    .text_size(px(13.5))
                                    .text_color(c(ui::muted()))
                                    .child("Every bot works through a coding agent you already use. Pick one to see which bots run on it."),
                            ),
                    )
                    .when(none_ready, |d| {
                        d.child(
                            div()
                                .text_size(px(13.))
                                .text_color(c(ui::t_amber()))
                                .child("None of the agents' command-line tools were found. Install one (for example Claude Code or Codex) and it shows up here."),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .gap(px(space::M))
                            .items_start()
                            .child(list)
                            .child(detail),
                    )
                    .child(brain),
            )
            .into_any_element()
    }

    /// Every agent Squad Code knows: the ones on this computer first.
    fn engine_list(
        &mut self,
        st: &AppSettings,
        ready: &[usize],
        default_i: usize,
        sel: usize,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut list = ui::card()
            .w(px(300.))
            .flex_none()
            .overflow_hidden()
            .flex()
            .flex_col();
        let missing: Vec<usize> = (0..agent::PROVIDERS.len())
            .filter(|i| !ready.contains(i))
            .collect();
        for (title, group) in [
            ("On this computer", ready.to_vec()),
            ("Not installed", missing),
        ] {
            if group.is_empty() {
                continue;
            }
            list = list.child(
                div()
                    .h(px(32.))
                    .px(px(14.))
                    .flex()
                    .items_center()
                    .bg(c(ui::raised()))
                    .border_b_1()
                    .border_color(c(ui::line2()))
                    .text_size(px(11.5))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(c(ui::muted()))
                    .child(title),
            );
            for i in group {
                let p = &agent::PROVIDERS[i];
                let ok = ready.contains(&i);
                let bots = Member::all()
                    .into_iter()
                    .filter(|m| engine::engine(st, *m).0 == i)
                    .count();
                list = list.child(
                    div()
                        .id(("engine-row", i))
                        .h(px(48.))
                        .px(px(14.))
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .border_b_1()
                        .border_color(c(ui::line2()))
                        .cursor_pointer()
                        .map(|d| {
                            if i == sel {
                                d.bg(c(ui::hover()))
                            } else {
                                d.hover(|s| s.bg(c(ui::hover())))
                            }
                        })
                        .child(
                            div()
                                .size(px(26.))
                                .flex_none()
                                .rounded(px(8.))
                                .bg(ca(p.color, if ok { 0.18 } else { 0.08 }))
                                .text_color(c(if ok { p.color } else { ui::muted() }))
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(12.5))
                                .font_weight(FontWeight::BOLD)
                                .child(p.mark),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .child(
                                    div()
                                        .text_size(px(13.))
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(c(if ok { ui::ink() } else { ui::body() }))
                                        .truncate()
                                        .child(p.name),
                                )
                                .child(div().text_size(px(11.5)).text_color(c(ui::muted())).child(
                                    if ok && bots > 0 {
                                        format!(
                                            "{} \u{b7} {bots} {}",
                                            vendor(p.id),
                                            if bots == 1 { "bot" } else { "bots" }
                                        )
                                    } else {
                                        vendor(p.id).to_string()
                                    },
                                )),
                        )
                        .child(if i == default_i && ok {
                            div()
                                .px(px(7.))
                                .py(px(1.))
                                .rounded_full()
                                .border_1()
                                .border_color(c(ui::line_hi()))
                                .text_size(px(10.5))
                                .text_color(c(ui::ink()))
                                .child("Default")
                        } else if ok {
                            div().size(px(7.)).rounded_full().bg(c(ui::GREEN))
                        } else {
                            div()
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.engine_sel = Some(i);
                            cx.notify();
                        })),
                );
            }
        }
        list
    }

    /// One agent: whether it's here, how to get it, and the bots on it.
    fn engine_detail(
        &mut self,
        st: &AppSettings,
        ready: &[usize],
        default_i: usize,
        sel: usize,
        cx: &mut Context<Self>,
    ) -> Div {
        let p = &agent::PROVIDERS[sel];
        let ok = ready.contains(&sel);
        let id = p.id;
        let spent: f64 = self
            .crew
            .store
            .jobs
            .iter()
            .filter(|j| j.agent == id && super::store::now().saturating_sub(j.started) < 86_400)
            .filter_map(|j| super::home::cost(&j.summary))
            .sum::<f64>()
            + 0.0;
        let jobs = self
            .crew
            .store
            .jobs
            .iter()
            .filter(|j| j.agent == id)
            .count();
        let mut head =
            div()
                .flex()
                .items_center()
                .gap(px(12.))
                .child(
                    div()
                        .size(px(40.))
                        .flex_none()
                        .rounded(px(12.))
                        .bg(ca(p.color, 0.18))
                        .text_color(c(p.color))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(18.))
                        .font_weight(FontWeight::BOLD)
                        .child(p.mark),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .text_size(px(17.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(p.name),
                        )
                        .child(div().text_size(px(12.5)).text_color(c(ui::muted())).child(
                            format!(
                                "{} \u{b7} {} \u{b7} {jobs} jobs \u{b7} ${spent:.2} today",
                                vendor(id),
                                if ok { "ready" } else { "not installed" }
                            ),
                        )),
                );
        if ok && sel != default_i {
            head = head.child(
                ui::solid("engine-default", "Make default").on_click(cx.listener(
                    move |this, _, _, cx| {
                        save(cx, |st| st.default_provider = id.to_string());
                        this.say(format!(
                            "New jobs now run on {} unless a bot has its own engine.",
                            agent::provider(id).name
                        ));
                        cx.notify();
                    },
                )),
            );
        } else if ok {
            head = head.child(ui::status(ui::GREEN, "Default for new jobs"));
        }
        let mut body = ui::card()
            .flex_1()
            .min_w_0()
            .p(px(space::L))
            .flex()
            .flex_col()
            .gap(px(space::M))
            .child(head);
        if !ok {
            let install = crate::registry::spec(id).and_then(|s| {
                s.install
                    .npm
                    .as_ref()
                    .map(|n| format!("npm install -g {n}"))
                    .or_else(|| {
                        if cfg!(windows) {
                            s.install.windows.clone()
                        } else {
                            s.install.unix.clone()
                        }
                    })
            });
            body = body.child(
                div()
                    .text_size(px(13.))
                    .text_color(c(ui::body()))
                    .child(format!("{} isn't on this computer. Install its command-line tool, sign in once, and it shows as ready here.", p.name)),
            );
            if let Some(cmd) = install {
                let copy = cmd.clone();
                body = body.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .px(px(12.))
                                .py(px(8.))
                                .rounded(px(8.))
                                .bg(c(ui::bg()))
                                .border_1()
                                .border_color(c(ui::line()))
                                .font_family(ui::MONO)
                                .text_size(px(12.))
                                .truncate()
                                .child(cmd),
                        )
                        .child(ui::ghost("engine-copy", "Copy").on_click(cx.listener(
                            move |this, _, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(copy.clone()));
                                this.say("Copied. Paste it in a terminal.");
                                cx.notify();
                            },
                        ))),
                );
            }
            return body;
        }
        // The bots on this engine, each with its model; and the rest, one click
        // from moving over.
        let models = AgentPick::models(sel);
        let mut on = div().flex().flex_col();
        let mut off = div().flex().flex_wrap().gap(px(6.));
        let mut any_off = false;
        for (n, who) in Member::all().into_iter().enumerate() {
            let own = st
                .bot_engines
                .get(&engine::bot_key(who))
                .cloned()
                .unwrap_or_default();
            let (i, model) = engine::engine(st, who);
            if i != sel {
                any_off = true;
                off = off.child(
                    choice(("engine-move", n), format!("+ {}", who.name()), false, true).on_click(
                        cx.listener(move |_, _, _, cx| {
                            save(cx, |st| {
                                engine::set(
                                    st,
                                    who,
                                    BotEngine {
                                        agent: id.to_string(),
                                        model: String::new(),
                                    },
                                )
                            });
                            cx.notify();
                        }),
                    ),
                );
                continue;
            }
            let face = self.avatar(who, 26.);
            let open = self.engine_open == Some(who);
            let current = models
                .iter()
                .find(|m| m.value == model)
                .map(|m| m.label.clone())
                .unwrap_or_else(|| {
                    if model.is_empty() {
                        "Its default".into()
                    } else {
                        model.clone()
                    }
                });
            let mut picks = div().flex().flex_wrap().items_center().gap(px(6.));
            if !open {
                picks = picks
                    .child(
                        div()
                            .px(px(10.))
                            .h(px(26.))
                            .flex()
                            .items_center()
                            .rounded(px(7.))
                            .border_1()
                            .border_color(c(ui::line()))
                            .text_size(px(12.))
                            .text_color(c(ui::ink()))
                            .child(current),
                    )
                    .child(
                        div()
                            .id(("engine-change", n))
                            .px(px(6.))
                            .text_size(px(12.))
                            .text_color(c(ui::muted()))
                            .cursor_pointer()
                            .hover(|s| s.text_color(c(ui::ink())))
                            .child("Change model")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.engine_open = Some(who);
                                cx.notify();
                            })),
                    );
            }
            for (k, m) in models.iter().take(if open { 6 } else { 0 }).enumerate() {
                let value = m.value.clone();
                picks = picks.child(
                    ui::pill(
                        ("engine-model", n * 100 + k),
                        m.label.clone(),
                        model == m.value,
                    )
                    .h(px(26.))
                    .text_size(px(12.))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let value = value.clone();
                        save(cx, |st| {
                            engine::set(
                                st,
                                who,
                                BotEngine {
                                    agent: id.to_string(),
                                    model: value,
                                },
                            )
                        });
                        this.engine_open = None;
                        cx.notify();
                    })),
                );
            }
            on = on.child(
                div()
                    .py(px(10.))
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .border_t_1()
                    .border_color(c(ui::line2()))
                    .child(face)
                    .child(
                        div()
                            .w(px(130.))
                            .flex_none()
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(who.name()),
                            )
                            .child(div().text_size(px(11.5)).text_color(c(ui::faint())).child(
                                if own.agent.is_empty() {
                                    "follows the default"
                                } else {
                                    "its own pick"
                                },
                            )),
                    )
                    .child(div().flex_1().min_w_0().child(picks))
                    .when(!own.agent.is_empty(), |d| {
                        d.child(
                            div()
                                .id(("engine-reset", n))
                                .text_size(px(12.))
                                .text_color(c(ui::muted()))
                                .cursor_pointer()
                                .hover(|s| s.text_color(c(ui::ink())))
                                .child("Use default")
                                .on_click(cx.listener(move |_, _, _, cx| {
                                    save(cx, |st| engine::set(st, who, BotEngine::default()));
                                    cx.notify();
                                })),
                        )
                    }),
            );
        }
        body = body.child(ui::caption("BOTS ON THIS ENGINE")).child(on);
        if any_off {
            body = body.child(ui::caption("MOVE A BOT HERE")).child(off);
        }
        body
    }
}
