//! Fin: Squad Code's mascot. A hex bot with a glass shell that hovers on a ring
//! of light and does its feeling with its fins and brows.
//!
//! A port of the "Fin character sheet" and "Fin props and roles" designs, drawn
//! on the design's 120 × 120 grid. Like the bubble crew, a frame is a pure
//! function of time: every CSS keyframe from the design is evaluated through
//! [`crate::anim`], so the same pose plays the same everywhere.
//!
//! Three signals carry everything: the fins show effort, the brows show mood,
//! and the ring under Fin shows status. Fin comes in three materials (liquid
//! glass, a rainbow "bubble glow" glass and solid) and can wear one prop on each
//! of four anchors: head, left fin, right fin and chest.

use vello_cpu::kurbo::{
    Affine, BezPath, Cap, Circle, Ellipse, Join, Point, Rect, RoundedRect, Shape, Stroke,
};
use vello_cpu::peniko::{
    color::{AlphaColor, Srgb},
    ColorStop, ColorStops, Fill, Gradient, LinearGradientPosition, RadialGradientPosition,
};
use vello_cpu::{Pixmap, RenderContext, Resources};

use crate::anim::{alternate, keys, phase, Ease};
use crate::bubble::Tint;
use crate::look::{Mood, Rgb};
use crate::Framing;

type Color = AlphaColor<Srgb>;

const WHITE: Rgb = Rgb(0xFFFFFF);
const BLACK: Rgb = Rgb(0x000000);
const INK: Rgb = Rgb(0x141416);

/// CSS `ease-in-out`.
const EIO: Ease = Ease(0.42, 0.0, 0.58, 1.0);
/// CSS `ease-out`.
const EOUT: Ease = Ease(0.0, 0.0, 0.58, 1.0);
/// CSS `ease`, the default timing function.
const EASE: Ease = Ease(0.25, 0.1, 0.25, 1.0);
/// The hop's `cubic-bezier(.3, 0, .3, 1)`.
const HOP: Ease = Ease(0.3, 0.0, 0.3, 1.0);
const LINEAR: Ease = Ease(0.0, 0.0, 1.0, 1.0);

/// The rainbow the bubble glow material turns through.
const RAINBOW: [Rgb; 7] = [
    Rgb(0xFF7AD9),
    Rgb(0xA98BFF),
    Rgb(0x5FCBFF),
    Rgb(0x6FF2C6),
    Rgb(0xFFE06A),
    Rgb(0xFF8F7A),
    Rgb(0xFF7AD9),
];

/// What Fin is doing. Effort lives in the fins, mood in the brows, status in
/// the ring.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum State {
    /// Hovers and breathes, fins drift, blinks now and then.
    Idle,
    /// Leans in, fins perk up, the ring sends out ripples.
    Listen,
    /// Tilts and scratches its head with a fin. Thought bubbles pop.
    Think,
    /// Fins flap hard, brows dig in, the visor scans, sparks rise.
    Work,
    /// Squash, jump, stretch. Fins cheer and sparkles burst.
    Done,
    /// Shakes it off. The visor flushes red, fins droop, the ring flickers.
    Error,
    /// Lands low on a dim ring, fins folded, floating Zs.
    Sleep,
    /// Leans over and waves with one fin.
    Hello,
}

impl State {
    pub const ALL: [State; 8] = [
        State::Idle,
        State::Listen,
        State::Think,
        State::Work,
        State::Done,
        State::Error,
        State::Sleep,
        State::Hello,
    ];

    pub fn name(self) -> &'static str {
        match self {
            State::Idle => "Idle",
            State::Listen => "Listening",
            State::Think => "Thinking",
            State::Work => "Working",
            State::Done => "Done",
            State::Error => "Error",
            State::Sleep => "Sleeping",
            State::Hello => "Hello",
        }
    }

    /// The state that shows a crew mood.
    pub fn of(mood: Mood) -> State {
        match mood {
            Mood::Signature => State::Hello,
            Mood::Idle => State::Idle,
            Mood::Thinking => State::Think,
            Mood::Working => State::Work,
            Mood::Done => State::Done,
            Mood::NeedsYou => State::Listen,
            Mood::Asleep => State::Sleep,
        }
    }
}

/// What Fin's shell and fins are made of.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Material {
    /// Tinted liquid glass with a bright rim and a soft inner glow.
    Glass,
    /// Clear iridescent glass that slowly shifts through the rainbow, with a
    /// glowing rainbow rim.
    Bubble,
    /// Solid, glossy plastic in the crew colour.
    Solid,
}

impl Material {
    pub const ALL: [Material; 3] = [Material::Glass, Material::Bubble, Material::Solid];

    pub fn name(self) -> &'static str {
        match self {
            Material::Glass => "Liquid glass",
            Material::Bubble => "Bubble glow",
            Material::Solid => "Solid",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Material::Glass => "glass",
            Material::Bubble => "bubble",
            Material::Solid => "solid",
        }
    }

    pub fn from_id(id: &str) -> Material {
        Material::ALL
            .into_iter()
            .find(|m| m.id() == id)
            .unwrap_or(Material::Glass)
    }
}

/// A prop on Fin's head. Hats that cover the crown hide the dorsal fin.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Hat {
    None,
    Security,
    Headset,
    Bandana,
    HardHat,
    Grad,
    Cap,
    Beret,
    Crown,
}

/// A prop held in the left fin.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Held {
    None,
    Shield,
    Clipboard,
    Parcel,
    Coffee,
}

/// A tool held in the right fin.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tool {
    None,
    Baton,
    Broom,
    Wrench,
    Magnifier,
    Pencil,
}

/// A badge on Fin's chest, under the visor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Badge {
    None,
    Star,
    NameTag,
    Id,
    Gear,
    Heart,
}

macro_rules! named {
    ($t:ty, [$($v:ident => ($id:literal, $name:literal)),* $(,)?]) => {
        impl $t {
            pub const ALL: &'static [$t] = &[$(<$t>::$v),*];
            pub fn id(self) -> &'static str {
                match self { $(<$t>::$v => $id),* }
            }
            pub fn name(self) -> &'static str {
                match self { $(<$t>::$v => $name),* }
            }
            pub fn from_id(id: &str) -> $t {
                Self::ALL.iter().copied().find(|v| v.id() == id).unwrap_or(<$t>::None)
            }
        }
    };
}

named!(Hat, [
    None => ("none", "None"),
    Security => ("security", "Security cap"),
    Headset => ("headset", "Headset"),
    Bandana => ("bandana", "Bandana"),
    HardHat => ("hardhat", "Hard hat"),
    Grad => ("grad", "Grad cap"),
    Cap => ("cap", "Ball cap"),
    Beret => ("beret", "Beret"),
    Crown => ("crown", "Crown"),
]);

named!(Held, [
    None => ("none", "None"),
    Shield => ("shield", "Shield"),
    Clipboard => ("clipboard", "Clipboard"),
    Parcel => ("parcel", "Parcel"),
    Coffee => ("coffee", "Coffee"),
]);

named!(Tool, [
    None => ("none", "None"),
    Baton => ("baton", "Baton"),
    Broom => ("broom", "Broom"),
    Wrench => ("wrench", "Wrench"),
    Magnifier => ("magnifier", "Magnifier"),
    Pencil => ("pencil", "Pencil"),
]);

named!(Badge, [
    None => ("none", "None"),
    Star => ("star", "Star badge"),
    NameTag => ("tag", "Name tag"),
    Id => ("id", "ID card"),
    Gear => ("gear", "Gear"),
    Heart => ("heart", "Heart"),
]);

impl Hat {
    /// Covers the crown, so the dorsal fin is hidden under it.
    fn covers_crown(self) -> bool {
        !matches!(self, Hat::None | Hat::Headset)
    }
}

/// The props Fin wears, one per anchor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Outfit {
    pub hat: Hat,
    pub left: Held,
    pub right: Tool,
    pub badge: Badge,
}

impl Outfit {
    pub const PLAIN: Outfit = Outfit {
        hat: Hat::None,
        left: Held::None,
        right: Tool::None,
        badge: Badge::None,
    };
}

/// A ready-made outfit and colour for a common agent job.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Role {
    pub id: &'static str,
    pub name: &'static str,
    pub line: &'static str,
    pub outfit: Outfit,
    pub tint: Tint,
}

const fn kit(hat: Hat, left: Held, right: Tool, badge: Badge) -> Outfit {
    Outfit {
        hat,
        left,
        right,
        badge,
    }
}

