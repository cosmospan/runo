//! Today: the crew up top, one box to hand them a job, and what is moving:
//! what needs you, what is being worked on, and what just finished.

use gpui_kit::assets::IconName;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::Icon;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use runo_holo::{Framing, Mood};

use super::data::{Member, Mode};
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

/// A mode tab's width: the bot's face and the word.
const TAB: f32 = 76.;

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
        // The line under the greeting carries the folder picker in its words.
        let lead = match (&folder, running, today_done) {
            (None, ..) => "Pick a folder to start:".to_string(),
            (Some(_), 0, 0) => "What should we do in".to_string(),
            (Some(_), 0, d) => format!("{d} {} done today. What next in", plural(d, "job", "jobs")),
            (Some(_), r, _) => format!(
                "{r} {} running. What else in",
                plural(r, "job is", "jobs are")
            ),
        };
        let sub = div()
            .mt(px(10.))
            .flex()
            .items_center()
            .gap(px(4.))
            .text_size(px(17.))
            .text_color(c(ui::MUTED))
            .child(lead)
            .child(self.folder_picker(cx));
        let has_jobs = !self.store.jobs.is_empty();

        // The group sits in the middle of the space above the dock, and
        // scrolls when the window is too short for it.
        let page = div()
            .w_full()
            .min_h_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .px(px(40.))
            .pt(px(if cfg!(target_os = "macos") { 64. } else { 40. }))
            .pb(px(28. + super::DOCK_ROOM))
            .child(ui::rise(
                div()
                    .text_size(px(52.))
                    .font_weight(FontWeight::BOLD)
                    .line_height(px(58.))
                    .child(greeting()),
                self.stagger(0),
            ))
            .child(ui::rise(sub, self.stagger(1)))
            .child(ui::rise(
                div()
                    .w_full()
                    .flex()
                    .justify_center()
                    .child(self.composer_card(cx)),
                self.stagger(3),
            ))
            .child(ui::rise(
                div()
                    .w_full()
                    .flex()
                    .justify_center()
                    .child(self.starters(cx)),
                self.stagger(4),
            ))
            .child(ui::rise(
                div().w_full().flex().justify_center().child(if has_jobs {
                    self.cards(cx).into_any_element()
                } else {
                    how_it_works().into_any_element()
                }),
                self.stagger(5),
            ));
        div()
            .id("today")
            .size_full()
            .overflow_y_scroll()
            .child(page)
            .into_any_element()
    }

    fn composer_card(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let target = self.doer();
        let ready =
            !self.composer.read(cx).value().trim().is_empty() || self.mode.default_job().is_some();
        let face = self.avatar(target, 26.);
        let accent = target.accent();
        // The white highlight glides under the tabs as the mode changes.
        let x = self.mode_x();
        let mut modes = div()
            .relative()
            .flex()
            .items_center()
            .p(px(3.))
            .gap(px(super::MODE_STEP - TAB))
            .rounded(px(12.))
            .bg(c(0xF1F2F7))
            .child(
                div()
                    .absolute()
                    .top(px(3.))
                    .left(px(3. + x))
                    .w(px(TAB))
                    .h(px(32.))
                    .rounded(px(9.))
                    .bg(c(0xFFFFFF))
                    .shadow(ui::lift()),
            );
        // Only the kinds of work someone on the crew takes.
        let shown: Vec<Mode> = Mode::ALL
            .into_iter()
            .filter(|m| Member::doer_for(*m).is_some())
            .collect();
        for (i, m) in shown.iter().copied().enumerate() {
            let on = self.mode == m;
            let who = Member::doer_for(m).unwrap_or(Member::for_mode(m));
            let mini = self.avatar(who, 16.);
            modes = modes.child(
                div()
                    .id(("mode", i))
                    .w(px(TAB))
                    .h(px(32.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(px(5.))
                    .child(div().when(!on, |d| d.opacity(0.75)).child(mini))
                    .rounded(px(9.))
                    .text_size(px(13.))
                    .cursor_pointer()
                    .map(|d| {
                        if on {
                            d.font_weight(FontWeight::SEMIBOLD).text_color(c(ui::INK))
                        } else {
                            d.text_color(c(ui::MUTED))
                                .hover(|s| s.text_color(c(ui::INK)))
                        }
                    })
                    .child(m.label())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.target = None;
                        this.set_mode(m, window, cx);
                        this.composer.update(cx, |i, cx| i.focus(window, cx));
                    })),
            );
        }
        let engine = super::engine::label(cx.global::<crate::settings::AppSettings>(), target);
        ui::card()
            .mt(px(30.))
            .w(px(840.))
            .rounded(px(24.))
            .px(px(22.))
            .pt(px(20.))
            .pb(px(14.))
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(
                div()
                    .min_h(px(78.))
                    .text_size(px(17.))
                    .line_height(px(26.))
                    .child(
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
                            // Dark once there is something to send.
                            .bg(c(if ready { accent } else { 0xD5D7E3 }))
                            .when(ready, |d| {
                                d.shadow(vec![BoxShadow {
                                    color: ca(accent, 0.45),
                                    offset: point(px(0.), px(6.)),
                                    blur_radius: px(16.),
                                    spread_radius: px(-2.),
                                    inset: false,
                                }])
                            })
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .when(ready, |d| d.hover(|s| s.opacity(0.9)))
                            .active(|s| s.opacity(0.8))
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

    /// The folder the crew works in, as words in a sentence you can click.
    fn folder_picker(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let folder = self.store.folder().map(|f| f.name.clone());
        let folder_chip = div()
            .id("folder")
            .flex()
            .items_center()
            .gap(px(6.))
            .px(px(8.))
            .h(px(30.))
            .rounded(px(9.))
            .text_color(c(if folder.is_some() {
                ui::INK
            } else {
                ui::ORANGE
            }))
            .font_weight(FontWeight::SEMIBOLD)
            .cursor_pointer()
            .hover(|s| s.bg(ca(0xFFFFFF, 0.8)))
            .child(
                Icon::new(IconName::Folder)
                    .size(px(16.))
                    .text_color(c(ui::MUTED)),
            )
            .child(folder.clone().unwrap_or_else(|| "Pick a folder".into()))
            .child(
                Icon::new(IconName::ChevronDown)
                    .size(px(14.))
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
                        .group("folder-item")
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .px(px(10.))
                        .py(px(7.))
                        .rounded(px(9.))
                        .cursor_pointer()
                        .when(on, |d| d.bg(c(0xF1F2F7)))
                        .hover(|s| s.bg(c(0xF6F7FB)))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
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
                                ),
                        )
                        // Forget a folder: only the list entry, never the files.
                        .child(
                            div()
                                .id(("folder-forget", i))
                                .flex_none()
                                .size(px(22.))
                                .rounded(px(6.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .invisible()
                                .group_hover("folder-item", |s| s.visible())
                                .hover(|s| s.bg(c(0xE7E8F0)))
                                .tooltip(|window, cx| {
                                    gpui_kit::component::tooltip::Tooltip::new(
                                        "Remove from this list (the files stay)",
                                    )
                                    .build(window, cx)
                                })
                                .child(
                                    Icon::new(IconName::X)
                                        .size(px(12.))
                                        .text_color(c(ui::MUTED)),
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.forget_folder(i, cx);
                                })),
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
        div()
            .relative()
            .child(folder_chip)
            .children(menu.map(|m| deferred(m).with_priority(1)))
    }

    /// Up to three things to ask the bot that would take the job, in one
    /// row under the box. They follow the tab, and go once you start typing.
    fn starters(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let who = self.doer();
        let typing = !self.composer.read(cx).value().trim().is_empty();
        let mut row = div()
            .mt(px(12.))
            .w(px(840.))
            .h(px(34.))
            .flex()
            .flex_wrap()
            .gap(px(8.))
            // Only what fits on one line; the rest wraps out of sight.
            .overflow_hidden();
        if typing {
            return row;
        }
        let mode = self.mode;
        let picks: Vec<&'static str> = match mode.default_job() {
            Some(job) => std::iter::once(job)
                .chain(who.starters().into_iter().take(2))
                .collect(),
            None => who.starters().to_vec(),
        };
        for (i, text) in picks.into_iter().enumerate() {
            row = row.child(
                div()
                    .id(("starter", i))
                    .flex_none()
                    .flex()
                    .items_center()
                    .px(px(13.))
                    .h(px(34.))
                    .rounded_full()
                    .bg(ca(0xFFFFFF, 0.6))
                    .border_1()
                    .border_color(ca(0xFFFFFF, 0.9))
                    .text_size(px(13.))
                    .text_color(c(ui::BODY))
                    .cursor_pointer()
                    .hover(|s| s.bg(c(0xFFFFFF)).text_color(c(ui::INK)))
                    .active(|s| s.bg(c(0xF1F2F7)))
                    .child(text)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.set_mode(mode, window, cx);
                        this.composer
                            .update(cx, |i, cx| i.set_value(text, window, cx));
                        this.composer.update(cx, |i, cx| i.focus(window, cx));
                        this.focus_moved = std::time::Instant::now();
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
            let face = self.avatar(j.bot, 34.);
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
            let face = self.bot(j.bot, Mood::Working, 34., Framing::Bust);
            let frac = j
                .progress()
                .map(|(d, n)| self.glide_bar(j.id, d as f32 / n.max(1) as f32));
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
            let face = self.avatar(j.bot, 34.);
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
