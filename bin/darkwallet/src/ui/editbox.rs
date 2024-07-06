use miniquad::{window, BufferId, KeyCode, KeyMods, TextureId};
use rand::{rngs::OsRng, Rng};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex as SyncMutex, Weak},
    time::Instant,
};

use crate::{
    error::Result,
    gfx2::{
        DrawCall, DrawInstruction, DrawMesh, GraphicsEventPublisherPtr, Rectangle, RenderApi,
        RenderApiPtr, Vertex,
    },
    mesh::{Color, MeshBuilder, MeshInfo, COLOR_BLUE, COLOR_WHITE},
    prop::{
        PropertyBool, PropertyColor, PropertyFloat32, PropertyPtr, PropertyStr, PropertyUint32,
    },
    pubsub::Subscription,
    scene::{Pimpl, SceneGraph, SceneGraphPtr2, SceneNodeId},
    text2::{self, Glyph, GlyphPositionIter, RenderedAtlas, SpritePtr, TextShaper, TextShaperPtr},
    util::zip3,
};

use super::{eval_rect, get_parent_rect, read_rect, DrawUpdate, OnModify, Stoppable};

const CURSOR_WIDTH: f32 = 4.;

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
    texture_id: TextureId,
}

pub type EditBoxPtr = Arc<EditBox>;

pub struct EditBox {
    node_id: SceneNodeId,
    tasks: Vec<smol::Task<()>>,
    sg: SceneGraphPtr2,
    render_api: RenderApiPtr,
    // So we can lock the event stream when we gain focus
    event_pub: GraphicsEventPublisherPtr,
    text_shaper: TextShaperPtr,
    key_repeat: SyncMutex<PressedKeysSmoothRepeat>,

    render_info: SyncMutex<Option<TextRenderInfo>>,
    glyphs: SyncMutex<Vec<Glyph>>,
    dc_key: u64,

    is_active: PropertyBool,
    rect: PropertyPtr,
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
}

impl EditBox {
    pub async fn new(
        ex: Arc<smol::Executor<'static>>,
        sg: SceneGraphPtr2,
        node_id: SceneNodeId,
        render_api: RenderApiPtr,
        event_pub: GraphicsEventPublisherPtr,
        text_shaper: TextShaperPtr,
    ) -> Pimpl {
        let scene_graph = sg.lock().await;
        let node = scene_graph.get_node(node_id).unwrap();
        let is_active = PropertyBool::wrap(node, "is_active", 0).unwrap();
        let rect = node.get_property("rect").expect("EditBox::rect");
        let baseline = PropertyFloat32::wrap(node, "baseline", 0).unwrap();
        let scroll = PropertyFloat32::wrap(node, "scroll", 0).unwrap();
        let cursor_pos = PropertyUint32::wrap(node, "cursor_pos", 0).unwrap();
        let font_size = PropertyFloat32::wrap(node, "font_size", 0).unwrap();
        let text = PropertyStr::wrap(node, "text", 0).unwrap();
        let text_color = PropertyColor::wrap(node, "text_color").unwrap();
        let cursor_color = PropertyColor::wrap(node, "cursor_color").unwrap();
        let hi_bg_color = PropertyColor::wrap(node, "hi_bg_color").unwrap();
        let selected = node.get_property("selected").unwrap();
        let z_index = PropertyUint32::wrap(node, "z_index", 0).unwrap();
        let debug = PropertyBool::wrap(node, "debug", 0).unwrap();
        drop(scene_graph);

        // testing
        window::show_keyboard(true);

        let glyphs = text_shaper.shape(text.get(), font_size.get()).await;

        let self_ = Arc::new_cyclic(|me: &Weak<Self>| {
            // Start a task monitoring for key down events
            let ev_sub = event_pub.subscribe_char();
            let me2 = me.clone();
            let char_task = ex.spawn(async move {
                loop {
                    Self::process_char(&me2, &ev_sub).await;
                }
            });

            let ev_sub = event_pub.subscribe_key_down();
            let me2 = me.clone();
            let key_down_task = ex.spawn(async move {
                loop {
                    Self::process_key_down(&me2, &ev_sub).await;
                }
            });

            /*
            let ev_sub = event_pub.subscribe_key_up();
            let me2 = me.clone();
            let key_up_task = ex.spawn(async move {
                loop {
                    let Ok((key, mods)) = ev_sub.receive().await else {
                        debug!(target: "ui::editbox", "Event relayer closed");
                        break
                    };

                    let Some(self_) = me2.upgrade() else {
                        // Should not happen
                        panic!("self destroyed before key_up_task was stopped!");
                    };

                    let key = PressedKey::Key(key);
                    let mut repeater = self_.key_repeat.lock().unwrap();
                    repeater.key_up(&key);
                }
            });
            */

            // on modify tasks too
            let tasks = vec![char_task, key_down_task];

            Self {
                node_id,
                tasks,
                sg,
                render_api,
                event_pub,
                text_shaper,
                key_repeat: SyncMutex::new(PressedKeysSmoothRepeat::new(400, 50)),

                render_info: SyncMutex::new(None),
                glyphs: SyncMutex::new(glyphs),
                dc_key: OsRng.gen(),

                is_active,
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
            }
        });

        Pimpl::EditBox(self_)
    }

