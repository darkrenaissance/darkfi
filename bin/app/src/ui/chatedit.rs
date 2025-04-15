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

use async_lock::Mutex as AsyncMutex;
use async_trait::async_trait;
use atomic_float::AtomicF32;
use darkfi::system::msleep;
use darkfi_serial::{deserialize, Decodable, Encodable, SerialDecodable, SerialEncodable};
use miniquad::{window, KeyCode, KeyMods, MouseButton, TouchPhase};
use parking_lot::Mutex as SyncMutex;
use rand::{rngs::OsRng, Rng};
use std::{
    collections::HashMap,
    io::Cursor,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, OnceLock, Weak,
    },
    time::Instant,
};

use crate::{
    error::Result,
    gfx::{
        GfxDrawCall, GfxDrawInstruction, GfxDrawMesh, GfxTextureId, GraphicsEventPublisherPtr,
        Point, Rectangle, RenderApi, Vertex,
    },
    mesh::{Color, MeshBuilder, MeshInfo, COLOR_BLUE, COLOR_RED, COLOR_WHITE},
    prop::{
        PropertyAtomicGuard, PropertyBool, PropertyColor, PropertyFloat32, PropertyPtr,
        PropertyRect, PropertyStr, PropertyUint32, Role,
    },
    pubsub::Subscription,
    scene::{MethodCallSub, Pimpl, SceneNodePtr, SceneNodeWeak},
    text::{self, Glyph, GlyphPositionIter, TextShaperPtr},
    text2::{self, Editor},
    util::{enumerate_ref, is_whitespace, min_f32, unixtime, zip4},
    AndroidSuggestEvent, ExecutorPtr,
};

use super::{
    editbox::{
        editable::{Editable, RenderedEditable, Selection, TextIdx, TextPos},
        eol_nudge,
        repeat::{PressedKey, PressedKeysSmoothRepeat},
        ALLOWED_KEYCODES, DISALLOWED_CHARS,
    },
    DrawUpdate, OnModify, UIObject,
};

// Minimum dist to update scroll when finger scrolling.
// Avoid updating too much makes scrolling smoother.
const VERT_SCROLL_UPDATE_INC: f32 = 1.;

macro_rules! d { ($($arg:tt)*) => { debug!(target: "ui::chatedit", $($arg)*); } }
macro_rules! t { ($($arg:tt)*) => { trace!(target: "ui::chatedit", $($arg)*); } }

// You must be careful working with string indexes in Java. They are UTF16 string indexs, not UTF8
fn char16_to_byte_index(s: &str, char_idx: usize) -> Option<usize> {
    let utf16_data: Vec<_> = s.encode_utf16().take(char_idx).collect();
    let prestr = String::from_utf16(&utf16_data).ok()?;
    Some(prestr.len())
}
fn byte_to_char16_index(s: &str, byte_idx: usize) -> Option<usize> {
    if byte_idx > s.len() || !s.is_char_boundary(byte_idx) {
        return None;
    }
    Some(s[..byte_idx].encode_utf16().count())
}

#[derive(Clone)]
struct TouchStartInfo {
    pos: Point,
    instant: std::time::Instant,
}

impl TouchStartInfo {
    fn new(pos: Point) -> Self {
        Self { pos, instant: std::time::Instant::now() }
    }
}

#[derive(Clone)]
enum TouchStateAction {
    Inactive,
    Started { pos: Point, instant: std::time::Instant },
    StartSelect,
    Select,
    DragSelectHandle { side: isize },
    ScrollVert { start_pos: Point, scroll_start: f32 },
    SetCursorPos,
}

struct TouchInfo {
    state: TouchStateAction,
    start: Option<TouchStartInfo>,

    scroll: PropertyFloat32,
}

impl TouchInfo {
    fn new(scroll: PropertyFloat32) -> Self {
        Self { state: TouchStateAction::Inactive, start: None, scroll }
    }

    fn start(&mut self, pos: Point) {
        debug!(target: "ui::chatedit::touch", "start touch: Started state");
        self.state = TouchStateAction::Started { pos, instant: std::time::Instant::now() };
    }

    fn stop(&mut self) -> TouchStateAction {
        debug!(target: "ui::chatedit::touch", "stop touch: Inactive state");
        std::mem::replace(&mut self.state, TouchStateAction::Inactive)
    }

    fn update(&mut self, pos: &Point) {
        match &self.state {
            TouchStateAction::Started { pos: start_pos, instant } => {
                let travel_dist = pos.dist_sq(&start_pos);
                let grad = (pos.y - start_pos.y) / (pos.x - start_pos.x);
                let elapsed = instant.elapsed().as_millis();
                //debug!(target: "ui::chatedit::touch", "TouchInfo::update() [travel_dist={travel_dist}, grad={grad}]");

                if travel_dist < 5. {
                    if elapsed > 1000 {
                        debug!(target: "ui::chatedit::touch", "update touch state: Started -> StartSelect");
                        self.state = TouchStateAction::StartSelect;
                    }
                } else if grad.abs() > 0.5 {
                    // Vertical movement
                    debug!(target: "ui::chatedit::touch", "update touch state: Started -> ScrollVert");
                    let scroll_start = self.scroll.get();
                    self.state =
                        TouchStateAction::ScrollVert { start_pos: *start_pos, scroll_start };
                } else {
                    // Horizontal movement
                    debug!(target: "ui::chatedit::touch", "update touch state: Started -> SetCursorPos");
                    self.state = TouchStateAction::SetCursorPos;
                }
            }
            _ => {}
        }
    }
}

/*
impl std::fmt::Debug for Editor {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut changes = vec![];
        let sel = self.editor.raw_selection();
        if sel.is_collapsed() {
            let cursor = sel.focus().index();
            changes.push((cursor, '|'));
        } else {
            let sel = sel.text_range();
            changes.push((sel.start, '{'));
            changes.push((sel.end, '}'));
        }

        if let Some(compose) = self.editor.compose() {
            changes.push((compose.start, '['));
            changes.push((compose.end, ']'));
        }

        changes.sort_by(|a, b| b.0.cmp(&a.0));

        write!(f, "'")?;
        let mut buffer = self.editor.raw_text();
        for (byte_idx, c) in buffer.char_indices() {
            while let Some((idx, d)) = changes.last() {
                if *idx > byte_idx {
                    break
                }

                write!(f, "{}", d)?;
                let _ = changes.pop();
            }

            write!(f, "{}", c)?;
        }
        write!(f, "'")
    }
}
*/

enum ColoringState {
    Start,
    IsCommand,
    Normal,
}

pub type ChatEditPtr = Arc<ChatEdit>;

pub struct ChatEdit {
    node: SceneNodeWeak,
    tasks: OnceLock<Vec<smol::Task<()>>>,
    render_api: RenderApi,
    text_shaper: TextShaperPtr,
    key_repeat: SyncMutex<PressedKeysSmoothRepeat>,

    glyphs: SyncMutex<Vec<Glyph>>,
    main_dc_key: u64,
    /// DC key for the text
    text_dc_key: u64,
    cursor_mesh: SyncMutex<Option<GfxDrawMesh>>,
    /// DC key for the cursor. Allows updating cursor independently.
    cursor_dc_key: u64,

