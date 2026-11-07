//! Motion: Plasma's animation speed and the timing of a slide. Plasma
//! keeps its speed as `AnimationDurationFactor` under `[KDE]` in
//! `kdeglobals` (1 by default, 0 for "Instant"); every duration here is
//! scaled by it, and at 0 nothing animates.

use std::time::{Duration, Instant};

use crate::storage;

/// Plasma's animation speed, as the factor every duration is scaled by.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Speed(f32);

impl Speed {
    /// Plasma's default: durations as written.
    pub const NORMAL: Speed = Speed(1.0);
    /// Plasma's "Instant": no animation at all. What the tests and
    /// `App::new` use, so a screen never captures mid-slide.
    pub const INSTANT: Speed = Speed(0.0);

    /// A factor as Plasma writes it; below 0 counts as 0.
    pub fn factor(factor: f32) -> Speed {
        Speed(if factor.is_finite() {
            factor.max(0.0)
        } else {
            1.0
        })
    }

    /// The speed from this computer's `kdeglobals`
    /// (`$XDG_CONFIG_HOME/kdeglobals`, else `~/.config/kdeglobals`);
    /// normal without the file or the key.
    pub fn from_plasma() -> Speed {
        storage::config_home()
            .map(|home| home.join("kdeglobals"))
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map_or(Speed::NORMAL, |text| Speed::parse(&text))
    }

    /// `AnimationDurationFactor` under `[KDE]` in a kdeglobals text.
    pub fn parse(text: &str) -> Speed {
        let mut in_kde = false;
        for line in text.lines() {
            let line = line.trim();
            if let Some(group) = line.strip_prefix('[') {
                in_kde = group.trim_end_matches(']') == "KDE";
            } else if in_kde
                && let Some((key, value)) = line.split_once('=')
                && key.trim() == "AnimationDurationFactor"
                && let Ok(factor) = value.trim().parse::<f32>()
            {
                return Speed::factor(factor);
            }
        }
        Speed::NORMAL
    }

    /// Whether nothing animates.
    pub fn instant(self) -> bool {
        self.0 <= 0.0
    }

    /// `base` at this speed.
    pub fn scale(self, base: Duration) -> Duration {
        base.mul_f32(self.0)
    }

    /// The factor itself: 1 at normal speed, 0 at Instant.
    pub fn value(self) -> f32 {
        self.0
    }
}

/// A damped spring, as KWin moves the desktops when it switches them: its
/// Slide effect (`src/plugins/slide/slide.cpp`, KWin 6.8) drives a
/// `SpringMotion` (`src/effect/springmotion.cpp`) of stiffness 300 divided
/// by Plasma's speed factor and damping ratio 1.1, per unit mass, toward
/// the new desktop. A slower setting softens the spring rather than
/// stretching a fixed curve, so the time grows with the square root of the
/// factor and a switch at 0.25 still shows. KWin integrates it with RK4 in
/// 10 ms steps; this is the same motion solved exactly. Overdamped: it
/// never overshoots.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spring {
    /// The slow root of the motion, per second (negative).
    slow: f32,
    /// The fast root (negative, below `slow`).
    fast: f32,
}

impl Spring {
    /// KWin's desktop switch at `speed`; `None` at Instant.
    pub fn desktop_switch(speed: Speed) -> Option<Spring> {
        if speed.instant() {
            return None;
        }
        Some(Spring::new(300.0 / speed.value(), 1.1))
    }

    /// A spring of `stiffness` (per unit mass) and an overdamped `damping`
    /// ratio (above 1).
    pub fn new(stiffness: f32, damping: f32) -> Spring {
        let omega = stiffness.sqrt();
        let root = (damping * damping - 1.0).max(1e-4).sqrt();
        Spring {
            slow: -omega * (damping - root),
            fast: -omega * (damping + root),
        }
    }

    /// The same motion taking `by` times as long: the shape kept, every
    /// moment stretched (a spring of a `by`²-th of the stiffness).
    pub fn slowed(self, by: f32) -> Spring {
        let by = by.max(1e-3);
        Spring {
            slow: self.slow / by,
            fast: self.fast / by,
        }
    }

    /// The distance from the anchor and the velocity `t` seconds after
    /// starting `distance` away at `velocity`.
    pub fn at(self, distance: f32, velocity: f32, t: f32) -> (f32, f32) {
        let (a, b) = (self.slow, self.fast);
        let slow = (velocity - b * distance) / (a - b);
        let fast = distance - slow;
        let (ea, eb) = ((a * t).exp(), (b * t).exp());
        (slow * ea + fast * eb, a * slow * ea + b * fast * eb)
    }

