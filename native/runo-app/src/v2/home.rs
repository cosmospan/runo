//! Home's working half: a strip of numbers, the jobs table and the inspector
//! for the job picked in it.

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use runo_holo::{Framing, Mood};

use super::store::{self, Job, Status};
use super::ui::{self, c};
use super::{Place, Shell};

/// The table's tabs.
const TABS: [&str; 3] = ["Active", "Needs you", "Finished"];

/// Status colours: text, fill and edge of a pill.
const RUN: (u32, u32, u32) = (0x2563EB, 0xEFF4FF, 0xDCE6FE);
const ASK: (u32, u32, u32) = (0xB45309, 0xFFF7EB, 0xFDE7C7);
const OK: (u32, u32, u32) = (0x15803D, 0xEFFBF3, 0xD3F2DE);
const BAD: (u32, u32, u32) = (0xB91C1C, 0xFEF1F1, 0xFBD9D9);
const GREY: (u32, u32, u32) = (0x52525B, 0xF4F4F5, 0xE4E4E7);

impl Shell {
    /// Running, needs you, done today and what today cost.
    pub(super) fn kpis(&mut self) -> impl IntoElement {
        let day = 86_400;
        let today: Vec<&Job> = self
            .store
            .jobs
            .iter()
            .filter(|j| store::now().saturating_sub(j.ended.unwrap_or(j.started)) < day)
            .collect();
        let running: Vec<&Job> = self.store.running().collect();
        let needs: Vec<&Job> = self.store.jobs.iter().filter(|j| j.needs_you()).collect();
        let done = today.iter().filter(|j| j.status == Status::Done).count();
        let spend: f64 = today.iter().filter_map(|j| cost(&j.summary)).sum();
        let who = running
            .first()
            .map(|j| j.bot.name().to_string())
            .unwrap_or_else(|| "nobody busy".into());
        let oldest = needs
            .iter()
            .map(|j| j.ended.unwrap_or(j.started))
            .min()
            .map(|t| match store::ago(t).as_str() {
                "just now" => "just now".to_string(),
                a => format!("oldest {a}"),
            })
            .unwrap_or_else(|| "all clear".into());
        let cell =
            |dot: Option<u32>, label: &'static str, value: String, note: String, last: bool| {
                div()
                    .flex_1()
                    .flex_basis(px(0.))
                    .px(px(18.))
                    .py(px(14.))
                    .when(!last, |d| d.border_r_1().border_color(c(0xEFEFF1)))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .text_size(px(12.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(c(ui::MUTED))
                            .when_some(dot, |d, col| {
                                d.child(div().size(px(7.)).rounded_full().bg(c(col)))
                            })
                            .child(label),
                    )
                    .child(
                        div()
                            .mt(px(4.))
                            .flex()
                            .items_baseline()
                            .gap(px(8.))
                            .child(
                                div()
                                    .text_size(px(22.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(value),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(c(ui::FAINT))
                                    .child(note),
                            ),
                    )
            };
        div()
            .w_full()
            .flex()
            .bg(c(0xFFFFFF))
            .border_1()
            .border_color(c(ui::LINE))
            .rounded(px(12.))
            .child(cell(
                Some(RUN.0),
                "Running",
                running.len().to_string(),
                who,
                false,
            ))
            .child(cell(
                Some(0xD97706),
                "Needs you",
                needs.len().to_string(),
                oldest,
                false,
            ))
            .child(cell(
                Some(0x16A34A),
                "Done today",
                done.to_string(),
                format!("{} today", today.len()),
                false,
            ))
            .child(cell(
                None,
                "Spend today",
                format!("${spend:.2}"),
                "across engines".into(),
                true,
            ))
    }

    /// Jobs as a table: active, waiting on you, or finished.
    pub(super) fn jobs_table(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let tab = (self.home_tab as usize).min(TABS.len() - 1);
        let active = |j: &Job| j.status == Status::Running || j.needs_you();
        let counts = [
            self.store.jobs.iter().filter(|j| active(j)).count(),
            self.store.jobs.iter().filter(|j| j.needs_you()).count(),
            self.store
                .jobs
                .iter()
                .filter(|j| j.status == Status::Done && !j.needs_you())
                .count(),
        ];
        let rows: Vec<Job> = self
            .store
            .jobs
            .iter()
            .rev()
            .filter(|j| match tab {
                0 => active(j),
                1 => j.needs_you(),
                _ => j.status == Status::Done && !j.needs_you(),
            })
            .take(8)
            .cloned()
            .collect();

        let mut tabs = div()
            .h(px(46.))
            .px(px(14.))
            .flex()
            .items_center()
            .gap(px(4.))
            .border_b_1()
            .border_color(c(ui::LINE));
        for (i, label) in TABS.into_iter().enumerate() {
            let on = i == tab;
            tabs = tabs.child(
                div()
                    .id(("home-tab", i))
                    .h(px(28.))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .rounded(px(6.))
                    .text_size(px(13.))
                    .font_weight(FontWeight::MEDIUM)
                    .cursor_pointer()
                    .map(|d| {
                        if on {
                            d.bg(c(0xF4F4F5)).text_color(c(ui::INK))
                        } else {
                            d.text_color(c(ui::MUTED))
                                .hover(|s| s.text_color(c(ui::INK)))
                        }
                    })
                    .child(label)
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(c(ui::FAINT))
                            .child(counts[i].to_string()),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.home_tab = i as u8;
                        cx.notify();
                    })),
            );
        }
        tabs = tabs.child(div().flex_1()).child(
            div()
                .id("home-all")
                .text_size(px(13.))
                .text_color(c(ui::MUTED))
                .cursor_pointer()
                .hover(|s| s.text_color(c(ui::INK)))
                .child("View all jobs →")
                .on_click(cx.listener(|this, _, _, cx| this.go(Place::History, cx))),
        );