    is_active: PropertyBool,
    is_focused: PropertyBool,
    min_height: PropertyFloat32,
    max_height: PropertyFloat32,
    content_height: PropertyFloat32,
    rect: PropertyRect,
    baseline: PropertyFloat32,
    linespacing: PropertyFloat32,
    descent: PropertyFloat32,
    lineheight: PropertyFloat32,
    scroll: PropertyFloat32,
    scroll_speed: PropertyFloat32,
    padding: PropertyPtr,
    cursor_pos: PropertyUint32,
    font_size: PropertyFloat32,
    text: PropertyStr,
    text_color: PropertyColor,
    text_hi_color: PropertyColor,
    text_cmd_color: PropertyColor,
    cursor_color: PropertyColor,
    cursor_width: PropertyFloat32,
    cursor_ascent: PropertyFloat32,
    cursor_descent: PropertyFloat32,
    cursor_blink_time: PropertyUint32,
    cursor_idle_time: PropertyUint32,
    hi_bg_color: PropertyColor,
    cmd_bg_color: PropertyColor,
    select_ascent: PropertyFloat32,
    select_descent: PropertyFloat32,
    handle_descent: PropertyFloat32,
    select_text: PropertyPtr,
    z_index: PropertyUint32,
    priority: PropertyUint32,
    debug: PropertyBool,

    mouse_btn_held: AtomicBool,
    cursor_is_visible: AtomicBool,
    blink_is_paused: AtomicBool,
    /// Used to explicitly hide the cursor. Must be manually re-enabled.
    hide_cursor: AtomicBool,

    touch_info: SyncMutex<TouchInfo>,
    is_phone_select: AtomicBool,

    old_window_scale: AtomicF32,
    window_scale: PropertyFloat32,
    parent_rect: SyncMutex<Option<Rectangle>>,
    is_mouse_hover: AtomicBool,

    editor: AsyncMutex<Editor>,
}

impl ChatEdit {
    pub async fn new(
        node: SceneNodeWeak,
        window_scale: PropertyFloat32,
        render_api: RenderApi,
        text_shaper: TextShaperPtr,
        ex: ExecutorPtr,
    ) -> Pimpl {
        t!("ChatEdit::new()");

        let node_ref = &node.upgrade().unwrap();
        let is_active = PropertyBool::wrap(node_ref, Role::Internal, "is_active", 0).unwrap();
        let is_focused = PropertyBool::wrap(node_ref, Role::Internal, "is_focused", 0).unwrap();
        let min_height =
            PropertyFloat32::wrap(node_ref, Role::Internal, "height_range", 0).unwrap();
        let max_height =
            PropertyFloat32::wrap(node_ref, Role::Internal, "height_range", 1).unwrap();
        let content_height =
            PropertyFloat32::wrap(node_ref, Role::Internal, "content_height", 0).unwrap();
        let rect = PropertyRect::wrap(node_ref, Role::Internal, "rect").unwrap();
        let baseline = PropertyFloat32::wrap(node_ref, Role::Internal, "baseline", 0).unwrap();
        let linespacing =
            PropertyFloat32::wrap(node_ref, Role::Internal, "linespacing", 0).unwrap();
        let descent = PropertyFloat32::wrap(node_ref, Role::Internal, "descent", 0).unwrap();
        let lineheight = PropertyFloat32::wrap(node_ref, Role::Internal, "lineheight", 0).unwrap();
        let scroll = PropertyFloat32::wrap(node_ref, Role::Internal, "scroll", 0).unwrap();
        let scroll_speed =
            PropertyFloat32::wrap(node_ref, Role::Internal, "scroll_speed", 0).unwrap();
        let padding = node_ref.get_property("padding").unwrap();
        let cursor_pos = PropertyUint32::wrap(node_ref, Role::Internal, "cursor_pos", 0).unwrap();
        let font_size = PropertyFloat32::wrap(node_ref, Role::Internal, "font_size", 0).unwrap();
        let text = PropertyStr::wrap(node_ref, Role::Internal, "text", 0).unwrap();
        let text_color = PropertyColor::wrap(node_ref, Role::Internal, "text_color").unwrap();
        let text_hi_color = PropertyColor::wrap(node_ref, Role::Internal, "text_hi_color").unwrap();
        let text_cmd_color =
            PropertyColor::wrap(node_ref, Role::Internal, "text_cmd_color").unwrap();
        let cursor_color = PropertyColor::wrap(node_ref, Role::Internal, "cursor_color").unwrap();
        let cursor_width =
            PropertyFloat32::wrap(node_ref, Role::Internal, "cursor_width", 0).unwrap();
        let cursor_ascent =
            PropertyFloat32::wrap(node_ref, Role::Internal, "cursor_ascent", 0).unwrap();
        let cursor_descent =
            PropertyFloat32::wrap(node_ref, Role::Internal, "cursor_descent", 0).unwrap();
        let hi_bg_color = PropertyColor::wrap(node_ref, Role::Internal, "hi_bg_color").unwrap();
        let cmd_bg_color = PropertyColor::wrap(node_ref, Role::Internal, "cmd_bg_color").unwrap();
        let select_ascent =
            PropertyFloat32::wrap(node_ref, Role::Internal, "select_ascent", 0).unwrap();
        let select_descent =
            PropertyFloat32::wrap(node_ref, Role::Internal, "select_descent", 0).unwrap();
        let handle_descent =
            PropertyFloat32::wrap(node_ref, Role::Internal, "handle_descent", 0).unwrap();
        let select_text = node_ref.get_property("select_text").unwrap();
        let cursor_blink_time =
            PropertyUint32::wrap(node_ref, Role::Internal, "cursor_blink_time", 0).unwrap();
        let cursor_idle_time =
            PropertyUint32::wrap(node_ref, Role::Internal, "cursor_idle_time", 0).unwrap();
        let z_index = PropertyUint32::wrap(node_ref, Role::Internal, "z_index", 0).unwrap();
        let priority = PropertyUint32::wrap(node_ref, Role::Internal, "priority", 0).unwrap();
        let debug = PropertyBool::wrap(node_ref, Role::Internal, "debug", 0).unwrap();

        let node_name = node_ref.name.clone();
        let node_id = node_ref.id;

        // Must do this whenever the text changes
        let glyphs = text_shaper.shape(text.get(), font_size.get(), window_scale.get());

        let self_ = Arc::new(Self {
            node,
            tasks: OnceLock::new(),
            render_api,
            text_shaper: text_shaper.clone(),
            key_repeat: SyncMutex::new(PressedKeysSmoothRepeat::new(400, 50)),

            glyphs: SyncMutex::new(glyphs),
            main_dc_key: OsRng.gen(),
            text_dc_key: OsRng.gen(),
            cursor_mesh: SyncMutex::new(None),
            cursor_dc_key: OsRng.gen(),

            is_active,
            is_focused,
            min_height,
            max_height,
            content_height,
            rect,
            baseline,
            linespacing,
            descent,
            lineheight: lineheight.clone(),
            scroll: scroll.clone(),
            scroll_speed,
            padding,
            cursor_pos,
            font_size: font_size.clone(),
            text,
            text_color: text_color.clone(),
            text_hi_color,
            text_cmd_color,
            cursor_color,
            cursor_width,
            cursor_ascent,
            cursor_descent,
            cursor_blink_time,
            cursor_idle_time,
            hi_bg_color,
            cmd_bg_color,
            select_ascent,
            select_descent,
            handle_descent,
            select_text,
            z_index,
            priority,
            debug,

            mouse_btn_held: AtomicBool::new(false),
            cursor_is_visible: AtomicBool::new(true),
            blink_is_paused: AtomicBool::new(false),
            hide_cursor: AtomicBool::new(false),

            touch_info: SyncMutex::new(TouchInfo::new(scroll)),
            is_phone_select: AtomicBool::new(false),

            old_window_scale: AtomicF32::new(window_scale.get()),
            window_scale: window_scale.clone(),
            parent_rect: SyncMutex::new(None),
            is_mouse_hover: AtomicBool::new(false),

            editor: AsyncMutex::new(
                Editor::new(font_size, text_color, window_scale, lineheight).await,
            ),
        });

        //self_
        //    .text_wrap
        //    .lock()
        //    .editable
        //    .set_text("".to_string(), "king!😁🍆jelly 🍆1234".to_string());
        //self_.text_wrap.lock().editable.set_text(
        //    "".to_string(),
        //    "A berry is a small, pulpy, and often edible fruit. Typically, berries are juicy, rounded, brightly colored, sweet, sour or tart, and do not have a stone or pit, although many pips or seeds may be present. Common examples of berries in the culinary sense are strawberries, raspberries, blueberries, blackberries, white currants, blackcurrants, and redcurrants. In Britain, soft fruit is a horticultural term for such fruits. The common usage of the term berry is different from the scientific or botanical definition of a berry, which refers to a fruit produced from the ovary of a single flower where the outer layer of the ovary wall develops into an edible fleshy portion (pericarp). The botanical definition includes many fruits that are not commonly known or referred to as berries, such as grapes, tomatoes, cucumbers, eggplants, bananas, and chili peppers.".to_string()
        //);

        Pimpl::ChatEdit(self_)
    }

