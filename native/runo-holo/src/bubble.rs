//! The bubble crew: every bot is a soft glass bubble with a rainbow film that
//! slowly turns, and little bubbles that split off, merge back and orbit.
//!
//! A port of the "Runo · the bubble crew" design. The body (liquid union of
//! circles, the blurred colour blobs inside, the white inner edge and the
//! rainbow rim) is shaded per pixel, which is what the SVG's goo filters
//! amount to; the face, highlights and props are drawn on top with Vello.
//! Everything is a pure function of time, in the design's 200 × 200 view box.

use vello_cpu::kurbo::{
    Affine, BezPath, Cap, Circle, Ellipse, Join, Rect, RoundedRect, Shape, Stroke,
};
use vello_cpu::peniko::color::{AlphaColor, Srgb};
use vello_cpu::{Pixmap, RenderContext, Resources};

use crate::anim::{keys, phase, Ease, SOFT};
use crate::look::{Bot, Mood, Rgb};
use crate::Framing;

const INK: Rgb = Rgb(0x1B1B2E);
const WHITE: Rgb = Rgb(0xFFFFFF);
const IRI: [(f32, [f32; 3]); 6] = [
    (0.0, [1.0, 1.0, 1.0]),
    (0.2, [1.0, 0.702, 0.902]),
    (0.4, [0.659, 0.894, 1.0]),
    (0.6, [1.0, 0.957, 0.690]),
    (0.8, [0.722, 1.0, 0.878]),
    (1.0, [1.0, 1.0, 1.0]),
];

/// A bubble's colour kit: the light, the strong and the accent tone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Tint(pub [Rgb; 3]);

impl Tint {
    pub const ORANGE: Tint = Tint([Rgb(0xFFB58A), Rgb(0xFF6A2B), Rgb(0xFFDB7A)]);
    pub const GREEN: Tint = Tint([Rgb(0x8EF0C0), Rgb(0x1FC27A), Rgb(0x9AD4FF)]);
    pub const AMBER: Tint = Tint([Rgb(0xFFDB7A), Rgb(0xFFAA1F), Rgb(0xFFB58A)]);
    pub const RED: Tint = Tint([Rgb(0xFF9AA6), Rgb(0xF2364F), Rgb(0xC7B4FF)]);
    pub const BLUE: Tint = Tint([Rgb(0x9AD4FF), Rgb(0x1E95F0), Rgb(0x8EF0C0)]);
    pub const VIOLET: Tint = Tint([Rgb(0xC7B4FF), Rgb(0x7B4DF5), Rgb(0xFF9AA6)]);
    pub const ALL: [Tint; 6] = [
        Tint::ORANGE,
        Tint::GREEN,
        Tint::AMBER,
        Tint::RED,
        Tint::BLUE,
        Tint::VIOLET,
    ];

    /// The strong tone, for dots, rings and accents around the bot.
    pub fn strong(self) -> Rgb {
        self.0[1]
    }
}

/// How the little bubbles sit around the big one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Layout {
    Solo,
    Pair,
    Trio,
    Orbit,
    Rising,
    Shield,
    Think,
    Busy,
    Splash,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Eyes {
    Dots,
    Smile,
    Lens,
    Visor,
    Sleepy,
    Squint,
    Happy,
    Up,
    You,
    Read,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mouth {
    None,
    Grin,
    Small,
    Flat,
    Open,
    O,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Sway {
    Float,
    Steady,
    Bounce,
    Hop,
    Wobble,
    Drift,
    Pop,
    Hope,
}

/// What a bot looks like: its colour, bubbles, eyes and way of moving.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Bubble {
    pub tint: Tint,
    pub layout: Layout,
    pub eyes: Eyes,
    pub sway: Sway,
}

/// A bubble in a mood: the bubble with its face and props for that moment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Pose {
    pub tint: Tint,
    pub layout: Layout,
    pub eyes: Eyes,
    pub mouth: Mouth,
    pub sway: Sway,
    pub sparks: bool,
    pub tick: bool,
    pub ask: bool,
    /// The light sweep across the bubble: for hover and for moments worth
    /// marking (a job done), not all the time.
    pub shine: bool,
}

impl Bubble {
    pub const LAYOUTS: [Layout; 6] = [
        Layout::Solo,
        Layout::Pair,
        Layout::Trio,
        Layout::Orbit,
        Layout::Rising,
        Layout::Shield,
    ];
    pub const EYES: [Eyes; 6] = [
        Eyes::Dots,
        Eyes::Smile,
        Eyes::Lens,
        Eyes::Visor,
        Eyes::Sleepy,
        Eyes::Squint,
    ];
    pub const SWAYS: [Sway; 5] = [
        Sway::Float,
        Sway::Bounce,
        Sway::Hop,
        Sway::Wobble,
        Sway::Drift,
    ];

    /// The bubble picked by `seed`, for "Surprise me".
    pub fn from_seed(seed: u64) -> Bubble {
        let mut s = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03;
        let mut next = |n: usize| {
            s ^= s >> 31;
            s = s.wrapping_mul(0xBF58_476D_1CE4_E5B9);
            s ^= s >> 29;
            (s % n as u64) as usize
        };
        Bubble {
            tint: Tint::ALL[next(6)],
            layout: Self::LAYOUTS[next(6)],
            eyes: Self::EYES[next(6)],
            sway: Self::SWAYS[next(5)],
        }
    }

