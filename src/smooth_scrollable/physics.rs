//! Physics for Firefox-style smooth scrolling.
//!
//! This module is a faithful Rust port of the animation model that Gecko uses
//! for `ScrollMode::Smooth`, as implemented by
//! [`ScrollAnimationBezierPhysics`][bezier] and [`SMILKeySpline`][spline].
//!
//! ## The model
//!
//! Firefox documents `ScrollMode::Smooth` like this (see
//! [`ScrollTypes.h`][types]):
//!
//! > `Smooth` scrolls have a symmetrical acceleration and deceleration curve
//! > modeled with a set of splines that guarantee that the destination will be
//! > reached over a fixed time interval. `Smooth` will only be smooth if smooth
//! > scrolling is actually enabled. This behavior is utilized by keyboard and
//! > mouse wheel scrolling events.
//!
//! Concretely, each animation is a cubic Bézier easing curve applied
//! independently to the horizontal and vertical axes. The duration is not
//! fixed a priori: it is derived from the *rate of incoming scroll events* so
//! that a fast sequence of wheel notches feels snappy while a slow one is easy
//! to follow. The curve's first control point is derived from the velocity the
//! animation already had, which lets consecutive wheel events be chained into
//! one continuous motion instead of restarting from rest.
//!
//! ## Relevant preferences
//!
//! | Preference | Default | Meaning |
//! | --- | --- | --- |
//! | `general.smoothScroll.currentVelocityWeighting` | `0.25` | How much the current velocity shapes the curve |
//! | `general.smoothScroll.stopDecelerationWeighting` | `0.4` | Where the curve starts decelerating |
//! | `general.smoothScroll.durationToIntervalRatio` | `200` | Duration as a percentage of the event interval |
//! | `general.smoothScroll.mouseWheel.durationMinMS` | `50` | Lower bound for wheel durations |
//! | `general.smoothScroll.mouseWheel.durationMaxMS` | `200` | Upper bound for wheel durations |
//! | `general.smoothScroll.pixels.duration*MS` | `150`/`150` | Bounds for pixel deltas (e.g. trackpads) |
//!
//! [bezier]: https://searchfox.org/mozilla-central/source/layout/generic/ScrollAnimationBezierPhysics.cpp
//! [spline]: https://searchfox.org/mozilla-central/source/gfx/smil/SMILKeySpline.cpp
//! [types]: https://searchfox.org/mozilla-central/source/layout/base/ScrollTypes.h

use iced_core::Vector;
use iced_core::time::{Duration, Instant};

/// Firefox's `general.smoothScroll.currentVelocityWeighting` default.
pub const CURRENT_VELOCITY_WEIGHTING: f64 = 0.25;

/// Firefox's `general.smoothScroll.stopDecelerationWeighting` default.
pub const STOP_DECELERATION_WEIGHTING: f64 = 0.4;

/// Firefox's `general.smoothScroll.durationToIntervalRatio` default, as a
/// ratio instead of a percentage.
pub const DURATION_TO_INTERVAL_RATIO: f64 = 2.0;

/// The tuning parameters of a [`BezierPhysics`] animation.
///
/// These mirror the `general.smoothScroll.*` preferences of Firefox. The
/// constructors return the exact defaults shipped by Firefox for each scroll
/// origin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SmoothScrollSettings {
    /// The minimum duration of an animation, in milliseconds.
    pub duration_min_ms: i32,
    /// The maximum duration of an animation, in milliseconds.
    pub duration_max_ms: i32,
    /// How much longer than the average event interval the duration should be.
    pub interval_ratio: f64,
    /// The weight of the current velocity in the generated timing function.
    pub current_velocity_weighting: f64,
    /// The weight of the stopping deceleration in the generated timing function.
    pub stop_deceleration_weighting: f64,
}

impl SmoothScrollSettings {
    /// Creates settings with the given duration bounds and Firefox's default
    /// weightings.
    pub const fn new(duration_min_ms: i32, duration_max_ms: i32) -> Self {
        Self {
            duration_min_ms,
            duration_max_ms,
            interval_ratio: DURATION_TO_INTERVAL_RATIO,
            current_velocity_weighting: CURRENT_VELOCITY_WEIGHTING,
            stop_deceleration_weighting: STOP_DECELERATION_WEIGHTING,
        }
    }

    /// Settings for a mouse wheel (`ScrollDelta::Lines`), matching
    /// `general.smoothScroll.mouseWheel`.
    pub const fn mouse_wheel() -> Self {
        Self::new(50, 200)
    }

