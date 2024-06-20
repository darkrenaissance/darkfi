use async_lock::Mutex;
use async_recursion::async_recursion;
use futures::{stream::FuturesUnordered, StreamExt};
use rand::{rngs::OsRng, Rng};
use std::{
    sync::{mpsc, Arc, Weak},
    thread,
};

use crate::{
    chatapp,
    error::{Error, Result},
    expr::{Op, SExprMachine, SExprVal},
    gfx::Rectangle,
    gfx2::{
        self, DrawCall, DrawInstruction, DrawMesh, GraphicsEventPublisherPtr, RenderApiPtr, Vertex,
    },
    prop::{
        Property, PropertyBool, PropertyColor, PropertyFloat32, PropertyPtr, PropertyStr,
        PropertySubType, PropertyType, PropertyUint32,
    },
    pubsub::PublisherPtr,
    scene::{
        MethodResponseFn, Pimpl, SceneGraph, SceneGraphPtr2, SceneNode, SceneNodeId, SceneNodeInfo,
        SceneNodeType,
    },
};

use super::{eval_rect, read_rect, DrawUpdate, Stoppable};

pub struct Mesh {
    render_api: RenderApiPtr,
    vertex_buffer: miniquad::BufferId,
    index_buffer: miniquad::BufferId,
    // Texture
    num_elements: i32,

    dc_key: u64,

    node_id: SceneNodeId,
    rect: PropertyPtr,
}

impl Mesh {
    pub async fn new(
        sg: SceneGraphPtr2,
        node_id: SceneNodeId,
        render_api: RenderApiPtr,
        verts: Vec<Vertex>,
        indices: Vec<u16>,
    ) -> Pimpl {
        let num_elements = indices.len() as i32;
        let vertex_buffer = render_api.new_vertex_buffer(verts).await.unwrap();
        let index_buffer = render_api.new_index_buffer(indices).await.unwrap();

        let mut sg = sg.lock().await;
        let node = sg.get_node_mut(node_id).unwrap();
        let rect = node.get_property("rect").expect("RenderLayer::rect");

        Pimpl::Mesh(Self {
            render_api,
            vertex_buffer,
            index_buffer,
            num_elements,
            dc_key: OsRng.gen(),
            node_id,
            rect,
        })
    }

    pub fn draw(&self, sg: &SceneGraph, parent_rect: &Rectangle<f32>) -> Option<DrawUpdate> {
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
