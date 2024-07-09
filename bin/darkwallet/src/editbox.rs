/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

use log::{debug, info};
use miniquad::{window, KeyMods, MouseButton, TextureId, UniformType};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Instant,
};

use crate::{
    error::{Error, Result},
    gfx::{
        FreetypeFace, Point, Rectangle, RenderContext, COLOR_DARKGREY, COLOR_GREEN, COLOR_WHITE,
    },
    prop::{Property, PropertyBool, PropertyColor, PropertyFloat32, PropertyStr, PropertyUint32},
    scene::{Pimpl, SceneGraph, SceneNodeId},
    text::{Glyph, TextShaper},
};

const CURSOR_WIDTH: f32 = 4.;

struct PressedKeysSmoothRepeat {
    /// When holding keys, we track from start and last sent time.
    /// This is useful for initial delay and smooth scrolling.
    pressed_keys: HashMap<String, RepeatingKeyTimer>,
    /// Initial delay before allowing keys
    start_delay: u32,
    /// Minimum time between repeated keys
    step_time: u32,
}

impl PressedKeysSmoothRepeat {
    fn new(start_delay: u32, step_time: u32) -> Self {
        Self { pressed_keys: HashMap::new(), start_delay, step_time }
    }

    fn key_down(&mut self, key: &str, repeat: bool) -> u32 {
        if !repeat {
            return 1;
        }

        // Insert key if not exists
        if !self.pressed_keys.contains_key(key) {
            self.pressed_keys.insert(key.to_string(), RepeatingKeyTimer::new());
        }

        let repeater = self.pressed_keys.get_mut(key).expect("repeat map");
        repeater.update(self.start_delay, self.step_time)
    }

    fn key_up(&mut self, key: &str) {
        self.pressed_keys.remove(key);
    }
}

struct RepeatingKeyTimer {
    start: Instant,
    actions: u32,
}

impl RepeatingKeyTimer {
    fn new() -> Self {
        Self { start: Instant::now(), actions: 0 }
    }

    fn update(&mut self, start_delay: u32, step_time: u32) -> u32 {
        let elapsed = self.start.elapsed().as_millis();
        if elapsed < start_delay as u128 {
            return 0
        }
        let total_actions = ((elapsed - start_delay as u128) / step_time as u128) as u32;
        let remaining_actions = total_actions - self.actions;
        self.actions = total_actions;
        remaining_actions
    }
}

pub type EditBoxPtr = Arc<EditBox>;

pub struct EditBox {
    node_name: String,
    is_active: PropertyBool,
    debug: PropertyBool,
    baseline: PropertyFloat32,
    scroll: PropertyFloat32,
    cursor_pos: PropertyUint32,
    selected: Arc<Property>,
    text: PropertyStr,
    font_size: PropertyFloat32,
    text_color: PropertyColor,
    cursor_color: PropertyColor,
    hi_bg_color: PropertyColor,
    // Used for mouse clicks
    world_rect: Mutex<Rectangle<f32>>,
    glyphs: Mutex<Vec<Glyph>>,
    text_shaper: TextShaper,
    key_repeat: Mutex<PressedKeysSmoothRepeat>,
    mouse_btn_held: AtomicBool,
    window_scale: f32,
    screen_size: Arc<Property>,
    atlas: Mutex<HashMap<u32, TextureId>>,
}