        let table = self.job_table(&rows, false, tab, cx);
        div()
            .w_full()
            .bg(c(0xFFFFFF))
            .border_1()
            .border_color(c(ui::LINE))
            .rounded(px(12.))
            .overflow_hidden()
            .child(tabs)
            .child(table)
    }

    /// The jobs table's header and rows. `wide` adds engine, time and cost, for
    /// the Jobs page; on Home a click picks the row for the inspector.
    pub(super) fn job_table(
        &mut self,
        rows: &[Job],
        wide: bool,
        tab: usize,
        cx: &mut Context<Self>,
    ) -> Div {
        let head = |label: &'static str, w: Option<f32>, right: bool| {
            div()
                .map(|d| match w {
                    Some(w) => d.w(px(w)).flex_none(),
                    None => d.flex_1().min_w_0(),
                })
                .when(right, |d| d.flex().justify_end())
                .child(label)
        };
        let mut table = div().flex().flex_col().child(
            div()
                .h(px(34.))
                .px(px(14.))
                .flex()
                .items_center()
                .gap(px(14.))
                .bg(c(0xFCFCFD))
                .border_b_1()
                .border_color(c(0xEFEFF1))
                .text_size(px(11.5))
                .font_weight(FontWeight::MEDIUM)
                .text_color(c(ui::FAINT))
                .child(head("Job", None, false))
                .child(head("Bot", Some(120.), false))
                .child(head("Status", Some(150.), false))
                .child(head("Progress", Some(120.), false))
                .when(wide, |d| {
                    d.child(head("Engine", Some(130.), false))
                        .child(head("Time", Some(64.), true))
                        .child(head("Cost", Some(60.), true))
                })
                .child(head("Files", Some(52.), true))
                .child(head("Updated", Some(64.), true)),
        );
        if rows.is_empty() {
            table = table.child(
                div()
                    .px(px(14.))
                    .py(px(22.))
                    .text_size(px(13.))
                    .text_color(c(ui::FAINT))
                    .child(match tab {
                        0 => "Nothing is running. Describe a job above to start one.",
                        1 => "Nothing is waiting on you.",
                        _ => "Finished jobs show up here.",
                    }),
            );
        }
        for j in rows {
            let running = j.status == Status::Running;
            let face = if running {
                self.bot(j.bot, Mood::Working, 22., Framing::Bust)
            } else {
                self.avatar(j.bot, 22.)
            };
            let (word, tone) = status_of(j);
            let progress = j.progress();
            let frac = progress.map(|(d, n)| {
                if running {
                    self.glide_bar(j.id, d as f32 / n.max(1) as f32)
                } else {
                    d as f32 / n.max(1) as f32
                }
            });
            let sub = if running {
                j.doing()
            } else {
                format!("#{} · {}", j.id, j.folder.name)
            };
            let when = match store::ago(j.ended.unwrap_or(j.started)).as_str() {
                "just now" => "now".to_string(),
                a => a.to_string(),
            };
            let id = j.id;
            table = table.child(
                div()
                    .id(("home-row", id as usize))
                    .h(px(50.))
                    .px(px(14.))
                    .flex()
                    .items_center()
                    .gap(px(14.))
                    .border_b_1()
                    .border_color(c(0xF2F2F4))
                    .text_size(px(13.))
                    .cursor_pointer()
                    .hover(|s| s.bg(c(0xF8F9FB)))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .font_weight(FontWeight::MEDIUM)
                                    .truncate()
                                    .child(j.title.clone()),
                            )
                            .child(
                                div()
                                    .mt(px(2.))
                                    .text_size(px(11.5))
                                    .text_color(c(ui::FAINT))
                                    .truncate()
                                    .child(sub),
                            ),
                    )
                    .child(
                        div()
                            .w(px(120.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(face)
                            .child(j.bot.name()),
                    )
                    .child(
                        div()
                            .w(px(150.))
                            .flex_none()
                            .flex()
                            .child(pill(&word, tone)),
                    )
                    .child(
                        div()
                            .w(px(120.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .text_size(px(12.))
                            .text_color(c(ui::MUTED))
                            .map(|d| match (frac, progress) {
                                (Some(f), Some((done, n))) => d
                                    .child(div().w(px(64.)).child(ui::bar(f, tone.0).h(px(3.))))
                                    .child(format!("{done} / {n}")),
                                _ if running => {
                                    d.child(div().w(px(64.)).child(ui::sweep(tone.0, 0.).h(px(3.))))
                                }
                                _ => d.text_color(c(ui::FAINT)).child("—"),
                            }),
                    )
                    .when(wide, |d| {
                        let secs = j.seconds();
                        d.child(
                            div()
                                .w(px(130.))
                                .flex_none()
                                .truncate()
                                .text_size(px(12.))
                                .text_color(c(ui::MUTED))
                                .child(j.engine.split(" · ").next().unwrap_or("—").to_string()),
                        )
                        .child(
                            div()
                                .w(px(64.))
                                .flex_none()
                                .flex()
                                .justify_end()
                                .font_family(ui::MONO)
                                .text_size(px(11.5))
                                .text_color(c(ui::MUTED))
                                .child(if secs == 0 {
                                    "—".to_string()
                                } else if secs >= 60 {
                                    format!("{}m {:02}s", secs / 60, secs % 60)
                                } else {
                                    format!("{secs}s")
                                }),
                        )
                        .child(
                            div()
                                .w(px(60.))
                                .flex_none()
                                .flex()
                                .justify_end()
                                .font_family(ui::MONO)
                                .text_size(px(11.5))
                                .child(
                                    cost(&j.summary)
                                        .map_or("—".to_string(), |c| format!("${c:.2}")),
                                ),
                        )
                    })
                    .child(
                        div()
                            .w(px(52.))
                            .flex_none()
                            .flex()
                            .justify_end()
                            .font_family(ui::MONO)
                            .text_size(px(11.5))
                            .text_color(c(if j.files.is_empty() {
                                ui::FAINT
                            } else {
                                ui::INK
                            }))
                            .child(if j.files.is_empty() {
                                "—".to_string()
                            } else {
                                j.files.len().to_string()
                            }),
                    )
                    .child(
                        div()
                            .w(px(64.))
                            .flex_none()
                            .flex()
                            .justify_end()
                            .text_color(c(ui::MUTED))
                            .child(when),
                    )
                    .when(!wide && self.picked() == Some(id), |d| d.bg(c(0xF4F7FF)))
                    .on_click(cx.listener(move |this, e: &ClickEvent, _, cx| {
                        if wide || e.click_count() > 1 {
                            this.go(Place::Live(id), cx);
                        } else {
                            this.home_pick = Some(id);
                            cx.notify();
                        }
                    })),
            );
        }
        table
    }
}

