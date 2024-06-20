use async_recursion::async_recursion;
use rand::{rngs::OsRng, Rng};
use std::sync::{Arc, Weak};

use crate::{
    gfx2::{DrawCall, DrawInstruction, Rectangle, RenderApiPtr},
    prop::{PropertyBool, PropertyPtr},
    scene::{Pimpl, SceneGraph, SceneGraphPtr2, SceneNodeId},
};

use super::{eval_rect, get_parent_rect, read_rect, DrawUpdate, OnModify, Stoppable};

pub type RenderLayerPtr = Arc<RenderLayer>;

pub struct RenderLayer {
    sg: SceneGraphPtr2,
    node_id: SceneNodeId,
    // Task is dropped at the end of the scope for RenderLayer, hence ending it
    #[allow(dead_code)]
    tasks: Vec<smol::Task<()>>,
    render_api: RenderApiPtr,

    dc_key: u64,

    is_visible: PropertyBool,
    rect: PropertyPtr,
}

impl RenderLayer {
    pub async fn new(
        sg_ptr: SceneGraphPtr2,
        node_id: SceneNodeId,
        ex: Arc<smol::Executor<'static>>,
        render_api: RenderApiPtr,
    ) -> Pimpl {
        let sg = sg_ptr.lock().await;
        let node = sg.get_node(node_id).unwrap();
        let node_name = node.name.clone();

        let is_visible =
            PropertyBool::wrap(node, "is_visible", 0).expect("RenderLayer::is_visible");
        let rect = node.get_property("rect").expect("RenderLayer::rect");
        drop(sg);

        let self_ = Arc::new_cyclic(|me: &Weak<Self>| {
            let mut on_modify = OnModify::new(ex.clone(), node_name, node_id, me.clone());
            on_modify.when_change(rect.clone(), Self::redraw);

            Self {
                sg: sg_ptr,
                node_id,
                tasks: on_modify.tasks,
                render_api,
                dc_key: OsRng.gen(),
                is_visible,
                rect,
            }
        });

        Pimpl::RenderLayer(self_)
    }

    async fn redraw(self: Arc<Self>) {
        let sg = self.sg.lock().await;
        let node = sg.get_node(self.node_id).unwrap();

        let Some(parent_rect) = get_parent_rect(&sg, node) else {
            return;
        };

        let Some(draw_update) = self.draw(&sg, &parent_rect).await else {
            error!("RenderLayer {:?} failed to draw", node);
            return;
        };
        self.render_api.replace_draw_calls(draw_update.draw_calls).await;
        debug!("replace draw calls done");
    }

    #[async_recursion]
    pub async fn draw(&self, sg: &SceneGraph, parent_rect: &Rectangle) -> Option<DrawUpdate> {
        debug!(target: "app", "RenderLayer::draw()");
        let node = sg.get_node(self.node_id).unwrap();

        if !self.is_visible.get() {
            debug!(target: "app", "invisible layer node '{}':{}", node.name, node.id);
            return None
        }

        if let Err(err) = eval_rect(self.rect.clone(), parent_rect) {
            panic!("Node {:?} bad rect property: {}", node, err);
        }

        let Ok(mut rect) = read_rect(self.rect.clone()) else {
            panic!("Node {:?} bad rect property", node);
        };

        rect.x += parent_rect.x;
        rect.y += parent_rect.x;

        if !parent_rect.includes(&rect) {
            error!(
                target: "app",
                "layer '{}':{} rect {:?} is not inside parent {:?}",
                node.name, node.id, rect, parent_rect
            );
            return None
        }

        debug!(target: "app", "Parent rect: {:?}", parent_rect);
        debug!(target: "app", "Viewport rect: {:?}", rect);

        // Apply viewport

        let mut draw_calls = vec![];
        let mut child_calls = vec![];
        for child_inf in node.get_children2() {
            let node = sg.get_node(child_inf.id).unwrap();

            let dcs = match &node.pimpl {
                Pimpl::RenderLayer(layer) => layer.draw(&sg, &rect).await,
                Pimpl::Mesh(mesh) => mesh.draw(&sg, &rect),
                _ => {
                    error!(target: "app", "unhandled pimpl type");
                    continue
                }
            };
            let Some(mut draw_update) = dcs else { continue };
            draw_calls.append(&mut draw_update.draw_calls);
            child_calls.push(draw_update.key);
        }

        let dc = DrawCall { instrs: vec![DrawInstruction::ApplyViewport(rect)], dcs: child_calls };
        draw_calls.push((self.dc_key, dc));
        Some(DrawUpdate { key: self.dc_key, draw_calls })
    }
}

impl Stoppable for RenderLayer {
    async fn stop(&self) {}
}
