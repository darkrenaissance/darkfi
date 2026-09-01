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

//! Pure gesture recognizer state machines.
//!
//! Everything here is deterministic and driven by explicit timestamps
//! so recognition behavior can be pinned by unit tests. Timers are not
//! part of the library: the session drives `Arena::long_press_due`
//! from its own timer task. Recognition decides *what* resolved and
//! *which chain node* claimed it; the session owns positions delivered
//! (start/prev/curr tracking), throttling, and coordinate translation.

use std::time::Duration;

use crate::gfx::{Point, Vector};

use super::{Axes, GestureConstants, GestureSet};

/// Tracks recent touch samples to derive a release velocity.
///
/// Every move is observed (`push`), regardless of delivery throttling,
/// but samples older than the window are dropped so the velocity
/// reflects the recent movement history only.
pub struct VelocityTracker {
    /// Sample window
    window: Duration,
    /// Samples inside the window, oldest first
    samples: Vec<(Duration, Point)>,
}

impl VelocityTracker {
    pub fn new(sample_window_ms: u32) -> Self {
        Self { window: Duration::from_millis(sample_window_ms as u64), samples: vec![] }
    }

    /// Record a sample at `t` (time since touch start).
    pub fn push(&mut self, t: Duration, pos: Point) {
        self.samples.push((t, pos));

        let cutoff = t.saturating_sub(self.window);
        while let Some((t0, _)) = self.samples.first() {
            if *t0 < cutoff {
                self.samples.remove(0);
            } else {
                break
            }
        }
    }

    /// Velocity in px/sec across the sample window. Zero when there is
    /// not enough time between the oldest and newest sample.
    pub fn velocity(&self) -> Vector {
        let (Some((t0, p0)), Some((t1, p1))) = (self.samples.first(), self.samples.last()) else {
            return Vector { x: 0., y: 0. }
        };

        let dt = t1.saturating_sub(*t0).as_secs_f32();
        if dt < 0.001 {
            return Vector { x: 0., y: 0. }
        }

        Vector { x: (p1.x - p0.x) / dt, y: (p1.y - p0.y) / dt }
    }
}

/// Gates `DragMove` delivery to at most one event per period.
pub struct MoveThrottle {
    /// Delivery period
    period: Duration,
    /// Time of the last delivered move
    last: Option<Duration>,
}

impl MoveThrottle {
    pub fn new(period_ms: u32) -> Self {
        Self { period: Duration::from_millis(period_ms as u64), last: None }
    }

    /// Whether a move at `t` (time since touch start) may be delivered.
    /// Records the delivery when it returns true.
    pub fn should_deliver(&mut self, t: Duration) -> bool {
        let due = match self.last {
            Some(last) => t.saturating_sub(last) >= self.period,
            None => true,
        };

        if due {
            self.last = Some(t);
        }

        due
    }
}

/// What a recognizer resolved, and the index of the chain node whose
/// recognizer claimed it. Positions are composed by the session.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Recognition {
    Tap,
    LongPress,
    DragStart,
    DragMove,
    DragEnd { vel: Vector },
}

/// Per-node recognizer state for one touch.
struct NodeRec {
    tap: Option<Axes>,
    tap_alive: bool,
    drag: Option<super::DragCfg>,
    drag_alive: bool,
    drag_started: bool,
    long_press_alive: bool,
    long_press_fired: bool,
}

/// The recognition arena for one touch: all recognizers configured by
/// the resolved target chain observe the stream, and the first to
/// resolve claims the gesture while competing recognizers are
/// cancelled (first-resolved-wins arbitration).
pub struct Arena {
    /// Recognition thresholds
    consts: GestureConstants,
    /// Recognizer state per chain node, chain order (root first)
    nodes: Vec<NodeRec>,
    /// Index of the node whose drag recognizer claimed the touch
    drag_claimant: Option<usize>,
}

impl Arena {
    /// Build the arena from the chain's gesture sets (root first).
    pub fn new(consts: GestureConstants, sets: &[GestureSet]) -> Self {
        let nodes = sets
            .iter()
            .map(|set| NodeRec {
                tap: set.tap.map(|cfg| cfg.axes),
                tap_alive: set.tap.is_some(),
                drag: set.drag,
                drag_alive: set.drag.is_some(),
                drag_started: false,
                long_press_alive: set.long_press.is_some(),
                long_press_fired: false,
            })
            .collect();

        Self { consts, nodes, drag_claimant: None }
    }

