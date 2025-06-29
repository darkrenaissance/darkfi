/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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

use async_trait::async_trait;
use miniquad::{KeyCode, KeyMods};
use std::sync::Arc;

use crate::{
    prop::{
        PropertyPtr,
        PropertyUint32, Role,
    },
    scene::{Pimpl, SceneNodeWeak},
};

use super::UIObject;

macro_rules! d { ($($arg:tt)*) => { debug!(target: "ui::shortcut", $($arg)*); } }
macro_rules! t { ($($arg:tt)*) => { trace!(target: "ui::shortcut", $($arg)*); } }

fn vec_to_string(v: Vec<&str>) -> Vec<String> {
    v.into_iter().map(|s| s.to_string()).collect()
}

pub type ShortcutPtr = Arc<Shortcut>;

pub struct Shortcut {
    node: SceneNodeWeak,
    key: PropertyPtr,
    priority: PropertyUint32,
}

impl Shortcut {
    pub async fn new(node: SceneNodeWeak) -> Pimpl {
        t!("Shortcut::new()");

        let node_ref = &node.upgrade().unwrap();
        let key = node_ref.get_property("key").unwrap();
        let priority = PropertyUint32::wrap(node_ref, Role::Internal, "priority", 0).unwrap();

        let self_ = Arc::new(Self { node, key, priority });

        Pimpl::Shortcut(self_)
    }

    fn get_key_combo(&self) -> Option<Vec<String>> {
        let Ok(key) = self.key.get_str(0) else { return None };
        let keys: Vec<&str> = key.split('+').collect();
        Some(vec_to_string(keys))
    }
}

#[async_trait]
impl UIObject for Shortcut {
    fn priority(&self) -> u32 {
        self.priority.get()
    }

    async fn handle_key_down(&self, key: KeyCode, mods: KeyMods, repeat: bool) -> bool {
        t!("handle_key_down({key:?}, {mods:?}, {repeat})");
        if repeat {
            return false
        }

        let Some(mut shortcut) = self.get_key_combo() else { return false };
        let mut keys = keycode_to_strs(key, mods);

        shortcut.sort();
        keys.sort();

        if shortcut != keys {
            t!("shortcut({:?}): {shortcut:?} != {keys:?}", self.node.upgrade().unwrap());
            return false
        }

        let node = self.node.upgrade().unwrap();
        d!("Shortcut invoked: {node:?}");
        node.trigger("shortcut", vec![]).await.unwrap();

        true
    }
}

