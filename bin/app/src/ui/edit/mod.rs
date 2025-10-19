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
use darkfi_serial::Decodable;
use futures::{select, FutureExt};
use miniquad::{KeyCode, KeyMods, MouseButton, TouchPhase};
use parking_lot::Mutex as SyncMutex;
use rand::{rngs::OsRng, Rng};
use std::{
    io::Cursor,
    mem::swap,
    ops::{Deref, DerefMut},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Weak,
    },
};

#[cfg(target_os = "android")]
use crate::AndroidSuggestEvent;
use crate::{
    gfx::{gfxtag, DrawCall, DrawInstruction, DrawMesh, Point, Rectangle, RenderApi, Vertex},
    mesh::MeshBuilder,
    prop::{
        BatchGuardId, BatchGuardPtr, PropertyAtomicGuard, PropertyBool, PropertyColor,
        PropertyFloat32, PropertyPtr, PropertyRect, PropertyStr, PropertyUint32, Role,
    },
    scene::{MethodCallSub, Pimpl, SceneNodePtr, SceneNodeWeak},
    text2::{self, Editor},
    util::unixtime,
    ExecutorPtr,
};

use super::{DrawUpdate, OnModify, UIObject};

mod filter;
use filter::{ALLOWED_KEYCODES, DISALLOWED_CHARS};
mod behave;
pub use behave::BaseEditType;
use behave::{EditorBehavior, MultiLine, ScrollDir, SingleLine};
mod repeat;
use repeat::{PressedKey, PressedKeysSmoothRepeat};

/// The travel threshold on long hold select before activating select.
const HOLD_TRAVEL_THRESHOLD_SQ: f32 = 100.;
/// How long to hold before select is enabled in ms.
const HOLD_ENABLE_TIME: u128 = 500;

/// Minimum dist to update scroll when finger scrolling.
/// Avoid updating too much makes scrolling smoother.
const VERT_SCROLL_UPDATE_INC: f32 = 1.;

/// How often to update the scrolling selection with mouse.
const SELECT_TASK_UPDATE_TIME: u64 = 500;
// Should be a property
const SELECT_SCROLL_TRAVEL_SPEED: f32 = 1.;

macro_rules! d { ($($arg:tt)*) => { debug!(target: "ui::edit", $($arg)*); } }
macro_rules! t { ($($arg:tt)*) => { trace!(target: "ui::edit", $($arg)*); } }

#[derive(Debug, Clone)]
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
    scroll: Arc<AtomicF32>,
    scroll_ctrl: ScrollDir,
}

impl TouchInfo {
    fn new(scroll: Arc<AtomicF32>, scroll_ctrl: ScrollDir) -> Self {
        Self { state: TouchStateAction::Inactive, scroll, scroll_ctrl }
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
                let travel_dist_sq = pos.dist_sq(*start_pos);
                let grad = (pos.y - start_pos.y) / (pos.x - start_pos.x);
                let elapsed = instant.elapsed().as_millis();
                //debug!(target: "ui::chatedit::touch", "TouchInfo::update() [travel_dist_sq={travel_dist_sq}, grad={grad}]");

                if travel_dist_sq < HOLD_TRAVEL_THRESHOLD_SQ {
                    if elapsed > HOLD_ENABLE_TIME {
                        debug!(target: "ui::chatedit::touch", "update touch state: Started -> StartSelect");
                        self.state = TouchStateAction::StartSelect;
                    }
                } else if self.scroll_ctrl.cmp(grad) {
                    // Vertical movement
                    debug!(target: "ui::chatedit::touch", "update touch state: Started -> ScrollVert");
                    let scroll_start = self.scroll.load(Ordering::Relaxed);
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

struct EditorHandle<'a> {
    guard: async_lock::MutexGuard<'a, Option<Editor>>,
}

impl<'a> Deref for EditorHandle<'a> {
    type Target = Editor;

    fn deref(&self) -> &Self::Target {
        self.guard.as_ref().unwrap()
    }
}
impl<'a> DerefMut for EditorHandle<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.guard.as_mut().unwrap()
    }
}

pub type BaseEditPtr = Arc<BaseEdit>;

pub struct BaseEdit {
    node: SceneNodeWeak,
    tasks: SyncMutex<Vec<smol::Task<()>>>,
    render_api: RenderApi,
    key_repeat: SyncMutex<PressedKeysSmoothRepeat>,

    // Moves the draw cursor and applies scroll
    root_dc_key: u64,
    phone_select_handle_dc_key: u64,
    // Applies the clipping view
    content_dc_key: u64,
    select_dc_key: u64,
    text_dc_key: u64,
    cursor_dc_key: u64,
    cursor_mesh: SyncMutex<Option<DrawMesh>>,

    is_active: PropertyBool,
    is_focused: PropertyBool,
    rect: PropertyRect,
    baseline: PropertyFloat32,
    lineheight: PropertyFloat32,
    scroll_speed: PropertyFloat32,
    padding: PropertyPtr,
    font_size: PropertyFloat32,
    text: PropertyStr,
    text_color: PropertyColor,
    text_hi_color: PropertyColor,
    //text_cmd_color: PropertyColor,
    cursor_color: PropertyColor,
    cursor_width: PropertyFloat32,
    cursor_ascent: PropertyFloat32,
    cursor_descent: PropertyFloat32,
    cursor_blink_time: PropertyUint32,
    cursor_idle_time: PropertyUint32,
    hi_bg_color: PropertyColor,
    //cmd_bg_color: PropertyColor,
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
    /// Used to start select and scroll when mouse moves outside widget rect.
    sel_sender: SyncMutex<Option<async_channel::Sender<Option<(Point, Option<isize>)>>>>,
    scroll: Arc<AtomicF32>,

    touch_info: SyncMutex<TouchInfo>,
    is_phone_select: AtomicBool,