    pub fn pose(self, mood: Mood) -> Pose {
        let mouth = match self.eyes {
            Eyes::Lens | Eyes::Sleepy => Mouth::Small,
            Eyes::Squint => Mouth::Flat,
            Eyes::Smile => Mouth::Open,
            _ => Mouth::Grin,
        };
        let base = Pose {
            tint: self.tint,
            layout: self.layout,
            eyes: self.eyes,
            mouth,
            sway: self.sway,
            sparks: false,
            tick: false,
            ask: false,
            shine: false,
        };
        match mood {
            Mood::Signature => base,
            Mood::Idle => Pose {
                sway: Sway::Float,
                ..base
            },
            Mood::Thinking => Pose {
                layout: Layout::Think,
                eyes: Eyes::Up,
                mouth: Mouth::O,
                sway: Sway::Float,
                ..base
            },
            Mood::Working => Pose {
                layout: Layout::Busy,
                eyes: Eyes::Read,
                mouth: Mouth::Grin,
                sway: Sway::Bounce,
                sparks: true,
                ..base
            },
            Mood::Done => Pose {
                layout: Layout::Splash,
                eyes: Eyes::Happy,
                mouth: Mouth::Open,
                sway: Sway::Pop,
                sparks: true,
                tick: true,
                shine: true,
                ..base
            },
            Mood::NeedsYou => Pose {
                layout: Layout::Pair,
                eyes: Eyes::You,
                mouth: Mouth::Small,
                sway: Sway::Hope,
                ask: true,
                ..base
            },
            Mood::Asleep => Pose {
                layout: Layout::Solo,
                eyes: Eyes::Sleepy,
                mouth: Mouth::Small,
                sway: Sway::Drift,
                ..base
            },
        }
    }
}

impl Bot {
    /// The bot's bubble, as in the design.
    pub fn bubble(self) -> Bubble {
        let (tint, layout, eyes, sway) = match self {
            Bot::Builder => (Tint::ORANGE, Layout::Trio, Eyes::Dots, Sway::Hop),
            Bot::Checker => (Tint::GREEN, Layout::Orbit, Eyes::Lens, Sway::Float),
            Bot::Guard => (Tint::AMBER, Layout::Shield, Eyes::Visor, Sway::Steady),
            Bot::Fixer => (Tint::RED, Layout::Pair, Eyes::Squint, Sway::Wobble),
            Bot::Explainer => (Tint::BLUE, Layout::Rising, Eyes::Smile, Sway::Bounce),
            Bot::Keeper => (Tint::VIOLET, Layout::Rising, Eyes::Sleepy, Sway::Drift),
        };
        Bubble {
            tint,
            layout,
            eyes,
            sway,
        }
    }
}

/// A loop of `dur` seconds through `ks` with the design's soft easing.
fn kf(t: f64, dur: f64, delay: f64, ks: &[(f64, f64)]) -> f64 {
    keys(phase(t, dur, delay), ks, SOFT)
}

fn kfe(t: f64, dur: f64, delay: f64, ks: &[(f64, f64)], e: Ease) -> f64 {
    keys(phase(t, dur, delay), ks, e)
}

/// The sway and breath of the whole bot, as a transform in view-box units.
fn body_xf(sway: Sway, t: f64) -> Affine {
    let c = (100.0, 110.0);
    let around = |a: Affine| Affine::translate(c) * a * Affine::translate((-c.0, -c.1));
    let m = match sway {
        Sway::Float => {
            Affine::translate((0.0, kf(t, 5.0, 0.0, &[(0.0, 0.0), (0.5, -6.0), (1.0, 0.0)])))
        }
        Sway::Steady => {
            Affine::translate((0.0, kf(t, 7.0, 0.0, &[(0.0, 0.0), (0.5, -6.0), (1.0, 0.0)])))
        }
        Sway::Bounce => {
            Affine::translate((0.0, kf(t, 1.8, 0.0, &[(0.0, 0.0), (0.5, -5.0), (1.0, 0.0)])))
        }
        Sway::Hop => {
            let k = [
                (0.0, 0.0),
                (0.55, 0.0),
                (0.62, 2.0),
                (0.74, -12.0),
                (0.86, 0.0),
                (0.93, 0.0),
                (1.0, 0.0),
            ];
            let sx = [
                (0.0, 1.0),
                (0.55, 1.0),
                (0.62, 1.04),
                (0.74, 0.98),
                (0.86, 1.03),
                (0.93, 1.0),
                (1.0, 1.0),
            ];
            let sy = [
                (0.0, 1.0),
                (0.55, 1.0),
                (0.62, 0.95),
                (0.74, 1.03),
                (0.86, 0.97),
                (0.93, 1.0),
                (1.0, 1.0),
            ];
            Affine::translate((0.0, kf(t, 3.2, 0.0, &k)))
                * around(Affine::scale_non_uniform(
                    kf(t, 3.2, 0.0, &sx),
                    kf(t, 3.2, 0.0, &sy),
                ))
        }
        Sway::Wobble => {
            let r = kf(t, 4.0, 0.0, &[(0.0, -5.0), (0.5, 5.0), (1.0, -5.0)]);
            let y = kf(t, 4.0, 0.0, &[(0.0, 0.0), (0.5, -3.0), (1.0, 0.0)]);
            around(Affine::rotate(r.to_radians()) * Affine::translate((0.0, y)))
        }
        Sway::Drift => {
            let r = kf(t, 7.0, 0.0, &[(0.0, -3.0), (0.5, 3.0), (1.0, -3.0)]);
            let y = kf(t, 7.0, 0.0, &[(0.0, 0.0), (0.5, -8.0), (1.0, 0.0)]);
            around(Affine::translate((0.0, y)) * Affine::rotate(r.to_radians()))
        }
        Sway::Pop => {
            let sx = [
                (0.0, 1.0),
                (0.4, 1.0),
                (0.5, 0.9),
                (0.6, 1.12),
                (0.72, 0.97),
                (0.84, 1.0),
                (1.0, 1.0),
            ];
            let sy = [
                (0.0, 1.0),
                (0.4, 1.0),
                (0.5, 1.08),
                (0.6, 0.92),
                (0.72, 1.03),
                (0.84, 1.0),
                (1.0, 1.0),
            ];
            around(Affine::scale_non_uniform(
                kf(t, 3.2, 0.0, &sx),
                kf(t, 3.2, 0.0, &sy),
            ))
        }
        Sway::Hope => {
            let r = kf(t, 4.0, 0.0, &[(0.0, -3.0), (0.5, 3.0), (1.0, -3.0)]);
            let y = kf(t, 4.0, 0.0, &[(0.0, 0.0), (0.5, -4.0), (1.0, 0.0)]);
            around(Affine::rotate(r.to_radians()) * Affine::translate((0.0, y)))
        }
    };
    let bx = kf(t, 4.8, 0.0, &[(0.0, 1.0), (0.5, 1.02), (1.0, 1.0)]);
    let by = kf(t, 4.8, 0.0, &[(0.0, 1.0), (0.5, 0.98), (1.0, 1.0)]);
    m * around(Affine::scale_non_uniform(bx, by))
}

