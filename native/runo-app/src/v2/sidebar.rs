//! The sidebar: the folder you're in, search, the places, and the crew.
//!
//! The crew lives here, one row per bot: its face with a status badge, its name
//! and what it's doing right now, with a thin progress bar while it works. It
//! replaces the dock, so the bots are always in view and never sit on top of a
//! page.
//!
//! The look: a ground a shade darker than the page, rows without boxes, and the
//! place you're on marked by a soft tint and a short violet bar on its left
//! edge. Status colours only show where something is happening.

use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::data::Member;
use super::ui::{self, c, ca};
use super::{Place, Shell};

/// The sidebar's width.
pub(super) const WIDTH: f32 = 252.;

/// The sidebar's ground, a shade darker than the page so the page reads as
/// the thing in front.
fn ground() -> u32 {
    ui::well()
}

/// How tall a place row is.
const ROW_H: f32 = 30.;

/// The short violet bar on the left edge of the row you're on.
fn marker() -> Div {
    div()
        .absolute()
        .left(px(-10.))
        .top(px(8.))
        .bottom(px(8.))
        .w(px(3.))
        .rounded_r(px(3.))
        .bg(c(ui::RUNO))
}

/// A row's look when it is the place you're on, or when it isn't.
fn selectable(d: Stateful<Div>, on: bool) -> Stateful<Div> {
    if on {
        d.bg(c(ui::hover())).text_color(c(ui::ink())).child(marker())
    } else {
        d.text_color(c(ui::body()))
            .hover(|s| s.bg(ca(ui::hover(), 0.7)).text_color(c(ui::ink())))
    }
}

impl Shell {
    pub(super) fn sidebar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let place = self.place;
        let inbox = self.inbox_count();
        self.inbox_seen = inbox;
        let jobs = self.crew.store.listed().len();
        let folder = self.crew.store.folder().cloned();