    /// Whether any chain node accepts a long-press, i.e. whether the
    /// session needs to arm a long-press timer for this touch.
    pub fn wants_long_press(&self) -> bool {
        self.nodes.iter().any(|n| n.long_press_alive)
    }

    /// Whether the claiming drag recognizer has started.
    pub fn drag_started(&self) -> bool {
        self.drag_claimant.is_some()
    }

    /// Observe a move. `t` is time since touch start, `delta` is the
    /// movement since touch start. Returns recognitions in delivery
    /// order (at most one).
    pub fn on_move(&mut self, t: Duration, delta: Point) -> Vec<(usize, Recognition)> {
        let _ = t;

        if let Some(claimant) = self.drag_claimant {
            return vec![(claimant, Recognition::DragMove)]
        }

        // Not dragging yet. Check drag starts deepest-first: between
        // recognizers resolving at the same moment, the descendant
        // wins ("the ancestor's drag wins over the descendant's
        // pending tap" is the tap case below; among drags the deeper
        // node is the more specific one).
        for i in (0..self.nodes.len()).rev() {
            let Some(cfg) = self.nodes[i].drag else { continue };

            if !self.nodes[i].drag_alive {
                continue
            }

            let travel = cfg.axes.travel(delta);
            if travel > cfg.threshold(self.consts.touch_slop) && cfg.direction.matches(delta) {
                self.claim_drag(i);
                return vec![(i, Recognition::DragStart)]
            }
        }

        // No drag started. Movement beyond slop cancels pending taps
        // and long-presses.
        let slop = self.consts.touch_slop;
        for node in &mut self.nodes {
            if node.tap_alive {
                let axes = node.tap.unwrap_or(Axes::Both);
                node.tap_alive = axes.travel(delta) <= slop;
            }

            if node.long_press_alive {
                node.long_press_alive = Axes::Both.travel(delta) <= slop;
            }
        }

        vec![]
    }

    /// Observe the touch ending at `t`. `vel` is the release velocity
    /// from the session's sample tracker. Returns recognitions in
    /// delivery order: `DragEnd` for an active drag, else `Tap`.
    pub fn on_up(&mut self, t: Duration, vel: Vector) -> Vec<(usize, Recognition)> {
        if let Some(claimant) = self.drag_claimant {
            return vec![(claimant, Recognition::DragEnd { vel })]
        }

        if t > Duration::from_millis(self.consts.tap_max_duration as u64) {
            return vec![]
        }

        // The descendant's tap wins over any pending ancestor tap.
        for i in (0..self.nodes.len()).rev() {
            if self.nodes[i].tap_alive {
                return vec![(i, Recognition::Tap)]
            }
        }

        vec![]
    }

    /// Called by the session's long-press timer at the timeout.
    /// Long-press fires while the finger is still down, once per
    /// touch, and cancels every pending tap.
    pub fn long_press_due(&mut self) -> Option<(usize, Recognition)> {
        if self.drag_claimant.is_some() {
            return None
        }

        for i in (0..self.nodes.len()).rev() {
            let node = &mut self.nodes[i];
            if node.long_press_alive && !node.long_press_fired {
                node.long_press_fired = true;

                for n in &mut self.nodes {
                    n.tap_alive = false;
                }

                return Some((i, Recognition::LongPress))
            }
        }

        None
    }

