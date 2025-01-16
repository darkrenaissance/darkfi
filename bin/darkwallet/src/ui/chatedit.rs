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
        PropertyBool, PropertyColor, PropertyFloat32, PropertyPtr, PropertyRect, PropertyStr,
        PropertyUint32, Role,
    },
    pubsub::Subscription,
    scene::{MethodCallSub, Pimpl, SceneNodePtr, SceneNodeWeak},
    text::{self, Glyph, GlyphPositionIter, TextShaperPtr},
    util::{enumerate_ref, is_whitespace, min_f32, unixtime, zip4, Clipboard},
    ExecutorPtr,
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

macro_rules! d { ($($arg:tt)*) => { debug!(target: "ui::chatview", $($arg)*); } }

fn is_all_whitespace(glyphs: &[Glyph]) -> bool {
    for glyph in glyphs {
        if !is_whitespace(&glyph.substr) {
            return false
        }
    }
    true
}

struct TextWrap {
    editable: Editable,
    select: Vec<Selection>,
    rendered: Option<RenderedEditable>,

    font_size: PropertyFloat32,
    window_scale: PropertyFloat32,
    baseline: PropertyFloat32,
    linespacing: PropertyFloat32,
}

impl TextWrap {
    fn new(
        text_shaper: TextShaperPtr,
        font_size: PropertyFloat32,
        window_scale: PropertyFloat32,
        baseline: PropertyFloat32,
        linespacing: PropertyFloat32,
    ) -> Self {
        Self {
            editable: Editable::new(
                text_shaper,
                font_size.clone(),
                window_scale.clone(),
                baseline.clone(),
            ),
            select: vec![],
            rendered: None,
            font_size,
            window_scale,
            baseline,
            linespacing,
        }
    }

    fn clear_cache(&mut self) {
        self.rendered = None;
    }
    fn get_render(&mut self) -> &RenderedEditable {
        if self.rendered.is_none() {
            //debug!(target: "ui::chatedit::text_wrap", "Regenerating render cache");
            self.rendered = Some(self.editable.render());
        }
        self.rendered.as_ref().unwrap()
    }

    fn wrap(&mut self, width: f32) -> WrappedLines {
        let font_size = self.font_size.get();
        let window_scale = self.window_scale.get();
        let baseline = self.baseline.get();
        let linespacing = self.linespacing.get();

        let rendered = self.get_render();
        let wrapped_glyphs = text::wrap(width, font_size, window_scale, &rendered.glyphs);

        let mut curr_pos = 0;
        let mut lines: Vec<_> = wrapped_glyphs
            .into_iter()
            .map(|glyphs| {
                let off_pos = curr_pos;
                curr_pos += glyphs.len();
                WrappedLine::new(glyphs, off_pos, font_size, window_scale, baseline)
            })
            .collect();

        if let Some(last) = lines.last_mut() {
            assert_eq!(last.last_pos(), rendered.glyphs.len());
            last.is_last = true;
        }

        WrappedLines::new(lines, font_size, linespacing)
    }

    fn cursor_pos(&mut self) -> TextPos {
        let rendered = self.get_render().clone();
        self.editable.get_cursor_pos(&rendered)
    }

    fn set_cursor_with_point(&mut self, point: Point, width: f32) -> TextPos {
        let wrapped_lines = self.wrap(width);
        let cursor_pos = wrapped_lines.point_to_pos(point);

        let rendered = self.get_render();
        let glyphs_len = rendered.glyphs.len();
        let cidx = rendered.pos_to_idx(cursor_pos);
        self.editable.set_cursor_idx(cidx);

        //debug!(target: "ui::chatedit::text_wrap", "set_cursor_with_point() -> {cursor_pos} / {glyphs_len}");
        cursor_pos
    }

    fn get_word_boundary(&mut self, pos: TextPos) -> (TextPos, TextPos) {
        let rendered = self.get_render();
        let final_pos = rendered.glyphs.len();

        // Find word start
        let mut pos_start = pos;
        while pos_start > 0 {
            // Is the glyph before this pos just whitespace?
            let glyph_str = &rendered.glyphs[pos_start - 1].substr;
            if is_whitespace(glyph_str) {
                break
            }
            pos_start -= 1;
        }

        // Find word end
        let mut pos_end = std::cmp::min(pos + 1, final_pos);
        while pos_end < final_pos {
            let glyph_str = &rendered.glyphs[pos_end].substr;
            if is_whitespace(glyph_str) {
                break
            }
            pos_end += 1;
        }

        (pos_start, pos_end)
    }

    fn delete_selected(&mut self) {
        let selection = std::mem::take(&mut self.select);

        let sel = selection.first().unwrap();
        let cursor_pos = std::cmp::min(sel.start, sel.end);

        let render = self.get_render();
        let mut before_text = String::new();
        let mut after_text = String::new();
        'next: for (pos, glyph) in render.glyphs.iter().enumerate() {
            for select in &selection {
                let start = std::cmp::min(select.start, select.end);
                let end = std::cmp::max(select.start, select.end);

                if start <= pos && pos < end {
                    continue 'next
                }
            }
            if pos <= cursor_pos {
                before_text += &glyph.substr;
            } else {
                after_text += &glyph.substr;
            }
        }
        self.editable.end_compose();
        self.editable.set_text(before_text, after_text);
        self.clear_cache();
    }
}