impl Shell {
    /// The job the inspector shows: the one picked, else the newest running or
    /// waiting job, else the newest one.
    pub(super) fn picked(&self) -> Option<u64> {
        let jobs = &self.store.jobs;
        self.home_pick
            .filter(|id| jobs.iter().any(|j| j.id == *id))
            .or_else(|| {
                jobs.iter()
                    .rev()
                    .find(|j| j.status == Status::Running || j.needs_you())
                    .or(jobs.last())
                    .map(|j| j.id)
            })
    }

    /// A panel down the right of Home: the picked job's bot, facts and plan.
    pub(super) fn inspector(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let id = self.picked()?;
        let j = self.store.jobs.iter().find(|j| j.id == id)?.clone();
        let running = j.status == Status::Running;
        let face = self.bot(
            j.bot,
            if running { Mood::Working } else { Mood::Idle },
            64.,
            Framing::Bust,
        );
        let (word, tone) = status_of(&j);
        let step = j
            .progress()
            .map(|(d, n)| format!("Step {} of {n}", (d + 1).min(n)));
        let fact = |k: &'static str, v: AnyElement| {
            div()
                .flex()
                .items_center()
                .min_h(px(24.))
                .text_size(px(12.5))
                .child(
                    div()
                        .w(px(86.))
                        .flex_none()
                        .text_color(c(ui::FAINT))
                        .child(k),
                )
                .child(div().flex_1().min_w_0().truncate().child(v))
        };
        let text = |s: String| div().child(s).into_any_element();
        let mut facts = div()
            .px(px(18.))
            .py(px(14.))
            .flex()
            .flex_col()
            .gap(px(6.))
            .border_b_1()
            .border_color(c(ui::LINE))
            .child(fact(
                "Status",
                div()
                    .flex()
                    .child(pill(&step.filter(|_| running).unwrap_or(word), tone))
                    .into_any_element(),
            ))
            .child(fact(
                "Engine",
                text(if j.engine.is_empty() {
                    "—".into()
                } else {
                    j.engine.clone()
                }),
            ))
            .child(fact("Folder", text(j.folder.name.clone())))
            .child(fact("Started", text(store::ago(j.started))));
        if !j.files.is_empty() {
            facts = facts.child(fact(
                "Files",
                div()
                    .font_family(ui::MONO)
                    .text_size(px(11.5))
                    .child(match j.files.len() {
                        1 => j.files[0].clone(),
                        n => format!("{} +{}", j.files[0], n - 1),
                    })
                    .into_any_element(),
            ));
        }
        if let Some(cost) = cost(&j.summary) {
            facts = facts.child(fact("Cost", text(format!("${cost:.2}"))));
        }

