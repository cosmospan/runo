//! Home's working half: a strip of numbers, the jobs table and the inspector
//! for the job picked in it.

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use runo_holo::{Framing, Mood};

use super::store::{self, Job, Status};
use super::ui::{self, c, ca};
use super::{Place, Shell};

impl Shell {
    /// Running, needs you, done today and what today cost.
    pub(super) fn kpis(&mut self) -> impl IntoElement {
        let day = 86_400;
        let today: Vec<&Job> = self
            .crew
            .store
            .jobs
            .iter()
            .filter(|j| store::now().saturating_sub(j.ended.unwrap_or(j.started)) < day)
            .collect();
        let running: Vec<&Job> = self.crew.store.running().collect();
        let needs: Vec<&Job> = self
            .crew
            .store
            .listed()
            .into_iter()
            .filter(|j| j.needs_you())
            .collect();
        let done = today
            .iter()
            .filter(|j| matches!(j.status, Status::Done | Status::Shipped))
            .count();
        let ended = today.iter().filter(|j| j.ended.is_some()).count();
        let spend: f64 = today.iter().filter_map(|j| cost(&j.summary)).sum::<f64>() + 0.0;
        let mut names: Vec<&str> = Vec::new();
        for j in &running {
            if !names.contains(&j.bot.name()) {
                names.push(j.bot.name());
            }
        }
        let who = match names.len() {
            0 => "nobody busy".to_string(),
            1..=2 => names.join(", "),
            n => format!("{}, {} and {} more", names[0], names[1], n - 2),
        };
        let oldest = needs
            .iter()
            .map(|j| j.ended.unwrap_or(j.started))
            .min()
            .map(|t| match store::ago(t).as_str() {
                "just now" => "just now".to_string(),
                a => format!("oldest {a}"),
            })
            .unwrap_or_else(|| "all clear".into());
        div()
            .w_full()
            .flex()
            .gap(px(12.))
            .child(ui::stat(
                Some(ui::t_blue()),
                "Running",
                running.len().to_string(),
                who,
            ))
            .child(ui::stat(
                Some(ui::t_amber()),
                "Needs you",
                needs.len().to_string(),
                oldest,
            ))
            .child(ui::stat(
                Some(ui::t_green()),
                "Done today",
                done.to_string(),
                format!("of {ended} finished"),
            ))
            .child(ui::stat(
                None,
                "Spend today",
                format!("${spend:.2}"),
                "across engines".into(),
            ))
    }