        // The folder you're working in, at the top: a violet tile with its
        // first letter, its name and where it is.
        let (name, path) = match &folder {
            Some(f) => (f.name.clone(), short_path(&f.path)),
            None => ("No folder".to_string(), "Pick one on Jobs".to_string()),
        };
        let head = div()
            .id("side-folder")
            .flex()
            .items_center()
            .gap(px(10.))
            .px(px(6.))
            .py(px(6.))
            .rounded(px(9.))
            .cursor_pointer()
            .hover(|s| s.bg(c(ui::hover())))
            .child(
                div()
                    .size(px(30.))
                    .flex_none()
                    .rounded(px(8.))
                    .bg(ca(ui::RUNO, 0.22))
                    .border_1()
                    .border_color(ca(ui::RUNO, 0.45))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(14.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(c(ui::t_violet()))
                    .child(
                        name.chars()
                            .next()
                            .unwrap_or('S')
                            .to_uppercase()
                            .to_string(),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .text_size(px(13.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(c(ui::ink()))
                            .truncate()
                            .child(name),
                    )
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(c(ui::faint()))
                            .truncate()
                            .child(path),
                    ),
            )
            .child(
                Icon::new(IconName::ChevronsUpDown)
                    .size(px(14.))
                    .text_color(c(ui::faint())),
            )
            .on_click(cx.listener(|this, _, _, cx| this.go(Place::Today, cx)));

        // Search: the palette finds places, bots, jobs and folders.
        let search = div()
            .id("side-search")
            .mt(px(10.))
            .mb(px(14.))
            .h(px(32.))
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(8.))
            .rounded(px(8.))
            .bg(ca(ui::ink(), 0.035))
            .border_1()
            .border_color(c(ui::line2()))
            .text_size(px(13.))
            .text_color(c(ui::faint()))
            .cursor_pointer()
            .hover(|s| s.border_color(c(ui::line_hi())).text_color(c(ui::muted())))
            .child(Icon::new(IconName::Search).size(px(14.)))
            .child(div().flex_1().child("Search or run…"))
            .child(kbd(if cfg!(target_os = "macos") {
                "⌘K"
            } else {
                "Ctrl K"
            }))
            .on_click(cx.listener(|this, _, window, cx| this.open_palette(window, cx)));

        let row = |id: &'static str, icon: IconName, label: &'static str, on: bool| {
            selectable(
                div()
                    .id(id)
                    .relative()
                    .h(px(ROW_H))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .rounded(px(7.))
                    .text_size(px(13.))
                    .font_weight(FontWeight::MEDIUM)
                    .cursor_pointer(),
                on,
            )
            .child(Icon::new(icon).size(px(16.)).text_color(c(if on {
                ui::t_violet()
            } else {
                ui::muted()
            })))
            .child(div().flex_1().child(label))
        };
        // A count at the end of a row: amber when it is waiting on you, quiet
        // otherwise.
        let count = |n: usize, hot: bool| {
            div()
                .min_w(px(20.))
                .h(px(18.))
                .px(px(6.))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .text_size(px(11.))
                .font_weight(FontWeight::SEMIBOLD)
                .map(|d| {
                    if hot {
                        d.bg(ca(ui::AMBER, 0.16)).text_color(c(ui::t_amber()))
                    } else {
                        d.text_color(c(ui::faint()))
                    }
                })
                .child(n.min(999).to_string())
        };
        let nav = div()
            .flex()
            .flex_col()
            .gap(px(1.))
            .child(
                row(
                    "side-inbox",
                    IconName::Inbox,
                    "Needs you",
                    place == Place::Inbox,
                )
                .when(inbox > 0, |d| d.child(count(inbox, true)))
                .on_click(cx.listener(|this, _, _, cx| this.go(Place::Inbox, cx))),
            )
            .child(
                row(
                    "side-jobs",
                    IconName::LayoutDashboard,
                    "Jobs",
                    matches!(
                        place,
                        Place::Today
                            | Place::History
                            | Place::Live(_)
                            | Place::Race(_)
                            | Place::Goal(_)
                    ),
                )
                .when(jobs > 0, |d| d.child(count(jobs, false)))
                .on_click(cx.listener(|this, _, _, cx| this.go(Place::Today, cx))),
            )
            .child(
                row(
                    "side-term",
                    IconName::SquareTerminal,
                    "Terminal",
                    self.term_open.is_some_and(super::term::is_project),
                )
                .child(
                    div()
                        .font_family(ui::MONO)
                        .text_size(px(10.5))
                        .text_color(c(ui::faint()))
                        .child(super::term::KEY),
                )
                .on_click(cx.listener(|this, _, window, cx| this.toggle_project_term(window, cx))),
            )
            .child(
                row(
                    "side-usage",
                    IconName::ChartColumn,
                    "Usage",
                    place == Place::Usage,
                )
                .on_click(cx.listener(|this, _, _, cx| this.go(Place::Usage, cx))),
            );

        // The crew: every bot, what it's doing, and where clicking takes you.
        let mut crew = div().flex().flex_col().gap(px(1.));
        let (shown, more) = self.sidebar_crew();
        let working = self.crew.store.running().count();
        for who in shown {
            let size = 28.;
            let faces = self.faces.clone();
            let face = div()
                .relative()
                .flex_none()
                .child(self.side_bot(who, size))
                // Notes where the bot is, so its eyes can follow the pointer.
                .child(
                    canvas(
                        move |bounds, _, _| {
                            faces.borrow_mut().insert(who, bounds);
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .size_full(),
                );
            let busy = self
                .crew
                .store
                .busy(who)
                .map(|j| (j.id, j.title.clone(), j.progress()));
            let asking = self
                .crew
                .store
                .jobs
                .iter()
                .any(|j| j.bot == who && j.needs_you());
            let (word, dot) = self.status_of(who);
            let line = match &busy {
                Some((_, title, _)) => title.clone(),
                None if asking => "Needs you".to_string(),
                None => capital(&word),
            };
            // How far along a working bot is, at the end of its name line.
            let steps = busy
                .as_ref()
                .and_then(|(_, _, p)| p.map(|(d, n)| format!("{d}/{n}")));
            let here = place == Place::Chat(who)
                || matches!(place, Place::Live(id) if busy.as_ref().is_some_and(|b| b.0 == id));
            let bar = busy.as_ref().map(|(id, _, p)| {
                let to = p
                    .map(|(d, n)| (d as f32 + 0.5) / n.max(1) as f32)
                    .unwrap_or(0.0);
                (p.is_some(), self.glide_bar(*id, to))
            });
            let to = busy
                .as_ref()
                .map(|b| Place::Live(b.0))
                .unwrap_or(Place::Chat(who));
            let badge = if busy.is_some() {
                Some(ui::BLUE)
            } else if asking {
                Some(ui::AMBER)
            } else if dot == ui::faint() {
                None
            } else {
                Some(dot)
            };
            // The face, with its status as a small badge on its corner, cut
            // out of the ground so it reads at a glance.
            let avatar = div()
                .relative()
                .size(px(28.))
                .flex_none()
                .child(face)
                .when_some(badge, |d, color| {
                    d.child(
                        div()
                            .absolute()
                            .right(px(-2.))
                            .bottom(px(-1.))
                            .size(px(10.))
                            .rounded_full()
                            .border_2()
                            .border_color(c(if here { ui::hover() } else { ground() }))
                            .bg(c(color)),
                    )
                });
            crew = crew.child(
                selectable(
                    div()
                        .id(("side-bot", who.key() as usize))
                        .relative()
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .px(px(6.))
                        .py(px(5.))
                        .rounded(px(8.))
                        .cursor_pointer(),
                    here,
                )
                .on_hover(cx.listener(move |this, on: &bool, _, cx| {
                    this.hover_bot(who, *on, cx);
                }))
                .child(avatar)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .text_size(px(13.))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(c(ui::ink()))
                                        .truncate()
                                        .child(who.name()),
                                )
                                .when_some(steps, |d, s| {
                                    d.child(
                                        div()
                                            .flex_none()
                                            .font_family(ui::MONO)
                                            .text_size(px(10.5))
                                            .text_color(c(ui::t_blue()))
                                            .child(s),
                                    )
                                })
                                // Idle bots say so quietly on the same line,
                                // so a big crew stays short.
                                .when(busy.is_none() && !asking, |d| {
                                    d.child(
                                        div()
                                            .flex_none()
                                            .text_size(px(11.5))
                                            .text_color(c(ui::faint()))
                                            .child(line.clone()),
                                    )
                                }),
                        )
                        .when(busy.is_some() || asking, |d| {
                            d.child(
                                div()
                                    .text_size(px(11.5))
                                    .text_color(c(if asking && busy.is_none() {
                                        ui::t_amber()
                                    } else {
                                        ui::muted()
                                    }))
                                    .truncate()
                                    .child(line),
                            )
                        })
                        .map(|d| match bar {
                            Some((true, f)) => d.child(
                                div()
                                    .mt(px(5.))
                                    .child(ui::bar(f, ui::BLUE).h(px(2.)).bg(c(ui::line2()))),
                            ),
                            Some((false, _)) => d.child(
                                div()
                                    .mt(px(5.))
                                    .child(ui::sweep(ui::BLUE, 0.).h(px(2.)).bg(c(ui::line2()))),
                            ),
                            None => d,
                        }),
                )
                .on_click(cx.listener(move |this, _, _, cx| this.go(to, cx))),
            );
        }

