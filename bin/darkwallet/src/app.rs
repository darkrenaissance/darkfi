use async_lock::Mutex;
use futures::{stream::FuturesUnordered, StreamExt};
use std::{
    sync::{mpsc, Arc, Weak},
    thread,
};

use crate::{
    chatapp,
    error::{Error, Result},
    expr::Op,
    gfx2::{GraphicsEvent, RenderApiPtr},
    prop::{Property, PropertySubType, PropertyType},
    pubsub::PublisherPtr,
    scene::{
        MethodResponseFn, Pimpl, SceneGraph, SceneGraphPtr2, SceneNode, SceneNodeId, SceneNodeInfo,
        SceneNodeType,
    },
};

trait Stoppable {
    async fn stop(self);
}

pub struct App {
    sg: SceneGraphPtr2,
    ex: Arc<smol::Executor<'static>>,
    render_api: RenderApiPtr,
    event_pub: PublisherPtr<GraphicsEvent>,
}

impl App {
    pub fn new(
        sg: SceneGraphPtr2,
        ex: Arc<smol::Executor<'static>>,
        render_api: RenderApiPtr,
        event_pub: PublisherPtr<GraphicsEvent>,
    ) -> Arc<Self> {
        debug!("App::new()");
        Arc::new(Self { sg, ex, render_api, event_pub })
    }

    pub async fn start(self: Arc<Self>) {
        debug!("App::start()");
        // Setup UI
        let mut sg = self.sg.lock().await;

        let window = sg.add_node("window", SceneNodeType::Window);

        let mut prop = Property::new("screen_size", PropertyType::Float32, PropertySubType::Pixel);
        prop.set_array_len(2);
        // Window not yet initialized so we can't set these.
        //prop.set_f32(0, screen_width);
        //prop.set_f32(1, screen_height);
        window.add_property(prop).unwrap();

        let mut prop = Property::new("scale", PropertyType::Float32, PropertySubType::Pixel);
        prop.set_defaults_f32(vec![1.]).unwrap();
        window.add_property(prop).unwrap();

        let window_id = window.id;

        // Create Window
        // Window::new(window, weak sg)
        drop(sg);
        let pimpl = Window::new(
            self.sg.clone(),
            window_id,
            self.ex.clone(),
            self.render_api.clone(),
            self.event_pub.clone(),
        )
        .await;
        // -> reads any props it needs
        // -> starts procs
        let mut sg = self.sg.lock().await;
        let node = sg.get_node_mut(window_id).unwrap();
        node.pimpl = pimpl;

        sg.link(window_id, SceneGraph::ROOT_ID).unwrap();

        self.make_me_a_schema_plox().await;

        // Access drawable in window node and call draw()
        self.trigger_redraw().await;

        let node = sg.get_node(window_id).unwrap();
        node.set_property_f32("scale", 2.).unwrap();
    }

    async fn make_me_a_schema_plox(&self) {
        let mut sg = self.sg.lock().await;
        let layer_node_id = chatapp::create_layer(&mut sg, "view");

        // Customize our layer
        let node = sg.get_node(layer_node_id).unwrap();
        let prop = node.get_property("rect").unwrap();
        prop.set_u32(0, 0).unwrap();
        prop.set_u32(1, 0).unwrap();
        let code = vec![Op::Float32ToUint32((Box::new(Op::LoadVar("sw".to_string()))))];
        prop.set_expr(2, code).unwrap();
        let code = vec![Op::Float32ToUint32((Box::new(Op::LoadVar("sh".to_string()))))];
        prop.set_expr(3, code).unwrap();

        // Setup the pimpl
        let node_id = node.id;
        drop(sg);
        let pimpl = RenderLayer::new().await;
        let mut sg = self.sg.lock().await;
        let node = sg.get_node_mut(node_id).unwrap();
        node.pimpl = pimpl;

        let window_id = sg.lookup_node("/window").unwrap().id;
        sg.link(node_id, window_id).unwrap();
    }

