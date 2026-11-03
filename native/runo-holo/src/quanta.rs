//! Quanta: Runo's crew as one living crystal.
//!
//! Every bot is the same character, a faceted glass crystal with a face, in
//! its own colour, home form (gem, shard, pyramid, cube, diamond or orb) and
//! kit (hard hat, lens, shield, wrench, book or broom). The forms are real
//! polyhedra: they are turned in 3D, lit from the top left with a warm fill
//! on the right, and drawn back faces first so the glass shows its inner
//! edges. The accessories are crystallised too: each is a polygon split into
//! a fan of lit facets.
//!
//! A port of the "Quanta · 3D character sheet" design. As with the bubbles,
//! a frame is a pure function of the pose and the time, in a 200 × 200 view
//! box, and it is all flat polygons and a few gradients, so it is cheap to
//! draw on the CPU at full frame rate.

use std::f64::consts::PI;
use std::sync::OnceLock;

use vello_cpu::kurbo::{Affine, BezPath, Cap, Circle, Ellipse, Join, Point, Shape, Stroke};
use vello_cpu::peniko::{
    color::{AlphaColor, Srgb},
    ColorStop, ColorStops, Gradient, LinearGradientPosition, RadialGradientPosition,
};
use vello_cpu::{Pixmap, RenderContext, Resources};

use crate::anim::{keys, phase, pulse, SOFT};
use crate::bubble::{body_xf, Eyes, Layout, Mouth, Pose, Sway};
use crate::look::Rgb;
use crate::Framing;

/// The shape a crystal takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Form {
    Gem,
    Shard,
    Pyramid,
    Cube,
    Diamond,
    Orb,
}

impl Form {
    pub const ALL: [Form; 6] = [
        Form::Gem,
        Form::Shard,
        Form::Pyramid,
        Form::Cube,
        Form::Diamond,
        Form::Orb,
    ];
}

/// What a crystal carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kit {
    None,
    HardHat,
    Lens,
    Shield,
    Wrench,
    Book,
    Broom,
}

type Color = AlphaColor<Srgb>;
type V3 = [f64; 3];

const D: f64 = PI / 180.0;

/// Where the face sits on a form, in the view box.
#[derive(Clone, Copy)]
struct Face {
    x: f64,
    y: f64,
    gap: f64,
    s: f64,
}

struct Solid {
    v: Vec<V3>,
    f: Vec<Vec<usize>>,
    yaw: f64,
    pitch: f64,
    face: Face,
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn norm(a: V3) -> V3 {
    let m = dot(a, a).sqrt().max(1e-9);
    [a[0] / m, a[1] / m, a[2] / m]
}
fn rot_x(p: V3, a: f64) -> V3 {
    let (s, c) = a.sin_cos();
    [p[0], p[1] * c - p[2] * s, p[1] * s + p[2] * c]
}
fn rot_y(p: V3, a: f64) -> V3 {
    let (s, c) = a.sin_cos();
    [p[0] * c + p[2] * s, p[1], -p[0] * s + p[2] * c]
}

fn icosahedron() -> (Vec<V3>, Vec<Vec<usize>>) {
    let phi = (1.0 + 5f64.sqrt()) / 2.0;
    let mut v = Vec::new();
    for [a, b, c] in [[0.0, 1.0, phi], [0.0, -1.0, phi], [0.0, 1.0, -phi], [0.0, -1.0, -phi]] {
        v.push([a, b, c]);
        v.push([b, c, a]);
        v.push([c, a, b]);
    }
    let edge = |a: V3, b: V3| (dot(sub(a, b), sub(a, b)) - 4.0).abs() < 1e-6;
    let mut f = Vec::new();
    for i in 0..12 {
        for j in i + 1..12 {
            for k in j + 1..12 {
                if edge(v[i], v[j]) && edge(v[j], v[k]) && edge(v[i], v[k]) {
                    f.push(vec![i, j, k]);
                }
            }
        }
    }
    // A vertex straight up, like the design's gem.
    let v = v.into_iter().map(|p| rot_x(p, -phi.atan())).collect();
    (v, f)
}

fn solids() -> &'static [Solid; 6] {
    static S: OnceLock<[Solid; 6]> = OnceLock::new();
    S.get_or_init(|| {
        let face = |x, y, gap, s| Face { x, y, gap, s };
        let (iv, if_) = icosahedron();
        let gem = Solid {
            v: iv.clone(),
            f: if_.clone(),
            yaw: 16.0,
            pitch: -14.0,
            face: face(100.0, 110.0, 23.0, 1.0),
        };
        let shard = {
            let mut v = Vec::new();
            let mut f = Vec::new();
            for y in [-0.5, 0.55] {
                for i in 0..6 {
                    let a = (i as f64 * 60.0 + 30.0) * D;
                    v.push([0.5 * a.cos(), y, 0.5 * a.sin()]);
                }
            }
            v.push([0.0, -1.08, 0.0]);
            v.push([0.0, 1.12, 0.0]);
            for i in 0..6 {
                let j = (i + 1) % 6;
                f.push(vec![i, j, 6 + j, 6 + i]);
                f.push(vec![12, j, i]);
                f.push(vec![13, 6 + i, 6 + j]);
            }
            Solid {
                v,
                f,
                yaw: 24.0,
                pitch: -12.0,
                face: face(100.0, 104.0, 17.0, 0.82),
            }
        };
        let pyramid = {
            let mut v: Vec<V3> = (0..4)
                .map(|i| {
                    let a = (i as f64 * 90.0 + 45.0) * D;
                    [0.95 * a.cos(), 0.62, 0.95 * a.sin()]
                })
                .collect();
            v.push([0.0, -1.0, 0.0]);
            Solid {
                v,
                f: vec![
                    vec![4, 0, 1],
                    vec![4, 1, 2],
                    vec![4, 2, 3],
                    vec![4, 3, 0],
                    vec![0, 1, 2, 3],
                ],
                yaw: 28.0,
                pitch: -16.0,
                face: face(100.0, 128.0, 20.0, 0.86),
            }
        };
        let cube = {
            let mut v = Vec::new();
            for x in [-1.0, 1.0] {
                for y in [-1.0, 1.0] {
                    for z in [-1.0, 1.0] {
                        v.push([x * 0.6, y * 0.6, z * 0.6]);
                    }
                }
            }
            Solid {
                v,
                f: vec![
                    vec![0, 1, 3, 2],
                    vec![4, 5, 7, 6],
                    vec![0, 1, 5, 4],
                    vec![2, 3, 7, 6],
                    vec![0, 2, 6, 4],
                    vec![1, 3, 7, 5],
                ],
                yaw: 45.0,
                pitch: -30.0,
                face: face(100.0, 128.0, 21.0, 0.92),
            }
        };
        let diamond = {
            let mut v: Vec<V3> = (0..8)
                .map(|i| {
                    let a = i as f64 * 45.0 * D;
                    [0.52 * a.cos(), -0.52, 0.52 * a.sin()]
                })
                .collect();
            for j in 0..16 {
                let a = j as f64 * 22.5 * D;
                v.push([a.cos(), -0.2, a.sin()]);
            }
            v.push([0.0, 1.0, 0.0]);
            let g = |j: usize| 8 + (j % 16);
            let mut f = vec![(0..8).collect::<Vec<_>>()];
            for i in 0..8 {
                let n = (i + 1) % 8;
                f.push(vec![i, n, g(2 * i + 1)]);
                f.push(vec![i, g(2 * i + 1), g(2 * i)]);
                f.push(vec![n, g(2 * i + 2), g(2 * i + 1)]);
            }
            for j in 0..16 {
                f.push(vec![g(j), g(j + 1), 24]);
            }
            Solid {
                v,
                f,
                yaw: 10.0,
                pitch: -26.0,
                face: face(100.0, 98.0, 22.0, 0.92),
            }
        };
        let orb = {
            let mut v: Vec<V3> = Vec::new();
            let mut f = Vec::new();
            let put = |p: V3, v: &mut Vec<V3>| {
                v.push(norm(p));
                v.len() - 1
            };
            let mid = |a: V3, b: V3| [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0, (a[2] + b[2]) / 2.0];
            for t in &if_ {
                let (a, b, c) = (iv[t[0]], iv[t[1]], iv[t[2]]);
                let ia = put(a, &mut v);
                let ib = put(b, &mut v);
                let ic = put(c, &mut v);
                let ab = put(mid(a, b), &mut v);
                let bc = put(mid(b, c), &mut v);
                let ca = put(mid(c, a), &mut v);
                f.push(vec![ia, ab, ca]);
                f.push(vec![ab, ib, bc]);
                f.push(vec![ca, bc, ic]);
                f.push(vec![ab, bc, ca]);
            }
            Solid {
                v,
                f,
                yaw: 12.0,
                pitch: -10.0,
                face: face(100.0, 108.0, 23.0, 1.0),
            }
        };
        let mut all = [gem, shard, pyramid, cube, diamond, orb];
        for s in all.iter_mut() {
            let m = s.v.iter().map(|p| dot(*p, *p).sqrt()).fold(0.0, f64::max);
            for p in s.v.iter_mut() {
                *p = [p[0] / m, p[1] / m, p[2] / m];
            }
        }
        all
    })
}