impl EditBox {
    pub fn new(
        scene_graph: &mut SceneGraph,
        node_id: SceneNodeId,
        font_faces: Vec<FreetypeFace>,
    ) -> Result<Pimpl> {
        let node = scene_graph.get_node(node_id).unwrap();
        let node_name = node.name.clone();
        let is_active = PropertyBool::wrap(node, "is_active", 0)?;
        let debug = PropertyBool::wrap(node, "debug", 0)?;
        let baseline = PropertyFloat32::wrap(node, "baseline", 0)?;
        let scroll = PropertyFloat32::wrap(node, "scroll", 0)?;
        let cursor_pos = PropertyUint32::wrap(node, "cursor_pos", 0)?;
        let selected = node.get_property("selected").ok_or(Error::PropertyNotFound)?;
        let text = PropertyStr::wrap(node, "text", 0)?;
        let font_size = PropertyFloat32::wrap(node, "font_size", 0)?;
        let text_color = PropertyColor::wrap(node, "text_color")?;
        let cursor_color = PropertyColor::wrap(node, "cursor_color")?;
        let hi_bg_color = PropertyColor::wrap(node, "hi_bg_color")?;

        let text_shaper = TextShaper { font_faces };

        // TODO: catch window resize event and regen glyphs
        // Used for scaling the font size
        let window_node = scene_graph.lookup_node("/window").expect("no window attached!");
        let window_scale = window_node.get_property_f32("scale")?;
        let screen_size = window_node.get_property("screen_size").ok_or(Error::PropertyNotFound)?;

        let self_ = Arc::new(Self {
            node_name: node_name.clone(),
            is_active,
            debug,
            baseline,
            scroll,
            cursor_pos,
            selected,
            text,
            font_size,
            text_color,
            cursor_color,
            hi_bg_color,
            world_rect: Mutex::new(Rectangle { x: 0., y: 0., w: 0., h: 0. }),
            glyphs: Mutex::new(vec![]),
            text_shaper,
            key_repeat: Mutex::new(PressedKeysSmoothRepeat::new(400, 50)),
            mouse_btn_held: AtomicBool::new(false),
            window_scale,
            screen_size,
            atlas: Mutex::new(HashMap::new()),
        });
        self_.regen_glyphs().unwrap();

        /*
        let weak_self = Arc::downgrade(&self_);
        let slot_key_down = Slot {
            name: format!("{}::key_down", node_name),
            func: Box::new(move |data| {
                let mut cur = Cursor::new(&data);
                let keymods = KeyMods {
                    shift: Decodable::decode(&mut cur).unwrap(),
                    ctrl: Decodable::decode(&mut cur).unwrap(),
                    alt: Decodable::decode(&mut cur).unwrap(),
                    logo: Decodable::decode(&mut cur).unwrap(),
                };
                let repeat = bool::decode(&mut cur).unwrap();
                let key = String::decode(&mut cur).unwrap();

                let self_ = weak_self.upgrade();
                if let Some(self_) = self_ {
                    self_.key_down(key, keymods, repeat);
                }
            }),
        };
        let weak_self = Arc::downgrade(&self_);
        let slot_key_up = Slot {
            name: format!("{}::key_up", node_name),
            func: Box::new(move |data| {
                let mut cur = Cursor::new(&data);
                let keymods = KeyMods {
                    shift: Decodable::decode(&mut cur).unwrap(),
                    ctrl: Decodable::decode(&mut cur).unwrap(),
                    alt: Decodable::decode(&mut cur).unwrap(),
                    logo: Decodable::decode(&mut cur).unwrap(),
                };
                let key = String::decode(&mut cur).unwrap();

                let self_ = weak_self.upgrade();
                if let Some(self_) = self_ {
                    self_.key_up(key, keymods);
                }
            }),
        };

        let keyb_node =
            scene_graph
            .lookup_node_mut("/window/input/keyboard")
            .expect("no keyboard attached!");
        keyb_node.register("key_down", slot_key_down).unwrap();
        keyb_node.register("key_up", slot_key_up).unwrap();

        let weak_self = Arc::downgrade(&self_);
        let slot_btn_down = Slot {
            name: format!("{}::mouse_button_down", node_name),
            func: Box::new(move |data| {
                let mut cur = Cursor::new(&data);
                let button = MouseButton::from_u8(u8::decode(&mut cur).unwrap());
                let x = f32::decode(&mut cur).unwrap();
                let y = f32::decode(&mut cur).unwrap();

                let self_ = weak_self.upgrade();
                if let Some(self_) = self_ {
                    self_.mouse_button_down(button, x, y);
                }
            }),
        };

        let weak_self = Arc::downgrade(&self_);
        let slot_btn_up = Slot {
            name: format!("{}::mouse_button_up", node_name),
            func: Box::new(move |data| {
                let mut cur = Cursor::new(&data);
                let button = MouseButton::from_u8(u8::decode(&mut cur).unwrap());
                let x = f32::decode(&mut cur).unwrap();
                let y = f32::decode(&mut cur).unwrap();

                let self_ = weak_self.upgrade();
                if let Some(self_) = self_ {
                    self_.mouse_button_up(button, x, y);
                }
            }),
        };

        let weak_self = Arc::downgrade(&self_);
        let slot_move = Slot {
            name: format!("{}::mouse_move", node_name),
            func: Box::new(move |data| {
                let mut cur = Cursor::new(&data);
                let x = f32::decode(&mut cur).unwrap();
                let y = f32::decode(&mut cur).unwrap();

                let self_ = weak_self.upgrade();
                if let Some(self_) = self_ {
                    self_.mouse_move(x, y);
                }
            }),
        };

        let mouse_node =
            scene_graph
            .lookup_node_mut("/window/input/mouse")
            .expect("no mouse attached!");
        mouse_node.register("button_down", slot_btn_down).unwrap();
        mouse_node.register("button_up", slot_btn_up).unwrap();
        mouse_node.register("move", slot_move).unwrap();

        let weak_self = Arc::downgrade(&self_);
        let slot_resize = Slot {
            name: format!("{}::window_resize", node_name),
            func: Box::new(move |data| {
                let mut cur = Cursor::new(&data);
                let w = f32::decode(&mut cur).unwrap();
                let h = f32::decode(&mut cur).unwrap();

                let self_ = weak_self.upgrade();
                if let Some(self_) = self_ {
                    self_.window_resize(w, h);
                }
            }),
        };
        */

        let window_node = scene_graph.lookup_node_mut("/window").expect("no window attached!");
        //window_node.register("resize", slot_resize).unwrap();

        // Save any properties we use
        Ok(Pimpl::EditBox(self_))
    }