    /// Called whenever the text or any text property changes.
    /// Not related to cursor, text highlighting or bounding (clip) rects.
    async fn regen_mesh(&self, mut clip: Rectangle) -> TextRenderInfo {
        clip.x = 0.;
        clip.y = 0.;

        let text = self.text.get();
        let font_size = self.font_size.get();
        let text_color = self.text_color.get();
        let baseline = self.baseline.get();
        let scroll = self.scroll.get();
        let cursor_pos = self.cursor_pos.get() as usize;
        let cursor_color = self.cursor_color.get();
        let debug = self.debug.get();
        debug!(target: "ui::editbox", "Rendering text '{}' clip={:?}", text, clip);

        let glyphs = self.glyphs.lock().unwrap().clone();
        let atlas = text2::make_texture_atlas(&self.render_api, font_size, &glyphs).await.unwrap();

        let mut mesh = MeshBuilder::with_clip(clip.clone());
        self.draw_selected(&mut mesh, &glyphs, clip.h).unwrap();

        let mut glyph_pos_iter = GlyphPositionIter::new(font_size, &glyphs, baseline);
        // Used for drawing the cursor when it's at the end of the line.
        let mut rhs = 0.;

        for (glyph_idx, uv_rect, mut glyph_rect, glyph) in
            zip3(atlas.uv_rects.into_iter(), glyph_pos_iter, glyphs.iter())
        {
            glyph_rect.x += scroll;

            //mesh.draw_outline(&glyph_rect, COLOR_BLUE, 2.);
            let mut color = text_color.clone();
            if glyph.sprite.has_color {
                color = COLOR_WHITE;
            }
            mesh.draw_box(&glyph_rect, color, &uv_rect);

            if cursor_pos != 0 && cursor_pos == glyph_idx {
                let cursor_rect =
                    Rectangle { x: glyph_rect.x - CURSOR_WIDTH, y: 0., w: CURSOR_WIDTH, h: clip.h };
                mesh.draw_box(&cursor_rect, cursor_color, &Rectangle::zero());
            }

            rhs = glyph_rect.rhs();
        }

        if cursor_pos == 0 {
            let cursor_rect = Rectangle { x: 0., y: 0., w: CURSOR_WIDTH, h: clip.h };
            mesh.draw_box(&cursor_rect, cursor_color, &Rectangle::zero());
        } else if cursor_pos == glyphs.len() {
            let cursor_rect =
                Rectangle { x: rhs - CURSOR_WIDTH, y: 0., w: CURSOR_WIDTH, h: clip.h };
            mesh.draw_box(&cursor_rect, cursor_color, &Rectangle::zero());
        }

        if debug {
            mesh.draw_outline(&clip, COLOR_BLUE, 1.);
        }

        let mesh = mesh.alloc(&self.render_api).await.unwrap();

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

        let sel_start = std::cmp::min(start, end);
        let sel_end = std::cmp::max(start, end);

        let font_size = self.font_size.get();
        let baseline = self.baseline.get();
        let scroll = self.scroll.get();
        let hi_bg_color = self.hi_bg_color.get();
        let mut glyph_pos_iter = GlyphPositionIter::new(font_size, &glyphs, baseline);

        let mut start_x = 0.;
        let mut end_x = 0.;
        // When cursor lands at the end of the line
        let mut rhs = 0.;

        for (glyph_idx, mut glyph_rect) in glyph_pos_iter.enumerate() {
            glyph_rect.x += scroll;

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
            end_x = rhs;
        }

        // We don't need to do manual clipping since MeshBuilder should do that
        let select_rect = Rectangle { x: start_x, y: 0., w: end_x - start_x, h: clip_h };
        mesh.draw_box(&select_rect, hi_bg_color, &Rectangle::zero());
        Ok(())
    }