/// The role kits from the design.
pub const ROLES: [Role; 8] = [
    Role {
        id: "security",
        name: "Security",
        line: "Guards the gate and checks every request.",
        outfit: kit(Hat::Security, Held::Shield, Tool::Baton, Badge::Star),
        tint: Tint::BLUE,
    },
    Role {
        id: "support",
        name: "Customer support",
        line: "Answers people, keeps notes, stays friendly.",
        outfit: kit(Hat::Headset, Held::Coffee, Tool::None, Badge::NameTag),
        tint: Tint::VIOLET,
    },
    Role {
        id: "ops",
        name: "Cleanup and ops",
        line: "Sweeps stale files and tidies the logs.",
        outfit: kit(Hat::Bandana, Held::None, Tool::Broom, Badge::Gear),
        tint: Tint::AMBER,
    },
    Role {
        id: "eng",
        name: "Engineer",
        line: "Builds and fixes. Hard hat on.",
        outfit: kit(Hat::HardHat, Held::None, Tool::Wrench, Badge::Id),
        tint: Tint::ORANGE,
    },
    Role {
        id: "research",
        name: "Researcher",
        line: "Digs into sources and checks the facts.",
        outfit: kit(Hat::Grad, Held::Clipboard, Tool::Magnifier, Badge::Id),
        tint: Tint::BLUE,
    },
    Role {
        id: "shipper",
        name: "Shipper",
        line: "Packs releases and gets them out the door.",
        outfit: kit(Hat::Cap, Held::Parcel, Tool::None, Badge::NameTag),
        tint: Tint::GREEN,
    },
    Role {
        id: "designer",
        name: "Designer",
        line: "Sketches, mocks and polishes.",
        outfit: kit(Hat::Beret, Held::None, Tool::Pencil, Badge::Heart),
        tint: Tint::RED,
    },
    Role {
        id: "lead",
        name: "Lead",
        line: "Runs the crew and signs off the work.",
        outfit: kit(Hat::Crown, Held::Clipboard, Tool::None, Badge::Star),
        tint: Tint::VIOLET,
    },
];

/// One Fin: its colour, what it's made of and what it wears.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Fin {
    pub tint: Tint,
    pub material: Material,
    pub outfit: Outfit,
}

impl Fin {
    pub fn pose(self, mood: Mood) -> FinPose {
        FinPose {
            fin: self,
            state: State::of(mood),
            shine: false,
        }
    }

    /// The role kit this Fin is dressed as, if its outfit and colour match one.
    pub fn role(self) -> Option<&'static Role> {
        ROLES
            .iter()
            .find(|r| r.outfit == self.outfit && r.tint == self.tint)
    }

    /// A short text form, `hat.left.right.badge`, for saving an outfit.
    pub fn outfit_code(o: Outfit) -> String {
        format!(
            "{}.{}.{}.{}",
            o.hat.id(),
            o.left.id(),
            o.right.id(),
            o.badge.id()
        )
    }

    /// Reads [`Fin::outfit_code`]; anything it doesn't know is left bare.
    pub fn outfit_from_code(code: &str) -> Outfit {
        let mut it = code.split('.');
        Outfit {
            hat: Hat::from_id(it.next().unwrap_or("")),
            left: Held::from_id(it.next().unwrap_or("")),
            right: Tool::from_id(it.next().unwrap_or("")),
            badge: Badge::from_id(it.next().unwrap_or("")),
        }
    }
}

/// Fin in a state, ready to draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FinPose {
    pub fin: Fin,
    pub state: State,
    /// Hovered or worth marking: the ring and glow burn a little brighter.
    pub shine: bool,
}

impl FinPose {
    /// Busy enough to need a quick frame rate.
    pub fn lively(&self) -> bool {
        matches!(
            self.state,
            State::Work | State::Done | State::Listen | State::Error | State::Hello
        ) || self.shine
    }
}

// ---------------------------------------------------------------------------
// Drawing.

fn mix(a: Rgb, b: Rgb, k: f64) -> Rgb {
    let m = |x: u8, y: u8| ((x as f64) * (1.0 - k) + (y as f64) * k).round() as u32;
    Rgb((m(a.r(), b.r()) << 16) | (m(a.g(), b.g()) << 8) | m(a.b(), b.b()))
}

fn svg(d: &str) -> BezPath {
    BezPath::from_svg(d).unwrap_or_default()
}

fn round(w: f64) -> Stroke {
    Stroke::new(w).with_caps(Cap::Round).with_join(Join::Round)
}

fn about(c: (f64, f64), t: Affine) -> Affine {
    Affine::translate(c) * t * Affine::translate((-c.0, -c.1))
}

fn rot(deg: f64) -> Affine {
    Affine::rotate(deg.to_radians())
}

fn bbox_xf(bb: Rect) -> Affine {
    Affine::translate((bb.x0, bb.y0))
        * Affine::scale_non_uniform(bb.width().max(1e-3), bb.height().max(1e-3))
}

/// A keyframe track held flat between keys, as CSS `steps(1)` does.
fn steps(p: f64, ks: &[(f64, f64)]) -> f64 {
    let mut v = ks[0].1;
    for &(o, k) in ks {
        if p >= o {
            v = k;
        }
    }
    v
}

/// The design's rounded hexagon shell.
fn hex() -> BezPath {
    let pts = [
        (60.0, 15.5),
        (94.5, 35.0),
        (94.5, 71.0),
        (60.0, 90.5),
        (25.5, 71.0),
        (25.5, 35.0),
    ];
    let r = 8.0;
    let n = pts.len();
    let mut p = BezPath::new();
    for i in 0..n {
        let c: (f64, f64) = pts[i];
        let a = pts[(i + n - 1) % n];
        let b = pts[(i + 1) % n];
        let u = (a.0 - c.0, a.1 - c.1);
        let v = (b.0 - c.0, b.1 - c.1);
        let lu = u.0.hypot(u.1);
        let lv = v.0.hypot(v.1);
        let s = Point::new(c.0 + u.0 / lu * r, c.1 + u.1 / lu * r);
        let e = Point::new(c.0 + v.0 / lv * r, c.1 + v.1 / lv * r);
        if i == 0 {
            p.move_to(s);
        } else {
            p.line_to(s);
        }
        p.quad_to(Point::new(c.0, c.1), e);
    }
    p.close_path();
    p
}

struct Pen<'a> {
    ctx: &'a mut RenderContext,
    xf: Affine,
    alpha: f64,
}

impl Pen<'_> {
    fn at(&mut self, t: Affine, f: impl FnOnce(&mut Self)) {
        let old = self.xf;
        self.xf = old * t;
        f(self);
        self.xf = old;
    }

    fn fade(&mut self, a: f64, f: impl FnOnce(&mut Self)) {
        if a <= 0.004 {
            return;
        }
        let old = self.alpha;
        self.alpha = old * a.min(1.0);
        f(self);
        self.alpha = old;
    }

    fn color(&self, c: Rgb, a: f64) -> Color {
        let a = (a * self.alpha).clamp(0.0, 1.0);
        Color::from_rgba8(c.r(), c.g(), c.b(), (a * 255.0).round() as u8)
    }

    fn fill(&mut self, shape: &impl Shape, c: Rgb, a: f64) {
        if a * self.alpha <= 0.004 {
            return;
        }
        self.ctx.set_transform(self.xf);
        self.ctx.reset_paint_transform();
        self.ctx.set_paint(self.color(c, a));
        self.ctx.fill_path(&shape.to_path(0.05));
    }

    fn stroke(&mut self, shape: &impl Shape, w: f64, c: Rgb, a: f64) {
        if a * self.alpha <= 0.004 {
            return;
        }
        self.ctx.set_transform(self.xf);
        self.ctx.reset_paint_transform();
        self.ctx.set_stroke(round(w));
        self.ctx.set_paint(self.color(c, a));
        self.ctx.stroke_path(&shape.to_path(0.05));
    }

    /// A soft line, standing in for the design's blurred strokes.
    fn glow(&mut self, shape: &impl Shape, w: f64, blur: f64, c: Rgb, a: f64) {
        for (k, share) in [(2.2, 0.22), (1.5, 0.3), (1.0, 0.5)] {
            self.stroke(shape, w + blur * k, c, a * share);
        }
    }

    fn stops(&self, stops: &[(f32, Rgb, f64)]) -> ColorStops {
        let v: Vec<ColorStop> = stops
            .iter()
            .map(|&(o, c, a)| ColorStop::from((o, self.color(c, a))))
            .collect();
        ColorStops::from(v.as_slice())
    }

    fn linear(&self, stops: &[(f32, Rgb, f64)], p0: (f64, f64), p1: (f64, f64)) -> Gradient {
        Gradient {
            kind: LinearGradientPosition {
                start: Point::new(p0.0, p0.1),
                end: Point::new(p1.0, p1.1),
            }
            .into(),
            stops: self.stops(stops),
            ..Default::default()
        }
    }

    fn radial(&self, stops: &[(f32, Rgb, f64)], c: (f64, f64), r: f64) -> Gradient {
        Gradient {
            kind: RadialGradientPosition {
                start_center: Point::new(c.0, c.1),
                start_radius: 0.0,
                end_center: Point::new(c.0, c.1),
                end_radius: r as f32,
            }
            .into(),
            stops: self.stops(stops),
            ..Default::default()
        }
    }

    /// Fills with a gradient laid out in the shape's bounding box, like SVG's
    /// `objectBoundingBox`, turned by `spin` about the box's middle.
    fn fill_grad(&mut self, shape: &impl Shape, g: Gradient, spin: Affine) {
        if self.alpha <= 0.004 {
            return;
        }
        let bb = shape.bounding_box();
        self.ctx.set_transform(self.xf);
        self.ctx
            .set_paint_transform(bbox_xf(bb) * about((0.5, 0.5), spin));
        self.ctx.set_paint(g);
        self.ctx.fill_path(&shape.to_path(0.05));
        self.ctx.reset_paint_transform();
    }

    fn stroke_grad(&mut self, shape: &impl Shape, w: f64, g: Gradient, spin: Affine) {
        if self.alpha <= 0.004 {
            return;
        }
        let bb = shape.bounding_box();
        self.ctx.set_transform(self.xf);
        self.ctx.set_stroke(round(w));
        self.ctx
            .set_paint_transform(bbox_xf(bb) * about((0.5, 0.5), spin));
        self.ctx.set_paint(g);
        self.ctx.stroke_path(&shape.to_path(0.05));
        self.ctx.reset_paint_transform();
    }

    fn clip(&mut self, path: &BezPath, even_odd: bool, f: impl FnOnce(&mut Self)) {
        self.ctx.set_transform(self.xf);
        if even_odd {
            self.ctx.set_fill_rule(Fill::EvenOdd);
        }
        self.ctx.push_clip_layer(path);
        self.ctx.set_fill_rule(Fill::NonZero);
        f(self);
        self.ctx.pop_layer();
    }
}

