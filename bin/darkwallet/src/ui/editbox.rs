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

use async_trait::async_trait;
use atomic_float::AtomicF32;
use miniquad::{window, KeyCode, KeyMods, MouseButton, TouchPhase};
use rand::{rngs::OsRng, Rng};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex as SyncMutex, Weak,
    },
    time::Instant,
};

use crate::{
    error::Result,
    gfx::{
        GfxDrawCall, GfxDrawInstruction, GfxDrawMesh, GfxTextureId, GraphicsEventPublisherPtr,
        Point, Rectangle, RenderApiPtr,
    },
    mesh::{MeshBuilder, MeshInfo, COLOR_BLUE, COLOR_WHITE},
    prop::{
        PropertyBool, PropertyColor, PropertyFloat32, PropertyPtr, PropertyRect, PropertyStr,
        PropertyUint32, Role,
    },
    pubsub::Subscription,
    scene::{Pimpl, SceneNodeWeak},
    text::{self, Glyph, GlyphPositionIter, TextShaperPtr},
    util::is_whitespace,
    ExecutorPtr,
};

use super::{DrawUpdate, OnModify, UIObject};

// Pixel width of the cursor
const CURSOR_WIDTH: f32 = 2.;
// EOL whitespace is given a nudge since it has a width of 0 after text shaping
const CURSOR_EOL_WS_NUDGE: f32 = 0.8;
// EOL chars are more aesthetic when given a smallish nudge
const CURSOR_EOL_NUDGE: f32 = 0.2;

fn eol_nudge(font_size: f32, glyphs: &Vec<Glyph>) -> f32 {
    if is_whitespace(&glyphs.last().unwrap().substr) {
        (font_size * CURSOR_EOL_WS_NUDGE).round()
    } else {
        (font_size * CURSOR_EOL_NUDGE).round()
    }
}

#[derive(Debug, Clone, Eq, Hash, PartialEq)]
enum PressedKey {
    Char(char),
    Key(KeyCode),
}

/// On key press (repeat=false), we immediately process the event.
/// Then there's a delay (repeat=true) and then for every step time
/// while key press events are being sent, we allow an event.
/// This ensures smooth typing in the editbox.
struct PressedKeysSmoothRepeat {
    /// When holding keys, we track from start and last sent time.
    /// This is useful for initial delay and smooth scrolling.
    pressed_keys: HashMap<PressedKey, RepeatingKeyTimer>,
    /// Initial delay before allowing keys
    start_delay: u32,
    /// Minimum time between repeated keys
    step_time: u32,
}

impl PressedKeysSmoothRepeat {
    fn new(start_delay: u32, step_time: u32) -> Self {
        Self { pressed_keys: HashMap::new(), start_delay, step_time }
    }

    fn key_down(&mut self, key: PressedKey, repeat: bool) -> u32 {
        //debug!(target: "PressedKeysSmoothRepeat", "key_down({:?}, {})", key, repeat);

        if !repeat {
            self.pressed_keys.remove(&key);
            return 1;
        }

        // Insert key if not exists
        if !self.pressed_keys.contains_key(&key) {
            //debug!(target: "PressedKeysSmoothRepeat", "insert key {:?}", key);
            self.pressed_keys.insert(key.clone(), RepeatingKeyTimer::new());
        }

        let repeater = self.pressed_keys.get_mut(&key).expect("repeat map");
        repeater.update(self.start_delay, self.step_time)
    }

    /*
    fn key_up(&mut self, key: &PressedKey) {
        //debug!(target: "PressedKeysSmoothRepeat", "key_up({:?})", key);
        println!("{:?}", self.pressed_keys.keys());
        assert!(self.pressed_keys.contains_key(key));
        self.pressed_keys.remove(key).expect("key was pressed");
    }
    */
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
        //debug!(target: "RepeatingKeyTimer", "update() elapsed={}, actions={}",
        //       elapsed, self.actions);
        if elapsed < start_delay as u128 {
            return 0
        }
        let total_actions = ((elapsed - start_delay as u128) / step_time as u128) as u32;
        let remaining_actions = total_actions - self.actions;
        self.actions = total_actions;
        remaining_actions
    }
}

#[derive(Clone)]
struct TextRenderInfo {
    mesh: MeshInfo,
    texture_id: GfxTextureId,
}

pub type EditBoxPtr = Arc<EditBox>;

pub struct EditBox {
    node: SceneNodeWeak,
    #[allow(dead_code)]
    tasks: Vec<smol::Task<()>>,
    render_api: RenderApiPtr,
    text_shaper: TextShaperPtr,
    key_repeat: SyncMutex<PressedKeysSmoothRepeat>,

    render_info: SyncMutex<Option<TextRenderInfo>>,
    glyphs: SyncMutex<Vec<Glyph>>,
    dc_key: u64,

