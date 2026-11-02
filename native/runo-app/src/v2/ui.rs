//! The v2 look: colour tokens and the few building blocks every screen uses
//! (cards, pills, chips, labels), taken from the Holo app screens design.

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

pub const INK: u32 = 0x17151F;
pub const BODY: u32 = 0x3A3C50;
pub const MUTED: u32 = 0x6A6C80;
pub const FAINT: u32 = 0x9A9CB0;
pub const LINE: u32 = 0xE7E8F0;
pub const RAIL: u32 = 0xF9FAFD;
pub const ORANGE: u32 = 0xFF6A2B;
pub const GREEN: u32 = 0x1FC27A;
pub const AMBER: u32 = 0xF5A300;
pub const BLUE: u32 = 0x1E95F0;
pub const VIOLET: u32 = 0x7B4DF5;
pub const RED: u32 = 0xF2364F;
pub const MONO: &str = "JetBrains Mono";

pub fn c(hex: u32) -> Hsla {
    rgb(hex).into()
}

pub fn ca(hex: u32, a: f32) -> Hsla {
    let mut h: Hsla = rgb(hex).into();
    h.a = a;
    h
}

/// The design's soft drop shadow under cards.
pub fn lift() -> Vec<BoxShadow> {
    vec![
        BoxShadow {
            color: ca(0x28285A, 0.10),
            offset: point(px(0.), px(14.)),
            blur_radius: px(34.),
            spread_radius: px(-18.),
            inset: false,
        },
        BoxShadow {
            color: ca(0x28285A, 0.05),
            offset: point(px(0.), px(1.)),
            blur_radius: px(2.),
            spread_radius: px(0.),
            inset: false,
        },
    ]
}

/// A white, softly lifted card.
pub fn card() -> Div {
    div()
        .bg(ca(0xFFFFFF, 0.86))
        .border_1()
        .border_color(c(0xFFFFFF))
        .rounded(px(20.))
        .shadow(lift())
}

/// A rounded outline button, like "Look first" or "Pause".
pub fn ghost(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Stateful<Div> {
    div()
        .id(id)
        .px(px(16.))
        .h(px(38.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(12.))
        .bg(c(0xFFFFFF))
        .border_1()
        .border_color(c(LINE))
        .text_size(px(13.5))
        .font_weight(FontWeight::MEDIUM)
        .text_color(c(INK))
        .cursor_pointer()
        .hover(|s| s.bg(c(0xF3F4F9)))
        .child(label.into())
}

/// The dark primary button, like "Use the fix".
pub fn solid(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Stateful<Div> {
    div()
        .id(id)
        .px(px(30.))
        .h(px(38.))
        .flex()
        .items_center()
        .justify_center()
        .gap(px(8.))
        .rounded(px(12.))
        .bg(c(0x17151F))
        .text_size(px(13.5))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(c(0xFFFFFF))
        .cursor_pointer()
        .hover(|s| s.bg(c(0x2C2A38)))
        .child(label.into())
}

/// A small rounded pill for filters and tabs; `on` is the selected one.
pub fn pill(id: impl Into<ElementId>, label: impl Into<SharedString>, on: bool) -> Stateful<Div> {
    div()
        .id(id)
        .px(px(13.))
        .h(px(32.))
        .flex()
        .items_center()
        .gap(px(6.))
        .rounded_full()
        .text_size(px(13.))
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .map(|d| {
            if on {
                d.bg(c(INK)).text_color(c(0xFFFFFF))
            } else {
                d.bg(c(0xFFFFFF))
                    .border_1()
                    .border_color(c(LINE))
                    .text_color(c(INK))
                    .hover(|s| s.bg(c(0xF3F4F9)))
            }
        })
        .child(label.into())
}

/// The small uppercase mono caption the design puts over and under things.
pub fn caption(text: impl Into<SharedString>) -> Div {
    div()
        .font_family(MONO)
        .text_size(px(9.5))
        .text_color(c(FAINT))
        .child(text.into())
}

/// A coloured dot and a word, like "● Working".
pub fn status(color: u32, label: impl Into<SharedString>) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(5.))
        .text_size(px(12.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(c(color))
        .child(div().size(px(7.)).rounded_full().bg(c(color)))
        .child(label.into())
}

/// A thin progress bar.
pub fn bar(frac: f32, color: u32) -> Div {
    div()
        .h(px(4.))
        .w_full()
        .rounded_full()
        .bg(c(0xEEEFF5))
        .child(
            div()
                .h_full()
                .rounded_full()
                .bg(c(color))
                .w(relative(frac.clamp(0.0, 1.0))),
        )
}

/// Inline code, like `round_cents()`.
pub fn code(text: impl Into<SharedString>) -> Div {
    div()
        .px(px(6.))
        .py(px(1.))
        .rounded(px(6.))
        .bg(c(0xF1F2F7))
        .border_1()
        .border_color(c(0xE6E7EF))
        .font_family(MONO)
        .text_size(px(12.))
        .text_color(c(INK))
        .child(text.into())
}