fn rainbow(a: f64) -> Vec<(f32, Rgb, f64)> {
    RAINBOW
        .iter()
        .enumerate()
        .map(|(i, c)| (i as f32 / 6.0, *c, a))
        .collect()
}

/// Every moving part of Fin at one moment.
struct Rig {
    /// The whole head: shell, visor, fins and props.
    head: Affine,
    fin_l: f64,
    fin_r: f64,
    dorsal: f64,
    ring: (f64, f64),
    glow: (f64, f64),
    shadow: (f64, f64),
    ripples: [(f64, f64); 2],
    particles: [(f64, f64); 3],
    scan: (f64, f64),
    flush: f64,
    dots: [f64; 3],
    sparks: [(f64, f64, f64); 4],
    zs: [(f64, (f64, f64), f64); 3],
    blink: f64,
    look: (f64, f64),
    cursor: f64,
}

fn rig(state: State, t: f64) -> Rig {
    let hc = (60.0, 62.0);
    let mut r = Rig {
        head: Affine::IDENTITY,
        fin_l: 0.0,
        fin_r: 0.0,
        dorsal: 0.0,
        ring: (1.0, 1.0),
        glow: (1.0, 0.55),
        shadow: (1.0, 1.0),
        ripples: [(1.0, 0.0); 2],
        particles: [(0.0, 0.0); 3],
        scan: (0.0, 0.0),
        flush: 0.0,
        dots: [0.0; 3],
        sparks: [(0.0, 0.0, 0.0); 4],
        zs: [(0.0, (0.0, 0.0), 1.0); 3],
        blink: 1.0,
        look: (0.0, 0.0),
        cursor: 1.0,
    };
    let swing = |dur: f64, to: f64| {
        keys(
            phase(t, dur, 0.0),
            &[(0.0, 0.0), (0.5, to), (1.0, 0.0)],
            EIO,
        )
    };
    let ripple = |dur: f64, delay: f64| {
        let p = phase(t, dur, delay);
        let k = EOUT.at(p);
        (1.0 + 0.9 * k, 0.8 * (1.0 - k))
    };
    let blink = || {
        keys(
            phase(t, 4.0, 0.0),
            &[(0.0, 1.0), (0.93, 1.0), (0.96, 0.08), (1.0, 1.0)],
            EASE,
        )
    };
    match state {
        State::Idle => {
            r.head = Affine::translate((0.0, swing(3.2, -4.0)));
            r.fin_l = swing(3.2, 14.0);
            r.fin_r = -swing(3.2, 14.0);
            r.dorsal = swing(3.2, -8.0);
            let k = swing(3.2, 1.0);
            r.ring = (1.0 - 0.1 * k, 1.0 - 0.4 * k);
            r.glow = (1.0 - 0.1 * k, 0.55 + 0.05 * k);
            r.shadow = (1.0 - 0.15 * k, 1.0 - 0.42 * k);
            r.blink = blink();
        }
        State::Listen => {
            let p = phase(t, 2.4, 0.0);
            let a = keys(p, &[(0.0, 6.0), (0.5, 8.0), (1.0, 6.0)], EIO);
            let y = keys(p, &[(0.0, -2.0), (0.5, -4.0), (1.0, -2.0)], EIO);
            r.head = Affine::translate((0.0, y)) * about(hc, rot(a));
            let q = keys(
                phase(t, 0.5, 0.0),
                &[(0.0, 24.0), (0.5, 30.0), (1.0, 24.0)],
                EIO,
            );
            r.fin_l = q;
            r.fin_r = -q;
            r.dorsal = -14.0;
            r.ripples = [ripple(1.8, 0.0), ripple(1.8, -0.9)];
            r.blink = blink();
        }
        State::Think => {
            let p = phase(t, 3.0, 0.0);
            let a = keys(
                p,
                &[(0.0, -2.0), (0.4, -9.0), (0.6, -9.0), (1.0, -2.0)],
                EIO,
            );
            let y = keys(p, &[(0.0, 0.0), (0.4, -3.0), (0.6, -3.0), (1.0, 0.0)], EIO);
            r.head = Affine::translate((0.0, y)) * about(hc, rot(a));
            r.fin_l = 38.0 + 10.0 * alternate(t, 0.6, EIO);
            r.fin_r = 6.0;
            let lp = keys(p, &[(0.0, 0.0), (0.5, 1.0), (1.0, 0.0)], EIO);
            r.look = (-2.0 + 4.0 * lp, -2.0 - 0.5 * lp);
            for (i, d) in r.dots.iter_mut().enumerate() {
                let p = phase(t, 1.5, 0.25 * i as f64);
                *d = steps(p, &[(0.0, 0.0), (0.2, 1.0), (0.8, 1.0)]);
            }
            r.glow.1 = 0.5;
        }
        State::Work => {
            let p = phase(t, 0.26, 0.0);
            let k = (p * 4.0).floor() / 2.0;
            let k = if k > 1.0 { 2.0 - k } else { k };
            r.head = Affine::translate((0.8 * k, -0.6 * k));
            r.fin_l = swing(0.2, 14.0);
            r.fin_r = -swing(0.2, 14.0);
            r.dorsal = swing(0.2, -8.0);
            let k = swing(0.5, 1.0);
            r.ring = (1.0 + 0.14 * k, 1.0 - 0.45 * k);
            r.glow = (1.0 + 0.14 * k, 0.55);
            r.ripples = [ripple(1.0, 0.0), ripple(1.0, -0.5)];
            for (i, pt) in r.particles.iter_mut().enumerate() {
                let p = phase(t, 1.1, -0.37 * i as f64);
                let a = keys(p, &[(0.0, 0.0), (0.2, 1.0), (1.0, 0.0)], LINEAR);
                *pt = (-22.0 * p, a);
            }
            let p = phase(t, 1.3, 0.0);
            let a = keys(
                p,
                &[(0.0, 0.0), (0.15, 0.35), (0.85, 0.35), (1.0, 0.0)],
                EIO,
            );
            r.scan = (26.0 * EIO.at(p), a);
            r.cursor = steps(phase(t, 1.0, 0.0), &[(0.0, 1.0), (0.5, 0.15)]);
        }
        State::Done => {
            let p = phase(t, 1.5, 0.0);
            let k = |v: [f64; 5]| {
                keys(
                    p,
                    &[
                        (0.0, v[0]),
                        (0.12, v[1]),
                        (0.34, v[2]),
                        (0.56, v[3]),
                        (0.72, v[4]),
                        (1.0, v[4]),
                    ],
                    HOP,
                )
            };
            let y = k([0.0, 4.0, -17.0, 2.0, 0.0]);
            let sx = k([1.0, 1.08, 0.95, 1.06, 1.0]);
            let sy = k([1.0, 0.9, 1.06, 0.94, 1.0]);
            r.head = Affine::translate((0.0, y)) * about(hc, Affine::scale_non_uniform(sx, sy));
            r.fin_l = swing(0.38, 14.0);
            r.fin_r = -swing(0.38, 14.0);
            let s = keys(p, &[(0.0, 0.0), (0.34, 1.0), (1.0, 0.0)], HOP);
            r.shadow = (1.0 - 0.45 * s, 1.0 - 0.58 * s);
            for (i, sp) in r.sparks.iter_mut().enumerate() {
                let p = phase(t, 1.5, 0.12 * i as f64);
                let a = keys(
                    p,
                    &[(0.0, 0.0), (0.2, 0.0), (0.4, 1.0), (0.7, 0.0), (1.0, 0.0)],
                    EOUT,
                );
                let sc = keys(
                    p,
                    &[(0.0, 0.0), (0.2, 0.0), (0.4, 1.15), (0.7, 0.4), (1.0, 0.4)],
                    EOUT,
                );
                let ro = keys(
                    p,
                    &[
                        (0.0, 0.0),
                        (0.2, 0.0),
                        (0.4, 45.0),
                        (0.7, 90.0),
                        (1.0, 90.0),
                    ],
                    EOUT,
                );
                *sp = (a, sc, ro);
            }
        }
        State::Error => {
            let p = phase(t, 1.6, 0.0);
            let x = keys(
                p,
                &[
                    (0.0, 0.0),
                    (0.05, -3.5),
                    (0.1, 3.5),
                    (0.15, -3.5),
                    (0.2, 3.5),
                    (0.25, -3.5),
                    (0.3, 3.5),
                    (0.4, 0.0),
                    (1.0, 0.0),
                ],
                EIO,
            );
            r.head = Affine::translate((x, 0.0)) * about(hc, rot(x * 3.0 / 3.5));
            r.fin_l = -22.0;
            r.fin_r = 22.0;
            r.dorsal = 14.0;
            r.flush = steps(p, &[(0.0, 0.35), (0.2, 0.15), (0.4, 0.35), (0.5, 0.22)]);
            r.ring.1 = steps(p, &[(0.0, 1.0), (0.1, 0.2), (0.3, 1.0)]);
        }
        State::Sleep => {
            let p = phase(t, 4.0, 0.0);
            let k = keys(p, &[(0.0, 0.0), (0.5, 1.0), (1.0, 0.0)], EIO);
            r.head = Affine::translate((0.0, 9.0 + k))
                * about(
                    hc,
                    rot(-4.0) * Affine::scale_non_uniform(1.0 + 0.02 * k, 0.98 + 0.02 * k),
                );
            r.fin_l = -26.0;
            r.fin_r = 26.0;
            r.dorsal = 18.0;
            r.ring.1 = 0.18;
            r.glow.1 = 0.18;
            r.shadow = (1.15, 1.33);
            for (i, z) in r.zs.iter_mut().enumerate() {
                let p = phase(t, 3.0, i as f64);
                let a = keys(p, &[(0.0, 0.0), (0.2, 1.0), (1.0, 0.0)], EOUT);
                let m = EOUT.at(p);
                *z = (a, (10.0 * m, -18.0 * m), 0.6 + 0.6 * m);
            }
        }
        State::Hello => {
            let p = phase(t, 2.0, 0.0);
            let a = keys(p, &[(0.0, -4.0), (0.5, -6.0), (1.0, -4.0)], EIO);
            let y = keys(p, &[(0.0, -2.0), (0.5, -4.0), (1.0, -2.0)], EIO);
            r.head = Affine::translate((0.0, y)) * about(hc, rot(a));
            r.fin_r = -18.0 - 40.0 * alternate(t, 0.5, EIO);
            r.fin_l = 8.0;
            r.dorsal = swing(1.0, -8.0);
        }
    }
    r
}