    window_scale: PropertyFloat32,
    parent_rect: Arc<SyncMutex<Option<Rectangle>>>,
    is_mouse_hover: AtomicBool,

    editor: Arc<AsyncMutex<Option<Editor>>>,
    behave: Box<dyn EditorBehavior>,
}

impl BaseEdit {
    pub async fn new(
        node: SceneNodeWeak,
        window_scale: PropertyFloat32,
        render_api: RenderApi,
        edit_type: BaseEditType,
    ) -> Pimpl {
        t!("BaseEdit::new()");

        let node_ref = &node.upgrade().unwrap();
        let is_active = PropertyBool::wrap(node_ref, Role::Internal, "is_active", 0).unwrap();
        let is_focused = PropertyBool::wrap(node_ref, Role::Internal, "is_focused", 0).unwrap();
        let rect = PropertyRect::wrap(node_ref, Role::Internal, "rect").unwrap();
        let baseline = PropertyFloat32::wrap(node_ref, Role::Internal, "baseline", 0).unwrap();
        let lineheight = PropertyFloat32::wrap(node_ref, Role::Internal, "lineheight", 0).unwrap();
        let scroll_speed =
            PropertyFloat32::wrap(node_ref, Role::Internal, "scroll_speed", 0).unwrap();
        let padding = node_ref.get_property("padding").unwrap();
        let font_size = PropertyFloat32::wrap(node_ref, Role::Internal, "font_size", 0).unwrap();
        let text = PropertyStr::wrap(node_ref, Role::Internal, "text", 0).unwrap();
        let text_color = PropertyColor::wrap(node_ref, Role::Internal, "text_color").unwrap();
        let text_hi_color = PropertyColor::wrap(node_ref, Role::Internal, "text_hi_color").unwrap();
        //let text_cmd_color =
        //    PropertyColor::wrap(node_ref, Role::Internal, "text_cmd_color").unwrap();
        let cursor_color = PropertyColor::wrap(node_ref, Role::Internal, "cursor_color").unwrap();
        let cursor_width =
            PropertyFloat32::wrap(node_ref, Role::Internal, "cursor_width", 0).unwrap();
        let cursor_ascent =
            PropertyFloat32::wrap(node_ref, Role::Internal, "cursor_ascent", 0).unwrap();
        let cursor_descent =
            PropertyFloat32::wrap(node_ref, Role::Internal, "cursor_descent", 0).unwrap();
        let hi_bg_color = PropertyColor::wrap(node_ref, Role::Internal, "hi_bg_color").unwrap();
        //let cmd_bg_color = PropertyColor::wrap(node_ref, Role::Internal, "cmd_bg_color").unwrap();
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

        let parent_rect = Arc::new(SyncMutex::new(None));
        let editor = Arc::new(AsyncMutex::new(None));
        let scroll = Arc::new(AtomicF32::new(0.));
        let behave: Box<dyn EditorBehavior> = match edit_type {
            BaseEditType::SingleLine => Box::new(SingleLine {
                rect: rect.clone(),
                padding: padding.clone(),
                cursor_width: cursor_width.clone(),
                parent_rect: parent_rect.clone(),
                editor: editor.clone(),
                content_height: AtomicF32::new(0.),
                scroll: scroll.clone(),
            }),
            BaseEditType::MultiLine => {
                let min_height =
                    PropertyFloat32::wrap(node_ref, Role::Internal, "height_range", 0).unwrap();
                let max_height =
                    PropertyFloat32::wrap(node_ref, Role::Internal, "height_range", 1).unwrap();

                Box::new(MultiLine {
                    min_height: min_height.clone(),
                    max_height: max_height.clone(),
                    rect: rect.clone(),
                    baseline: baseline.clone(),
                    padding: padding.clone(),
                    cursor_descent: cursor_descent.clone(),
                    parent_rect: parent_rect.clone(),
                    editor: editor.clone(),
                    content_height: AtomicF32::new(0.),
                    scroll: scroll.clone(),
                })
            }
        };

        let self_ = Arc::new(Self {
            node,
            tasks: SyncMutex::new(vec![]),
            render_api,
            key_repeat: SyncMutex::new(PressedKeysSmoothRepeat::new(400, 50)),

            root_dc_key: OsRng.gen(),
            phone_select_handle_dc_key: OsRng.gen(),
            content_dc_key: OsRng.gen(),
            select_dc_key: OsRng.gen(),
            text_dc_key: OsRng.gen(),
            cursor_dc_key: OsRng.gen(),
            cursor_mesh: SyncMutex::new(None),

            is_active,
            is_focused,
            rect,
            baseline,
            lineheight: lineheight.clone(),
            scroll_speed,
            padding,
            font_size: font_size.clone(),
            text: text.clone(),
            text_color: text_color.clone(),
            text_hi_color,
            //text_cmd_color,
            cursor_color,
            cursor_width,
            cursor_ascent,
            cursor_descent,
            cursor_blink_time,
            cursor_idle_time,
            hi_bg_color,
            //cmd_bg_color,
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
            sel_sender: SyncMutex::new(None),
            scroll: scroll.clone(),

            touch_info: SyncMutex::new(TouchInfo::new(scroll, behave.scroll_ctrl())),
            is_phone_select: AtomicBool::new(false),

            window_scale: window_scale.clone(),
            parent_rect,
            is_mouse_hover: AtomicBool::new(false),

            editor,
            behave,
        });

        Pimpl::Edit(self_)
    }

    fn node(&self) -> SceneNodePtr {
        self.node.upgrade().unwrap()
    }

    fn abs_to_local(&self, point: &mut Point) {
        let rect = self.rect.get();
        *point -= rect.pos();
        *point -= self.behave.inner_pos();
        *point -= self.behave.scroll();
    }