/// The circles that make the body at time `t`: (x, y, r).
fn circles(layout: Layout, t: f64) -> Vec<(f32, f32, f32)> {
    let mut v = vec![(100.0, 110.0, 48.0)];
    let mut add = |x: f64, y: f64, r: f64| v.push((x as f32, y as f32, r as f32));
    let pr1 = |t: f64| {
        let ks_x = [(0.0, 8.0), (0.5, -10.0), (1.0, 8.0)];
        let ks_y = [(0.0, -7.0), (0.5, 9.0), (1.0, -7.0)];
        let s = kf(t, 4.2, 0.0, &[(0.0, 1.0), (0.5, 0.85), (1.0, 1.0)]);
        (kf(t, 4.2, 0.0, &ks_x), kf(t, 4.2, 0.0, &ks_y), s)
    };
    let orbit = |t: f64, dur: f64, delay: f64, r: f64, add: &mut dyn FnMut(f64, f64, f64)| {
        let a = phase(t, dur, delay) * std::f64::consts::TAU;
        // (100, 50) turned about (100, 110).
        add(100.0 + 60.0 * a.sin(), 110.0 - 60.0 * a.cos(), r);
    };
    match layout {
        Layout::Solo | Layout::Shield => {
            let x = kf(t, 5.0, 0.0, &[(0.0, 0.0), (0.5, -6.0), (1.0, 0.0)]);
            let y = kf(t, 5.0, 0.0, &[(0.0, 0.0), (0.5, -8.0), (1.0, 0.0)]);
            let s = kf(t, 5.0, 0.0, &[(0.0, 1.0), (0.5, 1.2), (1.0, 1.0)]);
            add(150.0 + x, 60.0 + y, 12.0 * s);
        }
        Layout::Pair | Layout::Trio => {
            let (x, y, s) = pr1(t);
            add(150.0 + x, 80.0 + y, 21.0 * s);
            if layout == Layout::Trio {
                let x = kf(t, 5.0, -1.6, &[(0.0, -7.0), (0.5, 10.0), (1.0, -7.0)]);
                let y = kf(t, 5.0, -1.6, &[(0.0, -6.0), (0.5, 10.0), (1.0, -6.0)]);
                let s = kf(t, 5.0, -1.6, &[(0.0, 1.0), (0.5, 0.8), (1.0, 1.0)]);
                add(52.0 + x, 72.0 + y, 15.0 * s);
            }
        }
        Layout::Orbit => {
            orbit(t, 6.0, 0.0, 15.0, &mut add);
            orbit(t, 6.0, -3.0, 13.0, &mut add);
        }
        Layout::Busy => {
            orbit(t, 3.0, 0.0, 15.0, &mut add);
            orbit(t, 3.0, -1.0, 13.0, &mut add);
            orbit(t, 3.0, -2.0, 11.0, &mut add);
        }
        Layout::Rising | Layout::Think => {
            type Rise = (f64, (f64, f64), (f64, f64), f64, f64, (f64, f64), [f64; 3]);
            let (dur, from, to, s0, s1, base, rs): Rise = if layout == Layout::Rising {
                (
                    3.6,
                    (-4.0, 14.0),
                    (34.0, -62.0),
                    0.3,
                    1.1,
                    (118.0, 78.0),
                    [14.0, 12.0, 10.0],
                )
            } else {
                (
                    3.0,
                    (-8.0, 14.0),
                    (40.0, -56.0),
                    0.3,
                    1.15,
                    (130.0, 74.0),
                    [14.0, 12.0, 10.0],
                )
            };
            let step = if layout == Layout::Rising { 1.2 } else { 1.0 };
            for (i, r) in rs.iter().enumerate() {
                let p = phase(t, dur, -(i as f64) * step);
                let k = SOFT.at(p);
                let x = from.0 + (to.0 - from.0) * k;
                let y = from.1 + (to.1 - from.1) * k;
                let s = s0 + (s1 - s0) * k;
                // Fading out in the goo mask shrinks a bubble until it is gone.
                let fade = if layout == Layout::Rising {
                    if p < 0.6 {
                        1.0
                    } else {
                        1.0 - SOFT.at((p - 0.6) / 0.4)
                    }
                } else {
                    1.0 - k
                };
                let fade = ((fade - 0.25) / 0.75).clamp(0.0, 1.0);
                add(base.0 + x, base.1 + y, r * s * fade);
            }
        }
        Layout::Splash => {
            let out = [(-56.0, -46.0), (58.0, -40.0), (-52.0, 40.0), (54.0, 44.0)];
            let rs = [17.0, 15.0, 13.0, 14.0];
            for (o, r) in out.iter().zip(rs) {
                let p = phase(t, 3.2, 0.0);
                let k = keys(p, &[(0.0, 0.0), (0.5, 0.0), (0.7, 1.0), (1.0, 0.0)], SOFT);
                let s = 0.4 + 0.6 * k;
                add(100.0 + o.0 * k, 110.0 + o.1 * k, r * s);
            }
        }
    }
    v
}

