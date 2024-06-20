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
    gfx2::{self, DrawCall, DrawInstruction, DrawMesh, GraphicsEvent, RenderApiPtr, Vertex},
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

trait Stoppable {
    async fn stop(&self);
}

pub type AsyncRuntimePtr = Arc<AsyncRuntime>;

pub struct AsyncRuntime {
    signal: smol::channel::Sender<()>,
    shutdown: smol::channel::Receiver<()>,
    exec_threadpool: std::sync::Mutex<Option<thread::JoinHandle<()>>>,
    ex: Arc<smol::Executor<'static>>,
    tasks: std::sync::Mutex<Vec<smol::Task<()>>>,
}

impl AsyncRuntime {
    pub fn new(ex: Arc<smol::Executor<'static>>) -> Self {
        let (signal, shutdown) = smol::channel::unbounded::<()>();

        Self {
            signal,
            shutdown,
            exec_threadpool: std::sync::Mutex::new(None),
            ex,
            tasks: std::sync::Mutex::new(vec![]),
        }
    }

    pub fn start(&self) {
        let n_threads = std::thread::available_parallelism().unwrap().get();
        let shutdown = self.shutdown.clone();
        let ex = self.ex.clone();
        let exec_threadpool = thread::spawn(move || {
            easy_parallel::Parallel::new()
                // N executor threads
                .each(0..n_threads, |_| smol::future::block_on(ex.run(shutdown.recv())))
                .run();
        });
        *self.exec_threadpool.lock().unwrap() = Some(exec_threadpool);
        debug!(target: "async_runtime", "Started runtime");
    }

    pub fn push_task(&self, task: smol::Task<()>) {
        self.tasks.lock().unwrap().push(task);
    }

    pub fn stop(&self) {
        // Go through event graph and call stop on everything
        // Depth first
        debug!(target: "app", "Stopping app...");

        let tasks = std::mem::take(&mut *self.tasks.lock().unwrap());
        // Close all tasks
        smol::future::block_on(async {
            // Perform cleanup code
            // If not finished in certain amount of time, then just exit

            let mut futures = FuturesUnordered::new();
            for task in tasks {
                futures.push(task.cancel());
            }
            let _: Vec<_> = futures.collect().await;
        });

        if !self.signal.close() {
            error!(target: "app", "exec threadpool was already shutdown");
        }
        let exec_threadpool = std::mem::replace(&mut *self.exec_threadpool.lock().unwrap(), None);
        let exec_threadpool = exec_threadpool.expect("threadpool wasnt started");
        exec_threadpool.join();
        debug!(target: "app", "Stopped app");
    }
}

