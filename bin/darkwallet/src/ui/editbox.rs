use miniquad::{window, BufferId, KeyCode, KeyMods, TextureId};
use rand::{rngs::OsRng, Rng};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex as SyncMutex, Weak},
    time::Instant,
};

use crate::{
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
};

use super::{eval_rect, get_parent_rect, read_rect, DrawUpdate, OnModify, Stoppable};

// First refactor the event system
// Each event should have its own unique pipe
// Advantages:
// - less overhead when publishing msgs to ppl who dont need them
// - more advanced locking of streams when widgets capture input
// also add capturing and make use of it with editbox.

const CURSOR_WIDTH: f32 = 4.;

#[derive(Debug, Clone, Eq, Hash, PartialEq)]
enum PressedKey {
    Char(char),
    Key(KeyCode),
}

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
    glyphs: Vec<Glyph>,
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

    render_info: SyncMutex<TextRenderInfo>,
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

        let render_info = Self::regen_mesh(
            &render_api,
            &text_shaper,
            text.get(),
            font_size.get(),
            text_color.get(),
            baseline.get(),
            debug.get(),
        )
        .await;

        // testing
        //window::show_keyboard(true);

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

                render_info: SyncMutex::new(render_info),
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
    async fn regen_mesh(
        render_api: &RenderApi,
        text_shaper: &TextShaper,
        text: String,
        font_size: f32,
        text_color: Color,
        baseline: f32,
        debug: bool,
    ) -> TextRenderInfo {
        debug!(target: "ui::editbox", "Rendering text '{}'", text);
        let glyphs = text_shaper.shape(text, font_size).await;
        let atlas = text2::make_texture_atlas(render_api, font_size, &glyphs).await.unwrap();

        let mut mesh = MeshBuilder::new();
        let mut glyph_pos_iter = GlyphPositionIter::new(font_size, &glyphs, baseline);
        for ((uv_rect, glyph_rect), glyph) in
            atlas.uv_rects.into_iter().zip(glyph_pos_iter).zip(glyphs.iter())
        {
            //mesh.draw_outline(&glyph_rect, COLOR_BLUE, 2.);
            let mut color = text_color.clone();
            if glyph.sprite.has_color {
                color = COLOR_WHITE;
            }
            mesh.draw_box(&glyph_rect, color, &uv_rect);
        }

        let mesh = mesh.alloc(&render_api).await.unwrap();

        TextRenderInfo { glyphs, mesh, texture_id: atlas.texture_id }
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
        let disallowed_keys = ['\r', '\u{8}', '\u{7f}', '\t'];
        if disallowed_keys.contains(&key) {
            return
        }

        let Some(self_) = me.upgrade() else {
            // Should not happen
            panic!("self destroyed before char_task was stopped!");
        };

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
        debug!(target: "ui::editbox", "Key {:?} has {} actions", key, actions);
        for _ in 0..actions {
            self_.handle_key(&key, &mods).await;
        }
    }

    async fn insert_char(&self, key: char) {
        let mut text = String::new();

        let cursor_pos = self.cursor_pos.get();

        let glyphs = self.render_info.lock().unwrap().glyphs.clone();

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

    async fn handle_key(&self, key: &KeyCode, mods: &KeyMods) {
        debug!(target: "ui::editbox", "handle_key({:?}, {:?})", key, mods);
    }

    async fn redraw(&self) {
        let old = self.render_info.lock().unwrap().clone();

        let render_info = Self::regen_mesh(
            &self.render_api,
            &self.text_shaper,
            self.text.get(),
            self.font_size.get(),
            self.text_color.get(),
            self.baseline.get(),
            self.debug.get(),
        )
        .await;
        *self.render_info.lock().unwrap() = render_info;

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
        self.render_api.delete_buffer(old.mesh.vertex_buffer);
        self.render_api.delete_buffer(old.mesh.index_buffer);
        self.render_api.delete_texture(old.texture_id);
    }

    pub async fn draw(&self, sg: &SceneGraph, parent_rect: &Rectangle) -> Option<DrawUpdate> {
        debug!(target: "ui::editbox", "EditBox::draw()");
        // Only used for debug messages
        let node = sg.get_node(self.node_id).unwrap();

        let render_info = self.render_info.lock().unwrap().clone();

        let mesh = DrawMesh {
            vertex_buffer: render_info.mesh.vertex_buffer,
            index_buffer: render_info.mesh.index_buffer,
            texture: Some(render_info.texture_id),
            num_elements: render_info.mesh.num_elements,
        };

        if let Err(err) = eval_rect(self.rect.clone(), parent_rect) {
            panic!("Node {:?} bad rect property: {}", node, err);
        }

        let Ok(mut rect) = read_rect(self.rect.clone()) else {
            panic!("Node {:?} bad rect property", node);
        };

        rect.x += parent_rect.x;
        rect.y += parent_rect.x;

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
];