    async fn do_key_action(&self, key: char, mods: &KeyMods) {
        match key {
            //KeyCode::Left => {}
            _ => self.insert_char(key).await,
        }
    }

    async fn process_char(me: &Weak<Self>, ev_sub: &Subscription<(char, KeyMods, bool)>) {
        let Ok((key, mods, repeat)) = ev_sub.receive().await else {
            debug!(target: "ui::editbox", "Event relayer closed");
            return
        };

        // First filter for only single digit keys
        if DISALLOWED_CHARS.contains(&key) {
            return
        }

        let Some(self_) = me.upgrade() else {
            // Should not happen
            panic!("self destroyed before char_task was stopped!");
        };

        if mods.ctrl || mods.alt {
            if repeat {
                return
            }
            self_.handle_shortcut(key, &mods).await;
            return
        }

        let actions = {
            let mut repeater = self_.key_repeat.lock().unwrap();
            repeater.key_down(PressedKey::Char(key), repeat)
        };
        debug!(target: "ui::editbox", "Key {:?} has {} actions", key, actions);
        for _ in 0..actions {
            self_.insert_char(key).await;
        }
    }

    async fn process_key_down(me: &Weak<Self>, ev_sub: &Subscription<(KeyCode, KeyMods, bool)>) {
        let Ok((key, mods, repeat)) = ev_sub.receive().await else {
            debug!(target: "ui::editbox", "Event relayer closed");
            return
        };

        // First filter for only single digit keys
        // Avoid processing events handled by insert_char()
        if !ALLOWED_KEYCODES.contains(&key) {
            return
        }

        let Some(self_) = me.upgrade() else {
            // Should not happen
            panic!("self destroyed before char_task was stopped!");
        };

        let actions = {
            let mut repeater = self_.key_repeat.lock().unwrap();
            repeater.key_down(PressedKey::Key(key), repeat)
        };
        // Suppress noisy message
        if actions > 0 {
            debug!(target: "ui::editbox", "Key {:?} has {} actions", key, actions);
        }
        for _ in 0..actions {
            self_.handle_key(&key, &mods).await;
        }
    }

    async fn insert_char(&self, key: char) {
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
                    self.selected.set_null(0).unwrap();
                    self.selected.set_null(1).unwrap();
                } else if self.selected.is_null(0).unwrap() {
                    assert!(self.selected.is_null(1).unwrap());
                    self.selected.set_u32(0, cursor_pos).unwrap();
                }

                if cursor_pos > 0 {
                    cursor_pos -= 1;
                    debug!(target: "ui::editbox", "Left cursor_pos={}", cursor_pos);
                    self.cursor_pos.set(cursor_pos);
                }

                // Update selection
                if mods.shift {
                    self.selected.set_u32(1, cursor_pos).unwrap();
                }

