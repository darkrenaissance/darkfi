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
use std::{
    collections::{HashMap, VecDeque},
    time::Instant,
};

use crate::gfx::{Point, Segment, Vector};

/// Gesture recognition thresholds
const TAP_MAX_MOVEMENT: f32 = 10.0;
const TAP_MAX_DURATION: f32 = 300.;
const DRAG_MIN_MOVEMENT: f32 = 15.0;
const FLICK_MIN_VELOCITY: f32 = 500.0;
const LONG_PRESS_MIN_DURATION: f32 = 500.;
const LONG_PRESS_MAX_MOVEMENT: f32 = 20.0;

/// Types of gestures that can be recognized
#[derive(Debug, Clone, Copy)]
pub enum GestureAction {
    /// A quick tap without significant movement
    Tap(Point),
    /// Continuous drag gesture
    Drag(Segment),
    /// Quick flick with velocity
    Flick { start: Point, vel: Vector },
    /// Long press without movement
    LongPress(Point),
}

/// Internal state tracking for an active touch point
struct TouchState {
    start_pos: Point,
    start_time: Instant,
    curr_pos: Point,
    is_dragging: bool,
    long_press_emitted: bool,
    /// Used for flick scrolling - stores (time, position) samples
    samples: VecDeque<(Instant, Point)>,
}

impl TouchState {
    fn push_sample(&mut self, pos: Point) {
        self.samples.push_back((Instant::now(), pos));

        // Drop all old samples older than 40ms
        while let Some((instant, _)) = self.samples.front() {
            if instant.elapsed().as_micros() <= 40_000 {
                break;
            }
            self.samples.pop_front();
        }
    }

    fn first_sample(&self) -> Option<(f32, Point)> {
        self.samples.front().map(|(t, p)| (t.elapsed().as_micros() as f32 / 1000., *p))
    }
}

/// Main gesture processor maintaining state for all touch points
pub struct GestureProcessor {
    touches: HashMap<u64, TouchState>,
}

impl GestureProcessor {
    /// Create a new gesture processor with default thresholds
    pub fn new() -> Self {
        Self { touches: Default::default() }
    }

    pub fn process(&mut self, phase: TouchPhase, id: u64, pos: Point) -> Option<GestureAction> {
        match phase {
            TouchPhase::Started => self.handle_touch_start(id, pos),
            TouchPhase::Moved => self.handle_touch_move(id, pos),
            TouchPhase::Ended => self.handle_touch_end(id, pos),
            TouchPhase::Cancelled => self.handle_touch_cancel(id),
        }
    }

    fn handle_touch_start(&mut self, id: u64, pos: Point) -> Option<GestureAction> {
        let state = TouchState {
            start_pos: pos,
            start_time: Instant::now(),
            curr_pos: pos,
            is_dragging: false,
            long_press_emitted: false,
            samples: VecDeque::new(),
        };
        self.touches.insert(id, state);
        None
    }

    fn check_long_press(state: &mut TouchState, pos: Point) -> Option<GestureAction> {
        if state.long_press_emitted {
            return None;
        }
        // Once drag starts no long press can be emitted
        if state.is_dragging {
            return None;
        }

        let dur = state.start_time.elapsed().as_millis() as f32;
        if dur >= LONG_PRESS_MIN_DURATION && pos.dist(state.start_pos) <= LONG_PRESS_MAX_MOVEMENT {
            state.long_press_emitted = true;
            return Some(GestureAction::LongPress(pos))
        }

        None
    }

    fn handle_touch_move(&mut self, id: u64, pos: Point) -> Option<GestureAction> {
        let Some(state) = self.touches.get_mut(&id) else { return None };

        if let Some(gesture) = Self::check_long_press(state, pos) {
            return Some(gesture);
        }

        state.curr_pos = pos;

        // Collect sample for flick detection
        state.push_sample(pos);

        let dist = pos.dist(state.start_pos);
        if dist >= DRAG_MIN_MOVEMENT {
            state.is_dragging = true;
        }

        if state.is_dragging {
            return Some(GestureAction::Drag(Segment { start: state.start_pos, end: pos }))
        }

        None
    }

    fn handle_touch_end(&mut self, id: u64, pos: Point) -> Option<GestureAction> {
        let mut state = self.touches.remove(&id)?;

        // Update current position one last time
        state.curr_pos = pos;
        state.push_sample(pos);

        // Calculate velocity from samples
        if let Some((dt_ms, start_pos)) = state.first_sample() {
            let dt_sec = dt_ms / 1000.0;

            if dt_sec > 0.001 {
                // Calculate velocity using Point operations, convert to Vector
                let vel: Vector = ((pos - start_pos) / dt_sec).into();

                // Check for flick (high velocity movement)
                if vel.mag() >= FLICK_MIN_VELOCITY && !state.long_press_emitted {
                    return Some(GestureAction::Flick { start: state.start_pos, vel });
                }
            }
        }

        // Then check for drag
        if state.is_dragging {
            return Some(GestureAction::Drag(Segment { start: state.start_pos, end: pos }))
        }

        // Then check for long press
        if let Some(gesture) = Self::check_long_press(&mut state, pos) {
            return Some(gesture);
        }

        // Finally check for tap
        let dur = state.start_time.elapsed().as_millis() as f32;
        let dist = pos.dist(state.start_pos);

        if dist <= TAP_MAX_MOVEMENT && dur <= TAP_MAX_DURATION {
            Some(GestureAction::Tap(pos))
        } else {
            None
        }
    }

    fn handle_touch_cancel(&mut self, id: u64) -> Option<GestureAction> {
        self.touches.remove(&id);
        None
    }
}

impl Default for GestureProcessor {
    fn default() -> Self {
        Self::new()
    }
}