/// The soft colour blobs swirling inside: (x, y, r, colour index, alpha).
fn blobs(t: f64) -> [(f32, f32, f32, usize, f32); 6] {
    let alt = |dur: f64| crate::anim::alternate(t, dur, SOFT);
    let b = |x: f64, y: f64, r: f64, c: usize, a: f64, dur: f64, dx: f64, dy: f64, s: f64| {
        let k = alt(dur);
        (
            (x + dx * k) as f32,
            (y + dy * k) as f32,
            (r * (1.0 + (s - 1.0) * k)) as f32,
            c,
            a as f32,
        )
    };
    [
        b(70.0, 80.0, 42.0, 0, 1.0, 9.0, 60.0, 40.0, 1.15),
        b(140.0, 140.0, 40.0, 1, 1.0, 11.0, -70.0, -50.0, 0.9),
        b(140.0, 60.0, 30.0, 2, 1.0, 7.0, -50.0, 60.0, 1.2),
        b(50.0, 150.0, 34.0, 1, 0.85, 8.0, 70.0, -60.0, 1.1),
        b(36.0, 96.0, 30.0, 2, 1.0, 10.0, 20.0, 50.0, 1.2),
        b(100.0, 30.0, 30.0, 1, 0.7, 12.0, 50.0, 10.0, 0.9),
    ]
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn smin(a: f32, b: f32, k: f32) -> f32 {
    let h = (k - (a - b).abs()).max(0.0) / k;
    a.min(b) - h * h * k * 0.25
}

/// The rainbow film at `(x, y)` in view-box units, turning once every 9 s.
fn iri(x: f32, y: f32, turn: (f32, f32)) -> [f32; 3] {
    // objectBoundingBox of the 240 × 240 masked rect at (-20, -20).
    let u = (x + 20.0) / 240.0 - 0.5;
    let v = (y + 20.0) / 240.0 - 0.5;
    let s = (0.5 + 0.5 * (u * turn.0 + v * turn.1)).clamp(0.0, 1.0);
    for w in IRI.windows(2) {
        if s <= w[1].0 {
            let k = (s - w[0].0) / (w[1].0 - w[0].0);
            let (a, b) = (w[0].1, w[1].1);
            return [
                a[0] + (b[0] - a[0]) * k,
                a[1] + (b[1] - a[1]) * k,
                a[2] + (b[2] - a[2]) * k,
            ];
        }
    }
    [1.0; 3]
}

fn rgbf(c: Rgb) -> [f32; 3] {
    [
        c.r() as f32 / 255.0,
        c.g() as f32 / 255.0,
        c.b() as f32 / 255.0,
    ]
}

/// Straight colour `c` at alpha `a` over a premultiplied pixel.
fn over(px: &mut [f32; 4], c: [f32; 3], a: f32) {
    let k = 1.0 - a;
    px[0] = c[0] * a + px[0] * k;
    px[1] = c[1] * a + px[1] * k;
    px[2] = c[2] * a + px[2] * k;
    px[3] = a + px[3] * k;
}

/// The part of the view box a framing shows: (x0, y0, side).
fn view(framing: Framing) -> (f64, f64, f64) {
    match framing {
        Framing::Full => (14.0, 28.0, 172.0),
        Framing::Bust => (34.0, 40.0, 132.0),
    }
}

/// Draws bubbles into premultiplied RGBA8, reusing buffers between frames.
pub struct BubbleRenderer {
    px: u16,
    ctx: RenderContext,
    resources: Resources,
    out: Vec<u8>,
}

type Color = AlphaColor<Srgb>;

fn color(c: Rgb, a: f64) -> Color {
    Color::from_rgba8(
        c.r(),
        c.g(),
        c.b(),
        (a.clamp(0.0, 1.0) * 255.0).round() as u8,
    )
}

fn round(w: f64) -> Stroke {
    Stroke::new(w).with_caps(Cap::Round).with_join(Join::Round)
}

fn svg(d: &str) -> BezPath {
    BezPath::from_svg(d).unwrap_or_default()
}

struct Ink<'a> {
    ctx: &'a mut RenderContext,
    xf: Affine,
}

impl Ink<'_> {
    fn fill(&mut self, shape: &impl Shape, at: Affine, c: Rgb, a: f64) {
        if a <= 0.004 {
            return;
        }
        self.ctx.set_transform(self.xf * at);
        self.ctx.set_paint(color(c, a));
        self.ctx.fill_path(&shape.to_path(0.05));
    }

    fn stroke(&mut self, shape: &impl Shape, at: Affine, w: f64, c: Rgb, a: f64) {
        if a <= 0.004 {
            return;
        }
        self.ctx.set_transform(self.xf * at);
        self.ctx.set_stroke(round(w));
        self.ctx.set_paint(color(c, a));
        self.ctx.stroke_path(&shape.to_path(0.05));
    }
}

fn about(c: (f64, f64), a: Affine) -> Affine {
    Affine::translate(c) * a * Affine::translate((-c.0, -c.1))
}

impl BubbleRenderer {
    pub fn new(px: u16) -> Self {
        BubbleRenderer {
            px,
            ctx: RenderContext::new(px, px),
            resources: Resources::new(),
            out: vec![0; px as usize * px as usize * 4],
        }
    }

    pub fn size(&self) -> u16 {
        self.px
    }

    /// Draws `pose` at time `t`; premultiplied RGBA8, `size()` pixels square.
    pub fn draw(&mut self, pose: &Pose, t: f64, framing: Framing) -> &[u8] {
        let n = self.px as usize;
        let (x0, y0, side) = view(framing);
        let k = n as f64 / side;
        let view_xf = Affine::scale(k) * Affine::translate((-x0, -y0));
        let bot = body_xf(pose.sway, t);
        let small = n < 40;
        self.shade(pose, t, framing, bot, small);
        // Face, highlights and props, drawn with Vello and laid over the body.
        self.ctx.reset();
        {
            let mut ink = Ink {
                ctx: &mut self.ctx,
                xf: view_xf,
            };
            face(&mut ink, pose, t, bot, small);
        }
        self.ctx.flush();
        let mut pix = Pixmap::new(self.px, self.px);
        self.ctx.render(&mut pix, &mut self.resources);
        for (o, f) in self
            .out
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(pix.data_as_u8_slice().as_chunks::<4>().0)
        {
            let fa = f[3] as u32;
            if fa == 0 {
                continue;
            }
            let k = 255 - fa;
            for i in 0..4 {
                o[i] = (f[i] as u32 + (o[i] as u32 * k + 127) / 255).min(255) as u8;
            }
        }
        &self.out
    }

