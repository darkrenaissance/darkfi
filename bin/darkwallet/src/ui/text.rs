use async_lock::Mutex;
use miniquad::{BufferId, TextureId};
use rand::{rngs::OsRng, Rng};
use std::sync::{Arc, Weak};

use crate::{
    gfx2::{DrawCall, DrawInstruction, DrawMesh, Rectangle, RenderApiPtr, Vertex},
    mesh::{MeshBuilder, COLOR_BLUE, COLOR_WHITE},
    prop::{PropertyPtr, PropertyUint32},
    scene::{Pimpl, SceneGraph, SceneGraphPtr2, SceneNodeId},
    text2::{self, Glyph, GlyphPositionIter, RenderedAtlas, SpritePtr, TextShaperPtr},
};

use super::{eval_rect, get_parent_rect, read_rect, DrawUpdate, OnModify, Stoppable};

pub type TextPtr = Arc<Text>;

pub struct Text {
    sg: SceneGraphPtr2,
    render_api: RenderApiPtr,
    text_shaper: TextShaperPtr,
    tasks: Vec<smol::Task<()>>,

    glyph_sprites: Mutex<Vec<SpritePtr>>,

    texture_id: TextureId,
    vertex_buffer: BufferId,
    index_buffer: BufferId,
    num_elements: i32,
    dc_key: u64,

    node_id: SceneNodeId,
    rect: PropertyPtr,
    z_index: PropertyUint32,
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
        let z_index_prop = node.get_property("z_index").expect("Text::z_index");
        let z_index = PropertyUint32::from(z_index_prop.clone(), 0).unwrap();
        let text = node.get_property("text").expect("Text::text");
        let font_size = node.get_property("font_size").expect("Text::font_size");
        let color = node.get_property("color").expect("Text::color");
        let debug = node.get_property("debug").expect("Text::debug");
        let baseline = node.get_property("baseline").expect("Text::baseline");
        drop(scene_graph);

        let text_str = text.get_str(0).unwrap();
        let font_size_val = font_size.get_f32(0).unwrap();
        debug!(target: "ui::text", "Rendering label '{}'", text_str);
        let glyphs = text_shaper.shape(text_str, font_size_val).await;
        let atlas =
            text2::make_texture_atlas(render_api.clone(), font_size_val, &glyphs).await.unwrap();

        let (x1, y1) = (0., 0.);
        let (x2, y2) = (1., 1.);
        let verts = vec![
            // top left
            Vertex { pos: [x1, y1], color: [1., 0., 0., 1.], uv: [0., 0.] },
            // top right
            Vertex { pos: [x2, y1], color: [1., 0., 1., 1.], uv: [1., 0.] },
            // bottom left
            Vertex { pos: [x1, y2], color: [0., 0., 1., 1.], uv: [0., 1.] },
            // bottom right
            Vertex { pos: [x2, y2], color: [1., 1., 0., 1.], uv: [1., 1.] },
        ];
        let indices = vec![0, 2, 1, 1, 2, 3];
        assert_eq!(atlas.uv_rects.len(), glyphs.len());

        let baseline_y = baseline.get_f32(0).unwrap();

        let mut mesh = MeshBuilder::new();
        let mut glyph_pos_iter = GlyphPositionIter::new(font_size_val, &glyphs, baseline_y);
        for (uv_rect, glyph_rect) in atlas.uv_rects.into_iter().zip(glyph_pos_iter) {
            mesh.draw_box(&glyph_rect, COLOR_WHITE, &uv_rect);
            //mesh.draw_outline(&rect, COLOR_BLUE, 2.);
        }

        let num_elements = mesh.num_elements();
        let (vertex_buffer, index_buffer) = mesh.alloc(&render_api).await.unwrap();

        let sprites = glyphs.into_iter().map(|glyph| glyph.sprite).collect();

        let self_ = Arc::new_cyclic(|me: &Weak<Self>| {
            let mut on_modify = OnModify::new(ex, node_name, node_id, me.clone());
            on_modify.when_change(rect.clone(), Self::redraw);
            on_modify.when_change(z_index_prop, Self::redraw);
            on_modify.when_change(text, Self::redraw);
            on_modify.when_change(font_size, Self::redraw);
            on_modify.when_change(color, Self::redraw);
            on_modify.when_change(debug, Self::redraw);
            on_modify.when_change(baseline, Self::redraw);

            Self {
                sg,
                render_api,
                text_shaper,
                tasks: on_modify.tasks,
                //glyphs: Mutex::new((glyphs, atlas)),
                glyph_sprites: Mutex::new(sprites),
                texture_id: atlas.texture_id,
                vertex_buffer,
                index_buffer,
                num_elements,
                dc_key: OsRng.gen(),
                node_id,
                rect,
                z_index,
            }
        });

        Pimpl::Text(self_)
    }

    async fn redraw(self: Arc<Self>) {
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
    }

    pub async fn draw(&self, sg: &SceneGraph, parent_rect: &Rectangle) -> Option<DrawUpdate> {
        debug!(target: "ui::text", "Text::draw()");
        // Only used for debug messages
        let node = sg.get_node(self.node_id).unwrap();

        let mesh = DrawMesh {
            vertex_buffer: self.vertex_buffer,
            index_buffer: self.index_buffer,
            texture: Some(self.texture_id),
            num_elements: self.num_elements,
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
        //self.render_api.delete_buffer(self.vertex_buffer);
        //self.render_api.delete_buffer(self.index_buffer);
    }
}
