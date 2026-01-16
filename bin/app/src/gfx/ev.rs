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

use miniquad::{KeyCode, KeyMods, MouseButton, TouchPhase};
use std::sync::Arc;

use super::{Dimension, Point};

struct EventChannel<T> {
    sender: async_channel::Sender<T>,
    recvr: async_channel::Receiver<T>,
}

impl<T> EventChannel<T> {
    fn new() -> Self {
        let (sender, recvr) = async_channel::unbounded();
        Self { sender, recvr }
    }

    fn notify(&self, ev: T) {
        self.sender.try_send(ev).unwrap();
    }

    fn clone_recvr(&self) -> async_channel::Receiver<T> {
        self.recvr.clone()
    }
}

pub type GraphicsEventPublisherPtr = Arc<GraphicsEventPublisher>;

pub struct GraphicsEventPublisher {
    resize: EventChannel<Dimension>,
    key_down: EventChannel<(KeyCode, KeyMods, bool)>,
    key_up: EventChannel<(KeyCode, KeyMods)>,
    chr: EventChannel<(char, KeyMods, bool)>,
    mouse_btn_down: EventChannel<(MouseButton, Point)>,
    mouse_btn_up: EventChannel<(MouseButton, Point)>,
    mouse_move: EventChannel<Point>,
    mouse_wheel: EventChannel<Point>,
    touch: EventChannel<(TouchPhase, u64, Point)>,
}

pub type GraphicsEventResizeSub = async_channel::Receiver<Dimension>;
pub type GraphicsEventKeyDownSub = async_channel::Receiver<(KeyCode, KeyMods, bool)>;
pub type GraphicsEventKeyUpSub = async_channel::Receiver<(KeyCode, KeyMods)>;
pub type GraphicsEventCharSub = async_channel::Receiver<(char, KeyMods, bool)>;
pub type GraphicsEventMouseButtonDownSub = async_channel::Receiver<(MouseButton, Point)>;
pub type GraphicsEventMouseButtonUpSub = async_channel::Receiver<(MouseButton, Point)>;
pub type GraphicsEventMouseMoveSub = async_channel::Receiver<Point>;
pub type GraphicsEventMouseWheelSub = async_channel::Receiver<Point>;
pub type GraphicsEventTouchSub = async_channel::Receiver<(TouchPhase, u64, Point)>;

impl GraphicsEventPublisher {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            resize: EventChannel::new(),
            key_down: EventChannel::new(),
            key_up: EventChannel::new(),
            chr: EventChannel::new(),
            mouse_btn_down: EventChannel::new(),
            mouse_btn_up: EventChannel::new(),
            mouse_move: EventChannel::new(),
            mouse_wheel: EventChannel::new(),
            touch: EventChannel::new(),
        })
    }

    pub(super) fn notify_resize(&self, screen_size: Dimension) {
        self.resize.notify(screen_size);
    }
    pub(super) fn notify_key_down(&self, key: KeyCode, mods: KeyMods, repeat: bool) {
        let ev = (key, mods, repeat);
        self.key_down.notify(ev);
    }
    pub(super) fn notify_key_up(&self, key: KeyCode, mods: KeyMods) {
        let ev = (key, mods);
        self.key_up.notify(ev);
    }
    pub(super) fn notify_char(&self, chr: char, mods: KeyMods, repeat: bool) {
        let ev = (chr, mods, repeat);
        self.chr.notify(ev);
    }
    pub(super) fn notify_mouse_btn_down(&self, button: MouseButton, mouse_pos: Point) {
        let ev = (button, mouse_pos);
        self.mouse_btn_down.notify(ev);
    }
    pub(super) fn notify_mouse_btn_up(&self, button: MouseButton, mouse_pos: Point) {
        let ev = (button, mouse_pos);
        self.mouse_btn_up.notify(ev);
    }

    pub(super) fn notify_mouse_move(&self, mouse_pos: Point) {
        self.mouse_move.notify(mouse_pos);
    }
    pub(super) fn notify_mouse_wheel(&self, wheel_pos: Point) {
        self.mouse_wheel.notify(wheel_pos);
    }
    pub(super) fn notify_touch(&self, phase: TouchPhase, id: u64, touch_pos: Point) {
        let ev = (phase, id, touch_pos);
        self.touch.notify(ev);
    }

    pub fn subscribe_resize(&self) -> GraphicsEventResizeSub {
        self.resize.clone_recvr()
    }
    pub fn subscribe_key_down(&self) -> GraphicsEventKeyDownSub {
        self.key_down.clone_recvr()
    }
    pub fn subscribe_key_up(&self) -> GraphicsEventKeyUpSub {
        self.key_up.clone_recvr()
    }
    pub fn subscribe_char(&self) -> GraphicsEventCharSub {
        self.chr.clone_recvr()
    }
    pub fn subscribe_mouse_btn_down(&self) -> GraphicsEventMouseButtonDownSub {
        self.mouse_btn_down.clone_recvr()
    }
    pub fn subscribe_mouse_btn_up(&self) -> GraphicsEventMouseButtonUpSub {
        self.mouse_btn_up.clone_recvr()
    }
    pub fn subscribe_mouse_move(&self) -> GraphicsEventMouseMoveSub {
        self.mouse_move.clone_recvr()
    }
    pub fn subscribe_mouse_wheel(&self) -> GraphicsEventMouseWheelSub {
        self.mouse_wheel.clone_recvr()
    }
    pub fn subscribe_touch(&self) -> GraphicsEventTouchSub {
        self.touch.clone_recvr()
    }
}
