//! Animations, all switchable, from `core/anim.py`: Settings → Animations has a master switch, a speed, a style
//! and one switch per kind, and Performance mode turns every one of them off.
//!
//! Immediate mode needs no animation objects: a widget asks for the value it should show now with
//! [`value`], passing where it wants to end up. While the value is still moving the call asks for one more
//! frame, so a window repaints only while something in it moves and goes quiet the moment it arrives. With the
//! kind switched off the target comes back at once and nothing repaints.

use std::sync::Arc;

use egui::{Context, Id};
use lum_core::settings::Settings;

/// The kinds Settings → Animations has a switch for, as `KINDS` in `core/anim.py`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Overlay,
    Pages,
    Nav,
    Boxes,
    Theme,
    Toasts,
    Panels,
    Zoom,
    Pulses,
}

impl Kind {
    pub const ALL: [Kind; 9] = [
        Kind::Overlay,
        Kind::Pages,
        Kind::Nav,
        Kind::Boxes,
        Kind::Theme,
        Kind::Toasts,
        Kind::Panels,
        Kind::Zoom,
        Kind::Pulses,
    ];

    /// The key under `animations.` in settings.json.
    pub fn key(self) -> &'static str {
        match self {
            Kind::Overlay => "overlay",
            Kind::Pages => "pages",
            Kind::Nav => "nav",
            Kind::Boxes => "boxes",
            Kind::Theme => "theme",
            Kind::Toasts => "toasts",
            Kind::Panels => "panels",
            Kind::Zoom => "zoom",
            Kind::Pulses => "pulses",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Style {
    /// A gentle ease out.
    #[default]
    Smooth,
    /// Expressive: things that move overshoot a touch and settle, like Caelestia's shell.
    Caelestia,
    /// Quick start, short.
    Snappy,
}

impl Style {
    fn from_key(key: &str) -> Style {
        match key {
            "caelestia" => Style::Caelestia,
            "snappy" => Style::Snappy,
            _ => Style::Smooth,
        }
    }

    /// How much longer or shorter this style makes everything.
    fn length(self) -> f32 {
        match self {
            Style::Smooth => 1.0,
            Style::Caelestia => 1.25,
            Style::Snappy => 0.75,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Speed {
    Fast,
    #[default]
    Normal,
    Relaxed,
}

impl Speed {
    fn from_key(key: &str) -> Speed {
        match key {
            "fast" => Speed::Fast,
            "relaxed" => Speed::Relaxed,
            _ => Speed::Normal,
        }
    }

    fn factor(self) -> f32 {
        match self {
            Speed::Fast => 0.6,
            Speed::Normal => 1.0,
            Speed::Relaxed => 1.6,
        }
    }
}

/// The shape of a movement. `Out` and `Move` follow the chosen style; the others are picked on purpose and
/// stay as they are (`_curve` in Python only swaps the default ease out).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Ease {
    /// Fades and other values: Smooth eases out (cubic), Caelestia longer (quint), Snappy fast (expo).
    #[default]
    Out,
    /// Things that move or change size: like `Out`, but Caelestia overshoots a little and settles.
    Move,
    /// Speeds up towards the end, for fading something out.
    In,
    Linear,
}

/// What Settings → Animations says, read once per settings change and kept in the context.
#[derive(Debug, Clone, PartialEq)]
pub struct Motion {
    pub enabled: bool,
    pub performance: bool,
    pub speed: Speed,
    pub style: Style,
    kinds: [bool; 9],
}

impl Default for Motion {
    fn default() -> Motion {
        Motion {
            enabled: true,
            performance: false,
            speed: Speed::Normal,
            style: Style::Smooth,
            kinds: [true; 9],
        }
    }
}

fn motion_id() -> Id {
    Id::unique("lum-motion")
}

impl Motion {
    pub fn from_settings(settings: &Settings) -> Motion {
        let mut kinds = [true; 9];
        for (on, kind) in kinds.iter_mut().zip(Kind::ALL) {
            *on = settings.get_bool(&format!("animations.{}", kind.key()), true);
        }
        Motion {
            enabled: settings.get_bool("animations.enabled", true),
            performance: settings.get_bool("ui.performance_mode", false),
            speed: Speed::from_key(settings.get_str("animations.speed").unwrap_or("normal")),
            style: Style::from_key(settings.get_str("animations.style").unwrap_or("smooth")),
            kinds,
        }
    }

    /// Everything off, as in performance mode (tests and snapshots use it so they never wait).
    pub fn off() -> Motion {
        Motion { enabled: false, ..Motion::default() }
    }

    pub fn of(ctx: &Context) -> Arc<Motion> {
        ctx.data(|d| d.get_temp::<Arc<Motion>>(motion_id())).unwrap_or_default()
    }

    pub fn install(self, ctx: &Context) {
        ctx.data_mut(|d| d.insert_temp(motion_id(), Arc::new(self)));
    }

    /// Is this kind of animation wanted right now? Like `anim.on(kind)`.
    pub fn on(&self, kind: Kind) -> bool {
        self.enabled
            && !self.performance
            && self.kinds[Kind::ALL.iter().position(|k| *k == kind).unwrap_or(0)]
    }

    pub fn set(&mut self, kind: Kind, on: bool) {
        if let Some(i) = Kind::ALL.iter().position(|k| *k == kind) {
            self.kinds[i] = on;
        }
    }

    /// A duration in seconds from a base in milliseconds, scaled by the speed and the style, like `anim.ms`.
    pub fn secs(&self, base_ms: f32) -> f32 {
        (base_ms * self.speed.factor() * self.style.length()).max(1.0) / 1000.0
    }

    fn curve(&self, ease: Ease) -> fn(f32) -> f32 {
        match (ease, self.style) {
            (Ease::Linear, _) => linear,
            (Ease::In, _) => in_cubic,
            (Ease::Out | Ease::Move, Style::Smooth) => out_cubic,
            (Ease::Out | Ease::Move, Style::Snappy) => out_expo,
            (Ease::Out, Style::Caelestia) => out_quint,
            (Ease::Move, Style::Caelestia) => out_back,
        }
    }
}

fn linear(t: f32) -> f32 {
    t
}

fn in_cubic(t: f32) -> f32 {
    t * t * t
}

fn out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

fn out_quint(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(5)
}

fn out_expo(t: f32) -> f32 {
    if t >= 1.0 { 1.0 } else { 1.0 - 2f32.powf(-10.0 * t) }
}

/// Qt's OutBack with its default overshoot of 1.70158 (about 10 %).
fn out_back(t: f32) -> f32 {
    const S: f32 = 1.701_58;
    let t = t - 1.0;
    t * t * ((S + 1.0) * t + S) + 1.0
}

#[derive(Debug, Clone, Copy)]
struct Track {
    from: f32,
    to: f32,
    /// `ctx.input().time` when it started.
    started: f64,
    secs: f32,
    ease: Ease,
}

impl Track {
    fn at(&self, now: f64, motion: &Motion) -> (f32, bool) {
        let t = ((now - self.started) as f32 / self.secs).clamp(0.0, 1.0);
        let eased = (motion.curve(self.ease))(t);
        (self.from + (self.to - self.from) * eased, t < 1.0)
    }
}

/// The value to show now for something heading to `target`. The first call for an `id` starts at the target
/// (nothing to animate from); later changes of `target` animate from wherever the value is.
pub fn value(ctx: &Context, id: Id, target: f32, kind: Kind, base_ms: f32, ease: Ease) -> f32 {
    let on = Motion::of(ctx).on(kind);
    run(ctx, id, None, target, on, base_ms, ease)
}

/// A small state change inside a control (a switch's knob, a chevron turning): about 120 ms, as the UI brief
/// asks of every animation. It has no switch of its own; the master switch and performance mode still stop it.
pub fn small(ctx: &Context, id: Id, target: f32) -> f32 {
    let motion = Motion::of(ctx);
    run(ctx, id, None, target, motion.enabled && !motion.performance, 120.0, Ease::Out)
}

/// Like [`value`], but the first call starts from `start`: a panel fading in as it opens.
pub fn value_from(
    ctx: &Context,
    id: Id,
    start: f32,
    target: f32,
    kind: Kind,
    base_ms: f32,
    ease: Ease,
) -> f32 {
    let on = Motion::of(ctx).on(kind);
    run(ctx, id, Some(start), target, on, base_ms, ease)
}

fn run(ctx: &Context, id: Id, start: Option<f32>, target: f32, on: bool, base_ms: f32, ease: Ease) -> f32 {
    let motion = Motion::of(ctx);
    if !on {
        ctx.data_mut(|d| d.remove::<Track>(id));
        return target;
    }
    let now = ctx.input(|i| i.time);
    let previous = ctx.data(|d| d.get_temp::<Track>(id));
    let track = match previous {
        Some(track) if track.to == target => track,
        Some(track) => {
            // the target moved while it was on its way (or after): carry on from where it is now
            let (here, _) = track.at(now, &motion);
            Track { from: here, to: target, started: now, secs: motion.secs(base_ms), ease }
        }
        None => Track {
            from: start.unwrap_or(target),
            to: target,
            started: now,
            secs: motion.secs(base_ms),
            ease,
        },
    };
    let (shown, moving) = track.at(now, &motion);
    ctx.data_mut(|d| d.insert_temp(id, track));
    if moving {
        ctx.request_repaint();
    }
    shown
}

/// Something with this id is still on its way.
pub fn is_moving(ctx: &Context, id: Id) -> bool {
    let now = ctx.input(|i| i.time);
    let motion = Motion::of(ctx);
    ctx.data(|d| d.get_temp::<Track>(id)).is_some_and(|t| t.at(now, &motion).1)
}

/// Forgets an animation, so the next [`value_from`] for this id starts over (a panel that opens again).
pub fn forget(ctx: &Context, id: Id) {
    ctx.data_mut(|d| d.remove::<Track>(id));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pass(ctx: &Context, time: f64, mut f: impl FnMut(&Context)) -> bool {
        let input = egui::RawInput { time: Some(time), ..Default::default() };
        let out = crate::headless_pass(ctx, input, |ui| f(ui.ctx()));
        out.viewport_output.values().any(|v| v.repaint_delay.is_zero())
    }

    #[test]
    fn moves_only_while_it_has_somewhere_to_go() {
        let ctx = Context::default();
        let id = Id::unique("fade");
        let mut seen = 0.0;
        // the first frame starts at 0 and asks for more frames
        assert!(pass(&ctx, 0.0, |c| seen = value_from(c, id, 0.0, 1.0, Kind::Panels, 100.0, Ease::Linear)));
        assert_eq!(seen, 0.0);
        assert!(pass(&ctx, 0.05, |c| seen = value_from(c, id, 0.0, 1.0, Kind::Panels, 100.0, Ease::Linear)));
        assert!((seen - 0.5).abs() < 1e-4, "{seen}");
        // arrived: the value stays, and after the one extra frame egui always runs after a request, nothing
        // asks for another
        pass(&ctx, 0.2, |c| seen = value_from(c, id, 0.0, 1.0, Kind::Panels, 100.0, Ease::Linear));
        assert_eq!(seen, 1.0);
        assert!(!pass(&ctx, 0.3, |c| seen = value(c, id, 1.0, Kind::Panels, 100.0, Ease::Linear)));
        assert!(!pass(&ctx, 5.0, |c| seen = value(c, id, 1.0, Kind::Panels, 100.0, Ease::Linear)));
    }

    #[test]
    fn switched_off_means_the_end_value_at_once() {
        let ctx = Context::default();
        let mut motion = Motion::default();
        motion.set(Kind::Panels, false);
        motion.install(&ctx);
        let mut seen = 0.0;
        let id = Id::unique("fade");
        // the very first pass of a context always asks for a second one; skip it
        pass(&ctx, 0.0, |_| {});
        pass(&ctx, 0.01, |_| {});
        assert!(!pass(&ctx, 0.02, |c| seen = value_from(c, id, 0.0, 1.0, Kind::Panels, 100.0, Ease::Out)));
        assert_eq!(seen, 1.0);

        Motion { performance: true, ..Motion::default() }.install(&ctx);
        let b = Id::unique("b");
        assert!(!pass(&ctx, 0.03, |c| seen = value_from(c, b, 0.0, 1.0, Kind::Boxes, 100.0, Ease::Out)));
        assert_eq!(seen, 1.0);
    }

    #[test]
    fn speed_and_style_scale_the_length_like_python() {
        let m = Motion { speed: Speed::Fast, style: Style::Caelestia, ..Motion::default() };
        // int(180 * 0.6 * 1.25) = 135 ms
        assert!((m.secs(180.0) - 0.135).abs() < 1e-6);
        let m = Motion { speed: Speed::Relaxed, style: Style::Snappy, ..Motion::default() };
        assert!((m.secs(100.0) - 0.12).abs() < 1e-6);
    }

    #[test]
    fn curves() {
        for f in [linear, in_cubic, out_cubic, out_quint, out_expo, out_back] {
            assert!(f(0.0).abs() < 1e-3 && (f(1.0) - 1.0).abs() < 1e-3);
        }
        let peak = (0..=100).map(|i| out_back(i as f32 / 100.0)).fold(0.0, f32::max);
        assert!(peak > 1.05 && peak < 1.15, "Caelestia overshoots by about 10 %: {peak}");
    }
}