    is_active: PropertyBool,
    is_focused: PropertyBool,
    rect: PropertyRect,
    baseline: PropertyFloat32,
    scroll: PropertyFloat32,
    cursor_pos: PropertyUint32,
    font_size: PropertyFloat32,
    text: PropertyStr,
    text_color: PropertyColor,
    cursor_color: PropertyColor,
    hi_bg_color: PropertyColor,
    selected: PropertyPtr,
    z_index: PropertyUint32,
    debug: PropertyBool,

    mouse_btn_held: AtomicBool,

    old_window_scale: AtomicF32,
    window_scale: PropertyFloat32,
    parent_rect: SyncMutex<Option<Rectangle>>,
}

impl EditBox {
    pub async fn new(
        node: SceneNodeWeak,
        window_scale: PropertyFloat32,
        render_api: RenderApiPtr,
        text_shaper: TextShaperPtr,
        ex: ExecutorPtr,
    ) -> Pimpl {
        debug!(target: "ui::editbox", "EditBox::new()");

        let node_ref = &node.upgrade().unwrap();
        let is_active = PropertyBool::wrap(node_ref, Role::Internal, "is_active", 0).unwrap();
        let is_focused = PropertyBool::wrap(node_ref, Role::Internal, "is_focused", 0).unwrap();
        let rect = PropertyRect::wrap(node_ref, Role::Internal, "rect").unwrap();
        let baseline = PropertyFloat32::wrap(node_ref, Role::Internal, "baseline", 0).unwrap();
        let scroll = PropertyFloat32::wrap(node_ref, Role::Internal, "scroll", 0).unwrap();
        let cursor_pos = PropertyUint32::wrap(node_ref, Role::Internal, "cursor_pos", 0).unwrap();
        let font_size = PropertyFloat32::wrap(node_ref, Role::Internal, "font_size", 0).unwrap();
        let text = PropertyStr::wrap(node_ref, Role::Internal, "text", 0).unwrap();
        let text_color = PropertyColor::wrap(node_ref, Role::Internal, "text_color").unwrap();
        let cursor_color = PropertyColor::wrap(node_ref, Role::Internal, "cursor_color").unwrap();
        let hi_bg_color = PropertyColor::wrap(node_ref, Role::Internal, "hi_bg_color").unwrap();
        let selected = node_ref.get_property("selected").unwrap();
        let z_index = PropertyUint32::wrap(node_ref, Role::Internal, "z_index", 0).unwrap();
        let debug = PropertyBool::wrap(node_ref, Role::Internal, "debug", 0).unwrap();

        let node_name = node_ref.name.clone();
        let node_id = node_ref.id;

        // Must do this whenever the text changes
        let glyphs = text_shaper.shape(text.get(), font_size.get(), window_scale.get()).await;

        let self_ = Arc::new_cyclic(|me: &Weak<Self>| {
            let mut on_modify = OnModify::new(ex, node_name, node_id, me.clone());
            on_modify.when_change(is_focused.prop(), Self::change_focus);

            // When text has been changed.
            // Cursor and selection might be invalidated.
            async fn reset(self_: Arc<EditBox>) {
                self_.cursor_pos.set(0);
                self_.selected.set_null(Role::Internal, 0).unwrap();
                self_.selected.set_null(Role::Internal, 1).unwrap();
                self_.scroll.set(0.);
                self_.regen_glyphs().await;
                self_.redraw().await;
            }
            async fn redraw(self_: Arc<EditBox>) {
                self_.redraw().await;
            }
            on_modify.when_change(rect.prop(), redraw);
            on_modify.when_change(baseline.prop(), redraw);
            // The commented properties are modified on input events
            // So then redraw() will get repeatedly triggered when these properties
            // are changed. We should find a solution. For now the hooks are disabled.
            //on_modify.when_change(scroll.prop(), redraw);
            //on_modify.when_change(cursor_pos.prop(), redraw);
            on_modify.when_change(font_size.prop(), redraw);
            // We must also reshape text
            on_modify.when_change(text.prop(), reset);
            on_modify.when_change(text_color.prop(), redraw);
            on_modify.when_change(cursor_color.prop(), redraw);
            on_modify.when_change(hi_bg_color.prop(), redraw);
            //on_modify.when_change(selected.clone(), redraw);
            on_modify.when_change(z_index.prop(), redraw);
            on_modify.when_change(debug.prop(), redraw);

            let tasks = on_modify.tasks;

            Self {
                node,
                tasks,
                render_api,
                text_shaper,
                key_repeat: SyncMutex::new(PressedKeysSmoothRepeat::new(400, 50)),

                render_info: SyncMutex::new(None),
                glyphs: SyncMutex::new(glyphs),
                dc_key: OsRng.gen(),

                is_active,
                is_focused,
                rect,
                baseline,
                scroll,
                cursor_pos,
                font_size,
                text,
                text_color,
                cursor_color,
                hi_bg_color,
                selected,
                z_index,
                debug,

                mouse_btn_held: AtomicBool::new(false),

                old_window_scale: AtomicF32::new(window_scale.get()),
                window_scale,
                parent_rect: SyncMutex::new(None),
            }
        });

        Pimpl::EditBox(self_)
    }

