use async_recursion::async_recursion;
use futures::{stream::FuturesUnordered, StreamExt};
use std::{sync::Arc, thread};

use crate::{
    expr::Op,
    gfx2::{GraphicsEventPublisherPtr, RenderApiPtr, Vertex},
    prop::{Property, PropertySubType, PropertyType},
    scene::{Pimpl, SceneGraph, SceneGraphPtr2, SceneNodeId, SceneNodeType},
    ui::{Mesh, RenderLayer, Stoppable, Window},
};

//fn print_type_of<T>(_: &T) {
//    println!("{}", std::any::type_name::<T>())
//}

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

            let futures = FuturesUnordered::new();
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
        exec_threadpool.join().unwrap();
        debug!(target: "app", "Stopped app");
    }
}

pub struct App {
    sg: SceneGraphPtr2,
    ex: Arc<smol::Executor<'static>>,
    render_api: RenderApiPtr,
    event_pub: GraphicsEventPublisherPtr,
}

impl App {
    pub fn new(
        sg: SceneGraphPtr2,
        ex: Arc<smol::Executor<'static>>,
        render_api: RenderApiPtr,
        event_pub: GraphicsEventPublisherPtr,
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

    #[async_recursion]
    async fn stop_node(&self, sg: &SceneGraph, node_id: SceneNodeId) {
        let node = sg.get_node(node_id).unwrap();
        for child_inf in node.get_children2() {
            self.stop_node(sg, child_inf.id).await;
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
        let layer_node_id = create_layer(&mut sg, "view");

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
        let node_id = create_mesh(&mut sg, "bg");

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
        let node_id = create_mesh(&mut sg, "box");

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

pub fn create_layer(sg: &mut SceneGraph, name: &str) -> SceneNodeId {
    let node = sg.add_node(name, SceneNodeType::RenderLayer);
    let prop = Property::new("is_visible", PropertyType::Bool, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("rect", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_array_len(4);
    prop.allow_exprs();
    node.add_property(prop).unwrap();

    node.id
}

pub fn create_mesh(sg: &mut SceneGraph, name: &str) -> SceneNodeId {
    let node = sg.add_node(name, SceneNodeType::RenderMesh);

    let mut prop = Property::new("rect", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_array_len(4);
    prop.allow_exprs();
    node.add_property(prop).unwrap();

    let prop = Property::new("z_index", PropertyType::Uint32, PropertySubType::Null);
    node.add_property(prop).unwrap();

    node.id
}