    /// Settings for a pixel delta (e.g. a precision trackpad), matching
    /// `general.smoothScroll.pixels`.
    pub const fn pixels() -> Self {
        Self::new(150, 150)
    }

    /// Settings for a line delta, matching `general.smoothScroll.lines`.
    pub const fn lines() -> Self {
        Self::new(150, 150)
    }

    /// Settings for a page delta, matching `general.smoothScroll.pages`.
    pub const fn pages() -> Self {
        Self::new(150, 150)
    }

    /// Settings for scrolling coming from the scrollbars, matching
    /// `general.smoothScroll.scrollbars`.
    pub const fn scrollbars() -> Self {
        Self::new(150, 150)
    }

    /// Settings for any other scroll origin, matching
    /// `general.smoothScroll.other`.
    pub const fn other() -> Self {
        Self::new(150, 150)
    }
}

impl Default for SmoothScrollSettings {
    fn default() -> Self {
        Self::mouse_wheel()
    }
}

/// A cubic Bézier timing function with fixed endpoints `(0, 0)` and `(1, 1)`.
///
/// This is a direct port of [`SMILKeySpline`][spline] (itself derived from
/// WebKit's `UnitBezier`), which Gecko uses to sample the `ScrollMode::Smooth`
/// animation curve.
///
/// [spline]: https://searchfox.org/mozilla-central/source/gfx/smil/SMILKeySpline.cpp
#[derive(Debug, Clone, Copy)]
struct CubicBezier {
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    /// Precomputed `x` values of the curve at evenly spaced `t`.
    samples: [f64; SAMPLE_COUNT],
    /// Whether the curve is the identity (no easing).
    identity: bool,
}

const SAMPLE_COUNT: usize = 11;
const SAMPLE_STEP: f64 = 1.0 / (SAMPLE_COUNT as f64 - 1.0);
const NEWTON_ITERATIONS: usize = 4;
const NEWTON_MIN_SLOPE: f64 = 0.001;
const SUBDIVISION_PRECISION: f64 = 1e-7;
const SUBDIVISION_MAX_ITERATIONS: usize = 10;

fn coefficient_a(a1: f64, a2: f64) -> f64 {
    1.0 - 3.0 * a2 + 3.0 * a1
}

fn coefficient_b(a1: f64, a2: f64) -> f64 {
    3.0 * a2 - 6.0 * a1
}

fn coefficient_c(a1: f64) -> f64 {
    3.0 * a1
}

fn sample_curve(a: f64, b: f64, c: f64, t: f64) -> f64 {
    ((a * t + b) * t + c) * t
}

fn sample_curve_derivative(a: f64, b: f64, c: f64, t: f64) -> f64 {
    (3.0 * a * t + 2.0 * b) * t + c
}

impl CubicBezier {
    fn new(x1: f64, y1: f64, x2: f64, y2: f64) -> Self {
        let identity = x1 == y1 && x2 == y2;

        let mut samples = [0.0; SAMPLE_COUNT];

        if !identity {
            let ax = coefficient_a(x1, x2);

            for (i, sample) in samples.iter_mut().enumerate() {
                *sample = sample_curve(
                    ax,
                    coefficient_b(x1, x2),
                    coefficient_c(x1),
                    i as f64 * SAMPLE_STEP,
                );
            }

            // Guarantee the last sample is exactly `1.0`, like WebKit does.
            samples[SAMPLE_COUNT - 1] = 1.0;
        }

        Self {
            x1,
            y1,
            x2,
            y2,
            samples,
            identity,
        }
    }

    /// Returns the value `y` of the curve at the given `x`, where `x` is the
    /// normalized time in `0.0..=1.0`.
    fn value_at(&self, x: f64) -> f64 {
        if self.identity {
            return x;
        }

        if x <= 0.0 {
            return 0.0;
        }

        if x >= 1.0 {
            return 1.0;
        }

        let t = self.t_for_x(x);

        sample_curve(
            coefficient_a(self.y1, self.y2),
            coefficient_b(self.y1, self.y2),
            coefficient_c(self.y1),
            t,
        )
    }