const FIN_L: &str = "M44 46Q16 30 5 44Q4 58 44 63Z";
const FIN_R: &str = "M76 46Q104 30 115 44Q116 58 76 63Z";
const FIN_L_EDGE: &str = "M44 46Q16 30 5 44Q4 58 44 63";
const FIN_R_EDGE: &str = "M76 46Q104 30 115 44Q116 58 76 63";
const RIBS_L: &str = "M29 46L13 42M29 53L12 52M29 59L26 60";
const RIBS_R: &str = "M91 46L107 42M91 53L108 52M91 59L94 60";
const DORSAL: &str = "M53 21Q57 4 72 9Q64 13 66 22Z";
const DORSAL_EDGE: &str = "M53 21Q57 4 72 9Q64 13 66 22";

fn visor() -> RoundedRect {
    RoundedRect::new(36.0, 35.0, 84.0, 70.0, 13.0)
}

/// The face: brows, eyes and mouth for a state.
fn face(p: &mut Pen, state: State, ac: Rgb, rg: &Rig) {
    const L: f64 = 52.0;
    const R: f64 = 68.0;
    const Y: f64 = 53.0;
    let w = WHITE;
    let pill = |p: &mut Pen, x: f64, h: f64, y: f64| {
        p.fill(
            &RoundedRect::new(x - 2.7, y - h / 2.0, x + 2.7, y + h / 2.0, 2.7),
            w,
            1.0,
        )
    };
    let arc = |p: &mut Pen, x: f64, up: bool| {
        let d = if up {
            format!("M{} {}q4 -6 8 0", x - 4.0, Y + 1.5)
        } else {
            format!("M{} {}q4 6 8 0", x - 4.0, Y - 1.5)
        };
        p.stroke(&svg(&d), 2.8, w, 1.0);
    };
    let brows = |p: &mut Pen, a: f64, b: f64, c: f64, d: f64| {
        let mut path = BezPath::new();
        path.move_to((L - 5.0, a));
        path.line_to((L + 4.0, b));
        path.move_to((R - 4.0, c));
        path.line_to((R + 5.0, d));
        p.stroke(&path, 2.4, w, 1.0);
    };
    let blink = |p: &mut Pen, f: &dyn Fn(&mut Pen)| {
        p.at(
            about((60.0, Y), Affine::scale_non_uniform(1.0, rg.blink)),
            |p| f(p),
        );
    };
    match state {
        State::Idle => {
            brows(p, 43.0, 43.0, 43.0, 43.0);
            blink(p, &|p| {
                pill(p, L, 10.0, Y);
                pill(p, R, 10.0, Y);
            });
            p.fill(&RoundedRect::new(57.0, 62.0, 63.0, 64.6, 1.3), ac, 1.0);
        }
        State::Listen => {
            brows(p, 40.0, 39.0, 39.0, 40.0);
            blink(p, &|p| {
                for x in [L, R] {
                    p.fill(&Circle::new((x, Y), 4.0), WHITE, 1.0);
                    p.fill(&Circle::new((x + 1.3, Y - 1.3), 1.3), INK, 1.0);
                }
            });
            p.stroke(&Circle::new((60.0, 63.5), 2.2), 1.8, ac, 1.0);
        }
        State::Think => {
            brows(p, 40.0, 41.5, 44.0, 43.0);
            p.at(Affine::translate(rg.look), |p| {
                pill(p, L, 10.0, Y);
                pill(p, R, 10.0, Y);
            });
            p.stroke(&svg("M59 63.5l5.5-1.5"), 2.2, ac, 1.0);
        }
        State::Work => {
            brows(p, 42.0, 44.5, 44.5, 42.0);
            pill(p, L, 6.0, 54.0);
            pill(p, R, 6.0, 54.0);
            p.fill(
                &RoundedRect::new(56.5, 62.0, 63.5, 64.6, 1.3),
                ac,
                rg.cursor,
            );
        }
        State::Done | State::Hello => {
            if state == State::Done {
                brows(p, 41.0, 40.0, 40.0, 41.0);
            } else {
                brows(p, 41.0, 40.0, 39.0, 40.0);
            }
            arc(p, L, true);
            arc(p, R, true);
            p.fill(&svg("M55.5 61.5q4.5 5.5 9 0z"), ac, 1.0);
        }
        State::Error => {
            brows(p, 44.5, 42.0, 42.0, 44.5);
            for x in [L, R] {
                let d = format!("M{} {}l6 6M{} {}l-6 6", x - 3.0, Y - 3.0, x + 3.0, Y - 3.0);
                p.stroke(&svg(&d), 2.6, w, 1.0);
            }
            p.stroke(&svg("M55 63.5l2.5-2 2.5 2 2.5-2 2.5 2"), 1.9, ac, 1.0);
        }
        State::Sleep => {
            arc(p, L, false);
            arc(p, R, false);
            p.stroke(&Circle::new((60.0, 63.5), 2.2), 1.8, ac, 1.0);
        }
    }
}

