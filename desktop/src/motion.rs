//! Springs, after Apple's "Designing Fluid Interfaces".
//!
//! A spring has no duration: it has a damping ratio (1.0 = no overshoot) and
//! a response (seconds to reach the target, roughly). Everything a person
//! can touch moves on a spring, always starting from its live value and
//! velocity, so it can be grabbed and reversed at any instant.

use egui::{Context, Id};
use std::f32::consts::TAU;

#[derive(Clone, Copy, Debug)]
pub struct SpringCfg {
    /// Seconds. Lower is snappier. Not a duration.
    pub response: f32,
    /// 1.0 is critically damped; below 1.0 overshoots.
    pub damping: f32,
}

impl SpringCfg {
    pub const fn new(response: f32, damping: f32) -> Self {
        SpringCfg { response, damping }
    }
}

/// Default for UI state changes: critically damped, no bounce.
pub const SMOOTH: SpringCfg = SpringCfg::new(0.32, 1.0);
/// Hover and press feedback.
pub const QUICK: SpringCfg = SpringCfg::new(0.18, 1.0);
/// Reposition (Apple ships 1.0 / 0.4 for this).
pub const MOVE: SpringCfg = SpringCfg::new(0.4, 1.0);
/// Only after a gesture that carried momentum (a flick or a drag release).
pub const MOMENTUM: SpringCfg = SpringCfg::new(0.3, 0.8);

#[derive(Clone, Copy, Debug, Default)]
pub struct Spring {
    pub value: f32,
    pub velocity: f32,
    pub target: f32,
}

impl Spring {
    pub fn at(value: f32) -> Spring {
        Spring {
            value,
            velocity: 0.0,
            target: value,
        }
    }

    /// Advance by `dt` seconds. Sub-steps keep it stable at any frame rate.
    pub fn step(&mut self, dt: f32, cfg: SpringCfg) {
        let omega = TAU / cfg.response.max(0.01);
        let k = omega * omega;
        let c = 2.0 * cfg.damping * omega;
        let mut left = dt.clamp(0.0, 0.05);
        while left > 0.0 {
            let h = left.min(1.0 / 240.0);
            let a = -k * (self.value - self.target) - c * self.velocity;
            self.velocity += a * h;
            self.value += self.velocity * h;
            left -= h;
        }
    }

    pub fn at_rest(&self) -> bool {
        (self.value - self.target).abs() < 0.0005 && self.velocity.abs() < 0.005
    }
}

/// Where a gesture would come to rest if released now, Apple's projection
/// (`decelerationRate` 0.998). `velocity` is in units per second.
pub fn project(velocity: f32) -> f32 {
    const RATE: f32 = 0.998;
    (velocity / 1000.0) * RATE / (1.0 - RATE)
}

/// Progressive resistance past a bound: the further, the less it follows.
pub fn rubber_band(overshoot: f32, dimension: f32) -> f32 {
    const C: f32 = 0.55;
    let d = dimension.max(1.0);
    (overshoot * d * C) / (d + C * overshoot.abs())
}

/// Animate a value toward `target`, persisting the spring in egui memory.
/// With reduced motion the value jumps; callers that want a gentler
/// equivalent cross-fade with [`fade`].
pub fn animate(ctx: &Context, id: Id, target: f32, cfg: SpringCfg, reduced: bool) -> f32 {
    let dt = ctx.input(|i| i.unstable_dt);
    let mut s: Spring = ctx
        .data_mut(|d| d.get_temp(id))
        .unwrap_or_else(|| Spring::at(target));
    s.target = target;
    if reduced {
        s.value = target;
        s.velocity = 0.0;
    } else {
        s.step(dt, cfg);
    }
    if !s.at_rest() {
        ctx.request_repaint();
    } else {
        s.value = s.target;
        s.velocity = 0.0;
    }
    ctx.data_mut(|d| d.insert_temp(id, s));
    s.value
}

/// Read and write a persisted spring directly (for drags that set velocity).
pub fn with_spring<R>(ctx: &Context, id: Id, init: f32, f: impl FnOnce(&mut Spring) -> R) -> R {
    let mut s: Spring = ctx
        .data_mut(|d| d.get_temp(id))
        .unwrap_or_else(|| Spring::at(init));
    let r = f(&mut s);
    ctx.data_mut(|d| d.insert_temp(id, s));
    r
}

/// Opacity cross-fade, the reduced-motion replacement for slides and scales.
/// Always short and never overshoots.
pub fn fade(ctx: &Context, id: Id, on: bool) -> f32 {
    animate(ctx, id, if on { 1.0 } else { 0.0 }, QUICK, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settle(cfg: SpringCfg) -> (f32, f32) {
        let mut s = Spring::at(0.0);
        s.target = 1.0;
        let mut peak: f32 = 0.0;
        let mut t = 0.0;
        while t < 3.0 {
            s.step(1.0 / 120.0, cfg);
            peak = peak.max(s.value);
            t += 1.0 / 120.0;
        }
        (s.value, peak)
    }

    #[test]
    fn critically_damped_never_overshoots() {
        let (end, peak) = settle(SMOOTH);
        assert!((end - 1.0).abs() < 0.001);
        assert!(peak < 1.001, "peak {peak}");
    }

    #[test]
    fn momentum_spring_overshoots_then_settles() {
        let (end, peak) = settle(MOMENTUM);
        assert!((end - 1.0).abs() < 0.001);
        assert!(peak > 1.005, "peak {peak}");
    }

    #[test]
    fn retarget_keeps_velocity() {
        let mut s = Spring::at(0.0);
        s.target = 1.0;
        for _ in 0..10 {
            s.step(1.0 / 120.0, SMOOTH);
        }
        let v = s.velocity;
        assert!(v > 0.0);
        s.target = 0.0; // reverse mid-flight
        s.step(1.0 / 120.0, SMOOTH);
        assert!(s.velocity < v, "velocity bends, it does not snap");
        assert!(s.velocity > -v, "no hard cut to the opposite direction");
    }

    #[test]
    fn projection_and_rubber_band() {
        assert!((project(1000.0) - 499.0).abs() < 1.0);
        assert!(rubber_band(100.0, 300.0) < 100.0);
        assert!(
            rubber_band(1000.0, 300.0) < 300.0,
            "never exceeds the dimension"
        );
        assert!(
            rubber_band(1000.0, 300.0) > rubber_band(100.0, 300.0),
            "keeps resisting, monotonic"
        );
    }
}
