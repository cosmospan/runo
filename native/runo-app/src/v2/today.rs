//! Today: the crew up top, one box to hand them a job, and what is moving:
//! what needs you, what is being worked on, and what just finished.

use gpui_kit::assets::IconName;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::Icon;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use runo_holo::{Framing, Mood};

use super::data::{self, Member, Mode};
use super::store::{self, Status};
use super::ui::{self, c, ca};
use super::{Place, Shell};

/// The person's first name, from the account they're signed in to.
fn first_name() -> Option<String> {
    let raw = std::env::var("RUNO_NAME")
        .ok()
        .filter(|n| !n.trim().is_empty())
        .or_else(full_name)
        .ok_or(std::env::VarError::NotPresent)
        .or_else(|_| std::env::var("USER"))
        .or_else(|_| std::env::var("USERNAME"))
        .ok()?;
    let word: String = raw
        .split(|c: char| !c.is_alphabetic())
        .find(|w| !w.is_empty())?
        .to_lowercase();
    if word.len() < 2 || ["root", "admin", "user", "runner"].contains(&word.as_str()) {
        return None;
    }
    let mut cs = word.chars();
    Some(cs.next()?.to_uppercase().chain(cs).collect())
}

/// The account's full name ("Anish T R"), which reads better than a login
/// name like "anishtr".
fn full_name() -> Option<String> {
    #[cfg(target_os = "macos")]
    let out = std::process::Command::new("id").arg("-F").output().ok()?;
    #[cfg(not(target_os = "macos"))]
    let out = {
        let user = std::env::var("USER").ok()?;
        std::process::Command::new("getent")
            .args(["passwd", &user])
            .output()
            .ok()?
    };
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    #[cfg(not(target_os = "macos"))]
    let text = text.split(':').nth(4)?.split(',').next()?.to_string();
    (!text.is_empty()).then_some(text)
}

fn greeting() -> String {
    use chrono::Timelike;
    let part = match chrono::Local::now().hour() {
        0..=4 => "Working late",
        5..=11 => "Good morning",
        12..=17 => "Good afternoon",
        _ => "Good evening",
    };
    match first_name() {
        Some(n) => format!("{part}, {n}."),
        None => format!("{part}."),
    }
}

impl Shell {
    pub(super) fn today(&mut self, cx: &mut Context<Self>) -> AnyElement {
        // The crew, a bot that waits on you bigger and in front.
        let waiting = Member::CREW.into_iter().find(|m| {
            self.store
                .jobs
                .iter()
                .any(|j| j.bot == *m && j.needs_you() && j.status != Status::Done)
        });
        let mut row = div()
            .flex_none()
            .h(px(186.))
            .flex()
            .items_end()
            .justify_center()
            .gap(px(18.));
        for who in Member::CREW {
            let asking = waiting == Some(who);
            let mood = if asking {
                Mood::NeedsYou
            } else {
                self.mood(who)
            };
            let size = if asking { 140. } else { 104. };
            let face = self.bot(who, mood, size, Framing::Full);
            let (line, color) = self.status_of(who);
            let to = self.store.busy(who).map(|j| j.id).or_else(|| {
                self.store
                    .jobs
                    .iter()
                    .rev()
                    .find(|j| j.bot == who && j.needs_you())
                    .map(|j| j.id)
            });
            row = row.child(
                div()
                    .id(("crew-top", who.key() as usize))
                    .on_hover(cx.listener(move |this, on: &bool, _, cx| {
                        this.hover_bot(who, *on, cx);
                    }))
                    .relative()
                    .w(px(size))
                    .flex()
                    .flex_col()
                    .items_center()
                    .cursor_pointer()
                    .when(asking, |d| {
                        d.child(
                            div()
                                .absolute()
                                .top(px(-4.))
                                .px(px(10.))
                                .py(px(5.))
                                .rounded(px(10.))
                                .bg(c(ui::INK))
                                .text_color(c(0xFFFFFF))
                                .text_size(px(12.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("Got a sec?"),
                        )
                    })
                    .child(face)
                    .child(
                        div()
                            .mt(px(-8.))
                            .text_size(px(14.))
                            .font_weight(FontWeight::BOLD)
                            .child(who.name()),
                    )
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(c(if color == ui::FAINT { ui::MUTED } else { color }))
                            .when(color != ui::FAINT, |d| d.font_weight(FontWeight::MEDIUM))
                            .child(line),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.go(to.map(Place::Live).unwrap_or(Place::Chat(who)), cx);
                    })),
            );
        }