    /// When the motion is over, as KWin ends it (`SpringMotion::isMoving`):
    /// both the distance left and the speed at most `epsilon` (KWin's is one
    /// pixel, and one pixel a second), found in 1 ms steps; at most 5 s.
    pub fn settle(self, distance: f32, velocity: f32, epsilon: f32) -> Duration {
        let mut ms = 0;
        while ms < 5000 {
            let (x, v) = self.at(distance, velocity, ms as f32 / 1000.0);
            if x.abs() <= epsilon && v.abs() <= epsilon {
                break;
            }
            ms += 1;
        }
        Duration::from_millis(ms)
    }
}

/// The shape of a slide, as CSS's `cubic-bezier(x1, y1, x2, y2)`: time
/// along x, distance along y, from (0, 0) to (1, 1) through the two
/// control points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Easing {
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
}

impl Easing {
    /// Material 3's standard curve, `cubic-bezier(0.2, 0, 0, 1)`: a short
    /// ramp-up, 60 % of the way at a quarter of the time, 88 % at half,
    /// and settling over the rest, so the slowing shows while there is
    /// still distance left. A cubic ease-out, by contrast, starts at
    /// three times the average speed and reads as a snap, and a cubic
    /// ease-in has barely moved by half time and leaves at full speed.
    pub const STANDARD: Easing = Easing::bezier(0.2, 0.0, 0.0, 1.0);

    pub const fn bezier(x1: f32, y1: f32, x2: f32, y2: f32) -> Easing {
        Easing { x1, y1, x2, y2 }
    }

    fn x(self, s: f32) -> f32 {
        let r = 1.0 - s;
        3.0 * r * r * s * self.x1 + 3.0 * r * s * s * self.x2 + s * s * s
    }

    fn y(self, s: f32) -> f32 {
        let r = 1.0 - s;
        3.0 * r * r * s * self.y1 + 3.0 * r * s * s * self.y2 + s * s * s
    }

    /// The distance at time `t` (0 to 1): the parameter where the curve's
    /// x is `t`, found by bisection since x is monotonic, then its y.
    fn apply(self, t: f32) -> f32 {
        if t <= 0.0 {
            return 0.0;
        }
        if t >= 1.0 {
            return 1.0;
        }
        let (mut lo, mut hi) = (0.0_f32, 1.0_f32);
        for _ in 0..24 {
            let mid = (lo + hi) / 2.0;
            if self.x(mid) < t {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        self.y((lo + hi) / 2.0)
    }
}

/// One run from 0 to 1 over `duration` from `since`, eased.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Slide {
    since: Instant,
    duration: Duration,
    easing: Easing,
}

impl Slide {
    pub fn start(since: Instant, duration: Duration, easing: Easing) -> Slide {
        Slide {
            since,
            duration,
            easing,
        }
    }

    /// Where the slide is at `now`, eased, 0 before it starts and 1 once
    /// it is over.
    pub fn progress(&self, now: Instant) -> f32 {
        if self.duration.is_zero() {
            return 1.0;
        }
        let elapsed = now.saturating_duration_since(self.since);
        let t = (elapsed.as_secs_f32() / self.duration.as_secs_f32()).clamp(0.0, 1.0);
        self.easing.apply(t)
    }

    pub fn done(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.since) >= self.duration
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_factor_comes_from_the_kde_group_and_zero_is_instant() {
        let text = "[General]\nAnimationDurationFactor=3\n\n[KDE]\nAnimationDurationFactor=0.25\nwidgetStyle=Darkly\n";
        let speed = Speed::parse(text);
        assert_eq!(speed, Speed::factor(0.25));
        assert_eq!(
            speed.scale(Duration::from_millis(200)),
            Duration::from_millis(50)
        );
        assert!(!speed.instant());
        assert!(Speed::parse("[KDE]\nAnimationDurationFactor=0\n").instant());
        assert_eq!(Speed::parse("[KDE]\nwidgetStyle=Darkly\n"), Speed::NORMAL);
        assert_eq!(Speed::parse(""), Speed::NORMAL);
        assert_eq!(
            Speed::parse("[KDE]\nAnimationDurationFactor=fast\n"),
            Speed::NORMAL
        );
        assert_eq!(Speed::factor(-1.0), Speed::INSTANT);
    }

