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

//! Gesture recognition and delivery for the app UI.
//!
//! One recognition system with unified thresholds replaces the five
//! hand-rolled per-widget recognizers. Recognition mechanics (distance,
//! time, velocity state machines) live in [`recognizer`] exactly once;
//! [`session`] owns the touch stream, target resolution, long-press
//! timers, move throttling, and arbitration, and delivers the
//! [`GestureAction`] stream to widgets through `UIObject::handle_gesture`.

mod recognizer;
pub use recognizer::{Arena, MoveThrottle, VelocityTracker};
mod session;
pub(crate) use session::scan_children;
pub use session::{GestureSession, GestureSessionPtr, GestureTarget};

use crate::gfx::{Point, Vector};

use super::long_press_timeout;

/// The single set of recognition thresholds used by every widget.
///
/// These replace the per-widget scatter (tap strictness ranged from
/// 0.05px to 10px across widgets). Per-node config survives only where
/// semantic: axis lock, drag direction, `min_travel: 0.` for precision
/// drags (see [`GestureCfg`]).
#[derive(Debug, Clone, Copy)]
pub struct GestureConstants {
    /// Maximum travel between down and up that still counts as a tap,
    /// and the stationarity bound for long-press. Also the drag start
    /// threshold for slop-bounded drags.
    pub touch_slop: f32,
    /// Maximum duration of a tap, in milliseconds.
    pub tap_max_duration: u32,
    /// System long-press timeout in milliseconds.
    pub long_press_timeout: u32,
    /// Minimum time between delivered `DragMove` events, in milliseconds.
    /// Velocity sampling still observes every move.
    pub move_delivery_period: u32,
    /// Velocity sample window for `DragEnd` velocity, in milliseconds.
    pub sample_window_ms: u32,
}

impl GestureConstants {
    /// The platform-flavored constants: Android `ViewConfiguration`
    /// long-press timeout (via [`long_press_timeout`]), otherwise the
    /// usual defaults.
    pub fn platform() -> Self {
        Self {
            touch_slop: 10.,
            tap_max_duration: 300,
            long_press_timeout: long_press_timeout(),
            move_delivery_period: 20,
            sample_window_ms: 40,
        }
    }
}

impl Default for GestureConstants {
    fn default() -> Self {
        Self::platform()
    }
}

/// The gesture event stream delivered to widgets.
///
/// `Down`/`Up` are passthrough events delivered immediately at touch
/// start/end without waiting for recognition. `DragEnd` carries the
/// release velocity; flick is the consumer's threshold on it. All
/// positions are in the receiving widget's parent coordinate space.
#[derive(Debug, Clone, Copy)]
pub enum GestureAction {
    /// Passthrough: a touch began on this widget.
    Down { pos: Point },
    /// Passthrough: the touch ended or was cancelled. Delivered as a
    /// teardown-only notification — no recognized gesture accompanies
    /// it for a cancelled touch.
    Up { pos: Point },
    /// A quick touch within slop travel and the tap duration bound.
    Tap { pos: Point },
    /// Fired while the finger is still down, once per touch.
    LongPress { pos: Point },
    /// Travel beyond the drag threshold was detected.
    DragStart { start: Point },
    /// Drag movement, throttled to the move delivery period.
    DragMove { start: Point, prev: Point, curr: Point },
    /// The drag ended. `vel` is px/sec from the sample window.
    DragEnd { start: Point, curr: Point, vel: Vector },
}

impl GestureAction {
    /// Translate all carried positions by `v`. Used for coordinate
    /// translation when delivering through layers.
    pub fn translate(&mut self, v: Vector) {
        fn shift(p: &mut Point, v: Vector) {
            p.x += v.x;
            p.y += v.y;
        }

        match self {
            Self::Down { pos } | Self::Up { pos } | Self::Tap { pos } | Self::LongPress { pos } => {
                shift(pos, v)
            }
            Self::DragStart { start } => shift(start, v),
            Self::DragMove { start, prev, curr } => {
                shift(start, v);
                shift(prev, v);
                shift(curr, v);
            }
            Self::DragEnd { start, curr, .. } => {
                shift(start, v);
                shift(curr, v);
            }
        }
    }
}

/// Which axes a measurement is restricted to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axes {
    Both,
    Y,
    X,
}

