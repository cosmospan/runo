//! The sidebar: the folder you're in, search, the places, and the crew.
//!
//! The crew lives here, one row per bot: its face with a status badge, its name
//! and what it's doing right now, with a thin progress bar while it works. It
//! replaces the dock, so the bots are always in view and never sit on top of a
//! page.
//!
//! The look: no ground of its own (it sits on the window's, beside the page
//! canvas), rows without boxes, and the place you're on lifted out in the
//! canvas's colour with a short violet bar on its left edge. Status colours only show where something is happening.

use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::ui::{self, c};
use super::{Place, Shell};

/// The sidebar's width.
pub(super) const WIDTH: f32 = 64.;

/// The sidebar's ground, a shade darker than the page so the page reads as
/// the thing in front.
fn ground() -> u32 {
    ui::ground()
}

impl Shell {
    pub(super) fn sidebar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let place = self.place;
        let inbox = self.inbox_count();
        self.inbox_seen = inbox;
        let working = self.crew.store.running().count();
        let folder = self.crew.store.folder().cloned();
        let letter = folder
            .as_ref()
            .and_then(|f| f.name.chars().next())
            .unwrap_or('S')
            .to_uppercase()
            .to_string();
        let folder_name = folder
            .as_ref()
            .map(|f| format!("{} · {}", f.name, short_path(&f.path)))
            .unwrap_or_else(|| "No folder yet".into());