fn sparkle(x: f64, y: f64, s: f64) -> BezPath {
    svg(&format!(
        "M{x} {}Q{x} {y} {} {y}Q{x} {y} {x} {}Q{x} {y} {} {y}Q{x} {y} {x} {}Z",
        y - s,
        x + s,
        y + s,
        x - s,
        y - s
    ))
}

/// The ring, its glow and everything that floats around Fin.
fn under(p: &mut Pen, fin: &Fin, rg: &Rig, full: bool, simple: bool, shine: bool) {
    let [l, d, _] = fin.tint.0;
    let rc = (60.0, 106.0);
    if full {
        let (s, a) = rg.shadow;
        let base = if fin.material == Material::Solid {
            0.12
        } else {
            0.16
        };
        let c = if fin.material == Material::Solid {
            BLACK
        } else {
            d
        };
        p.at(about((60.0, 115.0), Affine::scale(s)), |p| {
            p.fill(&Ellipse::new((60.0, 115.0), (20.0, 3.0), 0.0), c, base * a);
        });
    }
    if !simple {
        let (s, a) = rg.glow;
        let a = if shine { (a * 1.4).min(1.0) } else { a };
        p.at(about(rc, Affine::scale(s)), |p| {
            p.fade(a, |p| {
                let g = p.radial(&[(0.0, l, 0.9), (1.0, l, 0.0)], (0.5, 0.5), 0.5);
                p.fill_grad(&Ellipse::new(rc, (30.0, 9.0), 0.0), g, Affine::IDENTITY);
            });
        });
    }
    let ring = Ellipse::new(rc, (21.0, 5.2), 0.0);
    for (s, a) in rg.ripples {
        p.at(about(rc, Affine::scale(s)), |p| p.stroke(&ring, 1.6, l, a));
    }
    let (s, a) = rg.ring;
    p.at(about(rc, Affine::scale(s)), |p| {
        if fin.material == Material::Bubble {
            p.fade(a, |p| {
                let g = p.linear(&rainbow(1.0), (0.0, 0.0), (1.0, 1.0));
                p.stroke_grad(&ring, 3.2, g, Affine::IDENTITY);
            });
        } else {
            p.stroke(&ring, 3.2, l, a);
        }
    });
    if !simple {
        let o = if fin.material == Material::Solid {
            0.6
        } else {
            0.7
        };
        p.stroke(
            &Ellipse::new((60.0, 104.6), (15.0, 2.2), 0.0),
            1.0,
            WHITE,
            o,
        );
    }
    for (i, (y, a)) in rg.particles.iter().enumerate() {
        let (cx, cy, r) = [(52.0, 100.0, 1.5), (60.0, 98.0, 1.8), (68.0, 100.0, 1.5)][i];
        p.fill(&Circle::new((cx, cy + y), r), l, *a);
    }
}

fn over(p: &mut Pen, fin: &Fin, rg: &Rig) {
    let [l, d, _] = fin.tint.0;
    for (i, a) in rg.dots.iter().enumerate() {
        let (x, y, r) = [(91.0, 22.0, 2.2), (98.0, 15.0, 3.0), (107.0, 7.0, 3.8)][i];
        p.fill(&Circle::new((x, y), r), d, *a);
    }
    for (i, (a, s, ro)) in rg.sparks.iter().enumerate() {
        let (x, y, size, c) = [
            (22.0, 24.0, 6.0, Rgb(0xFFD25A)),
            (100.0, 20.0, 5.0, l),
            (16.0, 72.0, 4.0, Rgb(0xFFD25A)),
            (106.0, 70.0, 5.5, l),
        ][i];
        p.at(about((x, y), rot(*ro) * Affine::scale(s.max(0.001))), |p| {
            p.fill(&sparkle(x, y, size), c, *a);
        });
    }
    for (i, (a, (dx, dy), s)) in rg.zs.iter().enumerate() {
        let (x, y) = (84.0 + 2.0 * i as f64, 30.0 - 2.0 * i as f64);
        let z = svg(&format!("M{x} {y}h6l-6 7h6"));
        p.at(
            Affine::translate((*dx, *dy)) * about((x + 3.0, y + 3.5), Affine::scale(*s)),
            |p| {
                p.stroke(&z, 2.0, d, *a);
            },
        );
    }
}

fn solid(p: &mut Pen, fin: &Fin, state: State, rg: &Rig, simple: bool) {
    let [l, d, _] = fin.tint.0;
    let dk = mix(d, BLACK, 0.35);
    let crown = fin.outfit.hat.covers_crown();
    let fin_fill = |p: &Pen| {
        p.linear(
            &[(0.0, d, 1.0), (1.0, mix(d, BLACK, 0.3), 1.0)],
            (0.0, 0.0),
            (0.0, 1.0),
        )
    };
    if !crown {
        p.at(about((58.7, 22.0), rot(rg.dorsal)), |p| {
            let g = fin_fill(p);
            p.fill_grad(&svg(DORSAL), g, Affine::IDENTITY);
        });
        if !simple {
            p.stroke(&svg("M57 17Q61 10 67 10"), 1.2, WHITE, 0.35);
        }
    }
    p.at(about((44.0, 50.0), rot(rg.fin_l)), |p| {
        let g = fin_fill(p);
        p.fill_grad(&svg(FIN_L), g, Affine::IDENTITY);
        if !simple {
            p.stroke(&svg(RIBS_L), 1.4, dk, 0.55);
            p.stroke(&svg("M8 44Q14 37 24 40"), 1.3, WHITE, 0.35);
        }
    });
    p.at(about((76.0, 50.0), rot(rg.fin_r)), |p| {
        let g = fin_fill(p);
        p.fill_grad(&svg(FIN_R), g, Affine::IDENTITY);
        if !simple {
            p.stroke(&svg(RIBS_R), 1.4, dk, 0.55);
            p.stroke(&svg("M96 40Q106 37 112 44"), 1.3, WHITE, 0.35);
        }
    });
    let shell = svg("M60 20L90 37V69L60 86L30 69V37Z");
    let shell_g = |p: &Pen| {
        p.linear(
            &[
                (0.0, l, 1.0),
                (0.55, d, 1.0),
                (1.0, mix(d, BLACK, 0.22), 1.0),
            ],
            (0.0, 0.0),
            (0.45, 1.0),
        )
    };
    let g = shell_g(p);
    p.fill_grad(&shell, g, Affine::IDENTITY);
    let g = shell_g(p);
    p.stroke_grad(&shell, 9.0, g, Affine::IDENTITY);
    if !simple {
        p.stroke(&svg("M33 36L59 21.5M61 21.5L87 36"), 2.6, WHITE, 0.42);
        p.stroke(&svg("M31 70L60 86.5L89 70"), 3.0, dk, 0.35);
        p.fill(&Circle::new((35.0, 66.0), 1.5), dk, 0.5);
        p.fill(&Circle::new((85.0, 66.0), 1.5), dk, 0.5);
    }
    p.fill(&RoundedRect::new(35.5, 34.5, 84.5, 71.5, 14.0), dk, 0.35);
    let g = p.linear(
        &[(0.0, Rgb(0x2A2B33), 1.0), (1.0, Rgb(0x0A0A0C), 1.0)],
        (0.0, 0.0),
        (0.0, 1.0),
    );
    p.fill_grad(&visor(), g, Affine::IDENTITY);
    p.fill(&visor(), Rgb(0xFF2D46), rg.flush);
    p.fill(
        &RoundedRect::new(38.0, 38.0 + rg.scan.0, 82.0, 40.4 + rg.scan.0, 1.2),
        l,
        rg.scan.1,
    );
    if !simple {
        p.stroke(&svg("M42 39.5H60"), 2.2, WHITE, 0.16);
        p.stroke(&svg("M80 44V58"), 1.6, WHITE, 0.1);
    }
    face(p, state, l, rg);
    p.stroke(&svg("M54 77.5h12"), 1.6, dk, 0.5);
}