impl Axes {
    /// Travel of `delta` projected onto these axes.
    pub fn travel(&self, delta: Point) -> f32 {
        match self {
            Self::Both => Point::new(delta.x, delta.y).dist(Point::zero()),
            Self::Y => delta.y.abs(),
            Self::X => delta.x.abs(),
        }
    }
}

/// The required dominant direction before a drag can start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Any,
    Vertical,
    Horizontal,
}

impl Direction {
    /// Whether `delta` movement is dominantly along this direction.
    pub fn matches(&self, delta: Point) -> bool {
        match self {
            Self::Any => true,
            Self::Vertical => delta.y.abs() > delta.x.abs(),
            Self::Horizontal => delta.x.abs() > delta.y.abs(),
        }
    }
}

/// Configuration of a tap recognizer. All numeric thresholds come from
/// [`GestureConstants`].
#[derive(Debug, Clone, Copy)]
pub struct TapCfg {
    /// Axes the slop travel is measured on.
    pub axes: Axes,
}

/// Configuration of a long-press recognizer.
#[derive(Debug, Clone, Copy, Default)]
pub struct LongPressCfg {}

/// Configuration of a drag recognizer.
#[derive(Debug, Clone, Copy)]
pub struct DragCfg {
    /// Axes the start travel is measured on.
    pub axes: Axes,
    /// Required dominant movement direction before starting.
    pub direction: Direction,
    /// Travel needed before the drag starts. `None` uses the touch
    /// slop (the standard dead-zone before scrolling). `Some(0.)` is a
    /// precision drag that starts on the first movement (selection
    /// handles).
    pub min_travel: Option<f32>,
}

impl DragCfg {
    /// Travel threshold for starting.
    pub fn threshold(&self, slop: f32) -> f32 {
        self.min_travel.unwrap_or(slop)
    }
}

/// The set of gestures a widget accepts.
#[derive(Debug, Clone, Copy, Default)]
pub struct GestureSet {
    /// Tap recognizer config, if taps are accepted.
    pub tap: Option<TapCfg>,
    /// Long-press recognizer config, if long-presses are accepted.
    pub long_press: Option<LongPressCfg>,
    /// Drag recognizer config, if drags are accepted.
    pub drag: Option<DragCfg>,
}

impl GestureSet {
    /// Accepts nothing. Non-participating widgets are inert.
    pub const NONE: GestureSet = GestureSet { tap: None, long_press: None, drag: None };

    /// Accepts taps anywhere in the hit region.
    pub const TAP: GestureSet =
        GestureSet { tap: Some(TapCfg { axes: Axes::Both }), long_press: None, drag: None };

    /// Vertical scroller: 1:1 vertical drag after slop plus taps.
    pub const SCROLL_VERT: GestureSet = GestureSet {
        tap: Some(TapCfg { axes: Axes::Both }),
        long_press: None,
        drag: Some(DragCfg { axes: Axes::Y, direction: Direction::Any, min_travel: None }),
    };

    /// Chat view: scroll + flick, long-press select/URL, tap forward.
    pub const CHATVIEW: GestureSet = GestureSet {
        tap: Some(TapCfg { axes: Axes::Both }),
        long_press: Some(LongPressCfg {}),
        drag: Some(DragCfg { axes: Axes::Y, direction: Direction::Any, min_travel: None }),
    };

    /// Menu: long-press edit mode, tap select/delete, drag scroll/reorder.
    pub const MENU: GestureSet = GestureSet {
        tap: Some(TapCfg { axes: Axes::Both }),
        long_press: Some(LongPressCfg {}),
        drag: Some(DragCfg { axes: Axes::Y, direction: Direction::Any, min_travel: None }),
    };

    /// Text edit hybrid: tap cursor, long-press word select, precision
    /// drag for selection handles (armed at `Down`) and content scroll.
    pub const EDIT: GestureSet = GestureSet {
        tap: Some(TapCfg { axes: Axes::Both }),
        long_press: Some(LongPressCfg {}),
        drag: Some(DragCfg { axes: Axes::Both, direction: Direction::Any, min_travel: Some(0.) }),
    };

    /// Whether any recognizer is configured.
    pub fn is_empty(&self) -> bool {
        self.tap.is_none() && self.long_press.is_none() && self.drag.is_none()
    }
}