    /// Jobs grouped the way you act on them: what needs you, what's in
    /// progress, and what finished lately.
    pub(super) fn jobs_table(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let needs: Vec<Job> = self
            .crew
            .store
            .listed()
            .into_iter()
            .filter(|j| j.needs_you())
            .cloned()
            .collect();
        let going: Vec<Job> = self
            .crew
            .store
            .listed()
            .into_iter()
            .filter(|j| !j.needs_you() && matches!(j.status, Status::Running | Status::Shipping))
            .cloned()
            .collect();
        let done: Vec<Job> = self
            .crew
            .store
            .listed()
            .into_iter()
            .filter(|j| !j.needs_you() && !matches!(j.status, Status::Running | Status::Shipping))
            .take(8)
            .cloned()
            .collect();
        // No table box: each group is a heading on the canvas over its own
        // floating rows.
        let mut list = div().w_full().flex().flex_col().gap(px(8.));
        for (key, label, color, rows, empty) in [
            (
                0usize,
                "Needs you",
                ui::AMBER,
                needs,
                "Nothing is waiting on you.",
            ),
            (
                1,
                "In progress",
                ui::BLUE,
                going,
                "Nothing is running. Describe a job above to start one.",
            ),
            (2, "Done", ui::GREEN, done, ""),
        ] {
            if rows.is_empty() && empty.is_empty() {
                continue;
            }
            let shut = self.home_tab & (1 << key) != 0;
            list = list.child(
                div()
                    .id(("home-group", key))
                    .when(key > 0, |d| d.mt(px(10.)))
                    .h(px(30.))
                    .px(px(4.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .cursor_pointer()
                    .child(
                        div()
                            .w(px(10.))
                            .text_color(c(ui::faint()))
                            .text_size(px(10.))
                            .child(if shut { "\u{25b8}" } else { "\u{25be}" }),
                    )
                    .child(div().size(px(7.)).rounded_full().bg(c(color)))
                    .child(div().text_color(c(ui::ink())).child(label))
                    .child(
                        div()
                            .h(px(20.))
                            .px(px(7.))
                            .flex()
                            .items_center()
                            .rounded(px(5.))
                            .bg(c(ui::raised()))
                            .text_size(px(11.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(c(ui::muted()))
                            .child(rows.len().to_string()),
                    )
                    .when(key == 2, |d| {
                        d.child(div().flex_1()).child(
                            div()
                                .id("home-all")
                                .text_color(c(ui::muted()))
                                .hover(|s| s.text_color(c(ui::ink())))
                                .child("All jobs \u{2192}")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.go(Place::History, cx)
                                })),
                        )
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.home_tab ^= 1 << key;
                        cx.notify();
                    })),
            );
            if shut {
                continue;
            }
            if rows.is_empty() {
                list = list.child(
                    div()
                        .px(px(16.))
                        .py(px(16.))
                        .rounded(px(12.))
                        .border_1()
                        .border_color(c(ui::line2()))
                        .text_size(px(13.))
                        .text_color(c(ui::faint()))
                        .child(empty),
                );
            } else {
                list = list.child(self.job_rows(&rows, false, cx));
            }
        }
        list
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
        let mut table = div().flex().flex_col().child(table_head(wide));
        if rows.is_empty() {
            table = table.child(
                div()
                    .px(px(14.))
                    .py(px(22.))
                    .text_size(px(13.))
                    .text_color(c(ui::faint()))
                    .child(match tab {
                        0 => "Nothing is running. Describe a job above to start one.",
                        1 => "Nothing is waiting on you.",
                        _ => "Finished jobs show up here.",
                    }),
            );
        }
        table.child(self.job_rows(rows, wide, cx))
    }