    #[test]
    fn a_slide_ramps_up_settles_and_ends_on_time() {
        let start = Instant::now();
        let slide = Slide::start(start, Duration::from_millis(200), Easing::STANDARD);
        assert_eq!(slide.progress(start), 0.0);
        let early = slide.progress(start + Duration::from_millis(20));
        assert!(
            (0.1..0.2).contains(&early),
            "the ramp-up: a sixth of the way at a tenth of the time: {early}"
        );
        let quarter = slide.progress(start + Duration::from_millis(50));
        assert!(
            (0.55..0.65).contains(&quarter),
            "most of the way at quarter time: {quarter}"
        );
        let late = slide.progress(start + Duration::from_millis(150));
        assert!(
            (0.96..0.99).contains(&late),
            "still settling at three quarters: {late}"
        );
        assert_eq!(slide.progress(start + Duration::from_millis(200)), 1.0);
        assert_eq!(slide.progress(start + Duration::from_secs(5)), 1.0);
        assert!(!slide.done(start + Duration::from_millis(199)));
        assert!(slide.done(start + Duration::from_millis(200)));

        // Before it starts (a test's slide set in the future) it has not moved.
        assert_eq!(slide.progress(start - Duration::from_millis(5)), 0.0);
        // An instant slide is over at once.
        assert_eq!(
            Slide::start(start, Duration::ZERO, Easing::STANDARD).progress(start),
            1.0
        );
        // A bezier through the diagonal is linear.
        let linear = Easing::bezier(0.0, 0.0, 1.0, 1.0);
        assert!((linear.apply(0.3) - 0.3).abs() < 1e-4);
        assert!((linear.apply(0.9) - 0.9).abs() < 1e-4);
    }

    #[test]
    fn the_desktop_switch_spring_matches_kwins_and_keeps_its_speed() {
        assert_eq!(Spring::desktop_switch(Speed::INSTANT), None);
        let normal = Spring::desktop_switch(Speed::NORMAL).unwrap();
        let fast = Spring::desktop_switch(Speed::factor(0.25)).unwrap();
        // A 1000 px switch, ended as KWin ends it.
        let (normal_end, fast_end) = (
            normal.settle(1000.0, 0.0, 1.0),
            fast.settle(1000.0, 0.0, 1.0),
        );
        // The time grows about with the square root of the factor, not the
        // factor (a little more, as the stiffer spring's pixel stop comes later).
        let ratio = fast_end.as_secs_f32() / normal_end.as_secs_f32();
        assert!((0.5..0.56).contains(&ratio), "{ratio}");
        assert!((400..500).contains(&fast_end.as_millis()), "{fast_end:?}");

        let at = |ms: u64| 1.0 - fast.at(1.0, 0.0, ms as f32 / 1000.0).0;
        assert_eq!(at(0), 0.0);
        assert!((0.05..0.2).contains(&at(16)), "from rest: {}", at(16));
        assert!((0.4..0.6).contains(&at(50)), "halfway: {}", at(50));
        assert!(
            (0.8..0.95).contains(&at(120)),
            "still closing in: {}",
            at(120)
        );
        let mut last = 0.0;
        for ms in 0..500 {
            let p = at(ms);
            assert!(
                p >= last && p <= 1.0,
                "never back, never past: {p} at {ms} ms"
            );
            last = p;
        }

        // A start already moving keeps its velocity: KWin's switch during a switch.
        let (x, v) = fast.at(1.0, -20.0, 0.0);
        assert!((x - 1.0).abs() < 1e-5 && (v + 20.0).abs() < 1e-3, "{x} {v}");
        assert!(fast.at(1.0, -20.0, 0.016).0 < fast.at(1.0, 0.0, 0.016).0);
        // Against KWin's own steps: RK4 at 10 ms from 1000 px.
        let (k, c) = (1200.0_f32, 2.0 * 1200.0_f32.sqrt() * 1.1);
        let (mut x, mut v) = (1000.0_f32, 0.0_f32);
        let f = |x: f32, v: f32| (v, -k * x - c * v);
        for _ in 0..10 {
            let h = 0.01;
            let (a1, b1) = f(x, v);
            let (a2, b2) = f(x + a1 * h / 2.0, v + b1 * h / 2.0);
            let (a3, b3) = f(x + a2 * h / 2.0, v + b2 * h / 2.0);
            let (a4, b4) = f(x + a3 * h, v + b3 * h);
            x += h / 6.0 * (a1 + 2.0 * a2 + 2.0 * a3 + a4);
            v += h / 6.0 * (b1 + 2.0 * b2 + 2.0 * b3 + b4);
        }
        let (exact, _) = fast.at(1000.0, 0.0, 0.1);
        assert!((exact - x).abs() < 1.0, "{exact} against KWin's {x}");

        // Slowed to half speed: the same curve, every moment twice as late,
        // which at 0.25 is KWin's own spring at normal speed.
        let half = fast.slowed(2.0);
        assert!((half.at(1.0, 0.0, 0.1).0 - fast.at(1.0, 0.0, 0.05).0).abs() < 1e-5);
        assert!((half.at(1.0, 0.0, 0.2).0 - normal.at(1.0, 0.0, 0.2).0).abs() < 1e-4);
    }
}
