/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! The chatview scroll controller: physics only.
//!
//! The position is internal state — a plain `f32` of px from the live
//! content bottom (0 = bottom, growing up into history) — never a scene
//! property. Recognition, slop, long-press timers, and velocity
//! sampling belong to the gesture session; this controller consumes
//! intents (drag lifecycle, `DragEnd` velocity, page ticks) and owns
//! the motion: 1:1 drags, flick glide with exponential decay, eased
//! page animations, clamping, height-change compensation, and
//! anchor-based save/restore.

use std::time::{Duration, Instant};

use super::MessageId;

macro_rules! t { ($($arg:tt)*) => { trace!(target: "ui::chatview::scroll", $($arg)*); } }

/// Release velocity below this (px/sec) is not a flick.
const FLICK_MIN_VEL: f32 = 50.;
/// Exponential glide decay rate (1/sec), matching the old chatview's
/// 0.9-per-10ms resistance.
const GLIDE_DECEL: f32 = 10.5;
/// Glide stops when the velocity decays below this (px/sec).
const GLIDE_STOP_VEL: f32 = 20.;
/// Page animation duration (wheel ticks, PageUp/PageDown).
const ANIM_MS: u64 = 250;
/// scroll within this many px of 0 counts as at the live bottom.
const BOTTOM_EPSILON: f32 = 0.5;
/// Animator wake cadence for motion frames (glide and animation).
const GLIDE_FRAME: Duration = Duration::from_millis(16);

/// What is moving the content right now.
#[derive(Debug, Clone, PartialEq)]
pub enum ScrollState {
    Idle,
    /// 1:1 finger tracking; `scroll0` is the position at grab time.
    Drag {
        start_y: f32,
        scroll0: f32,
    },
    /// Inertial glide, decaying from the release velocity (px/sec).
    Glide {
        velocity: f32,
    },
    /// Eased animation toward `to` (wheel/PageUp/PageDown).
    Anim {
        from: f32,
        to: f32,
        started: Instant,
    },
}

/// Serialized "what the user is looking at", used only at
/// save/restore boundaries (channel exit/entry, reflow). Runtime
/// stability is compensation's job, never the anchor's.
#[derive(Debug, Clone, PartialEq)]
pub struct Anchor {
    /// The oldest visible message (top edge at/above the viewport
    /// top). None = bottom (scroll == 0) or nothing visible.
    pub msg: Option<MessageId>,
    /// How far the viewport top sits below the anchor message's top,
    /// in px: `dy = (scroll + view_h) - pos_of(msg)`. 0 = the message's
    /// top is exactly at the viewport top.
    pub dy: f32,
}

impl Anchor {
    /// The bottom-pinned anchor.
    pub fn bottom() -> Self {
        Self { msg: None, dy: 0. }
    }
}

/// The scroll controller. Geometry inputs (`view_h`, content height)
/// are fed in by the chatview; the controller never reads the scene.
#[derive(Debug, Clone)]
pub struct ScrollController {
    state: ScrollState,
    /// Px from the live content bottom; 0 = live bottom.
    scroll: f32,
    /// `total_height - view_h`, maintained via [`Self::set_content`].
    max_scroll: f32,
    /// Timestamp of the last `tick`, for glide deltas.
    last_tick: Instant,
}

impl Default for ScrollController {
    fn default() -> Self {
        Self { state: ScrollState::Idle, scroll: 0., max_scroll: 0., last_tick: Instant::now() }
    }
}

impl ScrollController {
    pub fn new() -> Self {
        Self::default()
    }

    /// The current position: px from the live content bottom.
    pub fn scroll(&self) -> f32 {
        self.scroll
    }

    /// The current motion state.
    pub fn state(&self) -> &ScrollState {
        &self.state
    }

    /// Whether the view sits at the live bottom (drives the
    /// `is_at_bottom` scene property).
    pub fn is_at_bottom(&self) -> bool {
        self.scroll <= BOTTOM_EPSILON
    }

