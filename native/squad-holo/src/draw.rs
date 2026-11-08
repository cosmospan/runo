//! Draws a bot at time `t` into a Vello scene.
//!
//! This is a direct port of the Holo v2 SVG: the same layers in the same
//! order, in the same 200 × 200 view box, with every CSS and SMIL animation
//! evaluated through [`crate::anim`].

use vello_cpu::kurbo::{
    Affine, BezPath, Cap, Circle, Ellipse, Join, Point, Rect, RoundedRect, Shape, Stroke,
};
use vello_cpu::peniko::{
    color::{AlphaColor, Srgb},
    ColorStop, ColorStops, Gradient, LinearGradientPosition, RadialGradientPosition,
};
use vello_cpu::RenderContext;

use crate::anim::{self, keys, keys_eased, phase, pulse, Ease, RISE, SOFT};
use crate::look::{Eye, Look, Motion, Mouth, Rgb};

type Color = AlphaColor<Srgb>;

const INK: Rgb = Rgb(0x1B1B2E);
const WHITE: Rgb = Rgb(0xFFFFFF);
const IRI: [(f32, Rgb); 5] = [
    (0.0, Rgb(0xFFB3D9)),
    (0.25, Rgb(0xFFE9A8)),
    (0.5, Rgb(0xB8F5DA)),
    (0.75, Rgb(0xA9D8FF)),
    (1.0, Rgb(0xD6C2FF)),
];

/// Which layers to draw. Small avatars drop the projector, particles, scan,
/// shine, glitch and floating props, and keep the body, face and bob.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Detail {
    pub projector: bool,
    pub glow: bool,
    pub effects: bool,
    pub props: bool,
}

impl Detail {
    pub const FULL: Detail = Detail {
        projector: true,
        glow: true,
        effects: true,
        props: true,
    };
    /// For avatars around 40 to 96 px.
    pub const MEDIUM: Detail = Detail {
        projector: false,
        glow: false,
        effects: true,
        props: false,
    };
    /// For avatars under 40 px.
    pub const TINY: Detail = Detail {
        projector: false,
        glow: false,
        effects: false,
        props: false,
    };
}

/// A thin painter over Vello's context: a transform stack and a group alpha.
pub struct Pen<'a> {
    pub ctx: &'a mut RenderContext,
    xf: Affine,
    alpha: f64,
}

impl<'a> Pen<'a> {
    pub fn new(ctx: &'a mut RenderContext, xf: Affine) -> Self {
        Pen {
            ctx,
            xf,
            alpha: 1.0,
        }
    }

    /// Draws `f` with `t` applied on top of the current transform.
    fn with(&mut self, t: Affine, f: impl FnOnce(&mut Self)) {
        let old = self.xf;
        self.xf = old * t;
        f(self);
        self.xf = old;
    }

    /// Draws `f` at a group opacity. Opacity multiplies into each paint
    /// rather than pushing a layer, which is far cheaper and looks the same
    /// for these mostly non-overlapping groups.
    fn fade(&mut self, a: f64, f: impl FnOnce(&mut Self)) {
        if a <= 0.004 {
            return;
        }
        let old = self.alpha;
        self.alpha = old * a.min(1.0);
        f(self);
        self.alpha = old;
    }