    /// Returns the raw derivative `(dx/dt, dy/dt)` of the curve at the given
    /// `x`.
    ///
    /// Firefox uses these values to recover the velocity of an in-flight
    /// animation.
    fn derivative_at(&self, x: f64) -> (f64, f64) {
        if self.identity {
            return (1.0, 1.0);
        }

        let t = self.t_for_x(x);

        let dx = sample_curve_derivative(
            coefficient_a(self.x1, self.x2),
            coefficient_b(self.x1, self.x2),
            coefficient_c(self.x1),
            t,
        );

        let dy = sample_curve_derivative(
            coefficient_a(self.y1, self.y2),
            coefficient_b(self.y1, self.y2),
            coefficient_c(self.y1),
            t,
        );

        (dx, dy)
    }

    fn t_for_x(&self, x: f64) -> f64 {
        // Find the sample interval that contains `x`.
        let mut interval_start = 0.0;
        let mut current_sample = 0;

        while current_sample < SAMPLE_COUNT - 1
            && self.samples[current_sample] <= x
        {
            current_sample += 1;
        }

        if current_sample > 0 {
            interval_start = (current_sample - 1) as f64 * SAMPLE_STEP;
        }

        let previous = self.samples[current_sample.saturating_sub(1)];
        let next = self.samples[current_sample.min(SAMPLE_COUNT - 1)];

        let distance = if next > previous {
            (x - previous) / (next - previous)
        } else {
            0.0
        };

        let guess = interval_start + distance * SAMPLE_STEP;

        let slope = sample_curve_derivative(
            coefficient_a(self.x1, self.x2),
            coefficient_b(self.x1, self.x2),
            coefficient_c(self.x1),
            guess,
        );

        if slope >= NEWTON_MIN_SLOPE {
            self.newton_raphson(x, guess)
        } else if slope == 0.0 {
            guess
        } else {
            self.binary_subdivide(
                x,
                interval_start,
                interval_start + SAMPLE_STEP,
            )
        }
    }

    fn newton_raphson(&self, x: f64, mut guess: f64) -> f64 {
        let ax = coefficient_a(self.x1, self.x2);
        let bx = coefficient_b(self.x1, self.x2);
        let cx = coefficient_c(self.x1);

        for _ in 0..NEWTON_ITERATIONS {
            let slope = sample_curve_derivative(ax, bx, cx, guess);

            if slope == 0.0 {
                return guess;
            }

            guess -= (sample_curve(ax, bx, cx, guess) - x) / slope;
        }

        guess
    }

    fn binary_subdivide(&self, x: f64, mut a: f64, mut b: f64) -> f64 {
        let ax = coefficient_a(self.x1, self.x2);
        let bx = coefficient_b(self.x1, self.x2);
        let cx = coefficient_c(self.x1);

        let mut current_t = 0.0;

        for _ in 0..SUBDIVISION_MAX_ITERATIONS {
            current_t = a + (b - a) / 2.0;

            let current_x = sample_curve(ax, bx, cx, current_t) - x;

            if current_x.abs() < SUBDIVISION_PRECISION {
                break;
            }

            if current_x > 0.0 {
                b = current_t;
            } else {
                a = current_t;
            }
        }

        current_t
    }
}

/// A two-axis [`CubicBezier`]-based smooth scroll animation.
///
/// This is a port of Gecko's `ScrollAnimationBezierPhysics`: the physics
/// backing `ScrollMode::Smooth`.
#[derive(Debug, Clone)]
pub struct BezierPhysics {
    settings: SmoothScrollSettings,
    timing_x: CubicBezier,
    timing_y: CubicBezier,
    start_pos: Vector,
    destination: Vector,
    start_time: Instant,
    duration: Duration,
    prev_event_time: [Instant; 3],
    is_first_iteration: bool,
}

impl BezierPhysics {
    /// Creates a new [`BezierPhysics`] with the given [`SmoothScrollSettings`]
    /// and starting position.
    pub fn new(settings: SmoothScrollSettings, start_pos: Vector) -> Self {
        let epoch = Instant::now();

        Self {
            settings,
            timing_x: CubicBezier::new(0.0, 0.0, 0.0, 1.0),
            timing_y: CubicBezier::new(0.0, 0.0, 0.0, 1.0),
            start_pos,
            destination: Vector::ZERO,
            start_time: epoch,
            duration: Duration::ZERO,
            prev_event_time: [epoch; 3],
            is_first_iteration: true,
        }
    }

    /// The settings currently in use.
    pub fn settings(&self) -> SmoothScrollSettings {
        self.settings
    }