        let folder = self.store.folder().map(|f| f.name.clone());
        let running = self.store.running().count();
        let today_done = self
            .store
            .jobs
            .iter()
            .filter(|j| {
                j.status == Status::Done
                    && j.ended
                        .is_some_and(|e| store::now().saturating_sub(e) < 86_400)
            })
            .count();
        let sub = match (&folder, running, today_done) {
            (None, ..) => "Pick a folder and hand the crew its first job.".to_string(),
            (Some(f), 0, 0) => format!("The crew is ready. What should we do in {f}?"),
            (Some(f), 0, d) => format!(
                "{d} {} finished today in {f}. Nothing is running.",
                plural(d, "job", "jobs")
            ),
            (Some(f), r, _) => format!("{r} {} running in {f}.", plural(r, "job is", "jobs are")),
        };
        let has_jobs = !self.store.jobs.is_empty();

        div()
            .id("today")
            .size_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .items_center()
            .px(px(40.))
            .pt(px(28.))
            .pb(px(40.))
            .child(row)
            .child(
                div()
                    .mt(px(14.))
                    .text_size(px(48.))
                    .font_weight(FontWeight::BOLD)
                    .line_height(px(56.))
                    .child(greeting()),
            )
            .child(
                div()
                    .mt(px(6.))
                    .text_size(px(15.))
                    .text_color(c(ui::MUTED))
                    .child(sub),
            )
            .child(self.composer_card(cx))
            .child(self.starters(cx))
            .child(if has_jobs {
                self.cards(cx).into_any_element()
            } else {
                how_it_works().into_any_element()
            })
            .into_any_element()
    }

    fn composer_card(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let target = Member::for_mode(self.mode);
        let face = self.avatar(target, 22.);
        let mut modes = div()
            .flex()
            .items_center()
            .p(px(3.))
            .gap(px(2.))
            .rounded(px(12.))
            .bg(c(0xF1F2F7));
        for (i, m) in Mode::ALL.into_iter().enumerate() {
            let on = self.mode == m;
            modes = modes.child(
                div()
                    .id(("mode", i))
                    .px(px(11.))
                    .h(px(30.))
                    .flex()
                    .items_center()
                    .rounded(px(9.))
                    .text_size(px(13.))
                    .cursor_pointer()
                    .map(|d| {
                        if on {
                            d.bg(c(0xFFFFFF))
                                .shadow(ui::lift())
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(c(ui::INK))
                        } else {
                            d.text_color(c(ui::MUTED))
                                .hover(|s| s.text_color(c(ui::INK)))
                        }
                    })
                    .child(m.label())
                    .on_click(cx.listener(move |this, _, window, cx| this.set_mode(m, window, cx))),
            );
        }
        let engine = super::engine::label(cx.global::<crate::settings::AppSettings>(), target);
        let folder = self.store.folder().map(|f| f.name.clone());
        let folder_chip = div()
            .id("folder")
            .flex()
            .items_center()
            .gap(px(6.))
            .px(px(11.))
            .h(px(32.))
            .rounded_full()
            .border_1()
            .border_color(c(if folder.is_some() {
                ui::LINE
            } else {
                ui::ORANGE
            }))
            .bg(c(0xFFFFFF))
            .text_size(px(13.))
            .font_weight(FontWeight::MEDIUM)
            .cursor_pointer()
            .hover(|s| s.bg(c(0xF6F7FB)))
            .child(
                Icon::new(IconName::Folder)
                    .size(px(14.))
                    .text_color(c(ui::MUTED)),
            )
            .child(folder.clone().unwrap_or_else(|| "Pick a folder".into()))
            .child(
                Icon::new(IconName::ChevronDown)
                    .size(px(12.))
                    .text_color(c(ui::FAINT)),
            )
            .on_click(cx.listener(|this, _, _, cx| {
                if this.store.folders.is_empty() {
                    this.pick_folder(cx);
                } else {
                    this.folder_menu = !this.folder_menu;
                    cx.notify();
                }
            }));
        let menu = self.folder_menu.then(|| {
            let mut list = div()
                .absolute()
                .top(px(38.))
                .left_0()
                .w(px(300.))
                .p(px(6.))
                .rounded(px(14.))
                .bg(c(0xFFFFFF))
                .border_1()
                .border_color(c(ui::LINE))
                .shadow(ui::lift())
                .flex()
                .flex_col();
            for (i, f) in self.store.folders.iter().enumerate() {
                let on = i == self.store.current;
                list = list.child(
                    div()
                        .id(("folder-item", i))
                        .px(px(10.))
                        .py(px(7.))
                        .rounded(px(9.))
                        .cursor_pointer()
                        .when(on, |d| d.bg(c(0xF1F2F7)))
                        .hover(|s| s.bg(c(0xF6F7FB)))
                        .child(
                            div()
                                .text_size(px(13.))
                                .font_weight(FontWeight::MEDIUM)
                                .child(f.name.clone()),
                        )
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(c(ui::FAINT))
                                .truncate()
                                .child(f.path.clone()),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.store.current = i;
                            this.folder_menu = false;
                            this.store.save();
                            cx.notify();
                        })),
                );
            }
            list.child(div().my(px(4.)).h(px(1.)).bg(c(ui::LINE)))
                .child(
                    div()
                        .id("folder-add")
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .px(px(10.))
                        .py(px(8.))
                        .rounded(px(9.))
                        .cursor_pointer()
                        .hover(|s| s.bg(c(0xF6F7FB)))
                        .text_size(px(13.))
                        .child(Icon::new(IconName::Plus).size(px(14.)))
                        .child("Add a folder…")
                        .on_click(cx.listener(|this, _, _, cx| this.pick_folder(cx))),
                )
        });
        ui::card()
            .mt(px(26.))
            .w(px(840.))
            .px(px(20.))
            .pt(px(16.))
            .pb(px(14.))
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(
                div().text_size(px(16.)).line_height(px(24.)).child(
                    Textarea::new(&self.composer)
                        .appearance(false)
                        .bordered(false),
                ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .relative()
                            .child(folder_chip)
                            .children(menu.map(|m| deferred(m).with_priority(1))),
                    )
                    .child(modes)
                    .child(div().flex_1())
                    .child(
                        div()
                            .id("goes-to")
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .pl(px(6.))
                            .pr(px(12.))
                            .h(px(32.))
                            .rounded_full()
                            .text_size(px(12.5))
                            .text_color(c(ui::MUTED))
                            .cursor_pointer()
                            .hover(|s| s.bg(c(0xF6F7FB)).text_color(c(ui::INK)))
                            .child(face)
                            .child(format!("{} · {engine}", target.name()))
                            .on_click(cx.listener(|this, _, _, cx| this.go(Place::Engines, cx))),
                    )
                    .child(
                        div()
                            .id("send")
                            .size(px(38.))
                            .rounded_full()
                            .bg(c(ui::INK))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .hover(|s| s.bg(c(0x2C2A38)))
                            .child(
                                Icon::new(IconName::ArrowUp)
                                    .size(px(16.))
                                    .text_color(c(0xFFFFFF)),
                            )
                            .on_click(cx.listener(|this, _, window, cx| {
                                let text = this.composer.read(cx).value().trim().to_string();
                                this.composer
                                    .update(cx, |i, cx| i.set_value("", window, cx));
                                this.hand_over(text, cx);
                            })),
                    ),
            )
    }

    fn starters(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut row = div()
            .mt(px(14.))
            .flex()
            .flex_wrap()
            .gap(px(8.))
            .justify_center()
            .max_w(px(900.));
        for (i, (mode, text)) in data::STARTERS.into_iter().enumerate() {
            let face = self.avatar(Member::for_mode(mode), 20.);
            row = row.child(
                div()
                    .id(("starter", i))
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .pl(px(8.))
                    .pr(px(13.))
                    .h(px(34.))
                    .rounded_full()
                    .bg(ca(0xFFFFFF, 0.7))
                    .border_1()
                    .border_color(c(ui::LINE))
                    .text_size(px(13.))
                    .text_color(c(ui::BODY))
                    .cursor_pointer()
                    .hover(|s| s.bg(c(0xFFFFFF)).text_color(c(ui::INK)))
                    .child(face)
                    .child(text)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.set_mode(mode, window, cx);
                        this.composer
                            .update(cx, |i, cx| i.set_value(text, window, cx));
                        cx.notify();
                    })),
            );
        }
        row
    }

    /// What needs you, what's running and what finished, as three cards.
    fn cards(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let needs: Vec<_> = self
            .store
            .jobs
            .iter()
            .rev()
            .filter(|j| j.needs_you())
            .take(4)
            .cloned()
            .collect();
        let running: Vec<_> = self.store.running().cloned().collect();
        let finished: Vec<_> = self
            .store
            .jobs
            .iter()
            .rev()
            .filter(|j| j.status == Status::Done && !j.needs_you())
            .take(4)
            .cloned()
            .collect();
        let mut needs_card = column_card("Needs you", needs.len(), ui::AMBER);
        if needs.is_empty() {
            needs_card = needs_card.child(empty_line("Nothing is waiting on you."));
        }
        for j in &needs {
            let face = self.avatar(j.bot, 26.);
            let (word, color) = match j.status {
                Status::Done => ("Done · have a look", ui::GREEN),
                Status::Failed => ("Stopped with a problem", ui::RED),
                _ => ("Interrupted", ui::AMBER),
            };
            needs_card = needs_card.child(job_row(j.id, face, &j.title, word, color, None, cx));
        }
        let mut work_card = column_card("Working now", running.len(), ui::ORANGE);
        if running.is_empty() {
            work_card =
                work_card.child(empty_line("No one is working. Hand them something above."));
        }
        for j in &running {
            let face = self.bot(j.bot, Mood::Working, 26., Framing::Bust);
            let frac = j.progress().map(|(d, n)| d as f32 / n.max(1) as f32);
            let doing = format!("{} · {}", j.doing(), store::span(j.seconds()));
            work_card = work_card.child(job_row(
                j.id,
                face,
                &j.title,
                &doing,
                j.bot.accent(),
                Some(frac),
                cx,
            ));
        }
        let mut done_card = column_card("Finished", finished.len(), ui::GREEN);
        if finished.is_empty() {
            done_card = done_card.child(empty_line("Finished jobs land here."));
        }
        for j in &finished {
            let face = self.avatar(j.bot, 26.);
            let when = match store::ago(j.ended.unwrap_or(j.started)).as_str() {
                "just now" => format!("just now · {}", j.bot.name()),
                a => format!("{a} ago · {}", j.bot.name()),
            };
            done_card = done_card.child(job_row(j.id, face, &j.title, &when, ui::MUTED, None, cx));
        }
        // Only the cards with something in them; one alone sits under the box.
        let all = done_card.child(
            div()
                .id("all-jobs")
                .mt(px(4.))
                .text_size(px(12.5))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(c(ui::ORANGE))
                .cursor_pointer()
                .hover(|s| s.text_color(c(ui::INK)))
                .child("All jobs →")
                .on_click(cx.listener(|this, _, _, cx| this.go(Place::History, cx))),
        );
        let shown: Vec<Div> = [
            (!needs.is_empty()).then_some(needs_card),
            (!running.is_empty()).then_some(work_card),
            (!finished.is_empty()).then_some(all),
        ]
        .into_iter()
        .flatten()
        .collect();
        let n = shown.len();
        div()
            .mt(px(36.))
            .w_full()
            .max_w(px(match n {
                1 => 840.,
                2 => 1040.,
                _ => 1240.,
            }))
            .flex()
            .gap(px(18.))
            .children(shown)
    }
}