        // A section's heading: a quiet label, with room for more on the right.
        let section = |label: &'static str| {
            div()
                .mt(px(22.))
                .mb(px(6.))
                .pl(px(8.))
                .pr(px(2.))
                .h(px(20.))
                .flex()
                .items_center()
                .gap(px(8.))
                .text_size(px(11.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(c(ui::faint()))
                .child(div().flex_1().child(label))
        };
        let crew_head = section("Crew")
            .when(working > 0, |d| {
                d.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(5.))
                        .text_size(px(11.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(c(ui::t_blue()))
                        .child(div().size(px(6.)).rounded_full().bg(c(ui::BLUE)))
                        .child(format!("{working} working")),
                )
            })
            .child(
                div()
                    .id("side-add")
                    .size(px(22.))
                    .rounded(px(6.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(c(ui::muted()))
                    .cursor_pointer()
                    .hover(|s| s.bg(c(ui::hover())).text_color(c(ui::ink())))
                    .tooltip(|window, cx| {
                        gpui_kit::component::tooltip::Tooltip::new("Add or make a bot")
                            .build(window, cx)
                    })
                    .child(Icon::new(IconName::Plus).size(px(14.)))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.maker_open = true;
                        this.go(Place::Crew, cx)
                    })),
            );

        // Under the crew, one quiet row: the rest of the crew when some is
        // folded away, or a way to add a bot when all of it is in view.
        let crew_more = div()
            .id("side-more")
            .mt(px(2.))
            .h(px(28.))
            .px(px(8.))
            .flex()
            .items_center()
            .gap(px(10.))
            .rounded(px(7.))
            .text_size(px(12.5))
            .text_color(c(ui::faint()))
            .cursor_pointer()
            .hover(|s| s.bg(ca(ui::hover(), 0.7)).text_color(c(ui::body())))
            .map(|d| {
                if more > 0 {
                    d.child(
                        div()
                            .w(px(28.))
                            .flex()
                            .justify_center()
                            .child(Icon::new(IconName::Users).size(px(14.))),
                    )
                    .child(div().flex_1().child(format!("{more} more in the crew")))
                    .child(Icon::new(IconName::ChevronRight).size(px(13.)))
                    .on_click(cx.listener(|this, _, _, cx| this.go(Place::Crew, cx)))
                } else {
                    d.child(
                        div()
                            .w(px(28.))
                            .flex()
                            .justify_center()
                            .child(Icon::new(IconName::Plus).size(px(14.))),
                    )
                    .child("Add a bot")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.maker_open = true;
                        this.go(Place::Crew, cx)
                    }))
                }
            });

        // Settings.
        let settings = row(
            "side-settings",
            IconName::Settings,
            "Settings",
            matches!(place, Place::Settings(_)),
        )
        .on_click(cx.listener(|this, _, _, cx| {
            let pane = this.settings_pane;
            this.go(Place::Settings(pane), cx)
        }));

        let guide = self.getting_started(cx);
        let foot = div()
            .mt_auto()
            .pt(px(14.))
            .flex()
            .flex_col()
            .gap(px(6.))
            .children(guide)
            .child(settings);

        div()
            .id("sidebar")
            .w(px(WIDTH))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            // On a Mac the window buttons sit in the top-left corner.
            .pt(px(if cfg!(target_os = "macos") { 40. } else { 12. }))
            .pb(px(10.))
            .px(px(10.))
            .bg(c(ui::bg()))
            .border_r_1()
            .border_color(c(ui::line2()))
            .overflow_y_scroll()
            .child(head)
            .child(search)
            .child(nav)
            .child(crew_head)
            .child(crew)
            .child(crew_more)
            .child(foot)
    }

    /// The top bar over every page: where you are, the engine, and a new job.
    pub(super) fn topbar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let folder = self
            .crew
            .store
            .folder()
            .map(|f| f.name.clone())
            .unwrap_or_else(|| "Squad Code".into());
        let here: String = match self.place {
            Place::Today => "Jobs".into(),
            Place::Inbox => "Needs you".into(),
            Place::History => "All jobs".into(),
            Place::Chat(m) => format!("Crew / {}", m.name()),
            Place::Crew => "Crew".into(),
            Place::Live(id) => match self.crew.store.job(id) {
                Some(j) => format!("Jobs / #{id} {}", j.title),
                None => format!("Jobs / #{id}"),
            },
            Place::Race(id) => match self.crew.store.job(id) {
                Some(j) => format!("Jobs / Race / {}", j.title),
                None => "Jobs / Race".into(),
            },
            Place::Goal(id) => match self.crew.store.goals.iter().find(|g| g.id == id) {
                Some(g) => format!("Jobs / Goal / {}", g.text),
                None => "Jobs / Goal".into(),
            },
            Place::Settings(pane) => format!("Settings / {}", pane.name()),
            Place::Usage => "Usage".into(),
            Place::Welcome => "Welcome".into(),
        };
        let running = self.crew.store.running().count();
        div()
            .h(px(52.))
            .flex_none()
            .w_full()
            .flex()
            .items_center()
            .gap(px(10.))
            .px(px(24.))
            .bg(c(ui::panel()))
            .border_b_1()
            .border_color(c(ui::line()))
            .text_size(px(13.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .min_w_0()
                    .text_color(c(ui::muted()))
                    .child(folder)
                    .child(div().text_color(c(ui::faint())).child("/"))
                    .child(
                        div()
                            .text_color(c(ui::ink()))
                            .font_weight(FontWeight::SEMIBOLD)
                            .truncate()
                            .child(here),
                    ),
            )
            .child(div().flex_1())
            .when(running > 0, |d| {
                d.child(
                    div()
                        .h(px(24.))
                        .px(px(9.))
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .rounded(px(6.))
                        .bg(c(ui::run_tone().1))
                        .border_1()
                        .border_color(c(ui::run_tone().2))
                        .text_size(px(12.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(c(ui::t_blue()))
                        .child(div().size(px(7.)).rounded_full().bg(c(ui::t_blue())))
                        .child(format!("{running} running")),
                )
            })
            .child(
                div()
                    .id("top-new")
                    .h(px(30.))
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .rounded(px(7.))
                    .bg(c(ui::fill()))
                    .text_color(c(ui::on_fill()))
                    .text_size(px(13.))
                    .font_weight(FontWeight::MEDIUM)
                    .cursor_pointer()
                    .hover(|s| s.bg(c(ui::fill_hover())))
                    .child("New job")
                    .on_click(cx.listener(|this, _, _, cx| this.go(Place::Today, cx))),
            )
    }
}

/// A small keyboard hint, like ⌘K.
fn kbd(text: &'static str) -> Div {
    div()
        .px(px(5.))
        .rounded(px(4.))
        .border_1()
        .border_color(c(ui::line()))
        .bg(c(ui::panel()))
        .font_family(ui::MONO)
        .text_size(px(10.5))
        .text_color(c(ui::faint()))
        .child(text)
}

fn capital(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

/// A path with the home folder written as `~`.
fn short_path(path: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    match path.strip_prefix(&home) {
        Some(rest) if !home.is_empty() => format!("~{rest}"),
        _ => path.to_string(),
    }
}

/// How many bots the sidebar lists before folding the rest into "+N more".
pub(super) const SIDEBAR_BOTS: usize = 6;

/// The order the sidebar lists bots in: working first, then the ones waiting on
/// you, then pinned bots, then the rest of the crew as you arranged it.
pub(super) fn crew_order(
    all: &[Member],
    busy: impl Fn(Member) -> bool,
    asking: impl Fn(Member) -> bool,
    pinned: &[Member],
) -> Vec<Member> {
    let rank = |m: Member| -> u8 {
        if busy(m) {
            0
        } else if asking(m) {
            1
        } else if pinned.contains(&m) {
            2
        } else {
            3
        }
    };
    let mut v = all.to_vec();
    // Stable, so bots of the same rank keep the crew's own order.
    v.sort_by_key(|m| rank(*m));
    v
}

impl Shell {
    /// The bots the sidebar shows and how many more are folded away.
    fn sidebar_crew(&self) -> (Vec<Member>, usize) {
        let all = Member::all();
        let order = crew_order(
            &all,
            |m| self.crew.store.busy(m).is_some(),
            |m| {
                self.crew
                    .store
                    .jobs
                    .iter()
                    .any(|j| j.bot == m && j.needs_you())
            },
            &self.crew.store.pinned,
        );
        // Everything that's working or waiting on you stays in view.
        let must = order
            .iter()
            .take_while(|m| {
                self.crew.store.busy(**m).is_some()
                    || self
                        .crew
                        .store
                        .jobs
                        .iter()
                        .any(|j| j.bot == **m && j.needs_you())
            })
            .count();
        let cap = SIDEBAR_BOTS.max(must);
        // Folding one bot away saves nothing: show it instead of "+1 more".
        if order.len() <= cap + 1 {
            return (order, 0);
        }
        let more = order.len() - cap;
        (order.into_iter().take(cap).collect(), more)
    }
}

#[cfg(test)]
mod tests {
    use super::{crew_order, Member};

    #[test]
    fn busy_bots_come_first_then_waiting_then_pinned() {
        use Member::*;
        let all = [Builder, Checker, Guard, Fixer, Explainer, Keeper];
        let order = crew_order(&all, |m| m == Fixer, |m| m == Keeper, &[Guard]);
        assert_eq!(
            order,
            vec![Fixer, Keeper, Guard, Builder, Checker, Explainer]
        );
    }
}