    async fn trigger_redraw(&self) {
        let sg = self.sg.lock().await;
        let window_node = sg.lookup_node("/window").expect("no window attached!");
        match &window_node.pimpl {
            Pimpl::Window(win) => win.draw().await,
            _ => panic!("wrong pimpl"),
        }
    }

    pub async fn stop(&self) {
        // Go through event graph and call stop on everything
        // Depth first
        debug!("Stopping app...");
    }
}

fn print_type_of<T>(_: &T) {
    println!("{}", std::any::type_name::<T>())
}

pub type WindowPtr = Arc<Window>;

pub struct Window {
    sg: SceneGraphPtr2,
    node_id: SceneNodeId,
    render_api: RenderApiPtr,
    resize_task: smol::Task<()>,
    modify_task: smol::Task<()>,
}

impl Window {
    pub async fn new(
        sg: SceneGraphPtr2,
        node_id: SceneNodeId,
        ex: Arc<smol::Executor<'static>>,
        render_api: RenderApiPtr,
        event_pub: PublisherPtr<GraphicsEvent>,
    ) -> Pimpl {
        debug!("Window::new()");

        let screen_size_prop = {
            let sg = sg.lock().await;
            let node = sg.get_node(node_id).unwrap();
            node.get_property("screen_size").unwrap()
        };

        // Start a task monitoring for window resize events
        // which updates screen_size
        let ev_sub = event_pub.clone().subscribe();
        let screen_size_prop2 = screen_size_prop.clone();
        let resize_task = ex.spawn(async move {
            loop {
                let Ok(ev) = ev_sub.receive().await else {
                    debug!("Event relayer closed");
                    break
                };
                let (w, h) = match ev {
                    GraphicsEvent::Resize((w, h)) => (w, h),
                    _ => continue,
                };

                // Now update the properties
                screen_size_prop2.set_f32(0, w);
                screen_size_prop2.set_f32(1, h);
            }
        });

        // Monitor for changes to screen_size or scale properties
        // If so then trigger draw
        let scale_sub = {
            let sg = sg.lock().await;
            let node = sg.get_node(node_id).unwrap();
            let prop = node.get_property("scale").unwrap();
            prop.subscribe_modify()
        };
        let screen_size_sub = screen_size_prop.subscribe_modify();

        let self_ = Arc::new_cyclic(|me: &Weak<Self>| {
            // Modify task needs a Weak<Self>
            let me2 = me.clone();
            let modify_task = ex.spawn(async move {
                loop {
                    let mut futures = FuturesUnordered::new();
                    futures.push(scale_sub.receive());
                    futures.push(screen_size_sub.receive());

                    while let Some(ev) = futures.next().await {
                        let Ok(_) = ev else {
                            debug!("prop sub closed");
                            break
                        };

                        let Some(self_) = me2.upgrade() else {
                            // Should not happen
                            panic!("self destroyed before modify_task was stopped!");
                        };

                        self_.draw().await;
                    }
                }
            });

            Self { sg, node_id, render_api, resize_task, modify_task }
        });

        Pimpl::Window(self_)
    }

    async fn draw(&self) {
        // This should remain locked for the entire draw
        let sg = self.sg.lock().await;
        let self_node = sg.get_node(self.node_id).unwrap();

        for child_inf in self_node.get_children2() {
            let node = sg.get_node(child_inf.id).unwrap();

            let sg_ref: &SceneGraph = &sg;
            match &node.pimpl {
                //Pimpl::RenderLayer(layer) => layer.draw(sg_ref).await,
                _ => error!("unhandled pimpl type"),
            }
        }
    }
}

// Nodes should be stopped before being removed
impl Stoppable for Window {
    async fn stop(self) {
        self.resize_task.cancel().await;
    }
}

pub type RenderLayerPtr = Arc<RenderLayer>;

pub struct RenderLayer {}

impl RenderLayer {
    pub async fn new() -> Pimpl {
        let self_ = Arc::new(Self {});

        Pimpl::RenderLayer(self_)
    }

    pub async fn draw(&self, sg: &SceneGraph) {}
}