    /// Feed the content geometry: `total` is the loaded content height,
    /// `view_h` the viewport height. Updates the clamp range and
    /// re-clamps the current position (and any animation target).
    pub fn set_content(&mut self, total: f32, view_h: f32) {
        self.max_scroll = (total - view_h).max(0.);
        self.scroll = self.clamp(self.scroll);
        if let ScrollState::Anim { to, .. } = &mut self.state {
            let clamped = self.max_scroll.min(*to).max(0.);
            *to = clamped;
        }
    }

    /// Clamp a requested position into `[0, max_scroll]`.
    pub fn clamp(&self, scroll: f32) -> f32 {
        scroll.clamp(0., self.max_scroll)
    }

    /// A touch began over the view (gesture session `Down`): cancel any
    /// in-flight glide or animation immediately, before the touch even
    /// travels past the slop. No drag state is entered — that needs
    /// `drag_start`.
    pub fn touch_down(&mut self) {
        if self.state != ScrollState::Idle {
            t!("touch_down cancels {:?}", self.state);
        }
        self.state = ScrollState::Idle;
        self.last_tick = Instant::now();
    }

    /// The drag travelled past the slop: grab the content 1:1, killing
    /// any in-flight motion.
    pub fn drag_start(&mut self, y: f32) {
        t!("drag_start(y={y}) from scroll={}", self.scroll);
        self.state = ScrollState::Drag { start_y: y, scroll0: self.scroll };
        self.last_tick = Instant::now();
    }

    /// 1:1 drag tracking. Chat scroll grows back into history, so the
    /// finger offset adds: `scroll = scroll0 + dy`. Returns the applied
    /// (clamped) position.
    pub fn drag_move(&mut self, y: f32) -> f32 {
        let ScrollState::Drag { start_y, scroll0 } = self.state else { return self.scroll };
        self.scroll = self.clamp(scroll0 + (y - start_y));
        self.scroll
    }

    /// The drag ended. `velocity` is the session's `DragEnd` velocity
    /// (px/sec on the chat axis); above the flick threshold it becomes
    /// a decaying glide, otherwise the content just stays put.
    pub fn drag_end(&mut self, velocity: f32) {
        if matches!(self.state, ScrollState::Drag { .. }) {
            self.state = ScrollState::Idle;
        }
        if velocity.abs() >= FLICK_MIN_VEL {
            t!("drag_end flick velocity={velocity}");
            self.state = ScrollState::Glide { velocity };
            self.last_tick = Instant::now();
        }
    }

    /// Wheel tick / PageUp / PageDown: animate `page` px in `dir`
    /// (+1 into history, -1 toward the bottom) with easing. Repeated
    /// ticks coalesce — the target extends from the in-flight target,
    /// never accumulating velocity.
    pub fn page_tick(&mut self, dir: f32, page: f32) {
        let base = match self.state {
            ScrollState::Anim { to, .. } => to,
            _ => self.scroll,
        };
        let to = self.clamp(base + dir * page);
        t!("page_tick(dir={dir}) target {to}");
        self.state = ScrollState::Anim { from: self.scroll, to, started: Instant::now() };
    }

    /// The down-arrow: teleport to the live bottom, cancel all motion.
    pub fn scroll_to_bottom(&mut self) {
        t!("scroll_to_bottom from {}", self.scroll);
        self.state = ScrollState::Idle;
        self.scroll = 0.;
    }

    /// Height-change compensation, applied by the chatview after the
    /// buffer reports a delta. When a message entirely below the
    /// viewport changed height and the view is not bottom-pinned, the
    /// position shifts by the delta so the viewed content stays put.
    pub fn compensate(&mut self, delta: f32, msg_below_viewport: bool) {
        if msg_below_viewport && self.scroll > 0. {
            self.scroll = self.clamp(self.scroll + delta);
        }
    }

