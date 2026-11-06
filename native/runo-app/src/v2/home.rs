//! Home's working half: a strip of numbers, the jobs table and the inspector
//! for the job picked in it.

use gpui_kit::assets::IconName;
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
                (IconName::LoaderCircle, ui::t_blue()),
                "Running",
                running.len().to_string(),
                who,
            ))
            .child(ui::stat(
                (IconName::Inbox, ui::t_amber()),
                "Needs you",
                needs.len().to_string(),
                oldest,
            ))
            .child(ui::stat(
                (IconName::CircleCheck, ui::t_green()),
                "Done today",
                done.to_string(),
                format!("of {ended} finished"),
            ))
            .child(ui::stat(
                (IconName::Wallet, ui::t_violet()),
                "Spend today",
                format!("${spend:.2}"),
                "across engines".into(),
            ))
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
                                    .child(j.label().to_string()),
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
pub(super) fn pill(word: &str, tone: (u32, u32, u32)) -> Div {
    ui::badge(tone, word.to_string())
}

/// The dollars in a summary like "12s · 3 turns · $0.04".
pub(super) fn cost(summary: &str) -> Option<f64> {
    summary
        .split('·')
        .map(str::trim)
        .find_map(|p| p.strip_prefix('$'))
        .and_then(|v| v.parse().ok())
}

/// How wide the job list beside a job is.
const LIST_W: f32 = 360.;

impl Shell {
    /// A page with the job list beside it: the split view every job page uses.
    /// `pick` is the job open on the right, if any.
    pub(super) fn split(
        &mut self,
        pick: Option<u64>,
        detail: AnyElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let list = self.job_list(pick, cx);
        div()
            .size_full()
            .flex()
            .child(list)
            .child(div().flex_1().min_w_0().h_full().child(detail))
            .into_any_element()
    }

