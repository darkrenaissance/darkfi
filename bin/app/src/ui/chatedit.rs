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
        return None
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
                let travel_dist = pos.dist_sq(*start_pos);
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
    root_dc_key: u64,
    content_dc_key: u64,
    /// DC key for the text
    text_dc_key: u64,
    cursor_mesh: SyncMutex<Option<GfxDrawMesh>>,
    /// DC key for the cursor. Allows updating cursor independently.
    cursor_dc_key: u64,
    /// DC key for the selection.
    select_dc_key: u64,

    is_active: PropertyBool,
    is_focused: PropertyBool,
    min_height: PropertyFloat32,
    max_height: PropertyFloat32,
    content_height: PropertyFloat32,
    rect: PropertyRect,
    baseline: PropertyFloat32,
    lineheight: PropertyFloat32,
    scroll: PropertyFloat32,
    scroll_speed: PropertyFloat32,
    padding: PropertyPtr,
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
        let lineheight = PropertyFloat32::wrap(node_ref, Role::Internal, "lineheight", 0).unwrap();
        let scroll = PropertyFloat32::wrap(node_ref, Role::Internal, "scroll", 0).unwrap();
        let scroll_speed =
            PropertyFloat32::wrap(node_ref, Role::Internal, "scroll_speed", 0).unwrap();
        let padding = node_ref.get_property("padding").unwrap();
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
            root_dc_key: OsRng.gen(),
            content_dc_key: OsRng.gen(),
            text_dc_key: OsRng.gen(),
            cursor_mesh: SyncMutex::new(None),
            cursor_dc_key: OsRng.gen(),
            select_dc_key: OsRng.gen(),

            is_active,
            is_focused,
            min_height,
            max_height,
            content_height,
            rect,
            baseline,
            lineheight: lineheight.clone(),
            scroll: scroll.clone(),
            scroll_speed,
            padding,
            font_size: font_size.clone(),
            text: text.clone(),
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
                Editor::new(text, font_size, text_color, window_scale, lineheight).await,
            ),
        });

        Pimpl::ChatEdit(self_)
    }

    fn node(&self) -> SceneNodePtr {
        self.node.upgrade().unwrap()
    }

    fn padding_top(&self) -> f32 {
        self.padding.get_f32(0).unwrap()
    }
    fn padding_bottom(&self) -> f32 {
        self.padding.get_f32(1).unwrap()
    }

    fn abs_to_local(&self, point: &mut Point) {
        let rect = self.rect.get();
        *point -= rect.pos();
        *point -= self.inner_pos();
        point.y += self.scroll.get();
    }

    /// Inner position used for rendering
    fn inner_pos(&self) -> Point {
        let content_height = self.content_height.get();
        let rect_h = self.rect.get_height();
        let mut inner_pos = Point::zero();
        if content_height < rect_h {
            inner_pos.y = (rect_h - content_height) / 2.;
        }
        inner_pos
    }

    /// Maximum allowed scroll value
    fn max_scroll(&self) -> f32 {
        let content_height = self.content_height.get();
        let rect_h = self.rect.get_height();
        let mut max_scroll = (content_height - rect_h).max(0.);
        // Arbitrary little space at the bottom
        max_scroll += self.font_size.get() / 3.;
        max_scroll
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

    fn draw_phone_select_handle(&self, mesh: &mut MeshBuilder, pos: Point, side: f32) {
        let baseline = self.baseline.get();
        let select_ascent = self.select_ascent.get();
        let handle_descent = self.handle_descent.get();
        let color = self.text_hi_color.get();
        // Transparent for fade
        let mut color_trans = color.clone();
        color_trans[3] = 0.;

        let x = pos.x;
        let mut y = pos.y + baseline;

        // Vertical line downwards. We use this instead of draw_box() so we have a fade.
        let verts = vec![
            Vertex { pos: [x - side * 1., y - select_ascent], color: color_trans, uv: [0., 0.] },
            Vertex { pos: [x + side * 4., y - select_ascent], color: color_trans, uv: [0., 0.] },
            Vertex { pos: [x - side * 1., y + handle_descent + 5.], color, uv: [0., 0.] },
            Vertex { pos: [x + side * 4., y + handle_descent + 5.], color, uv: [0., 0.] },
        ];
        let indices = vec![0, 2, 1, 1, 2, 3];
        mesh.append(verts, indices);

        y += handle_descent;

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

        let mut txt_ctx = text2::TEXT_CTX.get().await;
        let mut editor = self.editor.lock().await;
        let mut drv = editor.driver(&mut txt_ctx).await.unwrap();
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

        let mut txt_ctx = text2::TEXT_CTX.get().await;
        let mut editor = self.editor.lock().await;
        let mut drv = editor.driver(&mut txt_ctx).await.unwrap();

        match key {
            'a' => {
                if action_mod {
                    drv.select_all();
                }
            }
            'c' => {
                if action_mod {
                    if let Some(text) = editor.selected_text() {
                        miniquad::window::clipboard_set(&text);
                    }
                }
            }
            'v' => {
                if action_mod {
                    if let Some(text) = miniquad::window::clipboard_get() {
                        drv.insert_or_replace_selection(&text);
                    }
                }
            }
            _ => return false,
        }

        drop(editor);
        drop(txt_ctx);

        self.redraw().await;
        true
    }

    async fn handle_key(
        &self,
        key: &KeyCode,
        mods: &KeyMods,
        atom: &mut PropertyAtomicGuard,
    ) -> bool {
        #[cfg(not(target_os = "macos"))]
        let action_mod = mods.ctrl;

        #[cfg(target_os = "macos")]
        let action_mod = mods.logo;

        t!("handle_key({:?}, {:?}) action_mod={action_mod}", key, mods);

        let mut txt_ctx = text2::TEXT_CTX.get().await;
        let mut editor = self.editor.lock().await;
        let mut drv = editor.driver(&mut txt_ctx).await.unwrap();

        match key {
            KeyCode::Left => {
                if action_mod {
                    if mods.shift {
                        drv.select_word_left();
                    } else {
                        drv.move_word_left();
                    }
                } else if mods.shift {
                    drv.select_left();
                } else {
                    drv.move_left();
                }
            }
            KeyCode::Right => {
                if action_mod {
                    if mods.shift {
                        drv.select_word_right();
                    } else {
                        drv.move_word_right();
                    }
                } else if mods.shift {
                    drv.select_right();
                } else {
                    drv.move_right();
                }
            }
            KeyCode::Up => {
                if mods.shift {
                    drv.select_up();
                } else {
                    drv.move_up();
                }
            }
            KeyCode::Down => {
                if mods.shift {
                    drv.select_down();
                } else {
                    drv.move_down();
                }
            }
            KeyCode::Enter | KeyCode::KpEnter => {
                if mods.shift {
                    drv.insert_or_replace_selection("\n");
                }
            }
            KeyCode::Delete => {
                if action_mod {
                    drv.delete_word();
                } else {
                    drv.delete();
                }
            }
            KeyCode::Backspace => {
                if action_mod {
                    drv.backdelete_word();
                } else {
                    drv.backdelete();
                }
            }
            KeyCode::Home => {
                if action_mod {
                    if mods.shift {
                        drv.select_to_text_start();
                    } else {
                        drv.move_to_text_start();
                    }
                } else if mods.shift {
                    drv.select_to_line_start();
                } else {
                    drv.move_to_line_start();
                }
            }
            KeyCode::End => {
                if action_mod {
                    if mods.shift {
                        drv.select_to_text_end();
                    } else {
                        drv.move_to_text_end();
                    }
                } else if mods.shift {
                    drv.select_to_line_end();
                } else {
                    drv.move_to_line_end();
                }
            }
            _ => return false,
        }

        drop(editor);
        drop(txt_ctx);

        self.pause_blinking();
        self.redraw().await;

        return true
    }

    /// This will select the entire word rather than move the cursor to that location
    async fn start_touch_select(&self, touch_pos: Point, atom: &mut PropertyAtomicGuard) {
        t!("start_touch_select({touch_pos:?})");

        let mut editor = self.editor.lock().await;
        editor.select_word_at_point(touch_pos);
        editor.refresh(atom).await;

        let seltext = editor.selected_text().unwrap();
        d!("Selected {seltext:?} from {touch_pos:?}");
        self.select_text.clone().set_str(atom, Role::Internal, 0, seltext).unwrap();

        drop(editor);

        // if start != end {
        self.is_phone_select.store(true, Ordering::Relaxed);
        self.hide_cursor.store(true, Ordering::Relaxed);
        // }
    }

    async fn handle_touch_start(&self, mut touch_pos: Point) -> bool {
        t!("handle_touch_start({touch_pos:?})");

        if self.try_handle_drag(touch_pos).await {
            return true
        }

        let rect = self.rect.get();
        if !rect.contains(touch_pos) {
            t!("rect!cont rect={rect:?}, touch_pos={touch_pos:?}");
            return false
        }

        let mut touch_info = self.touch_info.lock();
        touch_info.start(touch_pos);
        true
    }

    async fn get_select_handles(&self) -> Option<(Point, Point)> {
        let editor = self.editor.lock().await;
        let layout = editor.layout();

        let sel = editor.selection();
        if sel.is_collapsed() {
            assert!(!self.is_phone_select.load(Ordering::Relaxed));
            return None
        }
        assert!(self.is_phone_select.load(Ordering::Relaxed));

        let mut first_pos = None;
        let mut last_pos = Default::default();

        sel.geometry_with(layout, |rect: parley::Rect, _| {
            let rect = Rectangle::from(rect);
            if first_pos.is_none() {
                first_pos = Some(rect.bot_left());
            }
            last_pos = rect.corner();
        });

        let first_pos = first_pos.unwrap();
        Some((first_pos, last_pos))
    }

    async fn try_handle_drag(&self, mut touch_pos: Point) -> bool {
        let Some((mut first_pos, mut last_pos)) = self.get_select_handles().await else {
            return false
        };

        self.abs_to_local(&mut touch_pos);
        t!("localize touch_pos = {touch_pos:?}");

        let handle_off_y = self.handle_descent.get();
        first_pos.y += handle_off_y;
        last_pos.y += handle_off_y;

        // Are we within range of either one?
        t!("handle center points = ({first_pos:?}, {last_pos:?})");

        const TOUCH_RADIUS_SQ: f32 = 10_000.;

        let first_dist_sq = first_pos.dist_sq(touch_pos);
        let last_dist_sq = last_pos.dist_sq(touch_pos);

        let is_first = first_dist_sq <= TOUCH_RADIUS_SQ;
        let is_last = last_dist_sq <= TOUCH_RADIUS_SQ;

        let mut side = 0;

        if is_first && is_last {
            // Are we closer to the first or last?
            // Break the tie
            if first_dist_sq < last_dist_sq {
                side = -1;
            } else {
                side = 1;
            }
        } else if is_first {
            side = -1;
        } else if is_last {
            side = 1;
        }

        if side != 0 {
            d!("start touch: DragSelectHandle state [side={side}]");
            // Set touch_state status to enable begin dragging them
            let mut touch_info = self.touch_info.lock();
            touch_info.state = TouchStateAction::DragSelectHandle { side };
            return true
        }

        false
    }

    async fn handle_touch_move(&self, mut touch_pos: Point) -> bool {
        //t!("handle_touch_move({touch_pos:?})");
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
                    self.start_touch_select(touch_pos, atom).await;
                    self.redraw_select().await;
                }
                d!("touch state: StartSelect -> Select");
                self.touch_info.lock().state = TouchStateAction::Select;
            }
            TouchStateAction::DragSelectHandle { side } => {
                let handle_descent = self.handle_descent.get();
                self.abs_to_local(&mut touch_pos);

                let editor = self.editor.lock().await;
                let sel = editor.selection();

                assert!(*side == -1 || *side == 1);
                assert!(self.is_phone_select.load(Ordering::Relaxed));
                assert!(!sel.is_collapsed());

                let mut pos = touch_pos;
                // Only allow selecting text that is visible in the box
                // We do our calcs relative to (0, 0) so bhs = rect_h
                pos.y -= handle_descent + 25.;
                //let bhs = wrapped_lines.height();
                //pos.y = min_f32(pos.y, bhs);

                let mut select = sel.text_range();

                let layout = editor.layout();
                t!("select  (pre): {:?}", sel.text_range());
                let mut cursor = parley::Cursor::from_point(layout, pos.x, pos.y).index();
                t!("cursor: {cursor}");

                // The selection is NOT allowed to cross over itself.
                if *side == -1 {
                    let max_start = sel.focus().previous_visual(layout).index();
                    t!("side == -1  max={max_start}");
                    select.start = std::cmp::min(cursor, max_start);
                } else {
                    assert_eq!(*side, 1);
                    let min_end = sel.anchor().next_visual(layout).index();
                    t!("side == +1  min={min_end}");
                    select.end = std::cmp::max(cursor, min_end);
                };
                t!("set_select({select:?})");

                editor.set_selection(select.start, select.end);
                drop(editor);

                self.redraw_select().await;
            }
            TouchStateAction::ScrollVert { start_pos, scroll_start } => {
                let y_dist = start_pos.y - touch_pos.y;
                let mut scroll = scroll_start + y_dist;
                scroll = scroll.clamp(0., self.max_scroll());
                if (self.scroll.get() - scroll).abs() < VERT_SCROLL_UPDATE_INC {
                    return true
                }
                self.scroll.set(atom, scroll);
                self.redraw_scroll().await;
            }
            TouchStateAction::SetCursorPos => {
                // TBH I can't even see the cursor under my thumb so I'll just
                // comment this for now.
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
        node.trigger("focus_request", vec![]).await.unwrap();

        true
    }

    async fn touch_set_cursor_pos(&self, mut touch_pos: Point, atom: &mut PropertyAtomicGuard) {
        t!("touch_set_cursor_pos({touch_pos:?})");

        let mut editor = self.editor.lock().await;
        editor.move_to_pos(touch_pos);
        editor.refresh(atom).await;
        drop(editor);

        self.pause_blinking();

        self.is_phone_select.store(false, Ordering::Relaxed);
        self.hide_cursor.store(false, Ordering::Relaxed);
    }

    /// Whenever the cursor property is modified this MUST be called
    /// to recalculate the scroll x property.
    fn apply_cursor_scrolling(&self, atom: &mut PropertyAtomicGuard) {
        /*
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
        */
    }

    fn pause_blinking(&self) {
        self.blink_is_paused.store(true, Ordering::Relaxed);
        self.cursor_is_visible.store(true, Ordering::Relaxed);
    }

    async fn redraw(&self) {
        let atom = &mut PropertyAtomicGuard::new();
        let trace_id = rand::random();
        let timest = unixtime();
        let draw_update = self.make_draw_calls(trace_id, atom).await;
        self.render_api.replace_draw_calls(timest, draw_update.draw_calls);
    }

    /// Called when scroll changes. Moves content up or down. Nothing more.
    async fn redraw_scroll(&self) {
        let timest = unixtime();
        let rect = self.rect.get();
        let scroll = self.scroll.get();

        let draw_main = vec![(
            self.content_dc_key,
            GfxDrawCall {
                instrs: vec![GfxDrawInstruction::Move(Point::new(0., -scroll))],
                dcs: vec![self.text_dc_key, self.cursor_dc_key, self.select_dc_key],
                z_index: self.z_index.get(),
            },
        )];
        self.render_api.replace_draw_calls(timest, draw_main);
    }

    async fn redraw_cursor(&self) {
        let timest = unixtime();
        let instrs = self.get_cursor_instrs().await;
        let draw_calls =
            vec![(self.cursor_dc_key, GfxDrawCall { instrs, dcs: vec![], z_index: 2 })];
        self.render_api.replace_draw_calls(timest, draw_calls);
    }

    async fn redraw_select(&self) {
        let timest = unixtime();
        let instrs = self.regen_select_mesh().await;
        let draw_calls =
            vec![(self.select_dc_key, GfxDrawCall { instrs, dcs: vec![], z_index: 0 })];
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

        let mut cursor_pos = self.editor.lock().await.get_cursor_pos();
        cursor_pos += self.inner_pos();
        cursor_instrs.push(GfxDrawInstruction::Move(cursor_pos));

        let cursor_mesh =
            self.cursor_mesh.lock().get_or_insert_with(|| self.regen_cursor_mesh()).clone();
        cursor_instrs.push(GfxDrawInstruction::Draw(cursor_mesh));

        cursor_instrs
    }

    fn regen_bg_mesh(&self) -> Vec<GfxDrawInstruction> {
        let padding_top = self.padding_top();
        let padding_bottom = self.padding_bottom();

        let mut instrs = vec![];
        if self.debug.get() {
            let mut rect = self.rect.get();
            rect.x = 0.;
            rect.y = 0.;
            let mut mesh = MeshBuilder::new();
            mesh.draw_outline(&rect, [0., 1., 0., 1.], 1.);
            instrs.push(GfxDrawInstruction::Draw(mesh.alloc(&self.render_api).draw_untextured()));

            rect.y = padding_top;
            rect.h -= padding_top + padding_bottom;
            let mut mesh = MeshBuilder::new();
            mesh.draw_outline(&rect, [0., 1., 0., 0.5], 1.);
            instrs.push(GfxDrawInstruction::Draw(mesh.alloc(&self.render_api).draw_untextured()));
        }
        instrs
    }

    async fn regen_txt_mesh(&self) -> Vec<GfxDrawInstruction> {
        let mut instrs = vec![GfxDrawInstruction::Move(self.inner_pos())];

        let editor = self.editor.lock().await;
        let layout = editor.layout();

        let mut render_instrs = text2::render_layout(layout, &self.render_api);
        instrs.append(&mut render_instrs);

        instrs
    }

    async fn regen_select_mesh(&self) -> Vec<GfxDrawInstruction> {
        let padding_top = self.padding_top();

        let mut instrs = vec![GfxDrawInstruction::Move(self.inner_pos())];

        let editor = self.editor.lock().await;
        let layout = editor.layout();

        let sel = editor.selection();
        let sel_color = self.hi_bg_color.get();
        if !sel.is_collapsed() {
            let mut first_pos = None;
            let mut last_pos = Default::default();

            let mut mesh = MeshBuilder::new();
            sel.geometry_with(layout, |rect: parley::Rect, _| {
                let rect = Rectangle::from(rect);
                if first_pos.is_none() {
                    first_pos = Some(rect.pos());
                }
                last_pos = rect.top_right();

                mesh.draw_filled_box(&rect, sel_color);
            });

            if self.is_phone_select.load(Ordering::Relaxed) {
                self.draw_phone_select_handle(&mut mesh, first_pos.unwrap(), -1.);
                self.draw_phone_select_handle(&mut mesh, last_pos, 1.);
            }

            instrs.push(GfxDrawInstruction::Draw(mesh.alloc(&self.render_api).draw_untextured()));
        }

        instrs
    }

    fn bounded_height(&self, mut height: f32) -> f32 {
        let min_height = self.min_height.get();
        let max_height = self.max_height.get();

        if height < min_height {
            height = min_height;
        }
        if height > max_height {
            height = max_height;
        }

        height
    }

    async fn eval_rect(&self, atom: &mut PropertyAtomicGuard) {
        let parent_rect = self.parent_rect.lock().clone().unwrap();
        // First we evaluate the width based off the parent dimensions
        self.rect
            .eval_with(
                vec![2],
                vec![
                    ("parent_w".to_string(), parent_rect.w),
                    ("parent_h".to_string(), parent_rect.h),
                ],
            )
            .unwrap();

        // Use the width to adjust the height calcs
        let rect_w = self.rect.get_width();
        let content_height = {
            let mut editor = self.editor.lock().await;
            editor.set_width(rect_w);
            editor.refresh(atom).await;
            editor.height()
        };
        self.content_height.set(atom, content_height);
        let outer_height = content_height + self.padding_top() + self.padding_bottom();
        let rect_h = self.bounded_height(outer_height);
        self.rect.prop().set_f32(atom, Role::Internal, 3, rect_h);

        // Finally calculate the position
        self.rect
            .eval_with(
                vec![0, 1],
                vec![
                    ("parent_w".to_string(), parent_rect.w),
                    ("parent_h".to_string(), parent_rect.h),
                    ("rect_w".to_string(), rect_w),
                    ("rect_h".to_string(), rect_h),
                ],
            )
            .unwrap();
    }

    async fn make_draw_calls(&self, trace_id: u32, atom: &mut PropertyAtomicGuard) -> DrawUpdate {
        self.eval_rect(atom).await;

        let rect = self.rect.get();
        let max_scroll = self.max_scroll();
        let mut scroll = self.scroll.get();
        if scroll > max_scroll {
            scroll = max_scroll;
            self.scroll.set(atom, scroll);
        }

        let cursor_instrs = self.get_cursor_instrs().await;
        let txt_instrs = self.regen_txt_mesh().await;
        let sel_instrs = self.regen_select_mesh().await;

        let mut root_instrs = vec![GfxDrawInstruction::ApplyView(rect)];
        let mut bg_instrs = self.regen_bg_mesh();
        root_instrs.append(&mut bg_instrs);

        DrawUpdate {
            key: self.root_dc_key,
            draw_calls: vec![
                (
                    self.root_dc_key,
                    GfxDrawCall {
                        instrs: root_instrs,
                        dcs: vec![self.content_dc_key],
                        z_index: self.z_index.get(),
                    },
                ),
                (
                    self.content_dc_key,
                    GfxDrawCall {
                        instrs: vec![GfxDrawInstruction::Move(Point::new(0., -scroll))],
                        dcs: vec![self.text_dc_key, self.cursor_dc_key, self.select_dc_key],
                        z_index: 0,
                    },
                ),
                (self.select_dc_key, GfxDrawCall { instrs: sel_instrs, dcs: vec![], z_index: 0 }),
                (self.text_dc_key, GfxDrawCall { instrs: txt_instrs, dcs: vec![], z_index: 1 }),
                (
                    self.cursor_dc_key,
                    GfxDrawCall { instrs: cursor_instrs, dcs: vec![], z_index: 2 },
                ),
            ],
        }
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

    async fn process_focus_method(me: &Weak<Self>, sub: &MethodCallSub) -> bool {
        let Ok(method_call) = sub.receive().await else {
            debug!(target: "ui::chatedit", "Event relayer closed");
            return false
        };

        t!("method called: focus({method_call:?})");
        assert!(method_call.send_res.is_none());
        assert!(method_call.data.is_empty());

        let Some(self_) = me.upgrade() else {
            // Should not happen
            panic!("self destroyed before insert_text_method_task was stopped!");
        };

        let mut editor = self_.editor.lock().await;
        editor.focus();
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
            AndroidSuggestEvent::ComposeRegion { .. } | AndroidSuggestEvent::FinishCompose => {}
            // Destructive text edits
            AndroidSuggestEvent::Compose { .. } |
            AndroidSuggestEvent::DeleteSurroundingText { .. } => {
                // Any editing will collapse selections
                self.is_phone_select.store(false, Ordering::Relaxed);
                self.hide_cursor.store(false, Ordering::Relaxed);
            }
        }

        let atom = &mut PropertyAtomicGuard::new();
        editor.refresh(atom).await;
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

        let method_sub = node_ref.subscribe_method_call("focus").unwrap();
        let me2 = me.clone();
        let focus_task =
            ex.spawn(async move { while Self::process_focus_method(&me2, &method_sub).await {} });

        let mut on_modify = OnModify::new(ex.clone(), self.node.clone(), me.clone());
        on_modify.when_change(self.is_focused.prop(), Self::change_focus);

        // When text has been changed.
        // Cursor and selection might be invalidated.
        async fn reset(self_: Arc<ChatEdit>) {
            let atom = &mut PropertyAtomicGuard::new();
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

        let mut tasks = vec![insert_text_task, focus_task, blinking_cursor_task];
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

        Some(self.make_draw_calls(trace_id, atom).await)
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

        {
            let mut txt_ctx = text2::TEXT_CTX.get().await;
            let mut editor = self.editor.lock().await;
            let mut drv = editor.driver(&mut txt_ctx).await.unwrap();
            drv.move_to_point(mouse_pos.x, mouse_pos.y);
        }

        if !self.select_text.is_null(0).unwrap() {
            self.select_text.clone().set_null(atom, Role::Internal, 0).unwrap();
        }

        self.mouse_btn_held.store(true, Ordering::Relaxed);

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
            return false
        }

        let atom = &mut PropertyAtomicGuard::new();

        // if active and selection_active, then use x to modify the selection.
        // also implement scrolling when cursor is to the left or right
        // just scroll to the end
        // also set cursor_pos too

        // Move mouse pos within this widget
        self.abs_to_local(&mut mouse_pos);

        let seltext = {
            let mut txt_ctx = text2::TEXT_CTX.get().await;
            let mut editor = self.editor.lock().await;
            let mut drv = editor.driver(&mut txt_ctx).await.unwrap();
            drv.extend_selection_to_point(mouse_pos.x, mouse_pos.y);
            editor.selected_text()
        };
        d!("Select {seltext:?} from {mouse_pos:?}");

        // Will be None when drag select just started
        if let Some(seltext) = seltext {
            self.select_text.clone().set_str(atom, Role::Internal, 0, seltext).unwrap();
        }

        self.pause_blinking();
        //self.apply_cursor_scrolling();
        self.redraw_select().await;
        true
    }

    async fn handle_mouse_wheel(&self, wheel_pos: Point) -> bool {
        if !self.is_mouse_hover.load(Ordering::Relaxed) {
            return false
        }

        let atom = &mut PropertyAtomicGuard::new();

        let mut scroll = self.scroll.get() - wheel_pos.y * self.scroll_speed.get();
        scroll = scroll.clamp(0., self.max_scroll());
        t!("handle_mouse_wheel({wheel_pos:?}) [scroll={scroll}]");
        self.scroll.set(atom, scroll);
        self.redraw_scroll().await;

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