    /// Animator advance: step Glide/Anim up to `now`, applying the
    /// frame's scroll internally. Returns the position when motion
    /// advanced this frame (including the frame that finishes it), and
    /// None when there was nothing to animate.
    pub fn tick(&mut self, now: Instant) -> Option<f32> {
        let dt = now.saturating_duration_since(self.last_tick).as_secs_f32();
        self.last_tick = now;

        match self.state.clone() {
            ScrollState::Glide { mut velocity } => {
                if dt <= 0. {
                    return None
                }
                velocity *= (-GLIDE_DECEL * dt).exp();
                self.scroll = self.clamp(self.scroll + velocity * dt);
                let hit_edge = self.scroll <= 0. || self.scroll >= self.max_scroll;
                if velocity.abs() < GLIDE_STOP_VEL || hit_edge {
                    t!("glide stopped at {}", self.scroll);
                    self.state = ScrollState::Idle;
                } else {
                    self.state = ScrollState::Glide { velocity };
                }
                Some(self.scroll)
            }
            ScrollState::Anim { from, to, started } => {
                let anim_secs = ANIM_MS as f32 / 1000.;
                let t = (now.saturating_duration_since(started).as_secs_f32() / anim_secs).min(1.);
                if t >= 1. {
                    self.scroll = to;
                    self.state = ScrollState::Idle;
                } else {
                    // Ease-out cubic.
                    let eased = 1. - (1. - t).powi(3);
                    self.scroll = from + (to - from) * eased;
                }
                Some(self.scroll)
            }
            ScrollState::Idle | ScrollState::Drag { .. } => None,
        }
    }

    /// When the animator task should wake next: on the next frame of
    /// the motion cadence. The deadline is a frame step (not the
    /// animation's end) so eased intermediate positions actually render;
    /// None when nothing moves.
    pub fn next_deadline(&self, now: Instant) -> Option<Instant> {
        match &self.state {
            ScrollState::Idle | ScrollState::Drag { .. } => None,
            ScrollState::Glide { .. } | ScrollState::Anim { .. } => Some(now + GLIDE_FRAME),
        }
    }

    /// Snapshot the current view position. `anchor_msg` is the oldest
    /// visible message (the chatview derives it from the buffer's
    /// visible window); `pos_of` resolves a message id to the px offset
    /// of its top edge from the content bottom.
    pub fn anchor(
        &self,
        view_h: f32,
        anchor_msg: Option<&MessageId>,
        mut pos_of: impl FnMut(&MessageId) -> Option<f32>,
    ) -> Anchor {
        if self.scroll <= BOTTOM_EPSILON {
            return Anchor::bottom()
        }
        let Some(msg) = anchor_msg else { return Anchor::bottom() };
        let Some(pos) = pos_of(msg) else { return Anchor::bottom() };
        Anchor { msg: Some(*msg), dy: (self.scroll + view_h) - pos }
    }

