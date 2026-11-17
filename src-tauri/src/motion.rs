//! How the panel and the edge handle slide: a duration and a CSS-style `cubic-bezier` curve for each
//! kind of slide. Release builds use the constants below; development builds let the tweak window
//! change them live to compare.

use std::time::Duration;

use serde::Deserialize;

/// `cubic-bezier(x1, y1, x2, y2)`: the curve from (0, 0) to (1, 1) with those two control points.
/// `x` is the fraction of time and must stay within 0..=1 so the curve is a function of time; `y`,
/// the fraction of the way, may leave 0..=1 to overshoot.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(try_from = "[f64; 4]")]
pub struct Curve([f64; 4]);

/// The furthest a control point's `y` may go: enough for a strong overshoot, never a wild swing.
const Y_RANGE: std::ops::RangeInclusive<f64> = -1.0..=2.0;

impl TryFrom<[f64; 4]> for Curve {
    type Error = String;

    fn try_from(points: [f64; 4]) -> Result<Self, Self::Error> {
        let [x1, y1, x2, y2] = points;
        if !(0.0..=1.0).contains(&x1) || !(0.0..=1.0).contains(&x2) {
            return Err(format!("control point x must be within 0..=1: {points:?}"));
        }
        if !Y_RANGE.contains(&y1) || !Y_RANGE.contains(&y2) {
            return Err(format!("control point y must be within -1..=2: {points:?}"));
        }
        Ok(Self(points))
    }
}

/// One coordinate of the curve at parameter `t`, with control values `p1` and `p2`.
fn bezier(p1: f64, p2: f64, t: f64) -> f64 {
    let c = 3.0 * p1;
    let b = 3.0 * (p2 - p1) - c;
    let a = 1.0 - c - b;
    ((a * t + b) * t + c) * t
}

fn bezier_slope(p1: f64, p2: f64, t: f64) -> f64 {
    let c = 3.0 * p1;
    let b = 3.0 * (p2 - p1) - c;
    let a = 1.0 - c - b;
    (3.0 * a * t + 2.0 * b) * t + c
}

impl Curve {
    /// Fraction of the way at fraction `x` of the time. Exactly 0 at the start and 1 at the end,
    /// so a slide lands on its destination; in between it may overshoot.
    fn ease(self, x: f64) -> f64 {
        if x <= 0.0 {
            return 0.0;
        }
        if x >= 1.0 {
            return 1.0;
        }
        let [x1, y1, x2, y2] = self.0;
        // With both x within 0..=1, x(t) never decreases: a few Newton steps usually find t, and
        // bisection always does.
        let mut t = x;
        for _ in 0..8 {
            let error = bezier(x1, x2, t) - x;
            if error.abs() < 1e-7 {
                return bezier(y1, y2, t);
            }
            let slope = bezier_slope(x1, x2, t);
            if slope.abs() < 1e-6 {
                break;
            }
            t -= error / slope;
            // Outside 0..=1 the curve may fold back and match `x` at a wrong `t`.
            if !(0.0..=1.0).contains(&t) {
                break;
            }
        }
        let (mut low, mut high) = (0.0, 1.0);
        t = x;
        for _ in 0..60 {
            if bezier(x1, x2, t) < x {
                low = t;
            } else {
                high = t;
            }
            t = (low + high) / 2.0;
        }
        bezier(y1, y2, t)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct Motion {
    #[serde(deserialize_with = "millis")]
    pub duration: Duration,
    pub curve: Curve,
}

/// Longer slides are no longer a tweak but a stall.
const MAX_MILLIS: u64 = 2000;

fn millis<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Duration, D::Error> {
    let millis = u64::deserialize(deserializer)?;
    if millis > MAX_MILLIS {
        return Err(serde::de::Error::custom(format!(
            "duration above {MAX_MILLIS} ms"
        )));
    }
    Ok(Duration::from_millis(millis))
}

impl Motion {
    const fn new(millis: u64, curve: [f64; 4]) -> Self {
        Self {
            duration: Duration::from_millis(millis),
            curve: Curve(curve),
        }
    }

    /// Position after `elapsed`. It depends on elapsed time, not frame count: a late frame jumps
    /// ahead instead of extending the slide, and the last frame lands exactly on `to`.
    pub fn position(self, from: i32, to: i32, elapsed: Duration) -> i32 {
        let t = if self.duration.is_zero() {
            1.0
        } else {
            elapsed.as_secs_f64() / self.duration.as_secs_f64()
        };
        from + (f64::from(to - from) * self.curve.ease(t)).round() as i32
    }

    pub fn done(self, elapsed: Duration) -> bool {
        elapsed >= self.duration
    }
}

/// Which slide a motion drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Open,
    Close,
    HandleIn,
    HandleOut,
}

/// One motion per kind of slide.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Motions {
    pub open: Motion,
    pub close: Motion,
    pub handle_in: Motion,
    pub handle_out: Motion,
}