struct WrappedLine {
    glyphs: Vec<Glyph>,
    off_pos: TextPos,

    font_size: f32,
    window_scale: f32,
    baseline: f32,

    /// Last line in this paragraph?
    /// In which case the cursor does not wrap
    is_last: bool,
}

impl WrappedLine {
    fn new(
        glyphs: Vec<Glyph>,
        off_pos: TextPos,
        font_size: f32,
        window_scale: f32,
        baseline: f32,
    ) -> Self {
        Self { glyphs, off_pos, font_size, window_scale, baseline, is_last: false }
    }

    fn len(&self) -> usize {
        self.glyphs.len()
    }

    fn first_pos(&self) -> TextPos {
        self.off_pos
    }
    fn last_pos(&self) -> TextPos {
        self.off_pos + self.len()
    }

    fn pos_iter(&self) -> GlyphPositionIter {
        GlyphPositionIter::new(self.font_size, self.window_scale, &self.glyphs, self.baseline)
    }

    fn rhs(&self) -> f32 {
        match self.pos_iter().last() {
            None => 0.,
            Some(rect) => rect.rhs(),
        }
    }

    fn find_closest(&self, x: f32) -> TextPos {
        for (pos, glyph_rect) in self.pos_iter().enumerate() {
            let next_x = glyph_rect.center().x;
            if x < next_x {
                return pos
            }
        }

        if self.is_last {
            self.glyphs.len()
        } else {
            self.glyphs.len() - 1
        }
    }
}

struct WrappedLines {
    lines: Vec<WrappedLine>,
    font_size: f32,
    linespacing: f32,
}

impl WrappedLines {
    fn new(lines: Vec<WrappedLine>, font_size: f32, linespacing: f32) -> Self {
        Self { lines, font_size, linespacing }
    }

    /// Convert an (x, y) point to a glyph pos
    fn point_to_pos(&self, mut point: Point) -> TextPos {
        if self.lines.is_empty() {
            return 0
        }
        let mut pos = 0;
        for (line_idx, wrap_line) in self.lines.iter().enumerate() {
            // Is it within this line?
            if point.y < self.linespacing || wrap_line.is_last {
                //debug!(target: "ui::chatedit::wrapped_lines", "point to pos found line: {line_idx}");
                pos += wrap_line.find_closest(point.x);
                return pos
            }

            // Continue to the next line
            point.y -= self.linespacing;
            pos += wrap_line.len();
        }
        //debug!(target: "ui::chatedit::wrapped_lines", "point to pos using last line");
        panic!("point_to_pos() went past the last line")
    }

    fn height(&self) -> f32 {
        self.last_y() + self.linespacing
    }

    fn last_y(&self) -> f32 {
        if self.lines.is_empty() {
            return 0.
        }
        (self.lines.len() - 1) as f32 * self.linespacing
    }

    fn last_rhs(&self) -> f32 {
        if self.lines.is_empty() {
            return 0.
        }
        let last_line = self.lines.last().unwrap();
        let mut rhs = last_line.rhs();
        rhs += eol_nudge(self.font_size, &last_line.glyphs);
        rhs
    }

    fn get_glyph_info(&self, mut pos: TextPos) -> (Rectangle, usize) {
        let mut y = 0.;
        for (line_idx, wrap_line) in self.lines.iter().enumerate() {
            assert!(!wrap_line.glyphs.is_empty());

            if pos < wrap_line.len() {
                // Cursor is on this line
                let mut pos_iter = wrap_line.pos_iter();
                pos_iter.advance_by(pos).unwrap();

                let mut glyph_rect = pos_iter.next().unwrap();
                return (glyph_rect, line_idx)
            }

            pos -= wrap_line.len();
            y += self.linespacing;
        }

        let rhs = self.last_rhs();
        let last_y = self.last_y();
        let final_rect = Rectangle::new(rhs, last_y, 0., self.linespacing);
        let last_idx = if self.lines.is_empty() { 0 } else { self.lines.len() - 1 };
        (final_rect, last_idx)
    }
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
    max_height: PropertyFloat32,
    rect: PropertyRect,
    baseline: PropertyFloat32,
    linespacing: PropertyFloat32,
    descent: PropertyFloat32,
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

    text_wrap: SyncMutex<TextWrap>,

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
}

