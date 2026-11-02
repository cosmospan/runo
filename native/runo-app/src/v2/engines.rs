//! Engines: which coding agent and model each bot runs on, and the models Runo
//! itself thinks with. Choices are saved to Runo's settings straight away.

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use runo_holo::Mood;

use crate::agent;
use crate::agent_pick::AgentPick;
use crate::settings::{AppSettings, BotEngine};

use super::data::Member;
use super::engine::{self, vendor};
use super::ui::{self, c, ca, space};
use super::Shell;

/// The bots whose engine can be picked: the six from Runo.
const BOTS: [Member; 6] = [
    Member::Builder,
    Member::Checker,
    Member::Guard,
    Member::Fixer,
    Member::Explainer,
    Member::Keeper,
];

fn save(cx: &mut App, f: impl FnOnce(&mut AppSettings)) {
    cx.update_global::<AppSettings, _>(|st, _| {
        f(st);
        st.save();
    });
}

/// A small chip for one choice; `ok` false greys it as not installed.
fn choice(
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
                d.bg(c(ui::INK))
                    .text_color(c(0xFFFFFF))
                    .font_weight(FontWeight::MEDIUM)
            } else {
                d.border_1()
                    .border_color(c(ui::LINE))
                    .bg(c(0xFFFFFF))
                    .text_color(c(if ok { ui::INK } else { ui::FAINT }))
                    .hover(|s| s.bg(c(0xF3F4F9)))
            }
        })
        .child(label.into())
}

fn section(title: &'static str, line: &'static str) -> Div {
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
                .text_color(c(ui::MUTED))
                .child(line),
        )
}