impl Motions {
    fn get(self, kind: Kind) -> Motion {
        match kind {
            Kind::Open => self.open,
            Kind::Close => self.close,
            Kind::HandleIn => self.handle_in,
            Kind::HandleOut => self.handle_out,
        }
    }
}

// Release motions: duration in milliseconds and `cubic-bezier` control points. The tweak window's
// export rewrites these lines; keep each on one line in this form.
// The curve is the usual cubic-bezier fit of a cubic ease-in-out.
const OPEN: Motion = Motion::new(150, [0.65, 0.0, 0.35, 1.0]);
const CLOSE: Motion = Motion::new(150, [0.65, 0.0, 0.35, 1.0]);
const HANDLE_IN: Motion = Motion::new(100, [0.65, 0.0, 0.35, 1.0]);
const HANDLE_OUT: Motion = Motion::new(100, [0.65, 0.0, 0.35, 1.0]);

const MOTIONS: Motions = Motions {
    open: OPEN,
    close: CLOSE,
    handle_in: HANDLE_IN,
    handle_out: HANDLE_OUT,
};

#[cfg(debug_assertions)]
static TUNED: std::sync::Mutex<Option<Motions>> = std::sync::Mutex::new(None);

/// The motion for a slide of `kind` starting now.
pub fn current(kind: Kind) -> Motion {
    #[cfg(debug_assertions)]
    if let Some(motions) = TUNED.lock().ok().and_then(|tuned| *tuned) {
        return motions.get(kind);
    }
    MOTIONS.get(kind)
}

/// Development builds only: the tweak window's choice, applied from the next slide on.
#[cfg(debug_assertions)]
pub fn tune(motions: Motions) {
    if let Ok(mut tuned) = TUNED.lock() {
        *tuned = Some(motions);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINEAR: Curve = Curve([0.0, 0.0, 1.0, 1.0]);
    /// `ease-out-back`: passes the destination and comes back.
    const BACK_OUT: Curve = Curve([0.34, 1.56, 0.64, 1.0]);
    /// `ease-in-back`: first moves away from the destination.
    const BACK_IN: Curve = Curve([0.36, 0.0, 0.66, -0.56]);
    const STEEP: Curve = Curve([1.0, 0.0, 0.0, 1.0]);

    /// Catches a curve that starts with a jump or stops short of the destination.
    #[test]
    fn every_curve_starts_still_and_ends_exactly_at_one() {
        for curve in [LINEAR, BACK_OUT, BACK_IN, STEEP, OPEN.curve] {
            assert_eq!(curve.ease(0.0), 0.0, "{curve:?}");
            assert_eq!(curve.ease(-1.0), 0.0, "{curve:?}");
            assert_eq!(curve.ease(1.0), 1.0, "{curve:?}");
            assert_eq!(curve.ease(2.0), 1.0, "{curve:?}");
        }
    }

    /// Catches a solver that finds the wrong time on the curve, including where x(t) is flat.
    #[test]
    fn solver_follows_the_curve() {
        for curve in [LINEAR, BACK_OUT, BACK_IN, STEEP, OPEN.curve] {
            let [x1, y1, x2, y2] = curve.0;
            for step in 1..100 {
                let t = f64::from(step) / 100.0;
                let (x, y) = (bezier(x1, x2, t), bezier(y1, y2, t));
                assert!((curve.ease(x) - y).abs() < 1e-3, "{curve:?} at t={t}");
            }
        }
        assert!((OPEN.curve.ease(0.5) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn overshoot_passes_the_destination_and_still_lands_on_it() {
        let motion = Motion {
            curve: BACK_OUT,
            ..OPEN
        };
        let peak = (1..30)
            .map(|step| motion.position(0, 100, motion.duration * step / 30))
            .max();
        assert!(peak > Some(100), "{peak:?}");
        assert_eq!(motion.position(0, 100, motion.duration), 100);
    }

    /// A control point with x outside 0..=1 makes x(t) turn back: no longer a function of time.
    #[test]
    fn curve_rejects_x_outside_unit_range_but_allows_overshooting_y() {
        assert!(Curve::try_from([1.2, 0.0, 0.5, 1.0]).is_err());
        assert!(Curve::try_from([0.5, 0.0, -0.1, 1.0]).is_err());
        assert!(Curve::try_from([f64::NAN, 0.0, 0.5, 1.0]).is_err());
        assert!(Curve::try_from([0.5, 5.0, 0.5, 1.0]).is_err());
        assert!(Curve::try_from([0.34, 1.56, 0.64, -0.5]).is_ok());
    }

    #[test]
    fn slide_ends_exactly_at_destination_even_with_late_frame() {
        let motion = OPEN;
        assert_eq!(motion.position(3840, 3315, Duration::ZERO), 3840);
        assert_eq!(motion.position(3840, 3315, motion.duration), 3315);
        assert_eq!(motion.position(3840, 3315, motion.duration * 3), 3315);
        assert!(motion.done(motion.duration));
        let instant = Motion {
            duration: Duration::ZERO,
            ..OPEN
        };
        assert_eq!(instant.position(0, 30, Duration::ZERO), 30);
    }
}