    fn node(&self) -> SceneNodePtr {
        self.node.upgrade().unwrap()
    }

    fn wrap_width(&self) -> f32 {
        let w = self.rect.prop().get_f32(2).unwrap() - self.cursor_width.get();
        if w < 0. {
            return 0.
        }
        w
    }

    fn abs_to_local(&self, point: &mut Point) {
        let rect = self.rect.get();
        *point -= rect.pos();
        point.y += self.scroll.get();
    }

    fn regen_cursor_mesh(&self) -> GfxDrawMesh {
        let cursor_width = self.cursor_width.get();
        let cursor_ascent = self.cursor_ascent.get();
        let cursor_descent = self.cursor_descent.get();
        let baseline = self.baseline.get();

        let cursor_rect = Rectangle {
            x: 0.,
            y: baseline - cursor_ascent,
            w: cursor_width,
            h: cursor_ascent + cursor_descent,
        };
        let cursor_color = self.cursor_color.get();

        let mut mesh = MeshBuilder::new();
        mesh.draw_filled_box(&cursor_rect, cursor_color);
        mesh.alloc(&self.render_api).draw_untextured()
    }

    /*
    fn draw_phone_select_handle(
        &self,
        mesh: &mut MeshBuilder,
        gpos: TextPos,
        wrapped_lines: &WrappedLines,
        side: f32,
    ) {
        //debug!(target: "ui::chatedit", "draw_phone_select_handle(..., {x}, {side})");
        let baseline = self.baseline.get();
        let select_ascent = self.select_ascent.get();
        let handle_descent = self.handle_descent.get();
        let color = self.text_hi_color.get();
        let linespacing = self.linespacing.get();
        let scroll = self.scroll.get();
        // Transparent for fade
        let mut color_trans = color.clone();
        color_trans[3] = 0.;

        // find start_x
        let (glyph_rect, line_idx) = wrapped_lines.get_glyph_info(gpos);
        let x = glyph_rect.x;
        let y_off = line_idx as f32 * linespacing - scroll;

        let y = y_off + baseline + handle_descent;
        if y < 0. {
            return
        }

        // Vertical line downwards. We use this instead of draw_box() so we have a fade.
        let verts = vec![
            Vertex {
                pos: [x - side * 1., y_off + baseline - select_ascent],
                color: color_trans,
                uv: [0., 0.],
            },
            Vertex {
                pos: [x + side * 4., y_off + baseline - select_ascent],
                color: color_trans,
                uv: [0., 0.],
            },
            Vertex {
                pos: [x - side * 1., y_off + baseline + handle_descent + 5.],
                color,
                uv: [0., 0.],
            },
            Vertex {
                pos: [x + side * 4., y_off + baseline + handle_descent + 5.],
                color,
                uv: [0., 0.],
            },
        ];
        let indices = vec![0, 2, 1, 1, 2, 3];
        mesh.append(verts, indices);

        // The arrow itself.
        // Go anti-clockwise
        let verts = vec![
            Vertex { pos: [x, y], color, uv: [0., 0.] },
            Vertex { pos: [x, y + 50.], color, uv: [0., 0.] },
            Vertex { pos: [x + side * 30., y + 50.], color, uv: [0., 0.] },
            Vertex { pos: [x + side * 50., y + 25.], color, uv: [0., 0.] },
        ];
        let indices = vec![0, 1, 2, 0, 2, 3];
        mesh.append(verts, indices);
    }
    */

    async fn change_focus(self: Arc<Self>) {
        if !self.is_active.get() {
            return
        }
        t!("Focus changed");

        // Cursor visibility will change so just redraw everything lol
        self.redraw().await;
    }

    async fn insert_char(&self, key: char) {
        t!("insert_char({key})");
        let mut tmp = [0; 4];
        let key_str = key.encode_utf8(&mut tmp);

        let mut editor = self.editor.lock().await;
        let mut drv = editor.driver().await.unwrap();
        drv.insert_or_replace_selection(&key_str);
    }

    async fn handle_shortcut(
        &self,
        key: char,
        mods: &KeyMods,
        atom: &mut PropertyAtomicGuard,
    ) -> bool {
        t!("handle_shortcut({:?}, {:?})", key, mods);

        #[cfg(not(target_os = "macos"))]
        let action_mod = mods.ctrl;

        #[cfg(target_os = "macos")]
        let action_mod = mods.logo;

        match key {
            'a' => {
                if action_mod {
                    {
                        //let mut editor = self.editor.lock();
                        //let mut drv = editor.driver();
                        //drv.select_all();
                    }

                    self.redraw().await;
                    return true
                }
            }
            'c' => {
                if action_mod {
                    return true
                }
            }
            'v' => {
                if action_mod {
                    //if let Some(text) = miniquad::window::clipboard_get() {
                    //    let mut editor = self.editor.lock();
                    //    let mut drv = editor.driver();
                    //    drv.insert_or_replace_selection(&text);
                    //}
                    return true
                }
            }
            _ => {}
        }
        false
    }