    /// This MUST be called whenever the text property is changed.
    async fn regen_glyphs(&self) {
        let font_size = self.font_size.get();
        let window_scale = self.window_scale.get();
        let glyphs = self.text_shaper.shape(self.text.get(), font_size, window_scale).await;
        // TODO: we aren't freeing textures
        *self.glyphs.lock().unwrap() = glyphs;
    }

    /// Called whenever the text or any text property changes.
    /// Not related to cursor, text highlighting or bounding (clip) rects.
    fn regen_mesh(&self, mut clip: Rectangle) -> TextRenderInfo {
        clip.x = 0.;
        clip.y = 0.;

        let is_focused = self.is_focused.get();
        let text = self.text.get();
        let font_size = self.font_size.get();
        let window_scale = self.window_scale.get();
        let text_color = self.text_color.get();
        let baseline = self.baseline.get();
        let scroll = self.scroll.get();
        let cursor_pos = self.cursor_pos.get() as usize;
        let cursor_color = self.cursor_color.get();
        let debug = self.debug.get();
        debug!(target: "ui::editbox", "Rendering text '{text}' clip={clip:?}");
        debug!(target: "ui::editbox", "    cursor_pos={cursor_pos}, is_focused={is_focused}");

        let glyphs = self.glyphs.lock().unwrap().clone();
        let atlas = text::make_texture_atlas(&self.render_api, &glyphs);

        let mut mesh = MeshBuilder::with_clip(clip.clone());
        self.draw_selected(&mut mesh, &glyphs, clip.h).unwrap();

        let glyph_pos_iter = GlyphPositionIter::new(font_size, window_scale, &glyphs, baseline);
        // Used for drawing the cursor when it's at the end of the line.
        let mut rhs = 0.;

        for (glyph_idx, (mut glyph_rect, glyph)) in glyph_pos_iter.zip(glyphs.iter()).enumerate() {
            let uv_rect = atlas.fetch_uv(glyph.glyph_id).expect("missing glyph UV rect");

            glyph_rect.x -= scroll;

            //mesh.draw_outline(&glyph_rect, COLOR_BLUE, 2.);
            let mut color = text_color.clone();
            if glyph.sprite.has_color {
                color = COLOR_WHITE;
            }
            mesh.draw_box(&glyph_rect, color, uv_rect);

            if is_focused && cursor_pos != 0 && cursor_pos == glyph_idx {
                let cursor_rect = Rectangle { x: glyph_rect.x, y: 0., w: CURSOR_WIDTH, h: clip.h };
                mesh.draw_box(&cursor_rect, cursor_color, &Rectangle::zero());
            }

            rhs = glyph_rect.rhs();
        }

        if is_focused && cursor_pos == 0 {
            let cursor_rect = Rectangle { x: 0., y: 0., w: CURSOR_WIDTH, h: clip.h };
            mesh.draw_box(&cursor_rect, cursor_color, &Rectangle::zero());
        } else if is_focused && cursor_pos == glyphs.len() {
            rhs += eol_nudge(font_size, &glyphs);

            let cursor_rect = Rectangle { x: rhs, y: 0., w: CURSOR_WIDTH, h: clip.h };
            mesh.draw_box(&cursor_rect, cursor_color, &Rectangle::zero());
        }

        if debug {
            mesh.draw_outline(&clip, COLOR_BLUE, 1.);
        }

        let mesh = mesh.alloc(&self.render_api);

        TextRenderInfo { mesh, texture_id: atlas.texture_id }
    }

