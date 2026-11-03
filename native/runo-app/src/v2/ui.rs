//! The v2 look: colour tokens and the few building blocks every screen uses
//! (cards, pills, chips, labels), taken from the Holo app screens design.

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

// The dark, Linear-style ground: the page, panels on it, and raised controls.
pub const BG: u32 = 0x0D0D11;
pub const PANEL: u32 = 0x121217;
pub const RAISED: u32 = 0x18181F;
pub const HOVER: u32 = 0x1E1E26;
pub const PRESS: u32 = 0x26262F;
// Text, brightest first.
pub const INK: u32 = 0xE9E9EF;
pub const BODY: u32 = 0xB9B9C4;
pub const MUTED: u32 = 0x8B8B97;
pub const FAINT: u32 = 0x5D5D69;
// Hairlines: the usual one, a quieter one inside panels, a stronger one on hover.
pub const LINE: u32 = 0x24242D;
pub const LINE2: u32 = 0x1A1A21;
pub const LINE_HI: u32 = 0x34343F;
/// The primary button and other light fills, and the text that sits on them.
pub const FILL: u32 = 0xFFFFFF;
pub const ON_FILL: u32 = 0x0D0D11;
/// Runo's own voice: its suggestions and the selected row.
pub const RUNO: u32 = 0x7B4DF5;
/// Status colours as text on the dark ground: blue running, amber waiting on
/// you, green good, red broken.
pub const T_BLUE: u32 = 0x86AEFF;
pub const T_AMBER: u32 = 0xFBBF4A;
pub const T_GREEN: u32 = 0x5FE0A4;
pub const T_RED: u32 = 0xFF8292;
#[allow(dead_code)]
pub const T_VIOLET: u32 = 0xB9A2FF;
pub const ORANGE: u32 = 0xFF6A2B;
pub const GREEN: u32 = 0x1FC27A;
pub const AMBER: u32 = 0xF59E0B;
pub const BLUE: u32 = 0x3B82F6;
pub const VIOLET: u32 = 0x7B4DF5;
pub const RED: u32 = 0xF2364F;
pub const MONO: &str = "JetBrains Mono";

/// The spacing scale every screen keeps to, in pixels.
#[allow(dead_code)]
pub mod space {
    pub const S: f32 = 8.;
    pub const SM: f32 = 12.;
    pub const M: f32 = 16.;
    pub const L: f32 = 24.;
    pub const XL: f32 = 32.;
}

pub fn c(hex: u32) -> Hsla {
    rgb(hex).into()
}

pub fn ca(hex: u32, a: f32) -> Hsla {
    let mut h: Hsla = rgb(hex).into();
    h.a = a;
    h
}

/// A hairline shadow, for raised controls like the selected tab.
pub fn lift() -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: ca(0x000000, 0.35),
        offset: point(px(0.), px(1.)),
        blur_radius: px(2.),
        spread_radius: px(0.),
        inset: false,
    }]
}

/// A panel on the dark ground, with a hairline edge.
pub fn card() -> Div {
    div()
        .bg(c(PANEL))
        .border_1()
        .border_color(c(LINE))
        .rounded(px(12.))
}

/// A rounded outline button, like "Look first" or "Pause".
pub fn ghost(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Stateful<Div> {
    div()
        .id(id)
        .px(px(14.))
        .h(px(32.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(8.))
        .bg(c(RAISED))
        .border_1()
        .border_color(c(LINE))
        .text_size(px(13.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(c(INK))
        .cursor_pointer()
        .hover(|s| s.bg(c(HOVER)).border_color(c(LINE_HI)))
        .active(|s| s.bg(c(PRESS)))
        .child(label.into())
}

/// The light primary button, like "Approve & open PR".
pub fn solid(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Stateful<Div> {
    div()
        .id(id)
        .px(px(16.))
        .h(px(32.))
        .flex()
        .items_center()
        .justify_center()
        .gap(px(8.))
        .rounded(px(8.))
        .bg(c(FILL))
        .text_size(px(13.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(c(ON_FILL))
        .cursor_pointer()
        .hover(|s| s.bg(c(0xDCDCE3)))
        .active(|s| s.bg(c(0xC8C8D0)))
        .child(label.into())
}

/// A small rounded pill for filters and tabs; `on` is the selected one.
pub fn pill(id: impl Into<ElementId>, label: impl Into<SharedString>, on: bool) -> Stateful<Div> {
    div()
        .id(id)
        .px(px(11.))
        .h(px(28.))
        .flex()
        .items_center()
        .gap(px(6.))
        .rounded(px(7.))
        .text_size(px(13.))
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .map(|d| {
            if on {
                d.bg(c(HOVER))
                    .border_1()
                    .border_color(c(LINE_HI))
                    .text_color(c(INK))
            } else {
                d.border_1()
                    .border_color(c(LINE))
                    .text_color(c(BODY))
                    .hover(|s| s.bg(c(HOVER)).text_color(c(INK)))
                    .active(|s| s.bg(c(PRESS)))
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
    div().h(px(4.)).w_full().rounded_full().bg(c(RAISED)).child(
        div()
            .h_full()
            .rounded_full()
            .bg(c(color))
            .w(relative(frac.clamp(0.0, 1.0))),
    )
}

/// A thin bar with a soft segment sweeping across, for work with no known end.
pub fn sweep(color: u32, _phase: f32) -> Div {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    let p = runo_holo::anim::SOFT.at((t / 1.6).rem_euclid(1.0)) as f32;
    div()
        .h(px(4.))
        .w_full()
        .rounded_full()
        .bg(c(RAISED))
        .relative()
        .overflow_hidden()
        .child(
            div()
                .absolute()
                .top_0()
                .h_full()
                .w(relative(0.3))
                .left(relative(-0.3 + 1.3 * p))
                .rounded_full()
                .bg(c(color)),
        )
}

/// Eases an element in: `k` runs 0 → 1, fading it up from a few pixels lower.
pub fn rise<E: Styled>(el: E, k: f32) -> E {
    el.opacity(k.clamp(0.0, 1.0))
        .relative()
        .top(px(8. * (1. - k)))
}