fn solid(form: Form) -> &'static Solid {
    let i = Form::ALL.iter().position(|f| *f == form).unwrap_or(0);
    &solids()[i]
}

/// `hsl(h s% l%)` as a colour.
pub fn hsl(h: f64, s: f64, l: f64) -> Rgb {
    let h = h.rem_euclid(360.0) / 360.0;
    let s = (s / 100.0).clamp(0.0, 1.0);
    let l = (l / 100.0).clamp(0.0, 1.0);
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let ch = |mut t: f64| {
        t = t.rem_euclid(1.0);
        let v = if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        };
        (v * 255.0).round().clamp(0.0, 255.0) as u32
    };
    Rgb((ch(h + 1.0 / 3.0) << 16) | (ch(h) << 8) | ch(h - 1.0 / 3.0))
}

/// The hue of a colour, in degrees.
pub fn hue_of(c: Rgb) -> f64 {
    let (r, g, b) = (c.r() as f64 / 255.0, c.g() as f64 / 255.0, c.b() as f64 / 255.0);
    let mx = r.max(g).max(b);
    let mn = r.min(g).min(b);
    let d = mx - mn;
    if d < 1e-6 {
        return 0.0;
    }
    let h = if mx == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if mx == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    h * 60.0
}

fn color(c: Rgb, a: f64) -> Color {
    Color::from_rgba8(c.r(), c.g(), c.b(), (a.clamp(0.0, 1.0) * 255.0).round() as u8)
}

fn stops(s: &[(f32, Rgb, f64)]) -> ColorStops {
    let v: Vec<ColorStop> = s
        .iter()
        .map(|&(o, c, a)| ColorStop::from((o, color(c, a))))
        .collect();
    ColorStops::from(v.as_slice())
}

fn linear(p0: (f64, f64), p1: (f64, f64), s: &[(f32, Rgb, f64)]) -> Gradient {
    Gradient {
        kind: LinearGradientPosition {
            start: Point::new(p0.0, p0.1),
            end: Point::new(p1.0, p1.1),
        }
        .into(),
        stops: stops(s),
        ..Default::default()
    }
}

/// A radial gradient on the unit circle; place it with a paint transform.
fn radial(s: &[(f32, Rgb, f64)]) -> Gradient {
    Gradient {
        kind: RadialGradientPosition {
            start_center: Point::new(0.0, 0.0),
            start_radius: 0.0,
            end_center: Point::new(0.0, 0.0),
            end_radius: 1.0,
        }
        .into(),
        stops: stops(s),
        ..Default::default()
    }
}

