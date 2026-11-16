//! How the panel and the edge handle slide: a duration and an easing curve. Release builds use
//! [`MOTION`]; development builds let the tweak panel change it live to compare.

use std::time::Duration;

use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Curve {
    Linear,
    EaseOut,
    EaseInOut,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Motion {
    pub duration: Duration,
    pub curve: Curve,
}

const MOTION: Motion = Motion {
    duration: Duration::from_millis(150),
    curve: Curve::EaseInOut,
};

/// Fraction of the way at fraction `t` of the time, both clamped to 0..=1. Cubic curves.
fn ease(curve: Curve, t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    match curve {
        Curve::Linear => t,
        Curve::EaseOut => 1.0 - (1.0 - t).powi(3),
        Curve::EaseInOut if t < 0.5 => 4.0 * t.powi(3),
        Curve::EaseInOut => 1.0 - (2.0 - 2.0 * t).powi(3) / 2.0,
    }
}

impl Motion {
    /// Position after `elapsed`. It depends on elapsed time, not frame count: a late frame jumps
    /// ahead instead of extending the slide, and the last frame lands exactly on `to`.
    pub fn position(self, from: i32, to: i32, elapsed: Duration) -> i32 {
        let t = if self.duration.is_zero() {
            1.0
        } else {
            elapsed.as_secs_f64() / self.duration.as_secs_f64()
        };
        from + (f64::from(to - from) * ease(self.curve, t)).round() as i32
    }

    pub fn done(self, elapsed: Duration) -> bool {
        elapsed >= self.duration
    }
}

#[cfg(debug_assertions)]
static TUNED: std::sync::Mutex<Option<Motion>> = std::sync::Mutex::new(None);

/// The motion for a slide starting now.
pub fn current() -> Motion {
    #[cfg(debug_assertions)]
    if let Some(motion) = TUNED.lock().ok().and_then(|tuned| *tuned) {
        return motion;
    }
    MOTION
}

/// Development builds only: the tweak panel's choice, applied from the next slide on.
#[cfg(debug_assertions)]
pub fn tune(duration: Duration, curve: Curve) {
    if let Ok(mut tuned) = TUNED.lock() {
        *tuned = Some(Motion { duration, curve });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Catches a curve that starts with a jump or stops short of the destination.
    #[test]
    fn every_curve_starts_still_and_ends_at_one() {
        for curve in [Curve::Linear, Curve::EaseOut, Curve::EaseInOut] {
            assert_eq!(ease(curve, 0.0), 0.0, "{curve:?}");
            assert_eq!(ease(curve, 1.0), 1.0, "{curve:?}");
            assert_eq!(ease(curve, 2.0), 1.0, "{curve:?}");
        }
        assert_eq!(ease(Curve::EaseInOut, 0.5), 0.5);
    }

    #[test]
    fn slide_ends_exactly_at_destination_even_with_late_frame() {
        let motion = MOTION;
        assert_eq!(motion.position(3840, 3315, Duration::ZERO), 3840);
        assert_eq!(motion.position(3840, 3315, motion.duration), 3315);
        assert_eq!(motion.position(3840, 3315, motion.duration * 3), 3315);
        assert!(motion.done(motion.duration));
        let instant = Motion {
            duration: Duration::ZERO,
            ..MOTION
        };
        assert_eq!(instant.position(0, 30, Duration::ZERO), 30);
    }
}