    fn draw_selected(
        &self,
        mesh: &mut MeshBuilder,
        glyphs: &Vec<Glyph>,
        clip_h: f32,
    ) -> Result<()> {
        if self.selected.is_null(0)? || self.selected.is_null(1)? {
            // Nothing selected so do nothing
            return Ok(())
        }
        let start = self.selected.get_u32(0)? as usize;
        let end = self.selected.get_u32(1)? as usize;

        // Selection started but nothing selected yet so do nothing
        if start == end {
            return Ok(())
        }

        let sel_start = std::cmp::min(start, end);
        let sel_end = std::cmp::max(start, end);

        let font_size = self.font_size.get();
        let window_scale = self.window_scale.get();
        let baseline = self.baseline.get();
        let scroll = self.scroll.get();
        let hi_bg_color = self.hi_bg_color.get();
        let glyph_pos_iter = GlyphPositionIter::new(font_size, window_scale, &glyphs, baseline);

        let mut start_x = 0.;
        let mut end_x = 0.;
        // When cursor lands at the end of the line
        let mut rhs = 0.;

        for (glyph_idx, mut glyph_rect) in glyph_pos_iter.enumerate() {
            glyph_rect.x -= scroll;

            if glyph_idx == sel_start {
                start_x = glyph_rect.x;
            }
            if glyph_idx == sel_end {
                end_x = glyph_rect.x;
            }

            rhs = glyph_rect.rhs();
        }

        if sel_start == 0 {
            start_x = scroll;
        }

        if sel_end == glyphs.len() {
            rhs += eol_nudge(font_size, &glyphs);
            end_x = rhs;
        }

        // We don't need to do manual clipping since MeshBuilder should do that
        let select_rect = Rectangle { x: start_x, y: 0., w: end_x - start_x, h: clip_h };
        mesh.draw_box(&select_rect, hi_bg_color, &Rectangle::zero());
        Ok(())
    }

    async fn change_focus(self: Arc<Self>) {
        if !self.is_active.get() {
            return
        }
        debug!(target: "ui::editbox", "Focus changed");

        // Cursor visibility will change so just redraw everything lol
        self.redraw().await;
    }

    async fn handle_click_down(&self, btn: MouseButton, mouse_pos: Point) -> bool {
        if btn != MouseButton::Left {
            return false
        }

        let rect = self.rect.get();

        // clicking inside box will:
        // 1. make it active
        // 2. begin selection
        if !rect.contains(mouse_pos) {
            if self.is_focused.get() {
                debug!(target: "ui::editbox", "EditBox unfocused");
                self.is_focused.set(false);
                self.selected.set_null(Role::Internal, 0).unwrap();
                self.selected.set_null(Role::Internal, 1).unwrap();

                self.redraw().await;
            }
            return false
        }

        window::show_keyboard(true);

        if self.is_focused.get() {
            debug!(target: "ui::editbox", "EditBox clicked");
        } else {
            debug!(target: "ui::editbox", "EditBox focused");
            self.is_focused.set(true);
        }

        let cpos = self.find_closest_glyph_idx(mouse_pos.x, &rect);

        // set cursor pos
        self.cursor_pos.set(cpos);
        self.apply_cursor_scrolling();

        // begin selection
        self.selected.set_u32(Role::Internal, 0, cpos).unwrap();
        self.selected.set_u32(Role::Internal, 1, cpos).unwrap();

        self.mouse_btn_held.store(true, Ordering::Relaxed);

        true
    }
    fn handle_click_up(&self, btn: MouseButton, pos: Point) -> bool {
        if btn != MouseButton::Left {
            return false
        }

        // releasing mouse button will end selection
        self.mouse_btn_held.store(false, Ordering::Relaxed);
        false
    }
    async fn handle_cursor_move(&self, pos: Point) -> bool {
        if !self.mouse_btn_held.load(Ordering::Relaxed) {
            return false;
        }

        // if active and selection_active, then use x to modify the selection.
        // also implement scrolling when cursor is to the left or right
        // just scroll to the end
        // also set cursor_pos too

        let rect = self.rect.get();
        let cpos = self.find_closest_glyph_idx(pos.x, &rect);

        self.cursor_pos.set(cpos);
        self.selected.set_u32(Role::Internal, 1, cpos).unwrap();

        self.apply_cursor_scrolling();
        self.redraw().await;
        false
    }

    /// Used when clicking the text. Given the x coord of the mouse, it finds the index
    /// of the closest glyph to that x coord.
    fn find_closest_glyph_idx(&self, x: f32, rect: &Rectangle) -> u32 {
        let font_size = self.font_size.get();
        let window_scale = self.window_scale.get();
        let baseline = self.baseline.get();
        let glyphs = self.glyphs.lock().unwrap().clone();

        let mouse_x = x - rect.x;

        if mouse_x > rect.w {
            // Highlight to the end
            let cpos = glyphs.len() as u32;
            return cpos;
            // Scroll to the right handled in render
        } else if mouse_x < 0. {
            return 0;
        }

        let scroll = self.scroll.get();

        let mut cpos = 0;
        let lhs = 0.;
        let mut last_d = (lhs - mouse_x).abs();

        let glyph_pos_iter = GlyphPositionIter::new(font_size, window_scale, &glyphs, baseline);
        let mut rhs = 0.;

        for (i, glyph_rect) in glyph_pos_iter.skip(1).enumerate() {
            // Because we skip the first item
            let glyph_idx = (i + 1) as u32;

            let x1 = glyph_rect.x - scroll;

            // I don't know what this is doing but it works so I won't touch it for now.
            let curr_d = (x1 - mouse_x).abs();
            if curr_d < last_d {
                last_d = curr_d;
                cpos = glyph_idx;
            }

            rhs = glyph_rect.rhs();
        }

        // also check the right hand side
        let curr_d = (rhs - mouse_x).abs();
        if curr_d < last_d {
            //last_d = curr_d;
            cpos = glyphs.len() as u32;
        }

        cpos
    }