fn poly_path(pts: &[(f64, f64)], close: bool) -> BezPath {
    let mut p = BezPath::new();
    for (i, q) in pts.iter().enumerate() {
        if i == 0 {
            p.move_to(*q);
        } else {
            p.line_to(*q);
        }
    }
    if close {
        p.close_path();
    }
    p
}

fn round(w: f64) -> Stroke {
    Stroke::new(w).with_caps(Cap::Round).with_join(Join::Round)
}

struct Ink<'a> {
    ctx: &'a mut RenderContext,
    view: Affine,
    /// The bot's own motion, in the view box.
    at: Affine,
}

impl Ink<'_> {
    fn begin(&mut self, extra: Affine) {
        self.ctx.set_transform(self.view * self.at * extra);
        self.ctx.reset_paint_transform();
    }
    fn fill(&mut self, p: &BezPath, c: Rgb, a: f64) {
        if a <= 0.004 {
            return;
        }
        self.begin(Affine::IDENTITY);
        self.ctx.set_paint(color(c, a));
        self.ctx.fill_path(p);
    }
    fn fill_at(&mut self, p: &BezPath, xf: Affine, c: Rgb, a: f64) {
        if a <= 0.004 {
            return;
        }
        self.begin(xf);
        self.ctx.set_paint(color(c, a));
        self.ctx.fill_path(p);
    }
    fn stroke(&mut self, p: &BezPath, w: f64, c: Rgb, a: f64) {
        if a <= 0.004 {
            return;
        }
        self.begin(Affine::IDENTITY);
        self.ctx.set_stroke(round(w));
        self.ctx.set_paint(color(c, a));
        self.ctx.stroke_path(p);
    }
    fn grad(&mut self, p: &BezPath, g: Gradient, paint: Affine) {
        self.begin(Affine::IDENTITY);
        self.ctx.set_paint_transform(paint);
        self.ctx.set_paint(g);
        self.ctx.fill_path(p);
        self.ctx.reset_paint_transform();
    }
    fn grad_stroke(&mut self, p: &BezPath, w: f64, g: Gradient) {
        self.begin(Affine::IDENTITY);
        self.ctx.set_stroke(round(w));
        self.ctx.set_paint(g);
        self.ctx.stroke_path(p);
    }
    /// A soft round glow: `c` at its centre, fading to nothing at `rx`, `ry`.
    fn glow(&mut self, cx: f64, cy: f64, rx: f64, ry: f64, c: Rgb, a: f64) {
        if a <= 0.004 {
            return;
        }
        let e = Ellipse::new((cx, cy), (rx, ry), 0.0).to_path(0.1);
        let g = radial(&[(0.0, c, a), (0.45, c, a * 0.45), (1.0, c, 0.0)]);
        self.grad(&e, g, Affine::translate((cx, cy)) * Affine::scale_non_uniform(rx, ry));
    }
}