    async fn handle_key(
        &self,
        key: &KeyCode,
        mods: &KeyMods,
        atom: &mut PropertyAtomicGuard,
    ) -> bool {
        t!("handle_key({:?}, {:?})", key, mods);
        match key {
            KeyCode::Left => {
                //if !self.adjust_cursor(&mods, |editable| editable.move_cursor(-1), atom) {
                //    return false
                //}
                self.pause_blinking();
                //self.apply_cursor_scrolling();
                self.redraw().await;
                return true
            }
            KeyCode::Right => {
                //if !self.adjust_cursor(&mods, |editable| editable.move_cursor(1), atom) {
                //    return false
                //}
                self.pause_blinking();
                //self.apply_cursor_scrolling();
                self.redraw().await;
                return true
            }
            KeyCode::Kp0 => {
                self.insert_char('0').await;
                return true
            }
            KeyCode::Kp1 => {
                self.insert_char('1').await;
                return true
            }
            KeyCode::Kp2 => {
                self.insert_char('2').await;
                return true
            }
            KeyCode::Kp3 => {
                self.insert_char('3').await;
                return true
            }
            KeyCode::Kp4 => {
                self.insert_char('4').await;
                return true
            }
            KeyCode::Kp5 => {
                self.insert_char('5').await;
                return true
            }
            KeyCode::Kp6 => {
                self.insert_char('6').await;
                return true
            }
            KeyCode::Kp7 => {
                self.insert_char('7').await;
                return true
            }
            KeyCode::Kp8 => {
                self.insert_char('8').await;
                return true
            }
            KeyCode::Kp9 => {
                self.insert_char('9').await;
                return true
            }
            KeyCode::KpDecimal => {
                self.insert_char('.').await;
                return true
            }
            KeyCode::Enter | KeyCode::KpEnter => {
                if mods.shift {
                    // Does nothing for now. Later will enable multiline.
                }
            }
            KeyCode::Delete => {
                //self.delete(0, 1, atom);
                //self.clamp_scroll(&mut self.text_wrap.lock(), atom);
                self.pause_blinking();
                self.redraw().await;
                return true
            }
            KeyCode::Backspace => {
                //self.delete(1, 0, atom);
                //self.clamp_scroll(&mut self.text_wrap.lock(), atom);
                t!("KeyCode::Backspace");
                {
                    //let mut editor = self.editor.lock();
                    //t!("  editor (before): {editor:?}");
                    //let mut drv = editor.driver();
                    //drv.backdelete();
                    //t!("  editor (after): {editor:?}");
                }
                self.pause_blinking();
                self.redraw().await;
                return true
            }
            KeyCode::Home => {
                //self.adjust_cursor(&mods, |editable| editable.move_start(), atom);
                self.pause_blinking();
                //self.apply_cursor_scrolling();
                self.redraw().await;
                return true
            }
            KeyCode::End => {
                //self.adjust_cursor(&mods, |editable| editable.move_end(), atom);
                self.pause_blinking();
                //self.apply_cursor_scrolling();
                self.redraw().await;
                return true
            }
            _ => {}
        }
        false
    }

    /*
    fn delete(&self, before: usize, after: usize, atom: &mut PropertyAtomicGuard) {
        let mut text_wrap = &mut self.text_wrap.lock();
        if text_wrap.select.is_empty() {
            text_wrap.editable.delete(before, after);
            text_wrap.clear_cache();
        } else {
            text_wrap.delete_selected();
            self.update_select_text(&mut text_wrap, atom);
        }

        self.is_phone_select.store(false, Ordering::Relaxed);
        // Reshow cursor (if hidden)
        self.hide_cursor.store(false, Ordering::Relaxed);

        let text = text_wrap.editable.get_text();
        self.text.set(atom, text);
    }
    */

    /// This will select the entire word rather than move the cursor to that location
    fn start_touch_select(&self, touch_pos: Point, atom: &mut PropertyAtomicGuard) {
        /*
        let mut text_wrap = &mut self.text_wrap.lock();
        text_wrap.clear_cache();
        text_wrap.editable.end_compose();

        let width = self.wrap_width();
        let wrapped_lines = text_wrap.wrap(width);
        let pos = wrapped_lines.point_to_pos(touch_pos);

        let (word_start, word_end) = text_wrap.get_word_boundary(pos);

        // begin selection
        let select = &mut text_wrap.select;
        select.clear();
        if word_start != word_end {
            select.push(Selection::new(word_start, word_end));

            self.is_phone_select.store(true, Ordering::Relaxed);
            // redraw() will now hide the cursor
            self.hide_cursor.store(true, Ordering::Relaxed);
        }

        d!("Selected {select:?} from {touch_pos:?}");
        self.update_select_text(&mut text_wrap, atom);
        */
    }

    /*
    /// Call this whenever the selection changes to update the external property
    fn update_select_text(&self, text_wrap: &mut TextWrap, atom: &mut PropertyAtomicGuard) {
        let select = &text_wrap.select;
        let Some(select) = select.first().cloned() else {
            self.select_text.clone().set_null(atom, Role::Internal, 0).unwrap();
            return
        };

        let start = std::cmp::min(select.start, select.end);
        let end = std::cmp::max(select.start, select.end);

        let rendered = text_wrap.get_render();
        let glyphs = &rendered.glyphs[start..end];
        let text = text::glyph_str(glyphs);
        self.select_text.clone().set_str(atom, Role::Internal, 0, text).unwrap();
    }

    /// Call this whenever the cursor pos changes to update the external property
    fn update_cursor_pos(&self, text_wrap: &mut TextWrap, atom: &mut PropertyAtomicGuard) {
        let cursor_off = text_wrap.editable.get_text_before().len() as u32;
        self.cursor_pos.set(atom, cursor_off);
    }
    */

    async fn handle_touch_start(&self, mut touch_pos: Point) -> bool {
        t!("handle_touch_start({touch_pos:?})");
        let mut touch_info = self.touch_info.lock();

        if self.try_handle_drag(&mut touch_info, touch_pos) {
            return true
        }

        let rect = self.rect.get();
        if !rect.contains(touch_pos) {
            t!("rect!cont rect={rect:?}, touch_pos={touch_pos:?}");
            return false
        }

        touch_info.start(touch_pos);
        true
    }
    fn try_handle_drag(&self, touch_info: &mut TouchInfo, mut touch_pos: Point) -> bool {
        /*
                // Is the handle visible? Use y within rect before adding the scroll.
                let relative_y = touch_pos.y - self.rect.get().y;
                if relative_y < 0. {
                    return false
                }

                self.abs_to_local(&mut touch_pos);

                let linespacing = self.linespacing.get();
                let baseline = self.baseline.get();
                let select_descent = self.select_descent.get();
                let scroll = self.scroll.get();

                let mut text_wrap = self.text_wrap.lock();
                let width = self.wrap_width();
                let wrapped_lines = text_wrap.wrap(width);
                let selections = &text_wrap.select;

                if self.is_phone_select.load(Ordering::Relaxed) && selections.len() == 1 {
                    let select = selections.first().unwrap();

                    let handle_off_y = baseline + self.handle_descent.get();

                    // Get left handle centerpoint
                    let (glyph_rect, line_idx) = wrapped_lines.get_glyph_info(select.start);
                    let mut p1 = glyph_rect.pos();
                    // We always want the handles to be aligned so ignore the glyph's y pos
                    p1.y = line_idx as f32 * linespacing + handle_off_y;

                    // Get right handle centerpoint
                    let (glyph_rect, line_idx) = wrapped_lines.get_glyph_info(select.end);
                    let mut p2 = glyph_rect.top_right();
                    p2.y = line_idx as f32 * linespacing + handle_off_y;

                    // Are we within range of either one?
                    t!("handle center points = ({p1:?}, {p2:?})");

                    const TOUCH_RADIUS_SQ: f32 = 10_000.;

                    if p1.dist_sq(&touch_pos) <= TOUCH_RADIUS_SQ {
                        d!("start touch: DragSelectHandle state [side=-1]");
                        // Set touch_state status to enable begin dragging them
                        touch_info.state = TouchStateAction::DragSelectHandle { side: -1 };
                        return true;
                    }
                    if p2.dist_sq(&touch_pos) <= TOUCH_RADIUS_SQ {
                        d!("start touch: DragSelectHandle state [side=1]");
                        // Set touch_state status to enable begin dragging them
                        touch_info.state = TouchStateAction::DragSelectHandle { side: 1 };
                        return true;
                    }
                }

        */
        false
    }

