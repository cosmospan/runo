//! The v2 look: colour tokens and the few building blocks every screen uses
//! (cards, pills, chips, labels), taken from the Holo app screens design.

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

// Named, not globbed: names like `fill` would clash with the kit's prelude.
#[allow(unused_imports)]
pub use super::theme::{
    add, ask_tone, bad_tone, bg, body, del, faint, fill, fill_hover, fill_press, grey_tone, hover,
    ink, line, line2, line_hi, muted, off, ok_tone, on_fill, panel, press, raised, run_tone,
    set_theme, t_amber, t_blue, t_green, t_red, t_violet, theme, well, Tone, RUNO, THEMES,
};

// Status and member colours: the same in every theme, as dots, bars and fills.
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

/// A colour from `0xRRGGBB`, or `0xAARRGGBB` for a see-through one in a frosted
/// theme (a top byte of zero is solid).
pub fn c(hex: u32) -> Hsla {
    let mut h: Hsla = rgb(hex & 0xff_ffff).into();
    h.a = alpha(hex);
    h
}

/// A colour at opacity `a`, on top of any opacity the colour already has.
pub fn ca(hex: u32, a: f32) -> Hsla {
    let mut h = c(hex);
    h.a *= a;
    h
}

fn alpha(hex: u32) -> f32 {
    match hex >> 24 {
        0 => 1.0,
        a => a as f32 / 255.0,
    }
}

/// The gap around the page canvas, between it and the window's edge.
pub const GAP: f32 = 8.;
/// The page canvas's corner radius.
pub const CANVAS_R: f32 = 12.;
/// A card's corner radius.
pub const CARD_R: f32 = 10.;

/// The ground under everything: the sidebar sits on it and the page canvas
/// floats over it. A shade darker than the page in every theme, keeping a
/// frosted theme's opacity.
pub fn ground() -> u32 {
    let t = theme();
    let (base, k) = if t.light {
        (t.bg, 0.045)
    } else {
        (t.well, 0.3)
    };
    super::theme::mix(base & 0xff_ffff, 0x000000, k) | (base & 0xff00_0000)
}

/// The page canvas's shadow: a soft drop under it and a faint lit top edge.
pub fn float() -> Vec<BoxShadow> {
    let light = theme().light;
    vec![
        BoxShadow {
            color: ca(0x000000, if light { 0.05 } else { 0.35 }),
            offset: point(px(0.), px(1.)),
            blur_radius: px(3.),
            spread_radius: px(0.),
            inset: false,
        },
        BoxShadow {
            color: ca(0xFFFFFF, if light { 0.6 } else { 0.035 }),
            offset: point(px(0.), px(1.)),
            blur_radius: px(0.),
            spread_radius: px(0.),
            inset: true,
        },
    ]
}

/// A card's lit top edge and a soft shadow, so it reads as lifted.
pub fn sheen() -> Vec<BoxShadow> {
    let light = theme().light;
    vec![
        BoxShadow {
            color: ca(0x000000, if light { 0.04 } else { 0.2 }),
            offset: point(px(0.), px(1.)),
            blur_radius: px(2.),
            spread_radius: px(0.),
            inset: false,
        },
        BoxShadow {
            color: ca(0xFFFFFF, if light { 0.7 } else { 0.03 }),
            offset: point(px(0.), px(1.)),
            blur_radius: px(0.),
            spread_radius: px(0.),
            inset: true,
        },
    ]
}

/// A small lifted chip, like the selected row in the sidebar: a hairline ring,
/// a soft shadow and a lit top edge.
pub fn chip() -> Vec<BoxShadow> {
    let mut v = vec![BoxShadow {
        color: c(line()),
        offset: point(px(0.), px(0.)),
        blur_radius: px(0.),
        spread_radius: px(1.),
        inset: false,
    }];
    v.extend(sheen());
    v
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

/// A panel on the ground, with a hairline edge.
pub fn card() -> Div {
    div()
        .bg(c(panel()))
        .border_1()
        .border_color(c(line()))
        .rounded(px(CARD_R))
        .shadow(sheen())
}

/// A quiet outline button, like "Look first" or "Pause".
pub fn ghost(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Stateful<Div> {
    div()
        .id(id)
        .px(px(12.))
        .h(px(32.))
        .flex()
        .items_center()
        .justify_center()
        .gap(px(6.))
        .rounded(px(8.))
        .bg(c(panel()))
        .border_1()
        .border_color(c(line()))
        .shadow(edge())
        .text_size(px(13.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(c(ink()))
        .cursor_pointer()
        .hover(|s| s.bg(c(hover())).border_color(c(line_hi())))
        .active(|s| s.bg(c(press())))
        .child(label.into())
}

/// The primary button, like "Approve & open PR": the theme's solid fill,
/// high contrast and nothing else.
pub fn solid(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Stateful<Div> {
    div()
        .id(id)
        .px(px(14.))
        .h(px(32.))
        .flex()
        .items_center()
        .justify_center()
        .gap(px(6.))
        .rounded(px(8.))
        .bg(c(fill()))
        .text_size(px(13.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(c(on_fill()))
        .cursor_pointer()
        .hover(|s| s.bg(c(fill_hover())))
        .active(|s| s.bg(c(fill_press())))
        .child(label.into())
}

/// A small pill for filters and tabs; `on` is the selected one.
pub fn pill(id: impl Into<ElementId>, label: impl Into<SharedString>, on: bool) -> Stateful<Div> {
    div()
        .id(id)
        .px(px(10.))
        .h(px(28.))
        .flex()
        .items_center()
        .gap(px(6.))
        .rounded(px(7.))
        .text_size(px(12.5))
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .border_1()
        .map(|d| {
            if on {
                d.bg(c(raised()))
                    .border_color(c(line_hi()))
                    .text_color(c(ink()))
                    .shadow(edge())
            } else {
                d.border_color(c(line()))
                    .text_color(c(muted()))
                    .hover(|s| s.bg(c(hover())).text_color(c(ink())))
                    .active(|s| s.bg(c(press())))
            }
        })
        .child(label.into())
}

/// Just the lit top edge, for small controls.
pub fn edge() -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: ca(0xFFFFFF, if theme().light { 0.7 } else { 0.04 }),
        offset: point(px(0.), px(1.)),
        blur_radius: px(0.),
        spread_radius: px(0.),
        inset: true,
    }]
}

/// A stat card: a quiet label with a status dot, the figure, and a note.
pub fn stat(dot: Option<u32>, label: &'static str, value: String, note: String) -> Div {
    div()
        .flex_1()
        .flex_basis(px(0.))
        .min_w_0()
        .px(px(16.))
        .pt(px(14.))
        .pb(px(16.))
        .bg(c(panel()))
        .border_1()
        .border_color(c(line()))
        .rounded(px(CARD_R))
        .shadow(sheen())
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(7.))
                .text_size(px(12.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(c(muted()))
                .when_some(dot, |d, col| {
                    d.child(div().size(px(6.)).rounded_full().bg(c(col)))
                })
                .child(label),
        )
        .child(
            div()
                .mt(px(10.))
                .text_size(px(26.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(value),
        )
        .child(
            div()
                .mt(px(2.))
                .text_size(px(12.))
                .text_color(c(faint()))
                .truncate()
                .child(note),
        )
}

/// The small uppercase mono caption the design puts over and under things.
pub fn caption(text: impl Into<SharedString>) -> Div {
    div()
        .font_family(MONO)
        .text_size(px(9.5))
        .text_color(c(faint()))
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
        .bg(c(raised()))
        .child(
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
        .bg(c(raised()))
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