/// A polygon split into a fan of lit facets: the crystallised look of every prop.
fn gem_poly(ink: &mut Ink, pts: &[(f64, f64)], h: f64, sat: f64, dl: f64, glint: bool) {
    let n = pts.len() as f64;
    let (mut cx, mut cy) = (0.0, 0.0);
    let (mut x0, mut x1, mut y0, mut y1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for &(x, y) in pts {
        cx += x / n;
        cy += y / n;
        x0 = x0.min(x);
        x1 = x1.max(x);
        y0 = y0.min(y);
        y1 = y1.max(y);
    }
    let c = (cx - (x1 - x0) * 0.14, cy - (y1 - y0) * 0.16);
    for i in 0..pts.len() {
        let a = pts[i];
        let b = pts[(i + 1) % pts.len()];
        let (mx, my) = ((a.0 + b.0) / 2.0 - c.0, (a.1 + b.1) / 2.0 - c.1);
        let m = mx.hypot(my).max(1e-6);
        let d = (-0.6 * mx - 0.8 * my) / m;
        let l = (28.0 + 52.0 * (0.5 + 0.45 * d) + dl).clamp(10.0, 94.0);
        let tri = poly_path(&[c, a, b], true);
        ink.fill(&tri, hsl(h + 14.0 * mx / m, sat, l), 1.0);
        ink.stroke(&tri, 0.7, Rgb(0xFFFFFF), 0.4);
    }
    let out = poly_path(pts, true);
    ink.stroke(&out, 1.3, hsl(h, (sat + 10.0).min(100.0), (86.0 + dl * 0.2).min(94.0)), 1.0);
    if glint {
        let tl = pts
            .iter()
            .copied()
            .fold(pts[0], |p, q| if q.0 + q.1 < p.0 + p.1 { q } else { p });
        ink.fill(&Circle::new((tl.0 + 2.2, tl.1 + 2.2), 1.6).to_path(0.1), Rgb(0xFFFFFF), 1.0);
    }
}

/// A bevelled band along `pts`: headphone bands, handles and rings.
fn strip(ink: &mut Ink, pts: &[(f64, f64)], w: f64, h: f64, sat: f64, dl: f64) {
    for i in 0..pts.len().saturating_sub(1) {
        let (p, q) = (pts[i], pts[i + 1]);
        let (dx, dy) = (q.0 - p.0, q.1 - p.1);
        let m = dx.hypot(dy).max(1e-6);
        let (nx, ny) = (-dy / m * w / 2.0, dx / m * w / 2.0);
        for sg in [1.0, -1.0] {
            let d = sg * (-0.6 * nx - 0.8 * ny) / (w / 2.0);
            let l = (30.0 + 50.0 * (0.5 + 0.45 * d) + dl + if i % 2 == 1 { 5.0 } else { 0.0 })
                .clamp(10.0, 92.0);
            let quad = poly_path(
                &[p, q, (q.0 + sg * nx, q.1 + sg * ny), (p.0 + sg * nx, p.1 + sg * ny)],
                true,
            );
            let c = hsl(h, sat, l);
            ink.fill(&quad, c, 1.0);
            ink.stroke(&quad, 0.5, c, 1.0);
        }
    }
    ink.stroke(&poly_path(pts, false), 0.8, Rgb(0xFFFFFF), 0.6);
}

fn ngon(cx: f64, cy: f64, rx: f64, ry: f64, n: usize, rot: f64) -> Vec<(f64, f64)> {
    (0..n)
        .map(|i| {
            let a = (rot + i as f64 * 360.0 / n as f64 - 90.0) * D;
            (cx + rx * a.cos(), cy + ry * a.sin())
        })
        .collect()
}

fn arc_pts(cx: f64, cy: f64, rx: f64, ry: f64, a0: f64, a1: f64, n: usize) -> Vec<(f64, f64)> {
    (0..=n)
        .map(|i| {
            let a = (a0 + (a1 - a0) * i as f64 / n as f64) * D;
            (cx + rx * a.cos(), cy + ry * a.sin())
        })
        .collect()
}

fn star_pts(cx: f64, cy: f64, big: f64, small: f64, n: usize) -> Vec<(f64, f64)> {
    (0..n * 2)
        .map(|i| {
            let a = (-90.0 + i as f64 * 180.0 / n as f64) * D;
            let r = if i % 2 == 1 { small } else { big };
            (cx + r * a.cos(), cy + r * a.sin())
        })
        .collect()
}

/// A four-point sparkle, twinkling.
fn sparkle(ink: &mut Ink, x: f64, y: f64, s: f64, c: Rgb, k: f64) {
    if k <= 0.02 {
        return;
    }
    let p = poly_path(&star_pts(x, y, s * k, s * 0.28 * k, 4), true);
    ink.fill(&p, c, k.min(1.0));
}

/// How the crystal turns for this pose at time `t`: (yaw, pitch) in degrees.
fn turn(pose: &Pose, s: &Solid, t: f64) -> (f64, f64) {
    let yaw = match pose.layout {
        Layout::Busy => s.yaw + t * 90.0,
        Layout::Think => s.yaw + 22.0 * (t * 0.9).sin(),
        _ => s.yaw + 10.0 * (t * 0.7).sin(),
    };
    (yaw, s.pitch + 3.0 * (t * 0.5).sin())
}

struct Body {
    hull: Vec<(f64, f64)>,
    top: (f64, f64),
    left: (f64, f64),
    bottom: f64,
}

fn hull2(mut p: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    p.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    let cr = |o: (f64, f64), a: (f64, f64), b: (f64, f64)| {
        (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0)
    };
    let mut lo: Vec<(f64, f64)> = Vec::new();
    for &q in &p {
        while lo.len() > 1 && cr(lo[lo.len() - 2], lo[lo.len() - 1], q) <= 0.0 {
            lo.pop();
        }
        lo.push(q);
    }
    let mut up: Vec<(f64, f64)> = Vec::new();
    for &q in p.iter().rev() {
        while up.len() > 1 && cr(up[up.len() - 2], up[up.len() - 1], q) <= 0.0 {
            up.pop();
        }
        up.push(q);
    }
    lo.pop();
    up.pop();
    lo.extend(up);
    lo
}

/// The glass body: glow, back faces, core light, lit front faces, sheen and rim.
fn crystal(ink: &mut Ink, form: Form, hue: f64, t: f64, pose: &Pose, small: bool) -> Body {
    let s = solid(form);
    let (yaw, pitch) = turn(pose, s, t);
    let k = 88.0;
    let r: Vec<V3> = s
        .v
        .iter()
        .map(|p| rot_x(rot_y(*p, yaw * D), pitch * D))
        .collect();
    let p2: Vec<(f64, f64)> = r.iter().map(|p| (100.0 + k * p[0], 100.0 + k * p[1])).collect();
    struct F {
        n: V3,
        z: f64,
        cx: f64,
        path: BezPath,
    }
    let mut back = Vec::new();
    let mut front = Vec::new();
    for ix in &s.f {
        let q: Vec<V3> = ix.iter().map(|&i| r[i]).collect();
        let mut n = norm(cross(sub(q[1], q[0]), sub(q[2], q[0])));
        let len = q.len() as f64;
        let c = q.iter().fold([0.0; 3], |a, b| {
            [a[0] + b[0] / len, a[1] + b[1] / len, a[2] + b[2] / len]
        });
        if dot(n, c) < 0.0 {
            n = [-n[0], -n[1], -n[2]];
        }
        let pts: Vec<(f64, f64)> = ix.iter().map(|&i| p2[i]).collect();
        let f = F {
            n,
            z: c[2],
            cx: 100.0 + k * c[0],
            path: poly_path(&pts, true),
        };
        if n[2] > 0.02 {
            front.push(f);
        } else {
            back.push(f);
        }
    }
    back.sort_by(|a, b| a.z.total_cmp(&b.z));
    front.sort_by(|a, b| a.z.total_cmp(&b.z));
    let hull = hull2(p2.clone());
    let hp = poly_path(&hull, true);
    let bottom = hull.iter().map(|p| p.1).fold(f64::MIN, f64::max);
    let top = hull
        .iter()
        .copied()
        .fold(hull[0], |a, b| if b.1 < a.1 { b } else { a });
    let left = hull
        .iter()
        .copied()
        .fold(hull[0], |a, b| if b.0 < a.0 { b } else { a });
    let white = Rgb(0xFFFFFF);
    // Bloom behind the glass.
    let glow = pulse(t, 4.4, 0.0, 0.5, 0.8) * if pose.shine { 1.25 } else { 1.0 };
    ink.glow(100.0, 104.0, 112.0, 108.0, hsl(hue + 6.0, 100.0, 64.0), 0.42 * glow);
    // Inside the glass.
    ink.ctx.set_transform(ink.view * ink.at);
    ink.ctx.push_clip_layer(&hp);
    ink.fill(&hp, hsl(hue, 70.0, 30.0), 1.0);
    if !small {
        for f in &back {
            let h = hue - 38.0 * f.n[0];
            ink.fill(&f.path, hsl(h, 90.0, 58.0), 0.28);
            ink.stroke(&f.path, 0.8, white, 0.22);
        }
    }
    let core = pulse(t, 3.2, 0.0, 0.75, 1.0);
    let cg = radial(&[
        (0.0, white, 0.6 * core),
        (0.5, hsl(hue + 30.0, 100.0, 86.0), 0.16 * core),
        (1.0, white, 0.0),
    ]);
    let ce = Ellipse::new((100.0, 112.0), (60.0, 56.0), 0.0).to_path(0.1);
    ink.grad(&ce, cg, Affine::translate((100.0, 112.0)) * Affine::scale_non_uniform(60.0, 56.0));
    let lk = norm([-0.55, -0.7, 0.5]);
    let lf = norm([0.85, 0.05, 0.45]);
    let hv = norm([lk[0], lk[1], lk[2] + 1.0]);
    // A band of light that runs across the facets every few seconds.
    let sweep = keys(phase(t, 5.0, 0.0), &[(0.0, -60.0), (0.55, -60.0), (1.0, 260.0)], SOFT);
    for f in &front {
        let n = f.n;
        let b = 0.16 + 0.62 * dot(n, lk).max(0.0) + 0.3 * dot(n, lf).max(0.0);
        let sp = dot(n, hv).max(0.0).powi(18);
        let h = hue + 38.0 * n[0] - 8.0 * n[1];
        let l = (44.0 + 40.0 * b + 24.0 * sp).min(93.0);
        let sat = (62.0 + (1.0 - b.min(1.0)) * 24.0).min(100.0);
        ink.fill(&f.path, hsl(h, sat, l), 0.8);
        ink.stroke(&f.path, if small { 0.6 } else { 1.0 }, white, 0.32 + 0.4 * b.min(1.0));
        if !small {
            let lit = (1.0 - (f.cx - sweep).abs() / 34.0).max(0.0) * 0.32;
            ink.fill(&f.path, white, lit);
        }
    }
    let sheen = linear(
        (0.0, 0.0),
        (160.0, 200.0),
        &[
            (0.0, white, 0.28),
            (0.35, white, 0.0),
            (0.7, hsl(hue, 80.0, 15.0), 0.0),
            (1.0, hsl(hue, 80.0, 12.0), 0.4),
        ],
    );
    ink.grad(&hp, sheen, Affine::IDENTITY);
    if pose.shine && !small {
        let x = keys(phase(t, 2.4, 0.0), &[(0.0, -60.0), (0.55, 260.0), (1.0, 260.0)], SOFT);
        let band = poly_path(&[(x, -20.0), (x + 30.0, -20.0), (x - 54.0, 240.0), (x - 84.0, 240.0)], true);
        let g = linear(
            (x - 60.0, 0.0),
            (x, 0.0),
            &[(0.0, white, 0.0), (0.5, white, 0.5), (1.0, white, 0.0)],
        );
        ink.grad(&band, g, Affine::IDENTITY);
    }
    ink.ctx.set_transform(ink.view * ink.at);
    ink.ctx.pop_layer();
    let rim = linear(
        (20.0, 10.0),
        (180.0, 190.0),
        &[
            (0.0, white, 1.0),
            (0.5, hsl(hue - 30.0, 100.0, 84.0), 0.75),
            (1.0, hsl(hue + 40.0, 100.0, 80.0), 1.0),
        ],
    );
    ink.grad_stroke(&hp, if small { 1.6 } else { 2.2 }, rim);
    Body {
        hull,
        top,
        left,
        bottom,
    }
}

fn arc(ink: &mut Ink, x: f64, ey: f64, k: f64, up: bool, ink_c: Rgb) {
    let mut p = BezPath::new();
    let (a, b) = if up { (4.0, -10.0) } else { (-2.0, 9.0) };
    p.move_to((x - 10.0 * k, ey + a * k));
    p.quad_to((x, ey + b * k), (x + 10.0 * k, ey + a * k));
    ink.stroke(&p, 5.0 * k, ink_c, 1.0);
}

/// Eyes, mouth and cheeks.
fn face(ink: &mut Ink, pose: &Pose, fc: Face, hue: f64, t: f64, small: bool) {
    let k = fc.s;
    let ey = fc.y;
    let (el, er) = (fc.x - fc.gap, fc.x + fc.gap);
    let ink_c = hsl(hue + 10.0, 70.0, 9.0);
    let white = Rgb(0xFFFFFF);
    // Where the eyes look.
    let (lx, ly) = match pose.eyes {
        Eyes::Up => (2.0 + 2.0 * pulse(t, 4.0, 0.0, 0.0, 1.0), -3.4),
        Eyes::Read => (keys(phase(t, 1.6, 0.0), &[(0.0, -3.0), (0.5, 3.0), (1.0, -3.0)], SOFT), 1.4),
        Eyes::You => (0.0, 0.6),
        _ => {
            let p = phase(t, 8.0, 0.0);
            (
                keys(p, &[(0.0, 0.0), (0.14, 0.0), (0.2, 3.0), (0.38, 3.0), (0.44, -3.0), (0.6, -3.0), (0.66, 0.5), (0.82, 0.5), (0.88, 0.0), (1.0, 0.0)], SOFT),
                keys(p, &[(0.0, 0.0), (0.14, 0.0), (0.2, -0.8), (0.38, -0.8), (0.44, 0.4), (0.6, 0.4), (0.66, 1.4), (0.82, 1.4), (0.88, 0.0), (1.0, 0.0)], SOFT),
            )
        }
    };
    let blink = keys(phase(t, 5.2, 0.0), &[(0.0, 1.0), (0.44, 1.0), (0.47, 0.08), (0.49, 0.08), (0.52, 1.0), (1.0, 1.0)], SOFT);
    let round_eye = |ink: &mut Ink, x: f64, big: f64, lid: bool| {
        let (cx, cy) = (x + lx, ey + ly);
        let at = Affine::translate((cx, cy)) * Affine::scale_non_uniform(1.0, blink) * Affine::translate((-cx, -cy));
        let (rx, ry) = (12.5 * k * big, 15.5 * k * big);
        let iris = linear(
            (0.0, cy - ry),
            (0.0, cy + ry),
            &[
                (0.0, hsl(hue, 60.0, 6.0), 1.0),
                (0.65, hsl(hue, 55.0, 14.0), 1.0),
                (1.0, hsl(hue + 10.0, 60.0, 32.0), 1.0),
            ],
        );
        let e = Ellipse::new((cx, cy), (rx, ry), 0.0).to_path(0.1);
        ink.begin(at);
        ink.ctx.set_paint(iris);
        ink.ctx.fill_path(&e);
        let refl = Ellipse::new((cx, cy + ry * 0.5), (rx * 0.64, 4.0 * k), 0.0).to_path(0.1);
        ink.fill_at(&refl, at, hsl(hue + 20.0, 100.0, 72.0), 0.35);
        let cl = Ellipse::new((cx - 4.0 * k, cy - 6.0 * k), (5.2 * k, 6.0 * k), -20.0 * D).to_path(0.1);
        ink.fill_at(&cl, at, white, 1.0);
        let c2 = Circle::new((cx + 4.5 * k, cy + 5.0 * k), 2.0 * k).to_path(0.1);
        ink.fill_at(&c2, at, white, 0.9);
        if lid {
            // Determined brows over the top of the eye.
            let mut p = BezPath::new();
            let dir = if x < fc.x { 1.0 } else { -1.0 };
            p.move_to((x - dir * 13.0 * k, ey - 12.0 * k));
            p.line_to((x + dir * 11.0 * k, ey - 8.5 * k));
            ink.stroke(&p, 4.4 * k, ink_c, 1.0);
        }
    };
    match pose.eyes {
        Eyes::Smile | Eyes::Happy => {
            arc(ink, el, ey, k, true, ink_c);
            arc(ink, er, ey, k, true, ink_c);
        }
        Eyes::Sleepy => {
            arc(ink, el, ey, k, false, ink_c);
            arc(ink, er, ey, k, false, ink_c);
        }
        Eyes::Visor | Eyes::Squint => {
            round_eye(ink, el, 0.92, true);
            round_eye(ink, er, 0.92, true);
        }
        Eyes::You => {
            round_eye(ink, el, 1.1, false);
            round_eye(ink, er, 1.1, false);
        }
        _ => {
            round_eye(ink, el, 1.0, false);
            round_eye(ink, er, 1.0, false);
        }
    }
    let (mx, my) = (fc.x, ey + 19.0 * k);
    match pose.mouth {
        Mouth::None => {}
        Mouth::O => {
            let o = Ellipse::new((mx + 1.0, my + 2.0), (4.2 * k, 5.0 * k), 0.0).to_path(0.1);
            ink.fill(&o, ink_c, 1.0);
        }
        Mouth::Flat => {
            let p = poly_path(&[(mx - 6.0 * k, my + 2.0), (mx + 6.0 * k, my + 2.0)], false);
            ink.stroke(&p, 3.8 * k, ink_c, 1.0);
        }
        Mouth::Small => {
            let mut p = BezPath::new();
            p.move_to((mx - 5.0 * k, my + 1.0));
            p.quad_to((mx, my + 4.0 * k), (mx + 5.0 * k, my + 1.0));
            ink.stroke(&p, 3.4 * k, ink_c, 1.0);
        }
        Mouth::Grin | Mouth::Open => {
            let mut p = BezPath::new();
            p.move_to((mx - 10.0 * k, my - 1.0));
            p.quad_to((mx, my + 15.0 * k), (mx + 10.0 * k, my - 1.0));
            p.quad_to((mx, my + 2.0 * k), (mx - 10.0 * k, my - 1.0));
            p.close_path();
            ink.fill(&p, ink_c, 1.0);
            let mut tg = BezPath::new();
            tg.move_to((mx - 5.0 * k, my + 6.0 * k));
            tg.quad_to((mx, my + 3.0 * k), (mx + 5.0 * k, my + 6.0 * k));
            tg.quad_to((mx, my + 10.0 * k), (mx - 5.0 * k, my + 6.0 * k));
            tg.close_path();
            ink.fill(&tg, hsl(345.0, 90.0, 66.0), 1.0);
        }
    }
    if !small {
        let blush = hsl(335.0, 100.0, 74.0);
        for x in [el - 17.0 * k, er + 17.0 * k] {
            ink.glow(x, my + 1.0, 11.0 * k, 6.0 * k, blush, 0.6);
        }
    }
}

/// The bot's kit, in the view box. `dy` moves head props to the form's top.
fn kit(ink: &mut Ink, kit: Kit, t: f64, dy: f64) {
    match kit {
        Kit::None => {}
        Kit::HardHat => {
            let o = dy - 8.0;
            let mut dome = arc_pts(100.0, 58.0 + o, 62.0, 56.0, 180.0, 360.0, 10);
            dome.push((162.0, 58.0 + o));
            dome.push((38.0, 58.0 + o));
            gem_poly(ink, &dome, 40.0, 88.0, 2.0, true);
            gem_poly(ink, &[(24.0, 56.0 + o), (176.0, 56.0 + o), (182.0, 70.0 + o), (18.0, 70.0 + o)], 36.0, 88.0, -8.0, true);
            let lamp = pulse(t, 1.6, 0.0, 14.0, 30.0);
            gem_poly(ink, &ngon(100.0, 28.0 + o, 9.0, 9.0, 6, 0.0), 55.0, 88.0, lamp, false);
        }
        Kit::Lens => {
            let (ox, oy) = (
                pulse(t, 3.2, 0.0, 0.0, -6.0),
                pulse(t, 3.2, 0.0, 0.0, -4.0),
            );
            let c = (152.0 + ox, 150.0 + oy);
            ink.fill(&Circle::new(c, 20.0).to_path(0.1), hsl(200.0, 100.0, 87.0), 0.22);
            let mut ring = ngon(c.0, c.1, 20.0, 20.0, 12, 0.0);
            ring.push(ring[0]);
            strip(ink, &ring, 6.0, 215.0, 30.0, -12.0);
            strip(ink, &[(c.0 + 15.0, c.1 + 15.0), (c.0 + 42.0, c.1 + 42.0)], 10.0, 262.0, 85.0, -24.0);
        }
        Kit::Shield => {
            gem_poly(
                ink,
                &[(162.0, 110.0), (198.0, 122.0), (198.0, 150.0), (184.0, 180.0), (162.0, 196.0), (140.0, 180.0), (126.0, 150.0), (126.0, 122.0)],
                218.0,
                88.0,
                -2.0,
                true,
            );
            gem_poly(ink, &star_pts(162.0, 152.0, 15.0, 6.5, 5), 48.0, 88.0, 16.0, false);
        }
        Kit::Wrench => {
            let a = keys(phase(t, 2.4, 0.0), &[(0.0, 0.0), (0.3, -16.0), (0.45, 6.0), (0.6, -10.0), (0.75, 0.0), (1.0, 0.0)], SOFT);
            let old = ink.at;
            ink.at = old * Affine::translate((140.0, 192.0)) * Affine::rotate(a * D) * Affine::translate((-140.0, -192.0));
            strip(ink, &[(138.0, 194.0), (170.0, 162.0)], 12.0, 230.0, 18.0, 18.0);
            gem_poly(ink, &ngon(177.0, 155.0, 17.0, 17.0, 8, 0.0), 230.0, 18.0, 20.0, true);
            ink.fill(&Circle::new((185.0, 147.0), 7.0).to_path(0.1), Rgb(0x141026), 1.0);
            ink.at = old;
        }
        Kit::Book => {
            gem_poly(ink, &[(118.0, 170.0), (156.0, 156.0), (156.0, 192.0), (118.0, 206.0)], 214.0, 88.0, -4.0, true);
            gem_poly(ink, &[(156.0, 156.0), (194.0, 170.0), (194.0, 206.0), (156.0, 192.0)], 214.0, 88.0, 4.0, false);
            let mut p = BezPath::new();
            for (a, b) in [((126.0, 178.0), (148.0, 170.0)), ((126.0, 186.0), (148.0, 178.0)), ((164.0, 170.0), (186.0, 178.0)), ((164.0, 178.0), (186.0, 186.0))] {
                p.move_to(a);
                p.line_to(b);
            }
            ink.stroke(&p, 2.0, Rgb(0xFFFFFF), 0.7);
        }
        Kit::Broom => {
            let a = pulse(t, 2.6, 0.0, -10.0, 12.0);
            let old = ink.at;
            ink.at = old * Affine::translate((196.0, 56.0)) * Affine::rotate(a * D) * Affine::translate((-196.0, -56.0));
            strip(ink, &[(196.0, 56.0), (160.0, 162.0)], 7.0, 28.0, 70.0, -6.0);
            gem_poly(ink, &[(138.0, 156.0), (182.0, 164.0), (178.0, 200.0), (126.0, 190.0)], 44.0, 88.0, 4.0, true);
            ink.at = old;
        }
    }
}

/// A bubble over the bot's shoulder: `!` when it needs you, `?` when it thinks.
fn badge(ink: &mut Ink, t: f64, h: f64, mark: char) {
    let a = keys(phase(t, 2.2, 0.0), &[(0.0, -8.0), (0.5, 8.0), (1.0, -8.0)], SOFT);
    let y = keys(phase(t, 2.2, 0.0), &[(0.0, 0.0), (0.5, -5.0), (1.0, 0.0)], SOFT);
    let old = ink.at;
    ink.at = old * Affine::translate((178.0, 24.0 + y)) * Affine::rotate(a * D) * Affine::translate((-178.0, -24.0));
    gem_poly(ink, &ngon(178.0, 24.0, 21.0, 21.0, 10, 0.0), h, 88.0, 4.0, true);
    let white = Rgb(0xFFFFFF);
    if mark == '!' {
        ink.stroke(&poly_path(&[(178.0, 13.0), (178.0, 26.0)], false), 4.6, white, 1.0);
        ink.fill(&Circle::new((178.0, 34.0), 2.8).to_path(0.1), white, 1.0);
    } else {
        let mut p = BezPath::new();
        p.move_to((170.0, 18.0));
        p.quad_to((172.0, 10.0), (179.0, 10.0));
        p.quad_to((187.0, 11.0), (186.0, 18.0));
        p.quad_to((185.0, 23.0), (178.0, 25.0));
        p.line_to((178.0, 28.0));
        ink.stroke(&p, 4.2, white, 1.0);
        ink.fill(&Circle::new((178.0, 35.0), 2.6).to_path(0.1), white, 1.0);
    }
    ink.at = old;
}

fn check_badge(ink: &mut Ink, t: f64) {
    let p = phase(t, 2.6, 0.0);
    let s = keys(p, &[(0.0, 0.0), (0.1, 0.0), (0.24, 1.2), (0.32, 1.0), (0.92, 1.0), (1.0, 0.0)], SOFT);
    if s <= 0.02 {
        return;
    }
    let old = ink.at;
    ink.at = old * Affine::translate((186.0, 22.0)) * Affine::scale(s) * Affine::translate((-186.0, -22.0));
    gem_poly(ink, &ngon(186.0, 22.0, 17.0, 17.0, 10, 0.0), 148.0, 88.0, -2.0, true);
    ink.stroke(&poly_path(&[(178.0, 22.5), (183.5, 28.0), (193.5, 17.0)], false), 3.6, Rgb(0xFFFFFF), 1.0);
    ink.at = old;
}

impl Pose {
    /// The crystal's colour.
    pub fn hue(&self) -> f64 {
        hue_of(self.tint.strong())
    }
}

/// The part of the view box a framing shows: (x0, y0, side).
fn view(framing: Framing) -> (f64, f64, f64) {
    match framing {
        Framing::Full => (-14.0, -18.0, 228.0),
        Framing::Bust => (0.0, -2.0, 200.0),
    }
}

/// Draws Quanta crystals into premultiplied RGBA8, reusing buffers between frames.
pub struct QuantaRenderer {
    px: u16,
    ctx: RenderContext,
    resources: Resources,
    pix: Pixmap,
}

impl QuantaRenderer {
    pub fn new(px: u16) -> Self {
        QuantaRenderer {
            px,
            ctx: RenderContext::new(px, px),
            resources: Resources::new(),
            pix: Pixmap::new(px, px),
        }
    }

    pub fn size(&self) -> u16 {
        self.px
    }

    /// Draws `pose` at time `t`; premultiplied RGBA8, `size()` pixels square.
    pub fn draw(&mut self, pose: &Pose, t: f64, framing: Framing) -> &[u8] {
        let n = self.px as f64;
        let (x0, y0, side) = view(framing);
        let view_xf = Affine::scale(n / side) * Affine::translate((-x0, -y0));
        let small = self.px < 48;
        let tiny = self.px < 28;
        let hue = pose.hue();
        let body = body_xf(pose.sway, t);
        self.ctx.reset();
        {
            let mut ink = Ink {
                ctx: &mut self.ctx,
                view: view_xf,
                at: Affine::IDENTITY,
            };
            // The light pool under the crystal stays on the floor while it floats.
            let lift = body.as_coeffs()[5];
            let fl = (1.0 + lift / 40.0).clamp(0.6, 1.0);
            if framing == Framing::Full && !tiny {
                ink.glow(100.0, 206.0, 80.0 * fl, 12.0 * fl, hsl(hue + 6.0, 100.0, 66.0), 0.9 * fl);
                ink.glow(100.0, 206.0, 46.0 * fl, 4.0, Rgb(0xFFFFFF), 0.3);
            }
            ink.at = body;
            let b = crystal(&mut ink, pose.form, hue, t, pose, small);
            let fc = solid(pose.form).face;
            face(&mut ink, pose, fc, hue, t, small);
            if !tiny {
                let tw = |d: f64| pulse(t, 2.6, d, 0.5, 1.15);
                sparkle(&mut ink, b.top.0, b.top.1, 9.0, Rgb(0xFFFFFF), tw(0.0));
                sparkle(&mut ink, b.left.0 + 3.0, b.left.1, 6.0, Rgb(0xFFFFFF), tw(1.3));
            }
            if !small {
                kit(&mut ink, pose.kit, t, b.top.1 - 14.0);
            }
            if pose.sparks && !small {
                let sc = hsl(48.0, 100.0, 70.0);
                for (i, (x, y, s)) in [(-6.0, 60.0, 8.0), (206.0, 46.0, 7.0), (204.0, 150.0, 6.0), (-2.0, 150.0, 5.0)]
                    .into_iter()
                    .enumerate()
                {
                    let k = keys(phase(t, 1.6, i as f64 * 0.4), &[(0.0, 0.0), (0.5, 1.0), (1.0, 0.0)], SOFT);
                    sparkle(&mut ink, x, y, s, if i % 2 == 0 { sc } else { Rgb(0xFFFFFF) }, k);
                }
            }
            if pose.layout == Layout::Splash && !small {
                // Crystal confetti rising off the top.
                for (i, (x, h)) in [(10.0, 326.0), (60.0, 48.0), (140.0, 186.0), (190.0, 285.0)].into_iter().enumerate() {
                    let p = phase(t, 2.2, i as f64 * 0.55);
                    let y = 30.0 - 50.0 * p;
                    let a = (1.0 - p) * (p * 4.0).min(1.0);
                    if a > 0.05 {
                        let pts = [(x, y), (x + 9.0, y + 3.0), (x + 2.0, y + 10.0)];
                        let old = ink.at;
                        ink.at = old * Affine::translate((x + 3.0, y + 4.0)) * Affine::rotate(p * 2.8) * Affine::translate((-x - 3.0, -y - 4.0));
                        gem_poly(&mut ink, &pts, h, 88.0, 10.0 * a, false);
                        ink.at = old;
                    }
                }
            }
            if pose.tick && !small {
                check_badge(&mut ink, t);
            }
            if pose.ask && !small {
                badge(&mut ink, t, 18.0, '!');
            } else if pose.layout == Layout::Think && !small {
                badge(&mut ink, t, 285.0, '?');
            }
            let _ = b.hull.len() + b.bottom as usize;
        }
        self.ctx.flush();
        self.pix.data_as_u8_slice_mut().fill(0);
        self.ctx.render(&mut self.pix, &mut self.resources);
        self.pix.data_as_u8_slice()
    }
}

/// A bot's home form and kit, from its signature eyes.
pub fn home(eyes: Eyes) -> (Form, Kit) {
    match eyes {
        Eyes::Dots => (Form::Cube, Kit::HardHat),
        Eyes::Lens => (Form::Shard, Kit::Lens),
        Eyes::Visor => (Form::Gem, Kit::Shield),
        Eyes::Squint => (Form::Pyramid, Kit::Wrench),
        Eyes::Smile => (Form::Orb, Kit::Book),
        Eyes::Sleepy => (Form::Diamond, Kit::Broom),
        _ => (Form::Gem, Kit::None),
    }
}

#[allow(dead_code)]
fn _sway_used(s: Sway) -> Sway {
    s
}
