//! Motion as a pure function of time.
//!
//! Every track here mirrors a CSS `@keyframes` rule from the Holo design
//! (`runo-figma/holo-2d-v2.html`): the same key times, the same values and the
//! same cubic-bezier easing. Nothing ticks or keeps state, so a bot's pose at
//! any time `t` is one cheap evaluation, frames can be skipped without drift,
//! and two bots on the same clock never fall out of step.

/// A CSS `cubic-bezier(x1, y1, x2, y2)` timing function.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ease(pub f64, pub f64, pub f64, pub f64);

/// The design's house curve, `cubic-bezier(.45, 0, .55, 1)`: a soft ease in and out.
pub const SOFT: Ease = Ease(0.45, 0.0, 0.55, 1.0);
/// `linear`.
pub const LINEAR: Ease = Ease(0.0, 0.0, 1.0, 1.0);
/// The particles' rise, `cubic-bezier(.3, 0, .7, 1)`.
pub const RISE: Ease = Ease(0.3, 0.0, 0.7, 1.0);

impl Ease {
    /// Progress `y` at time fraction `x`, both in 0..=1.
    pub fn at(self, x: f64) -> f64 {
        let Ease(x1, y1, x2, y2) = self;
        if x <= 0.0 {
            return 0.0;
        }
        if x >= 1.0 {
            return 1.0;
        }
        if x1 == y1 && x2 == y2 {
            return x;
        }
        // Solve bx(s) = x for s with Newton, falling back to bisection.
        let bx = |s: f64| {
            3.0 * (1.0 - s) * (1.0 - s) * s * x1 + 3.0 * (1.0 - s) * s * s * x2 + s * s * s
        };
        let by = |s: f64| {
            3.0 * (1.0 - s) * (1.0 - s) * s * y1 + 3.0 * (1.0 - s) * s * s * y2 + s * s * s
        };
        let dx = |s: f64| {
            3.0 * (1.0 - s) * (1.0 - s) * x1
                + 6.0 * (1.0 - s) * s * (x2 - x1)
                + 3.0 * s * s * (1.0 - x2)
        };
        let mut s = x;
        for _ in 0..8 {
            let err = bx(s) - x;
            if err.abs() < 1e-6 {
                return by(s);
            }
            let d = dx(s);
            if d.abs() < 1e-6 {
                break;
            }
            s -= err / d;
        }
        let (mut lo, mut hi) = (0.0, 1.0);
        s = x;
        for _ in 0..40 {
            let v = bx(s);
            if (v - x).abs() < 1e-7 {
                break;
            }
            if v < x {
                lo = s;
            } else {
                hi = s;
            }
            s = 0.5 * (lo + hi);
        }
        by(s)
    }
}

/// Where an infinite animation of `dur` seconds, started `delay` seconds in,
/// is at time `t`, as a fraction in 0..1. A positive delay is treated as a
/// phase offset so loops stay continuous from the first frame.
pub fn phase(t: f64, dur: f64, delay: f64) -> f64 {
    ((t - delay) / dur).rem_euclid(1.0)
}

/// `animation-direction: alternate`: runs 0..1 then back 1..0, eased each way.
pub fn alternate(t: f64, dur: f64, ease: Ease) -> f64 {
    let p = (t / dur).rem_euclid(2.0);
    if p <= 1.0 {
        ease.at(p)
    } else {
        ease.at(2.0 - p)
    }
}

/// One channel of a keyframe rule, with the same easing on every segment.
/// `keys` are `(offset, value)` pairs sorted by offset, covering 0 and 1.
pub fn keys(p: f64, keys: &[(f64, f64)], ease: Ease) -> f64 {
    let n = keys.len();
    if n == 0 {
        return 0.0;
    }
    if p <= keys[0].0 {
        return keys[0].1;
    }
    for w in keys.windows(2) {
        let (a, b) = (w[0], w[1]);
        if p <= b.0 {
            let span = b.0 - a.0;
            if span <= 0.0 {
                return b.1;
            }
            let k = ease.at((p - a.0) / span);
            return a.1 + (b.1 - a.1) * k;
        }
    }
    keys[n - 1].1
}