    async fn handle_touch_move(&self, mut touch_pos: Point) -> bool {
        t!("handle_touch_move({touch_pos:?})");
        let atom = &mut PropertyAtomicGuard::new();
        // We must update with non relative touch_pos bcos when doing vertical scrolling
        // we will modify the scroll, which is used by abs_to_local(), which is used
        // to then calculate the max scroll. So it ends up jumping around.
        // We use the abs touch_pos without scroll adjust applied for vert scrolling.
        let touch_state = {
            let mut touch_info = self.touch_info.lock();
            touch_info.update(&touch_pos);
            touch_info.state.clone()
        };
        match &touch_state {
            TouchStateAction::Inactive => return false,
            TouchStateAction::StartSelect => {
                if self.text.get().is_empty() {
                    let node = self.node.upgrade().unwrap();
                    node.trigger("paste_request", vec![]).await.unwrap();
                } else {
                    self.abs_to_local(&mut touch_pos);
                    self.start_touch_select(touch_pos, atom);
                    self.redraw().await;
                }
                d!("touch state: StartSelect -> Select");
                self.touch_info.lock().state = TouchStateAction::Select;
            }
            TouchStateAction::DragSelectHandle { side } => {
                self.abs_to_local(&mut touch_pos);
                {
                    /*
                    let linespacing = self.linespacing.get();
                    let baseline = self.baseline.get();
                    let handle_descent = self.handle_descent.get();

                    let mut text_wrap = self.text_wrap.lock();
                    let width = self.wrap_width();
                    let wrapped_lines = text_wrap.wrap(width);
                    let rendered = text_wrap.get_render().clone();
                    let selections = &mut text_wrap.select;

                    assert!(*side == -1 || *side == 1);
                    assert!(self.is_phone_select.load(Ordering::Relaxed));
                    assert_eq!(selections.len(), 1);
                    let select = selections.first_mut().unwrap();

                    let mut point = touch_pos;

                    // Only allow selecting text that is visible in the box
                    // We do our calcs relative to (0, 0) so bhs = rect_h
                    point.y -= handle_descent + 25.;
                    let bhs = wrapped_lines.height();
                    point.y = min_f32(point.y, bhs);

                    let mut pos = wrapped_lines.point_to_pos(point);

                    if *side == -1 {
                        let select_other_pos = &mut select.end;
                        if pos >= *select_other_pos {
                            pos = *select_other_pos - 1;
                        }
                        select.start = pos;
                    } else {
                        let select_other_pos = &mut select.start;
                        if pos <= *select_other_pos {
                            pos = *select_other_pos + 1;
                        }
                        select.end = pos;
                    }

                    self.update_select_text(&mut text_wrap, atom);
                    */
                }
                self.redraw().await;
            }
            TouchStateAction::ScrollVert { start_pos, scroll_start } => {
                /*
                let max_scroll = {
                    let mut text_wrap = self.text_wrap.lock();
                    self.max_scroll(&mut text_wrap)
                };

                let y_dist = start_pos.y - touch_pos.y;
                let mut scroll = scroll_start + y_dist;
                scroll = scroll.clamp(0., max_scroll);
                if (self.scroll.get() - scroll).abs() < VERT_SCROLL_UPDATE_INC {
                    return true
                }
                self.scroll.set(atom, scroll);
                self.redraw().await;
                */
            }
            TouchStateAction::SetCursorPos => {
                // TBH I can't even see the cursor under my thumb so I'll just
                // comment this for now.
                self.abs_to_local(&mut touch_pos);
                self.touch_set_cursor_pos(touch_pos, atom).await
            }
            _ => {}
        }
        true
    }
    async fn handle_touch_end(&self, mut touch_pos: Point) -> bool {
        t!("handle_touch_end({touch_pos:?})");
        let atom = &mut PropertyAtomicGuard::new();
        self.abs_to_local(&mut touch_pos);

        let state = self.touch_info.lock().stop();
        match state {
            TouchStateAction::Inactive => return false,
            TouchStateAction::Started { pos: _, instant: _ } | TouchStateAction::SetCursorPos => {
                self.touch_set_cursor_pos(touch_pos, atom).await;
                self.redraw().await;
            }
            _ => {}
        }

        let node = self.node.upgrade().unwrap();
        node.trigger("keyboard_request", vec![]).await.unwrap();

        true
    }

