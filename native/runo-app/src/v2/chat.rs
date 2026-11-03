//! Chat: ask any bot in plain words. It reads the code in the current folder
//! and answers; nothing changes. Each bot keeps its own conversation.

use gpui_kit::assets::IconName;
use gpui_kit::component::input::Input;
use gpui_kit::component::Icon;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use runo_holo::{Framing, Mood};

use super::data::Member;
use super::live::{note_line, plain, typing};
use super::ui::{self, c, ca};
use super::{Place, Shell};

impl Shell {
    pub(super) fn chat(&mut self, who: Member, cx: &mut Context<Self>) -> AnyElement {
        div()
            .size_full()
            .flex()
            // Bots are picked in the sidebar's crew list.
            .child(self.chat_thread(who, cx))
            .into_any_element()
    }

    fn chat_thread(&mut self, who: Member, cx: &mut Context<Self>) -> impl IntoElement {
        let thinking = self.thinking(who);
        let hero = self.bot(
            who,
            if thinking {
                Mood::Thinking
            } else {
                self.mood(who)
            },
            92.,
            Framing::Full,
        );
        let engine = super::engine::label(cx.global::<crate::settings::AppSettings>(), who);
        let log = self.store.chat(who).to_vec();
        let header = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(14.))
            .px(px(28.))
            .pt(px(if cfg!(target_os = "macos") { 14. } else { 8. }))
            .pb(px(8.))
            .border_b_1()
            .border_color(c(ui::LINE))
            .child(hero)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .child(
                        div()
                            .text_size(px(22.))
                            .font_weight(FontWeight::BOLD)
                            .child(who.name()),
                    )
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(c(ui::MUTED))
                            .child(format!("{} · reads the code, never changes it", who.role())),
                    ),
            )
            .child(
                div()
                    .id("chat-engine")
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .px(px(12.))
                    .h(px(32.))
                    .rounded_full()
                    .border_1()
                    .border_color(c(ui::LINE))
                    .bg(c(0xFFFFFF))
                    .text_size(px(12.5))
                    .cursor_pointer()
                    .hover(|s| s.bg(c(0xF6F7FB)))
                    .child(
                        Icon::new(IconName::Cpu)
                            .size(px(13.))
                            .text_color(c(ui::MUTED)),
                    )
                    .child(engine)
                    .on_click(cx.listener(|this, _, _, cx| this.go(Place::Engines, cx))),
            )
            .when(!log.is_empty(), |d| {
                d.child(ui::ghost("chat-clear", "Clear").on_click(cx.listener(
                    move |this, _, _, cx| {
                        this.store.chats.remove(&who);
                        this.store.save();
                        cx.notify();
                    },
                )))
            });

        let mut body = div().flex().flex_col().gap(px(14.)).px(px(28.)).py(px(24.));
        if log.is_empty() {
            body = body.child(self.chat_empty(who, cx));
        }
        // The newest line slides in when it arrives.
        let last = log.len().saturating_sub(1);
        let fresh = if self.motion {
            runo_holo::anim::spring(self.chat_at.elapsed().as_secs_f64(), 0.3) as f32
        } else {
            1.0
        };
        for (i, m) in log.iter().enumerate() {
            let k = if i == last { fresh } else { 1.0 };
            let row: Div = if m.mine {
                div().child(
                    div().flex().justify_end().child(
                        div()
                            .max_w(px(620.))
                            .px(px(16.))
                            .py(px(10.))
                            .rounded(px(16.))
                            .bg(c(ui::INK))
                            .text_color(c(0xFFFFFF))
                            .text_size(px(14.))
                            .line_height(px(20.))
                            .child(m.text.clone()),
                    ),
                )
            } else if m.note {
                div().child(note_line(&m.text))
            } else {
                let face = self.avatar(who, 34.);
                div().child(
                    div().flex().gap(px(12.)).child(face).child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .max_w(px(760.))
                            .flex()
                            .flex_col()
                            .gap(px(5.))
                            .child(
                                div()
                                    .px(px(18.))
                                    .py(px(13.))
                                    .rounded(px(16.))
                                    .bg(ca(0xFFFFFF, 0.85))
                                    .border_1()
                                    .border_color(c(0xFFFFFF))
                                    .shadow(ui::lift())
                                    .text_size(px(14.))
                                    .line_height(px(21.))
                                    .child(plain(&m.text)),
                            )
                            .when_some(m.via.clone(), |d, via| {
                                d.child(
                                    div()
                                        .pl(px(4.))
                                        .text_size(px(11.))
                                        .text_color(c(ui::FAINT))
                                        .child(via),
                                )
                            }),
                    ),
                )
            };
            body = body.child(ui::rise(row, k));
        }
        if thinking {
            let secs = self
                .asking
                .as_ref()
                .map(|a| a.started.elapsed().as_secs())
                .unwrap_or(0);
            body = body.child(
                div()
                    .ml(px(40.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(typing(self.now()))
                    .child(
                        div()
                            .text_size(px(12.5))
                            .text_color(c(ui::MUTED))
                            .child(format!("reading the code · {secs}s")),
                    ),
            );
        }
        if !log.is_empty() && !thinking && who.like() != Member::Explainer {
            // Turn the conversation into work: a made bot takes the job itself.
            let mode = who.mode();
            let last_q = log.iter().rev().find(|m| m.mine).map(|m| m.text.clone());
            if let Some(q) = last_q {
                let doer = if who.is_made() {
                    who
                } else {
                    Member::for_mode(mode)
                };
                body = body.child(div().ml(px(40.)).flex().gap(px(8.)).child(
                    ui::ghost("hand-off", format!("Make it a job for {}", doer.name())).on_click(
                        cx.listener(move |this, _, window, cx| {
                            this.set_mode(mode, window, cx);
                            this.target = Some(doer);
                            this.composer
                                .update(cx, |i, cx| i.set_value(q.clone(), window, cx));
                            this.go(Place::Today, cx);
                            this.say(
                                "It's in the box on Today. Edit it if you like, then press Enter.",
                            );
                        }),
                    ),
                ));
            }
        }
        let len = log.len() + usize::from(thinking);
        if len != self.chat_len {
            if len > self.chat_len {
                self.chat_at = std::time::Instant::now();
            }
            self.chat_len = len;
            self.chat_scroll.scroll_to_bottom();
        }
        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .child(header)
            .child(
                div()
                    .id("chat-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.chat_scroll)
                    .child(body),
            )
            .child(
                div()
                    .flex_none()
                    .px(px(24.))
                    .pb(px(super::DOCK_ROOM))
                    .child(
                        ui::card()
                            .rounded(px(18.))
                            .px(px(16.))
                            .py(px(10.))
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(px(14.))
                                    .child(Input::new(&self.chat_input).appearance(false)),
                            )
                            .child(
                                div()
                                    .id("chat-send")
                                    .size(px(36.))
                                    .rounded_full()
                                    .bg(c(ui::INK))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .hover(|s| s.bg(c(0x2C2A38)))
                                    .child(
                                        Icon::new(IconName::ArrowUp)
                                            .size(px(15.))
                                            .text_color(c(0xFFFFFF)),
                                    )
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        let text =
                                            this.chat_input.read(cx).value().trim().to_string();
                                        if !text.is_empty() {
                                            this.chat_input
                                                .update(cx, |i, cx| i.set_value("", window, cx));
                                            this.ask(who, text, cx);
                                        }
                                    })),
                            ),
                    ),
            )
    }

    /// An empty chat: what this bot is for, and a few things to ask it.
    fn chat_empty(&mut self, who: Member, cx: &mut Context<Self>) -> impl IntoElement {
        let mut asks = div()
            .mt(px(14.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .items_center();
        for (i, q) in who.starters().into_iter().enumerate() {
            asks = asks.child(
                div()
                    .id(("chat-starter", i))
                    .px(px(14.))
                    .h(px(36.))
                    .flex()
                    .items_center()
                    .rounded_full()
                    .bg(ca(0xFFFFFF, 0.75))
                    .border_1()
                    .border_color(c(ui::LINE))
                    .text_size(px(13.))
                    .cursor_pointer()
                    .hover(|s| s.bg(c(0xFFFFFF)))
                    .child(q)
                    .on_click(cx.listener(move |this, _, _, cx| this.ask(who, q.to_string(), cx))),
            );
        }
        div()
            .pt(px(40.))
            .flex()
            .flex_col()
            .items_center()
            .child(
                div()
                    .text_size(px(18.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(format!("Ask {} anything", who.name())),
            )
            .child(
                div()
                    .mt(px(6.))
                    .max_w(px(460.))
                    .text_center()
                    .text_size(px(13.5))
                    .line_height(px(20.))
                    .text_color(c(ui::MUTED))
                    .child(who.pitch()),
            )
            .child(asks)
    }
}