/// Keyframes where each segment has its own timing function, as in the hop:
/// `(offset, value, easing of the segment that starts here)`.
pub fn keys_eased(p: f64, keys: &[(f64, f64, Ease)]) -> f64 {
    if p <= keys[0].0 {
        return keys[0].1;
    }
    for w in keys.windows(2) {
        let (a, b) = (w[0], w[1]);
        if p <= b.0 {
            let span = b.0 - a.0;
            if span <= 0.0 {
                return b.1;
            }
            return a.1 + (b.1 - a.1) * a.2.at((p - a.0) / span);
        }
    }
    keys[keys.len() - 1].1
}

/// `steps(1)` keyframes: each value holds until the next key.
pub fn steps(p: f64, keys: &[(f64, f64)]) -> f64 {
    let mut v = keys[0].1;
    for &(at, val) in keys {
        if p >= at {
            v = val;
        } else {
            break;
        }
    }
    v
}

/// A symmetric loop `a → b → a` over `dur`, eased softly: the shape of most
/// of the design's breathing, bobbing and glowing.
pub fn pulse(t: f64, dur: f64, delay: f64, a: f64, b: f64) -> f64 {
    keys(phase(t, dur, delay), &[(0.0, a), (0.5, b), (1.0, a)], SOFT)
}

/// A critically damped spring from 0 to 1, for blending one mood into the
/// next without a snap. `elapsed` is seconds since the change began; it
/// settles in about `settle` seconds.
pub fn spring(elapsed: f64, settle: f64) -> f64 {
    if elapsed <= 0.0 {
        return 0.0;
    }
    let w = 6.0 / settle.max(1e-3);
    let x = w * elapsed;
    (1.0 - (1.0 + x) * (-x).exp()).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ease_endpoints_and_symmetry() {
        assert_eq!(SOFT.at(0.0), 0.0);
        assert_eq!(SOFT.at(1.0), 1.0);
        assert!((SOFT.at(0.5) - 0.5).abs() < 1e-4);
        assert!((SOFT.at(0.25) + SOFT.at(0.75) - 1.0).abs() < 1e-4);
        assert!((LINEAR.at(0.3) - 0.3).abs() < 1e-9);
    }

    #[test]
    fn ease_is_monotonic() {
        let mut last = 0.0;
        for i in 1..=100 {
            let v = SOFT.at(i as f64 / 100.0);
            assert!(v >= last - 1e-9);
            last = v;
        }
    }

    #[test]
    fn keyframes_hold_and_interpolate() {
        let k = [(0.0, 0.0), (0.4, 0.0), (0.6, 10.0), (1.0, 10.0)];
        assert_eq!(keys(0.2, &k, SOFT), 0.0);
        assert!((keys(0.5, &k, LINEAR) - 5.0).abs() < 1e-9);
        assert_eq!(keys(0.8, &k, SOFT), 10.0);
    }

    #[test]
    fn loops_are_continuous() {
        // The value just before the end of a loop matches the start.
        let a = pulse(4.4 - 1e-6, 4.4, 0.0, 0.0, -7.0);
        let b = pulse(0.0, 4.4, 0.0, 0.0, -7.0);
        assert!((a - b).abs() < 1e-3);
    }

    #[test]
    fn steps_hold_values() {
        let k = [
            (0.0, 0.0),
            (0.72, 1.0),
            (0.74, 0.3),
            (0.76, 1.0),
            (1.0, 0.0),
        ];
        assert_eq!(steps(0.5, &k), 0.0);
        assert_eq!(steps(0.73, &k), 1.0);
        assert_eq!(steps(0.75, &k), 0.3);
    }

    #[test]
    fn spring_settles() {
        assert_eq!(spring(0.0, 0.25), 0.0);
        assert!(spring(0.25, 0.25) > 0.98);
        let mut last = 0.0;
        for i in 0..50 {
            let v = spring(i as f64 * 0.01, 0.25);
            assert!(v >= last);
            last = v;
        }
    }
}