    async fn touch_set_cursor_pos(&self, mut touch_pos: Point, atom: &mut PropertyAtomicGuard) {
        t!("touch_set_cursor_pos({touch_pos:?})");

        let mut editor = self.editor.lock().await;
        editor.move_to_pos(touch_pos);
        editor.refresh().await;

        //let layout = editor.layout();
        //let cursor = parley::Cursor::from_point(layout, touch_pos.x, touch_pos.y);
        //drop(editor);

        //let cursor_idx = cursor.index();
        //let buffer = crate::android::get_raw_text().unwrap();
        //let cursor_clsr = byte_to_char16_index(&buffer, cursor_idx).unwrap();
        //crate::android::set_cursor_pos(cursor_clsr as i32);
        //t!("  {cursor_idx} => {cursor_clsr}");

        /*
        // This is my own type that contains the editor and contexts.
        let mut editor = self.editor.lock();
        t!("  editor (before): {editor:?}");

        //if editor.editor.is_composing() {
        //    let mut drv = editor.driver();
        //    // commit the existing compose text
        //    drv.finish_compose();
        //    t!("  editor (finish_compose): {editor:?}");
        //}

        let mut drv = editor.driver();
        drv.move_to_point(touch_pos.x, touch_pos.y);

        let focus = editor.editor.raw_selection().focus();
        let layout = editor.layout();
        let idx = focus.index();

        // Android uses chars, so we have to convert the byte index to a char index
        let buffer = editor.editor.raw_text();
        let cursor_idx = byte_to_char16_index(buffer, idx).unwrap();
        t!("  idx = {idx} => cursor_idx = {cursor_idx}");
        crate::android::set_cursor_pos(cursor_idx as i32);
        t!("  editor (after): {editor:?}");
        t!("  editable: {}", crate::android::get_debug_editable());
        */

        // OLD
        /*
        let last_suggest_text = std::mem::take(&mut editor.last_suggest_text);
        let is_composing = editor.editor.is_composing();
        if is_composing {
            let mut drv = editor.driver();
            t!("clear and commit compose: {last_suggest_text}");
            // commit the existing compose text
            drv.clear_compose();
            drv.insert_or_replace_selection(&last_suggest_text);
            t!("  text: '{}'", editor.editor.raw_text());
        }

        let mut drv = editor.driver();
        // We move the cursor here first so we can get the offset within the composing word
        drv.move_to_point(touch_pos.x, touch_pos.y);
        drop(drv);

        let focus = editor.editor.raw_selection().focus();
        let layout = editor.layout();
        let start = focus.previous_logical_word(layout).index();
        let curr_idx = focus.index();
        assert!(curr_idx >= start);
        let curr_off = curr_idx - start;

        let txt = editor.editor.raw_text();
        t!("{}[{}|{}", &txt[..start], &txt[start..curr_idx], &txt[curr_idx..]);
        t!("start={start}, curr_idx={curr_idx}");

        let mut drv = editor.driver();
        // Move the word under the cursor to the composer
        drv.select_word_at_point(touch_pos.x, touch_pos.y);
        let current_word = drv.editor.selected_text().unwrap().to_string();
        drv.delete_selection();
        t!("selected word: '{}|{}'", &current_word[..curr_off], &current_word[curr_off..]);
        drv.set_compose(&current_word, Some((curr_off, curr_off)));
        drop(drv);

        let comp = editor.editor.compose().clone().unwrap();
        let txt = editor.editor.raw_text();
        let (pre, rest) = txt.split_at(comp.start);
        let (cmp, post) = rest.split_at(comp.end - comp.start);
        t!("after compose: '{}[{}]{}'", pre, cmp, post);

        // This will then trigger handlers that call drv.set_compose(...)
        crate::android::set_compose(&current_word, curr_off as i32);
        //crate::android::set_compose(&"", 0);
        editor.last_suggest_text = current_word;
        */

        /*
        let width = self.wrap_width();
        {
            let mut text_wrap = self.text_wrap.lock();
            let cursor_pos = text_wrap.set_cursor_with_point(touch_pos, width);
            self.update_cursor_pos(&mut text_wrap, atom);

            let select = &mut text_wrap.select;
            let select_is_empty = select.is_empty();
            select.clear();
            if !select_is_empty {
                self.update_select_text(&mut text_wrap, atom);
            }
        }

        self.is_phone_select.store(false, Ordering::Relaxed);
        // Reshow cursor (if hidden)
        self.pause_blinking();
        self.hide_cursor.store(false, Ordering::Relaxed);

        self.redraw().await;
        */
    }

    /// Whenever the cursor property is modified this MUST be called
    /// to recalculate the scroll x property.
    fn apply_cursor_scrolling(&self, atom: &mut PropertyAtomicGuard) {
        let rect = self.rect.get();

        let cursor_pos = self.cursor_pos.get() as usize;
        let cursor_width = self.cursor_width.get();
        let mut scroll = self.scroll.get();

        let cursor_x = {
            let font_size = self.font_size.get();
            let window_scale = self.window_scale.get();
            let baseline = self.baseline.get();
            let glyphs = self.glyphs.lock().clone();

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
        let cursor_rhs = cursor_lhs + cursor_width;

        // RHS is outside
        if cursor_rhs > rect.w {
            // We want a scroll so RHS = w
            // cursor_x - scroll + CURSOR_WIDTH = rect.w
            scroll = cursor_x + cursor_width - rect.w;
        // LHS is negative
        } else if cursor_lhs < 0. {
            // We want scroll so LHS = 0
            // cursor_x - scroll = 0
            scroll = cursor_x;
        }

        self.scroll.set(atom, scroll);
    }

    /*
    fn max_scroll(&self, text_wrap: &mut TextWrap) -> f32 {
        let width = self.wrap_width();
        let mut inner_height = text_wrap.wrap(width).height();
        // Top padding
        inner_height += self.padding.get_f32(0).unwrap();
        // Bottom padding
        inner_height += self.padding.get_f32(1).unwrap();

        let max_height = self.max_height.get();
        if inner_height < max_height {
            return 0.
        }

        let max_scroll = inner_height - max_height;
        max_scroll.clamp(0., f32::MAX)
    }
    */

    /// When we resize the screen, the rect changes so we may need to alter the scroll.
    /// Or if we delete text.
    /*
    fn clamp_scroll(&self, text_wrap: &mut TextWrap, atom: &mut PropertyAtomicGuard) {
        let max_scroll = self.max_scroll(text_wrap);
        let mut scroll = self.scroll.get();
        if scroll > max_scroll {
            self.scroll.set(atom, max_scroll);
        }
    }
    */

    fn pause_blinking(&self) {
        self.blink_is_paused.store(true, Ordering::Relaxed);
        self.cursor_is_visible.store(true, Ordering::Relaxed);
    }

    async fn redraw(&self) {
        let atom = &mut PropertyAtomicGuard::new();
        let trace_id = rand::random();
        let timest = unixtime();
        t!("redraw()");
        let Some(draw_update) = self.make_draw_calls(trace_id, atom).await else {
            error!(target: "ui::chatedit", "Text failed to draw");
            return;
        };
        self.render_api.replace_draw_calls(timest, draw_update.draw_calls);
    }

    async fn redraw_cursor(&self) {
        let timest = unixtime();
        let cursor_instrs = self.get_cursor_instrs().await;
        let draw_calls = vec![(
            self.cursor_dc_key,
            GfxDrawCall { instrs: cursor_instrs, dcs: vec![], z_index: self.z_index.get() },
        )];
        self.render_api.replace_draw_calls(timest, draw_calls);
    }

    async fn get_cursor_instrs(&self) -> Vec<GfxDrawInstruction> {
        if !self.is_focused.get() ||
            !self.cursor_is_visible.load(Ordering::Relaxed) ||
            self.hide_cursor.load(Ordering::Relaxed)
        {
            return vec![]
        }

        let lineheight = self.lineheight.get();

        let mut cursor_instrs = vec![];

        let Some(cursor_pos) = self.editor.lock().await.get_cursor_pos() else { return vec![] };
        //let cursor_pos = self.get_cursor_pos();

        // There is some mess here since ApplyView is in abs screen coords but should be
        // relative, and also work together with Move. We will fix this later in gfx subsys.
        let mut view_rect = self.rect.get();
        view_rect.h = self.max_height.get();

        cursor_instrs.push(GfxDrawInstruction::ApplyView(view_rect));
        cursor_instrs.push(GfxDrawInstruction::Move(cursor_pos));

        let cursor_mesh = {
            let mut cursor_mesh = self.cursor_mesh.lock();
            if cursor_mesh.is_none() {
                *cursor_mesh = Some(self.regen_cursor_mesh());
            }
            cursor_mesh.clone().unwrap()
        };

        cursor_instrs.push(GfxDrawInstruction::Draw(cursor_mesh));

        cursor_instrs
    }

    async fn regen_mesh(&self) -> Vec<GfxDrawInstruction> {
        let font_size = self.font_size.get();
        let text_color = self.text_color.get();
        let window_scale = self.window_scale.get();
        let lineheight = self.lineheight.get();

        let editor = self.editor.lock().await;
        let layout = editor.layout();
        text2::render_layout(layout, &self.render_api)
    }

    async fn make_draw_calls(
        &self,
        trace_id: u32,
        atom: &mut PropertyAtomicGuard,
    ) -> Option<DrawUpdate> {
        let parent_rect = self.parent_rect.lock().clone().unwrap();
        self.rect.eval_with(
            vec![2],
            vec![("parent_w".to_string(), parent_rect.w), ("parent_h".to_string(), parent_rect.h)],
        );
        self.rect.prop().set_f32(atom, Role::Internal, 3, 2000.);

        //let text_mesh = self.regen_text_mesh(trace_id, atom);
        let cursor_instrs = self.get_cursor_instrs().await;

        let rect = self.rect.get();

        let mut instrs = vec![GfxDrawInstruction::Move(rect.pos())];
        instrs.append(&mut self.regen_mesh().await);

        Some(DrawUpdate {
            key: self.main_dc_key,
            draw_calls: vec![
                (
                    self.main_dc_key,
                    GfxDrawCall {
                        instrs: vec![],
                        dcs: vec![self.text_dc_key, self.cursor_dc_key],
                        z_index: self.z_index.get(),
                    },
                ),
                (
                    self.text_dc_key,
                    GfxDrawCall { instrs, dcs: vec![], z_index: self.z_index.get() },
                ),
                (
                    self.cursor_dc_key,
                    GfxDrawCall { instrs: cursor_instrs, dcs: vec![], z_index: self.z_index.get() },
                ),
            ],
        })
    }

    async fn process_insert_text_method(me: &Weak<Self>, sub: &MethodCallSub) -> bool {
        let Ok(method_call) = sub.receive().await else {
            debug!(target: "ui::chatedit", "Event relayer closed");
            return false
        };

        t!("method called: insert_line({method_call:?})");
        assert!(method_call.send_res.is_none());

        fn decode_data(data: &[u8]) -> std::io::Result<String> {
            let mut cur = Cursor::new(&data);
            let text = String::decode(&mut cur)?;
            Ok(text)
        }

        let Ok(text) = decode_data(&method_call.data) else {
            error!(target: "ui::chatedit", "insert_text() method invalid arg data");
            return true
        };

        let Some(self_) = me.upgrade() else {
            // Should not happen
            panic!("self destroyed before insert_text_method_task was stopped!");
        };

        let atom = &mut PropertyAtomicGuard::new();
        //self_.insert_text(&text, atom).await;
        true
    }

    async fn handle_android_event(&self, ev: AndroidSuggestEvent) {
        t!("handle_android_event({ev:?})");
        if !self.is_active.get() {
            return
        }

        let mut editor = self.editor.lock().await;
        match ev {
            AndroidSuggestEvent::Init => {
                editor.init();
                return
            }
            AndroidSuggestEvent::CreateInputConnect => editor.setup(),
            _ => {} //editor.update(text.clone(), select_start, select_end, compose_start, compose_end),
        }

        editor.refresh().await;
        drop(editor);

        self.redraw().await;
    }
}

impl Drop for ChatEdit {
    fn drop(&mut self) {
        self.render_api
            .replace_draw_calls(unixtime(), vec![(self.text_dc_key, Default::default())]);
    }
}

#[async_trait]
impl UIObject for ChatEdit {
    fn priority(&self) -> u32 {
        self.priority.get()
    }

