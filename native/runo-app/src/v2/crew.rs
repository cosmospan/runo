//! Crew: every bot, what it's on, and making your own.

use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::*;
use runo_holo::{custom, Eye, Framing, Mood, Palette, Shape};

use super::data::{Maker, Member};
use super::ui::{self, c, ca};
use super::{Place, Shell};

impl Shell {
    pub(super) fn crew(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let working = Member::CREW.iter().filter(|m| m.is_working()).count();
        let needs = Member::CREW
            .iter()
            .filter(|m| m.needs_you() && self.guard_answer.is_none())
            .count();
        let team = Member::CREW
            .iter()
            .filter(|m| m.maker() != Maker::Runo)
            .count();
        let filters = [
            "All".to_string(),
            format!("Working · {working}"),
            format!("Needs you · {needs}"),
            format!("Made by your team · {team}"),
        ];
        let mut pills = div().flex().gap(px(8.));
        for (i, f) in filters.into_iter().enumerate() {
            pills = pills.child(
                ui::pill(("crew-filter", i), f, self.crew_filter == i).on_click(cx.listener(
                    move |this, _, _, cx| {
                        this.crew_filter = i;
                        cx.notify();
                    },
                )),
            );
        }
        let shown: Vec<Member> = Member::CREW
            .into_iter()
            .filter(|m| match self.crew_filter {
                1 => m.is_working(),
                2 => m.needs_you() && self.guard_answer.is_none(),
                3 => m.maker() != Maker::Runo,
                _ => true,
            })
            .collect();
        let mut grid = div().mt(px(22.)).grid().grid_cols(5).gap(px(16.));
        for who in shown {
            grid = grid.child(self.crew_card(who, cx));
        }
        if self.crew_filter == 0 {
            grid = grid.child(self.maker_card(cx));
        }
        div()
            .id("crew")
            .size_full()
            .overflow_y_scroll()
            .px(px(30.))
            .pt(px(30.))
            .pb(px(30.))
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_end()
                    .child(
                        div().child(ui::caption("CREW")).child(
                            div()
                                .mt(px(6.))
                                .text_size(px(34.))
                                .font_weight(FontWeight::BOLD)
                                .child(format!("Your crew, {} bots", Member::CREW.len())),
                        ),
                    )
                    .child(pills),
            )
            .child(grid)
            .into_any_element()
    }

    fn crew_card(&mut self, who: Member, cx: &mut Context<Self>) -> impl IntoElement {
        let mood = if who == Member::Guard && self.guard_answer.is_some() {
            Mood::Done
        } else {
            who.mood()
        };
        let face = self.bot(who, mood, 210., Framing::Full);
        let (status, color, line, tags, count) = who.card();
        let (status, color) = if who == Member::Guard && self.guard_answer.is_some() {
            ("All set", ui::GREEN)
        } else {
            (status, color)
        };
        let maker = match who.maker() {
            Maker::Runo => None,
            Maker::You => Some("Yours"),
            Maker::Teammate(name) => Some(name),
        };
        ui::card()
            .id(("crew-card", who.key() as usize))
            .relative()
            .h(px(366.))
            .px(px(16.))
            .pb(px(14.))
            .flex()
            .flex_col()
            .overflow_hidden()
            .cursor_pointer()
            .hover(|s| s.border_color(c(0xE2E4EE)))
            .children(maker.map(|m| {
                div()
                    .absolute()
                    .top(px(12.))
                    .right(px(12.))
                    .px(px(9.))
                    .py(px(3.))
                    .rounded_full()
                    .bg(c(0xFFFFFF))
                    .border_1()
                    .border_color(c(ui::LINE))
                    .text_size(px(11.5))
                    .child(m)
            }))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(face),
            )
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .text_size(px(17.))
                            .font_weight(FontWeight::BOLD)
                            .child(who.name()),
                    )
                    .child(ui::status(color, status)),
            )
            .child(
                div()
                    .mt(px(2.))
                    .text_size(px(12.5))
                    .text_color(c(ui::MUTED))
                    .child(who.role()),
            )
            .child(div().mt(px(14.)).text_size(px(13.)).truncate().child(line))
            .child(div().mt(px(10.)).h(px(1.)).bg(c(ui::LINE)))
            .child(
                div()
                    .mt(px(8.))
                    .flex()
                    .justify_between()
                    .child(ui::caption(tags))
                    .child(ui::caption(count)),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                if who == Member::Builder {
                    this.go(Place::Live(who), cx);
                } else if let Some(i) = super::data::CHATS
                    .iter()
                    .position(|ch| ch.who == who && !ch.group)
                {
                    this.chat_selected = i;
                    this.go(Place::Chat, cx);
                } else {
                    this.say(format!(
                        "{} is free. Mention it in a chat to give it a job.",
                        who.name()
                    ));
                    cx.notify();
                }
            }))
    }

    /// "Make a bot": cycles through the parts a person can pick from.
    fn maker_card(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        // A new combination every 2.4 s, unless the person is picking.
        let auto = (self.now() / 2.4) as u64;
        let n = auto + self.maker_seed;
        let shapes = [Shape::Cube, Shape::Tall, Shape::Wide, Shape::Soft];
        let eyes = [Eye::Dots, Eye::Smile, Eye::Sleepy, Eye::Squint, Eye::Happy];
        let palette = Palette::ALL[(n % Palette::ALL.len() as u64) as usize];
        let shape = shapes[((n / 2) % shapes.len() as u64) as usize];
        let eye = eyes[((n / 3) % eyes.len() as u64) as usize];
        let look = custom(palette, shape, eye);
        let face = self.look_img(9000 + n, &look, 168., Framing::Full);
        div()
            .id("maker-card")
            .h(px(366.))
            .rounded(px(20.))
            .border_1()
            .border_dashed()
            .border_color(c(0xC9CBD8))
            .bg(ca(0xFFFFFF, 0.45))
            .flex()
            .flex_col()
            .items_center()
            .px(px(16.))
            .pb(px(20.))
            .child(div().flex_1().flex().items_center().justify_center().child(face))
            .child(div().text_size(px(17.)).font_weight(FontWeight::BOLD).child("Make a bot"))
            .child(
                div()
                    .mt(px(6.))
                    .text_size(px(12.5))
                    .text_color(c(ui::MUTED))
                    .text_center()
                    .child("Pick a colour, shape, eyes and motion. Give it a job."),
            )
            .child(
                ui::solid("start-maker", "Start")
                    .mt(px(14.))
                    .px(px(18.))
                    .child(Icon::new(IconName::Plus).size(px(14.)).text_color(c(0xFFFFFF)))
                    .flex_row_reverse()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.maker_seed += 1;
                        this.say("The bot maker opens here next. For now, tap Start to try another look.");
                        cx.notify();
                    })),
            )
    }
}