    fn claim_drag(&mut self, claimant: usize) {
        // Cascade cancellation: a resolved drag cancels every other
        // pending recognizer, including other nodes' drags.
        for node in &mut self.nodes {
            node.tap_alive = false;
            node.long_press_alive = false;
            node.drag_alive = false;
        }

        self.nodes[claimant].drag_started = true;
        self.drag_claimant = Some(claimant);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::ui::gesture::{Direction, DragCfg, LongPressCfg, TapCfg};

    const CONSTS: GestureConstants = GestureConstants {
        touch_slop: 10.,
        tap_max_duration: 300,
        long_press_timeout: 400,
        move_delivery_period: 20,
        sample_window_ms: 40,
    };

    fn delta(x: f32, y: f32) -> Point {
        Point::new(x, y)
    }

    fn scroll_set() -> GestureSet {
        GestureSet {
            tap: Some(TapCfg { axes: Axes::Both }),
            long_press: Some(LongPressCfg {}),
            drag: Some(DragCfg { axes: Axes::Y, direction: Direction::Any, min_travel: None }),
        }
    }

    fn tap_set() -> GestureSet {
        GestureSet { tap: Some(TapCfg { axes: Axes::Both }), long_press: None, drag: None }
    }

    fn drag_set() -> GestureSet {
        GestureSet {
            tap: None,
            long_press: None,
            drag: Some(DragCfg { axes: Axes::Both, direction: Direction::Any, min_travel: None }),
        }
    }

    #[test]
    fn velocity_from_sample_window() {
        let mut vel = VelocityTracker::new(40);

        // Outside the window: ignored
        vel.push(Duration::from_millis(0), Point::new(0., 0.));
        vel.push(Duration::from_millis(100), Point::new(0., 0.));
        vel.push(Duration::from_millis(120), Point::new(0., 30.));
        vel.push(Duration::from_millis(140), Point::new(0., 60.));

        // Window is [100, 140]: 60px over 40ms = 1500 px/s
        let v = vel.velocity();
        assert!((v.y - 1500.).abs() < 0.01, "vel.y = {}", v.y);
        assert!(v.x.abs() < 0.01);
    }

    #[test]
    fn velocity_zero_when_stale_samples() {
        let mut vel = VelocityTracker::new(40);
        vel.push(Duration::from_millis(0), Point::new(0., 0.));
        assert_eq!(vel.velocity(), Vector { x: 0., y: 0. });

        // All samples at (nearly) the same time: no measurable dt
        let mut vel = VelocityTracker::new(40);
        vel.push(Duration::from_millis(10), Point::new(0., 0.));
        vel.push(Duration::from_millis(10), Point::new(0., 50.));
        assert_eq!(vel.velocity(), Vector { x: 0., y: 0. });
    }

    #[test]
    fn move_throttle_gates_delivery() {
        let mut throttle = MoveThrottle::new(20);

        assert!(throttle.should_deliver(Duration::from_millis(0)));
        assert!(!throttle.should_deliver(Duration::from_millis(5)));
        assert!(!throttle.should_deliver(Duration::from_millis(19)));
        assert!(throttle.should_deliver(Duration::from_millis(20)));
        assert!(throttle.should_deliver(Duration::from_millis(45)));
    }

    #[test]
    fn tap_within_slop() {
        let mut arena = Arena::new(CONSTS, &[scroll_set()]);

        let recs = arena.on_move(Duration::from_millis(50), delta(4., 4.));
        assert!(recs.is_empty());

        let recs = arena.on_up(Duration::from_millis(200), Vector { x: 0., y: 0. });
        assert_eq!(recs, vec![(0, Recognition::Tap)]);
    }

    #[test]
    fn tap_movement_beyond_slop_becomes_drag() {
        let mut arena = Arena::new(CONSTS, &[scroll_set()]);

        let recs = arena.on_move(Duration::from_millis(50), delta(3., 12.));
        assert_eq!(recs, vec![(0, Recognition::DragStart)]);

        let recs = arena.on_up(Duration::from_millis(120), Vector { x: 0., y: 0. });
        assert!(matches!(recs.as_slice(), [(0, Recognition::DragEnd { .. })]));
        assert!(!matches!(recs.as_slice(), [(0, Recognition::Tap)]));
    }

    #[test]
    fn tap_duration_bound() {
        let mut arena = Arena::new(CONSTS, &[tap_set()]);

        // Stationary but too slow
        arena.on_move(Duration::from_millis(50), delta(0., 0.));
        let recs = arena.on_up(Duration::from_millis(400), Vector { x: 0., y: 0. });
        assert!(recs.is_empty());
    }

    #[test]
    fn long_press_fires_during_hold() {
        let mut arena = Arena::new(CONSTS, &[scroll_set()]);
        assert!(arena.wants_long_press());

        arena.on_move(Duration::from_millis(100), delta(2., 2.));
        let rec = arena.long_press_due();
        assert_eq!(rec, Some((0, Recognition::LongPress)));

        // At most once per touch
        assert_eq!(arena.long_press_due(), None);
    }

    #[test]
    fn long_press_cancelled_by_movement() {
        let mut arena = Arena::new(CONSTS, &[scroll_set()]);

        arena.on_move(Duration::from_millis(100), delta(0., 15.));
        assert_eq!(arena.long_press_due(), None);
    }

    #[test]
    fn long_press_cancels_tap() {
        let mut arena = Arena::new(CONSTS, &[scroll_set()]);

        arena.long_press_due();
        let recs = arena.on_up(Duration::from_millis(500), Vector { x: 0., y: 0. });
        assert!(recs.is_empty());
    }

    #[test]
    fn drag_cancels_pending_long_press() {
        let mut arena = Arena::new(CONSTS, &[scroll_set()]);

        arena.on_move(Duration::from_millis(50), delta(0., 20.));
        assert_eq!(arena.long_press_due(), None);
    }

    #[test]
    fn descendant_tap_wins_within_slop() {
        // Chain: ancestor scroller (root, idx 0), descendant tapper (idx 1)
        let sets = [scroll_set(), tap_set()];
        let mut arena = Arena::new(CONSTS, &sets);

        let recs = arena.on_move(Duration::from_millis(50), delta(5., 5.));
        assert!(recs.is_empty());

        let recs = arena.on_up(Duration::from_millis(150), Vector { x: 0., y: 0. });
        assert_eq!(recs, vec![(1, Recognition::Tap)]);
    }

    #[test]
    fn ancestor_drag_wins_beyond_slop() {
        let sets = [scroll_set(), tap_set()];
        let mut arena = Arena::new(CONSTS, &sets);

        let recs = arena.on_move(Duration::from_millis(50), delta(0., 20.));
        assert_eq!(recs, vec![(0, Recognition::DragStart)]);

        let recs = arena.on_up(Duration::from_millis(150), Vector { x: 0., y: 0. });
        assert!(matches!(recs.as_slice(), [(0, Recognition::DragEnd { .. })]));
    }

    #[test]
    fn precision_drag_starts_on_first_movement() {
        let set = GestureSet {
            tap: Some(TapCfg { axes: Axes::Both }),
            long_press: None,
            drag: Some(DragCfg {
                axes: Axes::Both,
                direction: Direction::Any,
                min_travel: Some(0.),
            }),
        };
        let mut arena = Arena::new(CONSTS, &[set]);

        // 1px is beyond a zero threshold
        let recs = arena.on_move(Duration::from_millis(10), delta(1., 0.));
        assert_eq!(recs, vec![(0, Recognition::DragStart)]);

        let recs = arena.on_up(Duration::from_millis(100), Vector { x: 0., y: 0. });
        assert!(matches!(recs.as_slice(), [(0, Recognition::DragEnd { .. })]));
    }

    #[test]
    fn axis_locked_drag_ignores_off_axis_movement() {
        let mut arena = Arena::new(CONSTS, &[scroll_set()]);

        // Horizontal travel never starts a y-locked drag
        let recs = arena.on_move(Duration::from_millis(50), delta(25., 0.));
        assert!(recs.is_empty());

        // But it did cancel the tap
        let recs = arena.on_up(Duration::from_millis(100), Vector { x: 0., y: 0. });
        assert!(recs.is_empty());
    }

    #[test]
    fn directional_gate_blocks_orthogonal_drag() {
        let set = GestureSet {
            tap: Some(TapCfg { axes: Axes::Both }),
            long_press: None,
            drag: Some(DragCfg {
                axes: Axes::Both,
                direction: Direction::Horizontal,
                min_travel: None,
            }),
        };
        let mut arena = Arena::new(CONSTS, &[set]);

        let recs = arena.on_move(Duration::from_millis(50), delta(0., 25.));
        assert!(recs.is_empty(), "vertical movement must not start a horizontal drag");

        let recs = arena.on_move(Duration::from_millis(80), delta(25., 0.));
        assert_eq!(recs, vec![(0, Recognition::DragStart)]);
    }
}