        let mut plan = div()
            .px(px(18.))
            .py(px(14.))
            .flex()
            .flex_col()
            .gap(px(9.))
            .child(
                div()
                    .text_size(px(11.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(c(ui::FAINT))
                    .child("PLAN"),
            );
        if j.plan.is_empty() {
            plan = plan.child(div().text_size(px(12.5)).text_color(c(ui::MUTED)).child(
                match j.last_words() {
                    Some(w) => w.lines().next().unwrap_or(w).to_string(),
                    None => j.doing(),
                },
            ));
        }
        for (s, state) in &j.plan {
            let mark = match state {
                2 => div()
                    .size(px(14.))
                    .rounded_full()
                    .bg(c(0x16A34A))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(9.))
                    .text_color(c(0xFFFFFF))
                    .child("✓"),
                1 => div()
                    .size(px(14.))
                    .rounded_full()
                    .border_4()
                    .border_color(c(j.bot.accent())),
                _ => div()
                    .size(px(14.))
                    .rounded_full()
                    .border_1()
                    .border_color(c(0xD4D4D8)),
            };
            plan = plan.child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(9.))
                    .text_size(px(12.5))
                    .child(div().mt(px(1.)).flex_none().child(mark))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .map(|d| match state {
                                1 => d.font_weight(FontWeight::SEMIBOLD).text_color(c(ui::INK)),
                                2 => d.text_color(c(ui::BODY)),
                                _ => d.text_color(c(ui::MUTED)),
                            })
                            .child(s.clone()),
                    ),
            );
        }

        let foot = div()
            .p(px(14.))
            .flex()
            .gap(px(8.))
            .border_t_1()
            .border_color(c(ui::LINE))
            .child(
                ui::ghost("insp-open", "Open job")
                    .flex_1()
                    .on_click(cx.listener(move |this, _, _, cx| this.go(Place::Live(id), cx))),
            )
            .when(running, |d| {
                d.child(
                    ui::ghost("insp-stop", "Stop")
                        .flex_1()
                        .on_click(cx.listener(move |this, _, _, cx| this.stop_job(id, cx))),
                )
            });

        Some(
            div()
                .w(px(316.))
                .flex_none()
                .h_full()
                .flex()
                .flex_col()
                .bg(c(0xFFFFFF))
                .border_l_1()
                .border_color(c(ui::LINE))
                .child(
                    div()
                        .h(px(44.))
                        .px(px(18.))
                        .flex()
                        .items_center()
                        .text_size(px(12.5))
                        .text_color(c(ui::MUTED))
                        .child(format!("Job #{id}")),
                )
                .child(
                    div()
                        .px(px(18.))
                        .pb(px(14.))
                        .flex()
                        .items_center()
                        .gap(px(14.))
                        .border_b_1()
                        .border_color(c(ui::LINE))
                        .child(face)
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .child(
                                    div()
                                        .text_size(px(15.))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .line_height(px(20.))
                                        .child(j.title.clone()),
                                )
                                .child(
                                    div()
                                        .mt(px(3.))
                                        .text_size(px(12.))
                                        .text_color(c(ui::MUTED))
                                        .child(j.bot.name()),
                                ),
                        ),
                )
                .child(
                    div()
                        .id("insp-body")
                        .flex_1()
                        .overflow_y_scroll()
                        .child(facts)
                        .child(plan),
                )
                .child(foot)
                .into_any_element(),
        )
    }
}