    /// Gets the real cursor pos within the rect.
    async fn get_cursor_pos(&self) -> Point {
        // This is the position within the content.
        let cursor_pos = self.lock_editor().await.get_cursor_pos();
        // Apply the inner padding
        cursor_pos + self.behave.inner_pos()
    }

    /// Lazy-initializes the editor and returns a handle to it
    async fn lock_editor<'a>(&'a self) -> EditorHandle<'a> {
        EditorHandle { guard: self.editor.lock().await }
    }

    fn regen_cursor_mesh(&self) -> DrawMesh {
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

        let mut mesh = MeshBuilder::new(gfxtag!("chatedit_cursor"));
        mesh.draw_filled_box(&cursor_rect, cursor_color);
        mesh.alloc(&self.render_api).draw_untextured()
    }

    fn draw_phone_select_handle(&self, mesh: &mut MeshBuilder, mut pos: Point, side: f32) {
        let baseline = self.baseline.get();
        let select_ascent = self.select_ascent.get();
        let handle_descent = self.handle_descent.get();
        let color = self.text_hi_color.get();
        // Transparent for fade
        let mut color_trans = color.clone();
        color_trans[3] = 0.;

        pos += self.behave.inner_pos();
        let x = pos.x;
        let mut y = pos.y + baseline;

        // We can cache this

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

    async fn change_focus(self: Arc<Self>, batch: BatchGuardPtr) {
        if !self.is_active.get() {
            return
        }
        t!("Focus changed");

        let atom = &mut batch.spawn();
        // Cursor visibility will change so just redraw everything lol
        self.redraw(atom).await;
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
                    let mut txt_ctx = text2::TEXT_CTX.get().await;
                    let mut editor = self.lock_editor().await;
                    let mut drv = editor.driver(&mut txt_ctx).unwrap();

                    drv.select_all();
                    if let Some(seltext) = editor.selected_text() {
                        self.select_text.clone().set_str(atom, Role::Internal, 0, seltext).unwrap();
                    }
                }
            }
            'c' => {
                if action_mod {
                    let editor = self.lock_editor().await;
                    if let Some(txt) = editor.selected_text() {
                        miniquad::window::clipboard_set(&txt);
                    }
                }
            }
            'v' => {
                if action_mod {
                    if let Some(txt) = miniquad::window::clipboard_get() {
                        self.insert(&txt, atom).await;
                        // Maybe insert should call this?
                        self.behave.apply_cursor_scroll(atom).await;
                    }
                }
            }
            _ => return false,
        }

        self.redraw(atom).await;
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
        let mut editor = self.lock_editor().await;
        let mut drv = editor.driver(&mut txt_ctx).unwrap();

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
                    if self.behave.allow_endl() {
                        drv.insert_or_replace_selection("\n");
                        editor.on_buffer_changed(atom).await;
                    }
                } else {
                    //let node = self.node.upgrade().unwrap();
                    //node.trigger("enter_pressed", vec![]).await.unwrap();
                    return false;
                }
            }
            KeyCode::Delete => {
                if action_mod {
                    drv.delete_word();
                } else {
                    drv.delete();
                }
                editor.on_buffer_changed(atom).await;
            }
            KeyCode::Backspace => {
                if action_mod {
                    drv.backdelete_word();
                } else {
                    drv.backdelete();
                }
                editor.on_buffer_changed(atom).await;
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

        if let Some(seltext) = editor.selected_text() {
            self.select_text.clone().set_str(atom, Role::Internal, 0, seltext).unwrap();
        } else {
            self.select_text.clone().set_null(atom, Role::Internal, 0).unwrap();
        }

        drop(editor);
        drop(txt_ctx);

        self.behave.apply_cursor_scroll(atom).await;
        self.pause_blinking();
        self.redraw(atom).await;

        return true
    }

    /// This will select the entire word rather than move the cursor to that location
    async fn start_touch_select(&self, touch_pos: Point, atom: &mut PropertyAtomicGuard) {
        t!("start_touch_select({touch_pos:?})");

        let mut editor = self.lock_editor().await;
        editor.select_word_at_point(touch_pos);
        editor.refresh().await;

        let seltext = editor.selected_text().unwrap();
        d!("Selected {seltext:?} from {touch_pos:?}");
        self.select_text.clone().set_str(atom, Role::Internal, 0, seltext).unwrap();

        drop(editor);

        // if start != end {
        t!("is_phone_select = true");
        self.is_phone_select.store(true, Ordering::Relaxed);
        self.hide_cursor.store(true, Ordering::Relaxed);
        // }
    }

    async fn handle_touch_start(&self, touch_pos: Point) -> bool {
        t!("handle_touch_start({touch_pos:?})");

        let rect = self.rect.get();
        if !rect.contains(touch_pos) {
            t!("rect!cont rect={rect:?}, touch_pos={touch_pos:?}");
            return false
        }

        if self.try_handle_drag(touch_pos).await {
            return true
        }

        let mut touch_info = self.touch_info.lock();
        touch_info.start(touch_pos);
        true
    }

    async fn get_select_handles(&self, editor: &Editor) -> Option<(Point, Point)> {
        let layout = editor.layout();

        let sel = editor.selection(1);
        if sel.is_collapsed() {
            assert!(!self.is_phone_select.load(Ordering::Relaxed));
            return None
        }

        let first = Rectangle::from(sel.anchor().geometry(layout, 0.)).pos();
        let last = Rectangle::from(sel.focus().geometry(layout, 0.)).pos();
        Some((first, last))
    }

    async fn try_handle_drag(&self, mut touch_pos: Point) -> bool {
        let editor = self.lock_editor().await;
        let Some((mut first, mut last)) = self.get_select_handles(&editor).await else {
            return false
        };

        self.abs_to_local(&mut touch_pos);
        t!("localize touch_pos = {touch_pos:?}");

        let baseline = self.baseline.get();
        let handle_off_y = self.handle_descent.get();

        first.y += baseline + handle_off_y;
        last.y += baseline + handle_off_y;

        // Are we within range of either one?
        t!("handle center points = ({first:?}, {last:?})");

        const TOUCH_RADIUS_SQ: f32 = 10_000.;

        let first_dist_sq = first.dist_sq(touch_pos);
        let last_dist_sq = last.dist_sq(touch_pos);

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
        if !self.is_active.get() {
            return false
        }

        // We must update with non relative touch_pos bcos when doing vertical scrolling
        // we will modify the scroll, which is used by abs_to_local(), which is used
        // to then calculate the max scroll. So it ends up jumping around.
        // We use the abs touch_pos without scroll adjust applied for vert scrolling.
        let touch_state = {
            let mut touch_info = self.touch_info.lock();
            touch_info.update(&touch_pos);
            touch_info.state.clone()
        };
        //t!("handle_touch_move({touch_pos:?})  touch_state={touch_state:?}");
        match &touch_state {
            TouchStateAction::Inactive => return false,
            TouchStateAction::StartSelect => {
                if self.text.get().is_empty() {
                    self.node().trigger("paste_request", vec![]).await.unwrap();
                } else {
                    self.abs_to_local(&mut touch_pos);

                    let atom =
                        &mut self.render_api.make_guard(gfxtag!("BaseEdit::handle_touch_move"));
                    self.start_touch_select(touch_pos, atom).await;
                    self.redraw_select(atom.batch_id).await;
                }
                d!("touch state: StartSelect -> Select");
                self.touch_info.lock().state = TouchStateAction::Select;
            }
            TouchStateAction::DragSelectHandle { side } => {
                let handle_descent = self.handle_descent.get();

                // New code

                let rect = self.rect.get();
                let is_touch_hover = rect.contains(touch_pos);

                let sel_sender = self.sel_sender.lock().clone().unwrap();
                // Mouse is outside rect?
                // If so we gotta scroll it while selecting.
                if !is_touch_hover {
                    // This process will begin selecting text and applying scroll too.
                    sel_sender.send(Some((touch_pos, Some(*side)))).await.unwrap();
                } else {
                    // Stop any existing select/scroll process
                    sel_sender.send(None).await.unwrap();
                    // Mouse is inside so just select the text once and be done.
                    self.handle_select(touch_pos, Some(*side)).await;
                }
            }
            TouchStateAction::ScrollVert { start_pos, scroll_start } => {
                let travel_dist = self.behave.scroll_ctrl().travel(*start_pos, touch_pos);
                let mut scroll = scroll_start + travel_dist;
                scroll = scroll.clamp(0., self.behave.max_scroll().await);
                if (self.scroll.load(Ordering::Relaxed) - scroll).abs() < VERT_SCROLL_UPDATE_INC {
                    return true
                }
                self.scroll.store(scroll, Ordering::Release);
                let atom = &mut self.render_api.make_guard(gfxtag!("BaseEdit::handle_touch_move"));
                self.redraw_scroll(atom.batch_id).await;
            }
            TouchStateAction::SetCursorPos => {
                // TBH I can't even see the cursor under my thumb so I'll just
                // comment this for now.
            }
            _ => {}
        }
        true
    }
    async fn handle_touch_end(&self, atom: &mut PropertyAtomicGuard, mut touch_pos: Point) -> bool {
        //t!("handle_touch_end({touch_pos:?})");
        self.abs_to_local(&mut touch_pos);

        let state = self.touch_info.lock().stop();
        match state {
            TouchStateAction::Inactive => return false,
            TouchStateAction::Started { pos: _, instant: _ } | TouchStateAction::SetCursorPos => {
                self.touch_set_cursor_pos(atom, touch_pos).await;
                self.redraw(atom).await;
            }
            _ => {}
        }

        // Stop any selection scrolling
        let scroll_sender = self.sel_sender.lock().clone().unwrap();
        scroll_sender.send(None).await.unwrap();

        self.node().trigger("focus_request", vec![]).await.unwrap();

        true
    }

    async fn touch_set_cursor_pos(&self, atom: &mut PropertyAtomicGuard, touch_pos: Point) {
        t!("touch_set_cursor_pos({touch_pos:?})");

        let mut editor = self.lock_editor().await;
        editor.move_to_pos(touch_pos);
        editor.refresh().await;
        drop(editor);

        self.pause_blinking();
        self.finish_select(atom);
    }

    fn finish_select(&self, atom: &mut PropertyAtomicGuard) {
        self.is_phone_select.store(false, Ordering::Release);
        self.hide_cursor.store(false, Ordering::Release);
        self.select_text.clone().set_null(atom, Role::Internal, 0).unwrap();
    }

    async fn handle_select(&self, mouse_pos: Point, side: Option<isize>) {
        //t!("handle_select({mouse_pos:?}, {side:?})");
        let rect = self.rect.get();
        let is_mouse_hover = rect.contains(mouse_pos);

        let mut clip_mouse_pos = rect.clip_point(mouse_pos);

        let atom = &mut self.render_api.make_guard(gfxtag!("BaseEdit::handle_mouse_move"));

        // Handle scrolling
        if !is_mouse_hover {
            let scroll_ctrl = self.behave.scroll_ctrl();
            // How far is the cursor outside the widget rect
            let travel = scroll_ctrl.travel(mouse_pos, clip_mouse_pos);
            //t!("select autoscroll travel={travel}");

            let max_scroll = self.behave.max_scroll().await;
            let delta = travel * SELECT_SCROLL_TRAVEL_SPEED;
            let scroll = (self.scroll.load(Ordering::Relaxed) + delta).clamp(0., max_scroll);
            self.scroll.store(scroll, Ordering::Release);

            self.redraw_scroll(atom.batch_id).await;
        }

        // Move mouse pos within this widget
        self.abs_to_local(&mut clip_mouse_pos);

        let mut sel_side = 1;
        if let Some(side) = side {
            sel_side = side;
        }

        let (seltext, sel_start, sel_end) = {
            let mut editor = self.lock_editor().await;

            // The below lines rely on parley driver which android does not use
            //let mut txt_ctx = text2::TEXT_CTX.get().await;
            //let mut drv = editor.driver(&mut txt_ctx).unwrap();
            //drv.extend_selection_to_point(clip_mouse_pos.x, clip_mouse_pos.y);
            let layout = editor.layout();
            let mut prev_sel = editor.selection(sel_side);
            let sel = prev_sel.extend_to_point(layout, clip_mouse_pos.x, clip_mouse_pos.y);
            let (mut prev_start, mut prev_end) =
                (prev_sel.anchor().index(), prev_sel.focus().index());
            let (mut start, mut end) = (sel.anchor().index(), sel.focus().index());
            //t!("handle_select() setting ({start}, {end})");

            // When dragging phone handles we prevent the selection crossing over itself.
            // Mouse select does not have this limitation.
            if let Some(side) = side {
                assert!(side.abs() == 1);
                assert!(self.is_phone_select.load(Ordering::Relaxed));

                // Prevent selection crossing itself
                if side == -1 {
                    let max_end = sel.anchor().previous_visual(layout).index();
                    end = std::cmp::min(end, max_end);
                    //t!("handle_select(): LHS set focus {end} before {max_end}");
                    // When selecting from the LHS these will be swapped so swap them back
                    swap(&mut start, &mut end);
                    swap(&mut prev_start, &mut prev_end);
                } else {
                    let min_end = sel.anchor().next_visual(layout).index();
                    end = std::cmp::max(end, min_end);
                    //t!("handle_select(): RHS set focus {end} after {min_end}");
                }
            }

            if start == prev_start && end == prev_end {
                // No change so just return
                //t!("handle_select(): no change early exit");
                return
            }
            #[cfg(target_os = "android")]
            assert!(start <= end);

            //t!("handle_select(): set_selection({start}, {end})");
            editor.set_selection(start, end);
            editor.refresh().await;

            (editor.selected_text(), start, end)
        };
        //d!("Select {seltext:?} from {clip_mouse_pos:?} (unclipped: {mouse_pos:?}) to ({sel_start}, {sel_end})");
        // Android editor impl detail: selection disappears when anchor == index
        #[cfg(target_os = "android")]
        {
            assert!(sel_start != sel_end);
            //if sel_start == sel_end {
            //    self.finish_select(atom);
            //}
        }

        // Will be None when drag select just started
        if let Some(seltext) = seltext {
            self.select_text.clone().set_str(atom, Role::Internal, 0, seltext).unwrap();
        }

        self.pause_blinking();
        //self.behave.apply_cursor_scroll(atom).await;
        self.redraw_cursor(atom.batch_id).await;
        self.redraw_select(atom.batch_id).await;
    }

    fn pause_blinking(&self) {
        self.blink_is_paused.store(true, Ordering::Relaxed);
        self.cursor_is_visible.store(true, Ordering::Relaxed);
    }

    async fn redraw(&self, atom: &mut PropertyAtomicGuard) {
        let trace_id = rand::random();
        let timest = unixtime();
        let draw_update = self.make_draw_calls(trace_id, atom).await;
        self.render_api.replace_draw_calls(atom.batch_id, timest, draw_update.draw_calls);
    }

    /// Called when scroll changes. Moves content up or down. Nothing more.
    async fn redraw_scroll(&self, batch_id: BatchGuardId) {
        let timest = unixtime();
        let rect = self.rect.get();

        let phone_sel_instrs = self.regen_phone_select_handle_mesh().await;

        let mut content_instrs = vec![DrawInstruction::ApplyView(rect.with_zero_pos())];
        let mut bg_instrs = self.regen_bg_mesh();
        content_instrs.append(&mut bg_instrs);
        content_instrs.push(DrawInstruction::Move(self.behave.scroll()));

        let draw_main = vec![(
            self.content_dc_key,
            DrawCall::new(
                content_instrs,
                vec![
                    self.text_dc_key,
                    self.phone_select_handle_dc_key,
                    self.cursor_dc_key,
                    self.select_dc_key,
                ],
                0,
                "chatedit_content",
            ),
        )];
        self.render_api.replace_draw_calls(batch_id, timest, draw_main);
    }

    async fn redraw_cursor(&self, batch_id: BatchGuardId) {
        let timest = unixtime();
        let instrs = self.get_cursor_instrs().await;
        let draw_calls = vec![(self.cursor_dc_key, DrawCall::new(instrs, vec![], 2, "curs_redr"))];
        self.render_api.replace_draw_calls(batch_id, timest, draw_calls);
    }

    async fn redraw_select(&self, batch_id: BatchGuardId) {
        let timest = unixtime();
        let sel_instrs = self.regen_select_mesh().await;
        let phone_sel_instrs = self.regen_phone_select_handle_mesh().await;
        let draw_calls = vec![
            (self.select_dc_key, DrawCall::new(sel_instrs, vec![], 0, "chatedit_sel")),
            (
                self.phone_select_handle_dc_key,
                DrawCall::new(phone_sel_instrs, vec![], 1, "chatedit_phone_sel_redraw_sel"),
            ),
        ];
        self.render_api.replace_draw_calls(batch_id, timest, draw_calls);
    }

    async fn get_cursor_instrs(&self) -> Vec<DrawInstruction> {
        if !self.is_focused.get() ||
            !self.cursor_is_visible.load(Ordering::Relaxed) ||
            self.hide_cursor.load(Ordering::Relaxed)
        {
            return vec![]
        }

        let cursor_mesh =
            self.cursor_mesh.lock().get_or_insert_with(|| self.regen_cursor_mesh()).clone();

        vec![DrawInstruction::Move(self.get_cursor_pos().await), DrawInstruction::Draw(cursor_mesh)]
    }

    fn regen_bg_mesh(&self) -> Vec<DrawInstruction> {
        if !self.debug.get() {
            return vec![]
        }

        let mut rect = self.rect.get().with_zero_pos();

        let mut mesh = MeshBuilder::new(gfxtag!("chatedit_bg"));
        mesh.draw_outline(&rect, [0., 1., 0., 1.], 1.);

        let pad_top = self.padding.get_f32(0).unwrap();
        let pad_right = self.padding.get_f32(1).unwrap();
        let pad_bottom = self.padding.get_f32(2).unwrap();
        let pad_left = self.padding.get_f32(3).unwrap();

        rect.x = pad_left;
        rect.y = pad_top;
        rect.w -= pad_left + pad_right;
        rect.h -= pad_top + pad_bottom;
        mesh.draw_outline(&rect, [0., 1., 0., 0.5], 1.);

        vec![DrawInstruction::Draw(mesh.alloc(&self.render_api).draw_untextured())]
    }

    async fn regen_txt_mesh(&self) -> Vec<DrawInstruction> {
        let mut instrs = vec![DrawInstruction::Move(self.behave.inner_pos())];

        let editor = self.lock_editor().await;
        let layout = editor.layout();

        let mut render_instrs =
            text2::render_layout(layout, &self.render_api, gfxtag!("chatedit_txt_mesh"));
        instrs.append(&mut render_instrs);

        instrs
    }

    async fn regen_select_mesh(&self) -> Vec<DrawInstruction> {
        let mut instrs = vec![DrawInstruction::Move(self.behave.inner_pos())];

        let editor = self.lock_editor().await;
        let layout = editor.layout();

        let sel = editor.selection(1);
        let sel_color = self.hi_bg_color.get();
        if !sel.is_collapsed() {
            let mut mesh = MeshBuilder::new(gfxtag!("chatedit_select_mesh"));
            sel.geometry_with(layout, |rect: parley::BoundingBox, _| {
                mesh.draw_filled_box(&rect.into(), sel_color);
            });

            instrs.push(DrawInstruction::Draw(mesh.alloc(&self.render_api).draw_untextured()));
        }

        instrs
    }

    async fn regen_phone_select_handle_mesh(&self) -> Vec<DrawInstruction> {
        if !self.is_phone_select.load(Ordering::Acquire) {
            //t!("regen_phone_select_handle_mesh() [DISABLED]");
            return vec![]
        }
        let editor = self.lock_editor().await;
        let (first, last) = self.get_select_handles(&editor).await.unwrap();

        let sel = editor.selection(1);
        assert!(!sel.is_collapsed());

        // We could cache this and use Move instead but why bother?
        let mut mesh = MeshBuilder::new(gfxtag!("chatedit_phone_select_handle"));
        self.draw_phone_select_handle(&mut mesh, first, -1.);
        self.draw_phone_select_handle(&mut mesh, last, 1.);
        vec![DrawInstruction::Draw(mesh.alloc(&self.render_api).draw_untextured())]
    }

    async fn make_draw_calls(&self, _trace_id: u32, atom: &mut PropertyAtomicGuard) -> DrawUpdate {
        self.behave.eval_rect(atom).await;
        let rect = self.rect.get();

        let cursor_instrs = self.get_cursor_instrs().await;
        let txt_instrs = self.regen_txt_mesh().await;
        let sel_instrs = self.regen_select_mesh().await;
        let phone_sel_instrs = self.regen_phone_select_handle_mesh().await;

        let mut content_instrs = vec![DrawInstruction::ApplyView(rect.with_zero_pos())];
        let mut bg_instrs = self.regen_bg_mesh();
        content_instrs.append(&mut bg_instrs);
        content_instrs.push(DrawInstruction::Move(self.behave.scroll()));

        // + root (move)
        // -+ content (apply view)
        //  └╴select
        //  └╴text
        //  └╴phone_handle
        //  └╴cursor

        // Why do we have such a complicated layout?
        // When adjusting selection, it's slow to redraw everything, so the selection
        // must be drawn separately.
        // Lastly the cursor is blinking and that's on top but with clipping.

        DrawUpdate {
            key: self.root_dc_key,
            draw_calls: vec![
                (
                    self.root_dc_key,
                    DrawCall::new(
                        vec![DrawInstruction::Move(rect.pos())],
                        vec![self.content_dc_key],
                        self.z_index.get(),
                        "chatedit_root",
                    ),
                ),
                (
                    self.content_dc_key,
                    DrawCall::new(
                        content_instrs,
                        vec![
                            self.text_dc_key,
                            self.phone_select_handle_dc_key,
                            self.cursor_dc_key,
                            self.select_dc_key,
                        ],
                        0,
                        "chatedit_content",
                    ),
                ),
                (self.select_dc_key, DrawCall::new(sel_instrs, vec![], 0, "chatedit_sel")),
                (self.text_dc_key, DrawCall::new(txt_instrs, vec![], 1, "chatedit_text")),
                (self.cursor_dc_key, DrawCall::new(cursor_instrs, vec![], 2, "chatedit_curs")),
                (
                    self.phone_select_handle_dc_key,
                    DrawCall::new(phone_sel_instrs, vec![], 1, "chatedit_phone_sel"),
                ),
            ],
        }
    }

    async fn insert(&self, txt: &str, atom: &mut PropertyAtomicGuard) {
        let mut editor = self.lock_editor().await;
        editor.insert(txt, atom).await;
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

        let atom =
            &mut self_.render_api.make_guard(gfxtag!("BaseEdit::process_insert_text_method"));
        self_.insert(&text, atom).await;
        self_.redraw(atom).await;
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

        let editor = self_.lock_editor().await;
        editor.focus();
        true
    }
    async fn process_unfocus_method(me: &Weak<Self>, sub: &MethodCallSub) -> bool {
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

        let editor = self_.lock_editor().await;
        editor.unfocus();
        true
    }

    #[cfg(target_os = "android")]
    async fn handle_android_event(&self, ev: AndroidSuggestEvent) {
        if !self.is_active.get() {
            return
        }
        t!("handle_android_event({ev:?})");

        let atom = &mut self.render_api.make_guard(gfxtag!("BaseEdit::handle_android_event"));
        match ev {
            AndroidSuggestEvent::Init => {
                let mut editor = self.lock_editor().await;
                editor.init();
                // For debugging select, enable these and set a selection in the editor.
                //self.is_phone_select.store(true, Ordering::Relaxed);
                //self.hide_cursor.store(true, Ordering::Relaxed);

                // Debug code if we set text in editor.init()
                //editor.on_buffer_changed(&mut PropertyAtomicGuard::none()).await;
                return
            }
            AndroidSuggestEvent::CreateInputConnect => {
                let mut editor = self.lock_editor().await;
                editor.setup();
            }
            // Destructive text edits
            AndroidSuggestEvent::ComposeRegion { .. } |
            AndroidSuggestEvent::Compose { .. } |
            AndroidSuggestEvent::DeleteSurroundingText { .. } => {
                // Any editing will collapse selections
                self.finish_select(atom);

                let mut editor = self.lock_editor().await;
                editor.on_buffer_changed(atom).await;
                drop(editor);

                self.behave.apply_cursor_scroll(atom).await;
            }
            AndroidSuggestEvent::FinishCompose => {
                let mut editor = self.lock_editor().await;
                editor.on_buffer_changed(atom).await;
            }
        }

        // Only redraw once we have the parent_rect
        // Can happen when we receive an Android event before the canvas is ready
        if self.parent_rect.lock().is_some() {
            self.redraw(atom).await;
        }
    }
}