    async fn insert_char(&self, key: char) {
        if !self.selected.is_null(0).unwrap() {
            self.delete_highlighted();
            self.regen_glyphs().await;
        };

        let mut text = String::new();

        let cursor_pos = self.cursor_pos.get();

        let glyphs = self.glyphs.lock().unwrap().clone();

        // We rebuild the string but insert our substr at cursor_pos.
        // The substr is inserted before cursor_pos, and appending to the end
        // of the string is when cursor_pos = len(str).
        // We can't use String::insert() because sometimes multiple chars are combined
        // into a single glyph. We treat the cursor pos as acting on the substrs
        // themselves.
        for (i, glyph) in glyphs.iter().enumerate() {
            if cursor_pos == i as u32 {
                text.push(key);
            }
            text.push_str(&glyph.substr);
        }
        // Append to the end
        if cursor_pos == glyphs.len() as u32 {
            text.push(key);
        }

        self.text.set(text);
        // Not always true lol
        // If glyphs are recombined, this could get messed up
        // meh lets pretend it doesn't exist for now.
        self.cursor_pos.set(cursor_pos + 1);

        self.regen_glyphs().await;
        self.apply_cursor_scrolling();
        self.redraw().await;
    }

    async fn handle_shortcut(&self, key: char, mods: &KeyMods) {
        debug!(target: "ui::editbox", "handle_shortcut({:?}, {:?})", key, mods);

        match key {
            'c' => {
                if mods.ctrl {
                    self.copy_highlighted().unwrap();
                }
            }
            'v' => {
                if mods.ctrl {
                    if let Some(text) = window::clipboard_get() {
                        self.paste_text(text).await;
                    }
                }
            }
            _ => {}
        }
    }

