//! The sidebar: the folder you're in, search, the places, and the crew.
//!
//! The crew lives here, one row per bot: its bubble, its name and what it's
//! doing right now, with a progress bar while it works. It replaces the dock,
//! so the bots are always in view and never sit on top of a page.

use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use runo_holo::Framing;

use super::data::Member;
use super::ui::{self, c};
use super::{Place, Shell};

/// The sidebar's width.
pub(super) const WIDTH: f32 = 252.;

/// The sidebar's ground, a shade off the page.
const GROUND: u32 = 0xFBFBFC;

impl Shell {
    pub(super) fn sidebar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let place = self.place;
        let inbox = self.inbox_count();
        self.inbox_seen = inbox;
        let jobs = self.store.jobs.len();
        let folder = self.store.folder().cloned();

        // The folder you're working in, at the top.
        let (name, path) = match &folder {
            Some(f) => (f.name.clone(), short_path(&f.path)),
            None => ("No folder".to_string(), "Pick one on Home".to_string()),
        };
        let head = div()
            .id("side-folder")
            .flex()
            .items_center()
            .gap(px(10.))
            .px(px(8.))
            .py(px(6.))
            .rounded(px(8.))
            .cursor_pointer()
            .hover(|s| s.bg(c(0xF2F2F4)))
            .child(
                div()
                    .size(px(26.))
                    .flex_none()
                    .rounded(px(7.))
                    .bg(c(ui::INK))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(13.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(c(0xFFFFFF))
                    .child(name.chars().next().unwrap_or('R').to_uppercase().to_string()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .text_size(px(13.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .truncate()
                            .child(name),
                    )
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(c(ui::FAINT))
                            .truncate()
                            .child(path),
                    ),
            )
            .child(
                Icon::new(IconName::ChevronsUpDown)
                    .size(px(14.))
                    .text_color(c(ui::FAINT)),
            )
            .on_click(cx.listener(|this, _, _, cx| this.go(Place::Today, cx)));

        // Search: the palette finds places, bots, jobs and folders.
        let search = div()
            .id("side-search")
            .mt(px(12.))
            .mb(px(10.))
            .mx(px(2.))
            .h(px(32.))
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(8.))
            .rounded(px(8.))
            .bg(c(0xFFFFFF))
            .border_1()
            .border_color(c(ui::LINE))
            .text_size(px(13.))
            .text_color(c(ui::FAINT))
            .cursor_pointer()
            .hover(|s| s.border_color(c(0xD4D4DA)))
            .child(Icon::new(IconName::Search).size(px(15.)))
            .child(div().flex_1().child("Search or run a command"))
            .child(kbd("⌘K"))
            .on_click(cx.listener(|this, _, window, cx| this.open_palette(window, cx)));

        let row = |id: &'static str, icon: IconName, label: &'static str, on: bool| {
            div()
                .id(id)
                .h(px(32.))
                .px(px(10.))
                .flex()
                .items_center()
                .gap(px(10.))
                .rounded(px(7.))
                .text_size(px(13.))
                .font_weight(FontWeight::MEDIUM)
                .cursor_pointer()
                .map(|d| {
                    if on {
                        d.bg(c(0xFFFFFF))
                            .border_1()
                            .border_color(c(ui::LINE))
                            .text_color(c(ui::INK))
                    } else {
                        d.text_color(c(0x3F3F46))
                            .hover(|s| s.bg(c(0xF2F2F4)))
                    }
                })
                .child(Icon::new(icon).size(px(16.)))
                .child(div().flex_1().child(label))
        };
        let count = |n: usize, hot: bool| {
            div()
                .text_size(px(11.5))
                .font_weight(FontWeight::MEDIUM)
                .map(|d| {
                    if hot {
                        d.px(px(6.))
                            .rounded_full()
                            .bg(c(0xE5484D))
                            .text_color(c(0xFFFFFF))
                    } else {
                        d.text_color(c(ui::MUTED))
                    }
                })
                .child(n.min(999).to_string())
        };
        let on_chat = matches!(place, Place::Chat(_));
        let nav = div()
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(
                row("side-home", IconName::House, "Home", place == Place::Today)
                    .on_click(cx.listener(|this, _, _, cx| this.go(Place::Today, cx))),
            )
            .child(
                row("side-inbox", IconName::Inbox, "Inbox", place == Place::Inbox)
                    .when(inbox > 0, |d| d.child(count(inbox, true)))
                    .on_click(cx.listener(|this, _, _, cx| this.go(Place::Inbox, cx))),
            )
            .child(
                row(
                    "side-jobs",
                    IconName::LayoutDashboard,
                    "Jobs",
                    matches!(place, Place::History | Place::Live(_)),
                )
                .when(jobs > 0, |d| d.child(count(jobs, false)))
                .on_click(cx.listener(|this, _, _, cx| this.go(Place::History, cx))),
            )
            .child(
                row("side-chats", IconName::MessageSquare, "Chats", on_chat).on_click(
                    cx.listener(|this, _, _, cx| {
                        let to = Place::Chat(this.last_chat());
                        this.go(to, cx)
                    }),
                ),
            )
            .child(
                row("side-crew", IconName::Users, "Crew", place == Place::Crew)
                    .on_click(cx.listener(|this, _, _, cx| this.go(Place::Crew, cx))),
            );

        // The crew: every bot, what it's doing, and where clicking takes you.
        let mut crew = div().flex().flex_col().gap(px(1.));
        for who in Member::all() {
            let mood = self.mood(who);
            let face = self.bot(who, mood, 30., Framing::Bust);
            let busy = self
                .store
                .busy(who)
                .map(|j| (j.id, j.title.clone(), j.progress()));
            let asking = self
                .store
                .jobs
                .iter()
                .any(|j| j.bot == who && j.needs_you());
            let (word, dot) = self.status_of(who);
            let line = match &busy {
                Some((_, title, Some((d, n)))) => format!("{title} · {d} of {n}"),
                Some((_, title, None)) => title.clone(),
                None if asking => "Needs you".to_string(),
                None => capital(&word),
            };
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
            let dot_color = if busy.is_some() {
                ui::BLUE
            } else if asking {
                ui::AMBER
            } else if dot == ui::FAINT {
                0xD4D4D8
            } else {
                dot
            };
            crew = crew.child(
                div()
                    .id(("side-bot", who.key() as usize))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .px(px(8.))
                    .py(px(5.))
                    .rounded(px(8.))
                    .cursor_pointer()
                    .map(|d| {
                        if here {
                            d.bg(c(0xFFFFFF)).border_1().border_color(c(ui::LINE))
                        } else {
                            d.hover(|s| s.bg(c(0xF2F2F4)))
                        }
                    })
                    .on_hover(cx.listener(move |this, on: &bool, _, cx| {
                        this.hover_bot(who, *on, cx);
                    }))
                    .child(face)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .truncate()
                                    .child(who.name()),
                            )
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .text_color(c(if asking && busy.is_none() {
                                        0xB45309
                                    } else {
                                        ui::MUTED
                                    }))
                                    .truncate()
                                    .child(line),
                            )
                            .map(|d| match bar {
                                Some((true, f)) => {
                                    d.child(div().mt(px(5.)).child(ui::bar(f, ui::BLUE).h(px(3.))))
                                }
                                Some((false, _)) => d.child(
                                    div().mt(px(5.)).child(ui::sweep(ui::BLUE, 0.).h(px(3.))),
                                ),
                                None => d,
                            }),
                    )
                    .when(bar.is_none(), |d| {
                        d.child(div().size(px(7.)).flex_none().rounded_full().bg(c(dot_color)))
                    })
                    .on_click(cx.listener(move |this, _, _, cx| this.go(to, cx))),
            );
        }

        let section = |label: &'static str| {
            div()
                .mt(px(20.))
                .mb(px(6.))
                .mx(px(10.))
                .flex()
                .items_center()
                .text_size(px(11.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(c(ui::FAINT))
                .child(div().flex_1().child(label))
        };

        let foot = div()
            .mt_auto()
            .pt(px(10.))
            .border_t_1()
            .border_color(c(ui::LINE))
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(
                row("side-engines", IconName::Cpu, "Engines", place == Place::Engines)
                    .on_click(cx.listener(|this, _, _, cx| this.go(Place::Engines, cx))),
            )
            .child(
                row("side-v1", IconName::PanelLeft, "Open Runo v1", false)
                    .on_click(cx.listener(|_, _, _, cx| cx.defer(crate::open_main_window))),
            );

        div()
            .id("sidebar")
            .w(px(WIDTH))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            // On a Mac the window buttons sit in the top-left corner.
            .pt(px(if cfg!(target_os = "macos") { 40. } else { 14. }))
            .pb(px(12.))
            .px(px(10.))
            .bg(c(GROUND))
            .border_r_1()
            .border_color(c(ui::LINE))
            .overflow_y_scroll()
            .child(head)
            .child(search)
            .child(nav)
            .child(
                section("CREW").child(
                    div()
                        .id("side-add")
                        .size(px(20.))
                        .rounded(px(5.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .hover(|s| s.bg(c(0xEDEDF0)).text_color(c(ui::INK)))
                        .tooltip(|window, cx| {
                            gpui_kit::component::tooltip::Tooltip::new("Add or make a bot")
                                .build(window, cx)
                        })
                        .child(Icon::new(IconName::Plus).size(px(13.)))
                        .on_click(cx.listener(|this, _, _, cx| this.go(Place::Crew, cx))),
                ),
            )
            .child(crew)
            .child(foot)
    }

    /// The top bar over every page: where you are, the engine, and a new job.
    pub(super) fn topbar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let folder = self
            .store
            .folder()
            .map(|f| f.name.clone())
            .unwrap_or_else(|| "Runo".into());
        let here: String = match self.place {
            Place::Today => "Home".into(),
            Place::Inbox => "Inbox".into(),
            Place::History => "Jobs".into(),
            Place::Chat(m) => format!("Chats / {}", m.name()),
            Place::Crew => "Crew".into(),
            Place::Live(id) => match self.store.job(id) {
                Some(j) => format!("Jobs / #{id} {}", j.title),
                None => format!("Jobs / #{id}"),
            },
            Place::Engines => "Engines".into(),
            Place::Welcome => "Welcome".into(),
        };
        let running = self.store.running().count();
        div()
            .h(px(52.))
            .flex_none()
            .w_full()
            .flex()
            .items_center()
            .gap(px(10.))
            .px(px(24.))
            .bg(c(0xFFFFFF))
            .border_b_1()
            .border_color(c(ui::LINE))
            .text_size(px(13.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .min_w_0()
                    .text_color(c(ui::MUTED))
                    .child(folder)
                    .child(div().text_color(c(ui::FAINT)).child("/"))
                    .child(
                        div()
                            .text_color(c(ui::INK))
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
                        .bg(c(0xEFF4FF))
                        .border_1()
                        .border_color(c(0xDCE6FE))
                        .text_size(px(12.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(c(0x2563EB))
                        .child(div().size(px(7.)).rounded_full().bg(c(0x2563EB)))
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
                    .bg(c(ui::INK))
                    .text_color(c(0xFFFFFF))
                    .text_size(px(13.))
                    .font_weight(FontWeight::MEDIUM)
                    .cursor_pointer()
                    .hover(|s| s.bg(c(0x2C2A38)))
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
        .border_color(c(ui::LINE))
        .bg(c(0xFFFFFF))
        .font_family(ui::MONO)
        .text_size(px(10.5))
        .text_color(c(ui::FAINT))
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
