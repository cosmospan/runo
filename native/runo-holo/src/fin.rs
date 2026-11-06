//! Peek: Squad Code's mascot. Two glowing eyes in a dark pill, and nothing
//! else: every feeling is in the lids, the glow and how the pill moves.
//!
//! A port of the "Peek, alive" design. Like the bubble crew, a frame is a pure
//! function of time, so the same pose plays the same everywhere: blinks and
//! idle glances come from a steady hash of the clock, not from state.
//!
//! The types keep their Fin names ([`Fin`], [`FinPose`], [`Outfit`]) so saved
//! looks and the rest of the app carry over. Peek comes in three finishes
//! (glow, rainbow glow and solid). It can wear a hat on top and a badge on the
//! pill, and the prop it carries shows in a small badge beside it.

use vello_cpu::kurbo::{
    Affine, BezPath, Cap, Circle, Ellipse, Join, Point, Rect, RoundedRect, Shape, Stroke,
};
use vello_cpu::peniko::{
    color::{AlphaColor, Srgb},
    ColorStop, ColorStops, Fill, Gradient, LinearGradientPosition, RadialGradientPosition,
};
use vello_cpu::{Pixmap, RenderContext, Resources};

use crate::bubble::Tint;
use crate::look::{Mood, Rgb};
use crate::Framing;

type Color = AlphaColor<Srgb>;

const WHITE: Rgb = Rgb(0xFFFFFF);
const BLACK: Rgb = Rgb(0x000000);
const INK: Rgb = Rgb(0x141416);

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

/// What Peek is doing: the job states the app drives, and short reactions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum State {
    /// Waiting for a job. Breathes, blinks and glances around.
    Idle,
    /// You're typing. Eyes open wide, the pill leans in and ripples.
    Listen,
    /// Planning. Looks up and away while thought bubbles drift.
    Think,
    /// Reading and writing code. Focused lids, eyes scan line by line.
    Work,
    /// A long deep-work run. Narrow, steady eyes and a slow glow.
    Focused,
    /// The job landed. A happy squint, a bounce and sparkles.
    Done,
    /// Waiting on an answer. Big pleading eyes and an amber pulse.
    Needs,
    /// Something broke. X eyes and a little shake.
    Error,
    /// Failed after retrying. Droopy lids, looking down, a sweat drop.
    Sorry,
    /// Off for the night. Eyes closed, slow breathing, floating Zs.
    Sleep,
    /// A wiggle, a hop and a "hi".
    Hello,
    /// Heart eyes.
    Love,
    /// Smug half-lids, chin up, a twinkle.
    Proud,
    /// Glances away and blushes.
    Shy,
    /// Wide eyes with small pupils.
    Surprised,
    /// One eye winks.
    Wink,
    /// One lid down and a head tilt.
    Curious,
    /// One eye big, one squinty, a question mark.
    Confused,
    /// The Guard blocks a risky command with its shield.
    Blocked,
}

impl State {
    pub const ALL: [State; 19] = [
        State::Idle,
        State::Listen,
        State::Think,
        State::Work,
        State::Focused,
        State::Done,
        State::Needs,
        State::Error,
        State::Sorry,
        State::Sleep,
        State::Hello,
        State::Love,
        State::Proud,
        State::Shy,
        State::Surprised,
        State::Wink,
        State::Curious,
        State::Confused,
        State::Blocked,
    ];

    pub fn name(self) -> &'static str {
        match self {
            State::Idle => "Idle",
            State::Listen => "Listening",
            State::Think => "Thinking",
            State::Work => "Working",
            State::Focused => "Focused",
            State::Done => "Done",
            State::Needs => "Needs you",
            State::Error => "Error",
            State::Sorry => "Sorry",
            State::Sleep => "Asleep",
            State::Hello => "Hello",
            State::Love => "Love",
            State::Proud => "Proud",
            State::Shy => "Shy",
            State::Surprised => "Surprised",
            State::Wink => "Wink",
            State::Curious => "Curious",
            State::Confused => "Confused",
            State::Blocked => "Blocked",
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
            Mood::NeedsYou => State::Needs,
            Mood::Asleep => State::Sleep,
        }
    }
}

/// Peek's finish. The ids keep their old Fin names so saved settings still load.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Material {
    /// A dark pill with glowing eyes and a rim in the crew colour.
    Glass,
    /// The glow finish with a slowly turning rainbow rim.
    Bubble,
    /// A glossy pill in the crew colour with dark eyes.
    Solid,
}

impl Material {
    pub const ALL: [Material; 3] = [Material::Glass, Material::Bubble, Material::Solid];

    pub fn name(self) -> &'static str {
        match self {
            Material::Glass => "Glow",
            Material::Bubble => "Rainbow glow",
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
            gaze: (0, 0),
            offset: 0,
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
    /// Where the eyes look, each axis from -2 (left, up) to 2 (right, down).
    pub gaze: (i8, i8),
    /// Shifts the animation clock, in steps of [`OFFSET_STEP`] seconds, so a
    /// row of bots in the same state doesn't move in lockstep.
    pub offset: u8,
}

/// Seconds per step of [`FinPose::offset`].
pub const OFFSET_STEP: f64 = 0.37;

impl FinPose {
    /// Looking towards `gaze` (see [`FinPose::gaze`]).
    pub fn looking(mut self, gaze: (i8, i8)) -> Self {
        self.gaze = (gaze.0.clamp(-2, 2), gaze.1.clamp(-2, 2));
        self
    }

