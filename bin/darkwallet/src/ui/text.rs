//use async_lock::Mutex;
use miniquad::{BufferId, TextureId};
use rand::{rngs::OsRng, Rng};
use std::sync::{Arc, Mutex as SyncMutex, Weak};

use crate::{
    gfx2::{DrawCall, DrawInstruction, DrawMesh, Rectangle, RenderApi, RenderApiPtr, Vertex},
    mesh::{Color, MeshBuilder, MeshInfo, COLOR_BLUE, COLOR_WHITE},
    prop::{
        PropertyBool, PropertyColor, PropertyFloat32, PropertyPtr, PropertyStr, PropertyUint32,
    },
    scene::{Pimpl, SceneGraph, SceneGraphPtr2, SceneNodeId},
    text2::{self, Glyph, GlyphPositionIter, RenderedAtlas, SpritePtr, TextShaper, TextShaperPtr},
};

use super::{eval_rect, get_parent_rect, read_rect, DrawUpdate, OnModify, Stoppable};

pub type TextPtr = Arc<Text>;

#[derive(Clone)]
struct TextRenderInfo {
    glyph_sprites: Vec<SpritePtr>,
    mesh: MeshInfo,
    texture_id: TextureId,
}

pub struct Text {
    sg: SceneGraphPtr2,
    render_api: RenderApiPtr,
    text_shaper: TextShaperPtr,
    tasks: Vec<smol::Task<()>>,

    render_info: SyncMutex<TextRenderInfo>,
    dc_key: u64,

    node_id: SceneNodeId,
    rect: PropertyPtr,
    z_index: PropertyUint32,
    text: PropertyStr,
    font_size: PropertyFloat32,
    text_color: PropertyColor,
    baseline: PropertyFloat32,
    debug: PropertyBool,
}

impl Text {
    pub async fn new(
        ex: Arc<smol::Executor<'static>>,
        sg: SceneGraphPtr2,
        node_id: SceneNodeId,
        render_api: RenderApiPtr,
        text_shaper: TextShaperPtr,
    ) -> Pimpl {
        let scene_graph = sg.lock().await;
        let node = scene_graph.get_node(node_id).unwrap();
        let node_name = node.name.clone();
        let rect = node.get_property("rect").expect("Text::rect");
        let z_index = PropertyUint32::wrap(node, "z_index", 0).unwrap();
        let text = PropertyStr::wrap(node, "text", 0).unwrap();
        let font_size = PropertyFloat32::wrap(node, "font_size", 0).unwrap();
        let text_color = PropertyColor::wrap(node, "text_color").unwrap();
        let baseline = PropertyFloat32::wrap(node, "baseline", 0).unwrap();
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

        let self_ = Arc::new_cyclic(|me: &Weak<Self>| {
            let mut on_modify = OnModify::new(ex, node_name, node_id, me.clone());
            on_modify.when_change(rect.clone(), Self::redraw);
            on_modify.when_change(z_index.prop(), Self::redraw);
            on_modify.when_change(text.prop(), Self::redraw);
            on_modify.when_change(font_size.prop(), Self::redraw);
            on_modify.when_change(text_color.prop(), Self::redraw);
            on_modify.when_change(debug.prop(), Self::redraw);
            on_modify.when_change(baseline.prop(), Self::redraw);

            Self {
                sg,
                render_api,
                text_shaper,
                tasks: on_modify.tasks,
                render_info: SyncMutex::new(render_info),
                dc_key: OsRng.gen(),
                node_id,
                rect,
                z_index,
                text,
                font_size,
                text_color,
                baseline,
                debug,
            }
        });

        Pimpl::Text(self_)
    }

    async fn regen_mesh(
        render_api: &RenderApi,
        text_shaper: &TextShaper,
        text: String,
        font_size: f32,
        text_color: Color,
        baseline: f32,
        debug: bool,
    ) -> TextRenderInfo {
        debug!(target: "ui::text", "Rendering label '{}'", text);
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

        let glyph_sprites = glyphs.into_iter().map(|glyph| glyph.sprite).collect();

        TextRenderInfo { glyph_sprites, mesh, texture_id: atlas.texture_id }
    }

    async fn redraw(self: Arc<Self>) {
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
            error!(target: "ui::text", "Text {:?} failed to draw", node);
            return;
        };
        self.render_api.replace_draw_calls(draw_update.draw_calls).await;
        debug!(target: "ui::text", "replace draw calls done");

        // We're finished with these so clean up.
        self.render_api.delete_buffer(old.mesh.vertex_buffer);
        self.render_api.delete_buffer(old.mesh.index_buffer);
        self.render_api.delete_texture(old.texture_id);
    }

    pub async fn draw(&self, sg: &SceneGraph, parent_rect: &Rectangle) -> Option<DrawUpdate> {
        debug!(target: "ui::text", "Text::draw()");
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

impl Stoppable for Text {
    async fn stop(&self) {
        // TODO: Delete own draw call

        // Free buffers
        // Should this be in drop?
        let render_info = self.render_info.lock().unwrap().clone();
        let vertex_buffer = render_info.mesh.vertex_buffer;
        let index_buffer = render_info.mesh.index_buffer;
        let texture_id = render_info.texture_id;
        self.render_api.delete_buffer(vertex_buffer);
        self.render_api.delete_buffer(index_buffer);
        self.render_api.delete_texture(texture_id);
    }
}