pub type AppPtr = Arc<App>;

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

        // Testing
        let node = sg.get_node(window_id).unwrap();
        node.set_property_f32("scale", 2.).unwrap();

        drop(sg);

        self.make_me_a_schema_plox().await;

        // Access drawable in window node and call draw()
        self.trigger_redraw().await;
    }

    pub async fn stop(&self) {
        let sg = self.sg.lock().await;
        let window_id = sg.lookup_node("/window").unwrap().id;
        self.stop_node(&sg, window_id).await;
    }

    async fn stop_node(&self, sg: &SceneGraph, node_id: SceneNodeId) {
        let node = sg.get_node(node_id).unwrap();
        for child_inf in node.get_children2() {
            self.stop_node(sg, child_inf.id);
        }
        match &node.pimpl {
            Pimpl::Window(win) => win.stop().await,
            Pimpl::RenderLayer(layer) => layer.stop().await,
            Pimpl::Mesh(mesh) => mesh.stop().await,
            _ => panic!("unhandled pimpl type"),
        };
    }

    async fn make_me_a_schema_plox(&self) {
        // Create a layer called view
        let mut sg = self.sg.lock().await;
        let layer_node_id = chatapp::create_layer(&mut sg, "view");

        // Customize our layer
        let node = sg.get_node(layer_node_id).unwrap();
        let prop = node.get_property("rect").unwrap();
        prop.set_f32(0, 0.).unwrap();
        prop.set_f32(1, 0.).unwrap();
        let code = vec![Op::LoadVar("w".to_string())];
        prop.set_expr(2, code).unwrap();
        let code = vec![Op::LoadVar("h".to_string())];
        prop.set_expr(3, code).unwrap();
        node.set_property_bool("is_visible", true).unwrap();

        // Setup the pimpl
        let node_id = node.id;
        drop(sg);
        let pimpl =
            RenderLayer::new(self.sg.clone(), node_id, self.ex.clone(), self.render_api.clone())
                .await;
        let mut sg = self.sg.lock().await;
        let node = sg.get_node_mut(node_id).unwrap();
        node.pimpl = pimpl;

        let window_id = sg.lookup_node("/window").unwrap().id;
        sg.link(node_id, window_id).unwrap();

        // Create a bg mesh
        let node_id = chatapp::create_mesh(&mut sg, "bg");

        let node = sg.get_node_mut(node_id).unwrap();
        let prop = node.get_property("rect").unwrap();
        prop.set_f32(0, 0.).unwrap();
        prop.set_f32(1, 0.).unwrap();
        let code = vec![Op::LoadVar("w".to_string())];
        prop.set_expr(2, code).unwrap();
        let code = vec![Op::LoadVar("h".to_string())];
        prop.set_expr(3, code).unwrap();

        // Setup the pimpl
        let node_id = node.id;
        let (x1, y1) = (0., 0.);
        let (x2, y2) = (1., 1.);
        let verts = vec![
            // top left
            Vertex { pos: [x1, y1], color: [0., 0., 0., 1.], uv: [0., 0.] },
            // top right
            Vertex { pos: [x2, y1], color: [0., 0., 0., 1.], uv: [1., 0.] },
            // bottom left
            Vertex { pos: [x1, y2], color: [0., 0., 0., 1.], uv: [0., 1.] },
            // bottom right
            Vertex { pos: [x2, y2], color: [0., 0., 0., 1.], uv: [1., 1.] },
        ];
        let indices = vec![0, 2, 1, 1, 2, 3];
        drop(sg);
        let pimpl =
            Mesh::new(self.sg.clone(), node_id, self.render_api.clone(), verts, indices).await;
        let mut sg = self.sg.lock().await;
        let node = sg.get_node_mut(node_id).unwrap();
        node.pimpl = pimpl;

        sg.link(node_id, layer_node_id).unwrap();

        // Create another mesh
        let node_id = chatapp::create_mesh(&mut sg, "box");

        let node = sg.get_node_mut(node_id).unwrap();
        let prop = node.get_property("rect").unwrap();
        prop.set_f32(0, 10.).unwrap();
        prop.set_f32(1, 10.).unwrap();
        prop.set_f32(2, 60.).unwrap();
        prop.set_f32(3, 60.).unwrap();

        // Setup the pimpl
        let node_id = node.id;
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
        drop(sg);
        let pimpl =
            Mesh::new(self.sg.clone(), node_id, self.render_api.clone(), verts, indices).await;
        let mut sg = self.sg.lock().await;
        let node = sg.get_node_mut(node_id).unwrap();
        node.pimpl = pimpl;

        sg.link(node_id, layer_node_id).unwrap();
    }

    async fn trigger_redraw(&self) {
        let sg = self.sg.lock().await;
        let window_node = sg.lookup_node("/window").expect("no window attached!");
        match &window_node.pimpl {
            Pimpl::Window(win) => win.draw(&sg).await,
            _ => panic!("wrong pimpl"),
        }
    }
}

fn print_type_of<T>(_: &T) {
    println!("{}", std::any::type_name::<T>())
}

struct OnModify<T> {
    ex: Arc<smol::Executor<'static>>,
    node_name: String,
    node_id: SceneNodeId,
    me: Weak<T>,
    tasks: Vec<smol::Task<()>>,
}

