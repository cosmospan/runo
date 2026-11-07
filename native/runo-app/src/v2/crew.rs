//! Crew: every bot, what it is for, what it can do and what it has done. Make a
//! bot and each bot's Setup open the editor in `builder`.

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use runo_holo::{Framing, Mood};

use super::data::{Member, Mode};
use super::store::Status;
use super::ui::{self, c, ca};
use super::Pane;
use super::{Place, Shell};
use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;

impl Shell {
    pub(super) fn crew(&mut self, cx: &mut Context<Self>) -> AnyElement {
        if self.maker_open {
            let page = div()
                .w_full()
                .max_w(px(1160.))
                .px(px(28.))
                .pt(px(24.))
                .pb(px(40. + super::DOCK_ROOM))
                .child(self.maker_page(cx));
            return div()
                .id("crew-maker")
                .size_full()
                .overflow_y_scroll()
                .child(page)
                .into_any_element();
        }
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
                        ui::solid_i("crew-make", IconName::Plus, "Make a bot").on_click(cx.listener(|this, _, window, cx| {
                            this.open_maker(None, None, window, cx);
                        })),
                    ),
            )
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
                .px(px(8.))
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
        // What it can do: the words jobs come to it by and its skill files, and
        // its connected apps and permissions.
        let setup = super::setup::of(who);
        let mut skills: Vec<String> = who
            .skills()
            .split(' ')
            .filter(|w| !w.is_empty())
            .map(str::to_string)
            .collect();
        skills.extend(setup.skill_files.iter().map(|s| s.name.clone()));
        let mut apps: Vec<String> = setup
            .mcp
            .iter()
            .map(|k| k.rsplit('/').next().unwrap_or(k).to_string())
            .collect();
        apps.extend(setup.servers.iter().map(|s| s.name.clone()));
        let kind = who.mode().label().to_lowercase();
        let skills_row = can_row(
            "SKILLS",
            skills,
            format!("Takes {kind} jobs \u{00b7} no skill files yet"),
            None,
        );
        let tools_row = can_row(
            "TOOLS",
            apps,
            "No connected apps".into(),
            Some(format!("\u{00b7} {}", setup.perms_line())),
        );
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
                    .flex_col()
                    .gap(px(7.))
                    .child(skills_row)
                    .child(tools_row),
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
                    .child(small(("crew-setup", key), "Setup").on_click(
                        cx.listener(move |this, _, window, cx| this.open_setup(who, window, cx)),
                    ))
                    .child(
                        // A pin, not a word, so the row fits Setup and Give a job.
                        ui::icon_btn(("crew-pin", key), IconName::Pin)
                            .size(px(28.))
                            .when(pinned, |d| d.text_color(c(ui::ink())))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(i) =
                                    this.crew.store.pinned.iter().position(|m| *m == who)
                                {
                                    this.crew.store.pinned.remove(i);
                                } else {
                                    this.crew.store.pinned.push(who);
                                }
                                this.crew.store.save();
                                cx.notify();
                            })),
                    )
                    // Fin leads every crew, so it stays.
                    .when(who != Member::Fin, |d| {
                        d.child(
                            ui::icon_btn(("crew-remove", key), IconName::Trash)
                                .size(px(28.))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if who.is_made() {
                                        this.remove_made(who, cx)
                                    } else {
                                        this.set_on_crew(who, false, cx)
                                    }
                                })),
                        )
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
}

/// One line of what a bot can do on its card: a label, up to three chips, and
/// a quiet tail.
fn can_row(label: &'static str, items: Vec<String>, empty: String, tail: Option<String>) -> Div {
    let mut chips = div()
        .flex_1()
        .min_w_0()
        .flex()
        .items_center()
        .gap(px(5.))
        .overflow_hidden();
    if items.is_empty() {
        chips = chips.child(
            div()
                .text_size(px(12.))
                .text_color(c(ui::faint()))
                .truncate()
                .child(empty),
        );
    }
    let more = items.len().saturating_sub(3);
    for t in items.into_iter().take(3) {
        chips = chips.child(
            div()
                .flex_none()
                .h(px(22.))
                .px(px(8.))
                .flex()
                .items_center()
                .rounded(px(6.))
                .bg(c(ui::well()))
                .border_1()
                .border_color(c(ui::line2()))
                .text_size(px(11.5))
                .text_color(c(ui::body()))
                .child(t),
        );
    }
    if more > 0 {
        chips = chips.child(
            div()
                .flex_none()
                .text_size(px(11.5))
                .text_color(c(ui::faint()))
                .child(format!("+{more}")),
        );
    }
    if let Some(t) = tail {
        chips = chips.child(
            div()
                .min_w_0()
                .text_size(px(11.5))
                .text_color(c(ui::faint()))
                .truncate()
                .child(t),
        );
    }
    div()
        .flex()
        .items_center()
        .gap(px(10.))
        .child(div().w(px(46.)).flex_none().child(ui::caption(label)))
        .child(chips)
}
