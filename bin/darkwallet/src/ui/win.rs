use std::sync::{Arc, Weak};

use crate::{
    gfx2::{DrawCall, GraphicsEventPublisherPtr, Rectangle, RenderApiPtr},
    prop::PropertyPtr,
    scene::{Pimpl, SceneGraph, SceneGraphPtr2, SceneNodeId},
};

use super::{OnModify, Stoppable};

pub type WindowPtr = Arc<Window>;

pub struct Window {
    node_id: SceneNodeId,
    // Task is dropped at the end of the scope for Window, hence ending it
    #[allow(dead_code)]
    tasks: Vec<smol::Task<()>>,
    screen_size_prop: PropertyPtr,
    render_api: RenderApiPtr,
}

impl Window {
    pub async fn new(
        ex: Arc<smol::Executor<'static>>,
        sg: SceneGraphPtr2,
        node_id: SceneNodeId,
        render_api: RenderApiPtr,
        event_pub: GraphicsEventPublisherPtr,
    ) -> Pimpl {
        debug!(target: "ui::win", "Window::new()");

        let scene_graph = sg.lock().await;
        let node = scene_graph.get_node(node_id).unwrap();
        let node_name = node.name.clone();
        let screen_size_prop = node.get_property("screen_size").unwrap();
        let scale_prop = node.get_property("scale").unwrap();
        drop(scene_graph);

        let self_ = Arc::new_cyclic(|me: &Weak<Self>| {
            // Start a task monitoring for window resize events
            // which updates screen_size
            let ev_sub = event_pub.subscribe_resize();
            let screen_size_prop2 = screen_size_prop.clone();
            let me2 = me.clone();
            let sg2 = sg.clone();
            let resize_task = ex.spawn(async move {
                loop {
                    let Ok((w, h)) = ev_sub.receive().await else {
                        debug!(target: "ui::win", "Event relayer closed");
                        break
                    };

                    debug!(target: "ui::win", "Window resized ({w}, {h})");
                    // Now update the properties
                    screen_size_prop2.set_f32(0, w).unwrap();
                    screen_size_prop2.set_f32(1, h).unwrap();

                    let Some(self_) = me2.upgrade() else {
                        // Should not happen
                        panic!("self destroyed before modify_task was stopped!");
                    };

                    let sg = sg2.lock().await;
                    self_.draw(&sg).await;
                }
            });

            let sg2 = sg.clone();
            let redraw_fn = move |self_: Arc<Self>| {
                let sg = sg2.clone();
                async move {
                    let sg = sg.lock().await;
                    self_.draw(&sg).await;
                }
            };

            let mut on_modify = OnModify::new(ex.clone(), node_name, node_id, me.clone());
            on_modify.when_change(scale_prop, redraw_fn);

            let mut tasks = on_modify.tasks;
            tasks.push(resize_task);

            Self { node_id, tasks, screen_size_prop, render_api }
        });

        Pimpl::Window(self_)
    }

    pub async fn draw(&self, sg: &SceneGraph) {
        debug!(target: "ui::win", "Window::draw()");
        // SceneGraph should remain locked for the entire draw
        let self_node = sg.get_node(self.node_id).unwrap();

        let screen_width = self.screen_size_prop.get_f32(0).unwrap();
        let screen_height = self.screen_size_prop.get_f32(1).unwrap();

        let parent_rect = Rectangle::from_array([0., 0., screen_width, screen_height]);

        let mut draw_calls = vec![];
        let mut child_calls = vec![];
        for child_inf in self_node.get_children2() {
            let node = sg.get_node(child_inf.id).unwrap();
            debug!(target: "ui::win", "Window::draw() calling draw() for node '{}':{}", node.name, node.id);

            let dcs = match &node.pimpl {
                Pimpl::RenderLayer(layer) => layer.draw(sg, &parent_rect).await,
                _ => {
                    error!(target: "ui::win", "unhandled pimpl type");
                    continue
                }
            };
            let Some(mut draw_update) = dcs else { continue };
            draw_calls.append(&mut draw_update.draw_calls);
            child_calls.push(draw_update.key);
        }

        let root_dc = DrawCall { instrs: vec![], dcs: child_calls, z_index: 0 };
        draw_calls.push((0, root_dc));
        //debug!(target: "ui::win", "  => {:?}", draw_calls);

        self.render_api.replace_draw_calls(draw_calls).await;
        debug!(target: "ui::win", "Window::draw() - replaced draw call");
    }
}

// Nodes should be stopped before being removed
impl Stoppable for Window {
    async fn stop(&self) {}
}
