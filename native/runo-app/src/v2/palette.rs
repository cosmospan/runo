//! The command palette (⌘K): one box to jump anywhere, open any job, hand a
//! bot a job or switch folders, by typing a few letters.

use gpui_kit::assets::IconName;
use gpui_kit::component::input::Input;
use gpui_kit::component::Icon;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::data::{Member, Mode};
use super::store::Status;
use super::ui::{self, c, ca};
use super::{Place, Shell};

/// What picking an entry does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Act {
    Go(Place),
    Job(Mode),
    Folder(usize),
    AddFolder,
    V1,
}

pub struct Entry {
    pub label: String,
    /// What kind of thing it is, shown on the right ("Place", "Job", "⌘1").
    pub hint: String,
    pub icon: Option<IconName>,
    pub bot: Option<Member>,
    pub act: Act,
}

/// Every word typed appears somewhere in the label or hint.
pub fn matches(query: &str, label: &str, hint: &str) -> bool {
    let hay = format!("{label} {hint}").to_lowercase();
    query
        .split_whitespace()
        .all(|w| hay.contains(&w.to_lowercase()))
}

impl Shell {
    pub(super) fn palette_entries(&self, query: &str) -> Vec<Entry> {
        let mut all = vec![];
        let places = [
            (Place::Today, IconName::House, "Today", "⌘1"),
            (Place::Inbox, IconName::Inbox, "Inbox", "⌘2"),
            (Place::History, IconName::Clock, "History", "⌘3"),
            (Place::Crew, IconName::Users, "Crew", "⌘5"),
            (Place::Engines, IconName::Cpu, "Engines", "⌘6"),
        ];
        for (p, icon, label, key) in places {
            all.push(Entry {
                label: format!("Go to {label}"),
                hint: key.into(),
                icon: Some(icon),
                bot: None,
                act: Act::Go(p),
            });
        }
        for who in Member::all() {
            all.push(Entry {
                label: format!("Chat with {}", who.name()),
                hint: who.role().into(),
                icon: None,
                bot: Some(who),
                act: Act::Go(Place::Chat(who)),
            });
        }
        for mode in Mode::ALL.into_iter().filter(|m| *m != Mode::Ask) {
            let Some(who) = Member::doer_for(mode) else {
                continue;
            };
            all.push(Entry {
                label: format!("{}: give {} a job", mode.label(), who.name()),
                hint: "New job".into(),
                icon: None,
                bot: Some(who),
                act: Act::Job(mode),
            });
        }
        for j in self.store.jobs.iter().rev().take(12) {
            let state = match j.status {
                Status::Running => "Working",
                _ if j.needs_you() => "New",
                _ => j.status.label(),
            };
            all.push(Entry {
                label: j.title.clone(),
                hint: format!("{} · {state}", j.bot.name()),
                icon: None,
                bot: Some(j.bot),
                act: Act::Go(Place::Live(j.id)),
            });
        }
        for (i, f) in self.store.folders.iter().enumerate() {
            let on = i == self.store.current;
            all.push(Entry {
                label: format!("Work in {}", f.name),
                hint: if on {
                    "Current folder".into()
                } else {
                    "Folder".into()
                },
                icon: Some(IconName::Folder),
                bot: None,
                act: Act::Folder(i),
            });
        }
        all.push(Entry {
            label: "Add a folder…".into(),
            hint: "Folder".into(),
            icon: Some(IconName::FolderOpen),
            bot: None,
            act: Act::AddFolder,
        });
        all.push(Entry {
            label: "Open Runo v1".into(),
            hint: "Window".into(),
            icon: Some(IconName::ExternalLink),
            bot: None,
            act: Act::V1,
        });
        all.retain(|e| matches(query, &e.label, &e.hint));
        all
    }

