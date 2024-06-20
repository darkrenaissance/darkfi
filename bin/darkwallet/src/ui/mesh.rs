use rand::{rngs::OsRng, Rng};
use std::sync::{Arc, Weak};

use crate::{
    gfx2::{DrawCall, DrawInstruction, DrawMesh, Rectangle, RenderApiPtr, Vertex},
    prop::{PropertyPtr, PropertyUint32},
    scene::{Pimpl, SceneGraph, SceneGraphPtr2, SceneNodeId},
};

use super::{eval_rect, get_parent_rect, read_rect, DrawUpdate, OnModify, Stoppable};

pub type MeshPtr = Arc<Mesh>;

pub struct Mesh {
    sg: SceneGraphPtr2,
    render_api: RenderApiPtr,
    tasks: Vec<smol::Task<()>>,

    vertex_buffer: miniquad::BufferId,
    index_buffer: miniquad::BufferId,
    // Texture
    num_elements: i32,

    dc_key: u64,

    node_id: SceneNodeId,
    rect: PropertyPtr,
    z_index: PropertyUint32,
}

impl Mesh {
    pub async fn new(
        ex: Arc<smol::Executor<'static>>,
        sg: SceneGraphPtr2,
        node_id: SceneNodeId,
        render_api: RenderApiPtr,
        verts: Vec<Vertex>,
        indices: Vec<u16>,
    ) -> Pimpl {
        let num_elements = indices.len() as i32;
        let vertex_buffer = render_api.new_vertex_buffer(verts).await.unwrap();
        let index_buffer = render_api.new_index_buffer(indices).await.unwrap();

        let scene_graph = sg.lock().await;
        let node = scene_graph.get_node(node_id).unwrap();
        let node_name = node.name.clone();
        let rect = node.get_property("rect").expect("RenderLayer::rect");
        let z_index_prop = node.get_property("z_index").expect("RenderLayer::z_index");
        let z_index = PropertyUint32::from(z_index_prop.clone(), 0).unwrap();
        drop(scene_graph);

        let self_ = Arc::new_cyclic(|me: &Weak<Self>| {
            let mut on_modify = OnModify::new(ex, node_name, node_id, me.clone());
            on_modify.when_change(rect.clone(), Self::redraw);
            on_modify.when_change(z_index_prop, Self::redraw);

            Self {
                sg,
                render_api,
                tasks: on_modify.tasks,
                vertex_buffer,
                index_buffer,
                num_elements,
                dc_key: OsRng.gen(),
                node_id,
                rect,
                z_index,
            }
        });

        Pimpl::Mesh(self_)
    }

    async fn redraw(self: Arc<Self>) {
        let sg = self.sg.lock().await;
        let node = sg.get_node(self.node_id).unwrap();

        let Some(parent_rect) = get_parent_rect(&sg, node) else {
            return;
        };

        let Some(draw_update) = self.draw(&sg, &parent_rect) else {
            error!("Mesh {:?} failed to draw", node);
            return;
        };
        self.render_api.replace_draw_calls(draw_update.draw_calls).await;
        debug!("replace draw calls done");
    }

    pub fn draw(&self, sg: &SceneGraph, parent_rect: &Rectangle) -> Option<DrawUpdate> {
        debug!(target: "app", "Mesh::draw()");
        // Only used for debug messages
        let node = sg.get_node(self.node_id).unwrap();

        let mesh = DrawMesh {
            vertex_buffer: self.vertex_buffer,
            index_buffer: self.index_buffer,
            texture: None,
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
        let scale_x = rect.w / parent_rect.w;
        let scale_y = rect.h / parent_rect.h;
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

impl Stoppable for Mesh {
    async fn stop(&self) {
        // TODO: Delete own draw call

        // Free buffers
        // Should this be in drop?
        self.render_api.delete_buffer(self.vertex_buffer);
        self.render_api.delete_buffer(self.index_buffer);
    }
}
