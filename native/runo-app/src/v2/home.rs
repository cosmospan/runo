//! Home's working half: a strip of numbers and the jobs table.

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
        let cell = |dot: Option<u32>, label: &'static str, value: String, note: String, last: bool| {
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
                        .child(div().text_size(px(12.)).text_color(c(ui::FAINT)).child(note)),
                )
        };
        div()
            .w_full()
            .flex()
            .bg(c(0xFFFFFF))
            .border_1()
            .border_color(c(ui::LINE))
            .rounded(px(12.))
            .child(cell(Some(RUN.0), "Running", running.len().to_string(), who, false))
            .child(cell(Some(0xD97706), "Needs you", needs.len().to_string(), oldest, false))
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
        for j in &rows {
            let running = j.status == Status::Running;
            let face = if running {
                self.bot(j.bot, Mood::Working, 22., Framing::Bust)
            } else {
                self.avatar(j.bot, 22.)
            };
            let (word, tone) = match j.status {
                Status::Running => ("Running".to_string(), RUN),
                Status::Done if j.needs_you() => ("Ready to review".to_string(), ASK),
                Status::Done => ("Done".to_string(), OK),
                Status::Failed => ("Failed".to_string(), BAD),
                Status::Interrupted => ("Interrupted".to_string(), ASK),
                s => (s.label().to_string(), GREY),
            };
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
                    .child(div().w(px(150.)).flex_none().flex().child(pill(&word, tone)))
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
                    .on_click(cx.listener(move |this, _, _, cx| this.go(Place::Live(id), cx))),
            );
        }
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
}

/// A status pill: a dot and a word on a tinted ground.
fn pill(word: &str, (text, fill, edge): (u32, u32, u32)) -> Div {
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
fn cost(summary: &str) -> Option<f64> {
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