fn plural<'a>(n: usize, one: &'a str, many: &'a str) -> &'a str {
    if n == 1 {
        one
    } else {
        many
    }
}

fn column_card(title: &'static str, count: usize, color: u32) -> Div {
    ui::card()
        .flex_1()
        .flex_basis(px(0.))
        .min_w_0()
        .min_h(px(150.))
        .p(px(20.))
        .flex()
        .flex_col()
        .gap(px(12.))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_size(px(14.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title),
                )
                .when(count > 0, |d| {
                    d.child(
                        div()
                            .px(px(8.))
                            .h(px(20.))
                            .flex()
                            .items_center()
                            .rounded_full()
                            .bg(ca(color, 0.12))
                            .text_color(c(color))
                            .text_size(px(11.5))
                            .font_weight(FontWeight::BOLD)
                            .child(count.to_string()),
                    )
                }),
        )
}

fn empty_line(text: &'static str) -> Div {
    div()
        .py(px(6.))
        .text_size(px(13.))
        .text_color(c(ui::FAINT))
        .child(text)
}

pub(super) fn job_row(
    id: u64,
    face: Img,
    title: &str,
    note: &str,
    color: u32,
    progress: Option<Option<f32>>,
    cx: &mut Context<Shell>,
) -> Stateful<Div> {
    div()
        .id(("job-row", id as usize))
        .flex()
        .items_center()
        .gap(px(12.))
        .mx(px(-8.))
        .px(px(8.))
        .py(px(6.))
        .rounded(px(12.))
        .cursor_pointer()
        .hover(|s| s.bg(c(0xF6F7FB)))
        .child(face)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(3.))
                .child(
                    div()
                        .text_size(px(13.5))
                        .font_weight(FontWeight::MEDIUM)
                        .truncate()
                        .child(title.to_string()),
                )
                .child(
                    div()
                        .text_size(px(11.5))
                        .text_color(c(color))
                        .truncate()
                        .child(note.to_string()),
                )
                .when_some(progress, |d, frac| {
                    d.child(match frac {
                        Some(f) => ui::bar(f.max(0.04), color).into_any_element(),
                        None => ui::sweep(color, 0.0).into_any_element(),
                    })
                }),
        )
        .on_click(cx.listener(move |this, _, _, cx| this.go(Place::Live(id), cx)))
}

/// The first-run card: what happens when you hand the crew a job.
fn how_it_works() -> impl IntoElement {
    let step = |n: &'static str, title: &'static str, line: &'static str| {
        div()
            .flex_1()
            .flex_basis(px(0.))
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                div()
                    .size(px(28.))
                    .rounded_full()
                    .bg(c(ui::INK))
                    .text_color(c(0xFFFFFF))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(12.))
                    .font_weight(FontWeight::BOLD)
                    .child(n),
            )
            .child(
                div()
                    .mt(px(4.))
                    .text_size(px(15.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title),
            )
            .child(
                div()
                    .text_size(px(13.))
                    .line_height(px(19.))
                    .text_color(c(ui::MUTED))
                    .child(line),
            )
    };
    ui::card()
        .mt(px(40.))
        .w(px(840.))
        .p(px(26.))
        .flex()
        .gap(px(28.))
        .child(step(
            "1",
            "Pick a folder",
            "Any project on this computer. The crew only works inside it.",
        ))
        .child(step(
            "2",
            "Say what you want",
            "Build, fix, review or test. Each kind goes to the bot that does it best.",
        ))
        .child(step(
            "3",
            "Watch, then check",
            "Follow along live. It tells you in plain words what changed and how to try it.",
        ))
}