fn glass(p: &mut Pen, fin: &Fin, state: State, rg: &Rig, simple: bool, t: f64) {
    let [l, d, _] = fin.tint.0;
    let lt = mix(l, WHITE, 0.45);
    let bub = fin.material == Material::Bubble;
    // The rainbow turns once every 9 s; the rim the other way every 6 s.
    let spin = rot(360.0 * phase(t, 9.0, 0.0));
    let spin_rim = rot(-360.0 * phase(t, 6.0, 0.0));
    let hx = hex();
    let crown = fin.outfit.hat.covers_crown();
    let fin_g = |p: &Pen| {
        if bub {
            p.linear(&rainbow(0.5), (0.0, 0.0), (1.0, 1.0))
        } else {
            p.linear(&[(0.0, l, 0.55), (1.0, d, 0.55)], (0.0, 0.0), (0.2, 1.0))
        }
    };
    let fin_spin = if bub { spin } else { Affine::IDENTITY };
    let rim = |p: &mut Pen, s: &BezPath| {
        let w = if bub {
            if simple {
                2.0
            } else {
                1.5
            }
        } else if simple {
            1.6
        } else {
            1.2
        };
        if bub {
            let g = p.linear(&rainbow(1.0), (0.0, 0.0), (1.0, 1.0));
            p.stroke_grad(s, w, g, spin_rim);
        } else {
            let g = p.linear(
                &[
                    (0.0, WHITE, 0.95),
                    (0.4, WHITE, 0.2),
                    (0.75, WHITE, 0.08),
                    (1.0, WHITE, 0.7),
                ],
                (0.0, 0.0),
                (0.35, 1.0),
            );
            p.stroke_grad(s, w, g, Affine::IDENTITY);
        }
    };
    if bub && !simple {
        // The rainbow halo round the head.
        for (w, a) in [(20.0, 0.1), (14.0, 0.16), (9.0, 0.22)] {
            let g = p.linear(&rainbow(a), (0.0, 0.0), (1.0, 1.0));
            p.stroke_grad(&hx, w, g, spin_rim);
        }
    }
    // Fins and the dorsal sit behind the shell: only their parts outside it show.
    let mut outside = Rect::new(-40.0, -40.0, 160.0, 160.0).to_path(0.1);
    outside.extend(hx.iter());
    p.clip(&outside, true, |p| {
        if !crown {
            p.at(about((58.7, 22.0), rot(rg.dorsal)), |p| {
                let g = fin_g(p);
                p.fill_grad(&svg(DORSAL), g, fin_spin);
                rim(p, &svg(DORSAL_EDGE));
            });
        }
        for (c, a, body, ribs, edge) in [
            ((44.0, 50.0), rg.fin_l, FIN_L, RIBS_L, FIN_L_EDGE),
            ((76.0, 50.0), rg.fin_r, FIN_R, RIBS_R, FIN_R_EDGE),
        ] {
            p.at(about(c, rot(a)), |p| {
                let g = fin_g(p);
                p.fill_grad(&svg(body), g, fin_spin);
                if !simple {
                    p.stroke(&svg(ribs), 1.1, WHITE, 0.45);
                }
                rim(p, &svg(edge));
            });
        }
    });
    if simple {
        p.fill(&hx, d, 0.45);
    }
    let tint_g = |p: &Pen| {
        p.linear(
            &[
                (0.0, l, 0.5),
                (0.5, d, 0.26),
                (1.0, mix(d, BLACK, 0.15), 0.62),
            ],
            (0.0, 0.0),
            (0.5, 1.0),
        )
    };
    if bub {
        p.fade(0.3, |p| {
            let g = tint_g(p);
            p.fill_grad(&hx, g, Affine::IDENTITY);
        });
        let g = p.linear(&rainbow(0.42), (0.0, 0.0), (1.0, 1.0));
        p.fill_grad(&hx, g, spin);
        let g = p.radial(&[(0.55, WHITE, 0.0), (1.0, WHITE, 0.35)], (0.5, 0.45), 0.6);
        p.fill_grad(&hx, g, Affine::IDENTITY);
    } else {
        let g = tint_g(p);
        p.fill_grad(&hx, g, Affine::IDENTITY);
    }
    let g = p.radial(
        &[(0.0, mix(l, WHITE, 0.3), 0.75), (1.0, l, 0.0)],
        (0.5, 0.92),
        0.6,
    );
    p.fill_grad(&hx, g, Affine::IDENTITY);
    if !simple {
        let hx2 = hx.clone();
        p.clip(&hx2, false, |p| {
            if bub {
                p.fade(0.55, |p| {
                    for (w, a) in [(12.0, 0.25), (8.0, 0.35), (4.0, 0.5)] {
                        let g = p.linear(&rainbow(a), (0.0, 0.0), (1.0, 1.0));
                        p.stroke_grad(&hx, w, g, spin_rim);
                    }
                });
            } else {
                p.glow(&hx, 4.0, 3.0, d, 0.55);
            }
            p.glow(&hx, 1.6, 1.4, WHITE, 0.55);
            p.at(about((46.0, 30.0), rot(-29.0)), |p| {
                p.fill(&Ellipse::new((46.0, 30.0), (24.0, 10.5), 0.0), WHITE, 0.1);
                p.fill(&Ellipse::new((46.0, 30.0), (21.0, 8.0), 0.0), WHITE, 0.16);
            });
            p.glow(&svg("M31 70L60 86.5L89 70"), 1.8, 1.4, lt, 0.9);
        });
        p.stroke(&svg("M32 37L57 22.6"), 2.4, WHITE, 0.9);
        p.stroke(&svg("M63 22.6L86 35.6"), 1.4, WHITE, 0.5);
        p.fill(&Circle::new((29.5, 40.5), 1.3), WHITE, 0.9);
    }
    rim(p, &hx);
    if !simple {
        p.fill(&RoundedRect::new(35.0, 36.5, 85.0, 73.5, 14.0), BLACK, 0.1);
        p.fill(&RoundedRect::new(36.0, 37.0, 84.0, 72.0, 13.0), BLACK, 0.14);
    }
    let g = p.linear(
        &[(0.0, Rgb(0x1B1C24), 0.9), (1.0, Rgb(0x050507), 0.94)],
        (0.0, 0.0),
        (0.0, 1.0),
    );
    p.fill_grad(&visor(), g, Affine::IDENTITY);
    p.fill(&visor(), Rgb(0xFF2D46), rg.flush);
    p.fill(
        &RoundedRect::new(38.0, 38.0 + rg.scan.0, 82.0, 40.4 + rg.scan.0, 1.2),
        l,
        rg.scan.1,
    );
    if !simple {
        p.fill(&svg("M41 69L57 36H66L50 69Z"), WHITE, 0.05);
        p.stroke(&svg("M42 39.5H60"), 2.0, WHITE, 0.22);
    }
    p.stroke(&visor(), 0.8, WHITE, 0.28);
    if !simple {
        // The visor's glow round the face.
        p.fade(0.25, |p| face_glow(p, state, lt, rg));
    }
    face(p, state, lt, rg);
    p.stroke(&svg("M54 78h12"), 1.4, WHITE, 0.55);
}

fn face_glow(p: &mut Pen, state: State, ac: Rgb, rg: &Rig) {
    // A wider, faint copy of the face reads as the light the visor gives off.
    p.at(about((60.0, 56.0), Affine::scale(1.04)), |p| {
        face(p, state, ac, rg)
    });
}