                self.apply_cursor_scrolling();
                self.redraw().await;
            }
            KeyCode::Right => {
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
                    debug!(target: "ui::editbox", "Right cursor_pos={}", cursor_pos);
                    self.cursor_pos.set(cursor_pos);
                }

                // Update selection
                if mods.shift {
                    self.selected.set_u32(1, cursor_pos).unwrap();
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

        self.selected.set_null(0).unwrap();
        self.selected.set_null(1).unwrap();
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

    /// Beware of this method. Here be dragons.
    /// Possibly racy so we limit it just to cursor scrolling.
    fn cached_rect(&self) -> Rectangle {
        let Ok(rect) = read_rect(self.rect.clone()) else {
            panic!("Node bad rect property");
        };
        rect
    }

    /// Whenever the cursor property is modified this MUST be called
    /// to recalculate the scroll x property.
    fn apply_cursor_scrolling(&self) {
        // This may need updating but yolo rite
        let rect = self.cached_rect();

        let cursor_pos = self.cursor_pos.get() as usize;
        let mut scroll = self.scroll.get();

        let cursor_x = {
            let font_size = self.font_size.get();
            let baseline = self.baseline.get();
            let glyphs = &*self.glyphs.lock().unwrap();

            let mut glyph_pos_iter = GlyphPositionIter::new(font_size, &glyphs, baseline);

            if cursor_pos == 0 {
                0.
            } else if cursor_pos == glyphs.len() {
                let glyph_pos = glyph_pos_iter.last().unwrap();
                glyph_pos.rhs()
            } else {
                assert!(cursor_pos < glyphs.len());
                let glyph_pos = glyph_pos_iter.nth(cursor_pos).expect("glyph pos mismatch glyphs");
                glyph_pos.x
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

    async fn redraw(&self) {
        let old = self.render_info.lock().unwrap().clone();

        let glyphs = self.text_shaper.shape(self.text.get(), self.font_size.get()).await;
        *self.glyphs.lock().unwrap() = glyphs;

        // draw will recalc this when it's None
        *self.render_info.lock().unwrap() = None;

        let sg = self.sg.lock().await;
        let node = sg.get_node(self.node_id).unwrap();

        let Some(parent_rect) = get_parent_rect(&sg, node) else {
            return;
        };

        let Some(draw_update) = self.draw(&sg, &parent_rect).await else {
            error!(target: "ui::editbox", "Text {:?} failed to draw", node);
            return;
        };
        self.render_api.replace_draw_calls(draw_update.draw_calls).await;
        debug!(target: "ui::editbox", "replace draw calls done");

        // We're finished with these so clean up.
        if let Some(old) = old {
            self.render_api.delete_buffer(old.mesh.vertex_buffer);
            self.render_api.delete_buffer(old.mesh.index_buffer);
            self.render_api.delete_texture(old.texture_id);
        }
    }

    pub async fn draw(&self, sg: &SceneGraph, parent_rect: &Rectangle) -> Option<DrawUpdate> {
        debug!(target: "ui::editbox", "EditBox::draw()");
        // Only used for debug messages
        let node = sg.get_node(self.node_id).unwrap();

        if let Err(err) = eval_rect(self.rect.clone(), parent_rect) {
            panic!("Node {:?} bad rect property: {}", node, err);
        }

        let Ok(mut rect) = read_rect(self.rect.clone()) else {
            panic!("Node {:?} bad rect property", node);
        };

        rect.x += parent_rect.x;
        rect.y += parent_rect.y;

        let render_info = self.render_info.lock().unwrap().clone();
        let render_info = match render_info {
            Some(render_info) => render_info,
            None => {
                let render_info = self.regen_mesh(rect.clone()).await;
                *self.render_info.lock().unwrap() = Some(render_info.clone());
                render_info
            }
        };

        let mesh = DrawMesh {
            vertex_buffer: render_info.mesh.vertex_buffer,
            index_buffer: render_info.mesh.index_buffer,
            texture: Some(render_info.texture_id),
            num_elements: render_info.mesh.num_elements,
        };

        let off_x = rect.x / parent_rect.w;
        let off_y = rect.y / parent_rect.h;
        let scale_x = 1. / parent_rect.w;
        let scale_y = 1. / parent_rect.h;
        let model = glam::Mat4::from_translation(glam::Vec3::new(off_x, off_y, 0.)) *
            glam::Mat4::from_scale(glam::Vec3::new(scale_x, scale_y, 1.));

        Some(DrawUpdate {
            key: self.dc_key,
            draw_calls: vec![(
                self.dc_key,
                DrawCall {
                    instrs: vec![DrawInstruction::ApplyMatrix(model), DrawInstruction::Draw(mesh)],
                    dcs: vec![],
                    z_index: self.z_index.get(),
                },
            )],
        })
    }

    async fn send_event(&self) {
        let text = self.text.get();
        debug!(target: "ui::editbox", "sending text {}", text);

        // This should probably be unset instead
        self.text.set(String::new());
        self.cursor_pos.set(0);
        self.redraw().await;
    }
}

impl Stoppable for EditBox {
    async fn stop(&self) {
        // TODO: Delete own draw call

        // Free buffers
        // Should this be in drop?
        //self.render_api.delete_buffer(self.vertex_buffer);
        //self.render_api.delete_buffer(self.index_buffer);
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
];