    /// With its clock shifted by `offset` steps (see [`FinPose::offset`]).
    pub fn offset(mut self, offset: u8) -> Self {
        self.offset = offset;
        self
    }

    /// Busy enough to need a quick frame rate.
    pub fn lively(&self) -> bool {
        !matches!(
            self.state,
            State::Idle | State::Sleep | State::Focused | State::Sorry | State::Shy
        ) || self.shine
    }
}

// ---------------------------------------------------------------------------
// Drawing: Peek, two glowing eyes in a dark pill.

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

/// A badge from the old Fin kit, on its 120 grid.
fn badge(p: &mut Pen, b: Badge) {
    let f = |p: &mut Pen, d: &str, c: u32, a: f64| p.fill(&svg(d), Rgb(c), a);
    let s = |p: &mut Pen, d: &str, w: f64, c: u32, a: f64| p.stroke(&svg(d), w, Rgb(c), a);
    match b {
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
}

/// A hat from the old Fin kit, on its 120 grid.
fn hat(p: &mut Pen, h: Hat) {
    let f = |p: &mut Pen, d: &str, c: u32, a: f64| p.fill(&svg(d), Rgb(c), a);
    let s = |p: &mut Pen, d: &str, w: f64, c: u32, a: f64| p.stroke(&svg(d), w, Rgb(c), a);
    match h {
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

/// How long one round of the Guard's block takes, in seconds.
const BLOCK: f64 = 2.4;
const AMBER: Rgb = Rgb(0xF5B83D);
const PINK: Rgb = Rgb(0xFF6F91);
const GOLD: Rgb = Rgb(0xFFD84D);
const BADGE_INK: Rgb = Rgb(0x141219);
const PILL: (f64, f64, f64, f64, f64) = (14.0, 34.0, 106.0, 90.0, 28.0);
const STAR: &str = "M0 -6l1.6 4.4 4.4 1.6-4.4 1.6-1.6 4.4-1.6-4.4-4.4-1.6 4.4-1.6z";
const HEART: &str = "M0 6C-9 0-7-7 0-3C7-7 9 0 0 6Z";
const SHIELD: &str = "M0 -10L8 -7V-1C8 4 4.5 7.5 0 9.5C-4.5 7.5-8 4-8-1V-7Z";

fn pill() -> RoundedRect {
    let (x0, y0, x1, y1, r) = PILL;
    RoundedRect::new(x0, y0, x1, y1, r)
}

/// The shape of an eye.
#[derive(Clone, Copy, Debug, PartialEq)]
enum EyeShape {
    /// An oval with a glint, shaped by its lids.
    Dot,
    /// A happy upward arc.
    Arc,
    /// A closed, sleepy curve.
    Sleep,
    X,
    Heart,
    /// The right eye winks, the left stays open.
    Wink,
}

/// Every moving part of Peek at one moment.
#[derive(Clone, Copy, Debug)]
struct Face {
    /// Eye height, as a share of the radius.
    open: f64,
    r: f64,
    /// Top lid on each eye, 0 open to 1 shut.
    lid: [f64; 2],
    /// Lid tilt: positive frowns, negative pleads.
    tilt: f64,
    /// Lower lid, 0 open to 1 shut.
    low: f64,
    /// Where the eyes look, about -1 to 1 on each axis.
    look: (f64, f64),
    squash: (f64, f64),
    spin: f64,
    lift: f64,
    glow: f64,
    blush: f64,
    /// Size of each eye.
    size: [f64; 2],
    eye: EyeShape,
    /// Free to follow the pointer and look around.
    roams: bool,
}

impl Default for Face {
    fn default() -> Self {
        Face {
            open: 1.0,
            r: 9.0,
            lid: [0.0; 2],
            tilt: 0.0,
            low: 0.0,
            look: (0.0, 0.0),
            squash: (1.0, 1.0),
            spin: 0.0,
            lift: 0.0,
            glow: 1.0,
            blush: 0.0,
            size: [1.0; 2],
            eye: EyeShape::Dot,
            roams: false,
        }
    }
}

fn face(state: State, t: f64) -> Face {
    use std::f64::consts::PI;
    let s = f64::sin;
    let mut f = Face::default();
    match state {
        State::Idle => {
            f.roams = true;
            f.lift = s(t * 1.6);
        }
        State::Listen => {
            f.open = 1.15;
            f.r = 10.0;
            f.roams = true;
            f.lift = s(t * 3.0) * 1.6;
            f.squash.1 = 1.0 + s(t * 3.0) * 0.02;
        }
        State::Think => {
            f.look = (0.55 + s(t * 1.3) * 0.08, -0.85);
            f.lid = [0.15; 2];
            f.r = 8.5;
            f.lift = s(t * 1.6) * 1.5;
            f.spin = -4.0;
        }
        State::Work => {
            f.lid = [0.42; 2];
            f.tilt = 0.12;
            f.r = 8.5;
            let row = (t * 2.4 / PI).floor().rem_euclid(3.0);
            f.look = (s(t * 2.4) * 0.75, 0.25 + row * 0.12);
        }
        State::Focused => {
            f.lid = [0.52; 2];
            f.tilt = 0.22;
            f.r = 8.0;
            f.glow = 0.75 + 0.35 * (s(t * 1.5) + 1.0) / 2.0;
            f.look.0 = s(t * 0.6) * 0.12;
        }
        State::Done => {
            f.eye = EyeShape::Arc;
            let b = s(t * 4.0).abs();
            f.lift = -b * 6.0;
            f.squash = (1.0 - b * 0.03, 1.0 + b * 0.05);
        }
        State::Needs => {
            f.open = 1.2;
            f.r = 11.0;
            f.tilt = -0.22;
            f.lid = [0.06; 2];
            f.lift = s(t * 5.0) * 1.2;
            f.roams = true;
        }
        State::Error => {
            f.eye = EyeShape::X;
            let k = t.rem_euclid(2.2);
            let shake = if k < 0.5 {
                s(t * 40.0) * 4.0 * (1.0 - k / 0.5)
            } else {
                0.0
            };
            f.spin = shake;
            f.look.0 = shake * 0.05;
        }
        State::Sorry => {
            f.lid = [0.34; 2];
            f.tilt = -0.38;
            f.look = (-0.25, 0.85);
            f.r = 8.5;
            f.glow = 0.55;
            f.lift = 3.0 + s(t * 1.4) * 0.8;
            f.squash.1 = 0.97;
        }
        State::Sleep => {
            f.eye = EyeShape::Sleep;
            f.glow = 0.25;
            f.squash = (1.0 - s(t * 1.2) * 0.012, 1.0 + s(t * 1.2) * 0.025);
            f.lift = 3.0;
        }
        State::Hello => {
            f.eye = EyeShape::Arc;
            let h = t.rem_euclid(1.6);
            f.lift = if h < 0.5 {
                -(h / 0.5 * PI).sin() * 7.0
            } else {
                0.0
            };
            f.spin = s(t * 7.0) * 6.0 * if h < 0.9 { 1.0 } else { 0.2 };
        }
        State::Love => {
            f.eye = EyeShape::Heart;
            f.lift = s(t * 2.0) * 1.5;
            f.squash.1 = 1.0 + s(t * 4.0) * 0.02;
            f.blush = 0.6;
        }
        State::Proud => {
            f.lid = [0.42; 2];
            f.low = 0.12;
            f.tilt = -0.2;
            f.look.1 = -0.35;
            f.spin = -5.0;
            f.lift = -1.0 + s(t * 1.2) * 0.8;
        }
        State::Shy => {
            f.look = (-0.85 + s(t * 0.9) * 0.08, 0.45);
            f.lid = [0.14; 2];
            f.r = 8.0;
            f.blush = 1.0;
            f.squash.1 = 0.97;
            f.spin = 4.0 + s(t * 1.1) * 1.5;
        }
        State::Surprised => {
            f.open = 1.3;
            f.r = 8.0;
            f.squash.1 = 1.06;
            f.roams = true;
        }
        State::Wink => {
            f.eye = EyeShape::Wink;
            f.roams = true;
        }
        State::Curious => {
            f.lid = [0.38, 0.0];
            f.tilt = 0.18;
            f.spin = 8.0 + s(t * 1.5) * 2.0;
            f.look.0 = 0.3;
            f.roams = true;
        }
        State::Confused => {
            f.size = [1.15, 0.78];
            f.lid = [0.0, 0.28];
            f.look = (-0.25, -0.35);
            f.spin = -6.0 + s(t * 1.2) * 3.0;
        }
        State::Blocked => {
            let p = t.rem_euclid(BLOCK);
            f.lid = [0.32; 2];
            f.tilt = 0.26;
            f.look.0 = -0.85;
            if p > 0.45 && p < 0.8 {
                f.spin = -((p - 0.45) / 0.35 * PI).sin() * 5.0;
            }
        }
    }
    f
}

/// A steady pseudo-random number in 0..1 for slot `n`.
fn noise(n: f64) -> f64 {
    ((n * 12.9898).sin() * 43758.5453).rem_euclid(1.0)
}

/// Where an idle Peek glances on its own: a new spot every couple of seconds,
/// sometimes straight ahead, eased between spots.
fn wander(t: f64) -> (f64, f64) {
    let slot = 2.6;
    let n = (t / slot).floor();
    let at = |n: f64| {
        if noise(n + 0.5) < 0.3 {
            (0.0, 0.0)
        } else {
            (
                (noise(n) * 2.0 - 1.0) * 0.8,
                (noise(n + 7.0) * 2.0 - 1.0) * 0.5,
            )
        }
    };
    let (a, b) = (at(n - 1.0), at(n));
    let k = crate::anim::SOFT.at(((t - n * slot) / 0.35).min(1.0));
    (a.0 + (b.0 - a.0) * k, a.1 + (b.1 - a.1) * k)
}

/// How open the eyes are for blinking: 1 open, near 0 mid-blink. Blinks come
/// every few seconds, and every third one is a double blink.
fn blink(t: f64) -> f64 {
    let cycle = 4.6;
    let n = (t / cycle).floor();
    let k = t - n * cycle - noise(n) * 1.5;
    let one = |k: f64| {
        if (0.0..0.16).contains(&k) {
            ((k - 0.08).abs() / 0.08).max(0.06)
        } else {
            1.0
        }
    };
    let second = if n.rem_euclid(3.0) == 1.0 {
        one(k - 0.3)
    } else {
        1.0
    };
    one(k).min(second)
}

/// One eye at `(cx, cy)`. `halo` is the glow's strength, if it glows.
fn eye(p: &mut Pen, at: (f64, f64), f: &Face, side: usize, col: Rgb, halo: Option<f64>, open: f64) {
    let (cx, cy) = at;
    let r = f.r * f.size[side];
    let shape = match f.eye {
        EyeShape::Wink if side == 1 => EyeShape::Arc,
        EyeShape::Wink => EyeShape::Dot,
        e => e,
    };
    let line = |p: &mut Pen, path: &BezPath, w: f64, a: f64| {
        if let Some(h) = halo {
            p.glow(path, w, 3.5, col, 0.55 * h * a);
        }
        p.stroke(path, w, col, a);
    };
    match shape {
        EyeShape::Arc => {
            let mut path = BezPath::new();
            path.move_to((cx - r, cy + 3.0));
            path.quad_to((cx, cy - r * 1.5), (cx + r, cy + 3.0));
            line(p, &path, 5.5, 1.0);
        }
        EyeShape::Sleep => {
            let mut path = BezPath::new();
            path.move_to((cx - r, cy));
            path.quad_to((cx, cy + r * 0.9), (cx + r, cy));
            p.stroke(&path, 4.5, col, 0.55);
        }
        EyeShape::X => {
            let mut path = BezPath::new();
            path.move_to((cx - 7.0, cy - 7.0));
            path.line_to((cx + 7.0, cy + 7.0));
            path.move_to((cx + 7.0, cy - 7.0));
            path.line_to((cx - 7.0, cy + 7.0));
            line(p, &path, 5.0, 1.0);
        }
        EyeShape::Heart => {
            p.at(
                Affine::translate((cx, cy + 1.0)) * Affine::scale(1.45),
                |p| {
                    let h = svg(HEART);
                    if halo.is_some() {
                        p.glow(&h, 1.0, 3.0, PINK, 0.4);
                    }
                    p.fill(&h, PINK, 1.0);
                },
            );
        }
        EyeShape::Dot | EyeShape::Wink => {
            let ry = r * f.open * open;
            if ry < 1.6 {
                let mut path = BezPath::new();
                path.move_to((cx - r, cy));
                path.line_to((cx + r, cy));
                line(p, &path, 4.5, 1.0);
                return;
            }
            let tilt = if side == 1 { -f.tilt } else { f.tilt };
            let top = cy - ry + f.lid[side] * 2.0 * ry;
            let bot = cy + ry - f.low * 2.0 * ry;
            if bot - top < 1.0 {
                return;
            }
            if let Some(h) = halo {
                let (t0, b0) = (top.max(cy - ry), bot.min(cy + ry));
                let mid = (t0 + b0) / 2.0;
                let half = (b0 - t0).max(0.0) / 2.0;
                let e = Ellipse::new((cx, mid), (r + 7.0, half + 7.0), 0.0);
                let g = p.radial(&[(0.35, col, 0.55 * h), (1.0, col, 0.0)], (0.5, 0.5), 0.5);
                p.fill_grad(&e, g, Affine::IDENTITY);
            }
            let mut lids = BezPath::new();
            lids.move_to((cx - r - 4.0, top - tilt * 12.0));
            lids.line_to((cx + r + 4.0, top + tilt * 12.0));
            lids.line_to((cx + r + 4.0, bot));
            lids.line_to((cx - r - 4.0, bot));
            lids.close_path();
            p.clip(&lids, false, |p| {
                p.fill(&Ellipse::new((cx, cy), (r, ry), 0.0), col, 1.0);
                p.fill(
                    &Circle::new((cx + r * 0.33, cy - ry * 0.38), r * 0.33),
                    WHITE,
                    1.0,
                );
            });
        }
    }
}

/// A small prop shown in a badge beside the pill.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Prop {
    Shield,
    Wrench,
    Magnifier,
    Pencil,
    Broom,
    Baton,
    Clipboard,
    Parcel,
    Coffee,
    Hourglass,
    Branch,
}

/// The prop to show: what the job is right now, else what the bot carries.
fn prop_for(pose: &FinPose) -> Option<Prop> {
    let o = pose.fin.outfit;
    match pose.state {
        State::Sleep | State::Blocked => return None,
        State::Think => return Some(Prop::Magnifier),
        State::Focused => return Some(Prop::Hourglass),
        State::Proud => return Some(Prop::Branch),
        _ => {}
    }
    if o.left == Held::Shield {
        return Some(Prop::Shield);
    }
    match o.right {
        Tool::Wrench => return Some(Prop::Wrench),
        Tool::Magnifier => return Some(Prop::Magnifier),
        Tool::Pencil => return Some(Prop::Pencil),
        Tool::Broom => return Some(Prop::Broom),
        Tool::Baton => return Some(Prop::Baton),
        Tool::None => {}
    }
    match o.left {
        Held::Clipboard => Some(Prop::Clipboard),
        Held::Parcel => Some(Prop::Parcel),
        Held::Coffee => Some(Prop::Coffee),
        Held::Shield | Held::None => None,
    }
}

/// A prop drawn in a 24-unit box around the origin, in `col`, moving with `t`.
fn icon(p: &mut Pen, prop: Prop, col: Rgb, t: f64) {
    let s = |p: &mut Pen, d: &str, w: f64, a: f64| p.stroke(&svg(d), w, col, a);
    let f = |p: &mut Pen, d: &str, a: f64| p.fill(&svg(d), col, a);
    match prop {
        Prop::Shield => {
            f(p, SHIELD, 0.2);
            s(p, SHIELD, 2.0, 1.0);
            s(p, "M-3.5 -.5l2.5 2.5 4.5-4.5", 2.0, 1.0);
        }
        Prop::Wrench => p.at(rot((t * 5.0).sin() * 22.0), |p| {
            s(p, "M-7 7L1.5 -1.5", 3.4, 1.0);
            f(p, "M8.02 -4.35A5.2 5.2 0 1 1 4.35 -8.02L4.3 -4.3Z", 1.0);
        }),
        Prop::Magnifier => {
            let at = ((t * 1.8).sin() * 3.0, (t * 3.6).sin() * 1.5);
            p.at(Affine::translate(at), |p| {
                let lens = Circle::new((-2.0, -2.0), 5.6);
                p.fill(&lens, col, 0.15);
                p.stroke(&lens, 2.2, col, 1.0);
                s(p, "M2.2 2.2L7.5 7.5", 3.0, 1.0);
            });
        }
        Prop::Pencil => {
            let k = (t * 0.8).rem_euclid(1.0);
            let mut ink = BezPath::new();
            ink.move_to((-9.0, 10.5));
            ink.line_to((-9.0 + k * 17.0, 10.5));
            p.stroke(&ink, 1.6, col, 0.6);
            let at = (-6.0 + k * 14.0, (t * 14.0).sin() * 0.8);
            p.at(Affine::translate(at), |p| {
                let d = "M10 -9C3 -8 -3 -2 -4 7C1 4 7 -1 10 -9Z";
                f(p, d, 0.25);
                s(p, d, 1.8, 1.0);
                s(p, "M-4 7L4 -2", 1.4, 1.0);
            });
        }
        Prop::Broom => p.at(rot((t * 4.0).sin() * 12.0), |p| {
            s(p, "M-7 -9L2 2", 2.2, 1.0);
            let d = "M0 0L8 -3L11 8Q4 11 -2 7Z";
            f(p, d, 0.45);
            s(p, d, 1.6, 1.0);
        }),
        Prop::Baton => p.at(rot((t * 3.0).sin() * 10.0), |p| {
            s(p, "M-6 8L6 -8", 4.2, 1.0);
            s(p, "M-6 8L-3.5 4.6", 5.4, 0.55);
        }),
        Prop::Clipboard => {
            let board = RoundedRect::new(-6.5, -8.0, 6.5, 9.5, 2.0);
            p.fill(&board, col, 0.15);
            p.stroke(&board, 2.0, col, 1.0);
            p.fill(&RoundedRect::new(-3.0, -10.0, 3.0, -6.0, 1.2), col, 1.0);
            s(p, "M-3 -2h6M-3 1.5h6", 1.5, 0.7);
            let k = ((t * 1.2).rem_euclid(1.0) * 1.6).min(1.0);
            p.fade(k, |p| s(p, "M-3 5.5l1.6 1.6 3.2-3.4", 1.6, 1.0));
        }
        Prop::Parcel => {
            let lift = -((t * 3.0).sin().max(0.0)) * 1.5;
            p.at(Affine::translate((0.0, lift)), |p| {
                let b = RoundedRect::new(-8.0, -5.0, 8.0, 8.0, 1.5);
                p.fill(&b, col, 0.2);
                p.stroke(&b, 2.0, col, 1.0);
                s(p, "M-8 -1H8M0 -5V8", 1.6, 0.8);
            });
        }
        Prop::Coffee => {
            let cup = RoundedRect::new(-6.5, -3.0, 4.5, 9.0, 2.5);
            p.fill(&cup, col, 0.2);
            p.stroke(&cup, 2.0, col, 1.0);
            s(p, "M4.5 0h1.5a2.6 2.6 0 0 1 0 5.2H4.5", 1.8, 1.0);
            for i in 0..2 {
                let k = (t * 0.7 + i as f64 * 0.5).rem_euclid(1.0);
                let x = -3.5 + i as f64 * 4.0;
                let mut steam = BezPath::new();
                steam.move_to((x, -5.0 - k * 4.0));
                steam.quad_to((x + 1.6, -7.0 - k * 4.0), (x, -9.0 - k * 4.0));
                p.stroke(&steam, 1.3, col, 1.0 - k);
            }
        }
        Prop::Hourglass => {
            let c = t.rem_euclid(4.0);
            let (full, turn) = if c < 3.4 {
                (1.0 - c / 3.4, 0.0)
            } else {
                (0.0, (c - 3.4) / 0.6 * 180.0)
            };
            p.at(rot(turn), |p| {
                let h = 8.0 * full;
                let h2 = 8.0 * (1.0 - full);
                let w2 = 5.0 * (9.0 - h2) / 9.0;
                let mut top = BezPath::new();
                top.move_to((-5.0 * h / 9.0, -h));
                top.line_to((5.0 * h / 9.0, -h));
                top.line_to((0.0, 0.0));
                top.close_path();
                p.fill(&top, col, 0.7);
                let mut bot = BezPath::new();
                bot.move_to((-4.6, 8.4));
                bot.line_to((4.6, 8.4));
                bot.line_to((w2, 9.0 - h2));
                bot.line_to((-w2, 9.0 - h2));
                bot.close_path();
                p.fill(&bot, col, 0.7);
                if full > 0.0 {
                    s(p, "M0 0V8", 0.9, 0.7);
                }
                s(p, "M-6 -9H6M-6 9H6M-5 -9L0 0L5 9M5 -9L0 0L-5 9", 2.0, 1.0);
            });
        }
        Prop::Branch => {
            s(p, "M-4 -4V4.5M5 -2.6C5 2 -4 1.5 -4 4.5", 2.0, 1.0);
            for (i, (x, y)) in [(-4.0, -6.2), (5.0, -4.8), (-4.0, 6.8)]
                .into_iter()
                .enumerate()
            {
                let a = 0.35 + 0.65 * (t * 3.0 - i as f64 * 1.3).sin().max(0.0);
                p.fill(&Circle::new((x, y), 2.4), col, a);
            }
        }
    }
}

/// Things around the pill that tell the state at a glance.
fn extras(p: &mut Pen, state: State, f: &Face, col: Rgb, t: f64) {
    let ty = f.lift;
    let s = |p: &mut Pen, d: &str, w: f64, c: Rgb, a: f64| p.stroke(&svg(d), w, c, a);
    match state {
        State::Think => {
            for i in 0..3 {
                let i = i as f64;
                let a = ((t * 2.0 + i).sin() + 1.0) / 2.0;
                let dot = Circle::new((98.0 + i * 7.0, 30.0 - i * 8.0 + ty), 2.4 + i * 1.4);
                p.fill(&dot, col, 0.35 + a * 0.5);
            }
        }
        State::Needs => {
            let b = (t * 5.0).sin().abs() * 4.0;
            p.at(Affine::translate((104.0, 32.0 - b)), |p| {
                p.fill(&Circle::new((0.0, 0.0), 9.0), Rgb(0xFFB020), 1.0);
                p.fill(
                    &RoundedRect::new(-1.6, -5.5, 1.6, 1.5, 1.6),
                    Rgb(0x1A1205),
                    1.0,
                );
                p.fill(&Circle::new((0.0, 4.2), 1.7), Rgb(0x1A1205), 1.0);
            });
        }
        State::Sleep => {
            for i in 0..3 {
                let k = (t * 0.35 + i as f64 / 3.0).rem_euclid(1.0);
                let size = 0.9 + k * 0.8;
                let at = (100.0 + k * 14.0, 30.0 - k * 30.0);
                p.at(Affine::translate(at) * Affine::scale(size), |p| {
                    s(p, "M-3 -3H3L-3 3H3", 1.8, col, (1.0 - k) * 0.9);
                });
            }
        }
        State::Done => {
            for i in 0..5 {
                let i = i as f64;
                let a = i * 1.256 + t * 1.2;
                let rr = 56.0 + (t * 4.0 + i).sin() * 4.0;
                let k = 0.6 + 0.4 * (t * 3.0 + i).sin().abs();
                let at = (60.0 + a.cos() * rr, 62.0 + a.sin() * rr * 0.75);
                p.at(Affine::translate(at) * Affine::scale(k), |p| {
                    p.fill(&svg(STAR), GOLD, 1.0)
                });
            }
        }
        State::Love => {
            for i in 0..3 {
                let i = i as f64;
                let k = (t * 0.5 + i / 3.0).rem_euclid(1.0);
                let at = (24.0 + i * 36.0 + (t + i).sin() * 4.0, 30.0 - k * 26.0);
                p.at(Affine::translate(at) * Affine::scale(0.5 + k * 0.4), |p| {
                    p.fill(&svg(HEART), PINK, 1.0 - k)
                });
            }
        }
        State::Error => {
            if t.rem_euclid(2.2) < 0.8 {
                s(
                    p,
                    "M100 26l-3 8M106 30l-6 5M94 22l-1 8",
                    2.5,
                    Rgb(0xFF5A5F),
                    1.0,
                );
            }
        }
        State::Work => {
            for i in 0..3 {
                let i = i as f64;
                let a = ((t * 6.0 - i * 0.9).sin() + 1.0) / 2.0;
                p.fill(
                    &Circle::new((52.0 + i * 8.0, 100.0 + ty), 2.2),
                    col,
                    0.25 + a * 0.7,
                );
            }
        }
        State::Hello => {
            let h = t.rem_euclid(1.6);
            let k = if h < 0.25 { h / 0.25 } else { 1.0 };
            p.at(
                Affine::translate((100.0, 20.0 + ty)) * Affine::scale(1.4 * k),
                |p| {
                    p.fill(&RoundedRect::new(-10.0, -7.0, 10.0, 6.0, 6.5), col, 1.0);
                    p.fill(&svg("M-5 4.5L-8 10L-0.5 5.5Z"), col, 1.0);
                    s(
                        p,
                        "M-5 -3.5V3M-5 0.5Q-5 -1.5 -2.8 -1.5Q-0.6 -1.5 -0.6 0.5V3",
                        1.5,
                        INK,
                        1.0,
                    );
                    s(p, "M3.5 -0.8V3", 1.5, INK, 1.0);
                    p.fill(&Circle::new((3.5, -3.3), 0.9), INK, 1.0);
                },
            );
        }
        State::Confused => {
            let a = (t * 1.7).sin() * 10.0;
            let at = (104.0, 26.0 + ty + (t * 2.2).sin() * 2.0);
            p.at(Affine::translate(at) * rot(a) * Affine::scale(1.3), |p| {
                s(
                    p,
                    "M-3.5 -3.5Q-3.5 -7.5 0.5 -7.5Q4.5 -7.5 4.5 -4Q4.5 -1.5 0.5 0V2.5",
                    2.4,
                    col,
                    1.0,
                );
                p.fill(&Circle::new((0.5, 6.0), 1.5), col, 1.0);
            });
        }
        State::Sorry => {
            let k = (t * 0.45).rem_euclid(1.0);
            p.at(Affine::translate((99.0, 36.0 + k * 12.0 + ty)), |p| {
                let d =
                    "M0 -4.5C2 -1.5 3.2 .8 3.2 2.4A3.2 3.2 0 0 1 -3.2 2.4C-3.2 .8 -2 -1.5 0 -4.5Z";
                p.fill(&svg(d), Rgb(0x7CC4FF), 1.0 - k * k);
            });
        }
        State::Proud => {
            for (x, y, o) in [(22.0, 30.0, 0.0), (100.0, 24.0, 1.7)] {
                let k = (t * 2.4 + o).sin().max(0.0);
                if k > 0.01 {
                    p.at(Affine::translate((x, y + ty)) * Affine::scale(k), |p| {
                        p.fill(&svg(STAR), GOLD, 1.0)
                    });
                }
            }
        }
        State::Surprised => {
            s(
                p,
                "M18 26l-5-6M28 20l-2-7M92 20l2-7M102 26l5-6",
                2.5,
                col,
                0.8,
            );
        }
        State::Blocked => blocked(p, col, t.rem_euclid(BLOCK)),
        _ => {}
    }
}

/// The Guard's block: a risky command flies in, a shield pops up, the command
/// bounces off and falls away. `p` is seconds into the round.
fn blocked(p: &mut Pen, col: Rgb, ph: f64) {
    if ph > 0.3 {
        let g = ((ph - 0.3) / 0.12).min(1.0);
        let k = if g < 1.0 {
            g * 1.25
        } else {
            1.25 - ((ph - 0.42) * 0.6).min(0.1)
        } * 1.5;
        let a = if ph > 1.9 { (BLOCK - ph) / 0.5 } else { 1.0 };
        p.fade(a, |p| {
            p.at(Affine::translate((16.0, 62.0)) * Affine::scale(k), |p| {
                let halo = Circle::new((0.0, 0.0), 20.0);
                let gr = p.radial(&[(0.0, col, 0.35), (1.0, col, 0.0)], (0.5, 0.5), 0.5);
                p.fill_grad(&halo, gr, Affine::IDENTITY);
                let sh = svg(SHIELD);
                p.fill(&sh, BADGE_INK, 1.0);
                p.fill(&sh, col, 0.25);
                p.stroke(&sh, 1.6, col, 1.0);
                p.stroke(&svg("M-3.5 -.5l2.5 2.5 4.5-4.5"), 1.8, col, 1.0);
            });
        });
    }
    if ph > 0.45 && ph < 0.95 {
        let k = (ph - 0.45) / 0.5;
        p.stroke(
            &Circle::new((16.0, 62.0), 16.0 + k * 30.0),
            2.5 * (1.0 - k),
            col,
            1.0 - k,
        );
    }
    let (x, y, turn, a) = if ph < 0.45 {
        (-60.0 + ph / 0.45 * 62.0, 62.0, 0.0, 1.0)
    } else {
        let q = ph - 0.45;
        (
            2.0 - q * 46.0,
            62.0 + q * q * 140.0,
            -q * 260.0,
            (1.0 - q / 0.9).max(0.0),
        )
    };
    p.fade(a, |p| {
        p.at(Affine::translate((x, y)) * rot(turn), |p| {
            let chip = RoundedRect::new(-12.0, -6.5, 12.0, 6.5, 4.0);
            let red = Rgb(0xFF5A5F);
            p.fill(&chip, Rgb(0x2A1214), 1.0);
            p.stroke(&chip, 1.2, red, 1.0);
            p.stroke(
                &svg("M-7 -2.5L-4 0L-7 2.5M-1.5 2.5H7"),
                1.4,
                Rgb(0xFF8A8F),
                1.0,
            );
        });
    });
}

/// Peek's hat, sitting on top of the pill. The shapes come from the old Fin
/// kit, scaled down to fit.
fn wear_hat(p: &mut Pen, h: Hat) {
    if h == Hat::None {
        return;
    }
    let fit = if h == Hat::Headset {
        Affine::translate((60.0, 34.0)) * Affine::scale_non_uniform(1.25, 0.9)
    } else {
        Affine::translate((60.0, 36.5)) * Affine::scale(0.72)
    } * Affine::translate((-60.0, -27.0));
    p.at(fit, |p| hat(p, h));
}

/// Peek's badge, a sticker low on the pill's left.
fn wear_badge(p: &mut Pen, b: Badge) {
    if b == Badge::None {
        return;
    }
    let fit =
        Affine::translate((30.0, 79.0)) * Affine::scale(0.8) * Affine::translate((-60.0, -79.0));
    p.at(fit, |p| badge(p, b));
}

/// The part of the 120 grid a framing shows: `(x0, y0, side)`.
fn view(framing: Framing) -> (f64, f64, f64) {
    match framing {
        // Room for the floating bits, the prop badge and the Guard's block.
        Framing::Full => (-4.0, -2.0, 128.0),
        // Avatars: the pill and any hat, with room for a hop.
        Framing::Bust => (8.0, 8.0, 104.0),
    }
}

/// Draws Peek into `ctx` at time `t`, scaled so the framing fills `px` pixels.
fn draw_into(ctx: &mut RenderContext, pose: &FinPose, t: f64, framing: Framing, px: u16) {
    let (x0, y0, side) = view(framing);
    let k = px as f64 / side;
    let xf = Affine::scale(k) * Affine::translate((-x0, -y0));
    let simple = px < 36;
    let full = framing == Framing::Full;
    let t = t + pose.offset as f64 * OFFSET_STEP;
    let mut p = Pen {
        ctx,
        xf,
        alpha: 1.0,
    };
    let fin = pose.fin;
    let state = pose.state;
    let mut f = face(state, t);
    if f.roams {
        let (gx, gy) = pose.gaze;
        if (gx, gy) == (0, 0) && state == State::Idle {
            let w = wander(t);
            f.look = (f.look.0 + w.0, f.look.1 + w.1);
        }
        f.look = (f.look.0 + gx as f64 * 0.42, f.look.1 + gy as f64 * 0.42);
    }
    let open = if state == State::Sleep { 1.0 } else { blink(t) };
    let strong = fin.tint.strong();
    let light = fin.tint.0[0];
    let solid = fin.material == Material::Solid;
    let lit = if pose.shine { 1.3 } else { 1.0 };
    let eye_col = if solid { INK } else { mix(strong, light, 0.35) };

    if !solid && !simple {
        let e = Ellipse::new((60.0, 66.0), (70.0, 48.0), 0.0);
        let g = p.radial(
            &[(0.0, strong, 0.35 * f.glow * lit), (1.0, strong, 0.0)],
            (0.5, 0.5),
            0.5,
        );
        p.fill_grad(&e, g, Affine::IDENTITY);
    }
    if state == State::Listen && full {
        for i in 0..3 {
            let k = (t * 0.7 + i as f64 / 3.0).rem_euclid(1.0);
            let ring = RoundedRect::new(
                14.0 - k * 16.0,
                34.0 - k * 12.0,
                106.0 + k * 16.0,
                90.0 + k * 12.0,
                28.0 + k * 12.0,
            );
            p.stroke(&ring, 1.5, strong, (1.0 - k) * 0.5);
        }
    }

    let body = Affine::translate((0.0, f.lift))
        * about(
            (60.0, 62.0),
            rot(f.spin) * Affine::scale_non_uniform(f.squash.0, f.squash.1),
        );
    p.at(body, |p| {
        let shell = pill();
        if solid {
            let g = p.linear(
                &[
                    (0.0, mix(light, strong, 0.3), 1.0),
                    (1.0, mix(strong, BLACK, 0.2), 1.0),
                ],
                (0.5, 0.0),
                (0.5, 1.0),
            );
            p.fill_grad(&shell, g, Affine::IDENTITY);
            p.stroke(&shell, 2.0, mix(strong, BLACK, 0.35), 0.8);
        } else {
            let g = p.linear(
                &[(0.0, Rgb(0x2A2833), 1.0), (1.0, Rgb(0x0F0F14), 1.0)],
                (0.5, 0.0),
                (0.5, 1.0),
            );
            p.fill_grad(&shell, g, Affine::IDENTITY);
            if fin.material == Material::Bubble {
                let g = p.linear(&rainbow(0.9), (0.0, 0.0), (1.0, 1.0));
                p.stroke_grad(&shell, 2.2, g, rot(t * 40.0));
            } else {
                let rim = if state == State::Needs {
                    mix(strong, AMBER, 0.5 + (t * 6.0).sin() * 0.4)
                } else {
                    strong
                };
                p.stroke(&shell, 2.0, rim, (0.6 * lit).min(1.0));
            }
        }
        let shine = if solid { 0.4 } else { 0.16 };
        p.stroke(&svg("M36 42Q48 39 62 40"), 3.0, WHITE, shine);
        let (l, r) = (42.0 + f.look.0 * 8.0, 78.0 + f.look.0 * 8.0);
        let cy = 62.0 + f.look.1 * 6.0;
        if f.blush > 0.02 {
            for x in [32.0, 88.0] {
                let at = (x + f.look.0 * 4.0, 75.0 + f.look.1 * 4.0);
                p.fill(&Ellipse::new(at, (7.5, 3.6), 0.0), PINK, 0.5 * f.blush);
            }
        }
        let halo = (!solid && !simple).then_some(f.glow * lit);
        eye(p, (l, cy), &f, 0, eye_col, halo, open);
        eye(p, (r, cy), &f, 1, eye_col, halo, open);
        if !simple {
            wear_badge(p, fin.outfit.badge);
            wear_hat(p, fin.outfit.hat);
        }
    });

    if simple {
        return;
    }
    if let Some(prop) = prop_for(pose) {
        let (x, y, k) = if full {
            (108.0, 85.0, 0.8)
        } else {
            (101.0, 84.0, 0.6)
        };
        let bob = (t * 2.0).sin() * 1.5;
        p.at(
            Affine::translate((x, y + f.lift + bob)) * Affine::scale(k),
            |p| {
                let halo = Circle::new((0.0, 0.0), 24.0);
                let g = p.radial(&[(0.0, strong, 0.35), (1.0, strong, 0.0)], (0.5, 0.5), 0.5);
                p.fill_grad(&halo, g, Affine::IDENTITY);
                let disc = Circle::new((0.0, 0.0), 13.5);
                p.fill(&disc, BADGE_INK, 1.0);
                p.stroke(&disc, 1.5, strong, 0.55);
                let ic = if solid { strong } else { eye_col };
                p.at(Affine::scale(0.82), |p| icon(p, prop, ic, t));
            },
        );
    }
    if full {
        extras(&mut p, state, &f, if solid { strong } else { eye_col }, t);
    }
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
                    gaze: (0, 0),
                    offset: 0,
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
    fn same_time_same_frame() {
        let pose = fin(Material::Glass, ROLES[0].outfit).pose(Mood::Idle);
        let mut r = FinRenderer::new(64);
        let a = r.draw(&pose, 7.7, Framing::Full).to_vec();
        assert_eq!(a, r.draw(&pose, 7.7, Framing::Full));
    }

    #[test]
    fn blinks_close_the_eyes_now_and_then() {
        let shut = (0..2000).filter(|i| blink(*i as f64 * 0.01) < 0.5).count();
        assert!(shut > 0 && shut < 200, "{shut}");
    }

    #[test]
    fn needs_you_has_its_own_look() {
        assert_eq!(State::of(Mood::NeedsYou), State::Needs);
        let names: std::collections::HashSet<_> = State::ALL.iter().map(|s| s.name()).collect();
        assert_eq!(names.len(), State::ALL.len());
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