    pub(super) fn open_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.palette = true;
        self.palette_sel = 0;
        self.palette_at = std::time::Instant::now();
        self.focus_moved = std::time::Instant::now();
        self.folder_menu = false;
        self.palette_input.update(cx, |i, cx| {
            i.set_value("", window, cx);
            i.focus(window, cx);
        });
        cx.notify();
    }

    pub(super) fn close_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.palette = false;
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn run_palette(&mut self, act: Act, window: &mut Window, cx: &mut Context<Self>) {
        self.close_palette(window, cx);
        match act {
            Act::Go(p) => self.go(p, cx),
            Act::Job(mode) => {
                self.set_mode(mode, window, cx);
                self.go(Place::Today, cx);
                self.composer.update(cx, |i, cx| i.focus(window, cx));
                self.focus_moved = std::time::Instant::now();
            }
            Act::Folder(i) => {
                self.store.current = i;
                self.store.save();
                if let Some(f) = self.store.folder() {
                    let name = f.name.clone();
                    self.say(format!("The crew now works in {name}."));
                }
            }
            Act::AddFolder => self.pick_folder(cx),
            Act::V1 => cx.defer(crate::open_main_window),
        }
        cx.notify();
    }

    /// Up, Down, Enter and Escape while the palette is open.
    pub(super) fn palette_key(
        &mut self,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let query = self.palette_input.read(cx).value().to_string();
        let n = self.palette_entries(&query).len();
        match key {
            "escape" => self.close_palette(window, cx),
            "down" if n > 0 => self.palette_sel = (self.palette_sel + 1) % n,
            "up" if n > 0 => self.palette_sel = (self.palette_sel + n - 1) % n,
            "enter" => {
                if let Some(e) = self.palette_entries(&query).get(self.palette_sel) {
                    let act = e.act;
                    self.run_palette(act, window, cx);
                }
            }
            _ => return false,
        }
        cx.notify();
        true
    }

    pub(super) fn palette_view(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let query = self.palette_input.read(cx).value().to_string();
        let entries = self.palette_entries(&query);
        let sel = self.palette_sel.min(entries.len().saturating_sub(1));
        let k = runo_holo::anim::spring(self.palette_at.elapsed().as_secs_f64(), 0.22) as f32;
        let mut list = div()
            .id("palette-list")
            .max_h(px(420.))
            .overflow_y_scroll()
            .p(px(6.))
            .flex()
            .flex_col();
        if entries.is_empty() {
            list = list.child(
                div()
                    .px(px(14.))
                    .py(px(18.))
                    .text_size(px(13.))
                    .text_color(c(ui::FAINT))
                    .child("Nothing matches. Try a bot, a job or a place."),
            );
        }
        for (i, e) in entries.into_iter().enumerate() {
            let on = i == sel;
            // Rows ripple in, a few milliseconds apart.
            let rk = runo_holo::anim::spring(
                (self.palette_at.elapsed().as_secs_f64() - 0.02 * i.min(10) as f64).max(0.0),
                0.24,
            ) as f32;
            let lead = match (e.bot, e.icon) {
                (Some(who), _) => self.avatar(who, 26.).into_any_element(),
                (None, Some(icon)) => div()
                    .size(px(26.))
                    .rounded(px(8.))
                    .bg(c(if on { 0xFFFFFF } else { 0xF1F2F7 }))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(Icon::new(icon).size(px(14.)).text_color(c(ui::MUTED)))
                    .into_any_element(),
                _ => div().size(px(26.)).into_any_element(),
            };
            let act = e.act;
            list = list.child(
                div()
                    .id(("pal", i))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .px(px(10.))
                    .h(px(42.))
                    .rounded(px(12.))
                    .opacity(rk)
                    .relative()
                    .left(px(6. * (1. - rk)))
                    .cursor_pointer()
                    .when(on, |d| d.bg(c(0xF1F2F8)))
                    .on_mouse_move(cx.listener(move |this, _: &MouseMoveEvent, _, cx| {
                        if this.palette_sel != i {
                            this.palette_sel = i;
                            cx.notify();
                        }
                    }))
                    .on_click(
                        cx.listener(move |this, _, window, cx| this.run_palette(act, window, cx)),
                    )
                    .child(lead)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(13.5))
                            .font_weight(if on {
                                FontWeight::SEMIBOLD
                            } else {
                                FontWeight::NORMAL
                            })
                            .child(e.label),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(11.5))
                            .text_color(c(ui::FAINT))
                            .child(e.hint),
                    )
                    .when(on, |d| {
                        d.child(
                            Icon::new(IconName::CornerDownLeft)
                                .size(px(13.))
                                .text_color(c(ui::MUTED)),
                        )
                    }),
            );
        }
        let scrim = div()
            .id("palette-scrim")
            .absolute()
            .inset_0()
            // No dimming: GPUI orders some bot frames above a full-window wash, so
            // the card stands out by its shadow instead.
            .occlude()
            .on_click(cx.listener(|this, _, window, cx| this.close_palette(window, cx)));
        let card = div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .flex()
            .justify_center()
            .items_start()
            .child(
                div()
                    .id("palette-card")
                    .mt(px(110. + 14. * (1. - k)))
                    .w(px(620.))
                    .h_auto()
                    .max_h(px(520.))
                    .opacity(k)
                    .rounded(px(20.))
                    .bg(c(0xFFFFFF))
                    .border_1()
                    .border_color(c(ui::LINE))
                    .shadow(vec![BoxShadow {
                        color: ca(0x28285A, 0.30),
                        offset: point(px(0.), px(30.)),
                        blur_radius: px(80.),
                        spread_radius: px(-6.),
                        inset: false,
                    }])
                    .overflow_hidden()
                    .flex()
                    .flex_col()
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .px(px(16.))
                            .h(px(56.))
                            .border_b_1()
                            .border_color(c(ui::LINE))
                            .child(
                                Icon::new(IconName::Search)
                                    .size(px(17.))
                                    .text_color(c(ui::FAINT)),
                            )
                            .child(
                                div().flex_1().text_size(px(15.)).child(
                                    Input::new(&self.palette_input)
                                        .appearance(false)
                                        .bordered(false),
                                ),
                            )
                            .child(
                                div()
                                    .px(px(7.))
                                    .py(px(2.))
                                    .rounded(px(6.))
                                    .bg(c(0xF1F2F7))
                                    .font_family(ui::MONO)
                                    .text_size(px(10.))
                                    .text_color(c(ui::MUTED))
                                    .child("esc"),
                            ),
                    )
                    .child(list)
                    .child(
                        div()
                            .flex()
                            .gap(px(16.))
                            .px(px(16.))
                            .h(px(36.))
                            .items_center()
                            .border_t_1()
                            .border_color(c(ui::LINE))
                            .text_size(px(11.))
                            .text_color(c(ui::FAINT))
                            .child("↑↓ to move")
                            .child("↵ to open")
                            .child("⌘[ to go back"),
                    ),
            );
        div()
            .absolute()
            .inset_0()
            .child(deferred(scrim).with_priority(3))
            .child(deferred(card).with_priority(4))
    }
}

#[cfg(test)]
mod tests {
    use super::matches;

    #[test]
    fn every_word_must_match() {
        assert!(matches("", "Go to Inbox", "⌘2"));
        assert!(matches("inb", "Go to Inbox", "⌘2"));
        assert!(matches("chat fix", "Chat with Fixer", "Hunts the bug"));
        assert!(!matches("chat guard", "Chat with Fixer", "Hunts the bug"));
        assert!(matches("BUILD", "Build: give Builder a job", "New job"));
    }
}
