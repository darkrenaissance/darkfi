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

use miniquad::TouchPhase;

use crate::gfx::Point;

/// Maximum number of simultaneous touch points to track
const MAX_TOUCH_POINTS: usize = 10;

/// Gesture recognition thresholds
const TAP_MAX_MOVEMENT: f32 = 10.0;
const TAP_MAX_DURATION: u64 = 300;
const DRAG_MIN_MOVEMENT: f32 = 15.0;
const FLICK_MIN_VELOCITY: f32 = 500.0;
const LONG_PRESS_MIN_DURATION: u64 = 500;
const LONG_PRESS_MAX_MOVEMENT: f32 = 20.0;

/// Types of gestures that can be recognized
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GestureType {
    /// A quick tap without significant movement
    Tap,
    /// Continuous drag gesture
    Drag,
    /// Quick flick with velocity
    Flick,
    /// Long press without movement
    LongPress,
}

/// High-level gesture event with relevant data
#[derive(Debug, Clone)]
pub struct GestureEvent {
    /// Type of gesture recognized
    pub gesture_type: GestureType,
    /// Touch ID that generated this gesture
    pub touch_id: u64,
    /// Current position of the gesture
    pub position: Point,
    /// Starting position (for drag/flick)
    pub start_position: Point,
    /// Time elapsed since touch started (milliseconds)
    pub duration_ms: u64,
    /// Velocity in pixels per second (for flick)
    pub velocity: Option<Point>,
    /// Total displacement from start (for drag)
    pub displacement: Option<Point>,
}

/// State for tracking a single touch point
#[derive(Debug, Clone)]
struct TouchTracker {
    /// Start position
    start_pos: Point,
    /// Current position
    curr_pos: Point,
    /// Previous position (for velocity calculation)
    prev_pos: Option<Point>,
    /// Start timestamp
    start_instant: std::time::Instant,
    /// Last update timestamp
    last_update: std::time::Instant,
    /// Current phase
    phase: TouchPhase,
    /// Gesture recognized for this touch
    recognized_gesture: Option<GestureType>,
}

/// Main gesture processor maintaining state for all touch points
pub struct GestureProcessor {
    /// Active touch trackers
    touches: [Option<TouchTracker>; MAX_TOUCH_POINTS],
}

impl GestureProcessor {
    /// Create a new gesture processor with default thresholds
    pub fn new() -> Self {
        Self {
            touches: Default::default(),
        }
    }

    /// Process a raw touch event and return gesture event if recognized
    /// Returns None if no gesture recognized yet, or if should fall back to raw touch
    pub fn process_touch_event(
        &mut self,
        phase: TouchPhase,
        id: u64,
        pos: Point,
    ) -> Option<GestureEvent> {
        // STUB: Route to appropriate handler based on phase
        // TODO: Implement in next phase
        match phase {
            TouchPhase::Started => self.handle_touch_started(id as usize, pos),
            TouchPhase::Moved => self.handle_touch_moved(id as usize, pos),
            TouchPhase::Ended => self.handle_touch_ended(id as usize, pos),
            TouchPhase::Cancelled => self.handle_touch_cancelled(id as usize),
        }
    }

    /// Handle touch start - initialize tracker
    fn handle_touch_started(&mut self, id: usize, pos: Point) -> Option<GestureEvent> {
        // STUB: Initialize touch tracker
        // TODO: Implement in next phase
        None
    }

    /// Handle touch move - update tracker, check for gesture recognition
    fn handle_touch_moved(&mut self, id: usize, pos: Point) -> Option<GestureEvent> {
        // STUB: Update position, calculate displacement/velocity
        // TODO: Implement gesture recognition logic in next phase
        None
    }

    /// Handle touch end - finalize gesture
    fn handle_touch_ended(&mut self, id: usize, pos: Point) -> Option<GestureEvent> {
        // STUB: Check final gesture state
        // TODO: Implement final gesture determination in next phase
        None
    }

    /// Handle touch cancel - clean up
    fn handle_touch_cancelled(&mut self, id: usize) -> Option<GestureEvent> {
        // STUB: Clean up touch tracker
        // TODO: Implement cleanup in next phase
        None
    }
}

impl Default for GestureProcessor {
    fn default() -> Self {
        Self::new()
    }
}