    /// The job list: filters, then what needs you, what's running and what
    /// finished, one row per job with the bot's face.
    fn job_list(&mut self, pick: Option<u64>, cx: &mut Context<Self>) -> impl IntoElement {
        let all: Vec<Job> = self.crew.store.listed().into_iter().cloned().collect();
        let running = |j: &Job| matches!(j.status, Status::Running | Status::Shipping);
        let needs: Vec<Job> = all.iter().filter(|j| j.needs_you()).cloned().collect();
        let going: Vec<Job> = all
            .iter()
            .filter(|j| !j.needs_you() && running(j))
            .cloned()
            .collect();
        let done: Vec<Job> = all
            .iter()
            .filter(|j| !j.needs_you() && !running(j))
            .cloned()
            .collect();
        let filter = self.list_filter;

        let mut filters = div()
            .flex()
            .items_center()
            .p(px(3.))
            .gap(px(2.))
            .rounded(px(9.))
            .bg(c(ui::well()))
            .border_1()
            .border_color(c(ui::line2()));
        for (i, label, n) in [
            (0u8, "All", all.len()),
            (1, "Needs you", needs.len()),
            (2, "Running", going.len()),
            (3, "Done", done.len()),
        ] {
            let on = filter == i;
            filters = filters.child(
                div()
                    .id(("list-filter", i as usize))
                    .flex_1()
                    .h(px(26.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(px(5.))
                    .rounded(px(6.))
                    .text_size(px(12.))
                    .font_weight(FontWeight::MEDIUM)
                    .cursor_pointer()
                    .map(|d| {
                        if on {
                            d.bg(c(ui::raised())).text_color(c(ui::ink()))
                        } else {
                            d.text_color(c(ui::muted()))
                                .hover(|s| s.text_color(c(ui::ink())))
                        }
                    })
                    .child(label)
                    .when(n > 0 && i > 0, |d| {
                        d.child(
                            div()
                                .text_size(px(11.))
                                .text_color(c(ui::faint()))
                                .child(n.to_string()),
                        )
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.list_filter = i;
                        cx.notify();
                    })),
            );
        }

        let head = div()
            .flex_none()
            .px(px(16.))
            .pt(px(16.))
            .pb(px(12.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .text_size(px(17.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Jobs"),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(c(ui::faint()))
                            .child(all.len().to_string()),
                    )
                    .child(div().flex_1())
                    .child(
                        ui::icon_btn("list-all", IconName::ListFilter)
                            .size(px(28.))
                            .tooltip(|window, cx| {
                                gpui_kit::component::tooltip::Tooltip::new("All jobs")
                                    .build(window, cx)
                            })
                            .on_click(cx.listener(|this, _, _, cx| this.go(Place::History, cx))),
                    ),
            )
            .child(filters);

        let mut body = div()
            .id("job-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(px(8.))
            .pb(px(16.))
            .flex()
            .flex_col()
            .gap(px(2.));
        let groups = [
            (1u8, "Needs you", ui::AMBER, needs),
            (2, "Running", ui::BLUE, going),
            (3, "Done", ui::GREEN, done),
        ];
        let mut any = false;
        for (key, label, color, rows) in groups {
            if (filter != 0 && filter != key) || rows.is_empty() {
                continue;
            }
            any = true;
            body = body.child(
                ui::heading(label, Some(color), Some(rows.len()))
                    .px(px(10.))
                    .pt(px(14.))
                    .pb(px(6.)),
            );
            for j in &rows {
                body = body.child(self.list_row(j, pick == Some(j.id), cx));
            }
        }
        if !any {
            body = body.child(
                div()
                    .px(px(10.))
                    .py(px(20.))
                    .text_size(px(12.5))
                    .text_color(c(ui::faint()))
                    .child("Nothing here."),
            );
        }

        div()
            .w(px(LIST_W))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .bg(c(ui::panel()))
            .border_r_1()
            .border_color(c(ui::line2()))
            .child(head)
            .child(body)
    }

    /// One job in the list: the bot's face, the title, its state, and a thin
    /// progress line while it runs.
    fn list_row(&mut self, j: &Job, on: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let id = j.id;
        let is_running = j.status == Status::Running;
        let face = if is_running {
            self.bot(j.bot, Mood::Working, 34., Framing::Bust)
        } else {
            self.avatar(j.bot, 34.)
        };
        let (word, tone) = status_of(j);
        let frac = j.progress().map(|(d, n)| {
            let f = d as f32 / n.max(1) as f32;
            if is_running {
                self.glide_bar(id, f)
            } else {
                f
            }
        });
        let when = match store::ago(j.ended.unwrap_or(j.started)).as_str() {
            "just now" => "now".to_string(),
            a => a.to_string(),
        };
        let sub = if is_running {
            j.doing()
        } else {
            format!("{} · #{}", j.bot.name(), id)
        };
        let accent = j.bot.accent();
        div()
            .id(("list-row", id as usize))
            .relative()
            .flex()
            .gap(px(11.))
            .px(px(10.))
            .py(px(10.))
            .rounded(px(10.))
            .cursor_pointer()
            .map(|d| {
                if on {
                    d.bg(c(ui::raised())).child(
                        div()
                            .absolute()
                            .left(px(-8.))
                            .top(px(12.))
                            .bottom(px(12.))
                            .w(px(3.))
                            .rounded_r(px(3.))
                            .bg(c(accent)),
                    )
                } else {
                    d.hover(|s| s.bg(c(ui::hover())))
                }
            })
            .child(
                div()
                    .size(px(38.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(10.))
                    .bg(ca(accent, 0.10))
                    .border_1()
                    .border_color(ca(accent, 0.22))
                    .child(face),
            )
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
                            .gap(px(8.))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(c(ui::ink()))
                                    .child(j.label().to_string()),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .text_size(px(11.))
                                    .text_color(c(ui::faint()))
                                    .child(when),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .text_size(px(11.5))
                            .text_color(c(ui::muted()))
                            .child(div().size(px(6.)).flex_none().rounded_full().bg(c(tone.0)))
                            .child(div().flex_none().text_color(c(tone.0)).child(word))
                            .child(div().flex_none().text_color(c(ui::faint())).child("·"))
                            .child(div().min_w_0().truncate().child(sub)),
                    )
                    .when_some(frac.filter(|_| is_running), |d, f| {
                        d.child(div().mt(px(4.)).child(ui::bar(f, ui::BLUE).h(px(3.))))
                    }),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.go(Place::Live(id), cx)))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_the_cost() {
        assert_eq!(super::cost("12s · 3 turns · $0.04"), Some(0.04));
        assert_eq!(super::cost("12s"), None);
    }
}