impl Shell {
    pub(super) fn engines(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let st = cx.global::<AppSettings>().clone();
        let ready: Vec<usize> = (0..agent::PROVIDERS.len())
            .filter(|&i| engine::ready(&st, i))
            .collect();
        let default_i = engine::index_of(&st, "");

        // Every agent Runo knows, and which are on this computer.
        let mut tiles = div()
            .mt(px(space::M))
            .grid()
            .grid_cols(4)
            .gap(px(space::SM));
        for (i, p) in agent::PROVIDERS.iter().enumerate() {
            let ok = ready.contains(&i);
            let on = i == default_i;
            let id = p.id;
            tiles = tiles.child(
                div()
                    .id(("agent-tile", i))
                    .h(px(64.))
                    .px(px(14.))
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .rounded(px(14.))
                    .bg(c(0xFFFFFF))
                    .border_1()
                    .map(|d| {
                        if on {
                            d.border_2().border_color(c(ui::INK))
                        } else {
                            d.border_color(c(ui::LINE))
                        }
                    })
                    .when(!ok, |d| d.opacity(0.55))
                    .cursor_pointer()
                    .hover(|s| s.border_color(c(0xC9CBD8)))
                    .child(
                        div()
                            .size(px(32.))
                            .flex_none()
                            .rounded(px(10.))
                            .bg(ca(p.color, 0.14))
                            .text_color(c(p.color))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(15.))
                            .font_weight(FontWeight::BOLD)
                            .child(p.mark),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .truncate()
                                    .child(p.name),
                            )
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .text_color(c(ui::MUTED))
                                    .child(vendor(p.id)),
                            ),
                    )
                    .child(if on {
                        div()
                            .px(px(8.))
                            .py(px(2.))
                            .rounded_full()
                            .bg(c(ui::INK))
                            .text_color(c(0xFFFFFF))
                            .text_size(px(10.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Default")
                    } else if ok {
                        ui::status(ui::GREEN, "Ready")
                    } else {
                        div()
                            .text_size(px(11.5))
                            .text_color(c(ui::FAINT))
                            .child("Not installed")
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if ok {
                            save(cx, |st| st.default_provider = id.to_string());
                            this.say(format!(
                                "New jobs now run on {} unless a bot has its own engine.",
                                agent::provider(id).name
                            ));
                        } else {
                            this.say(format!(
                                "{} isn't installed. Install its CLI, then it shows as Ready.",
                                agent::provider(id).name
                            ));
                        }
                        cx.notify();
                    })),
            );
        }

        // Each bot's engine and model.
        let mut rows = div().mt(px(space::M)).flex().flex_col();
        for (n, who) in BOTS.into_iter().enumerate() {
            let own = st
                .bot_engines
                .get(&engine::bot_key(who))
                .cloned()
                .unwrap_or_default();
            let (i, model) = engine::engine(&st, who);
            let face = self.bot(who, Mood::Idle, 40., runo_holo::Framing::Bust);
            let mut agents = div().flex().flex_wrap().gap(px(6.));
            agents = agents.child(
                choice(
                    ("bot-def", n),
                    format!("Default · {}", agent::PROVIDERS[default_i].name),
                    own.agent.is_empty(),
                    true,
                )
                .on_click(cx.listener(move |_, _, _, cx| {
                    save(cx, |st| engine::set(st, who, BotEngine::default()));
                    cx.notify();
                })),
            );
            for &j in &ready {
                let p = &agent::PROVIDERS[j];
                let id = p.id;
                agents = agents.child(
                    choice(("bot-agent", n * 100 + j), p.name, own.agent == id, true).on_click(
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
            }
            let mut models = div().flex().flex_wrap().gap(px(6.));
            for (k, m) in AgentPick::models(i).into_iter().take(7).enumerate() {
                let value = m.value.clone();
                let agent_id = agent::PROVIDERS[i].id.to_string();
                models = models.child(
                    choice(
                        ("bot-model", n * 100 + k),
                        m.label.clone(),
                        model == m.value,
                        true,
                    )
                    .on_click(cx.listener(move |_, _, _, cx| {
                        let (agent_id, value) = (agent_id.clone(), value.clone());
                        save(cx, |st| {
                            engine::set(
                                st,
                                who,
                                BotEngine {
                                    agent: agent_id,
                                    model: value,
                                },
                            )
                        });
                        cx.notify();
                    })),
                );
            }
            rows = rows.child(
                div()
                    .flex()
                    .gap(px(space::M))
                    .py(px(space::M))
                    .when(n > 0, |d| d.border_t_1().border_color(c(ui::LINE)))
                    .child(
                        div()
                            .w(px(200.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .child(face)
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .child(
                                        div()
                                            .text_size(px(14.))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(who.name()),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.))
                                            .text_color(c(ui::MUTED))
                                            .child(who.role()),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .child(agents)
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .child(ui::caption("MODEL"))
                                    .child(models),
                            ),
                    ),
            );
        }

        // Runo's own thinking: shaping jobs, and a second opinion.
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
        let work_vendor = vendor(agent::PROVIDERS[default_i].id);
        let same_vendor =
            !st.challenge_agent.is_empty() && vendor(&st.challenge_agent) == work_vendor;
        let brain = ui::card()
            .p(px(space::L))
            .flex()
            .flex_col()
            .gap(px(space::M))
            .child(section("Runo's own thinking", "The model calls Runo makes itself, apart from the coding agents doing the work."))
            .child(
                div()
                    .flex()
                    .gap(px(space::M))
                    .child(div().w(px(200.)).flex_none().text_size(px(13.5)).font_weight(FontWeight::MEDIUM).child("Shapes jobs, writes the one question"))
                    .child(div().flex_1().flex().flex_col().gap(px(8.)).child(brain_agents).child(div().flex().items_center().gap(px(8.)).child(ui::caption("MODEL")).child(brain_models))),
            )
            .child(div().h(px(1.)).bg(c(ui::LINE)))
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
                            .child(div().text_size(px(12.)).text_color(c(if same_vendor { ui::AMBER } else { ui::MUTED })).child(if same_vendor {
                                format!("Same company as the work ({work_vendor}). One from another company catches more.")
                            } else {
                                "One vendor writes, another checks. Pick an agent from a different company than the default.".to_string()
                            })),
                    ),
            );

        let none_ready = ready.is_empty();
        div()
            .id("engines")
            .size_full()
            .overflow_y_scroll()
            .px(px(space::XL))
            .py(px(space::XL))
            .flex()
            .flex_col()
            .gap(px(space::L))
            .child(
                div()
                    .child(ui::caption("ENGINES"))
                    .child(div().mt(px(6.)).text_size(px(34.)).font_weight(FontWeight::BOLD).child("What runs your crew"))
                    .child(
                        div()
                            .mt(px(6.))
                            .text_size(px(14.))
                            .text_color(c(ui::MUTED))
                            .child("Every bot works through a coding agent you already use. Give each its own, or let them all follow the default."),
                    ),
            )
            .child(
                ui::card()
                    .p(px(space::L))
                    .child(section("Agents on this computer", "Tap one that's ready to make it the default."))
                    .when(none_ready, |d| {
                        d.child(
                            div()
                                .mt(px(space::S))
                                .text_size(px(13.))
                                .text_color(c(ui::AMBER))
                                .child("None of the agents' CLIs were found. Install one (for example Claude Code or Codex) and reopen Runo."),
                        )
                    })
                    .child(tiles),
            )
            .child(ui::card().p(px(space::L)).child(section("Each bot", "Which agent and model a bot works with. Changes apply to its next job.")).child(rows))
            .child(brain)
            .into_any_element()
    }
}