    /// One row per job.
    pub(super) fn job_rows(&mut self, rows: &[Job], wide: bool, cx: &mut Context<Self>) -> Div {
        let mut table = div().flex().flex_col().gap(px(6.));
        for j in rows {
            let running = j.status == Status::Running;
            let face = if running {
                self.bot(j.bot, Mood::Working, 26., Framing::Bust)
            } else {
                self.avatar(j.bot, 26.)
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
            let tries = if j.race == 0 {
                0
            } else {
                self.tries(j.race).len()
            };
            let sub = if tries > 1 {
                format!("Race of {tries} engines · {}", j.folder.name)
            } else if running {
                j.doing()
            } else {
                format!("#{} · {}", j.id, j.folder.name)
            };
            let when = match store::ago(j.ended.unwrap_or(j.started)).as_str() {
                "just now" => "now".to_string(),
                a => a.to_string(),
            };
            let id = j.id;
            let files = if j.changes.is_empty() {
                j.files.len()
            } else {
                j.changes.len()
            };
            table = table.child(
                div()
                    .id(("home-row", id as usize))
                    .h(px(54.))
                    .px(px(16.))
                    .flex()
                    .items_center()
                    .gap(px(14.))
                    .rounded(px(8.))
                    .bg(c(ui::panel()))
                    .border_1()
                    .border_color(c(ui::line2()))
                    .shadow(ui::edge())
                    .text_size(px(13.))
                    .cursor_pointer()
                    .hover(|s| s.bg(c(ui::raised())).border_color(c(ui::line_hi())))
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
                                    .text_color(c(ui::faint()))
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
                            .text_color(c(ui::muted()))
                            .map(|d| match (frac, progress) {
                                (Some(f), Some((done, n))) => d
                                    .child(div().w(px(64.)).child(ui::bar(f, tone.0).h(px(3.))))
                                    .child(format!("{done} / {n}")),
                                _ if running => {
                                    d.child(div().w(px(64.)).child(ui::sweep(tone.0, 0.).h(px(3.))))
                                }
                                _ => d.text_color(c(ui::faint())).child("—"),
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
                                .text_color(c(ui::muted()))
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
                                .text_color(c(ui::muted()))
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
                            .text_color(c(if files == 0 { ui::faint() } else { ui::ink() }))
                            .child(if files == 0 {
                                "—".to_string()
                            } else {
                                files.to_string()
                            }),
                    )
                    .child(
                        div()
                            .w(px(64.))
                            .flex_none()
                            .flex()
                            .justify_end()
                            .text_color(c(ui::muted()))
                            .child(when),
                    )
                    .when(!wide && self.picked() == Some(id), |d| {
                        d.bg(ca(ui::RUNO, 0.10))
                    })
                    .on_click(cx.listener(move |this, e: &ClickEvent, _, cx| {
                        if wide || e.click_count() > 1 {
                            this.open_job(id, cx);
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
        let jobs = &self.crew.store.jobs;
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
        let j = self.crew.store.jobs.iter().find(|j| j.id == id)?.clone();
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
                        .text_color(c(ui::faint()))
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
            .border_color(c(ui::line()))
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
                    .text_color(c(ui::faint()))
                    .child("PLAN"),
            );
        if j.plan.is_empty() {
            plan = plan.child(div().text_size(px(12.5)).text_color(c(ui::muted())).child(
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
                    .bg(c(ui::GREEN))
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
                    .border_color(c(ui::line_hi())),
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
                                1 => d.font_weight(FontWeight::SEMIBOLD).text_color(c(ui::ink())),
                                2 => d.text_color(c(ui::body())),
                                _ => d.text_color(c(ui::muted())),
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
            .border_color(c(ui::line()))
            .child(
                ui::ghost("insp-open", "Open job")
                    .flex_1()
                    .on_click(cx.listener(move |this, _, _, cx| this.open_job(id, cx))),
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
                // A floating card beside the page, not a strip glued to its edge.
                .my(px(12.))
                .mr(px(12.))
                .flex()
                .flex_col()
                .bg(c(ui::panel()))
                .border_1()
                .border_color(c(ui::line()))
                .rounded(px(12.))
                .shadow(ui::sheen())
                .overflow_hidden()
                .child(
                    div()
                        .h(px(44.))
                        .px(px(18.))
                        .flex()
                        .items_center()
                        .text_size(px(12.5))
                        .text_color(c(ui::muted()))
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
                        .border_color(c(ui::line()))
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
                                        .text_color(c(ui::muted()))
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
/// The jobs table's column names.
fn table_head(wide: bool) -> Div {
    let head = |label: &'static str, w: Option<f32>, right: bool| {
        div()
            .map(|d| match w {
                Some(w) => d.w(px(w)).flex_none(),
                None => d.flex_1().min_w_0(),
            })
            .when(right, |d| d.flex().justify_end())
            .child(label)
    };
    div()
        .h(px(30.))
        .px(px(17.))
        .flex()
        .items_center()
        .gap(px(14.))
        .text_size(px(11.5))
        .font_weight(FontWeight::MEDIUM)
        .text_color(c(ui::faint()))
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
        .child(head("Updated", Some(64.), true))
}

pub(super) fn status_of(j: &Job) -> (String, (u32, u32, u32)) {
    match j.status {
        Status::Running => ("Running".to_string(), ui::run_tone()),
        Status::Done if j.needs_you() => ("Ready to review".to_string(), ui::ask_tone()),
        Status::Done => ("Done".to_string(), ui::ok_tone()),
        Status::Failed => ("Failed".to_string(), ui::bad_tone()),
        Status::Interrupted => ("Interrupted".to_string(), ui::ask_tone()),
        Status::Review => ("Ready for review".to_string(), ui::ask_tone()),
        Status::Shipped => ("Shipped".to_string(), ui::ok_tone()),
        s => (s.label().to_string(), ui::grey_tone()),
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
        .rounded_full()
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
