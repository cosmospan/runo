//! Live: one job, as the bot works. What it says and does on the left, as it
//! happens; its plan, the files it touched and where things stand on the right.

use gpui_kit::assets::IconName;
use gpui_kit::component::input::Input;
use gpui_kit::component::Icon;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use runo_holo::{Framing, Mood};

use super::store::{self, Item, Job, Status};
use super::ui::{self, c, ca};
use super::{Place, Shell};

/// Agent text with the loudest Markdown taken out, so it reads as sentences.
pub(super) fn plain(text: &str) -> String {
    text.lines()
        .map(|l| {
            let l = l.trim_end();
            let l = l.trim_start_matches('#').trim_start();
            l.replace("**", "").replace("__", "")
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn tool_icon(kind: &str) -> IconName {
    match kind {
        "read" | "list" => IconName::Eye,
        "edit" | "write" => IconName::Pencil,
        "delete" => IconName::Trash,
        "shell" => IconName::SquareTerminal,
        "search" | "websearch" => IconName::Search,
        "fetch" => IconName::Globe,
        _ => IconName::Sparkles,
    }
}

impl Shell {
    pub(super) fn live(&mut self, id: u64, cx: &mut Context<Self>) -> AnyElement {
        let Some(job) = self.store.job(id).cloned() else {
            return self.empty_place(
                "That job is gone",
                "It may have been cleared. Everything else is in History.",
                cx,
            );
        };
        div()
            .size_full()
            .flex()
            .child(self.live_main(&job, cx))
            .child(self.live_side(&job, cx))
            .into_any_element()
    }

    fn live_main(&mut self, job: &Job, cx: &mut Context<Self>) -> impl IntoElement {
        let who = job.bot;
        let running = job.status == Status::Running;
        let mood = match job.status {
            Status::Running => Mood::Working,
            Status::Done => {
                if job
                    .ended
                    .is_some_and(|e| store::now().saturating_sub(e) < 6)
                {
                    Mood::Done
                } else {
                    Mood::Signature
                }
            }
            Status::Failed => Mood::NeedsYou,
            _ => Mood::Idle,
        };
        let hero = self.bot(who, mood, 132., Framing::Full);
        let verb = match job.status {
            Status::Running => "is working on",
            Status::Done => "finished",
            Status::Failed => "stopped on",
            Status::Stopped => "was stopped on",
            Status::Interrupted => "was interrupted on",
        };
        let progress = job.progress();
        // The engine's own summary already says how long it took.
        let mut meta = if job.summary.is_empty() || running {
            vec![store::span(job.seconds())]
        } else {
            Vec::new()
        };
        if let Some((d, n)) = progress {
            meta.insert(
                0,
                format!("step {} of {n}", (d + usize::from(running)).min(n)),
            );
        }
        if !job.summary.is_empty() {
            meta.push(job.summary.clone());
        }
        let accent = who.accent();
        let header = div()
            .flex_none()
            .flex()
            .gap(px(14.))
            .px(px(30.))
            .pt(px(if cfg!(target_os = "macos") { 22. } else { 14. }))
            .pb(px(16.))
            .border_b_1()
            .border_color(c(ui::LINE))
            .bg(linear_gradient(
                180.,
                linear_color_stop(ca(accent, 0.10), 0.0),
                linear_color_stop(ca(0xFFFFFF, 0.0), 1.0),
            ))
            .child(div().flex_none().mt(px(-4.)).child(hero))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .pt(px(14.))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .text_size(px(13.))
                            .child(div().font_weight(FontWeight::SEMIBOLD).child(who.name()))
                            .child(div().text_color(c(ui::MUTED)).child(verb))
                            .child(chip(IconName::Cpu, job.engine.clone()))
                            .child(chip(IconName::Folder, job.folder.name.clone())),
                    )
                    .child(
                        div()
                            .mt(px(6.))
                            .text_size(px(28.))
                            .line_height(px(34.))
                            .font_weight(FontWeight::BOLD)
                            .line_clamp(2)
                            .child(job.title.clone()),
                    )
                    .child(
                        div()
                            .mt(px(12.))
                            .flex()
                            .items_center()
                            .gap(px(12.))
                            .child(
                                div().w(px(220.)).child(match (running, progress) {
                                    (true, Some((d, n))) => {
                                        ui::bar((d as f32 + 0.5) / n.max(1) as f32, accent)
                                            .into_any_element()
                                    }
                                    (true, None) => ui::sweep(accent, 0.0).into_any_element(),
                                    (false, _) => {
                                        ui::bar(1.0, status_color(job.status)).into_any_element()
                                    }
                                }),
                            )
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .text_color(c(ui::MUTED))
                                    .child(meta.join(" · ")),
                            ),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .pt(px(16.))
                    .flex()
                    .gap(px(8.))
                    .when(running, |d| {
                        let id = job.id;
                        d.child(
                            ui::ghost("stop", "Stop")
                                .text_color(c(ui::RED))
                                .on_click(cx.listener(move |this, _, _, cx| this.stop_job(id, cx))),
                        )
                    })
                    .when(!running, |d| {
                        let path = job.folder.path.clone();
                        d.child(
                            ui::ghost("open-folder", "Open folder").on_click(cx.listener(
                                move |_, _, _, cx| cx.reveal_path(std::path::Path::new(&path)),
                            )),
                        )
                    }),
            );

        // The feed: what the bot said, the steps it took, and what you added.
        let mut feed = div().flex().flex_col().gap(px(10.)).px(px(30.)).py(px(20.));
        let mut tools: Vec<(String, String, u8)> = Vec::new();
        let flush = |feed: Div, tools: &mut Vec<(String, String, u8)>| -> Div {
            if tools.is_empty() {
                return feed;
            }
            let rows: Vec<_> = std::mem::take(tools);
            let mut list = div()
                .ml(px(40.))
                .px(px(14.))
                .py(px(8.))
                .rounded(px(14.))
                .bg(ca(0xFFFFFF, 0.6))
                .border_1()
                .border_color(c(ui::LINE))
                .flex()
                .flex_col()
                .gap(px(4.));
            // Long runs of steps fold, keeping the latest in view.
            let hidden = rows.len().saturating_sub(8);
            if hidden > 0 {
                list = list.child(
                    div()
                        .text_size(px(11.5))
                        .text_color(c(ui::FAINT))
                        .child(format!("{hidden} earlier steps")),
                );
            }
            for (label, kind, state) in rows.into_iter().skip(hidden) {
                let (mark, col) = match state {
                    0 => ("…", ui::ORANGE),
                    2 => ("✕", ui::RED),
                    _ => ("✓", ui::GREEN),
                };
                list = list.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .text_size(px(12.5))
                        .child(
                            Icon::new(tool_icon(&kind))
                                .size(px(13.))
                                .text_color(c(ui::MUTED)),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .font_family(ui::MONO)
                                .text_size(px(11.5))
                                .text_color(c(ui::BODY))
                                .child(label),
                        )
                        .child(
                            div()
                                .text_color(c(col))
                                .font_weight(FontWeight::BOLD)
                                .child(mark),
                        ),
                );
            }
            feed.child(list)
        };
        if job.feed.is_empty() && running {
            feed = feed.child(note_line("Starting the engine and reading the folder…"));
        }
        for item in &job.feed {
            if let Item::Tool {
                label, kind, state, ..
            } = item
            {
                tools.push((label.clone(), kind.clone(), *state));
                continue;
            }
            feed = flush(feed, &mut tools);
            feed = match item {
                Item::Text(t) => {
                    let t = plain(t);
                    if t.is_empty() {
                        feed
                    } else {
                        let face = self.avatar(who, 28.);
                        feed.child(
                            div().flex().gap(px(12.)).child(face).child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .px(px(18.))
                                    .py(px(13.))
                                    .rounded(px(16.))
                                    .bg(ca(0xFFFFFF, 0.85))
                                    .border_1()
                                    .border_color(c(0xFFFFFF))
                                    .shadow(ui::lift())
                                    .text_size(px(14.))
                                    .line_height(px(21.))
                                    .child(t),
                            ),
                        )
                    }
                }
                Item::You(t) => feed.child(
                    div().flex().justify_end().child(
                        div()
                            .max_w(px(620.))
                            .px(px(16.))
                            .py(px(10.))
                            .rounded(px(16.))
                            .bg(c(ui::INK))
                            .text_color(c(0xFFFFFF))
                            .text_size(px(14.))
                            .line_height(px(20.))
                            .child(t.clone()),
                    ),
                ),
                Item::Note(t) => feed.child(note_line(t)),
                Item::Error(t) => feed.child(
                    div()
                        .ml(px(40.))
                        .px(px(16.))
                        .py(px(12.))
                        .rounded(px(14.))
                        .bg(ca(ui::RED, 0.07))
                        .border_1()
                        .border_color(ca(ui::RED, 0.25))
                        .text_size(px(13.5))
                        .line_height(px(20.))
                        .text_color(c(0x9A1F30))
                        .child(t.clone()),
                ),
                Item::Tool { .. } => feed,
            };
        }
        feed = flush(feed, &mut tools);
        if running {
            let dots = typing(self.now());
            feed = feed.child(
                div()
                    .ml(px(40.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(dots)
                    .child(
                        div()
                            .text_size(px(12.5))
                            .text_color(c(ui::MUTED))
                            .child(job.doing()),
                    ),
            );
        }
        // Keep the newest line in view while it runs.
        let len = job.feed.len()
            + job
                .feed
                .last()
                .map(|i| match i {
                    Item::Text(t) => t.len() / 80,
                    _ => 0,
                })
                .unwrap_or(0);
        if len != self.live_len {
            self.live_len = len;
            self.live_scroll.scroll_to_bottom();
        }
        let input_hint = if running {
            "It's working. You can add to the job when it finishes."
        } else {
            "Ask a follow-up or ask for a change…"
        };
        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .child(header)
            .child(
                div()
                    .id("feed")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.live_scroll)
                    .child(feed),
            )
            .child(
                div().flex_none().px(px(30.)).pb(px(20.)).pt(px(6.)).child(
                    ui::card()
                        .rounded(px(18.))
                        .px(px(16.))
                        .py(px(8.))
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .when(running, |d| d.opacity(0.6))
                        .child(
                            div().flex_1().text_size(px(14.)).child(
                                Input::new(&self.live_input)
                                    .appearance(false)
                                    .disabled(running),
                            ),
                        )
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(c(ui::FAINT))
                                .child(input_hint),
                        ),
                ),
            )
    }

    fn live_side(&mut self, job: &Job, cx: &mut Context<Self>) -> impl IntoElement {
        let mut plan = div().flex().flex_col().gap(px(10.));
        if job.plan.is_empty() {
            plan = plan.child(
                div()
                    .text_size(px(12.5))
                    .text_color(c(ui::FAINT))
                    .line_height(px(18.))
                    .child(if job.status == Status::Running {
                        "Its steps show here when it writes a plan."
                    } else {
                        "No plan for this one: it went straight at it."
                    }),
            );
        }
        for (text, mark) in &job.plan {
            let (dot, col, weight) = match mark {
                2 => ("✓", ui::GREEN, FontWeight::NORMAL),
                1 => ("●", job.bot.accent(), FontWeight::SEMIBOLD),
                _ => ("○", ui::FAINT, FontWeight::NORMAL),
            };
            plan = plan.child(
                div()
                    .flex()
                    .gap(px(10.))
                    .text_size(px(13.))
                    .line_height(px(18.))
                    .child(div().w(px(14.)).flex_none().text_color(c(col)).child(dot))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .font_weight(weight)
                            .text_color(c(if *mark == 0 { ui::MUTED } else { ui::INK }))
                            .child(text.clone()),
                    ),
            );
        }
        let mut files = div().flex().flex_col().gap(px(6.));
        if job.files.is_empty() {
            files = files.child(div().text_size(px(12.5)).text_color(c(ui::FAINT)).child(
                if job.mode.writes() {
                    "Nothing changed yet."
                } else {
                    "This kind of job only reads. Nothing changes."
                },
            ));
        }
        for f in job.files.iter().take(14) {
            files = files.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        Icon::new(IconName::Pencil)
                            .size(px(12.))
                            .text_color(c(ui::MUTED)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font_family(ui::MONO)
                            .text_size(px(11.5))
                            .child(f.clone()),
                    ),
            );
        }
        if job.files.len() > 14 {
            files = files.child(note_line(&format!("and {} more", job.files.len() - 14)));
        }
        let color = status_color(job.status);
        let others: Vec<_> = self
            .store
            .jobs
            .iter()
            .rev()
            .filter(|j| j.id != job.id && j.bot == job.bot)
            .take(3)
            .map(|j| (j.id, j.title.clone(), j.status))
            .collect();
        let has_more = !others.is_empty();
        let mut more = div().flex().flex_col().gap(px(4.));
        for (id, title, status) in others {
            more = more.child(
                div()
                    .id(("other-job", id as usize))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .mx(px(-8.))
                    .px(px(8.))
                    .py(px(5.))
                    .rounded(px(9.))
                    .cursor_pointer()
                    .hover(|s| s.bg(c(0xFFFFFF)))
                    .child(
                        div()
                            .size(px(7.))
                            .rounded_full()
                            .bg(c(status_color(status))),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(12.5))
                            .child(title),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.go(Place::Live(id), cx))),
            );
        }
        div()
            .id("live-side")
            .w(px(360.))
            .flex_none()
            .h_full()
            .overflow_y_scroll()
            .border_l_1()
            .border_color(c(ui::LINE))
            .bg(ca(0xFFFFFF, 0.45))
            .px(px(24.))
            .pt(px(if cfg!(target_os = "macos") { 30. } else { 24. }))
            .pb(px(24.))
            .flex()
            .flex_col()
            .gap(px(26.))
            .child(
                ui::card()
                    .p(px(18.))
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(ui::status(color, job.status.label()))
                    .child(
                        div()
                            .text_size(px(13.))
                            .line_height(px(19.))
                            .text_color(c(ui::BODY))
                            .child(match job.status {
                                Status::Running => format!(
                                    "Started {} in {}. You can leave this screen; it keeps going.",
                                    ago_phrase(job.started),
                                    job.folder.name
                                ),
                                Status::Done => format!(
                                    "Finished {} after {}. Read its last note, then try it yourself.",
                                    ago_phrase(job.ended.unwrap_or(job.started)),
                                    store::span(job.seconds())
                                ),
                                _ => format!(
                                    "Ran for {}. Its last note says what went wrong; ask a follow-up to carry on.",
                                    store::span(job.seconds())
                                ),
                            }),
                    ),
            )
            .child(section("Plan", plan))
            .child(section("Files changed", files))
            .when(has_more, |d| {
                d.child(section(&format!("{}'s other jobs", job.bot.name()), more))
            })
    }

    /// A centred message for a place with nothing in it.
    pub(super) fn empty_place(
        &mut self,
        title: &'static str,
        line: &'static str,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        let keeper = self.bot(
            super::data::Member::Keeper,
            Mood::Asleep,
            160.,
            Framing::Full,
        );
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(8.))
            .child(keeper)
            .child(
                div()
                    .text_size(px(26.))
                    .font_weight(FontWeight::BOLD)
                    .child(title),
            )
            .child(
                div()
                    .text_size(px(14.))
                    .text_color(c(ui::MUTED))
                    .child(line),
            )
            .into_any_element()
    }
}