        // The folder you're in, as a tile at the top of the rail.
        let head = div()
            .id("side-folder")
            .size(px(36.))
            .mb(px(14.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(10.))
            .bg(c(ui::fill()))
            .text_color(c(ui::on_fill()))
            .text_size(px(15.))
            .font_weight(FontWeight::BOLD)
            .shadow(ui::lift())
            .cursor_pointer()
            .hover(|s| s.opacity(0.9))
            .child(letter)
            .tooltip(move |window, cx| {
                gpui_kit::component::tooltip::Tooltip::new(folder_name.clone()).build(window, cx)
            })
            .on_click(cx.listener(|this, _, _, cx| this.go(Place::Today, cx)));

        // One place: an icon in a rounded square, lit when you're on it, with
        // an optional count badge on its corner.
        let item = |id: &'static str,
                    icon: IconName,
                    tip: &'static str,
                    on: bool,
                    badge: Option<(usize, u32)>| {
            div()
                .id(id)
                .relative()
                .size(px(40.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(10.))
                .cursor_pointer()
                .map(|d| {
                    if on {
                        d.bg(c(ui::bg()))
                            .shadow(ui::chip())
                            .text_color(c(ui::ink()))
                            .child(
                                div()
                                    .absolute()
                                    .left(px(-12.))
                                    .top(px(10.))
                                    .bottom(px(10.))
                                    .w(px(3.))
                                    .rounded_r(px(3.))
                                    .bg(c(ui::ink())),
                            )
                    } else {
                        d.text_color(c(ui::muted()))
                            .hover(|s| s.bg(c(ui::hover())).text_color(c(ui::ink())))
                    }
                })
                .child(Icon::new(icon).size(px(18.)))
                .when_some(badge, |d, (n, color)| {
                    d.child(
                        div()
                            .absolute()
                            .top(px(2.))
                            .right(px(1.))
                            .min_w(px(16.))
                            .h(px(16.))
                            .px(px(4.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .bg(c(color))
                            .border_2()
                            .border_color(c(ground()))
                            .text_size(px(9.5))
                            .font_weight(FontWeight::BOLD)
                            .text_color(c(0x0B0B0E))
                            .child(n.min(99).to_string()),
                    )
                })
                .tooltip(move |window, cx| {
                    gpui_kit::component::tooltip::Tooltip::new(tip).build(window, cx)
                })
        };
        let jobs_on = matches!(
            place,
            Place::Today | Place::History | Place::Live(_) | Place::Race(_) | Place::Goal(_)
        );
        let nav = div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(6.))
            .child(
                item(
                    "side-jobs",
                    IconName::LayoutDashboard,
                    "Jobs",
                    jobs_on,
                    None,
                )
                .on_click(cx.listener(|this, _, _, cx| this.go(Place::Today, cx))),
            )
            .child(
                item(
                    "side-inbox",
                    IconName::Inbox,
                    "Needs you",
                    place == Place::Inbox,
                    (inbox > 0).then_some((inbox, ui::AMBER)),
                )
                .on_click(cx.listener(|this, _, _, cx| this.go(Place::Inbox, cx))),
            )
            .child(
                item(
                    "side-crew",
                    IconName::Users,
                    "Crew",
                    matches!(place, Place::Crew | Place::Chat(_)),
                    (working > 0).then_some((working, ui::BLUE)),
                )
                .on_click(cx.listener(|this, _, _, cx| this.go(Place::Crew, cx))),
            )
            .child(
                item(
                    "side-term",
                    IconName::SquareTerminal,
                    "Terminal",
                    self.term_open.is_some_and(super::term::is_project),
                    None,
                )
                .on_click(cx.listener(|this, _, window, cx| this.toggle_project_term(window, cx))),
            )
            .child(
                item(
                    "side-usage",
                    IconName::ChartColumn,
                    "Usage",
                    place == Place::Usage,
                    None,
                )
                .on_click(cx.listener(|this, _, _, cx| this.go(Place::Usage, cx))),
            );

        let settings = item(
            "side-settings",
            IconName::Settings,
            "Settings",
            matches!(place, Place::Settings(_)),
            None,
        )
        .on_click(cx.listener(|this, _, _, cx| {
            let pane = this.settings_pane;
            this.go(Place::Settings(pane), cx)
        }));

        div()
            .id("sidebar")
            .w(px(WIDTH))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .items_center()
            // On a Mac the window buttons sit in the top-left corner.
            .pt(px(if cfg!(target_os = "macos") { 40. } else { 14. }))
            .pb(px(12.))
            .bg(c(ground()))
            .border_r_1()
            .border_color(c(ui::line2()))
            .child(head)
            .child(nav)
            .child(div().flex_1())
            .child(settings)
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
        // Each step of the path after a chevron; the last, where you are, in full ink.
        let parts: Vec<&str> = here.split(" / ").collect();
        let last = parts.len() - 1;
        let crumbs: Vec<AnyElement> = parts
            .iter()
            .enumerate()
            .flat_map(|(i, part)| {
                let chev = Icon::new(IconName::ChevronRight)
                    .size(px(14.))
                    .flex_none()
                    .text_color(c(ui::faint()))
                    .into_any_element();
                let text = div()
                    .truncate()
                    .map(|d| {
                        if i == last {
                            d.min_w_0()
                                .text_color(c(ui::ink()))
                                .font_weight(FontWeight::SEMIBOLD)
                        } else {
                            d.flex_none().text_color(c(ui::muted()))
                        }
                    })
                    .child(part.to_string())
                    .into_any_element();
                [chev, text]
            })
            .collect();
        let mono = folder
            .chars()
            .next()
            .unwrap_or('S')
            .to_uppercase()
            .to_string();
        div()
            .h(px(54.))
            .flex_none()
            .w_full()
            .flex()
            .items_center()
            .gap(px(10.))
            .pl(px(20.))
            .pr(px(14.))
            .border_b_1()
            .border_color(c(ui::line2()))
            .text_size(px(13.))
            // Where you are: the folder as a small tile, then the place.
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .min_w_0()
                    .child(
                        div()
                            .h(px(24.))
                            .flex()
                            .flex_none()
                            .items_center()
                            .gap(px(8.))
                            .text_color(c(ui::muted()))
                            .child(
                                div()
                                    .size(px(20.))
                                    .rounded(px(5.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .bg(c(ui::raised()))
                                    .border_1()
                                    .border_color(c(ui::line()))
                                    .text_size(px(10.5))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(c(ui::ink()))
                                    .child(mono),
                            )
                            .child(folder),
                    )
                    .children(crumbs),
            )
            .child(div().flex_1())
            .when(running > 0, |d| {
                d.child(
                    div()
                        .h(px(26.))
                        .px(px(9.))
                        .flex()
                        .items_center()
                        .gap(px(7.))
                        .rounded(px(7.))
                        .bg(c(ui::run_tone().1))
                        .border_1()
                        .border_color(c(ui::run_tone().2))
                        .text_size(px(12.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(c(ui::t_blue()))
                        .child(div().size(px(6.)).rounded_full().bg(c(ui::t_blue())))
                        .child(format!("{running} running")),
                )
            })
            // Search, as a round button: the palette finds places, bots and jobs.
            .child(
                div()
                    .id("top-search")
                    .size(px(30.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(8.))
                    .border_1()
                    .border_color(c(ui::line()))
                    .text_color(c(ui::muted()))
                    .cursor_pointer()
                    .hover(|s| s.bg(c(ui::hover())).text_color(c(ui::ink())))
                    .child(Icon::new(IconName::Search).size(px(14.)))
                    .on_click(cx.listener(|this, _, window, cx| this.open_palette(window, cx))),
            )
            .child(
                ui::solid("top-new", "New job")
                    .h(px(30.))
                    .pl(px(10.))
                    .child(Icon::new(IconName::Plus).size(px(14.)))
                    .flex_row_reverse()
                    .on_click(cx.listener(|this, _, _, cx| this.go(Place::Today, cx))),
            )
    }
}

/// A path with the home folder written as `~`.
fn short_path(path: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    match path.strip_prefix(&home) {
        Some(rest) if !home.is_empty() => format!("~{rest}"),
        _ => path.to_string(),
    }
}