/// The props, each drawn on the design's 120 grid.
fn wear(p: &mut Pen, o: Outfit, rg: &Rig) {
    let f = |p: &mut Pen, d: &str, c: u32, a: f64| p.fill(&svg(d), Rgb(c), a);
    let s = |p: &mut Pen, d: &str, w: f64, c: u32, a: f64| p.stroke(&svg(d), w, Rgb(c), a);
    p.at(about((44.0, 50.0), rot(rg.fin_l)), |p| match o.left {
        Held::None => {}
        Held::Shield => {
            f(p, "M-3 39H23V54Q23 69 10 75Q-3 69 -3 54Z", 0xD5D9E1, 1.0);
            s(
                p,
                "M-3 39H23V54Q23 69 10 75Q-3 69 -3 54Z",
                1.6,
                0x8E939E,
                1.0,
            );
            f(p, "M1 43H19V54Q19 65 10 70Q1 65 1 54Z", 0x26324F, 1.0);
            f(
                p,
                "M10 47l1.9 3.8 4.2.6-3 3 .7 4.1L10 56.6l-3.8 2 .7-4.2-3-3 4.2-.6z",
                0xFFC94A,
                1.0,
            );
            s(p, "M0 42H8", 1.4, 0xFFFFFF, 0.6);
        }
        Held::Clipboard => {
            p.fill(
                &RoundedRect::new(-3.0, 40.0, 18.0, 67.0, 2.5),
                Rgb(0xC58B4E),
                1.0,
            );
            p.fill(&RoundedRect::new(0.0, 44.0, 15.0, 64.0, 1.0), WHITE, 1.0);
            p.fill(
                &RoundedRect::new(3.0, 37.5, 12.0, 42.5, 1.5),
                Rgb(0x8E939E),
                1.0,
            );
            s(p, "M3 49h9M3 53h9M3 57h6", 1.2, 0x9A9AA0, 1.0);
            s(p, "M10 60l1.6 1.6 3-3.4", 1.4, 0x1FC27A, 1.0);
        }
        Held::Parcel => {
            p.fill(
                &RoundedRect::new(-5.0, 41.0, 20.0, 62.0, 2.0),
                Rgb(0xD9A066),
                1.0,
            );
            f(p, "M-5 41L-2 37H23L20 41Z", 0xE8B57D, 1.0);
            s(p, "M7.5 37V62", 2.4, 0xB57E45, 1.0);
            p.fill(&RoundedRect::new(-2.0, 52.0, 5.0, 58.0, 1.0), WHITE, 0.85);
        }
        Held::Coffee => {
            let cup = RoundedRect::new(0.0, 44.0, 15.0, 62.0, 3.0);
            p.fill(&cup, WHITE, 1.0);
            p.stroke(&cup, 1.0, Rgb(0xD0D0D6), 1.0);
            s(p, "M0 48h-3a3.5 3.5 0 0 0 0 9h3", 2.0, 0xD0D0D6, 1.0);
            s(p, "M4.5 40q2.5-3 0-6M10.5 40q2.5-3 0-6", 1.4, 0xB9B9C0, 1.0);
            p.fill(
                &RoundedRect::new(3.0, 50.0, 12.0, 55.0, 1.2),
                Rgb(0x8B5CF6),
                0.75,
            );
        }
    });
    p.at(about((76.0, 50.0), rot(rg.fin_r)), |p| match o.right {
        Tool::None => {}
        Tool::Baton => {
            s(p, "M103 66L117 29", 4.6, 0x1F2026, 1.0);
            s(p, "M104.5 62L108 52.5", 5.8, 0x55575E, 1.0);
            s(p, "M112 40L115 32", 1.2, 0xFFFFFF, 0.35);
        }
        Tool::Broom => {
            s(p, "M98 22L114 70", 2.6, 0xA26B3A, 1.0);
            f(
                p,
                "M109.3 71.6L118.8 68.4L128.4 84.2Q121 90 111.3 90Z",
                0xE8C060,
                1.0,
            );
            s(
                p,
                "M114 71l4 17M118 70l6 15M111 73l1 15",
                1.0,
                0xC99A3A,
                1.0,
            );
            s(p, "M109.6 73.4L119.4 70.2", 2.4, 0xD2493E, 1.0);
        }
        Tool::Wrench => {
            s(p, "M104 66L115 38", 5.0, 0x9AA0AA, 1.0);
            p.fill(&Circle::new((117.0, 33.0), 7.0), Rgb(0xB9BEC7), 1.0);
            f(p, "M116 25L121 31L117 33Z", 0x55575E, 1.0);
            p.fill(&Circle::new((117.0, 33.0), 2.4), Rgb(0x55575E), 1.0);
            s(p, "M106 60L111 47", 1.2, 0xFFFFFF, 0.45);
        }
        Tool::Magnifier => {
            s(p, "M103 66L109 51", 4.0, 0x2B2C31, 1.0);
            let lens = Circle::new((113.5, 40.0), 9.5);
            p.fill(&lens, Rgb(0xBFE3FF), 0.5);
            p.stroke(&lens, 3.0, Rgb(0x2B2C31), 1.0);
            s(p, "M108 36q3-4.5 7.5-3.5", 1.8, 0xFFFFFF, 1.0);
        }
        Tool::Pencil => {
            p.ctx.set_stroke(Stroke::new(6.0));
            let body = |p: &mut Pen, d: &str, c: u32| {
                p.ctx.set_transform(p.xf);
                p.ctx.reset_paint_transform();
                p.ctx.set_stroke(Stroke::new(6.0));
                let col = p.color(Rgb(c), 1.0);
                p.ctx.set_paint(col);
                p.ctx.stroke_path(&svg(d));
            };
            body(p, "M102 64L116 32", 0xFFC94A);
            f(p, "M118.7 33.2L113.3 30.8L118.8 25.6Z", 0xF2D3A6, 1.0);
            f(p, "M118.1 27.6L116.7 26.9L118.8 25.6Z", 0x2B2C31, 1.0);
            body(p, "M102 64L103.6 60.3", 0xFF8FA3);
            body(p, "M103.8 59.5L104.6 57.6", 0xB9BEC7);
        }
    });
    match o.badge {
        Badge::None => {}
        Badge::Star => {
            let d = "M60 72.5l2.3 4.6 5 .7-3.6 3.5.9 5L60 84l-4.6 2.3.9-5-3.6-3.5 5-.7z";
            f(p, d, 0xFFC94A, 1.0);
            s(p, d, 0.8, 0xE5A100, 1.0);
        }
        Badge::NameTag => {
            p.fill(&RoundedRect::new(49.0, 73.5, 71.0, 83.5, 2.0), WHITE, 1.0);
            p.fill(
                &RoundedRect::new(49.0, 73.5, 71.0, 76.9, 1.5),
                Rgb(0x1E95F0),
                1.0,
            );
            s(p, "M52.5 80h12", 1.2, 0x9A9AA0, 1.0);
        }
        Badge::Id => {
            p.fill(&RoundedRect::new(52.5, 72.0, 67.5, 84.0, 2.0), WHITE, 1.0);
            p.fill(&Circle::new((56.5, 77.0), 2.1), Rgb(0x9A9AA0), 1.0);
            s(p, "M60.5 75.8h4.5M60.5 78.6h4.5", 1.1, 0x9A9AA0, 1.0);
            s(p, "M58 72v-2.5h4V72", 1.0, 0x55575E, 1.0);
        }
        Badge::Gear => {
            p.ctx.set_transform(p.xf);
            p.ctx.reset_paint_transform();
            p.ctx
                .set_stroke(Stroke::new(3.2).with_dashes(0.0, [2.1, 1.5]));
            let col = p.color(WHITE, 1.0);
            p.ctx.set_paint(col);
            p.ctx
                .stroke_path(&Circle::new((60.0, 79.0), 4.6).to_path(0.05));
            p.fill(&Circle::new((60.0, 79.0), 2.6), WHITE, 1.0);
        }
        Badge::Heart => {
            let d = "M60 85l-5.2-5.2a3.1 3.1 0 0 1 5.2-3.9a3.1 3.1 0 0 1 5.2 3.9z";
            f(p, d, 0xFF6F86, 1.0);
            s(p, d, 0.8, 0xFFFFFF, 1.0);
        }
    }
    match o.hat {
        Hat::None => {}
        Hat::Security => {
            f(p, "M37 24Q38 6 60 5Q82 6 83 24Z", 0x26324F, 1.0);
            s(p, "M44 13Q52 7 62 7", 1.6, 0xFFFFFF, 0.2);
            p.fill(
                &RoundedRect::new(36.0, 20.0, 84.0, 26.0, 2.0),
                Rgb(0x141A2B),
                1.0,
            );
            f(p, "M38 25Q60 33 82 25L86 29Q60 40 34 29Z", 0x0E1220, 1.0);
            f(
                p,
                "M60 9l1.9 3.8 4.2.6-3 3 .7 4.1L60 18.6l-3.8 2 .7-4.2-3-3 4.2-.6z",
                0xFFC94A,
                1.0,
            );
        }
        Hat::Headset => {
            s(p, "M29 44Q28 10 60 9Q92 10 91 44", 3.6, 0x2B2C31, 1.0);
            p.fill(
                &RoundedRect::new(22.0, 37.0, 32.0, 56.0, 5.0),
                Rgb(0x2B2C31),
                1.0,
            );
            p.fill(
                &RoundedRect::new(88.0, 37.0, 98.0, 56.0, 5.0),
                Rgb(0x2B2C31),
                1.0,
            );
            p.fill(
                &RoundedRect::new(24.0, 41.0, 27.0, 52.0, 1.5),
                Rgb(0x6A6C74),
                1.0,
            );
            s(p, "M27 55Q28 72 44 75", 2.4, 0x2B2C31, 1.0);
            p.fill(
                &RoundedRect::new(42.0, 72.0, 50.0, 77.0, 2.5),
                Rgb(0x2B2C31),
                1.0,
            );
        }
        Hat::Bandana => {
            f(p, "M33 31Q33 9 60 8Q87 9 87 31Q60 21 33 31Z", 0x2F6FD0, 1.0);
            for (x, y) in [
                (44.0, 18.0),
                (54.0, 13.0),
                (66.0, 13.0),
                (76.0, 19.0),
                (60.0, 21.0),
                (48.0, 25.0),
                (72.0, 25.0),
            ] {
                p.fill(&Circle::new((x, y), 1.4), WHITE, 0.85);
            }
            f(p, "M85 26L97 21L94 30Z", 0x2459AE, 1.0);
            f(p, "M85 28L95 36L86 35Z", 0x2459AE, 1.0);
        }
        Hat::HardHat => {
            f(p, "M36 26Q36 5 60 5Q84 5 84 26Z", 0xFFC21A, 1.0);
            s(
                p,
                "M60 6V25M49 8Q46 16 46 25M71 8Q74 16 74 25",
                2.2,
                0xE0A000,
                1.0,
            );
            s(p, "M42 14Q49 8 57 7", 2.0, 0xFFFFFF, 0.55);
            p.fill(
                &RoundedRect::new(29.0, 23.0, 91.0, 29.0, 3.0),
                Rgb(0xF2B000),
                1.0,
            );
        }
        Hat::Grad => {
            f(p, "M44 17V26Q60 32 76 26V17L60 23Z", 0x2B2C31, 1.0);
            f(p, "M60 4L91 14L60 24L29 14Z", 0x1F2026, 1.0);
            s(p, "M60 7L83 14", 1.0, 0xFFFFFF, 0.18);
            s(p, "M86 15V31", 1.6, 0xFFC94A, 1.0);
            f(p, "M84 30h4l1 6h-6z", 0xFFC94A, 1.0);
        }
        Hat::Cap => {
            f(p, "M38 25Q38 6 60 6Q82 6 82 25Z", 0x14B8A6, 1.0);
            s(p, "M60 6V24", 1.6, 0x0F9488, 1.0);
            s(p, "M45 13Q51 8 58 8", 1.8, 0xFFFFFF, 0.45);
            p.fill(&Circle::new((60.0, 6.5), 2.0), Rgb(0x0F9488), 1.0);
            p.fill(
                &RoundedRect::new(37.0, 21.5, 83.0, 26.5, 2.0),
                Rgb(0x0F9488),
                1.0,
            );
            f(p, "M68 23Q90 21 100 27Q86 31 68 28Z", 0x0D7F75, 1.0);
        }
        Hat::Beret => {
            f(p, "M34 24Q30 8 56 7Q84 6 82 18Q72 26 34 24Z", 0x2B2C31, 1.0);
            s(p, "M36 23Q58 27 80 18", 3.0, 0x16171B, 1.0);
            p.fill(
                &RoundedRect::new(55.0, 2.5, 58.0, 8.5, 1.5),
                Rgb(0x2B2C31),
                1.0,
            );
            s(p, "M44 12Q52 9 60 9", 1.6, 0xFFFFFF, 0.15);
        }
        Hat::Crown => {
            let d = "M41 23L38 5L50 14L60 1L70 14L82 5L79 23Z";
            f(p, d, 0xFFC94A, 1.0);
            s(p, d, 1.2, 0xE5A100, 1.0);
            p.fill(
                &RoundedRect::new(41.0, 19.0, 79.0, 24.0, 1.5),
                Rgb(0xF2B000),
                1.0,
            );
            p.fill(&Circle::new((60.0, 13.0), 2.2), Rgb(0xF2364F), 1.0);
            p.fill(&Circle::new((48.0, 17.0), 1.5), Rgb(0x1E95F0), 1.0);
            p.fill(&Circle::new((72.0, 17.0), 1.5), Rgb(0x1FC27A), 1.0);
        }
    }
}