fn ago_phrase(at: u64) -> String {
    match store::ago(at).as_str() {
        "just now" => "just now".into(),
        a => format!("{a} ago"),
    }
}

pub(super) fn status_color(s: Status) -> u32 {
    match s {
        Status::Running => ui::ORANGE,
        Status::Done => ui::GREEN,
        Status::Failed => ui::RED,
        _ => ui::AMBER,
    }
}

fn chip(icon: IconName, text: String) -> Div {
    div()
        .ml(px(4.))
        .flex()
        .items_center()
        .gap(px(5.))
        .px(px(9.))
        .py(px(3.))
        .rounded_full()
        .border_1()
        .border_color(c(ui::LINE))
        .bg(c(0xFFFFFF))
        .text_size(px(11.5))
        .child(Icon::new(icon).size(px(11.)).text_color(c(ui::MUTED)))
        .child(text)
}

fn section(title: &str, body: Div) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(12.))
        .child(
            div()
                .text_size(px(13.5))
                .font_weight(FontWeight::SEMIBOLD)
                .child(title.to_string()),
        )
        .child(body)
}

pub(super) fn note_line(text: &str) -> Div {
    div()
        .ml(px(40.))
        .text_size(px(12.5))
        .text_color(c(ui::MUTED))
        .italic()
        .child(text.to_string())
}

/// Three dots bouncing in turn, for a bot that is mid-thought.
pub(super) fn typing(now: f64) -> Div {
    let mut dots = div()
        .px(px(14.))
        .h(px(34.))
        .rounded(px(14.))
        .bg(ca(0xFFFFFF, 0.85))
        .border_1()
        .border_color(c(0xFFFFFF))
        .shadow(ui::lift())
        .flex()
        .items_center()
        .gap(px(5.));
    for i in 0..3 {
        let p = ((now * 1.6 - i as f64 * 0.18).rem_euclid(1.0) * std::f64::consts::TAU)
            .sin()
            .max(0.0);
        dots = dots.child(
            div()
                .size(px(7.))
                .rounded_full()
                .relative()
                .top(px(-3. * p as f32))
                .bg(ca(ui::BLUE, 0.45 + 0.55 * p as f32)),
        );
    }
    dots
}