impl ChatEdit {
    pub async fn new(
        node: SceneNodeWeak,
        window_scale: PropertyFloat32,
        render_api: RenderApi,
        text_shaper: TextShaperPtr,
        ex: ExecutorPtr,
    ) -> Pimpl {
        debug!(target: "ui::chatedit", "ChatEdit::new()");

        let node_ref = &node.upgrade().unwrap();
        let is_active = PropertyBool::wrap(node_ref, Role::Internal, "is_active", 0).unwrap();
        let is_focused = PropertyBool::wrap(node_ref, Role::Internal, "is_focused", 0).unwrap();
        let max_height = PropertyFloat32::wrap(node_ref, Role::Internal, "max_height", 0).unwrap();
        let rect = PropertyRect::wrap(node_ref, Role::Internal, "rect").unwrap();
        let baseline = PropertyFloat32::wrap(node_ref, Role::Internal, "baseline", 0).unwrap();
        let linespacing =
            PropertyFloat32::wrap(node_ref, Role::Internal, "linespacing", 0).unwrap();
        let descent = PropertyFloat32::wrap(node_ref, Role::Internal, "descent", 0).unwrap();
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
            max_height,
            rect,
            baseline: baseline.clone(),
            linespacing: linespacing.clone(),
            descent,
            scroll: scroll.clone(),
            scroll_speed,
            padding,
            cursor_pos,
            font_size: font_size.clone(),
            text,
            text_color,
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

            text_wrap: SyncMutex::new(TextWrap::new(
                text_shaper,
                font_size,
                window_scale.clone(),
                baseline,
                linespacing,
            )),

            mouse_btn_held: AtomicBool::new(false),
            cursor_is_visible: AtomicBool::new(true),
            blink_is_paused: AtomicBool::new(false),
            hide_cursor: AtomicBool::new(false),

            touch_info: SyncMutex::new(TouchInfo::new(scroll)),
            is_phone_select: AtomicBool::new(false),

            old_window_scale: AtomicF32::new(window_scale.get()),
            window_scale,
            parent_rect: SyncMutex::new(None),
            is_mouse_hover: AtomicBool::new(false),
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
        //self_
        //    .text_wrap
        //    .lock()
        //    .editable
        //    .set_text("A berry is small and pulpy.".to_string(), "".to_string());

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

    /// Called whenever the text or any text property changes.
    fn regen_text_mesh(&self) -> GfxDrawMesh {
        let is_focused = self.is_focused.get();
        let text = self.text.get();
        let font_size = self.font_size.get();
        let window_scale = self.window_scale.get();
        let text_color = self.text_color.get();
        let text_hi_color = self.text_hi_color.get();
        let text_cmd_color = self.text_cmd_color.get();
        let linespacing = self.linespacing.get();
        let baseline = self.baseline.get();
        let scroll = self.scroll.get();
        let cursor_color = self.cursor_color.get();
        let debug = self.debug.get();

        let parent_rect = self.parent_rect.lock().clone().unwrap();
        self.rect.eval_with(
            vec![2],
            vec![("parent_w".to_string(), parent_rect.w), ("parent_h".to_string(), parent_rect.h)],
        );
        // Height is calculated from width based on wrapping
        let width = self.wrap_width();

        let (atlas, wrapped_lines, selections, under_start, under_end) = {
            let mut text_wrap = self.text_wrap.lock();
            // Must happen after rect eval, which is inside regen_text_mesh
            // Maybe we should take the eval out of here.
            self.clamp_scroll(&mut text_wrap);

            let rendered = text_wrap.get_render();
            let under_start = rendered.under_start;
            let under_end = rendered.under_end;
            let atlas = text::make_texture_atlas(&self.render_api, &rendered.glyphs);
            let wrapped_lines = text_wrap.wrap(width);
            let selections = text_wrap.select.clone();
            (atlas, wrapped_lines, selections, under_start, under_end)
        };

        let mut height = wrapped_lines.height() + self.descent.get();
        height = height.clamp(0., self.max_height.get());

        self.rect.prop().set_f32(Role::Internal, 3, height);

        // Eval the rect
        let parent_rect = self.parent_rect.lock().clone().unwrap();
        self.rect.eval_with(
            vec![0, 1, 3],
            vec![
                ("parent_w".to_string(), parent_rect.w),
                ("parent_h".to_string(), parent_rect.h),
                ("rect_h".to_string(), height),
            ],
        );

        let mut clip = self.rect.get();
        //debug!(target: "ui::chatedit", "Rendering text '{text}' rect={clip:?} width={width}");
        clip.x = 0.;
        clip.y = 0.;

        let mut mesh = MeshBuilder::with_clip(clip.clone());
        let mut curr_y = -scroll;

        //debug!(target: "ui::chatedit", "regen_text_mesh() selections={selections:?}");

        for (line_idx, wrap_line) in wrapped_lines.lines.iter().enumerate() {
            // Instead of bools, maybe we should have a GlyphStyle enum
            let select_marks = self.mark_selected_glyphs(&wrap_line, &selections);
            let hi_bg_color = self.hi_bg_color.get();
            self.draw_text_bg_box(&mut mesh, &select_marks, &wrap_line, curr_y, hi_bg_color);

            let cmd_marks = if line_idx == 0 {
                self.mark_command_glyphs(&wrap_line)
            } else {
                vec![false; wrap_line.len()]
            };
            let cmd_bg_color = self.cmd_bg_color.get();
            self.draw_text_bg_box(&mut mesh, &cmd_marks, &wrap_line, curr_y, cmd_bg_color);

            if under_start != under_end {
                assert!(under_start < under_end);
                self.draw_underline(&mut mesh, &wrap_line, curr_y, under_start, under_end);
            }

            let pos_iter = wrap_line.pos_iter();

            for (_, mut glyph_rect, glyph, is_selected, is_cmd) in zip4(
                pos_iter,
                wrap_line.glyphs.iter(),
                select_marks.into_iter(),
                cmd_marks.into_iter(),
            ) {
                let uv_rect = atlas.fetch_uv(glyph.glyph_id).expect("missing glyph UV rect");

                glyph_rect.y += curr_y;

                //mesh.draw_outline(&glyph_rect, COLOR_BLUE, 2.);
                let mut color = text_color.clone();
                if is_selected {
                    color = text_hi_color.clone();
                } else if is_cmd {
                    color = text_cmd_color.clone();
                }

                if glyph.sprite.has_color {
                    color = COLOR_WHITE;
                }

                mesh.draw_box(&glyph_rect, color, uv_rect);
                if self.debug.get() {
                    mesh.draw_outline(&glyph_rect, COLOR_RED, 1.);
                }
            }

            curr_y += linespacing;
        }

        // Just an assert
        if self.is_phone_select.load(Ordering::Relaxed) {
            assert_eq!(selections.len(), 1);
            let select = selections.last().unwrap();
            self.draw_phone_select_handle(&mut mesh, select.start, &wrapped_lines, -1.);
            self.draw_phone_select_handle(&mut mesh, select.end, &wrapped_lines, 1.);
        }

        if self.debug.get() {
            mesh.draw_outline(&clip, COLOR_BLUE, 1.);
        }

        mesh.alloc(&self.render_api).draw_with_texture(atlas.texture)
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

    fn get_cursor_pos(&self) -> Point {
        assert!(self.is_focused.get());

        let font_size = self.font_size.get();
        let window_scale = self.window_scale.get();
        let linespacing = self.linespacing.get();
        let scroll = self.scroll.get();

        let width = self.wrap_width();

        let (cursor_pos, wrapped_lines) = {
            let mut text_wrap = self.text_wrap.lock();
            (text_wrap.cursor_pos(), text_wrap.wrap(width))
        };

        let glyph_info = wrapped_lines.get_glyph_info(cursor_pos);
        let lineidx = glyph_info.1;
        let mut point = glyph_info.0.pos();
        point.x = point.x.clamp(0., width);
        point.y = lineidx as f32 * linespacing - scroll;
        point
    }

    fn mark_selected_glyphs(
        &self,
        wrap_line: &WrappedLine,
        selections: &Vec<Selection>,
    ) -> Vec<bool> {
        let mut marks = vec![false; wrap_line.len()];
        for select in selections {
            let mut start = std::cmp::min(select.start, select.end);
            let mut end = std::cmp::max(select.start, select.end);

            let off_pos = wrap_line.off_pos;

            if end < off_pos || start > wrap_line.last_pos() {
                continue
            }

            end = std::cmp::min(end, wrap_line.last_pos()) - off_pos;
            start = start.saturating_sub(off_pos);

            for i in start..end {
                marks[i] = true;
            }
        }
        marks
    }

    fn mark_command_glyphs(&self, wrap_line: &WrappedLine) -> Vec<bool> {
        let mut state = ColoringState::Start;
        let mut marks = vec![false; wrap_line.len()];
        for (idx, glyph) in wrap_line.glyphs.iter().enumerate() {
            match state {
                ColoringState::Start => {
                    if glyph.substr == "/" {
                        state = ColoringState::IsCommand
                    } else {
                        state = ColoringState::Normal
                    }
                }
                ColoringState::IsCommand => {
                    if is_whitespace(&glyph.substr) {
                        state = ColoringState::Normal
                    }
                }
                _ => {}
            }

            match state {
                ColoringState::IsCommand => marks[idx] = true,
                _ => {}
            }
        }
        marks
    }

    fn draw_text_bg_box(
        &self,
        mesh: &mut MeshBuilder,
        select_marks: &Vec<bool>,
        wrap_line: &WrappedLine,
        y_off: f32,
        color: Color,
    ) {
        let font_size = self.font_size.get();
        let baseline = self.baseline.get();
        let select_ascent = self.select_ascent.get();
        let select_descent = self.select_descent.get();

        if select_marks.iter().all(|b| !b) {
            return
        }

        let mut start_x = None;
        let mut end_x = None;

        for (glyph_rect, is_selected) in wrap_line.pos_iter().zip(select_marks.iter()) {
            if *is_selected && start_x.is_none() {
                start_x = Some(glyph_rect.x);
            }
            if !*is_selected && start_x.is_some() && end_x.is_none() {
                end_x = Some(glyph_rect.x);
            }
        }

        let mut start_x = start_x.unwrap();

        if select_marks[0] {
            start_x = 0.;
        }

        let end_x = match end_x {
            Some(end_x) => end_x,
            None => {
                assert!(*select_marks.last().unwrap());
                wrap_line.rhs() + eol_nudge(font_size, &wrap_line.glyphs)
            }
        };

        let select_ascent = self.select_ascent.get();
        let select_descent = self.select_descent.get();

        let select_rect = Rectangle {
            x: start_x,
            y: y_off + baseline - select_ascent,
            w: end_x - start_x,
            h: select_ascent + select_descent,
        };
        mesh.draw_box(&select_rect, color, &Rectangle::zero());
    }

    fn draw_underline(
        &self,
        mesh: &mut MeshBuilder,
        wrap_line: &WrappedLine,
        y_off: f32,
        under_start: usize,
        under_end: usize,
    ) {
        if under_start >= wrap_line.last_pos() {
            return
        }
        if under_end < wrap_line.first_pos() {
            return
        }
        assert!(under_start < under_end);

        let baseline = self.baseline.get();
        let text_color = self.text_color.get();

        // Doing it like this but with advance should be easier and shorter.
        //let start = start as isize - self.start_pos as isize;
        //let end = end as isize - self.start_pos as isize;
        //let line_start = std::cmp::max(0, start) as usize;
        //assert!(end > 0);
        //let line_end = std::cmp::min(self.marks.len() as isize, end) as usize;

        let mut start_x = 0.;
        let mut end_x = 0.;
        // When cursor lands at the end of the line
        let mut rhs = 0.;

        for (glyph_idx, mut glyph_rect) in wrap_line.pos_iter().enumerate() {
            if glyph_idx == under_start {
                start_x = glyph_rect.x;
            }
            if glyph_idx == under_end {
                end_x = glyph_rect.x;
            }

            rhs = glyph_rect.rhs();
        }

        if under_start == 0 {
            start_x = 0.;
        }

        if under_end == wrap_line.last_pos() {
            end_x = rhs;
        }

        // We don't need to do manual clipping since MeshBuilder should do that
        let underline_rect =
            Rectangle { x: start_x, y: y_off + baseline + 6., w: end_x - start_x, h: 4. };
        mesh.draw_box(&underline_rect, text_color, &Rectangle::zero());
    }

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

    async fn change_focus(self: Arc<Self>) {
        if !self.is_active.get() {
            return
        }
        debug!(target: "ui::chatedit", "Focus changed");

        // Cursor visibility will change so just redraw everything lol
        self.redraw().await;
    }

    async fn insert_char(&self, key: char) {
        debug!(target: "ui::chatedit", "insert_char({key})");
        let mut tmp = [0; 4];
        let key_str = key.encode_utf8(&mut tmp);
        self.insert_text(key_str).await
    }
    async fn insert_text(&self, text: &str) {
        //debug!(target: "ui::chatedit", "insert_text({text})");
        let text = {
            let mut text_wrap = &mut self.text_wrap.lock();
            text_wrap.clear_cache();
            if !text_wrap.select.is_empty() {
                text_wrap.delete_selected();

                self.update_select_text(&mut text_wrap);

                self.is_phone_select.store(false, Ordering::Relaxed);
                // Reshow cursor (if hidden)
                self.hide_cursor.store(false, Ordering::Relaxed);
            }
            text_wrap.editable.compose(text, true);
            text_wrap.editable.get_text()
        };
        self.text.set(text);

        self.pause_blinking();
        self.redraw().await;
    }

    async fn handle_shortcut(&self, key: char, mods: &KeyMods) -> bool {
        debug!(target: "ui::chatedit", "handle_shortcut({:?}, {:?})", key, mods);

        match key {
            'a' => {
                if mods.ctrl {
                    {
                        let mut text_wrap = self.text_wrap.lock();
                        let rendered = text_wrap.get_render();
                        let end_pos = rendered.glyphs.len();

                        let select = &mut text_wrap.select;
                        select.clear();
                        select.push(Selection::new(0, end_pos));

                        self.update_select_text(&mut text_wrap);
                    }

                    self.redraw().await;
                    return true
                }
            }
            'c' => {
                if mods.ctrl {
                    //self.copy_highlighted().unwrap();
                    return true
                }
            }
            'v' => {
                if mods.ctrl {
                    let mut clip = Clipboard::new();
                    if let Some(text) = clip.get() {
                        self.insert_text(&text).await;
                    }
                    return true
                }
            }
            _ => {}
        }
        false
    }

    async fn handle_key(&self, key: &KeyCode, mods: &KeyMods) -> bool {
        debug!(target: "ui::chatedit", "handle_key({:?}, {:?})", key, mods);
        match key {
            KeyCode::Left => {
                if !self.adjust_cursor(&mods, |editable| editable.move_cursor(-1)) {
                    return false
                }
                self.pause_blinking();
                //self.apply_cursor_scrolling();
                self.redraw().await;
                return true
            }
            KeyCode::Right => {
                if !self.adjust_cursor(&mods, |editable| editable.move_cursor(1)) {
                    return false
                }
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
                self.delete(0, 1);
                self.clamp_scroll(&mut self.text_wrap.lock());
                self.pause_blinking();
                self.redraw().await;
                return true
            }
            KeyCode::Backspace => {
                self.delete(1, 0);
                self.clamp_scroll(&mut self.text_wrap.lock());
                self.pause_blinking();
                self.redraw().await;
                return true
            }
            KeyCode::Home => {
                self.adjust_cursor(&mods, |editable| editable.move_start());
                self.pause_blinking();
                //self.apply_cursor_scrolling();
                self.redraw().await;
                return true
            }
            KeyCode::End => {
                self.adjust_cursor(&mods, |editable| editable.move_end());
                self.pause_blinking();
                //self.apply_cursor_scrolling();
                self.redraw().await;
                return true
            }
            _ => {}
        }
        false
    }

    fn delete(&self, before: usize, after: usize) {
        let mut text_wrap = &mut self.text_wrap.lock();
        if text_wrap.select.is_empty() {
            text_wrap.editable.delete(before, after);
            text_wrap.clear_cache();
        } else {
            text_wrap.delete_selected();
            self.update_select_text(&mut text_wrap);
        }

        self.is_phone_select.store(false, Ordering::Relaxed);
        // Reshow cursor (if hidden)
        self.hide_cursor.store(false, Ordering::Relaxed);

        let text = text_wrap.editable.get_text();
        self.text.set(text);
    }

    fn adjust_cursor(&self, mods: &KeyMods, move_cursor: impl Fn(&mut Editable)) -> bool {
        if mods.ctrl || mods.alt || mods.logo {
            return false
        }

        let mut text_wrap = &mut self.text_wrap.lock();
        let rendered = text_wrap.get_render().clone();
        let prev_cursor_pos = text_wrap.editable.get_cursor_pos(&rendered);
        move_cursor(&mut text_wrap.editable);
        let cursor_pos = text_wrap.editable.get_cursor_pos(&rendered);
        debug!(target: "ui::editbox", "Adjust cursor pos to {cursor_pos}");

        let select = &mut text_wrap.select;

        // Start selection if shift is held
        if mods.shift {
            // Create a new selection
            if select.is_empty() {
                select.push(Selection::new(prev_cursor_pos, cursor_pos));
            }

            // Update the selection
            select.last_mut().unwrap().end = cursor_pos;
        } else {
            select.clear();
        }

        self.update_select_text(&mut text_wrap);
        true
    }

    /// This will select the entire word rather than move the cursor to that location
    fn start_touch_select(&self, touch_pos: Point) {
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

        debug!(target: "ui::chatview", "Selected {select:?} from {touch_pos:?}");
        self.update_select_text(&mut text_wrap);
    }

    /// Call this whenever the selection changes to update the external property
    fn update_select_text(&self, text_wrap: &mut TextWrap) {
        let select = &text_wrap.select;
        let Some(select) = select.first().cloned() else {
            self.select_text.set_null(Role::Internal, 0).unwrap();
            return
        };

        let start = std::cmp::min(select.start, select.end);
        let end = std::cmp::max(select.start, select.end);

        let rendered = text_wrap.get_render();
        let glyphs = &rendered.glyphs[start..end];
        let text = text::glyph_str(glyphs);
        self.select_text.set_str(Role::Internal, 0, text).unwrap();
    }

    /// Call this whenever the cursor pos changes to update the external property
    fn update_cursor_pos(&self, text_wrap: &mut TextWrap) {
        let cursor_off = text_wrap.editable.get_text_before().len() as u32;
        self.cursor_pos.set(cursor_off);
    }

    /*
    fn copy_highlighted(&self) -> Result<()> {
        let start = self.selected.get_u32(0)? as usize;
        let end = self.selected.get_u32(1)? as usize;

        let sel_start = std::cmp::min(start, end);
        let sel_end = std::cmp::max(start, end);

        let mut text = String::new();

        let glyphs = self.glyphs.lock().clone();
        for (glyph_idx, glyph) in glyphs.iter().enumerate() {
            if sel_start <= glyph_idx && glyph_idx < sel_end {
                text.push_str(&glyph.substr);
            }
        }

        info!(target: "ui::chatedit", "Copied '{}'", text);
        window::clipboard_set(&text);
        Ok(())
    }

    async fn paste_text(&self, key: String) {
        let mut text = String::new();

        let cursor_pos = self.cursor_pos.get();

        if cursor_pos == 0 {
            text = key.clone();
        }

        let glyphs = self.glyphs.lock().clone();
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
    */

    async fn handle_touch_start(&self, mut touch_pos: Point) -> bool {
        //debug!(target: "ui::chatedit", "handle_touch_start({touch_pos:?})");
        let mut touch_info = self.touch_info.lock();

        if self.try_handle_drag(&mut touch_info, touch_pos) {
            return true
        }

        let rect = self.rect.get();
        if !rect.contains(touch_pos) {
            return false
        }

        touch_info.start(touch_pos);
        true
    }
    fn try_handle_drag(&self, touch_info: &mut TouchInfo, mut touch_pos: Point) -> bool {
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
            //debug!(target: "ui::chatedit", "handle center points = ({p1:?}, {p2:?})");

            const TOUCH_RADIUS_SQ: f32 = 10_000.;

            if p1.dist_sq(&touch_pos) <= TOUCH_RADIUS_SQ {
                debug!(target: "ui::chatedit::touch", "start touch: DragSelectHandle state [side=-1]");
                // Set touch_state status to enable begin dragging them
                touch_info.state = TouchStateAction::DragSelectHandle { side: -1 };
                return true;
            }
            if p2.dist_sq(&touch_pos) <= TOUCH_RADIUS_SQ {
                debug!(target: "ui::chatedit::touch", "start touch: DragSelectHandle state [side=1]");
                // Set touch_state status to enable begin dragging them
                touch_info.state = TouchStateAction::DragSelectHandle { side: 1 };
                return true;
            }
        }

        false
    }

    async fn handle_touch_move(&self, mut touch_pos: Point) -> bool {
        //debug!(target: "ui::chatedit", "handle_touch_move({touch_pos:?})");
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
                    self.start_touch_select(touch_pos);
                    self.redraw().await;
                }
                debug!(target: "ui::chatedit::touch", "touch state: StartSelect -> Select");
                self.touch_info.lock().state = TouchStateAction::Select;
            }
            TouchStateAction::DragSelectHandle { side } => {
                self.abs_to_local(&mut touch_pos);
                {
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

                    self.update_select_text(&mut text_wrap);
                }
                self.redraw().await;
            }
            TouchStateAction::ScrollVert { start_pos, scroll_start } => {
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
                self.scroll.set(scroll);
                self.redraw().await;
            }
            TouchStateAction::SetCursorPos => {
                // TBH I can't even see the cursor under my thumb so I'll just
                // comment this for now.
                //self.abs_to_local(&mut touch_pos);
                //self.touch_set_cursor_pos(touch_pos).await
            }
            _ => {}
        }
        true
    }
    async fn handle_touch_end(&self, mut touch_pos: Point) -> bool {
        //debug!(target: "ui::chatedit", "handle_touch_end({touch_pos:?})");
        self.abs_to_local(&mut touch_pos);

        let state = self.touch_info.lock().stop();
        match state {
            TouchStateAction::Inactive => return false,
            TouchStateAction::Started { pos: _, instant: _ } | TouchStateAction::SetCursorPos => {
                self.touch_set_cursor_pos(touch_pos).await
            }
            _ => {}
        }

        let node = self.node.upgrade().unwrap();
        node.trigger("keyboard_request", vec![]).await.unwrap();

        true
    }

    async fn touch_set_cursor_pos(&self, mut touch_pos: Point) {
        debug!(target: "ui::chatedit", "touch_set_cursor_pos({touch_pos:?})");
        let width = self.wrap_width();
        {
            let mut text_wrap = self.text_wrap.lock();
            let cursor_pos = text_wrap.set_cursor_with_point(touch_pos, width);
            self.update_cursor_pos(&mut text_wrap);

            let select = &mut text_wrap.select;
            let select_is_empty = select.is_empty();
            select.clear();
            if !select_is_empty {
                self.update_select_text(&mut text_wrap);
            }
        }

        self.is_phone_select.store(false, Ordering::Relaxed);
        // Reshow cursor (if hidden)
        self.pause_blinking();
        self.hide_cursor.store(false, Ordering::Relaxed);

        self.redraw().await;
    }

    /// Whenever the cursor property is modified this MUST be called
    /// to recalculate the scroll x property.
    fn apply_cursor_scrolling(&self) {
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

        self.scroll.set(scroll);
    }

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

    /// When we resize the screen, the rect changes so we may need to alter the scroll.
    /// Or if we delete text.
    fn clamp_scroll(&self, text_wrap: &mut TextWrap) {
        let max_scroll = self.max_scroll(text_wrap);
        let mut scroll = self.scroll.get();
        if scroll > max_scroll {
            self.scroll.set(max_scroll);
        }
    }

    fn pause_blinking(&self) {
        self.blink_is_paused.store(true, Ordering::Relaxed);
        self.cursor_is_visible.store(true, Ordering::Relaxed);
    }

    async fn redraw(&self) {
        let timest = unixtime();
        //debug!(target: "ui::chatedit", "redraw()");
        let Some(draw_update) = self.make_draw_calls() else {
            error!(target: "ui::chatedit", "Text failed to draw");
            return;
        };
        self.render_api.replace_draw_calls(timest, draw_update.draw_calls);
    }

    fn redraw_cursor(&self) {
        let timest = unixtime();
        let cursor_instrs = self.get_cursor_instrs();
        let draw_calls = vec![(
            self.cursor_dc_key,
            GfxDrawCall { instrs: cursor_instrs, dcs: vec![], z_index: self.z_index.get() },
        )];
        self.render_api.replace_draw_calls(timest, draw_calls);
    }

    fn get_cursor_instrs(&self) -> Vec<GfxDrawInstruction> {
        if !self.is_focused.get() ||
            !self.cursor_is_visible.load(Ordering::Relaxed) ||
            self.hide_cursor.load(Ordering::Relaxed)
        {
            return vec![]
        }

        let mut cursor_instrs = vec![];

        let cursor_pos = self.get_cursor_pos();

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

    fn make_draw_calls(&self) -> Option<DrawUpdate> {
        let text_mesh = self.regen_text_mesh();
        let cursor_instrs = self.get_cursor_instrs();

        let rect = self.rect.get();

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
                    GfxDrawCall {
                        instrs: vec![
                            GfxDrawInstruction::Move(rect.pos()),
                            GfxDrawInstruction::Draw(text_mesh),
                        ],
                        dcs: vec![],
                        z_index: self.z_index.get(),
                    },
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

        //debug!(target: "ui::chatview", "method called: insert_line({method_call:?})");
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

        self_.insert_text(&text).await;
        true
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
        let node_name = node_ref.name.clone();
        let node_id = node_ref.id;

        let method_sub = node_ref.subscribe_method_call("insert_text").unwrap();
        let me2 = me.clone();
        let insert_text_task =
            ex.spawn(
                async move { while Self::process_insert_text_method(&me2, &method_sub).await {} },
            );

        let mut on_modify = OnModify::new(ex.clone(), node_name, node_id, me.clone());
        on_modify.when_change(self.is_focused.prop(), Self::change_focus);

        // When text has been changed.
        // Cursor and selection might be invalidated.
        async fn reset(self_: Arc<ChatEdit>) {
            self_.cursor_pos.set(0);
            //self_.select_text.set_null(Role::Internal, 0).unwrap();
            self_.scroll.set(0.);
            self_.redraw();
        }
        async fn redraw(self_: Arc<ChatEdit>) {
            self_.redraw().await;
        }
        async fn set_text(self_: Arc<ChatEdit>) {
            {
                let text = self_.text.get();

                let mut text_wrap = self_.text_wrap.lock();
                text_wrap.editable.end_compose();
                text_wrap.editable.set_text(text, String::new());
                text_wrap.clear_cache();

                let select = &mut text_wrap.select;
                select.clear();
            }

            self_.redraw().await;
        }

        on_modify.when_change(self.rect.prop(), redraw);
        on_modify.when_change(self.baseline.prop(), redraw);
        on_modify.when_change(self.linespacing.prop(), redraw);
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
                self_.redraw_cursor();
            }
        });

        let mut tasks = vec![insert_text_task, blinking_cursor_task];
        tasks.append(&mut on_modify.tasks);
        self.tasks.set(tasks);
    }

    async fn draw(&self, parent_rect: Rectangle) -> Option<DrawUpdate> {
        debug!(target: "ui::chatedit", "ChatEdit::draw({:?})", self.node.upgrade().unwrap());
        *self.parent_rect.lock() = Some(parent_rect);

        self.make_draw_calls()
    }

    async fn handle_char(&self, key: char, mods: KeyMods, repeat: bool) -> bool {
        //debug!(target: "ui::chatedit", "handle_char({key}, {mods:?}, {repeat})");
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

        if mods.ctrl || mods.alt {
            if repeat {
                return false
            }
            return self.handle_shortcut(key, &mods).await
        }

        //debug!(target: "ui::chatedit", "Key {:?} has {} actions", key, actions);
        for _ in 0..actions {
            self.insert_char(key).await;
        }
        true
    }

    async fn handle_key_down(&self, key: KeyCode, mods: KeyMods, repeat: bool) -> bool {
        //debug!(target: "ui::chatedit", "handle_key_down({key:?}, {mods:?}, {repeat})")
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
        //if actions > 0 {
        //    debug!(target: "ui::chatedit", "Key {:?} has {} actions", key, actions);
        //}

        let mut is_handled = false;
        for _ in 0..actions {
            if self.handle_key(&key, &mods).await {
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

        // clicking inside box will:
        // 1. make it active
        // 2. begin selection
        if self.is_focused.get() {
            debug!(target: "ui::chatedit", "ChatEdit clicked");
        } else {
            debug!(target: "ui::chatedit", "ChatEdit focused");
            self.is_focused.set(true);
        }

        // Move mouse pos within this widget
        self.abs_to_local(&mut mouse_pos);

        let width = self.wrap_width();

        {
            let mut text_wrap = self.text_wrap.lock();
            let cursor_pos = text_wrap.set_cursor_with_point(mouse_pos, width);
            self.update_cursor_pos(&mut text_wrap);
            debug!(target: "ui::editbox", "Mouse move cursor pos to {cursor_pos}");

            // begin selection
            let select = &mut text_wrap.select;
            let select_is_empty = select.is_empty();
            select.clear();
            if !select_is_empty {
                self.update_select_text(&mut text_wrap);
            }

            self.mouse_btn_held.store(true, Ordering::Relaxed);
        }

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

        // if active and selection_active, then use x to modify the selection.
        // also implement scrolling when cursor is to the left or right
        // just scroll to the end
        // also set cursor_pos too

        // Move mouse pos within this widget
        self.abs_to_local(&mut mouse_pos);

        let width = self.wrap_width();

        {
            let mut text_wrap = self.text_wrap.lock();
            let cursor_pos = text_wrap.set_cursor_with_point(mouse_pos, width);
            self.update_cursor_pos(&mut text_wrap);

            // modify current selection
            let select = &mut text_wrap.select;
            if select.is_empty() {
                select.push(Selection::new(cursor_pos, cursor_pos));
            }
            select.first_mut().unwrap().end = cursor_pos;
            self.update_select_text(&mut text_wrap);
        }

        self.pause_blinking();
        //self.apply_cursor_scrolling();
        self.redraw().await;
        true
    }

    async fn handle_mouse_wheel(&self, wheel_pos: Point) -> bool {
        //debug!(target: "ui::chatedit", "rect={rect:?}, wheel_pos={wheel_pos:?}");
        if !self.is_mouse_hover.load(Ordering::Relaxed) {
            return false
        }

        let max_scroll = {
            let mut text_wrap = self.text_wrap.lock();
            self.max_scroll(&mut text_wrap)
        };

        let mut scroll = self.scroll.get() - wheel_pos.y * self.scroll_speed.get();
        scroll = scroll.clamp(0., max_scroll);
        debug!(target: "ui::chatedit", "handle_mouse_wheel({wheel_pos:?}) [scroll={scroll}]");
        self.scroll.set(scroll);
        self.redraw().await;

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

    async fn handle_compose_text(&self, suggest_text: &str, is_commit: bool) -> bool {
        debug!(target: "ui::chatedit", "handle_compose_text({suggest_text}, {is_commit})");

        if !self.is_active.get() {
            return false
        }

        let text = {
            let mut text_wrap = self.text_wrap.lock();
            text_wrap.clear_cache();
            text_wrap.editable.compose(suggest_text, is_commit);

            self.clamp_scroll(&mut text_wrap);
            text_wrap.editable.get_text()
        };
        self.text.set(text);

        //self.apply_cursor_scrolling();
        self.redraw().await;

        true
    }
    async fn handle_set_compose_region(&self, start: usize, end: usize) -> bool {
        debug!(target: "ui::chatedit", "handle_set_compose_region({start}, {end})");

        if !self.is_active.get() {
            return false
        }

        {
            let mut text_wrap = self.text_wrap.lock();
            text_wrap.clear_cache();
            text_wrap.editable.set_compose_region(start, end);
        }

        self.redraw().await;

        true
    }
}