impl<T: Send + Sync + 'static> OnModify<T> {
    fn new(
        ex: Arc<smol::Executor<'static>>,
        node_name: String,
        node_id: SceneNodeId,
        me: Weak<T>,
    ) -> Self {
        Self { ex, node_name, node_id, me, tasks: vec![] }
    }

    fn when_change<F>(&mut self, prop: PropertyPtr, f: impl Fn(Arc<T>) -> F + Send + 'static)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        let node_name = self.node_name.clone();
        let node_id = self.node_id;
        let on_modify_sub = prop.subscribe_modify();
        let prop_name = prop.name.clone();
        let me = self.me.clone();
        let task = self.ex.spawn(async move {
            loop {
                let _ = on_modify_sub.receive().await;
                debug!(target: "app", "Property '{}':{}/'{}' modified", node_name, node_id, prop_name);

                let Some(self_) = me.upgrade() else {
                    // Should not happen
                    panic!(
                        "'{}':{}/'{}' self destroyed before modify_task was stopped!",
                        node_name, node_id, prop_name
                    );
                };

                debug!(target: "app", "property modified");
                f(self_).await;
            }
        });
        self.tasks.push(task);
    }
}

fn eval_rect(rect: PropertyPtr, parent_rect: &Rectangle<f32>) -> Result<()> {
    if rect.array_len != 4 {
        return Err(Error::PropertyWrongLen)
    }

    for i in 0..4 {
        if !rect.is_expr(i)? {
            continue
        }

        let expr = rect.get_expr(i).unwrap();

        let machine = SExprMachine {
            globals: vec![
                ("w".to_string(), SExprVal::Float32(parent_rect.w)),
                ("h".to_string(), SExprVal::Float32(parent_rect.h)),
            ],
            stmts: &expr,
        };

        let v = machine.call()?.as_f32()?;
        rect.set_cache_f32(i, v).unwrap();
    }
    Ok(())
}

fn read_rect(rect_prop: PropertyPtr) -> Result<Rectangle<f32>> {
    if rect_prop.array_len != 4 {
        return Err(Error::PropertyWrongLen)
    }

    let mut rect = [0.; 4];
    for i in 0..4 {
        if rect_prop.is_expr(i)? {
            rect[i] = rect_prop.get_cached(i)?.as_f32()?;
        } else {
            rect[i] = rect_prop.get_f32(i)?;
        }
    }
    Ok(Rectangle::from_array(rect))
}

fn get_parent_rect(sg: &SceneGraph, node: &SceneNode) -> Option<Rectangle<f32>> {
    // read our parent
    if node.parents.is_empty() {
        info!("RenderLayer {:?} has no parents so skipping", node);
        return None
    }
    if node.parents.len() != 1 {
        error!("RenderLayer {:?} has too many parents so skipping", node);
        return None
    }
    let parent_id = node.parents[0].id;
    let parent_node = sg.get_node(parent_id).unwrap();
    let parent_rect = match parent_node.typ {
        SceneNodeType::Window => {
            let Some(screen_size_prop) = parent_node.get_property("screen_size") else {
                error!(
                    "RenderLayer {:?} parent node {:?} missing screen_size property",
                    node, parent_node
                );
                return None
            };
            let screen_width = screen_size_prop.get_f32(0).unwrap();
            let screen_height = screen_size_prop.get_f32(1).unwrap();

            let parent_rect = Rectangle { x: 0., y: 0., w: screen_width, h: screen_height };
            parent_rect
        }
        SceneNodeType::RenderLayer => {
            // get their rect property
            let Some(parent_rect) = parent_node.get_property("rect") else {
                error!(
                    "RenderLayer {:?} parent node {:?} missing rect property",
                    node, parent_node
                );
                return None
            };
            // read parent's rect
            let Ok(parent_rect) = read_rect(parent_rect) else {
                error!(
                    "RenderLayer {:?} parent node {:?} malformed rect property",
                    node, parent_node
                );
                return None
            };
            parent_rect
        }
        _ => {
            error!(
                "RenderLayer {:?} parent node {:?} wrong type {:?}",
                node, parent_node, parent_node.typ
            );
            return None
        }
    };
    Some(parent_rect)
}

struct DrawUpdate {
    key: u64,
    draw_calls: Vec<(u64, DrawCall)>,
}

