//! Usage: what the crew cost and did over the last 30 days, from the jobs kept
//! on this computer.

use gpui_kit::assets::IconName;
use gpui_kit::*;

use super::data::Member;
use super::home::cost;
use super::store::{self, Status};
use super::ui::{self, c};
use super::Shell;

const DAYS: usize = 30;

/// One bot's share of the window.
struct Share {
    who: Member,
    jobs: usize,
    ok: usize,
    ended: usize,
    spend: f64,
}

impl Shell {
    pub(super) fn usage(&mut self, _cx: &mut Context<Self>) -> AnyElement {
        let now = store::now();
        let recent: Vec<_> = self
            .crew
            .store
            .jobs
            .iter()
            .filter(|j| now.saturating_sub(j.started) < DAYS as u64 * 86_400)
            .cloned()
            .collect();
        let spend: f64 = recent.iter().filter_map(|j| cost(&j.summary)).sum::<f64>() + 0.0;
        let ended = recent
            .iter()
            .filter(|j| j.status != Status::Running)
            .count();
        let ok = recent.iter().filter(|j| finished_well(j.status)).count();
        let busy: u64 = recent.iter().map(|j| j.seconds()).sum();

        // Per bot, busiest first.
        let mut shares: Vec<Share> = Vec::new();
        for j in &recent {
            let s = match shares.iter_mut().position(|s| s.who == j.bot) {
                Some(i) => &mut shares[i],
                None => {
                    shares.push(Share {
                        who: j.bot,
                        jobs: 0,
                        ok: 0,
                        ended: 0,
                        spend: 0.,
                    });
                    shares.last_mut().unwrap()
                }
            };
            s.jobs += 1;
            s.spend += cost(&j.summary).unwrap_or(0.);
            if j.status != Status::Running {
                s.ended += 1;
            }
            if finished_well(j.status) {
                s.ok += 1;
            }
        }
        shares.sort_by_key(|s| std::cmp::Reverse(s.jobs));

        // Each day's stack: by spend when anything cost money, else by jobs.
        let by_money = spend > 0.;
        let mut days = vec![Vec::<(Member, f64)>::new(); DAYS];
        for j in &recent {
            let back = (now.saturating_sub(j.started) / 86_400) as usize;
            let day = &mut days[DAYS - 1 - back.min(DAYS - 1)];
            let v = if by_money {
                cost(&j.summary).unwrap_or(0.)
            } else {
                1.
            };
            match day.iter_mut().find(|(m, _)| *m == j.bot) {
                Some(e) => e.1 += v,
                None => day.push((j.bot, v)),
            }
        }
        let top = days
            .iter()
            .map(|d| d.iter().map(|e| e.1).sum::<f64>())
            .fold(0., f64::max)
            .max(f64::EPSILON);

        let kpis = div()
            .flex()
            .gap(px(12.))
            .child(ui::stat(
                (IconName::Wallet, ui::t_violet()),
                "Spend",
                format!("${spend:.2}"),
                "reported by engines".into(),
            ))
            .child(ui::stat(
                (IconName::LayoutDashboard, ui::t_blue()),
                "Jobs",
                recent.len().to_string(),
                format!("{} running", self.crew.store.running().count()),
            ))
            .child(ui::stat(
                (IconName::CircleCheck, ui::t_green()),
                "Success rate",
                if ended == 0 {
                    "—".into()
                } else {
                    format!("{:.0}%", ok as f64 * 100. / ended as f64)
                },
                format!("{ok} of {ended} finished"),
            ))
            .child(ui::stat(
                (IconName::Timer, ui::t_amber()),
                "Time at work",
                hours(busy),
                "across the crew".into(),
            ));

        let mut bars = div().h(px(220.)).flex().items_end().gap(px(4.));
        for d in &days {
            let mut col = div()
                .flex_1()
                .h_full()
                .flex()
                .flex_col()
                .justify_end()
                .gap(px(1.));
            let mut sorted = d.clone();
            sorted.sort_by_key(|(m, _)| m.key());
            for (m, v) in sorted.into_iter().rev() {
                col = col.child(
                    div()
                        .w_full()
                        .h(px((v / top * 210.) as f32))
                        .rounded(px(4.))
                        .bg(c(m.accent())),
                );
            }
            if d.is_empty() {
                col = col.child(
                    div()
                        .w_full()
                        .h(px(2.))
                        .rounded(px(1.))
                        .bg(c(ui::line_hi())),
                );
            }
            bars = bars.child(col);
        }
        let mut legend = div().flex().gap(px(12.));
        for s in shares.iter().take(5) {
            legend = legend.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .text_size(px(11.5))
                    .text_color(c(ui::muted()))
                    .child(div().size(px(7.)).rounded_full().bg(c(s.who.accent())))
                    .child(s.who.name()),
            );
        }
        let chart = ui::card()
            .flex_1()
            .min_w_0()
            .p(px(18.))
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(13.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(if by_money {
                                "Spend per day, by bot"
                            } else {
                                "Jobs per day, by bot"
                            }),
                    )
                    .child(legend),
            )
            .child(bars)
            .child(
                div()
                    .flex()
                    .justify_between()
                    .text_size(px(11.))
                    .text_color(c(ui::faint()))
                    .child("30 days ago")
                    .child("Today"),
            );

        let mut table = ui::card().w(px(380.)).flex_none().overflow_hidden().child(
            div()
                .h(px(36.))
                .px(px(16.))
                .flex()
                .items_center()
                .gap(px(10.))
                .bg(c(ui::raised()))
                .border_b_1()
                .border_color(c(ui::line()))
                .text_size(px(11.5))
                .font_weight(FontWeight::MEDIUM)
                .text_color(c(ui::faint()))
                .child(div().flex_1().child("Bot"))
                .child(div().w(px(44.)).flex().justify_end().child("Jobs"))
                .child(div().w(px(64.)).flex().justify_end().child("Spend"))
                .child(div().w(px(44.)).flex().justify_end().child("OK")),
        );
        if shares.is_empty() {
            table = table.child(
                div()
                    .px(px(16.))
                    .py(px(18.))
                    .text_size(px(12.5))
                    .text_color(c(ui::faint()))
                    .child("No jobs in the last 30 days."),
            );
        }
        for s in &shares {
            let face = self.avatar(s.who, 22.);
            table = table.child(
                div()
                    .h(px(46.))
                    .px(px(16.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .border_b_1()
                    .border_color(c(ui::line2()))
                    .text_size(px(13.))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .child(face)
                            .child(s.who.name()),
                    )
                    .child(
                        div()
                            .w(px(44.))
                            .flex()
                            .justify_end()
                            .child(s.jobs.to_string()),
                    )
                    .child(
                        div()
                            .w(px(64.))
                            .flex()
                            .justify_end()
                            .font_family(ui::MONO)
                            .text_size(px(12.))
                            .child(if s.spend == 0. {
                                "—".into()
                            } else {
                                format!("${:.2}", s.spend)
                            }),
                    )
                    .child(
                        div()
                            .w(px(44.))
                            .flex()
                            .justify_end()
                            .text_color(c(ui::muted()))
                            .child(match (s.ok * 100).checked_div(s.ended) {
                                None => "—".into(),
                                Some(pct) => format!("{pct}%"),
                            }),
                    ),
            );
        }

        div()
            .id("usage")
            .size_full()
            .overflow_y_scroll()
            .px(px(28.))
            .pt(px(24.))
            .pb(px(32.))
            .flex()
            .flex_col()
            .gap(px(18.))
            .child(
                div()
                    .child(
                        div()
                            .text_size(px(24.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Usage"),
                    )
                    .child(
                        div()
                            .mt(px(4.))
                            .text_size(px(13.5))
                            .text_color(c(ui::muted()))
                            .child("Last 30 days, from the jobs kept on this computer"),
                    ),
            )
            .child(kpis)
            .child(
                div()
                    .flex()
                    .gap(px(18.))
                    .items_start()
                    .child(chart)
                    .child(table),
            )
            .into_any_element()
    }
}

/// "41 h", "35 min", "12 s".
fn hours(secs: u64) -> String {
    match secs {
        s if s >= 3600 => format!("{:.1} h", s as f64 / 3600.),
        s if s >= 60 => format!("{} min", s / 60),
        s => format!("{s} s"),
    }
}

/// A job that did what it was asked: done, waiting for your OK, or on its way in.
fn finished_well(status: Status) -> bool {
    matches!(
        status,
        Status::Done | Status::Review | Status::Shipping | Status::Shipped
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn says_time_in_the_right_unit() {
        assert_eq!(super::hours(12), "12 s");
        assert_eq!(super::hours(150), "2 min");
        assert_eq!(super::hours(5400), "1.5 h");
    }
}