    /// Starts, extends, or retargets the animation towards `destination` at
    /// `now`.
    ///
    /// When the animation is already running, its current position and
    /// velocity are carried over so that consecutive scroll events blend into a
    /// single continuous motion.
    pub fn update(&mut self, now: Instant, destination: Vector) {
        if self.is_first_iteration {
            self.initialize_history(now);
        }

        let duration = self.compute_duration(now);
        let mut current_velocity = Vector::ZERO;

        if !self.is_first_iteration {
            // If an additional event has not changed the destination, then do
            // not let another minimum duration reset slow things down. If it
            // would, then instead continue with the existing timing function.
            if destination == self.destination
                && now + duration > self.start_time + self.duration
            {
                return;
            }

            current_velocity = self.velocity_at(now);
            self.start_pos = self.position_at(now);
        }

        self.start_time = now;
        self.duration = duration;
        self.destination = destination;
        self.timing_x = self.init_timing(
            self.start_pos.x,
            current_velocity.x,
            destination.x,
        );
        self.timing_y = self.init_timing(
            self.start_pos.y,
            current_velocity.y,
            destination.y,
        );
        self.is_first_iteration = false;
    }

    /// Returns the animated position at `now`.
    pub fn position_at(&self, now: Instant) -> Vector {
        if self.is_finished(now) {
            return self.destination;
        }

        let progress = self.progress_at(now);

        let progress_x = self.timing_x.value_at(progress);
        let progress_y = self.timing_y.value_at(progress);

        Vector::new(
            lerp(
                self.start_pos.x as f64,
                self.destination.x as f64,
                progress_x,
            ) as f32,
            lerp(
                self.start_pos.y as f64,
                self.destination.y as f64,
                progress_y,
            ) as f32,
        )
    }

    /// Returns the velocity of the animation at `now`, in units per second.
    pub fn velocity_at(&self, now: Instant) -> Vector {
        if self.is_finished(now) {
            return Vector::ZERO;
        }

        let progress = self.progress_at(now);

        Vector::new(
            self.velocity_component(
                progress,
                self.timing_x,
                self.start_pos.x,
                self.destination.x,
            ),
            self.velocity_component(
                progress,
                self.timing_y,
                self.start_pos.y,
                self.destination.y,
            ),
        )
    }

    /// Returns whether the animation has reached its destination at `now`.
    pub fn is_finished(&self, now: Instant) -> bool {
        self.start_time + self.duration <= now
    }

    /// The destination the animation is heading towards.
    pub fn destination(&self) -> Vector {
        self.destination
    }

    fn progress_at(&self, now: Instant) -> f64 {
        if self.duration.is_zero() {
            return 1.0;
        }

        ((now - self.start_time).as_secs_f64() / self.duration.as_secs_f64())
            .clamp(0.0, 1.0)
    }

    fn velocity_component(
        &self,
        progress: f64,
        timing: CubicBezier,
        start: f32,
        destination: f32,
    ) -> f32 {
        let (dt, dxy) = timing.derivative_at(progress);

        if dt == 0.0 {
            return if dxy >= 0.0 { f32::MAX } else { f32::MIN };
        }

        let slope = dxy / dt;

        (slope * (destination as f64 - start as f64)
            / self.duration.as_secs_f64()) as f32
    }

    fn compute_duration(&mut self, now: Instant) -> Duration {
        // Average the last 3 deltas between events. The desired effect is to
        // use a longer duration when scrolling slowly, such that it is easier
        // to follow, but reduce the duration to make it feel snappier when
        // scrolling quickly.
        let events_delta_ms =
            ((now - self.prev_event_time[2]).as_millis() / 3) as i32;

        self.prev_event_time[2] = self.prev_event_time[1];
        self.prev_event_time[1] = self.prev_event_time[0];
        self.prev_event_time[0] = now;

        let duration_ms =
            (events_delta_ms as f64 * self.settings.interval_ratio).clamp(
                self.settings.duration_min_ms as f64,
                self.settings.duration_max_ms as f64,
            ) as i32;

        Duration::from_millis(duration_ms.max(0) as u64)
    }

    fn initialize_history(&mut self, now: Instant) {
        // Starting a new scroll (i.e. not extending an existing animation)
        // creates imaginary previous timestamps with the maximum relevant
        // interval between them, which yields the maximum duration.
        let ratio = self.settings.interval_ratio.max(1.0);
        let max_delta = Duration::from_millis(
            (self.settings.duration_max_ms.max(0) as f64 / ratio) as u64,
        );

        self.prev_event_time[0] = now - max_delta;
        self.prev_event_time[1] = self.prev_event_time[0] - max_delta;
        self.prev_event_time[2] = self.prev_event_time[1] - max_delta;
    }