    fn clip(&mut self, path: &BezPath, f: impl FnOnce(&mut Self)) {
        self.ctx.set_transform(self.xf);
        self.ctx.push_clip_layer(path);
        f(self);
        self.ctx.pop_layer();
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

    fn fill_grad(&mut self, shape: &impl Shape, g: Gradient, paint: Affine) {
        self.ctx.set_transform(self.xf);
        self.ctx.set_paint_transform(paint);
        self.ctx.set_paint(g);
        self.ctx.fill_path(&shape.to_path(0.05));
        self.ctx.reset_paint_transform();
    }

    fn stroke(&mut self, shape: &impl Shape, st: Stroke, c: Rgb, a: f64) {
        if a * self.alpha <= 0.004 {
            return;
        }
        self.ctx.set_transform(self.xf);
        self.ctx.reset_paint_transform();
        self.ctx.set_stroke(st);
        self.ctx.set_paint(self.color(c, a));
        self.ctx.stroke_path(&shape.to_path(0.05));
    }

    fn stroke_grad(&mut self, shape: &impl Shape, st: Stroke, g: Gradient, paint: Affine) {
        self.ctx.set_transform(self.xf);
        self.ctx.set_stroke(st);
        self.ctx.set_paint_transform(paint);
        self.ctx.set_paint(g);
        self.ctx.stroke_path(&shape.to_path(0.05));
        self.ctx.reset_paint_transform();
    }

    fn stops(&self, stops: &[(f32, Rgb, f64)]) -> ColorStops {
        let v: Vec<ColorStop> = stops
            .iter()
            .map(|&(o, c, a)| ColorStop::from((o, self.color(c, a))))
            .collect();
        ColorStops::from(v.as_slice())
    }

    /// A linear gradient in the shape's bounding box, from `(x0, y0)` to
    /// `(x1, y1)` in 0..1 units, like SVG's `objectBoundingBox`.
    fn bbox_linear(&self, stops: &[(f32, Rgb, f64)], p0: (f64, f64), p1: (f64, f64)) -> Gradient {
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

    /// The design's soft radial blob: full colour in the middle, fading out.
    fn blob(&self, c: Rgb) -> Gradient {
        Gradient {
            kind: RadialGradientPosition {
                start_center: Point::new(0.5, 0.5),
                start_radius: 0.0,
                end_center: Point::new(0.5, 0.5),
                end_radius: 0.5,
            }
            .into(),
            stops: self.stops(&[(0.0, c, 1.0), (0.55, c, 0.7), (1.0, c, 0.0)]),
            ..Default::default()
        }
    }

    fn fill_blob(&mut self, shape: &impl Shape, c: Rgb, a: f64) {
        if a * self.alpha <= 0.004 {
            return;
        }
        let bb = shape.bounding_box();
        let g = self.with_alpha(a, |p| p.blob(c));
        self.fill_grad(shape, g, bbox_xf(bb));
    }

    fn with_alpha<R>(&mut self, a: f64, f: impl FnOnce(&Self) -> R) -> R {
        let old = self.alpha;
        self.alpha = old * a;
        let r = f(self);
        self.alpha = old;
        r
    }

    /// Strokes with the iridescent rim gradient, turning once every 8 s.
    fn stroke_iri(&mut self, shape: &impl Shape, width: f64, t: f64) {
        let bb = shape.bounding_box();
        let stops: Vec<(f32, Rgb, f64)> = IRI.iter().map(|&(o, c)| (o, c, 1.0)).collect();
        let g = self.bbox_linear(&stops, (0.0, 0.0), (1.0, 1.0));
        let turn = phase(t, 8.0, 0.0) * std::f64::consts::TAU;
        let paint = bbox_xf(bb) * about((0.5, 0.5), Affine::rotate(turn));
        self.stroke_grad(shape, round(width), g, paint);
    }
}

fn bbox_xf(bb: Rect) -> Affine {
    Affine::translate((bb.x0, bb.y0))
        * Affine::scale_non_uniform(bb.width().max(1e-3), bb.height().max(1e-3))
}

/// `t` applied around the point `c`, like CSS `transform-origin`.
fn about(c: (f64, f64), t: Affine) -> Affine {
    Affine::translate(c) * t * Affine::translate((-c.0, -c.1))
}

fn round(w: f64) -> Stroke {
    Stroke::new(w).with_caps(Cap::Round).with_join(Join::Round)
}

fn svg(d: &str) -> BezPath {
    BezPath::from_svg(d).unwrap_or_default()
}

fn poly(pts: &[(f64, f64)], close: bool) -> BezPath {
    let mut p = BezPath::new();
    for (i, &pt) in pts.iter().enumerate() {
        if i == 0 {
            p.move_to(pt);
        } else {
            p.line_to(pt);
        }
    }
    if close {
        p.close_path();
    }
    p
}

fn ellipse(cx: f64, cy: f64, rx: f64, ry: f64) -> Ellipse {
    Ellipse::new((cx, cy), (rx, ry), 0.0)
}

/// The cube's faces at turn offset `u`, as in the design's `G()`.
struct Cube {
    top: BezPath,
    left: BezPath,
    right: BezPath,
    out: BezPath,
    edges: BezPath,
    back: BezPath,
    highlight: BezPath,
}

impl Cube {
    fn new(l: &Look, u: f64) -> Cube {
        let (w, h, s, y) = (l.w, l.h, l.s, l.top());
        let (lx, rx, f, k) = (100.0 - w, 100.0 + w, 100.0 + u, 100.0 - u);
        let mut edges = poly(&[(lx, y + h), (f, y + 2.0 * h), (rx, y + h)], false);
        edges.move_to((f, y + 2.0 * h));
        edges.line_to((f, y + 2.0 * h + s));
        let mut back = poly(&[(k, y), (k, y + s)], false);
        back.move_to((lx, y + h + s));
        back.line_to((k, y + s));
        back.line_to((rx, y + h + s));
        Cube {
            top: poly(&[(k, y), (rx, y + h), (f, y + 2.0 * h), (lx, y + h)], true),
            left: poly(
                &[
                    (lx, y + h),
                    (f, y + 2.0 * h),
                    (f, y + 2.0 * h + s),
                    (lx, y + h + s),
                ],
                true,
            ),
            right: poly(
                &[
                    (f, y + 2.0 * h),
                    (rx, y + h),
                    (rx, y + h + s),
                    (f, y + 2.0 * h + s),
                ],
                true,
            ),
            out: poly(
                &[
                    (k, y),
                    (rx, y + h),
                    (rx, y + h + s),
                    (f, y + 2.0 * h + s),
                    (lx, y + h + s),
                    (lx, y + h),
                ],
                true,
            ),
            edges,
            back,
            highlight: poly(&[(lx + 3.0, y + h - 1.0), (k - 2.0, y + 1.6)], false),
        }
    }
}

/// Draws one bot. The view box is 200 × 200; `pen`'s transform maps it to
/// the target.
pub fn bot(pen: &mut Pen, l: &Look, t: f64, d: Detail) {
    let a = l.palette.light;
    let y = l.top();
    let big_h = l.total_height();
    let bot_y = y + big_h;
    let mid_y = y + big_h / 2.0;
    // The turn: the front edge swings left and right, as SMIL `values="0;-a;0;a;0"`.
    let u = keys(
        phase(t, l.turn_secs, 0.0),
        &[
            (0.0, 0.0),
            (0.25, -l.turn),
            (0.5, 0.0),
            (0.75, l.turn),
            (1.0, 0.0),
        ],
        SOFT,
    );

    if d.projector {
        projector(pen, l, t);
        if d.effects {
            particles(pen, l, t);
        }
    }
    if d.props {
        back_props(pen, l, t, mid_y, bot_y);
    }

    // The whole body floats.
    let motion = match l.motion {
        Motion::Bob => Affine::translate((0.0, pulse(t, 4.4, 0.0, 0.0, -7.0))),
        Motion::BobFast => Affine::translate((0.0, pulse(t, 1.6, 0.0, 0.0, -7.0))),
        Motion::Hop => hop(t),
        Motion::Hope => {
            let p = phase(t, 2.2, 0.0);
            let ty = keys(p, &[(0.0, 0.0), (0.5, -4.0), (1.0, 0.0)], SOFT);
            let rot = keys(p, &[(0.0, 0.0), (0.5, -2.0), (1.0, 0.0)], SOFT);
            Affine::translate((0.0, ty)) * about((100.0, mid_y), Affine::rotate(rot.to_radians()))
        }
    };
    pen.with(motion, |pen| {
        if d.glow {
            let p = phase(t, 4.4, 0.0);
            let op = keys(p, &[(0.0, 0.55), (0.5, 0.85), (1.0, 0.55)], SOFT);
            let sc = keys(p, &[(0.0, 1.0), (0.5, 1.08), (1.0, 1.0)], SOFT);
            pen.with(about((100.0, mid_y), Affine::scale(sc)), |pen| {
                pen.fill_blob(
                    &ellipse(100.0, mid_y, l.w + 26.0, big_h / 2.0 + 22.0),
                    a,
                    op,
                );
            });
        }
        let tilt = if l.tilt {
            let r = pulse(t, 4.0, 0.0, -4.0, 4.0);
            about((100.0, 150.0), Affine::rotate(r.to_radians()))
        } else {
            Affine::IDENTITY
        };
        let breathe = if l.deep {
            let k = pulse(t, 6.0, 0.0, 0.0, 1.0);
            about(
                (100.0, 150.0),
                Affine::scale_non_uniform(1.0 + 0.03 * k, 1.0 - 0.03 * k),
            )
        } else {
            let k = pulse(t, 4.6, 0.0, 0.0, 1.0);
            about(
                (100.0, 150.0),
                Affine::scale_non_uniform(1.0 + 0.015 * k, 1.0 - 0.015 * k),
            )
        };
        pen.with(tilt * breathe, |pen| {
            let cube = Cube::new(l, u);
            body(pen, l, &cube, t, d);
            face(pen, l, t, u);
        });
        if d.props {
            front_props(pen, l, t, u);
        } else if l.props.antenna {
            antenna(pen, l, t, u);
        }
    });
}

fn hop(t: f64) -> Affine {
    let p = phase(t, 3.0, 0.0);
    let e1 = Ease(0.2, 0.7, 0.3, 1.0);
    let e2 = Ease(0.6, 0.0, 0.8, 0.4);
    let e3 = Ease(0.3, 0.6, 0.4, 1.0);
    let ty = keys_eased(
        p,
        &[
            (0.0, 0.0, SOFT),
            (0.4, 0.0, SOFT),
            (0.52, 1.0, e1),
            (0.66, -18.0, e2),
            (0.78, 0.0, e3),
            (0.88, -2.0, SOFT),
            (1.0, 0.0, SOFT),
        ],
    );
    let sx = keys_eased(
        p,
        &[
            (0.0, 1.0, SOFT),
            (0.4, 1.0, SOFT),
            (0.52, 1.05, e1),
            (0.66, 0.98, e2),
            (0.78, 1.04, e3),
            (0.88, 0.99, SOFT),
            (1.0, 1.0, SOFT),
        ],
    );
    let sy = keys_eased(
        p,
        &[
            (0.0, 1.0, SOFT),
            (0.4, 1.0, SOFT),
            (0.52, 0.95, e1),
            (0.66, 1.02, e2),
            (0.78, 0.96, e3),
            (0.88, 1.01, SOFT),
            (1.0, 1.0, SOFT),
        ],
    );
    about(
        (100.0, 160.0),
        Affine::translate((0.0, ty)) * Affine::scale_non_uniform(sx, sy),
    )
}

fn projector(pen: &mut Pen, l: &Look, t: f64) {
    let pal = l.palette;
    let (a, b) = (pal.light, pal.accent);
    let bot_y = l.top() + l.total_height();
    pen.fill_blob(&ellipse(100.0, 182.0, 56.0, 11.0), a, 0.7);
    // The beam up to the cube.
    let beam_op = keys(
        phase(t, 3.0, 0.0),
        &[
            (0.0, 0.75),
            (0.4, 0.5),
            (0.46, 0.85),
            (0.52, 0.6),
            (1.0, 0.75),
        ],
        SOFT,
    );
    let beam = poly(
        &[
            (100.0 - l.w * 0.75, bot_y),
            (100.0 + l.w * 0.75, bot_y),
            (128.0, 182.0),
            (72.0, 182.0),
        ],
        true,
    );
    let g = pen.bbox_linear(
        &[(0.0, a, 0.7 * beam_op), (1.0, a, 0.0)],
        (0.0, 1.0),
        (0.0, 0.0),
    );
    pen.fill_grad(&beam, g, bbox_xf(beam.bounding_box()));
    // Two rings ripple out of the disk.
    for delay in [0.0, 1.5] {
        let p = phase(t, 3.0, delay);
        let sc = keys(p, &[(0.0, 0.55), (1.0, 1.5)], SOFT);
        let op = keys(p, &[(0.0, 0.9), (1.0, 0.0)], SOFT);
        pen.with(about((100.0, 182.0), Affine::scale(sc)), |pen| {
            pen.stroke(
                &ellipse(100.0, 182.0, 40.0, 7.5),
                Stroke::new(1.4 / sc.max(0.3)),
                b,
                op,
            );
        });
    }
    let disk = ellipse(100.0, 182.0, 26.0, 5.0);
    pen.fill(&disk, WHITE, 1.0);
    pen.stroke_iri(&disk, 2.0, t);
    // The dashed track turns.
    let off = -60.0 * phase(t, 6.0, 0.0);
    pen.stroke(
        &ellipse(100.0, 182.0, 21.0, 3.8),
        Stroke::new(1.0).with_dashes(off, [2.0, 4.0]),
        b,
        0.8,
    );
    pen.fill(&ellipse(100.0, 181.4, 15.0, 2.6), a, 1.0);
}

fn particles(pen: &mut Pen, l: &Look, t: f64) {
    let b = l.palette.accent;
    for (cx, delay, r, dur) in [
        (84.0, 0.0, 1.4, 3.2),
        (96.0, -1.1, 1.0, 2.6),
        (108.0, -2.2, 1.6, 3.6),
        (116.0, -0.6, 1.1, 2.9),
        (90.0, -2.7, 0.9, 3.4),
        (103.0, -1.8, 1.2, 3.0),
    ] {
        let p = phase(t, dur, delay);
        let ty = keys(p, &[(0.0, 0.0), (1.0, -58.0)], RISE);
        let op = keys(p, &[(0.0, 0.0), (0.2, 1.0), (1.0, 0.0)], RISE);
        pen.fill(
            &Circle::new((cx, 178.0 + ty), r),
            if r > 1.3 { b } else { WHITE },
            op,
        );
    }
}

fn body(pen: &mut Pen, l: &Look, cube: &Cube, t: f64, d: Detail) {
    let pal = l.palette;
    let (a, b, c) = (pal.light, pal.accent, pal.swirl);
    let y = l.top();
    let bot_y = y + l.total_height();
    pen.clip(&cube.out, |pen| {
        pen.fill(&Rect::new(0.0, 0.0, 200.0, 200.0), WHITE, 0.35);
        // Colour drifting inside the glass.
        for (cx, cy, r, col, dur, dx, dy, sc) in [
            (72.0, y + 10.0, 46.0, a, 9.0, 40.0, 34.0, 1.2),
            (132.0, bot_y - 6.0, 50.0, b, 11.0, -44.0, -36.0, 0.85),
            (134.0, y + 16.0, 38.0, c, 7.0, -30.0, 40.0, 1.25),
            (66.0, bot_y - 8.0, 42.0, b, 10.0, 36.0, -30.0, 1.15),
        ] {
            let k = anim::alternate(t, dur, SOFT);
            let xf = Affine::translate((dx * k, dy * k))
                * about((cx, cy), Affine::scale(1.0 + (sc - 1.0) * k));
            pen.with(xf, |pen| pen.fill_blob(&Circle::new((cx, cy), r), col, 1.0));
        }
        pen.stroke(
            &cube.back,
            Stroke::new(1.1).with_dashes(0.0, [3.0, 4.0]),
            WHITE,
            0.55,
        );
        let topf = pen.bbox_linear(
            &[(0.0, WHITE, 0.75), (1.0, WHITE, 0.25)],
            (0.0, 0.0),
            (0.0, 1.0),
        );
        pen.fill_grad(&cube.top, topf, bbox_xf(cube.top.bounding_box()));
        let lf = pen.bbox_linear(
            &[(0.0, WHITE, 0.32), (1.0, WHITE, 0.0)],
            (0.0, 0.0),
            (1.0, 1.0),
        );
        pen.fill_grad(&cube.left, lf, bbox_xf(cube.left.bounding_box()));
        let rf = pen.bbox_linear(
            &[(0.0, Rgb(0x1B1B4E), 0.0), (1.0, Rgb(0x1B1B4E), 0.2)],
            (0.0, 0.0),
            (1.0, 1.0),
        );
        pen.fill_grad(&cube.right, rf, bbox_xf(cube.right.bounding_box()));
        if d.effects {
            // A scan line sweeps down through the glass.
            let p = phase(t, if l.fast { 1.4 } else { 3.6 }, 0.0);
            let sy = keys(p, &[(0.0, y - 8.0), (1.0, bot_y)], SOFT);
            let op = keys(p, &[(0.0, 0.0), (0.12, 1.0), (0.8, 1.0), (1.0, 0.0)], SOFT);
            pen.fade(op, |pen| {
                let band = Rect::new(0.0, sy, 200.0, sy + 8.0);
                let g = pen.bbox_linear(
                    &[(0.0, WHITE, 0.0), (0.5, WHITE, 0.95), (1.0, WHITE, 0.0)],
                    (0.0, 0.0),
                    (0.0, 1.0),
                );
                pen.fill_grad(&band, g, bbox_xf(band));
                pen.fill(&Rect::new(0.0, sy + 3.4, 200.0, sy + 4.6), b, 0.6);
            });
            // A shine passes across every few seconds.
            let sx = keys(
                phase(t, 6.0, 0.0),
                &[(0.0, -90.0), (0.5, -90.0), (0.85, 170.0), (1.0, 170.0)],
                SOFT,
            );
            pen.with(
                Affine::translate((sx, 0.0)) * Affine::skew((-20f64).to_radians().tan(), 0.0),
                |pen| {
                    let r = Rect::new(0.0, 0.0, 26.0, 200.0);
                    let g = pen.bbox_linear(
                        &[(0.0, WHITE, 0.0), (0.5, WHITE, 0.6), (1.0, WHITE, 0.0)],
                        (0.0, 0.0),
                        (1.0, 0.0),
                    );
                    pen.fill_grad(&r, g, bbox_xf(r));
                },
            );
        }
    });
    pen.stroke(&cube.edges, round(1.6), WHITE, 0.9);
    pen.stroke_iri(&cube.out, 2.4, t);
    pen.stroke(&cube.highlight, round(3.4), WHITE, 0.95);
    if d.effects {
        // A tiny colour glitch now and then.
        for (delay, col) in [(0.0, Rgb(0x3CE0FF)), (0.08, Rgb(0xFF4FC3))] {
            let p = phase(t, 7.0, delay);
            let op = anim::steps(
                p,
                &[
                    (0.0, 0.0),
                    (0.89, 0.75),
                    (0.90, 0.0),
                    (0.92, 0.6),
                    (0.93, 0.0),
                ],
            );
            let dx = anim::steps(
                p,
                &[
                    (0.0, 0.0),
                    (0.89, -3.0),
                    (0.90, 0.0),
                    (0.92, 2.5),
                    (0.93, 0.0),
                ],
            );
            if op > 0.0 {
                pen.with(Affine::translate((dx, 0.0)), |pen| {
                    pen.stroke(&cube.out, Stroke::new(1.6), col, op)
                });
            }
        }
    }
}

fn face(pen: &mut Pen, l: &Look, t: f64, u: f64) {
    let xf = Affine::translate((0.0, l.face_y() - 104.0))
        * about((100.0, 104.0), Affine::scale(l.face_scale))
        * Affine::translate((u * 0.6, 0.0));
    pen.with(xf, |pen| {
        if !matches!(l.eye, Eye::Lens | Eye::Visor) {
            let op = pulse(t, 3.2, 0.0, 0.42, 0.7);
            pen.fill(&ellipse(81.0, 112.0, 4.0, 2.2), Rgb(0xFF6F91), op);
            pen.fill(&ellipse(119.0, 112.0, 4.0, 2.2), Rgb(0xFF6F91), op);
        }
        eyes(pen, l.eye, t);
        mouth(pen, l.mouth, t);
    });
}

fn look_a(t: f64) -> (f64, f64) {
    let p = phase(t, 8.0, 0.0);
    let xs = [
        (0.0, 0.0),
        (0.14, 0.0),
        (0.2, 2.4),
        (0.38, 2.4),
        (0.44, -2.4),
        (0.6, -2.4),
        (0.66, 0.4),
        (0.82, 0.4),
        (0.88, 0.0),
        (1.0, 0.0),
    ];
    let ys = [
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
    ];
    (keys(p, &xs, SOFT), keys(p, &ys, SOFT))
}

fn blink(t: f64) -> f64 {
    keys(
        phase(t, 5.2, 0.0),
        &[
            (0.0, 1.0),
            (0.45, 1.0),
            (0.49, 0.08),
            (0.51, 0.08),
            (0.55, 1.0),
            (1.0, 1.0),
        ],
        SOFT,
    )
}

fn double_blink(t: f64) -> f64 {
    keys(
        phase(t, 6.0, 0.0),
        &[
            (0.0, 1.0),
            (0.4, 1.0),
            (0.43, 0.08),
            (0.47, 1.0),
            (0.51, 0.08),
            (0.55, 1.0),
            (1.0, 1.0),
        ],
        SOFT,
    )
}

fn dot_eyes(pen: &mut Pen, look: (f64, f64), sy: f64, s: f64) {
    pen.with(
        Affine::translate(look) * about((100.0, 104.0), Affine::scale_non_uniform(1.0, sy)),
        |pen| {
            for x in [89.0, 111.0] {
                pen.fill(&ellipse(x, 104.0, 3.8 * s, 4.8 * s), INK, 1.0);
                pen.fill(&Circle::new((x + 1.4, 102.2), 1.3), WHITE, 1.0);
            }
        },
    );
}

fn eyes(pen: &mut Pen, eye: Eye, t: f64) {
    match eye {
        Eye::Dots => dot_eyes(pen, look_a(t), double_blink(t), 1.0),
        Eye::Up => {
            let p = phase(t, 4.0, 0.0);
            let lk = (
                keys(p, &[(0.0, 1.5), (0.5, 3.0), (1.0, 1.5)], SOFT),
                keys(p, &[(0.0, -2.6), (0.5, -3.4), (1.0, -2.6)], SOFT),
            );
            dot_eyes(pen, lk, blink(t), 1.0);
        }
        Eye::Read => {
            let x = pulse(t, 1.8, 0.0, -2.4, 2.4);
            dot_eyes(pen, (x, 1.0), blink(t), 1.0);
        }
        Eye::You => {
            pen.stroke(
                &svg("M83 93q6-3 11 1M106 94q6-4 11-1"),
                round(2.2),
                INK,
                0.7,
            );
            dot_eyes(pen, look_a(t), blink(t), 1.12);
        }
        Eye::Smile => pen.stroke(
            &svg("M83 105q6-7 12 0M105 105q6-7 12 0"),
            round(2.8),
            INK,
            1.0,
        ),
        Eye::Happy => pen.stroke(
            &svg("M82 106q7-9 14 0M104 106q7-9 14 0"),
            round(3.2),
            INK,
            1.0,
        ),
        Eye::Lens => {
            let c = (114.0, 102.0);
            pen.fill(&Circle::new(c, 14.0), WHITE, 0.35);
            let spin = phase(t, 6.0, 0.0) * std::f64::consts::TAU;
            pen.with(about(c, Affine::rotate(spin)), |pen| {
                pen.stroke(
                    &Circle::new(c, 12.5),
                    Stroke::new(1.6).with_dashes(0.0, [3.0, 3.6]),
                    WHITE,
                    1.0,
                );
            });
            let rp = pulse(t, 3.0, 0.0, 1.0, 0.88);
            pen.with(about(c, Affine::scale(rp)), |pen| {
                pen.stroke(&Circle::new(c, 10.0), Stroke::new(1.4), Rgb(0x1FC27A), 0.7);
            });
            pen.fill(&Circle::new(c, 8.4), WHITE, 1.0);
            let p = phase(t, 5.0, 0.0);
            let sx = keys(
                p,
                &[
                    (0.0, -3.4),
                    (0.25, 3.4),
                    (0.4, 0.0),
                    (0.62, 0.0),
                    (0.8, -1.0),
                    (1.0, -3.4),
                ],
                SOFT,
            );
            let sy = keys(
                p,
                &[
                    (0.0, 1.0),
                    (0.25, 1.0),
                    (0.4, 2.6),
                    (0.62, 2.6),
                    (0.8, -1.0),
                    (1.0, 1.0),
                ],
                SOFT,
            );
            let iris = keys(
                p,
                &[
                    (0.0, 1.0),
                    (0.3, 0.72),
                    (0.46, 0.72),
                    (0.6, 1.15),
                    (0.72, 1.0),
                    (1.0, 1.0),
                ],
                SOFT,
            );
            pen.with(Affine::translate((sx, sy)), |pen| {
                pen.with(about(c, Affine::scale(iris)), |pen| {
                    pen.fill(&Circle::new(c, 4.8), INK, 1.0)
                });
                let gp = phase(t, 2.6, 0.0);
                let gs = keys(gp, &[(0.0, 1.0), (0.5, 0.7), (1.0, 1.0)], SOFT);
                let go = keys(gp, &[(0.0, 1.0), (0.5, 0.6), (1.0, 1.0)], SOFT);
                pen.with(about((115.8, 100.2), Affine::scale(gs)), |pen| {
                    pen.fill(&Circle::new((115.8, 100.2), 1.5), WHITE, go)
                });
            });
        }
        Eye::Visor => {
            pen.with(Affine::translate(look_a(t)), |pen| {
                pen.with(
                    about((100.0, 103.5), Affine::scale_non_uniform(1.0, blink(t))),
                    |pen| {
                        pen.fill(&RoundedRect::new(80.0, 99.0, 97.0, 108.0, 4.5), INK, 1.0);
                        pen.fill(&RoundedRect::new(103.0, 99.0, 120.0, 108.0, 4.5), INK, 1.0);
                    },
                );
                let p = phase(t, 4.0, 0.0);
                let dx = keys(
                    p,
                    &[
                        (0.0, 0.0),
                        (0.6, 0.0),
                        (0.8, 9.0),
                        (0.8001, 0.0),
                        (1.0, 0.0),
                    ],
                    SOFT,
                );
                let op = keys(p, &[(0.0, 0.95), (0.6, 0.95), (0.8, 0.0), (1.0, 0.0)], SOFT);
                for x in [82.0, 105.0] {
                    pen.fill(
                        &RoundedRect::new(x + dx, 100.6, x + dx + 5.0, 103.0, 1.2),
                        WHITE,
                        op,
                    );
                }
            });
        }
        Eye::Sleepy => {
            let p = phase(t, 7.0, 0.0);
            let shut = keys(
                p,
                &[
                    (0.0, 1.0),
                    (0.6, 1.0),
                    (0.64, 0.0),
                    (0.76, 0.0),
                    (0.8, 1.0),
                    (1.0, 1.0),
                ],
                SOFT,
            );
            pen.stroke(
                &svg("M83 103q6 6 12 0M105 103q6 6 12 0"),
                round(2.8),
                INK,
                shut,
            );
            pen.fill(&Circle::new((89.0, 104.0), 3.4), INK, 1.0 - shut);
            pen.fill(&Circle::new((111.0, 104.0), 3.4), INK, 1.0 - shut);
        }
        Eye::Squint => {
            let p = phase(t, 4.4, 0.0);
            let lx = keys(
                p,
                &[
                    (0.0, -1.8),
                    (0.16, -1.8),
                    (0.22, 1.8),
                    (0.4, 1.8),
                    (0.46, -0.4),
                    (0.64, -0.4),
                    (0.7, -1.8),
                    (1.0, -1.8),
                ],
                SOFT,
            );
            let ly = keys(
                p,
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
                SOFT,
            );
            pen.with(
                Affine::translate((lx, ly))
                    * about((89.0, 104.0), Affine::scale_non_uniform(1.0, blink(t))),
                |pen| {
                    pen.fill(&ellipse(89.0, 104.0, 3.8, 4.8), INK, 1.0);
                    pen.fill(&Circle::new((90.4, 102.2), 1.3), WHITE, 1.0);
                },
            );
            pen.stroke(&svg("M105 104h12"), round(3.0), INK, 1.0);
        }
    }
}

fn mouth(pen: &mut Pen, m: Mouth, t: f64) {
    match m {
        Mouth::Grin => pen.stroke(&svg("M94 115q6 4.6 12 0"), round(2.4), INK, 1.0),
        Mouth::O => pen.fill(&ellipse(102.0, 118.0, 3.0, 2.8), INK, 1.0),
        Mouth::Open => pen.fill(&svg("M93 115Q100 125 107 115Q100 117.6 93 115Z"), INK, 1.0),
        Mouth::Small => pen.stroke(&svg("M96 117q4 2 8 0"), round(2.4), INK, 1.0),
        Mouth::Flat => pen.stroke(&svg("M95 118q5-2.4 10 0"), round(2.4), INK, 1.0),
        Mouth::Talk => {
            let sy = pulse(t, 0.9, 0.0, 1.0, 0.35);
            pen.with(
                about((100.0, 116.5), Affine::scale_non_uniform(1.0, sy)),
                |pen| {
                    pen.fill(&svg("M93 114Q100 124 107 114Q100 116.6 93 114Z"), INK, 1.0);
                },
            );
        }
        Mouth::Check => {
            // Doubtful while scanning, then a smile as the tick pops.
            let p = phase(t, 5.0, 0.0);
            let doubt = keys(
                p,
                &[
                    (0.0, 1.0),
                    (0.46, 1.0),
                    (0.5, 0.0),
                    (0.88, 0.0),
                    (0.92, 1.0),
                    (1.0, 1.0),
                ],
                SOFT,
            );
            pen.stroke(&svg("M107 119q4-1.6 8 0"), round(2.4), INK, doubt);
            pen.stroke(&svg("M105 117q6 4.6 12 0"), round(2.4), INK, 1.0 - doubt);
        }
    }
}

fn spark(pen: &mut Pen, x: f64, y: f64, c: Rgb, delay: f64, s: f64, t: f64) {
    let p = phase(t, 1.8, delay);
    let k = keys(p, &[(0.0, 0.0), (0.5, 1.0), (1.0, 0.0)], SOFT);
    if k <= 0.01 {
        return;
    }
    let path = svg(&format!(
        "M{x} {y0}l{a} {b}l{b} {a}l{nb} {a}l{na} {b}l{na} {nb}l{nb} {na}l{b} {na}z",
        y0 = y - 6.0 * s,
        a = 1.6 * s,
        b = 4.4 * s,
        na = -1.6 * s,
        nb = -4.4 * s
    ));
    pen.with(
        about(
            (x, y),
            Affine::rotate((90.0 * k).to_radians()) * Affine::scale(k),
        ),
        |pen| pen.fill(&path, c, k),
    );
}

fn antenna(pen: &mut Pen, l: &Look, t: f64, u: f64) {
    let pal = l.palette;
    let y = l.top();
    let sway = pulse(t, 3.2, 0.0, -9.0, 9.0);
    pen.with(
        Affine::translate((-u, 0.0)) * about((100.0, y), Affine::rotate(sway.to_radians())),
        |pen| {
            pen.stroke(
                &poly(&[(100.0, y), (100.0, y - 18.0)], false),
                round(2.2),
                pal.accent,
                1.0,
            );
            let p = phase(t, 2.2, 0.0);
            let sc = keys(p, &[(0.0, 1.0), (0.5, 1.5), (1.0, 1.0)], SOFT);
            let op = keys(p, &[(0.0, 1.0), (0.5, 0.6), (1.0, 1.0)], SOFT);
            let c = (100.0, y - 21.0);
            pen.with(about(c, Affine::scale(sc)), |pen| {
                pen.fill(&Circle::new(c, 4.4), pal.light, op);
                pen.stroke(&Circle::new(c, 4.4), Stroke::new(1.6), WHITE, op);
            });
        },
    );
}

fn back_props(pen: &mut Pen, l: &Look, t: f64, mid_y: f64, bot_y: f64) {
    let pal = l.palette;
    let (a, b) = (pal.light, pal.accent);
    let (y, w, h) = (l.top(), l.w, l.h);
    if l.props.shield {
        let hex = poly(
            &[
                (100.0, y - 22.0),
                (100.0 + w + 22.0, y + h - 6.0),
                (100.0 + w + 22.0, bot_y - h + 6.0),
                (100.0, bot_y + 18.0),
                (100.0 - w - 22.0, bot_y - h + 6.0),
                (100.0 - w - 22.0, y + h - 6.0),
            ],
            true,
        );
        let p = phase(t, 3.4, 0.0);
        let sc = keys(p, &[(0.0, 1.0), (0.5, 1.06), (1.0, 1.0)], SOFT);
        let op = keys(p, &[(0.0, 0.65), (0.5, 0.3), (1.0, 0.65)], SOFT);
        let c = hex.bounding_box().center();
        let off = -80.0 * phase(t, 9.0, 0.0);
        pen.with(about((c.x, c.y), Affine::scale(sc)), |pen| {
            pen.fill(&hex, a, 0.14 * op);
            pen.stroke(
                &hex,
                Stroke::new(1.6)
                    .with_join(Join::Round)
                    .with_dashes(off, [5.0, 5.0]),
                b,
                op,
            );
        });
    }
    if l.props.done {
        let p = phase(t, 2.4, 0.0);
        let sc = keys(p, &[(0.0, 0.5), (0.7, 1.7), (1.0, 1.7)], SOFT);
        let op = keys(p, &[(0.0, 0.9), (0.7, 0.0), (1.0, 0.0)], SOFT);
        let half = l.total_height() / 2.0;
        pen.with(about((100.0, mid_y), Affine::scale(sc)), |pen| {
            pen.stroke(
                &ellipse(100.0, mid_y, w + 14.0, half + 14.0),
                Stroke::new(2.0 / sc),
                b,
                op,
            );
        });
    }
}

fn front_props(pen: &mut Pen, l: &Look, t: f64, u: f64) {
    let pal = l.palette;
    let (a, b) = (pal.light, pal.accent);
    let (y, w, h) = (l.top(), l.w, l.h);
    let pr = l.props;
    if pr.antenna {
        antenna(pen, l, t, u);
    }
    if pr.code {
        // `</>` and `{ }` float up and away from the cube.
        for (delay, x, yy, glyph, size) in [
            (0.0, 100.0 - w - 14.0, y + 18.0, 0, 10.0),
            (2.1, 100.0 + w + 2.0, y + 28.0, 1, 9.0),
        ] {
            let p = phase(t, 4.2, delay);
            let dx = keys(p, &[(0.0, 0.0), (0.55, 0.0), (1.0, -8.0)], SOFT);
            let dy = keys(p, &[(0.0, 6.0), (0.55, 6.0), (1.0, -22.0)], SOFT);
            let op = keys(p, &[(0.0, 0.0), (0.55, 0.0), (0.65, 1.0), (1.0, 0.0)], SOFT);
            let d = if glyph == 0 {
                "M3.4 -7.2L0.4 -3.8L3.4 -0.4M7.8 -8.2L5.4 0.6M9.8 -7.2L12.8 -3.8L9.8 -0.4"
            } else {
                "M3.2 -8.4Q1.6 -8.4 1.6 -6.8V-5.4Q1.6 -4.2 0.2 -4.2Q1.6 -4.2 1.6 -3V-1.6Q1.6 0 3.2 0M7.6 -8.4Q9.2 -8.4 9.2 -6.8V-5.4Q9.2 -4.2 10.6 -4.2Q9.2 -4.2 9.2 -3V-1.6Q9.2 0 7.6 0"
            };
            pen.with(
                Affine::translate((x + dx, yy + dy)) * Affine::scale(size / 10.0),
                |pen| {
                    pen.stroke(&svg(d), round(1.8), b, op);
                },
            );
        }
    }
    if pr.laser {
        let lx = 100.0 + 14.0 * l.face_scale;
        let ly = l.face_y() - 2.0 * l.face_scale;
        let p = phase(t, 5.0, 0.0);
        pen.with(Affine::translate((u * 0.6, 0.0)), |pen| {
            let op = keys(
                p,
                &[
                    (0.0, 0.0),
                    (0.08, 0.0),
                    (0.14, 0.9),
                    (0.36, 0.9),
                    (0.44, 0.0),
                    (1.0, 0.0),
                ],
                SOFT,
            );
            let rot = keys(
                p,
                &[(0.0, -24.0), (0.08, -24.0), (0.36, 24.0), (1.0, 24.0)],
                SOFT,
            );
            let sy = keys(p, &[(0.0, 0.6), (0.08, 0.6), (0.36, 1.0), (1.0, 1.0)], SOFT);
            if op > 0.0 {
                pen.with(
                    about(
                        (lx, ly),
                        Affine::rotate(rot.to_radians()) * Affine::scale_non_uniform(1.0, sy),
                    ),
                    |pen| {
                        let tri = poly(
                            &[(lx, ly), (lx - 16.0, ly + 70.0), (lx + 16.0, ly + 70.0)],
                            true,
                        );
                        let g = pen.with_alpha(op, |p| {
                            p.bbox_linear(
                                &[(0.0, Rgb(0x5BE8A6), 0.75), (1.0, Rgb(0x8EF0C0), 0.0)],
                                (0.0, 0.0),
                                (0.0, 1.0),
                            )
                        });
                        pen.fill_grad(&tri, g, bbox_xf(tri.bounding_box()));
                    },
                );
            }
            let glint = keys(
                p,
                &[(0.0, 0.0), (0.6, 0.0), (0.7, 1.0), (0.8, 0.0), (1.0, 0.0)],
                SOFT,
            );
            pen.fill_blob(&Circle::new((lx, ly), 20.0), Rgb(0x8EF0C0), glint);
        });
        // The tick pops when the check passes.
        let c = (100.0 + w + 16.0, y + 10.0);
        let sc = keys(
            p,
            &[
                (0.0, 0.0),
                (0.44, 0.0),
                (0.52, 1.2),
                (0.58, 1.0),
                (0.86, 1.0),
                (0.94, 0.0),
                (1.0, 0.0),
            ],
            SOFT,
        );
        let rot = keys(
            p,
            &[(0.0, -30.0), (0.44, -30.0), (0.52, 0.0), (1.0, 0.0)],
            SOFT,
        );
        let op = keys(
            p,
            &[
                (0.0, 0.0),
                (0.44, 0.0),
                (0.52, 1.0),
                (0.86, 1.0),
                (0.94, 0.0),
                (1.0, 0.0),
            ],
            SOFT,
        );
        if sc > 0.01 {
            pen.with(
                about(c, Affine::rotate(rot.to_radians()) * Affine::scale(sc)),
                |pen| {
                    pen.fill(&Circle::new(c, 11.0), Rgb(0x1FC27A), op);
                    pen.stroke(
                        &poly(
                            &[
                                (c.0 - 5.0, c.1 + 0.4),
                                (c.0 - 1.4, c.1 + 4.0),
                                (c.0 + 5.0, c.1 - 3.0),
                            ],
                            false,
                        ),
                        round(2.6),
                        WHITE,
                        op,
                    );
                },
            );
        }
        let led = pulse(t, 2.4, 0.0, 0.35, 1.0);
        pen.fill(
            &Circle::new((100.0 - u * 0.2, y + h), 2.6),
            Rgb(0x1FC27A),
            led,
        );
    }
    if pr.lock {
        pen.with(Affine::translate((100.0 + w + 16.0, y - 8.0)), |pen| {
            pen.fill(&RoundedRect::new(-6.0, -1.0, 6.0, 9.0, 2.6), b, 1.0);
            pen.stroke(
                &svg("M-3.6 -1v-3a3.6 3.6 0 0 1 7.2 0v3"),
                Stroke::new(2.0),
                b,
                1.0,
            );
            pen.fill(
                &Circle::new((0.0, 4.0), 1.6),
                WHITE,
                pulse(t, 2.4, 0.0, 0.35, 1.0),
            );
        });
    }
    if pr.sweat {
        let p = phase(t, 3.6, 0.0);
        let ty = keys(
            p,
            &[(0.0, 0.0), (0.3, 0.0), (0.4, 0.0), (0.8, 10.0), (1.0, 0.0)],
            SOFT,
        );
        let sc = keys(
            p,
            &[(0.0, 0.0), (0.3, 0.0), (0.4, 1.0), (0.8, 0.8), (1.0, 1.0)],
            SOFT,
        );
        let op = keys(
            p,
            &[(0.0, 0.0), (0.3, 0.0), (0.4, 0.9), (0.8, 0.0), (1.0, 0.0)],
            SOFT,
        );
        let (x0, y0) = (100.0 - w + 2.0, y + h + 4.0);
        let drop = svg(&format!("M{x0} {y0}q3 5 0 7q-3-2 0-7z"));
        pen.with(
            Affine::translate((0.0, ty)) * about((x0, y0 + 3.5), Affine::scale(sc)),
            |pen| {
                pen.fill(&drop, Rgb(0x9AD4FF), op);
                pen.stroke(&drop, Stroke::new(0.8), WHITE, op);
            },
        );
    }
    if pr.stars {
        for (delay, cx, cy, r, col) in [
            (0.0, 100.0 - w - 12.0, y + 8.6, 4.6, Rgb(0xFFDB7A)),
            (-1.2, 100.0 + w + 16.0, y + 37.6, 3.6, Rgb(0xC7B4FF)),
        ] {
            let p = phase(t, 2.8, delay);
            let sc = keys(p, &[(0.0, 0.4), (0.5, 1.0), (1.0, 0.4)], SOFT);
            let op = keys(p, &[(0.0, 0.2), (0.5, 1.0), (1.0, 0.2)], SOFT);
            let k = r / 4.6;
            let star = svg(&format!(
                "M{cx} {y0}l{a} {b}l{b} {a}l{nb} {a}l{na} {b}l{na} {nb}l{nb} {na}l{b} {na}z",
                y0 = cy - 4.6 * k,
                a = 1.2 * k,
                b = 3.4 * k,
                na = -1.2 * k,
                nb = -3.4 * k
            ));
            pen.with(about((cx, cy), Affine::scale(sc)), |pen| {
                pen.fill(&star, col, op)
            });
        }
        let p = phase(t, 6.0, 0.0);
        let ty = keys(p, &[(0.0, 0.0), (0.5, -4.0), (1.0, 0.0)], SOFT);
        let rot = keys(p, &[(0.0, -6.0), (0.5, 6.0), (1.0, -6.0)], SOFT);
        let (mx, my) = (100.0 - w - 4.0, y - 14.0);
        pen.with(
            Affine::translate((0.0, ty))
                * about((mx - 2.0, my + 6.0), Affine::rotate(rot.to_radians())),
            |pen| {
                pen.fill(
                    &svg(&format!("M{mx} {my}a8 8 0 1 0 9 10a6.4 6.4 0 1 1 -9 -10z")),
                    Rgb(0xFFDB7A),
                    0.9,
                );
            },
        );
    }
    if pr.bolt {
        let op = anim::steps(
            phase(t, 3.0, 0.0),
            &[
                (0.0, 0.0),
                (0.72, 1.0),
                (0.74, 0.3),
                (0.76, 1.0),
                (1.0, 0.0),
            ],
        );
        // The design's steps(1) leaves the bolt lit from 76% to the end of
        // the loop; it reads better as a quick flash, so it fades by 90%.
        let p = phase(t, 3.0, 0.0);
        let op = if p > 0.76 {
            op * keys(p, &[(0.76, 1.0), (0.9, 0.0), (1.0, 0.0)], SOFT)
        } else {
            op
        };
        pen.fill(
            &svg(&format!(
                "M{} {}l-7 11h6l-4 10l10-13h-6l4-8z",
                100.0 + w + 6.0,
                y + 4.0
            )),
            b,
            op,
        );
    }
    if pr.talk_dots {
        for (i, delay) in [0.0, 0.18, 0.36].into_iter().enumerate() {
            let p = phase(t, 1.4, delay);
            let ty = keys(p, &[(0.0, 0.0), (0.3, -5.0), (0.6, 0.0), (1.0, 0.0)], SOFT);
            let op = keys(p, &[(0.0, 0.5), (0.3, 1.0), (0.6, 0.5), (1.0, 0.5)], SOFT);
            pen.fill(
                &Circle::new((90.0 + 10.0 * i as f64, y - 26.0 + ty), 3.2),
                b,
                op,
            );
        }
    }
    if pr.zz {
        for (delay, size) in [(0.0, 12.0), (2.0, 9.0)] {
            let p = phase(t, 4.0, delay);
            let k = keys(p, &[(0.0, 0.0), (1.0, 1.0)], SOFT);
            let op = keys(p, &[(0.0, 0.0), (0.25, 1.0), (1.0, 0.0)], SOFT);
            let (x0, y0) = (100.0 + w - 4.0, y + 2.0);
            let s = size / 12.0;
            let z = svg(&format!(
                "M{} {}h{}l{} {}h{}",
                x0,
                y0 - 8.0 * s,
                6.0 * s,
                -6.0 * s,
                8.0 * s,
                6.0 * s
            ));
            let c = (x0 + 3.0 * s, y0 - 4.0 * s);
            pen.with(
                Affine::translate((12.0 * k, -26.0 * k)) * about(c, Affine::scale(0.6 + 0.5 * k)),
                |pen| {
                    pen.stroke(
                        &z,
                        Stroke::new(2.2 * s)
                            .with_join(Join::Round)
                            .with_caps(Cap::Round),
                        b,
                        op,
                    );
                },
            );
        }
        spark(pen, 46.0, y + 10.0, a, 0.4, 0.7, t);
    }
    if pr.think {
        for (j, delay) in [0.0, -1.1, -2.2].into_iter().enumerate() {
            let p = phase(t, 3.3, delay);
            let dx = keys(p, &[(0.0, 0.0), (1.0, 10.0)], SOFT);
            let dy = keys(p, &[(0.0, 8.0), (1.0, -20.0)], SOFT);
            let sc = keys(p, &[(0.0, 0.4), (1.0, 1.0)], SOFT);
            let op = keys(p, &[(0.0, 0.0), (0.25, 1.0), (0.8, 1.0), (1.0, 0.0)], SOFT);
            let c = (130.0 + j as f64 * 2.0, y - 4.0);
            let r = 4.0 + j as f64 * 1.6;
            pen.with(
                Affine::translate((dx, dy)) * about(c, Affine::scale(sc)),
                |pen| {
                    pen.fade(op, |pen| {
                        pen.fill(&Circle::new(c, r), WHITE, 1.0);
                        pen.stroke_iri(&Circle::new(c, r), 1.4, t);
                    });
                },
            );
        }
    }
    if pr.sparks {
        spark(pen, 36.0, 62.0, b, 0.0, 1.0, t);
        spark(pen, 166.0, 52.0, b, 0.6, 1.0, t);
        spark(pen, 164.0, 140.0, b, 1.2, 1.0, t);
    }
    if pr.done {
        let p = phase(t, 2.4, 0.0);
        let sc = keys(
            p,
            &[
                (0.0, 0.0),
                (0.1, 0.0),
                (0.3, 1.15),
                (0.4, 1.0),
                (0.9, 1.0),
                (1.0, 0.0),
            ],
            SOFT,
        );
        let c = (100.0 + w + 8.0, y + 4.0);
        if sc > 0.01 {
            pen.with(about(c, Affine::scale(sc)), |pen| {
                pen.fill(&Circle::new(c, 12.0), b, 1.0);
                pen.stroke(
                    &poly(
                        &[
                            (c.0 - 5.4, c.1 + 0.4),
                            (c.0 - 1.6, c.1 + 4.2),
                            (c.0 + 5.2, c.1 - 3.2),
                        ],
                        false,
                    ),
                    round(2.8),
                    WHITE,
                    1.0,
                );
            });
        }
        spark(pen, 40.0, 60.0, b, 0.3, 1.0, t);
        spark(pen, 162.0, 142.0, b, 0.9, 1.0, t);
    }
    if pr.ask {
        let p = phase(t, 2.2, 0.0);
        let ty = keys(p, &[(0.0, 0.0), (0.5, -5.0), (1.0, 0.0)], SOFT);
        let rot = keys(p, &[(0.0, -6.0), (0.5, 6.0), (1.0, -6.0)], SOFT);
        let c = (100.0 + w + 10.0, y + 2.0);
        pen.with(
            Affine::translate((0.0, ty)) * about(c, Affine::rotate(rot.to_radians())),
            |pen| {
                pen.fill(&Circle::new(c, 14.0), WHITE, 1.0);
                pen.stroke_iri(&Circle::new(c, 14.0), 2.0, t);
                pen.stroke(
                    &poly(&[(c.0, c.1 - 6.0), (c.0, c.1 + 1.0)], false),
                    round(3.2),
                    Rgb(0xFF6A5C),
                    1.0,
                );
                pen.fill(&Circle::new((c.0, c.1 + 5.4), 1.9), Rgb(0xFF6A5C), 1.0);
            },
        );
    }
}