    async fn start(self: Arc<Self>, ex: ExecutorPtr) {
        let me = Arc::downgrade(&self);

        let node_ref = &self.node.upgrade().unwrap();

        let method_sub = node_ref.subscribe_method_call("insert_text").unwrap();
        let me2 = me.clone();
        let insert_text_task =
            ex.spawn(
                async move { while Self::process_insert_text_method(&me2, &method_sub).await {} },
            );

        let mut on_modify = OnModify::new(ex.clone(), self.node.clone(), me.clone());
        on_modify.when_change(self.is_focused.prop(), Self::change_focus);

        // When text has been changed.
        // Cursor and selection might be invalidated.
        async fn reset(self_: Arc<ChatEdit>) {
            let atom = &mut PropertyAtomicGuard::new();
            self_.cursor_pos.set(atom, 0);
            //self_.select_text.set_null(Role::Internal, 0).unwrap();
            self_.scroll.set(atom, 0.);
            self_.redraw().await;
        }
        async fn redraw(self_: Arc<ChatEdit>) {
            self_.redraw().await;
        }
        async fn set_text(self_: Arc<ChatEdit>) {
            {
                let text = self_.text.get();

                //let mut text_wrap = self_.text_wrap.lock();
                //text_wrap.editable.end_compose();
                //text_wrap.editable.set_text(text, String::new());
                //text_wrap.clear_cache();

                //let select = &mut text_wrap.select;
                //select.clear();
            }

            self_.redraw().await;
        }

        on_modify.when_change(self.rect.prop(), redraw);
        on_modify.when_change(self.baseline.prop(), redraw);
        on_modify.when_change(self.linespacing.prop(), redraw);
        on_modify.when_change(self.lineheight.prop(), redraw);
        on_modify.when_change(self.select_ascent.prop(), redraw);
        on_modify.when_change(self.select_descent.prop(), redraw);
        on_modify.when_change(self.handle_descent.prop(), redraw);
        on_modify.when_change(self.padding.clone(), redraw);
        on_modify.when_change(self.text.prop(), set_text);
        // The commented properties are modified on input events
        // So then redraw() will get repeatedly triggered when these properties
        // are changed. We should find a solution. For now the hooks are disabled.
        //on_modify.when_change(scroll.prop(), redraw);
        //on_modify.when_change(cursor_pos.prop(), redraw);
        on_modify.when_change(self.font_size.prop(), redraw);
        // We must also reshape text
        on_modify.when_change(self.text.prop(), reset);
        on_modify.when_change(self.text_color.prop(), redraw);
        on_modify.when_change(self.hi_bg_color.prop(), redraw);
        //on_modify.when_change(selected.clone(), redraw);
        on_modify.when_change(self.z_index.prop(), redraw);
        on_modify.when_change(self.debug.prop(), redraw);

        async fn regen_cursor(self_: Arc<ChatEdit>) {
            let mesh = std::mem::take(&mut *self_.cursor_mesh.lock());
        }
        on_modify.when_change(self.cursor_color.prop(), regen_cursor);
        on_modify.when_change(self.cursor_ascent.prop(), regen_cursor);
        on_modify.when_change(self.cursor_descent.prop(), regen_cursor);
        on_modify.when_change(self.cursor_width.prop(), regen_cursor);

        let me2 = me.clone();
        let cursor_blink_time = self.cursor_blink_time.clone();
        let cursor_idle_time = self.cursor_idle_time.clone();
        let blinking_cursor_task = ex.spawn(async move {
            loop {
                msleep(cursor_blink_time.get() as u64).await;

                let self_ = me2.upgrade().unwrap();

                if self_.blink_is_paused.swap(false, Ordering::Relaxed) {
                    msleep(cursor_idle_time.get() as u64).await;
                    continue
                }

                if !self_.rect.has_cached() {
                    continue
                }

                // Invert the bool
                self_.cursor_is_visible.fetch_not(Ordering::Relaxed);
                self_.redraw_cursor().await;
            }
        });

        let mut tasks = vec![insert_text_task, blinking_cursor_task];
        tasks.append(&mut on_modify.tasks);

        #[cfg(target_os = "android")]
        {
            let (sender, recvr) = async_channel::unbounded();
            let composer_id = crate::android::create_composer(sender);
            self.editor.lock().await.composer_id = composer_id;
            t!("Created composer [{composer_id}]");

            let me2 = me.clone();
            let autosuggest_task = ex.spawn(async move {
                loop {
                    let Ok(ev) = recvr.recv().await else {
                        t!("Event relayer closed");
                        break
                    };

                    let Some(self_) = me2.upgrade() else {
                        // Should not happen
                        panic!("self destroyed before autosuggest_task was stopped!");
                    };

                    self_.handle_android_event(ev).await;
                }
            });
            tasks.push(autosuggest_task);
        }

        self.tasks.set(tasks);
    }