    /// Resolve an anchor against current geometry: the same content
    /// reappears at the same place. A message that no longer resolves
    /// falls back to the current position clamped — explicit, logged,
    /// never a crash. Returns the applied position.
    pub fn restore(
        &mut self,
        anchor: &Anchor,
        view_h: f32,
        mut pos_of: impl FnMut(&MessageId) -> Option<f32>,
    ) -> f32 {
        self.state = ScrollState::Idle;
        self.scroll = match &anchor.msg {
            None => 0.,
            Some(msg) => match pos_of(msg) {
                Some(pos) => self.clamp(pos + anchor.dy - view_h),
                None => {
                    t!("restore: anchor message {msg} not resolvable, clamping current position");
                    self.clamp(self.scroll)
                }
            },
        };
        self.scroll
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stepped_ticks(c: &mut ScrollController, step_ms: u64, frames: usize) {
        let mut now = Instant::now();
        for _ in 0..frames {
            now += Duration::from_millis(step_ms);
            c.tick(now);
        }
    }

    #[test]
    fn drag_tracks_one_to_one() {
        let mut c = ScrollController::new();
        c.set_content(10_000., 500.);
        c.drag_start(100.);
        assert_eq!(c.drag_move(150.), 50.);
        assert_eq!(c.drag_move(90.), 0.);
        // Finger down the screen drags the content back into history.
        c.drag_start(100.);
        assert_eq!(c.drag_move(300.), 200.);
        assert_eq!(c.drag_move(5_000.), 4_900.);
        // Clamped at the top of loaded content.
        assert_eq!(c.drag_move(20_000.), 9_500.);
    }

    #[test]
    fn grab_cancels_motion() {
        let mut c = ScrollController::new();
        c.set_content(10_000., 500.);
        c.page_tick(1., 200.);
        assert!(matches!(c.state(), ScrollState::Anim { .. }));
        c.drag_start(0.);
        assert_eq!(*c.state(), ScrollState::Drag { start_y: 0., scroll0: 0. });

        // A bare touch (no slop yet) also stops glides.
        c.drag_end(2_000.);
        assert!(matches!(c.state(), ScrollState::Glide { .. }));
        c.touch_down();
        assert_eq!(*c.state(), ScrollState::Idle);
    }

    #[test]
    fn wheel_coalescing_extends_target() {
        let mut c = ScrollController::new();
        c.set_content(10_000., 500.);

        c.page_tick(1., 100.);
        let ScrollState::Anim { to, .. } = c.state() else { panic!() };
        assert_eq!(*to, 100.);

        // A second tick before finishing extends from the target.
        c.page_tick(1., 100.);
        let ScrollState::Anim { to, .. } = c.state() else { panic!() };
        assert_eq!(*to, 200.);

        // Downward ticks reverse from the current target.
        c.page_tick(-1., 150.);
        let ScrollState::Anim { to, .. } = c.state() else { panic!() };
        assert_eq!(*to, 50.);

        // Targets stay clamped.
        c.page_tick(1., 100_000.);
        let ScrollState::Anim { to, .. } = c.state() else { panic!() };
        assert_eq!(*to, 9_500.);
    }

    #[test]
    fn anim_completes_at_target() {
        let mut c = ScrollController::new();
        c.set_content(10_000., 500.);
        c.page_tick(1., 300.);
        let start = Instant::now();
        let mid = start + Duration::from_millis(ANIM_MS / 2);
        let half = c.tick(mid).expect("mid-anim frame");
        assert!(half > 0. && half < 300., "eased halfway: {half}");

        let end = start + Duration::from_millis(ANIM_MS + 20);
        let done = c.tick(end).expect("final frame");
        assert_eq!(done, 300.);
        assert_eq!(*c.state(), ScrollState::Idle);
        assert_eq!(c.tick(end + Duration::from_millis(50)), None);
    }

    #[test]
    fn flick_decays_to_stop_within_range() {
        let mut c = ScrollController::new();
        c.set_content(100_000., 500.);
        c.drag_start(0.);
        c.drag_move(0.);
        c.drag_end(1_500.);
        stepped_ticks(&mut c, 16, 200);
        assert_eq!(*c.state(), ScrollState::Idle, "decayed to a stop");
        assert!(c.scroll() > 0., "glided into history: {}", c.scroll());
        assert!(c.scroll() <= c.max_scroll);

        // Downward flick from near the bottom clamps hard at 0.
        let mut c = ScrollController::new();
        c.set_content(100_000., 500.);
        c.drag_start(0.);
        c.drag_move(300.);
        c.drag_end(-5_000.);
        stepped_ticks(&mut c, 16, 200);
        assert_eq!(c.scroll(), 0.);
        assert_eq!(*c.state(), ScrollState::Idle);
        assert!(c.is_at_bottom());
    }

    #[test]
    fn small_release_velocity_is_not_a_flick() {
        let mut c = ScrollController::new();
        c.set_content(100_000., 500.);
        c.drag_start(0.);
        c.drag_move(800.);
        c.drag_end(30.);
        assert_eq!(*c.state(), ScrollState::Idle);
        assert_eq!(c.scroll(), 800.);
    }

    #[test]
    fn scroll_to_bottom_cancels_everything() {
        let mut c = ScrollController::new();
        c.set_content(10_000., 500.);
        c.page_tick(1., 2_000.);
        c.scroll_to_bottom();
        assert_eq!(c.scroll(), 0.);
        assert_eq!(*c.state(), ScrollState::Idle);
        assert!(c.is_at_bottom());
        assert_eq!(c.tick(Instant::now() + Duration::from_millis(100)), None);
    }

    #[test]
    fn animator_wakes_at_frame_cadence_during_anim() {
        let mut c = ScrollController::new();
        c.set_content(10_000., 500.);
        assert_eq!(c.next_deadline(Instant::now()), None);

        // The deadline is a frame step, not the animation's end, so
        // intermediate eased positions actually render.
        c.page_tick(1., 100.);
        let now = Instant::now();
        let deadline = c.next_deadline(now).unwrap();
        assert!(deadline <= now + Duration::from_millis(20), "deadline {deadline:?}");
    }

    #[test]
    fn set_content_reclamps() {
        let mut c = ScrollController::new();
        c.set_content(10_000., 500.);
        c.drag_start(0.);
        c.drag_move(5_000.);
        // Content shrank (e.g. filter reload).
        c.set_content(2_000., 500.);
        assert_eq!(c.scroll(), 1_500.);
        assert_eq!(c.max_scroll, 1_500.);
        // Content shorter than the viewport pins to 0.
        c.set_content(200., 500.);
        assert_eq!(c.scroll(), 0.);
        assert!(c.is_at_bottom());
    }

    #[test]
    fn compensation_applies_only_below_viewport_unpinned() {
        let mut c = ScrollController::new();
        c.set_content(10_000., 500.);
        c.drag_start(0.);
        c.drag_move(1_000.);

        c.compensate(50., true);
        assert_eq!(c.scroll(), 1_050.);
        c.compensate(-20., true);
        assert_eq!(c.scroll(), 1_030.);
        // In-viewport changes never move the position.
        c.compensate(500., false);
        assert_eq!(c.scroll(), 1_030.);
        // Bottom-pinned views auto-follow instead.
        c.scroll_to_bottom();
        c.compensate(50., true);
        assert_eq!(c.scroll(), 0.);
    }

    #[test]
    fn anchor_round_trip_survives_shifts() {
        // Message M's top sits at px 700; viewport [400, 900).
        fn pos_of(positions: &[(MessageId, f32)]) -> impl FnMut(&MessageId) -> Option<f32> + '_ {
            move |id: &MessageId| positions.iter().find(|(i, _)| i == id).map(|(_, p)| *p)
        }
        let mid = MessageId([7; 32]);
        let mut positions = vec![(mid, 700.)];

        let mut c = ScrollController::new();
        c.set_content(10_000., 500.);
        c.drag_start(0.);
        c.drag_move(400.);
        assert_eq!(c.scroll(), 400.);

        let anchor = c.anchor(500., Some(&mid), pos_of(&positions));
        assert_eq!(anchor.msg, Some(mid));
        assert_eq!(anchor.dy, 200.);

        // Older messages backfilled above M shift its position up.
        positions[0].1 = 750.;
        c.restore(&anchor, 500., pos_of(&positions));
        assert_eq!(c.scroll(), 450.);

        // Newer messages arriving below shift it further.
        positions[0].1 = 850.;
        c.restore(&anchor, 500., pos_of(&positions));
        assert_eq!(c.scroll(), 550.);
    }

    #[test]
    fn anchor_bottom_and_missing_fallbacks() {
        let pos_of = |id: &MessageId| (id.0[0] == 7).then_some(700_f32);

        let mut c = ScrollController::new();
        c.set_content(10_000., 500.);
        // Bottom-pinned: the anchor is the bottom shortcut.
        let anchor = c.anchor(500., Some(&MessageId([7; 32])), pos_of);
        assert_eq!(anchor, Anchor::bottom());
        c.restore(&anchor, 500., pos_of);
        assert_eq!(c.scroll(), 0.);
        assert!(c.is_at_bottom());

        // An anchor whose message vanished clamps, never panics.
        c.drag_start(0.);
        c.drag_move(2_000.);
        let gone = Anchor { msg: Some(MessageId([9; 32])), dy: 10. };
        c.restore(&gone, 500., pos_of);
        assert_eq!(c.scroll(), 2_000.);
    }
}