    async fn handle_key(&self, key: &KeyCode, mods: &KeyMods) {
        debug!(target: "ui::editbox", "handle_key({:?}, {:?})", key, mods);
        match key {
            KeyCode::Left => {
                let mut cursor_pos = self.cursor_pos.get();

                // Start selection if shift is held
                if !mods.shift {
                    self.selected.set_null(Role::Internal, 0).unwrap();
                    self.selected.set_null(Role::Internal, 1).unwrap();
                } else if self.selected.is_null(0).unwrap() {
                    assert!(self.selected.is_null(1).unwrap());
                    self.selected.set_u32(Role::Internal, 0, cursor_pos).unwrap();
                }

                if cursor_pos > 0 {
                    cursor_pos -= 1;
                    debug!(target: "ui::editbox", "Left cursor_pos={}", cursor_pos);
                    self.cursor_pos.set(cursor_pos);
                }

                // Update selection
                if mods.shift {
                    self.selected.set_u32(Role::Internal, 1, cursor_pos).unwrap();
                }

                self.apply_cursor_scrolling();
                self.redraw().await;
            }
            KeyCode::Right => {
                let mut cursor_pos = self.cursor_pos.get();

                // Start selection if shift is held
                if !mods.shift {
                    self.selected.set_null(Role::Internal, 0).unwrap();
                    self.selected.set_null(Role::Internal, 1).unwrap();
                } else if self.selected.is_null(0).unwrap() {
                    assert!(self.selected.is_null(1).unwrap());
                    self.selected.set_u32(Role::Internal, 0, cursor_pos).unwrap();
                }

                let glyphs_len = self.glyphs.lock().unwrap().len() as u32;
                if cursor_pos < glyphs_len {
                    cursor_pos += 1;
                    debug!(target: "ui::editbox", "Right cursor_pos={}", cursor_pos);
                    self.cursor_pos.set(cursor_pos);
                }

                // Update selection
                if mods.shift {
                    self.selected.set_u32(Role::Internal, 1, cursor_pos).unwrap();
                }

                self.apply_cursor_scrolling();
                self.redraw().await;
            }
            //KeyCode::Up,
            //KeyCode::Down,
            //KeyCode::Enter,
            KeyCode::Kp0 => self.insert_char('0').await,
            KeyCode::Kp1 => self.insert_char('1').await,
            KeyCode::Kp2 => self.insert_char('2').await,
            KeyCode::Kp3 => self.insert_char('3').await,
            KeyCode::Kp4 => self.insert_char('4').await,
            KeyCode::Kp5 => self.insert_char('5').await,
            KeyCode::Kp6 => self.insert_char('6').await,
            KeyCode::Kp7 => self.insert_char('7').await,
            KeyCode::Kp8 => self.insert_char('8').await,
            KeyCode::Kp9 => self.insert_char('9').await,
            KeyCode::KpDecimal => self.insert_char('.').await,
            KeyCode::Enter | KeyCode::KpEnter => self.send_event().await,
            KeyCode::Delete => {
                if !self.selected.is_null(0).unwrap() {
                    self.delete_highlighted();
                } else {
                    let glyphs = self.glyphs.lock().unwrap().clone();

                    let cursor_pos = self.cursor_pos.get();
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

                self.regen_glyphs().await;
                self.apply_cursor_scrolling();
                self.redraw().await;
            }
            KeyCode::Backspace => {
                if !self.selected.is_null(0).unwrap() {
                    self.delete_highlighted();
                } else {
                    let glyphs = self.glyphs.lock().unwrap().clone();

                    let cursor_pos = self.cursor_pos.get();
                    if cursor_pos == 0 {
                        return;
                    }

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

                self.regen_glyphs().await;
                self.apply_cursor_scrolling();
                self.redraw().await;
            }
            KeyCode::Home => {
                let cursor_pos = self.cursor_pos.get();

                if !mods.shift {
                    self.selected.set_null(Role::Internal, 0).unwrap();
                    self.selected.set_null(Role::Internal, 1).unwrap();
                } else if self.selected.is_null(0).unwrap() {
                    assert!(self.selected.is_null(1).unwrap());
                    self.selected.set_u32(Role::Internal, 0, cursor_pos).unwrap();
                }

                self.cursor_pos.set(0);

                // Update selection
                if mods.shift {
                    self.selected.set_u32(Role::Internal, 1, cursor_pos).unwrap();
                }

                self.apply_cursor_scrolling();
                self.redraw().await;
            }
            KeyCode::End => {
                let cursor_pos = self.cursor_pos.get();

                if !mods.shift {
                    self.selected.set_null(Role::Internal, 0).unwrap();
                    self.selected.set_null(Role::Internal, 1).unwrap();
                } else if self.selected.is_null(0).unwrap() {
                    assert!(self.selected.is_null(1).unwrap());
                    self.selected.set_u32(Role::Internal, 0, cursor_pos).unwrap();
                }

                let glyphs_len = self.glyphs.lock().unwrap().len();
                self.cursor_pos.set(glyphs_len as u32);

                // Update selection
                if mods.shift {
                    self.selected.set_u32(Role::Internal, 1, cursor_pos).unwrap();
                }

                self.apply_cursor_scrolling();
                self.redraw().await;
            }
            _ => {}
        }
    }

    fn delete_highlighted(&self) {
        assert!(!self.selected.is_null(0).unwrap());
        assert!(!self.selected.is_null(1).unwrap());

        let start = self.selected.get_u32(0).unwrap() as usize;
        let end = self.selected.get_u32(1).unwrap() as usize;

        let sel_start = std::cmp::min(start, end);
        let sel_end = std::cmp::max(start, end);

        let mut text = String::new();
        let glyphs = self.glyphs.lock().unwrap().clone();

        // Regen text
        for (i, glyph) in glyphs.iter().enumerate() {
            if sel_start <= i && i < sel_end {
                continue
            }
            text.push_str(&glyph.substr);
        }

        debug!(
            target: "ui::editbox",
            "delete_highlighted() text=\"{}\", cursor_pos={}",
            text, sel_start
        );
        self.text.set(text);

        self.selected.set_null(Role::Internal, 0).unwrap();
        self.selected.set_null(Role::Internal, 1).unwrap();
        self.cursor_pos.set(sel_start as u32);
    }

    fn copy_highlighted(&self) -> Result<()> {
        let start = self.selected.get_u32(0)? as usize;
        let end = self.selected.get_u32(1)? as usize;

        let sel_start = std::cmp::min(start, end);
        let sel_end = std::cmp::max(start, end);

        let mut text = String::new();

        let glyphs = self.glyphs.lock().unwrap().clone();
        for (glyph_idx, glyph) in glyphs.iter().enumerate() {
            if sel_start <= glyph_idx && glyph_idx < sel_end {
                text.push_str(&glyph.substr);
            }
        }

        info!(target: "ui::editbox", "Copied '{}'", text);
        window::clipboard_set(&text);
        Ok(())
    }

    async fn paste_text(&self, key: String) {
        let mut text = String::new();

        let cursor_pos = self.cursor_pos.get();

        if cursor_pos == 0 {
            text = key.clone();
        }

        let glyphs = self.glyphs.lock().unwrap().clone();
        for (glyph_idx, glyph) in glyphs.iter().enumerate() {
            text.push_str(&glyph.substr);
            if cursor_pos == glyph_idx as u32 + 1 {
                text.push_str(&key);
            }
        }

        self.text.set(text);
        // Not always true lol
        self.cursor_pos.set(cursor_pos + 1);

        self.apply_cursor_scrolling();
        self.redraw().await;
    }

    /// Whenever the cursor property is modified this MUST be called
    /// to recalculate the scroll x property.
    fn apply_cursor_scrolling(&self) {
        let rect = self.rect.get();

        let cursor_pos = self.cursor_pos.get() as usize;
        let mut scroll = self.scroll.get();

        let cursor_x = {
            let font_size = self.font_size.get();
            let window_scale = self.window_scale.get();
            let baseline = self.baseline.get();
            let glyphs = self.glyphs.lock().unwrap().clone();

            let mut glyph_pos_iter =
                GlyphPositionIter::new(font_size, window_scale, &glyphs, baseline);

            if cursor_pos == 0 {
                0.
            } else if cursor_pos == glyphs.len() {
                let glyph_pos = glyph_pos_iter.last().unwrap();

                let rhs = glyph_pos.rhs() + eol_nudge(font_size, &glyphs);
                rhs
            } else {
                assert!(cursor_pos < glyphs.len());
                let glyph_pos = glyph_pos_iter.nth(cursor_pos).expect("glyph pos mismatch glyphs");
                glyph_pos.x
            }
        };

        // The LHS and RHS of the cursor box
        let cursor_lhs = cursor_x - scroll;
        let cursor_rhs = cursor_lhs + CURSOR_WIDTH;

        // RHS is outside
        if cursor_rhs > rect.w {
            // We want a scroll so RHS = w
            // cursor_x - scroll + CURSOR_WIDTH = rect.w
            scroll = cursor_x + CURSOR_WIDTH - rect.w;
        // LHS is negative
        } else if cursor_lhs < 0. {
            // We want scroll so LHS = 0
            // cursor_x - scroll = 0
            scroll = cursor_x;
        }

        self.scroll.set(scroll);
    }

    async fn redraw(&self) {
        let Some(draw_update) = self.draw_cached().await else {
            error!(target: "ui::editbox", "Text failed to draw");
            return;
        };

        self.render_api.replace_draw_calls(draw_update.draw_calls);
        //debug!(target: "ui::editbox", "replace draw calls done");
        for buffer_id in draw_update.freed_buffers {
            self.render_api.delete_buffer(buffer_id);
        }
        for texture_id in draw_update.freed_textures {
            self.render_api.delete_texture(texture_id);
        }
    }

    async fn draw_cached(&self) -> Option<DrawUpdate> {
        let rect = self.rect.get();

        let mut freed_textures = vec![];
        let mut freed_buffers = vec![];

        let window_scale = self.window_scale.get();
        if self.old_window_scale.swap(window_scale, Ordering::Relaxed) != window_scale {
            self.regen_glyphs().await;

            let render_info = std::mem::replace(&mut *self.render_info.lock().unwrap(), None);
            // We're finished with these so clean up.
            if let Some(old) = render_info {
                freed_textures.push(old.texture_id);
                freed_buffers.push(old.mesh.vertex_buffer);
                freed_buffers.push(old.mesh.index_buffer);
            }
        }

        // draw will recalc this when it's None
        let render_info = self.regen_mesh(rect.clone());
        let old_render_info =
            std::mem::replace(&mut *self.render_info.lock().unwrap(), Some(render_info.clone()));

        // We're finished with these so clean up.
        if let Some(old) = old_render_info {
            freed_textures.push(old.texture_id);
            freed_buffers.push(old.mesh.vertex_buffer);
            freed_buffers.push(old.mesh.index_buffer);
        }

        let mesh = GfxDrawMesh {
            vertex_buffer: render_info.mesh.vertex_buffer,
            index_buffer: render_info.mesh.index_buffer,
            texture: Some(render_info.texture_id),
            num_elements: render_info.mesh.num_elements,
        };

        Some(DrawUpdate {
            key: self.dc_key,
            draw_calls: vec![(
                self.dc_key,
                GfxDrawCall {
                    instrs: vec![
                        GfxDrawInstruction::Move(rect.pos()),
                        GfxDrawInstruction::Draw(mesh),
                    ],
                    dcs: vec![],
                    z_index: self.z_index.get(),
                },
            )],
            freed_textures,
            freed_buffers,
        })
    }

    async fn send_event(&self) {
        let text = self.text.get();
        debug!(target: "ui::editbox", "sending text {}", text);

        // This should probably be unset instead
        //self.text.set(String::new());
        //self.cursor_pos.set(0);
        //self.redraw().await;
    }
}

impl Drop for EditBox {
    fn drop(&mut self) {
        let render_info = std::mem::replace(&mut *self.render_info.lock().unwrap(), None);
        // We're finished with these so clean up.
        if let Some(old) = render_info {
            self.render_api.delete_buffer(old.mesh.vertex_buffer);
            self.render_api.delete_buffer(old.mesh.index_buffer);
            self.render_api.delete_texture(old.texture_id);
        }
    }
}

#[async_trait]
impl UIObject for EditBox {
    fn z_index(&self) -> u32 {
        self.z_index.get()
    }

    async fn draw(&self, parent_rect: Rectangle) -> Option<DrawUpdate> {
        debug!(target: "ui::editbox", "EditBox::draw()");
        *self.parent_rect.lock().unwrap() = Some(parent_rect);
        self.rect.eval(&parent_rect).ok()?;
        self.draw_cached().await
    }

    async fn handle_char(&self, key: char, mods: KeyMods, repeat: bool) -> bool {
        // First filter for only single digit keys
        if DISALLOWED_CHARS.contains(&key) {
            return false
        }

        if !self.is_focused.get() {
            return false
        }

        if mods.ctrl || mods.alt {
            if repeat {
                return false
            }
            self.handle_shortcut(key, &mods).await;
            return true
        }

        let actions = {
            let mut repeater = self.key_repeat.lock().unwrap();
            repeater.key_down(PressedKey::Char(key), repeat)
        };
        debug!(target: "ui::editbox", "Key {:?} has {} actions", key, actions);
        for _ in 0..actions {
            self.insert_char(key).await;
        }
        true
    }

    async fn handle_key_down(&self, key: KeyCode, mods: KeyMods, repeat: bool) -> bool {
        // First filter for only single digit keys
        // Avoid processing events handled by insert_char()
        if !ALLOWED_KEYCODES.contains(&key) {
            return false
        }

        if !self.is_focused.get() {
            return false
        }

        let actions = {
            let mut repeater = self.key_repeat.lock().unwrap();
            repeater.key_down(PressedKey::Key(key), repeat)
        };
        // Suppress noisy message
        if actions > 0 {
            debug!(target: "ui::editbox", "Key {:?} has {} actions", key, actions);
        }
        for _ in 0..actions {
            self.handle_key(&key, &mods).await;
        }
        true
    }

    async fn handle_mouse_btn_down(&self, btn: MouseButton, mouse_pos: Point) -> bool {
        if !self.is_active.get() {
            return true
        }

        self.handle_click_down(btn, mouse_pos).await
    }

    async fn handle_mouse_btn_up(&self, btn: MouseButton, mouse_pos: Point) -> bool {
        if !self.is_active.get() {
            return true
        }

        self.handle_click_up(btn, mouse_pos)
    }

    async fn handle_mouse_move(&self, mouse_pos: Point) -> bool {
        if !self.is_active.get() {
            return false
        }

        self.handle_cursor_move(mouse_pos).await
    }

    async fn handle_touch(&self, phase: TouchPhase, id: u64, touch_pos: Point) -> bool {
        if !self.is_active.get() {
            return true
        }

        // Ignore multi-touch
        if id != 0 {
            return false
        }

        // Simulate mouse events
        match phase {
            TouchPhase::Started => self.handle_click_down(MouseButton::Left, touch_pos).await,
            TouchPhase::Moved => self.handle_cursor_move(touch_pos).await,
            TouchPhase::Ended => self.handle_click_up(MouseButton::Left, touch_pos),
            TouchPhase::Cancelled => false,
        }
    }
}

/// Filter these char events from being handled since we handle them
/// using the key_up/key_down events.
/// Avoids duplicate processing of keyboard input events.
static DISALLOWED_CHARS: &'static [char] = &['\r', '\u{8}', '\u{7f}', '\t', '\n'];

/// These keycodes are handled via normal key_up/key_down events.
/// Anything in this list must be disallowed char events.
static ALLOWED_KEYCODES: &'static [KeyCode] = &[
    KeyCode::Left,
    KeyCode::Right,
    KeyCode::Up,
    KeyCode::Down,
    KeyCode::Enter,
    KeyCode::Kp0,
    KeyCode::Kp1,
    KeyCode::Kp2,
    KeyCode::Kp3,
    KeyCode::Kp4,
    KeyCode::Kp5,
    KeyCode::Kp6,
    KeyCode::Kp7,
    KeyCode::Kp8,
    KeyCode::Kp9,
    KeyCode::KpDecimal,
    KeyCode::KpEnter,
    KeyCode::Delete,
    KeyCode::Backspace,
    KeyCode::Home,
    KeyCode::End,
];

/*
impl Stoppable for EditBox {
    async fn stop(&self) {
        // TODO: Delete own draw call

        // Free buffers
        // Should this be in drop?
        //self.render_api.delete_buffer(self.vertex_buffer);
        //self.render_api.delete_buffer(self.index_buffer);
    }
}
*/