    async fn draw(
        &self,
        parent_rect: Rectangle,
        trace_id: u32,
        atom: &mut PropertyAtomicGuard,
    ) -> Option<DrawUpdate> {
        t!("ChatEdit::draw({:?}, {trace_id})", self.node.upgrade().unwrap());
        *self.parent_rect.lock() = Some(parent_rect);

        self.make_draw_calls(trace_id, atom).await
    }

    async fn handle_char(&self, key: char, mods: KeyMods, repeat: bool) -> bool {
        t!("handle_char({key}, {mods:?}, {repeat})");
        // First filter for only single digit keys
        if DISALLOWED_CHARS.contains(&key) {
            return false
        }

        if !self.is_focused.get() {
            return false
        }

        // Must be updated before checking the mods. You can press ctrl+a, then release ctrl
        // before a is released. Then the repeater never gets reset, and uses any old value
        // it has from before for a.
        let actions = {
            let mut repeater = self.key_repeat.lock();
            repeater.key_down(PressedKey::Char(key), repeat)
        };

        let atom = &mut PropertyAtomicGuard::new();

        if mods.ctrl || mods.alt || mods.logo {
            if repeat {
                return false
            }
            return self.handle_shortcut(key, &mods, atom).await
        }

        let atom = &mut PropertyAtomicGuard::new();
        t!("Key {:?} has {} actions", key, actions);
        for _ in 0..actions {
            self.insert_char(key).await;
        }
        self.redraw().await;
        true
    }

    async fn handle_key_down(&self, key: KeyCode, mods: KeyMods, repeat: bool) -> bool {
        t!("handle_key_down({key:?}, {mods:?}, {repeat})");
        // First filter for only single digit keys
        // Avoid processing events handled by insert_char()
        if !ALLOWED_KEYCODES.contains(&key) {
            return false
        }

        if !self.is_focused.get() {
            return false
        }

        let actions = {
            let mut repeater = self.key_repeat.lock();
            repeater.key_down(PressedKey::Key(key), repeat)
        };

        // Suppress noisy message
        if actions > 0 {
            t!("Key {:?} has {} actions", key, actions);
        }

        let atom = &mut PropertyAtomicGuard::new();

        let mut is_handled = false;
        for _ in 0..actions {
            if self.handle_key(&key, &mods, atom).await {
                is_handled = true;
            }
        }
        is_handled
    }

    async fn handle_mouse_btn_down(&self, btn: MouseButton, mut mouse_pos: Point) -> bool {
        if !self.is_active.get() {
            return false
        }

        let rect = self.rect.get();

        if btn != MouseButton::Left {
            if btn == MouseButton::Right && rect.contains(mouse_pos) {
                if self.text.get().is_empty() {
                    let node = self.node.upgrade().unwrap();
                    node.trigger("paste_request", vec![]).await.unwrap();
                }
                return true
            }
            return false
        }

        if !rect.contains(mouse_pos) {
            return false
        }

        let atom = &mut PropertyAtomicGuard::new();

        // clicking inside box will:
        // 1. make it active
        // 2. begin selection
        if self.is_focused.get() {
            d!("ChatEdit clicked");
        } else {
            d!("ChatEdit focused");
            self.is_focused.set(atom, true);
        }

        // Move mouse pos within this widget
        self.abs_to_local(&mut mouse_pos);

        let width = self.wrap_width();

        /*
        {
            let mut text_wrap = self.text_wrap.lock();
            let cursor_pos = text_wrap.set_cursor_with_point(mouse_pos, width);
            self.update_cursor_pos(&mut text_wrap, atom);
            d!("Mouse move cursor pos to {cursor_pos}");

            // begin selection
            let select = &mut text_wrap.select;
            let select_is_empty = select.is_empty();
            select.clear();
            if !select_is_empty {
                self.update_select_text(&mut text_wrap, atom);
            }

            self.mouse_btn_held.store(true, Ordering::Relaxed);
        }
        */

        self.pause_blinking();
        self.redraw().await;
        true
    }

    async fn handle_mouse_btn_up(&self, btn: MouseButton, mouse_pos: Point) -> bool {
        if !self.is_active.get() {
            return false
        }

        // releasing mouse button will end selection
        self.mouse_btn_held.store(false, Ordering::Relaxed);
        false
    }

    async fn handle_mouse_move(&self, mut mouse_pos: Point) -> bool {
        if !self.is_active.get() {
            return false
        }

        let rect = self.rect.get();
        self.is_mouse_hover.store(rect.contains(mouse_pos), Ordering::Relaxed);

        if !self.mouse_btn_held.load(Ordering::Relaxed) {
            return false;
        }

        let atom = &mut PropertyAtomicGuard::new();

        // if active and selection_active, then use x to modify the selection.
        // also implement scrolling when cursor is to the left or right
        // just scroll to the end
        // also set cursor_pos too

        // Move mouse pos within this widget
        self.abs_to_local(&mut mouse_pos);

        let width = self.wrap_width();

        /*
        {
            let mut text_wrap = self.text_wrap.lock();
            let cursor_pos = text_wrap.set_cursor_with_point(mouse_pos, width);
            self.update_cursor_pos(&mut text_wrap, atom);

            // modify current selection
            let select = &mut text_wrap.select;
            if select.is_empty() {
                select.push(Selection::new(cursor_pos, cursor_pos));
            }
            select.first_mut().unwrap().end = cursor_pos;
            self.update_select_text(&mut text_wrap, atom);
        }
        */

        self.pause_blinking();
        //self.apply_cursor_scrolling();
        self.redraw().await;
        true
    }

    async fn handle_mouse_wheel(&self, wheel_pos: Point) -> bool {
        if !self.is_mouse_hover.load(Ordering::Relaxed) {
            return false
        }

        let atom = &mut PropertyAtomicGuard::new();

        /*
        let max_scroll = {
            let mut text_wrap = self.text_wrap.lock();
            self.max_scroll(&mut text_wrap)
        };

        let mut scroll = self.scroll.get() - wheel_pos.y * self.scroll_speed.get();
        scroll = scroll.clamp(0., max_scroll);
        t!("handle_mouse_wheel({wheel_pos:?}) [scroll={scroll}]");
        self.scroll.set(atom, scroll);
        self.redraw().await;
        */

        true
    }

    async fn handle_touch(&self, phase: TouchPhase, id: u64, touch_pos: Point) -> bool {
        if !self.is_active.get() {
            return false
        }

        // Ignore multi-touch
        if id != 0 {
            return false
        }

        match phase {
            TouchPhase::Started => self.handle_touch_start(touch_pos).await,
            TouchPhase::Moved => self.handle_touch_move(touch_pos).await,
            TouchPhase::Ended => self.handle_touch_end(touch_pos).await,
            TouchPhase::Cancelled => false,
        }
    }
}
