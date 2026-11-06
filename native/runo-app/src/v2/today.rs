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
use super::Pane;
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
        let folder = self.crew.store.folder().map(|f| f.name.clone());
        let running = self.crew.store.running().count();
        let today_done = self
            .crew
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
            .text_color(c(ui::muted()))
            .child(lead)
            .child(self.folder_picker(cx));
        let has_jobs = !self.crew.store.jobs.is_empty();

        // A working page: a short header, the job box, the numbers, the table.
        let page = div()
            .w_full()
            .max_w(px(1160.))
            .flex()
            .flex_col()
            .gap(px(20.))
            .px(px(28.))
            .pt(px(24.))
            .pb(px(32.))
            .child(ui::rise(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .flex()
                            .items_baseline()
                            .gap(px(10.))
                            .child(
                                div()
                                    .text_size(px(22.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Jobs"),
                            )
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .text_color(c(ui::faint()))
                                    .child(greeting()),
                            ),
                    )
                    .child(sub.mt(px(0.)).text_size(px(13.5))),
                self.stagger(0),
            ))
            .children(self.sample_banner(cx))
            .children(self.fin_offer(cx))
            .child(ui::rise(
                div().w_full().child(self.composer_card(cx)),
                self.stagger(1),
            ))
            .child(ui::rise(
                div().w_full().child(if has_jobs {
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(20.))
                        .children(self.goals_strip(cx))
                        .child(self.kpis())
                        .child(self.jobs_table(cx))
                        .into_any_element()
                } else {
                    self.start_here(cx)
                }),
                self.stagger(2),
            ));
        let inspector = if has_jobs { self.inspector(cx) } else { None };
        div()
            .size_full()
            .flex()
            .child(
                div()
                    .id("today")
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .overflow_y_scroll()
                    .child(page),
            )
            .children(inspector)
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
            .bg(c(ui::raised()))
            .child(
                div()
                    .absolute()
                    .top(px(3.))
                    .left(px(3. + x))
                    .w(px(TAB))
                    .h(px(32.))
                    .rounded(px(9.))
                    .bg(c(ui::panel()))
                    .shadow(ui::lift()),
            );
        // Every kind of work. One nobody on the crew does yet is fainter, and a
        // job sent in it makes Fin offer the bot for it.
        for (i, m) in Mode::ALL.into_iter().enumerate() {
            let on = self.mode == m;
            let staffed = Member::doer_for(m).is_some();
            modes = modes.child(
                div()
                    .id(("mode", i))
                    .w(px(TAB))
                    .h(px(32.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(px(5.))
                    .rounded(px(9.))
                    .text_size(px(13.))
                    .cursor_pointer()
                    .map(|d| {
                        if on {
                            d.font_weight(FontWeight::SEMIBOLD).text_color(c(ui::ink()))
                        } else {
                            d.text_color(c(if staffed { ui::muted() } else { ui::faint() }))
                                .hover(|s| s.text_color(c(ui::ink())))
                        }
                    })
                    .child(m.label())
                    .when(!staffed, |d| {
                        let who = Member::for_mode(m).name();
                        d.tooltip(move |window, cx| {
                            gpui_kit::component::tooltip::Tooltip::new(format!(
                                "Nobody on your crew does this yet. Send a job and Peek offers {who}."
                            ))
                            .build(window, cx)
                        })
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.target = None;
                        this.set_mode(m, window, cx);
                        this.composer.update(cx, |i, cx| i.focus(window, cx));
                    })),
            );
        }
        let engine = super::engine::label(cx.global::<crate::settings::AppSettings>(), target);
        let box_ = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(
                div()
                    .min_h(px(64.))
                    .text_size(px(15.))
                    .line_height(px(22.))
                    .child(
                        Textarea::new(&self.composer)
                            .appearance(false)
                            .bordered(false),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.))
                    .child(modes)
                    .child(div().flex_1())
                    .when(self.mode.writes(), |d| {
                        let on = self.splitting;
                        d.child(
                            ui::pill("split-toggle", "Split", on)
                                .tooltip(|window, cx| {
                                    gpui_kit::component::tooltip::Tooltip::new(
                                        "For a bigger goal: Squad Code splits it into a few jobs that run one after another",
                                    )
                                    .build(window, cx)
                                })
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.splitting = !this.splitting;
                                    if this.splitting {
                                        this.racing = 0;
                                    }
                                    cx.notify();
                                })),
                        )
                    })
                    .when(self.mode.writes(), |d| {
                        let n = self.racing;
                        let label = match n {
                            2 | 3 => format!("Race \u{d7}{n}"),
                            _ => "Race".to_string(),
                        };
                        d.child(
                            ui::pill("race-toggle", label, n > 1)
                                .tooltip(|window, cx| {
                                    gpui_kit::component::tooltip::Tooltip::new(
                                        "Run it on several engines at once and keep the best try",
                                    )
                                    .build(window, cx)
                                })
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.racing = match this.racing {
                                        0 | 1 => 2,
                                        2 => 3,
                                        _ => 0,
                                    };
                                    if this.racing > 1 {
                                        this.splitting = false;
                                    }
                                    cx.notify();
                                })),
                        )
                    })
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
                            .text_color(c(ui::muted()))
                            .cursor_pointer()
                            .hover(|s| s.bg(c(ui::hover())).text_color(c(ui::ink())))
                            .child(face)
                            .child(if target.on_crew() {
                                target.name().to_string()
                            } else {
                                format!("{} (Peek asks first)", target.name())
                            })
                            .tooltip(move |window, cx| {
                                gpui_kit::component::tooltip::Tooltip::new(format!(
                                    "Runs on {engine}. Click to change it."
                                ))
                                .build(window, cx)
                            })
                            .on_click(cx.listener(|this, _, _, cx| this.go(Place::Settings(Pane::Engines), cx))),
                    )
                    .child(
                        div()
                            .id("send")
                            .size(px(38.))
                            .rounded_full()
                            // Dark once there is something to send.
                            .bg(c(if ready { accent } else { ui::off() }))
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
            );
        div()
            .w_full()
            .flex()
            .gap(px(18.))
            .px(px(18.))
            .pt(px(18.))
            .pb(px(14.))
            .bg(c(ui::panel()))
            .border_1()
            .border_color(c(ui::line()))
            .rounded(px(12.))
            .child(box_)
    }

    /// Fin's question when a job needs a bot nobody on the crew is: add it and
    /// start, make your own, or not now.
    fn fin_offer(&mut self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let (who, mode, text) = {
            let o = self.offer.as_ref()?;
            (o.who, o.mode, o.text.clone())
        };
        let face = self.bot(who, Mood::NeedsYou, 44., Framing::Bust);
        let short: String = if text.chars().count() > 80 {
            format!(
                "{}\u{2026}",
                text.chars().take(80).collect::<String>().trim_end()
            )
        } else {
            text
        };
        Some(
            div()
                .w_full()
                .flex()
                .items_center()
                .gap(px(16.))
                .px(px(18.))
                .py(px(14.))
                .bg(c(ui::panel()))
                .border_1()
                .border_color(ca(who.accent(), 0.45))
                .rounded(px(12.))
                .child(face)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(3.))
                        .child(ui::caption("FIN"))
                        .child(
                            div()
                                .text_size(px(14.5))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(c(ui::ink()))
                                .child(format!(
                                    "Nobody on your crew {} yet. Add {} for this job?",
                                    mode.work(),
                                    who.name()
                                )),
                        )
                        .child(
                            div()
                                .text_size(px(12.5))
                                .line_height(px(18.))
                                .text_color(c(ui::muted()))
                                .child(format!("{} \u{b7} \u{201c}{short}\u{201d}", who.pitch())),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_none()
                        .items_center()
                        .gap(px(8.))
                        .child(ui::ghost("offer-skip", "Not now").on_click(
                            cx.listener(|this, _, window, cx| this.skip_offer(window, cx)),
                        ))
                        .child(ui::ghost("offer-make", "Make my own").on_click(
                            cx.listener(|this, _, window, cx| this.make_for_offer(window, cx)),
                        ))
                        .child(
                            ui::solid("offer-add", format!("Add {} and start", who.name()))
                                .on_click(
                                    cx.listener(|this, _, window, cx| this.take_offer(window, cx)),
                                ),
                        ),
                ),
        )
    }

    /// The folder the crew works in, as words in a sentence you can click.
    fn folder_picker(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let folder = self.crew.store.folder().map(|f| f.name.clone());
        let folder_chip = div()
            .id("folder")
            .flex()
            .items_center()
            .gap(px(6.))
            .px(px(8.))
            .h(px(30.))
            .rounded(px(9.))
            .text_color(c(if folder.is_some() {
                ui::ink()
            } else {
                ui::ORANGE
            }))
            .font_weight(FontWeight::SEMIBOLD)
            .cursor_pointer()
            .hover(|s| s.bg(c(ui::hover())))
            .child(
                Icon::new(IconName::Folder)
                    .size(px(16.))
                    .text_color(c(ui::muted())),
            )
            .child(folder.clone().unwrap_or_else(|| "Pick a folder".into()))
            .child(
                Icon::new(IconName::ChevronDown)
                    .size(px(14.))
                    .text_color(c(ui::faint())),
            )
            .on_click(cx.listener(|this, _, _, cx| {
                if this.crew.store.folders.is_empty() {
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
                .bg(c(ui::panel()))
                .border_1()
                .border_color(c(ui::line()))
                .shadow(ui::lift())
                .flex()
                .flex_col();
            for (i, f) in self.crew.store.folders.iter().enumerate() {
                let on = i == self.crew.store.current;
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
                        .when(on, |d| d.bg(c(ui::raised())))
                        .hover(|s| s.bg(c(ui::hover())))
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
                                        .text_color(c(ui::faint()))
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
                                .hover(|s| s.bg(c(ui::hover())))
                                .tooltip(|window, cx| {
                                    gpui_kit::component::tooltip::Tooltip::new(
                                        "Remove from this list (the files stay)",
                                    )
                                    .build(window, cx)
                                })
                                .child(
                                    Icon::new(IconName::X)
                                        .size(px(12.))
                                        .text_color(c(ui::muted())),
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.forget_folder(i, cx);
                                })),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.crew.store.current = i;
                            this.folder_menu = false;
                            this.crew.store.save();
                            cx.notify();
                        })),
                );
            }
            list.child(div().my(px(4.)).h(px(1.)).bg(c(ui::line())))
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
                        .hover(|s| s.bg(c(ui::hover())))
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
}

fn plural<'a>(n: usize, one: &'a str, many: &'a str) -> &'a str {
    if n == 1 {
        one
    } else {
        many
    }
}