    pub fn render<'a>(
        &self,
        render: &mut RenderContext<'a>,
        node_id: SceneNodeId,
        layer_rect: &Rectangle<f32>,
    ) -> Result<()> {
        let node = render.scene_graph.get_node(node_id).unwrap();

        let rect = RenderContext::get_dim(node, layer_rect)?;

        // Used for detecting mouse clicks
        let mut world_rect = rect.clone();
        world_rect.x += layer_rect.x as f32;
        world_rect.y += layer_rect.y as f32;
        world_rect.x *= self.window_scale;
        world_rect.y *= self.window_scale;
        world_rect.w *= self.window_scale;
        world_rect.h *= self.window_scale;
        *self.world_rect.lock().unwrap() = world_rect;

        let layer_w = layer_rect.w as f32;
        let layer_h = layer_rect.h as f32;
        let off_x = rect.x / layer_w;
        let off_y = rect.y / layer_h;
        // Use absolute pixel scale
        let scale_x = 1. / layer_w;
        let scale_y = 1. / layer_h;
        //let model = glam::Mat4::IDENTITY;
        let model = glam::Mat4::from_translation(glam::Vec3::new(off_x, off_y, 0.)) *
            glam::Mat4::from_scale(glam::Vec3::new(scale_x, scale_y, 1.));

        let mut uniforms_data = [0u8; 128];
        let data: [u8; 64] = unsafe { std::mem::transmute_copy(&render.proj) };
        uniforms_data[0..64].copy_from_slice(&data);
        let data: [u8; 64] = unsafe { std::mem::transmute_copy(&model) };
        uniforms_data[64..].copy_from_slice(&data);
        assert_eq!(128, 2 * UniformType::Mat4.size());

        render.ctx.apply_uniforms_from_bytes(uniforms_data.as_ptr(), uniforms_data.len());

        self.apply_cursor_scrolling(&rect);

        let node = render.scene_graph.get_node(node_id).unwrap();
        let debug = self.debug.get();
        let baseline = self.baseline.get();
        let scroll = self.scroll.get();
        let cursor_pos = self.cursor_pos.get() as usize;
        let cursor_color = self.cursor_color.get();
        let text_color = self.text_color.get();

        let glyphs = &*self.glyphs.lock().unwrap();
        let atlas = &mut self.atlas.lock().unwrap();

        if !self.selected.is_null(0)? && !self.selected.is_null(1)? {
            self.render_selected(render, &rect, glyphs)?;
        }

        let bound = Rectangle { x: 0., y: 0., w: rect.w, h: rect.h };

        let mut rhs = 0.;
        for (glyph_idx, glyph) in glyphs.iter().enumerate() {
            let x1 = glyph.pos.x + scroll;
            let y1 = glyph.pos.y + baseline;
            let x2 = x1 + glyph.pos.w;
            let y2 = y1 + glyph.pos.h;

            let texture = if atlas.contains_key(&glyph.id) {
                *atlas.get(&glyph.id).unwrap()
            } else {
                let texture = render.ctx.new_texture_from_rgba8(
                    glyph.bmp_width,
                    glyph.bmp_height,
                    &glyph.bmp,
                );
                atlas.insert(glyph.id, texture);
                texture
            };
            //let texture = render.ctx.new_texture_from_rgba8(glyph.bmp_width, glyph.bmp_height, &glyph.bmp);
            render.render_clipped_box_with_texture(&bound, x1, y1, x2, y2, COLOR_WHITE, texture);
            //render.render_box_with_texture(x1, y1, x2, y2, COLOR_WHITE, texture);
            //render.ctx.delete_texture(texture);

            // Glyph outlines
            //if debug {
            //    render.outline(x1, y1, x2, y2, COLOR_BLUE, 1.);
            //}

            if cursor_pos != 0 && cursor_pos == glyph_idx {
                render.render_box(x1 - CURSOR_WIDTH, 0., x1, rect.h, cursor_color);
            }

            rhs = x2;
        }

        if cursor_pos == 0 {
            render.render_box(0., 0., CURSOR_WIDTH, rect.h, cursor_color);
        } else if cursor_pos == glyphs.len() {
            render.render_box(rhs - CURSOR_WIDTH, 0., rhs, rect.h, cursor_color);
        }

        if debug {
            let outline_color = if self.is_active.get() { COLOR_GREEN } else { COLOR_DARKGREY };
            // Baseline
            //render.hline(0., rhs, 0., COLOR_RED, 1.);
            render.outline(0., 0., rect.w, rect.h, outline_color, 1.);
        }

        Ok(())
    }

    pub fn render_selected<'a>(
        &self,
        render: &mut RenderContext<'a>,
        rect: &Rectangle<f32>,
        glyphs: &Vec<Glyph>,
    ) -> Result<()> {
        let start = self.selected.get_u32(0)? as usize;
        let end = self.selected.get_u32(1)? as usize;

        let sel_start = std::cmp::min(start, end);
        let sel_end = std::cmp::max(start, end);

        let scroll = self.scroll.get();
        let hi_bg_color = self.hi_bg_color.get();

        let mut start_x = 0.;
        let mut end_x = 0.;

        for (glyph_idx, glyph) in glyphs.iter().enumerate() {
            let x1 = glyph.pos.x + scroll;

            if glyph_idx == sel_start {
                start_x = x1;
            }
            if glyph_idx == sel_end {
                end_x = x1;
            }
        }
        if sel_start == 0 {
            start_x = scroll;
        }
        if sel_end == glyphs.len() {
            let glyph = &glyphs.last().unwrap();
            let x2 = glyph.pos.x + scroll + glyph.pos.w;
            end_x = x2;
        }

        // Apply clipping
        if start_x < 0. {
            start_x = 0.;
        }
        if end_x > rect.w {
            end_x = rect.w;
        }
        render.render_box(start_x, 0., end_x, rect.h, hi_bg_color);
        Ok(())
    }

    fn delete_highlighted(&self) {
        assert!(!self.selected.is_null(0).unwrap());
        assert!(!self.selected.is_null(1).unwrap());

        let start = self.selected.get_u32(0).unwrap() as usize;
        let end = self.selected.get_u32(1).unwrap() as usize;

        let sel_start = std::cmp::min(start, end);
        let sel_end = std::cmp::max(start, end);

        let glyphs = &*self.glyphs.lock().unwrap();

        // Regen text
        let mut text = String::new();
        for (i, glyph) in glyphs.iter().enumerate() {
            let mut substr = glyph.substr.clone();
            if sel_start <= i && i < sel_end {
                continue
            }
            text.push_str(&substr);
        }
        debug!(
            "EditBox(\"{}\")::delete_highlighted() text=\"{}\", cursor_pos={}",
            self.node_name, text, sel_start
        );
        self.text.set(text);

        self.selected.set_null(0).unwrap();
        self.selected.set_null(1).unwrap();
        self.cursor_pos.set(sel_start as u32);
    }

    fn apply_cursor_scrolling(&self, rect: &Rectangle<f32>) {
        let cursor_pos = self.cursor_pos.get() as usize;
        let mut scroll = self.scroll.get();

        let cursor_x = {
            let glyphs = &*self.glyphs.lock().unwrap();
            if cursor_pos == 0 {
                0.
            } else if cursor_pos == glyphs.len() {
                let glyph = &glyphs.last().unwrap();
                glyph.pos.x + glyph.pos.w
            } else {
                assert!(cursor_pos < glyphs.len());
                let glyph = &glyphs[cursor_pos];
                glyph.pos.x
            }
        };

        let cursor_lhs = cursor_x + scroll;
        let cursor_rhs = cursor_lhs + CURSOR_WIDTH;

        if cursor_rhs > rect.w {
            scroll = rect.w - cursor_x;
        } else if cursor_lhs < 0. {
            scroll = -cursor_x + CURSOR_WIDTH;
        }

        self.scroll.set(scroll);
    }

    fn regen_glyphs(&self) -> Result<()> {
        let font_size = self.window_scale * self.font_size.get();

        debug!("shape start");
        let glyphs = self.text_shaper.shape(self.text.get(), font_size, self.text_color.get());
        debug!("shape end");
        if self.cursor_pos.get() > glyphs.len() as u32 {
            self.cursor_pos.set(glyphs.len() as u32);
        }
        *self.glyphs.lock().unwrap() = glyphs;
        Ok(())
    }

    fn key_down(self: Arc<Self>, key: String, mods: KeyMods, repeat: bool) {
        if !self.is_active.get() {
            return
        }
        let actions = {
            let mut repeater = self.key_repeat.lock().unwrap();
            repeater.key_down(&key, repeat)
        };
        for _ in 0..actions {
            self.do_key_down(&key, &mods)
        }
    }

    fn do_key_down(&self, key: &str, mods: &KeyMods) {
        match key {
            "Left" => {
                let mut cursor_pos = self.cursor_pos.get();

                // Start selection if shift is held
                if !mods.shift {
                    self.selected.set_null(0).unwrap();
                    self.selected.set_null(1).unwrap();
                } else if self.selected.is_null(0).unwrap() {
                    assert!(self.selected.is_null(1).unwrap());
                    self.selected.set_u32(0, cursor_pos).unwrap();
                }

                if cursor_pos > 0 {
                    cursor_pos -= 1;
                    debug!(
                        "EditBox(\"{}\")::key_down(Left) cursor_pos={}",
                        self.node_name, cursor_pos
                    );
                    self.cursor_pos.set(cursor_pos);
                }

                // Update selection
                if mods.shift {
                    self.selected.set_u32(1, cursor_pos).unwrap();
                }
            }
            "Right" => {
                let mut cursor_pos = self.cursor_pos.get();

                // Start selection if shift is held
                if !mods.shift {
                    self.selected.set_null(0).unwrap();
                    self.selected.set_null(1).unwrap();
                } else if self.selected.is_null(0).unwrap() {
                    assert!(self.selected.is_null(1).unwrap());
                    self.selected.set_u32(0, cursor_pos).unwrap();
                }

                let glyphs_len = self.glyphs.lock().unwrap().len() as u32;
                if cursor_pos < glyphs_len {
                    cursor_pos += 1;
                    debug!(
                        "EditBox(\"{}\")::key_down(Right) cursor_pos={}",
                        self.node_name, cursor_pos
                    );
                    self.cursor_pos.set(cursor_pos);
                }

                // Update selection
                if mods.shift {
                    self.selected.set_u32(1, cursor_pos).unwrap();
                }
            }
            "Delete" => {
                let cursor_pos = self.cursor_pos.get();

                let _text = if !self.selected.is_null(0).unwrap() {
                    self.delete_highlighted();
                } else {
                    let glyphs = &*self.glyphs.lock().unwrap();

                    if cursor_pos == glyphs.len() as u32 {
                        return;
                    }

                    // Regen text
                    let mut text = String::new();
                    for (i, glyph) in glyphs.iter().enumerate() {
                        let mut substr = glyph.substr.clone();
                        if cursor_pos as usize == i {
                            // Lmk if anyone knows a better way to do substr.pop_front()
                            let mut chars = substr.chars();
                            chars.next();
                            substr = chars.as_str().to_string();
                        }
                        text.push_str(&substr);
                    }
                    self.text.set(text);
                };
                self.regen_glyphs().unwrap();
            }
            "Backspace" => {
                let cursor_pos = self.cursor_pos.get();

                let _text = if !self.selected.is_null(0).unwrap() {
                    self.delete_highlighted();
                } else {
                    if cursor_pos == 0 {
                        return;
                    }

                    let glyphs = &*self.glyphs.lock().unwrap();

                    let mut text = String::new();
                    for (i, glyph) in glyphs.iter().enumerate() {
                        let mut substr = glyph.substr.clone();
                        if cursor_pos as usize - 1 == i {
                            substr.pop().unwrap();
                        }
                        text.push_str(&substr);
                    }
                    self.text.set(text);
                    self.cursor_pos.set(cursor_pos - 1);
                };
                self.regen_glyphs().unwrap();
            }
            "C" => {
                if mods.ctrl {
                    self.copy_highlighted_text().unwrap();
                } else {
                    self.insert_char(key, mods);
                }
            }
            "V" => {
                if mods.ctrl {
                    if let Some(text) = window::clipboard_get() {
                        self.insert_text(text);
                    }
                } else {
                    self.insert_char(key, mods);
                }
            }
            _ => {
                self.insert_char(key, mods);
            }
        }
    }

    fn insert_char(&self, key: &str, mods: &KeyMods) {
        // First filter for only single digit keys
        let allowed_keys = [
            "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q",
            "R", "S", "T", "U", "V", "W", "X", "Y", "Z", " ", ":", ";", "'", "-", ".", "/", "=",
            "(", "\\", ")", "`", "0", "1", "2", "3", "4", "5", "6", "7", "8", "9",
        ];
        if !allowed_keys.contains(&key) {
            return
        }

        // If we want to only allow specific chars in a String here
        //let ch = key.chars().next().unwrap();
        // if !self.allowed_chars.chars().any(|c| c == ch) { return }

        let key = if mods.shift { key.to_string() } else { key.to_lowercase() };

        self.insert_text(key);
    }

    fn insert_text(&self, key: String) {
        let mut text = String::new();

        let cursor_pos = self.cursor_pos.get();

        if cursor_pos == 0 {
            text = key;
        } else {
            let glyphs = &*self.glyphs.lock().unwrap();
            for (glyph_idx, glyph) in glyphs.iter().enumerate() {
                text.push_str(&glyph.substr);
                if cursor_pos == glyph_idx as u32 + 1 {
                    text.push_str(&key);
                }
            }
        }
        self.text.set(text);
        // Not always true lol
        self.cursor_pos.set(cursor_pos + 1);
        self.regen_glyphs().unwrap();
    }

    fn copy_highlighted_text(&self) -> Result<()> {
        let start = self.selected.get_u32(0)? as usize;
        let end = self.selected.get_u32(1)? as usize;

        let sel_start = std::cmp::min(start, end);
        let sel_end = std::cmp::max(start, end);

        let mut text = String::new();

        let glyphs = &*self.glyphs.lock().unwrap();
        for (glyph_idx, glyph) in glyphs.iter().enumerate() {
            if sel_start <= glyph_idx && glyph_idx < sel_end {
                text.push_str(&glyph.substr);
            }
        }

        info!("Copied '{}'", text);
        window::clipboard_set(&text);
        Ok(())
    }

    fn key_up(self: Arc<Self>, key: String, mods: KeyMods) {
        let mut repeater = self.key_repeat.lock().unwrap();
        repeater.key_up(&key);
    }

    fn mouse_button_down(self: Arc<Self>, button: MouseButton, x: f32, y: f32) {
        let mouse_pos = Point { x, y };
        let rect = self.world_rect.lock().unwrap().clone();
        debug!("mouse down event {} {} {} {} {} {}", x, y, rect.x, rect.y, rect.w, rect.h);

        // clicking inside box will:
        // 1. make it active
        // 2. begin selection
        if rect.contains(&mouse_pos) {
            debug!("showkeyb!");
            window::show_keyboard(true);
            if !self.is_active.get() {
                self.is_active.set(true);
                println!("inside!");
                // Send signal
            }

            let cpos = match self.find_closest_glyph_idx(x) {
                MouseClickGlyph::Pos(cpos) => cpos,
                _ => panic!("shouldn't be possible to reach here!"),
            };

            // set cursor pos
            self.cursor_pos.set(cpos);

            // begin selection
            self.selected.set_u32(0, cpos).unwrap();
            self.selected.set_u32(1, cpos).unwrap();
            self.mouse_btn_held.store(true, Ordering::Relaxed);
        }
        // click outside the box will:
        // 1. make it inactive
        else {
            if self.is_active.get() {
                self.is_active.set(false);
                // Send signal
            }
        }
    }
    fn mouse_button_up(self: Arc<Self>, button: MouseButton, x: f32, y: f32) {
        // releasing mouse button will:
        // 1. end selection
        self.mouse_btn_held.store(false, Ordering::Relaxed);
    }
    fn mouse_move(self: Arc<Self>, x: f32, y: f32) {
        if !self.mouse_btn_held.load(Ordering::Relaxed) {
            return;
        }

        // if active and selection_active, then use x to modify the selection.
        // also implement scrolling when cursor is to the left or right
        // just scroll to the end
        // also set cursor_pos too

        let cpos = match self.find_closest_glyph_idx(x) {
            MouseClickGlyph::Lhs => 0,
            MouseClickGlyph::Pos(cpos) => cpos,
            MouseClickGlyph::Rhs(cpos) => cpos,
        };

        self.cursor_pos.set(cpos);
        self.selected.set_u32(1, cpos).unwrap();
    }

    // Uses screen x pos
    fn find_closest_glyph_idx(&self, x: f32) -> MouseClickGlyph {
        let rect = self.world_rect.lock().unwrap().clone();
        let glyphs = &*self.glyphs.lock().unwrap();

        let mouse_x = x - rect.x;

        if mouse_x > rect.w {
            // Highlight to the end
            let cpos = glyphs.len() as u32;
            return MouseClickGlyph::Rhs(cpos);
            // Scroll to the right handled in render
        } else if mouse_x < 0. {
            return MouseClickGlyph::Lhs;
        }

        let scroll = self.scroll.get();

        let mouse_x = x - rect.x;
        let mut cpos = 0;
        let lhs = 0.;
        let mut last_d = (lhs - mouse_x).abs();

        for (i, glyph) in glyphs.iter().skip(1).enumerate() {
            // Because we skip the first item
            let glyph_idx = (i + 1) as u32;

            let x1 = glyph.pos.x + scroll;
            let curr_d = (x1 - mouse_x).abs();
            if curr_d < last_d {
                last_d = curr_d;
                cpos = glyph_idx;
            }
        }
        // also check the right hand side
        let rhs = {
            let glyph = &glyphs.last().unwrap();
            glyph.pos.x + scroll + glyph.pos.w
        };
        let curr_d = (rhs - mouse_x).abs();
        if curr_d < last_d {
            //last_d = curr_d;
            cpos = glyphs.len() as u32;
        }

        MouseClickGlyph::Pos(cpos)
    }

    fn window_resize(self: Arc<Self>, w: f32, h: f32) {}
}

enum MouseClickGlyph {
    Lhs,
    Pos(u32),
    Rhs(u32),
}