fn keycode_to_strs(key: KeyCode, mods: KeyMods) -> Vec<String> {
    let mut keys = vec![];
    if mods.shift {
        keys.push("shift");
    }
    if mods.ctrl {
        keys.push("ctrl");
    }
    if mods.alt {
        keys.push("alt");
    }
    if mods.logo {
        keys.push("logo");
    }

    match key {
        KeyCode::Space => keys.push("space"),
        KeyCode::Apostrophe => keys.push("'"),
        KeyCode::Comma => keys.push(","),
        KeyCode::Minus => keys.push("-"),
        KeyCode::Period => keys.push("."),
        KeyCode::Slash => keys.push("/"),
        KeyCode::Key0 => keys.push("0"),
        KeyCode::Key1 => keys.push("1"),
        KeyCode::Key2 => keys.push("2"),
        KeyCode::Key3 => keys.push("3"),
        KeyCode::Key4 => keys.push("4"),
        KeyCode::Key5 => keys.push("5"),
        KeyCode::Key6 => keys.push("6"),
        KeyCode::Key7 => keys.push("7"),
        KeyCode::Key8 => keys.push("8"),
        KeyCode::Key9 => keys.push("9"),
        KeyCode::Semicolon => keys.push(";"),
        KeyCode::Equal => keys.push("="),
        KeyCode::A => keys.push("a"),
        KeyCode::B => keys.push("b"),
        KeyCode::C => keys.push("c"),
        KeyCode::D => keys.push("d"),
        KeyCode::E => keys.push("e"),
        KeyCode::F => keys.push("f"),
        KeyCode::G => keys.push("g"),
        KeyCode::H => keys.push("h"),
        KeyCode::I => keys.push("i"),
        KeyCode::J => keys.push("j"),
        KeyCode::K => keys.push("k"),
        KeyCode::L => keys.push("l"),
        KeyCode::M => keys.push("m"),
        KeyCode::N => keys.push("n"),
        KeyCode::O => keys.push("o"),
        KeyCode::P => keys.push("p"),
        KeyCode::Q => keys.push("q"),
        KeyCode::R => keys.push("r"),
        KeyCode::S => keys.push("s"),
        KeyCode::T => keys.push("t"),
        KeyCode::U => keys.push("u"),
        KeyCode::V => keys.push("v"),
        KeyCode::W => keys.push("w"),
        KeyCode::X => keys.push("x"),
        KeyCode::Y => keys.push("y"),
        KeyCode::Z => keys.push("z"),
        KeyCode::LeftBracket => keys.push("("),
        KeyCode::Backslash => keys.push("\\"),
        KeyCode::RightBracket => keys.push(")"),
        KeyCode::GraveAccent => keys.push("graveaccent"),
        KeyCode::World1 => keys.push("world1"),
        KeyCode::World2 => keys.push("world2"),
        KeyCode::Escape => keys.push("esc"),
        KeyCode::Enter => keys.push("enter"),
        KeyCode::Tab => keys.push("tab"),
        KeyCode::Backspace => keys.push("backspace"),
        KeyCode::Insert => keys.push("ins"),
        KeyCode::Delete => keys.push("del"),
        KeyCode::Right => keys.push("right"),
        KeyCode::Left => keys.push("left"),
        KeyCode::Down => keys.push("down"),
        KeyCode::Up => keys.push("up"),
        KeyCode::PageUp => keys.push("pageup"),
        KeyCode::PageDown => keys.push("pagedown"),
        KeyCode::Home => keys.push("home"),
        KeyCode::End => keys.push("end"),
        KeyCode::CapsLock => keys.push("capslock"),
        KeyCode::ScrollLock => keys.push("scrolllock"),
        KeyCode::NumLock => keys.push("numlock"),
        KeyCode::PrintScreen => keys.push("printscreen"),
        KeyCode::Pause => keys.push("pause"),
        KeyCode::F1 => keys.push("f1"),
        KeyCode::F2 => keys.push("f2"),
        KeyCode::F3 => keys.push("f3"),
        KeyCode::F4 => keys.push("f4"),
        KeyCode::F5 => keys.push("f5"),
        KeyCode::F6 => keys.push("f6"),
        KeyCode::F7 => keys.push("f7"),
        KeyCode::F8 => keys.push("f8"),
        KeyCode::F9 => keys.push("f9"),
        KeyCode::F10 => keys.push("f10"),
        KeyCode::F11 => keys.push("f11"),
        KeyCode::F12 => keys.push("f12"),
        KeyCode::F13 => keys.push("f13"),
        KeyCode::F14 => keys.push("f14"),
        KeyCode::F15 => keys.push("f15"),
        KeyCode::F16 => keys.push("f16"),
        KeyCode::F17 => keys.push("f17"),
        KeyCode::F18 => keys.push("f18"),
        KeyCode::F19 => keys.push("f19"),
        KeyCode::F20 => keys.push("f20"),
        KeyCode::F21 => keys.push("f21"),
        KeyCode::F22 => keys.push("f22"),
        KeyCode::F23 => keys.push("f23"),
        KeyCode::F24 => keys.push("f24"),
        KeyCode::F25 => keys.push("f25"),
        KeyCode::Kp0 => keys.push("kp0"),
        KeyCode::Kp1 => keys.push("kp1"),
        KeyCode::Kp2 => keys.push("kp2"),
        KeyCode::Kp3 => keys.push("kp3"),
        KeyCode::Kp4 => keys.push("kp4"),
        KeyCode::Kp5 => keys.push("kp5"),
        KeyCode::Kp6 => keys.push("kp6"),
        KeyCode::Kp7 => keys.push("kp7"),
        KeyCode::Kp8 => keys.push("kp8"),
        KeyCode::Kp9 => keys.push("kp9"),
        KeyCode::KpDecimal => keys.push("kpdecimal"),
        KeyCode::KpDivide => keys.push("kpdivide"),
        KeyCode::KpMultiply => keys.push("kpmultiply"),
        KeyCode::KpSubtract => keys.push("kpsubtract"),
        KeyCode::KpAdd => keys.push("kpadd"),
        KeyCode::KpEnter => keys.push("kpenter"),
        KeyCode::KpEqual => keys.push("kpequal"),
        KeyCode::LeftShift |
        KeyCode::LeftControl |
        KeyCode::LeftAlt |
        KeyCode::LeftSuper |
        KeyCode::RightShift |
        KeyCode::RightControl |
        KeyCode::RightAlt |
        KeyCode::RightSuper => {}
        KeyCode::Menu => keys.push("menu"),
        KeyCode::Back => keys.push("back"),
        KeyCode::Unknown => {
            // Do nothing...
        }
    }
    vec_to_string(keys)
}