/// The part of the 120 grid a framing shows: `(x0, y0, side)`.
fn view(framing: Framing) -> (f64, f64, f64) {
    match framing {
        // Room for props that reach past the fins and for the floating bits.
        Framing::Full => (-10.0, -4.0, 140.0),
        // Avatars: the head and most of the fins, filling the square.
        Framing::Bust => (9.0, 5.0, 102.0),
    }
}

/// Draws Fin into `ctx` at time `t`, scaled so the framing fills `px` pixels.
fn draw_into(ctx: &mut RenderContext, pose: &FinPose, t: f64, framing: Framing, px: u16) {
    let (x0, y0, side) = view(framing);
    let k = px as f64 / side;
    let xf = Affine::scale(k) * Affine::translate((-x0, -y0));
    let simple = px < 36;
    let full = framing == Framing::Full;
    let rg = rig(pose.state, t);
    let mut p = Pen {
        ctx,
        xf,
        alpha: 1.0,
    };
    let fin = pose.fin;
    under(&mut p, &fin, &rg, full, simple, pose.shine);
    p.at(rg.head, |p| {
        match fin.material {
            Material::Solid => solid(p, &fin, pose.state, &rg, simple),
            Material::Glass | Material::Bubble => glass(p, &fin, pose.state, &rg, simple, t),
        }
        wear(p, fin.outfit, &rg);
    });
    over(&mut p, &fin, &rg);
}

/// Draws Fin into premultiplied RGBA8, reusing buffers between frames.
pub struct FinRenderer {
    ctx: RenderContext,
    resources: Resources,
    pixmap: Pixmap,
}

impl FinRenderer {
    pub fn new(px: u16) -> Self {
        FinRenderer {
            ctx: RenderContext::new(px, px),
            resources: Resources::new(),
            pixmap: Pixmap::new(px, px),
        }
    }

    pub fn size(&self) -> u16 {
        self.ctx.width()
    }

    /// Draws `pose` at time `t`; premultiplied RGBA8, `size()` pixels square.
    pub fn draw(&mut self, pose: &FinPose, t: f64, framing: Framing) -> &[u8] {
        let px = self.ctx.width();
        self.ctx.reset();
        draw_into(&mut self.ctx, pose, t, framing, px);
        self.ctx.flush();
        self.pixmap = Pixmap::new(px, px);
        self.ctx.render(&mut self.pixmap, &mut self.resources);
        self.pixmap.data_as_u8_slice()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coverage(px: &[u8]) -> usize {
        px.chunks(4).filter(|p| p[3] > 128).count()
    }

    fn fin(material: Material, outfit: Outfit) -> Fin {
        Fin {
            tint: Tint::RED,
            material,
            outfit,
        }
    }

    #[test]
    fn every_state_and_material_draws_something() {
        let mut r = FinRenderer::new(96);
        for m in Material::ALL {
            for s in State::ALL {
                let pose = FinPose {
                    fin: fin(m, Outfit::PLAIN),
                    state: s,
                    shine: false,
                };
                let px = r.draw(&pose, 1.3, Framing::Full);
                assert!(coverage(px) > 600, "{m:?} {s:?} drew almost nothing");
            }
        }
    }

    #[test]
    fn small_avatars_draw_too() {
        let mut r = FinRenderer::new(22);
        let pose = fin(Material::Glass, Outfit::PLAIN).pose(Mood::Idle);
        assert!(coverage(r.draw(&pose, 0.0, Framing::Bust)) > 60);
    }

    #[test]
    fn working_moves() {
        let pose = fin(Material::Solid, Outfit::PLAIN).pose(Mood::Working);
        let mut r = FinRenderer::new(64);
        let a = r.draw(&pose, 0.0, Framing::Full).to_vec();
        let b = r.draw(&pose, 0.13, Framing::Full).to_vec();
        assert_ne!(a, b);
    }

    #[test]
    fn props_add_to_the_drawing() {
        let mut r = FinRenderer::new(96);
        let bare = coverage(r.draw(
            &fin(Material::Solid, Outfit::PLAIN).pose(Mood::Idle),
            0.0,
            Framing::Full,
        ));
        for role in ROLES {
            let dressed = coverage(r.draw(
                &fin(Material::Solid, role.outfit).pose(Mood::Idle),
                0.0,
                Framing::Full,
            ));
            assert!(dressed > bare, "{} adds nothing", role.name);
        }
    }

    #[test]
    fn outfit_codes_round_trip() {
        for role in ROLES {
            assert_eq!(
                Fin::outfit_from_code(&Fin::outfit_code(role.outfit)),
                role.outfit
            );
        }
        assert_eq!(Fin::outfit_from_code("nonsense"), Outfit::PLAIN);
    }
}