impl Drop for BaseEdit {
    fn drop(&mut self) {
        let atom = self.render_api.make_guard(gfxtag!("BaseEdit::drop"));
        self.render_api.replace_draw_calls(
            atom.batch_id,
            unixtime(),
            vec![(self.text_dc_key, Default::default())],
        );
    }
}

#[async_trait]
impl UIObject for BaseEdit {
    fn priority(&self) -> u32 {
        self.priority.get()
    }

    fn init(&self) {
        let mut guard = self.editor.lock_blocking();
        assert!(guard.is_none());
        *guard = Some(Editor::new(
            self.text.clone(),
            self.font_size.clone(),
            self.text_color.clone(),
            self.window_scale.clone(),
            self.lineheight.clone(),
        ));
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

        let method_sub = node_ref.subscribe_method_call("unfocus").unwrap();
        let me2 = me.clone();
        let unfocus_task =
            ex.spawn(async move { while Self::process_unfocus_method(&me2, &method_sub).await {} });

        let mut on_modify = OnModify::new(ex.clone(), self.node.clone(), me.clone());
        on_modify.when_change(self.is_focused.prop(), Self::change_focus);

        // When text has been changed.
        // Cursor and selection might be invalidated.
        async fn reset(self_: Arc<BaseEdit>, batch: BatchGuardPtr) {
            let atom = &mut batch.spawn();
            //self_.select_text.set_null(Role::Internal, 0).unwrap();
            self_.scroll.store(0., Ordering::Release);
            self_.redraw(atom).await;
        }
        async fn redraw(self_: Arc<BaseEdit>, batch: BatchGuardPtr) {
            let atom = &mut batch.spawn();
            self_.redraw(atom).await;
        }
        async fn set_text(self_: Arc<BaseEdit>, batch: BatchGuardPtr) {
            self_.lock_editor().await.on_text_prop_changed().await;
            let atom = &mut batch.spawn();
            self_.redraw(atom).await;
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
        on_modify.when_change(self.text.prop(), reset);
        on_modify.when_change(self.text_color.prop(), redraw);
        on_modify.when_change(self.hi_bg_color.prop(), redraw);
        //on_modify.when_change(selected.clone(), redraw);
        on_modify.when_change(self.z_index.prop(), redraw);
        on_modify.when_change(self.debug.prop(), redraw);

        async fn regen_cursor(self_: Arc<BaseEdit>, _batch: BatchGuardPtr) {
            // Free the cache
            *self_.cursor_mesh.lock() = None;
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
                let atom = &mut self_.render_api.make_guard(gfxtag!("BaseEdit::start"));
                self_.redraw_cursor(atom.batch_id).await;
            }
        });