    /// The shadow, the shield ring and the glass body, per pixel.
    fn shade(&mut self, pose: &Pose, t: f64, framing: Framing, bot: Affine, small: bool) {
        let n = self.px as usize;
        let (x0, y0, side) = view(framing);
        let ps = (side / n as f64) as f32;
        let inv = bot.inverse();
        let ic = inv.as_coeffs().map(|v| v as f32);
        let cs: Vec<(f32, f32, f32)> = circles(pose.layout, t)
            .into_iter()
            .filter(|c| c.2 > 0.5)
            .collect();
        let bl = blobs(t);
        let tint = [
            rgbf(pose.tint.0[0]),
            rgbf(pose.tint.0[1]),
            rgbf(pose.tint.0[2]),
        ];
        let a = phase(t, 9.0, 0.0) * std::f64::consts::TAU;
        // The diagonal (1, 1) turned by the gradient's rotation.
        let turn = (((a.cos() - a.sin()) as f32), ((a.sin() + a.cos()) as f32));
        // The shine: a soft stripe sweeping across, then a pause, every 2.4 s.
        let sh = keys(
            phase(t, 2.4, 0.0),
            &[(0.0, 0.0), (0.55, 300.0), (1.0, 300.0)],
            SOFT,
        ) as f32;
        let (sin24, cos24) = (24f32.to_radians().sin(), 24f32.to_radians().cos());
        // Shadow under the bot.
        let gp = phase(t, 7.0, 0.0);
        let gsx = keys(gp, &[(0.0, 1.0), (0.5, 0.88), (1.0, 1.0)], SOFT) as f32;
        let ga = keys(gp, &[(0.0, 0.35), (0.5, 0.22), (1.0, 0.35)], SOFT) as f32;
        let strong = tint[1];
        // Shield ring.
        let shield = pose.layout == Layout::Shield;
        let ring = |delay: f64| {
            let p = phase(t, 3.4, delay);
            (
                keys(p, &[(0.0, 0.96), (0.5, 1.04), (1.0, 0.96)], SOFT) as f32 * 70.0,
                keys(p, &[(0.0, 0.9), (0.5, 0.5), (1.0, 0.9)], SOFT) as f32,
            )
        };
        let (r1, a1) = ring(0.0);
        let (r2, a2) = ring(-1.7);
        // Where the body can be, in bot space, to skip the rest quickly.
        let reach = cs
            .iter()
            .fold((f32::MAX, f32::MAX, f32::MIN, f32::MIN), |b, c| {
                (
                    b.0.min(c.0 - c.2),
                    b.1.min(c.1 - c.2),
                    b.2.max(c.0 + c.2),
                    b.3.max(c.1 + c.2),
                )
            });
        let pad = 10.0;
        let smooth = 10.0;
        for py in 0..n {
            for px in 0..n {
                let vx = x0 as f32 + (px as f32 + 0.5) * ps;
                let vy = y0 as f32 + (py as f32 + 0.5) * ps;
                let mut p = [0.0f32; 4];
                // Shadow (stays on the ground while the bot moves).
                let dx = (vx - 100.0) / (48.0 * gsx + 5.0);
                let dy = (vy - 186.0) / 9.0;
                let q = dx * dx + dy * dy;
                if q < 4.0 {
                    over(&mut p, strong, ga * (-q * 1.6).exp());
                }
                // Bot space.
                let bx = ic[0] * vx + ic[2] * vy + ic[4];
                let by = ic[1] * vx + ic[3] * vy + ic[5];
                if shield {
                    let d = ((bx - 100.0).powi(2) + (by - 110.0).powi(2)).sqrt();
                    if d < r1 {
                        over(
                            &mut p,
                            [1.0; 3],
                            0.08 * a1 * ((r1 - d) / ps).clamp(0.0, 1.0),
                        );
                    }
                    let c1 = ((1.5 - (d - r1).abs()) / ps + 0.5).clamp(0.0, 1.0);
                    if c1 > 0.0 {
                        over(&mut p, iri(bx, by, turn), c1 * a1);
                    }
                    let c2 = ((0.5 - (d - r2).abs()) / ps + 0.5).clamp(0.0, 1.0);
                    if c2 > 0.0 {
                        over(&mut p, [1.0; 3], c2 * 0.6 * a2);
                    }
                }
                if bx >= reach.0 - pad
                    && bx <= reach.2 + pad
                    && by >= reach.1 - pad
                    && by <= reach.3 + pad
                {
                    let mut d = f32::MAX;
                    for c in &cs {
                        let e = ((bx - c.0).powi(2) + (by - c.1).powi(2)).sqrt() - c.2;
                        d = if d == f32::MAX { e } else { smin(d, e, smooth) };
                    }
                    let cov = (0.5 - d / ps).clamp(0.0, 1.0);
                    if cov > 0.0 {
                        // The inside: faint white, colour blobs, the film, the shine.
                        let mut q = [0.0f32; 4];
                        over(&mut q, [1.0; 3], 0.2);
                        if !small {
                            for b in &bl {
                                let e = ((bx - b.0).powi(2) + (by - b.1).powi(2)).sqrt() - b.2;
                                let a = 1.0 - smoothstep(-18.0, 18.0, e);
                                if a > 0.0 {
                                    over(&mut q, tint[b.3], a * b.4);
                                }
                            }
                        } else {
                            // Tiny bots: one soft wash instead of six blobs.
                            let k = smoothstep(60.0, 160.0, bx + by);
                            let c = [
                                tint[0][0] + (tint[1][0] - tint[0][0]) * k,
                                tint[0][1] + (tint[1][1] - tint[0][1]) * k,
                                tint[0][2] + (tint[1][2] - tint[0][2]) * k,
                            ];
                            over(&mut q, c, 0.9);
                        }
                        // Screen the rainbow film at 22 %.
                        let f = iri(bx, by, turn);
                        for i in 0..3 {
                            let c = q[i];
                            let s = c + f[i] * q[3] - c * f[i];
                            q[i] = c + (s - c) * 0.22;
                        }
                        if pose.shine {
                            // Stripe: x' along the rotated axis.
                            let rx = (bx - 0.0) * cos24 + (by - 100.0) * sin24;
                            let center = -45.0 + sh;
                            let w = (rx - center).abs();
                            let a = 0.45 * (1.0 - smoothstep(8.0, 22.0, w));
                            if a > 0.0 {
                                over(&mut q, [1.0; 3], a);
                            }
                        }
                        // White inner edge, then the rainbow rim.
                        let depth = -d;
                        let edge = 0.75 * (1.0 - smoothstep(1.0, 9.0, depth));
                        over(&mut q, [1.0; 3], edge);
                        let rim = ((1.8 - depth) / ps + 0.5).clamp(0.0, 1.0);
                        if rim > 0.0 {
                            over(&mut q, f, rim);
                        }
                        // Masked by the body's coverage, over what is below.
                        let k = 1.0 - q[3] * cov;
                        for i in 0..3 {
                            p[i] = q[i] * cov + p[i] * k;
                        }
                        p[3] = q[3] * cov + p[3] * k;
                    }
                }
                let o = (py * n + px) * 4;
                for (d, v) in self.out[o..o + 4].iter_mut().zip(p) {
                    *d = (v * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
                }
            }
        }
    }
}

/// Eyes, mouth, cheeks, highlights and the props around the bot.
fn face(ink: &mut Ink, pose: &Pose, t: f64, bot: Affine, small: bool) {
    // Highlight arc and the small glint dot.
    let hl = kf(t, 5.0, 0.0, &[(0.0, 0.9), (0.5, 0.5), (1.0, 0.9)]);
    ink.stroke(&svg("M64 98A38 38 0 0 1 84 72"), bot, 4.5, WHITE, hl);
    ink.fill(&Circle::new((76.0, 122.0), 3.0), bot, WHITE, 0.6);
    eyes(ink, pose.eyes, t, bot);
    if !matches!(pose.eyes, Eyes::Lens | Eyes::Visor) {
        let ch = Rgb(0xFF6F91);
        ink.fill(&Ellipse::new((78.0, 116.0), (4.6, 2.6), 0.0), bot, ch, 0.45);
        ink.fill(
            &Ellipse::new((122.0, 116.0), (4.6, 2.6), 0.0),
            bot,
            ch,
            0.45,
        );
    }
    mouth(ink, pose.mouth, t, bot);
    if small {
        return;
    }
    let strong = pose.tint.strong();
    if pose.sparks {
        for (x, y, d) in [(26.0, 60.0, 0.0), (176.0, 50.0, 0.6), (172.0, 150.0, 1.2)] {
            let p = phase(t, 2.0, d);
            let s = keys(p, &[(0.0, 0.2), (0.4, 1.1), (0.7, 0.8), (1.0, 0.2)], SOFT);
            let r = keys(p, &[(0.0, 0.0), (0.4, 45.0), (0.7, 90.0), (1.0, 0.0)], SOFT);
            let a = keys(p, &[(0.0, 0.0), (0.4, 1.0), (0.7, 0.6), (1.0, 0.0)], SOFT);
            let star = svg(&format!(
                "M{x} {}l1.8 5.2l5.2 1.8l-5.2 1.8l-1.8 5.2l-1.8-5.2l-5.2-1.8l5.2-1.8z",
                y - 7.0
            ));
            let at = about((x, y), Affine::rotate(r.to_radians()) * Affine::scale(s));
            ink.fill(&star, at, strong, a);
        }
    }
    if pose.tick {
        let s = kf(
            t,
            3.2,
            0.0,
            &[(0.0, 0.0), (0.5, 0.0), (0.62, 1.2), (0.7, 1.0), (1.0, 1.0)],
        );
        let at = about((160.0, 64.0), Affine::scale(s.max(0.001)));
        ink.fill(&Circle::new((160.0, 64.0), 13.0), at, strong, 1.0);
        ink.stroke(&svg("M154 64.4l4.2 4.2l7.4-8"), at, 3.0, WHITE, 1.0);
    }
    if pose.ask {
        let y = kf(t, 2.4, 0.0, &[(0.0, 0.0), (0.5, -5.0), (1.0, 0.0)]);
        let s = kf(t, 1.8, 0.0, &[(0.0, 1.0), (0.5, 1.12), (1.0, 1.0)]);
        let at = Affine::translate((0.0, y));
        ink.fill(
            &Circle::new((160.0, 62.0), 13.0),
            at * about((160.0, 62.0), Affine::scale(s)),
            Rgb(0xFF6A5C),
            1.0,
        );
        ink.stroke(&svg("M160 55v8"), at, 3.0, WHITE, 1.0);
        ink.fill(&Circle::new((160.0, 68.6), 1.8), at, WHITE, 1.0);
    }
}

/// A look-around loop for the eyes, as a translation.
fn look(kind: u8, t: f64) -> Affine {
    let k2 = |dur: f64, xs: &[(f64, f64)], ys: &[(f64, f64)]| {
        Affine::translate((kf(t, dur, 0.0, xs), kf(t, dur, 0.0, ys)))
    };
    match kind {
        // lookA
        0 => k2(
            8.0,
            &[
                (0.0, 0.0),
                (0.14, 0.0),
                (0.2, 2.0),
                (0.38, 2.0),
                (0.44, -2.0),
                (0.6, -2.0),
                (0.66, 0.4),
                (0.82, 0.4),
                (0.88, 0.0),
                (1.0, 0.0),
            ],
            &[
                (0.0, 0.0),
                (0.14, 0.0),
                (0.2, -0.6),
                (0.38, -0.6),
                (0.44, 0.3),
                (0.6, 0.3),
                (0.66, 1.2),
                (0.82, 1.2),
                (0.88, 0.0),
                (1.0, 0.0),
            ],
        ),
        // lookHunt
        1 => k2(
            4.4,
            &[
                (0.0, -1.6),
                (0.16, -1.6),
                (0.22, 1.8),
                (0.4, 1.8),
                (0.46, -0.4),
                (0.64, -0.4),
                (0.7, -1.6),
                (1.0, -1.6),
            ],
            &[
                (0.0, 0.6),
                (0.16, 0.6),
                (0.22, -0.4),
                (0.4, -0.4),
                (0.46, 1.2),
                (0.64, 1.2),
                (0.7, 0.6),
                (1.0, 0.6),
            ],
        ),
        // lookScan
        2 => k2(
            6.0,
            &[
                (0.0, -2.6),
                (0.1, -2.6),
                (0.4, 2.6),
                (0.5, 2.6),
                (0.8, -2.6),
                (1.0, -2.6),
            ],
            &[(0.0, 0.0), (1.0, 0.0)],
        ),
        // lookUp
        3 => k2(
            5.0,
            &[(0.0, 1.8), (0.5, 2.6), (1.0, 1.8)],
            &[(0.0, -2.4), (0.5, -1.6), (1.0, -2.4)],
        ),
        // lookRead
        _ => Affine::translate((
            kfe(t, 2.8, 0.0, &[(0.0, -2.4), (0.8, 2.4), (1.0, -2.4)], SOFT),
            1.0,
        )),
    }
}

fn blink(double: bool, t: f64) -> f64 {
    if double {
        kf(
            t,
            6.0,
            0.0,
            &[
                (0.0, 1.0),
                (0.4, 1.0),
                (0.43, 0.08),
                (0.47, 1.0),
                (0.51, 0.08),
                (0.55, 1.0),
                (0.58, 1.0),
                (1.0, 1.0),
            ],
        )
    } else {
        kf(
            t,
            5.2,
            0.0,
            &[
                (0.0, 1.0),
                (0.45, 1.0),
                (0.49, 0.08),
                (0.51, 0.08),
                (0.55, 1.0),
                (1.0, 1.0),
            ],
        )
    }
}

fn dot_eyes(ink: &mut Ink, t: f64, bot: Affine, look_at: Affine, double: bool, s: f64) {
    let b = blink(double, t);
    let at = bot * look_at * about((100.0, 104.0), Affine::scale_non_uniform(1.0, b));
    let g = kf(t, 3.0, 0.0, &[(0.0, 1.0), (0.5, 0.55), (1.0, 1.0)]);
    for x in [87.0, 113.0] {
        ink.fill(
            &Ellipse::new((x, 104.0), (4.4 * s, 5.4 * s), 0.0),
            at,
            INK,
            1.0,
        );
        ink.fill(&Circle::new((x + 1.6, 102.0), 1.5), at, WHITE, g);
    }
}

fn eyes(ink: &mut Ink, e: Eyes, t: f64, bot: Affine) {
    match e {
        Eyes::Dots => dot_eyes(ink, t, bot, look(0, t), true, 1.0),
        Eyes::Read => dot_eyes(ink, t, bot, look(4, t), false, 1.0),
        Eyes::Up => {
            let y = kf(t, 4.0, 0.0, &[(0.0, 0.0), (0.5, -2.0), (1.0, 0.0)]);
            ink.stroke(
                &svg("M106 92q7-4 14 0"),
                bot * Affine::translate((0.0, y)),
                2.4,
                INK,
                0.7,
            );
            dot_eyes(ink, t, bot, look(3, t), false, 1.0);
        }
        Eyes::You => {
            ink.stroke(&svg("M80 93q6-3 12 1M108 94q6-4 12-1"), bot, 2.4, INK, 0.7);
            let y = kf(t, 4.0, 0.0, &[(0.0, -0.6), (0.5, -1.0), (1.0, -0.6)]);
            let s = kf(t, 4.0, 0.0, &[(0.0, 1.0), (0.5, 1.08), (1.0, 1.0)]);
            let la = Affine::translate((0.0, y)) * about((100.0, 104.0), Affine::scale(s));
            dot_eyes(ink, t, bot, la, false, 1.12);
        }
        Eyes::Smile => {
            let y = kf(t, 1.8, 0.0, &[(0.0, 0.0), (0.45, -1.6), (1.0, 0.0)]);
            ink.stroke(
                &svg("M81 105q6-7 12 0M107 105q6-7 12 0"),
                bot * Affine::translate((0.0, y)),
                3.0,
                INK,
                1.0,
            );
        }
        Eyes::Happy => ink.stroke(
            &svg("M80 106q7-9 14 0M106 106q7-9 14 0"),
            bot,
            3.4,
            INK,
            1.0,
        ),
        Eyes::Lens => {
            let p = phase(t, 3.2, 0.0);
            let rs = keys(p, &[(0.0, 0.92), (0.5, 1.1), (1.0, 0.92)], SOFT);
            let ra = keys(p, &[(0.0, 0.3), (0.5, 0.9), (1.0, 0.3)], SOFT);
            ink.stroke(
                &Circle::new((100.0, 102.0), 17.0),
                bot * about((100.0, 102.0), Affine::scale(rs)),
                2.6,
                WHITE,
                ra,
            );
            ink.fill(&Circle::new((100.0, 102.0), 12.0), bot, WHITE, 0.9);
            let la = bot * look(2, t);
            let f = keys(p, &[(0.0, 1.0), (0.5, 1.2), (1.0, 1.0)], SOFT);
            ink.fill(
                &Circle::new((100.0, 102.0), 6.0),
                la * about((100.0, 102.0), Affine::scale(f)),
                INK,
                1.0,
            );
            ink.fill(&Circle::new((102.0, 100.0), 1.8), la, WHITE, 1.0);
        }
        Eyes::Visor => {
            let la = bot * look(0, t);
            let b = blink(false, t);
            let at = la * about((100.0, 103.5), Affine::scale_non_uniform(1.0, b));
            for x in [78.0, 104.0] {
                ink.fill(
                    &RoundedRect::from_rect(Rect::new(x, 99.0, x + 18.0, 108.0), 4.5),
                    at,
                    INK,
                    1.0,
                );
            }
            let p = phase(t, 3.4, 0.0);
            let gx = keys(p, &[(0.0, 0.0), (0.3, 0.0), (0.8, 9.0), (1.0, 9.0)], SOFT);
            let ga = keys(
                p,
                &[
                    (0.0, 0.0),
                    (0.3, 0.0),
                    (0.4, 0.9),
                    (0.8, 0.9),
                    (0.9, 0.0),
                    (1.0, 0.0),
                ],
                SOFT,
            );
            for x in [80.0, 106.0] {
                let r = RoundedRect::from_rect(Rect::new(x, 100.6, x + 5.0, 103.2), 1.3);
                ink.fill(&r, la * Affine::translate((gx, 0.0)), WHITE, ga);
            }
        }
        Eyes::Sleepy => {
            let p = phase(t, 7.0, 0.0);
            let open = keys(
                p,
                &[
                    (0.0, 0.0),
                    (0.58, 0.0),
                    (0.64, 1.0),
                    (0.76, 1.0),
                    (0.82, 0.0),
                    (1.0, 0.0),
                ],
                SOFT,
            );
            ink.stroke(
                &svg("M81 103q6 6 12 0M107 103q6 6 12 0"),
                bot,
                3.0,
                INK,
                1.0 - open,
            );
            let s = 0.2 + 0.8 * open;
            let at = bot * about((100.0, 104.0), Affine::scale_non_uniform(1.0, s));
            for x in [87.0, 113.0] {
                ink.fill(&Circle::new((x, 104.0), 3.6), at, INK, open);
            }
        }
        Eyes::Squint => {
            let la = bot * look(1, t);
            let b = blink(false, t);
            let at = la * about((87.0, 104.0), Affine::scale_non_uniform(1.0, b));
            ink.fill(&Ellipse::new((87.0, 104.0), (4.4, 5.4), 0.0), at, INK, 1.0);
            let g = kf(t, 3.0, 0.0, &[(0.0, 1.0), (0.5, 0.55), (1.0, 1.0)]);
            ink.fill(&Circle::new((88.6, 102.0), 1.5), at, WHITE, g);
            let p = phase(t, 3.0, 0.0);
            let sx = keys(
                p,
                &[(0.0, 1.0), (0.7, 1.0), (0.78, 0.6), (0.86, 1.0), (1.0, 1.0)],
                SOFT,
            );
            let sy = keys(
                p,
                &[
                    (0.0, 0.0),
                    (0.7, 0.0),
                    (0.78, -0.6),
                    (0.86, 0.0),
                    (1.0, 0.0),
                ],
                SOFT,
            );
            let at = bot
                * Affine::translate((0.0, sy))
                * about((112.5, 104.0), Affine::scale_non_uniform(sx, 1.0));
            ink.stroke(&svg("M106 104h13"), at, 3.2, INK, 1.0);
        }
    }
}

fn mouth(ink: &mut Ink, m: Mouth, t: f64, bot: Affine) {
    match m {
        Mouth::None => {}
        Mouth::O => {
            let sx = kf(t, 4.0, 0.0, &[(0.0, 1.0), (0.5, 0.8), (1.0, 1.0)]);
            let sy = kf(t, 4.0, 0.0, &[(0.0, 1.0), (0.5, 1.15), (1.0, 1.0)]);
            let at = bot * about((104.0, 121.0), Affine::scale_non_uniform(sx, sy));
            ink.fill(&Ellipse::new((104.0, 121.0), (3.4, 3.0), 0.0), at, INK, 1.0);
        }
        Mouth::Open => {
            ink.fill(
                &svg("M91 117Q100 129 109 117Q100 120 91 117Z"),
                bot,
                INK,
                1.0,
            );
            ink.fill(
                &svg("M95 121.6Q100 125 105 121.6Q100 120.6 95 121.6Z"),
                bot,
                Rgb(0xFF7A8E),
                1.0,
            );
        }
        Mouth::Small => ink.stroke(&svg("M95 121q5 2 10 0"), bot, 2.6, INK, 1.0),
        Mouth::Flat => ink.stroke(&svg("M94 121q6-3 12 0"), bot, 2.6, INK, 1.0),
        Mouth::Grin => {
            let p = phase(t, 6.4, 0.0);
            let sx = keys(
                p,
                &[
                    (0.0, 1.0),
                    (0.62, 1.0),
                    (0.68, 1.25),
                    (0.8, 1.25),
                    (0.86, 1.0),
                    (1.0, 1.0),
                ],
                SOFT,
            );
            let sy = keys(
                p,
                &[
                    (0.0, 1.0),
                    (0.62, 1.0),
                    (0.68, 1.15),
                    (0.8, 1.15),
                    (0.86, 1.0),
                    (1.0, 1.0),
                ],
                SOFT,
            );
            let at = bot * about((100.0, 119.5), Affine::scale_non_uniform(sx, sy));
            ink.stroke(&svg("M93 118q7 5 14 0"), at, 2.6, INK, 1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::look::Mood;

    fn cover(px: &[u8]) -> usize {
        px.chunks(4).filter(|p| p[3] > 128).count()
    }

    #[test]
    fn every_bubble_and_mood_draws() {
        for tint in Tint::ALL {
            for (i, layout) in Bubble::LAYOUTS.into_iter().enumerate() {
                let b = Bubble {
                    tint,
                    layout,
                    eyes: Bubble::EYES[i],
                    sway: Bubble::SWAYS[i % 5],
                };
                for mood in Mood::ALL {
                    let mut r = BubbleRenderer::new(64);
                    let px = r.draw(&b.pose(mood), 1.3, Framing::Full);
                    assert!(cover(px) > 300, "{b:?} {mood:?}");
                }
            }
        }
    }

    #[test]
    fn moves_over_time() {
        let b = Bubble::from_seed(3).pose(Mood::Working);
        let mut r = BubbleRenderer::new(64);
        let a = r.draw(&b, 0.0, Framing::Full).to_vec();
        assert_ne!(a, r.draw(&b, 0.5, Framing::Full));
    }

    #[test]
    fn shines_only_when_asked() {
        let calm = Bot::Builder.bubble().pose(Mood::Idle);
        assert!(!calm.shine);
        assert!(Bot::Builder.bubble().pose(Mood::Done).shine);
        let lit = Pose {
            shine: true,
            ..calm
        };
        for px in [34, 96] {
            let mut r = BubbleRenderer::new(px);
            // Mid-sweep, the stripe crosses the bubble.
            let a = r.draw(&calm, 0.66, Framing::Full).to_vec();
            assert_ne!(a, r.draw(&lit, 0.66, Framing::Full));
        }
    }
}