/// A job's status as a word and pill colours.
pub(super) fn status_of(j: &Job) -> (String, (u32, u32, u32)) {
    match j.status {
        Status::Running => ("Running".to_string(), RUN),
        Status::Done if j.needs_you() => ("Ready to review".to_string(), ASK),
        Status::Done => ("Done".to_string(), OK),
        Status::Failed => ("Failed".to_string(), BAD),
        Status::Interrupted => ("Interrupted".to_string(), ASK),
        s => (s.label().to_string(), GREY),
    }
}

/// A status pill: a dot and a word on a tinted ground.
pub(super) fn pill(word: &str, (text, fill, edge): (u32, u32, u32)) -> Div {
    div()
        .h(px(22.))
        .px(px(8.))
        .flex()
        .items_center()
        .gap(px(6.))
        .rounded(px(6.))
        .bg(c(fill))
        .border_1()
        .border_color(c(edge))
        .text_size(px(12.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(c(text))
        .child(div().size(px(6.)).rounded_full().bg(c(text)))
        .child(word.to_string())
}

/// The dollars in a summary like "12s · 3 turns · $0.04".
pub(super) fn cost(summary: &str) -> Option<f64> {
    summary
        .split('·')
        .map(str::trim)
        .find_map(|p| p.strip_prefix('$'))
        .and_then(|v| v.parse().ok())
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_the_cost() {
        assert_eq!(super::cost("12s · 3 turns · $0.04"), Some(0.04));
        assert_eq!(super::cost("12s"), None);
    }
}