        let (sel_sender, sel_recvr) = async_channel::unbounded();
        *self.sel_sender.lock() = Some(sel_sender);
        let me2 = me.clone();
        // We don't get continuous mouse move events. Instead this task is used to smoothly
        // scroll when selecting text.
        let sel_task = ex.spawn(async move {
            let mut scroll_stat = None;
            // Too much code duplication here but I didn't find a solution that looks any cleaner.
            loop {
                if scroll_stat.is_some() {
                    futures::select! {
                        rcv = sel_recvr.recv().fuse() => {
                            scroll_stat = rcv.unwrap();

                            if let Some((mouse_pos, side)) = scroll_stat {
                                let self_ = me2.upgrade().unwrap();
                                //t!("select task interrupt: {mouse_pos:?} (side={side:?})");
                                self_.handle_select(mouse_pos, side).await;
                            };
                        }
                        _ = msleep(SELECT_TASK_UPDATE_TIME).fuse() => {
                            if let Some((mouse_pos, side)) = scroll_stat {
                                let self_ = me2.upgrade().unwrap();
                                //t!("select task update: {mouse_pos:?} (side={side:?})");
                                self_.handle_select(mouse_pos, side).await;
                            };
                        }
                    }
                } else {
                    scroll_stat = sel_recvr.recv().await.unwrap();

                    if let Some((mouse_pos, side)) = scroll_stat {
                        let self_ = me2.upgrade().unwrap();
                        //t!("select task wake up: {mouse_pos:?} (side={side:?})");
                        self_.handle_select(mouse_pos, side).await;
                    };
                }
            }
        });

