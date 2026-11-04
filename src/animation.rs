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
}

/// How a slide runs: fast first and settling (ease-out, cubic), or slow
/// first and leaving fast (ease-in, cubic).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Easing {
    Out,
    In,
}

impl Easing {
    fn apply(self, t: f32) -> f32 {
        match self {
            Easing::Out => 1.0 - (1.0 - t).powi(3),
            Easing::In => t.powi(3),
        }
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
    fn a_slide_eases_out_or_in_and_ends_on_time() {
        let start = Instant::now();
        let out = Slide::start(start, Duration::from_millis(200), Easing::Out);
        assert_eq!(out.progress(start), 0.0);
        let half = out.progress(start + Duration::from_millis(100));
        assert!(
            half > 0.8,
            "ease-out is most of the way at half time: {half}"
        );
        assert_eq!(out.progress(start + Duration::from_millis(200)), 1.0);
        assert_eq!(out.progress(start + Duration::from_secs(5)), 1.0);
        assert!(!out.done(start + Duration::from_millis(199)));
        assert!(out.done(start + Duration::from_millis(200)));

        let r#in = Slide::start(start, Duration::from_millis(150), Easing::In);
        let half = r#in.progress(start + Duration::from_millis(75));
        assert!(half < 0.2, "ease-in has barely moved at half time: {half}");
        assert_eq!(r#in.progress(start + Duration::from_millis(150)), 1.0);

        // Before it starts (a test's slide set in the future) it has not moved.
        assert_eq!(out.progress(start - Duration::from_millis(5)), 0.0);
        // An instant slide is over at once.
        assert_eq!(
            Slide::start(start, Duration::ZERO, Easing::Out).progress(start),
            1.0
        );
    }
}