pub type WindowPtr = Arc<Window>;

pub struct Window {
    node_id: SceneNodeId,
    tasks: Vec<smol::Task<()>>,
    screen_size_prop: PropertyPtr,
    render_api: RenderApiPtr,
}

impl Window {
    pub async fn new(
        sg: SceneGraphPtr2,
        node_id: SceneNodeId,
        ex: Arc<smol::Executor<'static>>,
        render_api: RenderApiPtr,
        event_pub: PublisherPtr<GraphicsEvent>,
    ) -> Pimpl {
        debug!(target: "app", "Window::new()");

        let scene_graph = sg.lock().await;
        let node = scene_graph.get_node(node_id).unwrap();
        let node_name = node.name.clone();
        let screen_size_prop = node.get_property("screen_size").unwrap();
        let scale_prop = node.get_property("scale").unwrap();
        drop(scene_graph);

        let self_ = Arc::new_cyclic(|me: &Weak<Self>| {
            // Start a task monitoring for window resize events
            // which updates screen_size
            let ev_sub = event_pub.subscribe();
            let screen_size_prop2 = screen_size_prop.clone();
            let me2 = me.clone();
            let sg2 = sg.clone();
            let resize_task = ex.spawn(async move {
                loop {
                    let Ok(ev) = ev_sub.receive().await else {
                        debug!(target: "app", "Event relayer closed");
                        break
                    };
                    let (w, h) = match ev {
                        GraphicsEvent::Resize((w, h)) => (w, h),
                        _ => continue,
                    };

                    debug!(target: "app", "Window resized ({w}, {h})");
                    // Now update the properties
                    screen_size_prop2.set_f32(0, w);
                    screen_size_prop2.set_f32(1, h);

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

    async fn draw(&self, sg: &SceneGraph) {
        debug!(target: "app", "Window::draw()");
        // SceneGraph should remain locked for the entire draw
        let self_node = sg.get_node(self.node_id).unwrap();

        let screen_width = self.screen_size_prop.get_f32(0).unwrap();
        let screen_height = self.screen_size_prop.get_f32(1).unwrap();

        let parent_rect = Rectangle { x: 0., y: 0., w: screen_width, h: screen_height };

        let mut draw_calls = vec![];
        let mut child_calls = vec![];
        for child_inf in self_node.get_children2() {
            let node = sg.get_node(child_inf.id).unwrap();
            debug!(target: "app", "Window::draw() calling draw() for node '{}':{}", node.name, node.id);

            let dcs = match &node.pimpl {
                Pimpl::RenderLayer(layer) => layer.draw(sg, &parent_rect).await,
                _ => {
                    error!(target: "app", "unhandled pimpl type");
                    continue
                }
            };
            let Some(mut draw_update) = dcs else { continue };
            draw_calls.append(&mut draw_update.draw_calls);
            child_calls.push(draw_update.key);
        }

        let root_dc = DrawCall { instrs: vec![], dcs: child_calls };
        draw_calls.push((0, root_dc));
        //debug!("  => {:?}", draw_calls);

        self.render_api.replace_draw_calls(draw_calls).await;
        debug!("Window::draw() - replaced draw call");
    }
}

// Nodes should be stopped before being removed
impl Stoppable for Window {
    async fn stop(&self) {}
}

pub type RenderLayerPtr = Arc<RenderLayer>;

pub struct RenderLayer {
    sg: SceneGraphPtr2,
    node_id: SceneNodeId,
    tasks: Vec<smol::Task<()>>,
    render_api: RenderApiPtr,

    dc_key: u64,

    is_visible: PropertyBool,
    rect: PropertyPtr,

    parent_rect: Mutex<Rectangle<f32>>,
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

        // Monitor for changes to screen_size or scale properties
        // If so then trigger draw
        let rect_sub = rect.subscribe_modify();

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
                parent_rect: Mutex::new(Rectangle { x: 0., y: 0., w: 0., h: 0. }),
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
    pub async fn draw(&self, sg: &SceneGraph, parent_rect: &Rectangle<f32>) -> Option<DrawUpdate> {
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