        let mut tasks =
            vec![insert_text_task, focus_task, unfocus_task, blinking_cursor_task, sel_task];
        tasks.append(&mut on_modify.tasks);

        #[cfg(target_os = "android")]
        {
            let recvr = self.lock_editor().await.recvr.clone();
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

        *self.tasks.lock() = tasks;
    }

    fn stop(&self) {
        self.tasks.lock().clear();
        *self.parent_rect.lock() = None;
        self.key_repeat.lock().clear();
        *self.cursor_mesh.lock() = None;
        *self.editor.lock_blocking() = None;
    }

    async fn draw(
        &self,
        parent_rect: Rectangle,
        trace_id: u32,
        atom: &mut PropertyAtomicGuard,
    ) -> Option<DrawUpdate> {
        t!("BaseEdit::draw({:?}, {trace_id})", self.node());
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

        let atom = &mut self.render_api.make_guard(gfxtag!("BaseEdit::handle_char"));

        if mods.ctrl || mods.alt || mods.logo {
            if repeat {
                return false
            }
            return self.handle_shortcut(key, &mods, atom).await
        }

        // Do nothing
        if actions == 0 {
            return true
        }

        t!("Key {:?} has {} actions", key, actions);
        let key_str = key.to_string().repeat(actions as usize);
        self.insert(&key_str, atom).await;
        self.behave.apply_cursor_scroll(atom).await;
        self.redraw(atom).await;
        true
    }