    /// Builds the timing function for one axis, mirroring Firefox's
    /// `InitTimingFunction`.
    fn init_timing(
        &self,
        current_pos: f32,
        current_velocity: f32,
        destination: f32,
    ) -> CubicBezier {
        let stop_weighting = self.settings.stop_deceleration_weighting;
        let velocity_weighting = self.settings.current_velocity_weighting;

        if destination == current_pos || velocity_weighting == 0.0 {
            return CubicBezier::new(0.0, 0.0, 1.0 - stop_weighting, 1.0);
        }

        let duration_secs = self.duration.as_secs_f64();

        if duration_secs == 0.0 {
            return CubicBezier::new(0.0, 0.0, 1.0 - stop_weighting, 1.0);
        }

        let slope = current_velocity as f64 * duration_secs
            / (destination as f64 - current_pos as f64);

        let normalization = (1.0 + slope * slope).sqrt();

        let dt = 1.0 / normalization * velocity_weighting;
        let dxy = slope / normalization * velocity_weighting;

        CubicBezier::new(dt, dxy, 1.0 - stop_weighting, 1.0)
    }
}

fn lerp(start: f64, end: f64, progress: f64) -> f64 {
    start + (end - start) * progress
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_bezier_is_linear() {
        let curve = CubicBezier::new(0.25, 0.25, 0.75, 0.75);

        for i in 0..=10 {
            let x = i as f64 / 10.0;

            assert!((curve.value_at(x) - x).abs() < 1e-9);
        }
    }

    #[test]
    fn ease_in_out_endpoints() {
        let curve =
            CubicBezier::new(0.0, 0.0, 1.0 - STOP_DECELERATION_WEIGHTING, 1.0);

        assert_eq!(curve.value_at(0.0), 0.0);
        assert_eq!(curve.value_at(1.0), 1.0);
        assert!((0.0..=1.0).contains(&curve.value_at(0.5)));
    }

    #[test]
    fn animation_reaches_destination() {
        let now = Instant::now();
        let mut physics = BezierPhysics::new(
            SmoothScrollSettings::mouse_wheel(),
            Vector::ZERO,
        );

        physics.update(now, Vector::new(0.0, 500.0));

        let finished = now + Duration::from_secs(1);

        assert!(physics.is_finished(finished));
        assert_eq!(physics.position_at(finished), Vector::new(0.0, 500.0));
    }

    #[test]
    fn duration_respects_bounds() {
        let now = Instant::now();
        let settings = SmoothScrollSettings::mouse_wheel();
        let mut physics = BezierPhysics::new(settings, Vector::ZERO);

        // The first update always uses the maximum duration because of the
        // imaginary event history.
        physics.update(now, Vector::new(0.0, 100.0));

        assert!(
            physics.duration.as_millis() <= settings.duration_max_ms as u128
        );
        assert!(
            physics.duration.as_millis() >= settings.duration_min_ms as u128
        );
    }

    #[test]
    fn animation_moves_towards_destination() {
        let now = Instant::now();
        let mut physics = BezierPhysics::new(
            SmoothScrollSettings::mouse_wheel(),
            Vector::ZERO,
        );

        physics.update(now, Vector::new(0.0, 100.0));

        let midway = physics.position_at(now + physics.duration / 2);

        assert!(midway.y > 0.0);
        assert!(midway.y < 100.0);
    }

    #[test]
    fn extending_keeps_velocity() {
        let now = Instant::now();
        let mut physics = BezierPhysics::new(
            SmoothScrollSettings::mouse_wheel(),
            Vector::ZERO,
        );

        physics.update(now, Vector::new(0.0, 100.0));

        let later = now + Duration::from_millis(10);
        let velocity = physics.velocity_at(later);

        assert!(velocity.y > 0.0);

        physics.update(later, Vector::new(0.0, 200.0));

        assert_eq!(physics.destination(), Vector::new(0.0, 200.0));
    }

    #[test]
    fn repeated_same_destination_does_not_restart() {
        let now = Instant::now();
        let mut physics = BezierPhysics::new(
            SmoothScrollSettings::mouse_wheel(),
            Vector::ZERO,
        );

        physics.update(now, Vector::new(0.0, 100.0));

        let start_time = physics.start_time;
        let duration = physics.duration;

        physics.update(now + Duration::from_millis(1), Vector::new(0.0, 100.0));

        // Either the animation is retargeted with the same timing (no reset)
        // or it is left untouched; in both cases the end time must not be
        // pushed further out.
        assert!(physics.start_time + physics.duration <= start_time + duration);
    }
}