    async fn handle_key_down(&self, key: KeyCode, mods: KeyMods, repeat: bool) -> bool {
        t!("handle_key_down({key:?}, {mods:?}, {repeat})");
        // First filter for only single digit keys
        // Avoid processing events handled by handle_char()
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

        let atom = &mut self.render_api.make_guard(gfxtag!("BaseEdit::handle_key_down"));

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
                    self.node().trigger("paste_request", vec![]).await.unwrap();
                }
                return true
            }
            return false
        }

        if !rect.contains(mouse_pos) {
            return false
        }

        let atom = &mut self.render_api.make_guard(gfxtag!("BaseEdit::handle_mouse_btn_down"));

        // clicking inside box will:
        // 1. make it active
        // 2. begin selection
        if self.is_focused.get() {
            d!("BaseEdit clicked");
        } else {
            d!("BaseEdit focused");
            self.is_focused.set(atom, true);
        }

        // Move mouse pos within this widget
        self.abs_to_local(&mut mouse_pos);

        {
            let mut txt_ctx = text2::TEXT_CTX.get().await;
            let mut editor = self.lock_editor().await;
            let mut drv = editor.driver(&mut txt_ctx).unwrap();
            drv.move_to_point(mouse_pos.x, mouse_pos.y);
        }

        if !self.select_text.is_null(0).unwrap() {
            self.select_text.clone().set_null(atom, Role::Internal, 0).unwrap();
        }

        self.mouse_btn_held.store(true, Ordering::Relaxed);

        self.pause_blinking();
        self.redraw(atom).await;
        true
    }

    async fn handle_mouse_btn_up(&self, _btn: MouseButton, _mouse_pos: Point) -> bool {
        if !self.is_active.get() {
            return false
        }

        // Stop any selection scrolling
        let scroll_sender = self.sel_sender.lock().clone().unwrap();
        scroll_sender.send(None).await.unwrap();

        // releasing mouse button will end selection
        self.mouse_btn_held.store(false, Ordering::Relaxed);
        false
    }

    async fn handle_mouse_move(&self, mut mouse_pos: Point) -> bool {
        if !self.is_active.get() {
            return false
        }

        let rect = self.rect.get();
        let is_mouse_hover = rect.contains(mouse_pos);
        self.is_mouse_hover.store(is_mouse_hover, Ordering::Relaxed);

        if !self.mouse_btn_held.load(Ordering::Relaxed) {
            return false
        }

        let sel_sender = self.sel_sender.lock().clone().unwrap();
        // Mouse is outside rect?
        // If so we gotta scroll it while selecting.
        if !is_mouse_hover {
            // This process will begin selecting text and applying scroll too.
            sel_sender.send(Some((mouse_pos, None))).await.unwrap();
        } else {
            // Stop any existing select/scroll process
            sel_sender.send(None).await.unwrap();
            // Mouse is inside so just select the text once and be done.
            self.handle_select(mouse_pos, None).await;
        }

        true
    }

    async fn handle_mouse_wheel(&self, wheel_pos: Point) -> bool {
        if !self.is_mouse_hover.load(Ordering::Relaxed) {
            return false
        }

        let atom = &mut self.render_api.make_guard(gfxtag!("BaseEdit::handle_mouse_wheel"));

        let mut scroll =
            self.scroll.load(Ordering::Relaxed) - wheel_pos.y * self.scroll_speed.get();
        scroll = scroll.clamp(0., self.behave.max_scroll().await);
        t!("handle_mouse_wheel({wheel_pos:?}) [scroll={scroll}]");
        self.scroll.store(scroll, Ordering::Release);
        self.redraw_scroll(atom.batch_id).await;

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

        let atom = &mut self.render_api.make_guard(gfxtag!("BaseEdit::handle_touch"));

        match phase {
            TouchPhase::Started => self.handle_touch_start(touch_pos).await,
            TouchPhase::Moved => self.handle_touch_move(touch_pos).await,
            TouchPhase::Ended => self.handle_touch_end(atom, touch_pos).await,
            TouchPhase::Cancelled => false,
        }
    }
}
