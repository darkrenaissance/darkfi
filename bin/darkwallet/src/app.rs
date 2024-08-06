/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use async_recursion::async_recursion;
use chrono::{NaiveDate, NaiveDateTime};
use darkfi_serial::Encodable;
use futures::{stream::FuturesUnordered, StreamExt};
use std::{sync::{Arc, Mutex as SyncMutex}, thread};
use smol::Task;

use crate::{
    error::Error,
    expr::Op,
    gfx2::{GraphicsEventPublisherPtr, RenderApiPtr, Vertex},
    prop::{Property, PropertySubType, PropertyType, Role, PropertyStr, PropertyBool},
    scene::{
        CallArgType, MethodResponseFn, Pimpl, SceneGraph, SceneGraphPtr2, SceneNodeId,
        SceneNodeType, Slot
    },
    text2::TextShaperPtr,
    ui::{chatview, Button, ChatView, EditBox, Image, Mesh, RenderLayer, Stoppable, Text, Window},
    ExecutorPtr,
};

//fn print_type_of<T>(_: &T) {
//    println!("{}", std::any::type_name::<T>())
//}

#[cfg(target_os = "android")]
const CHATDB_PATH: &str = "/data/data/darkfi.darkwallet/chatdb/";
#[cfg(target_os = "android")]
//const KING_PATH: &str = "/data/data/darkfi.darkwallet/assets/king.png";
const KING_PATH: &str = "king.png";

#[cfg(target_os = "linux")]
const CHATDB_PATH: &str = "chatdb";
#[cfg(target_os = "linux")]
const KING_PATH: &str = "assets/king.png";

const LIGHTMODE: bool = false;

pub struct AsyncRuntime {
    signal: async_channel::Sender<()>,
    shutdown: async_channel::Receiver<()>,
    exec_threadpool: SyncMutex<Option<thread::JoinHandle<()>>>,
    ex: ExecutorPtr,
    tasks: SyncMutex<Vec<Task<()>>>,
}

impl AsyncRuntime {
    pub fn new(ex: ExecutorPtr) -> Self {
        let (signal, shutdown) = async_channel::unbounded::<()>();

        Self {
            signal,
            shutdown,
            exec_threadpool: SyncMutex::new(None),
            ex,
            tasks: SyncMutex::new(vec![]),
        }
    }

    pub fn start(&self) {
        let n_threads = thread::available_parallelism().unwrap().get();
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

    pub fn push_task(&self, task: Task<()>) {
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

pub type AppPtr = Arc<App>;

pub struct App {
    sg: SceneGraphPtr2,
    ex: ExecutorPtr,
    render_api: RenderApiPtr,
    event_pub: GraphicsEventPublisherPtr,
    text_shaper: TextShaperPtr,
    tasks: SyncMutex<Vec<Task<()>>>,
}

impl App {
    pub fn new(
        sg: SceneGraphPtr2,
        ex: ExecutorPtr,
        render_api: RenderApiPtr,
        event_pub: GraphicsEventPublisherPtr,
        text_shaper: TextShaperPtr,
    ) -> Arc<Self> {
        Arc::new(Self { sg, ex, render_api, event_pub, text_shaper, tasks: SyncMutex::new(vec![]) })
    }

    pub async fn start(self: Arc<Self>) {
        debug!(target: "app", "App::start()");
        // Setup UI
        let mut sg = self.sg.lock().await;

        let window = sg.add_node("window", SceneNodeType::Window);

        let mut prop = Property::new("screen_size", PropertyType::Float32, PropertySubType::Pixel);
        prop.set_array_len(2);
        // Window not yet initialized so we can't set these.
        //prop.set_f32(Role::App, 0, screen_width);
        //prop.set_f32(Role::App, 1, screen_height);
        window.add_property(prop).unwrap();

        let mut prop = Property::new("scale", PropertyType::Float32, PropertySubType::Pixel);
        prop.set_defaults_f32(vec![1.]).unwrap();
        window.add_property(prop).unwrap();

        let window_id = window.id;

        // Create Window
        // Window::new(window, weak sg)
        drop(sg);
        let pimpl = Window::new(
            self.ex.clone(),
            self.sg.clone(),
            window_id,
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
        node.set_property_f32(Role::App, "scale", 2.).unwrap();

        drop(sg);

        self.make_me_a_schema_plox().await;
        debug!(target: "app", "Schema loaded");

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
        let mut tasks = vec![];
        // Create a layer called view
        let mut sg = self.sg.lock().await;
        let layer_node_id = create_layer(&mut sg, "view");

        // Customize our layer
        let node = sg.get_node(layer_node_id).unwrap();
        let prop = node.get_property("rect").unwrap();
        prop.set_f32(Role::App, 0, 0.).unwrap();
        prop.set_f32(Role::App, 1, 0.).unwrap();
        let code = vec![Op::LoadVar("w".to_string())];
        prop.set_expr(Role::App, 2, code).unwrap();
        let code = vec![Op::LoadVar("h".to_string())];
        prop.set_expr(Role::App, 3, code).unwrap();
        node.set_property_bool(Role::App, "is_visible", true).unwrap();

        // Setup the pimpl
        let node_id = node.id;
        drop(sg);
        let pimpl =
            RenderLayer::new(self.ex.clone(), self.sg.clone(), node_id, self.render_api.clone())
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
        prop.set_f32(Role::App, 0, 0.).unwrap();
        prop.set_f32(Role::App, 1, 0.).unwrap();
        let code = vec![Op::LoadVar("w".to_string())];
        prop.set_expr(Role::App, 2, code).unwrap();
        let code = vec![Op::LoadVar("h".to_string())];
        prop.set_expr(Role::App, 3, code).unwrap();

        let c = if LIGHTMODE { 1. } else { 0. };
        // Setup the pimpl
        let node_id = node.id;
        let (x1, y1) = (0., 0.);
        let (x2, y2) = (1., 1.);
        let verts = vec![
            // top left
            Vertex { pos: [x1, y1], color: [c, c, c, 1.], uv: [0., 0.] },
            // top right
            Vertex { pos: [x2, y1], color: [c, c, c, 1.], uv: [1., 0.] },
            // bottom left
            Vertex { pos: [x1, y2], color: [c, c, c, 1.], uv: [0., 1.] },
            // bottom right
            Vertex { pos: [x2, y2], color: [c, c, c, 1.], uv: [1., 1.] },
        ];
        let indices = vec![0, 2, 1, 1, 2, 3];
        drop(sg);
        let pimpl = Mesh::new(
            self.ex.clone(),
            self.sg.clone(),
            node_id,
            self.render_api.clone(),
            verts,
            indices,
        )
        .await;
        let mut sg = self.sg.lock().await;
        let node = sg.get_node_mut(node_id).unwrap();
        node.pimpl = pimpl;

        sg.link(node_id, layer_node_id).unwrap();

        // Create button bg
        let node_id = create_mesh(&mut sg, "btnbg");

        let node = sg.get_node_mut(node_id).unwrap();
        let prop = node.get_property("rect").unwrap();
        let code = vec![Op::Sub((
            Box::new(Op::LoadVar("w".to_string())),
            Box::new(Op::ConstFloat32(220.)),
        ))];
        prop.set_expr(Role::App, 0, code).unwrap();
        prop.set_f32(Role::App, 1, 10.).unwrap();
        prop.set_f32(Role::App, 2, 200.).unwrap();
        prop.set_f32(Role::App, 3, 60.).unwrap();

        // Setup the pimpl
        let (x1, y1) = (0., 0.);
        let (x2, y2) = (1., 1.);
        let verts = if LIGHTMODE {
            vec![
                // top left
                Vertex { pos: [x1, y1], color: [1., 0., 0., 1.], uv: [0., 0.] },
                // top right
                Vertex { pos: [x2, y1], color: [1., 0., 0., 1.], uv: [1., 0.] },
                // bottom left
                Vertex { pos: [x1, y2], color: [1., 0., 0., 1.], uv: [0., 1.] },
                // bottom right
                Vertex { pos: [x2, y2], color: [1., 0., 0., 1.], uv: [1., 1.] },
            ]
        } else {
            vec![
                // top left
                Vertex { pos: [x1, y1], color: [1., 0., 0., 1.], uv: [0., 0.] },
                // top right
                Vertex { pos: [x2, y1], color: [1., 0., 1., 1.], uv: [1., 0.] },
                // bottom left
                Vertex { pos: [x1, y2], color: [0., 0., 1., 1.], uv: [0., 1.] },
                // bottom right
                Vertex { pos: [x2, y2], color: [1., 1., 0., 1.], uv: [1., 1.] },
            ]
        };
        let indices = vec![0, 2, 1, 1, 2, 3];
        drop(sg);
        let pimpl = Mesh::new(
            self.ex.clone(),
            self.sg.clone(),
            node_id,
            self.render_api.clone(),
            verts,
            indices,
        )
        .await;
        let mut sg = self.sg.lock().await;
        let node = sg.get_node_mut(node_id).unwrap();
        node.pimpl = pimpl;

        sg.link(node_id, layer_node_id).unwrap();

        // Create the button
        let node_id = create_button(&mut sg, "btn");

        let node = sg.get_node_mut(node_id).unwrap();
        node.set_property_bool(Role::App, "is_active", true).unwrap();
        let prop = node.get_property("rect").unwrap();
        let code = vec![Op::Sub((
            Box::new(Op::LoadVar("w".to_string())),
            Box::new(Op::ConstFloat32(220.)),
        ))];
        prop.set_expr(Role::App, 0, code).unwrap();
        prop.set_f32(Role::App, 1, 10.).unwrap();
        prop.set_f32(Role::App, 2, 200.).unwrap();
        prop.set_f32(Role::App, 3, 60.).unwrap();

        let (sender, btn_click_recvr) = async_channel::unbounded();
        let slot_click = Slot {
            name: "button_clicked".to_string(),
            notify: sender
        };
        node.register("click", slot_click).unwrap();

        drop(sg);
        let pimpl =
            Button::new(self.ex.clone(), self.sg.clone(), node_id, self.event_pub.clone()).await;
        let mut sg = self.sg.lock().await;
        let node = sg.get_node_mut(node_id).unwrap();
        node.pimpl = pimpl;

        sg.link(node_id, layer_node_id).unwrap();

        // Create another mesh
        let node_id = create_mesh(&mut sg, "box");

        let node = sg.get_node_mut(node_id).unwrap();
        let prop = node.get_property("rect").unwrap();
        prop.set_f32(Role::App, 0, 10.).unwrap();
        prop.set_f32(Role::App, 1, 10.).unwrap();
        prop.set_f32(Role::App, 2, 60.).unwrap();
        prop.set_f32(Role::App, 3, 60.).unwrap();

        // Setup the pimpl
        let (x1, y1) = (0., 0.);
        let (x2, y2) = (1., 1.);
        let verts = if LIGHTMODE {
            vec![
                // top left
                Vertex { pos: [x1, y1], color: [1., 0., 0., 1.], uv: [0., 0.] },
                // top right
                Vertex { pos: [x2, y1], color: [1., 0., 0., 1.], uv: [1., 0.] },
                // bottom left
                Vertex { pos: [x1, y2], color: [1., 0., 0., 1.], uv: [0., 1.] },
                // bottom right
                Vertex { pos: [x2, y2], color: [1., 0., 0., 1.], uv: [1., 1.] },
            ]
        } else {
            vec![
                // top left
                Vertex { pos: [x1, y1], color: [1., 0., 0., 1.], uv: [0., 0.] },
                // top right
                Vertex { pos: [x2, y1], color: [1., 0., 1., 1.], uv: [1., 0.] },
                // bottom left
                Vertex { pos: [x1, y2], color: [0., 0., 1., 1.], uv: [0., 1.] },
                // bottom right
                Vertex { pos: [x2, y2], color: [1., 1., 0., 1.], uv: [1., 1.] },
            ]
        };
        let indices = vec![0, 2, 1, 1, 2, 3];
        drop(sg);
        let pimpl = Mesh::new(
            self.ex.clone(),
            self.sg.clone(),
            node_id,
            self.render_api.clone(),
            verts,
            indices,
        )
        .await;
        let mut sg = self.sg.lock().await;
        let node = sg.get_node_mut(node_id).unwrap();
        node.pimpl = pimpl;

        sg.link(node_id, layer_node_id).unwrap();

        // Debugging tool
        let node_id = create_mesh(&mut sg, "debugtool");

        let node = sg.get_node_mut(node_id).unwrap();
        let prop = node.get_property("rect").unwrap();
        prop.set_f32(Role::App, 0, 0.).unwrap();
        let code =
            vec![Op::Div((Box::new(Op::LoadVar("h".to_string())), Box::new(Op::ConstFloat32(2.))))];
        prop.set_expr(Role::App, 1, code).unwrap();
        let code = vec![Op::LoadVar("w".to_string())];
        prop.set_expr(Role::App, 2, code).unwrap();
        prop.set_f32(Role::App, 3, 5.).unwrap();

        node.set_property_u32(Role::App, "z_index", 2).unwrap();

        // Setup the pimpl
        let (x1, y1) = (0., 0.);
        let (x2, y2) = (1., 1.);
        let verts = vec![
            // top left
            Vertex { pos: [x1, y1], color: [0., 1., 0., 1.], uv: [0., 0.] },
            // top right
            Vertex { pos: [x2, y1], color: [0., 1., 0., 1.], uv: [1., 0.] },
            // bottom left
            Vertex { pos: [x1, y2], color: [0., 1., 0., 1.], uv: [0., 1.] },
            // bottom right
            Vertex { pos: [x2, y2], color: [0., 1., 0., 1.], uv: [1., 1.] },
        ];
        let indices = vec![0, 2, 1, 1, 2, 3];
        drop(sg);
        let pimpl = Mesh::new(
            self.ex.clone(),
            self.sg.clone(),
            node_id,
            self.render_api.clone(),
            verts,
            indices,
        )
        .await;
        let mut sg = self.sg.lock().await;
        let node = sg.get_node_mut(node_id).unwrap();
        node.pimpl = pimpl;

        sg.link(node_id, layer_node_id).unwrap();

        // Debugging tool
        let node_id = create_mesh(&mut sg, "debugtool2");

        let node = sg.get_node_mut(node_id).unwrap();
        let prop = node.get_property("rect").unwrap();
        prop.set_f32(Role::App, 0, 0.).unwrap();
        let code = vec![Op::Sub((
            Box::new(Op::LoadVar("h".to_string())),
            Box::new(Op::ConstFloat32(200.)),
        ))];
        prop.set_expr(Role::App, 1, code).unwrap();
        let code = vec![Op::LoadVar("w".to_string())];
        prop.set_expr(Role::App, 2, code).unwrap();
        prop.set_f32(Role::App, 3, 5.).unwrap();

        node.set_property_u32(Role::App, "z_index", 2).unwrap();

        // Setup the pimpl
        let (x1, y1) = (0., 0.);
        let (x2, y2) = (1., 1.);
        let verts = vec![
            // top left
            Vertex { pos: [x1, y1], color: [0., 1., 0., 1.], uv: [0., 0.] },
            // top right
            Vertex { pos: [x2, y1], color: [0., 1., 0., 1.], uv: [1., 0.] },
            // bottom left
            Vertex { pos: [x1, y2], color: [0., 1., 0., 1.], uv: [0., 1.] },
            // bottom right
            Vertex { pos: [x2, y2], color: [0., 1., 0., 1.], uv: [1., 1.] },
        ];
        let indices = vec![0, 2, 1, 1, 2, 3];
        drop(sg);
        let pimpl = Mesh::new(
            self.ex.clone(),
            self.sg.clone(),
            node_id,
            self.render_api.clone(),
            verts,
            indices,
        )
        .await;
        let mut sg = self.sg.lock().await;
        let node = sg.get_node_mut(node_id).unwrap();
        node.pimpl = pimpl;

        sg.link(node_id, layer_node_id).unwrap();

        // Create KING GNU!
        let node_id = create_image(&mut sg, "king");

        let node = sg.get_node_mut(node_id).unwrap();
        let prop = node.get_property("rect").unwrap();
        prop.set_f32(Role::App, 0, 80.).unwrap();
        prop.set_f32(Role::App, 1, 10.).unwrap();
        prop.set_f32(Role::App, 2, 60.).unwrap();
        prop.set_f32(Role::App, 3, 60.).unwrap();

        node.set_property_str(Role::App, "path", KING_PATH).unwrap();

        // Setup the pimpl
        drop(sg);
        let pimpl =
            Image::new(self.ex.clone(), self.sg.clone(), node_id, self.render_api.clone()).await;
        let mut sg = self.sg.lock().await;
        let node = sg.get_node_mut(node_id).unwrap();
        node.pimpl = pimpl;

        sg.link(node_id, layer_node_id).unwrap();

        // Create some text
        let node_id = create_text(&mut sg, "label");

        let node = sg.get_node_mut(node_id).unwrap();
        let prop = node.get_property("rect").unwrap();
        prop.set_f32(Role::App, 0, 100.).unwrap();
        prop.set_f32(Role::App, 1, 100.).unwrap();
        prop.set_f32(Role::App, 2, 800.).unwrap();
        prop.set_f32(Role::App, 3, 200.).unwrap();
        node.set_property_f32(Role::App, "baseline", 40.).unwrap();
        node.set_property_f32(Role::App, "font_size", 60.).unwrap();
        node.set_property_str(Role::App, "text", "anon1🍆").unwrap();
        //node.set_property_str(Role::App, "text", "anon1").unwrap();
        let prop = node.get_property("text_color").unwrap();
        prop.set_f32(Role::App, 0, 0.).unwrap();
        prop.set_f32(Role::App, 1, 1.).unwrap();
        prop.set_f32(Role::App, 2, 0.).unwrap();
        prop.set_f32(Role::App, 3, 1.).unwrap();

        drop(sg);
        let pimpl = Text::new(
            self.ex.clone(),
            self.sg.clone(),
            node_id,
            self.render_api.clone(),
            self.text_shaper.clone(),
        )
        .await;
        let mut sg = self.sg.lock().await;
        let node = sg.get_node_mut(node_id).unwrap();
        node.pimpl = pimpl;

        sg.link(node_id, layer_node_id).unwrap();

        // Text edit
        let node_id = create_editbox(&mut sg, "editz");
        let node = sg.get_node(node_id).unwrap();
        node.set_property_bool(Role::App, "is_active", true).unwrap();
        let prop = node.get_property("rect").unwrap();
        prop.set_f32(Role::App, 0, 150.).unwrap();
        prop.set_f32(Role::App, 1, 150.).unwrap();
        prop.set_f32(Role::App, 2, 380.).unwrap();
        //let code = vec![Op::Sub((
        //    Box::new(Op::LoadVar("h".to_string())),
        //    Box::new(Op::ConstFloat32(60.)),
        //))];
        //prop.set_expr(Role::App, 1, code).unwrap();
        //let code = vec![Op::Sub((
        //    Box::new(Op::LoadVar("w".to_string())),
        //    Box::new(Op::ConstFloat32(120.)),
        //))];
        //prop.set_expr(Role::App, 2, code).unwrap();
        prop.set_f32(Role::App, 3, 60.).unwrap();
        node.set_property_f32(Role::App, "baseline", 40.).unwrap();
        node.set_property_f32(Role::App, "font_size", 20.).unwrap();
        node.set_property_f32(Role::App, "font_size", 40.).unwrap();
        node.set_property_str(Role::App, "text", "hello king!😁🍆jelly 🍆1234").unwrap();
        let prop = node.get_property("text_color").unwrap();
        if LIGHTMODE {
            prop.set_f32(Role::App, 0, 0.).unwrap();
            prop.set_f32(Role::App, 1, 0.).unwrap();
            prop.set_f32(Role::App, 2, 0.).unwrap();
            prop.set_f32(Role::App, 3, 1.).unwrap();
        } else {
            prop.set_f32(Role::App, 0, 1.).unwrap();
            prop.set_f32(Role::App, 1, 1.).unwrap();
            prop.set_f32(Role::App, 2, 1.).unwrap();
            prop.set_f32(Role::App, 3, 1.).unwrap();
        }
        let prop = node.get_property("cursor_color").unwrap();
        prop.set_f32(Role::App, 0, 1.).unwrap();
        prop.set_f32(Role::App, 1, 0.5).unwrap();
        prop.set_f32(Role::App, 2, 0.5).unwrap();
        prop.set_f32(Role::App, 3, 1.).unwrap();
        let prop = node.get_property("hi_bg_color").unwrap();
        if LIGHTMODE {
            prop.set_f32(Role::App, 0, 0.5).unwrap();
            prop.set_f32(Role::App, 1, 0.5).unwrap();
            prop.set_f32(Role::App, 2, 0.5).unwrap();
            prop.set_f32(Role::App, 3, 1.).unwrap();
        } else {
            prop.set_f32(Role::App, 0, 1.).unwrap();
            prop.set_f32(Role::App, 1, 1.).unwrap();
            prop.set_f32(Role::App, 2, 1.).unwrap();
            prop.set_f32(Role::App, 3, 0.5).unwrap();
        }
        let prop = node.get_property("selected").unwrap();
        prop.set_null(Role::App, 0).unwrap();
        prop.set_null(Role::App, 1).unwrap();
        node.set_property_u32(Role::App, "z_index", 1).unwrap();
        //node.set_property_bool(Role::App, "debug", true).unwrap();

        let editbox_text = PropertyStr::wrap(node, Role::App, "text", 0).unwrap();
        let editbox_focus = PropertyBool::wrap(node, Role::App, "is_focused", 0).unwrap();
        let task = self.ex.spawn(async move {
            while let Ok(_) = btn_click_recvr.recv().await {
                let text = editbox_text.get();
                editbox_text.prop().unset(Role::App, 0);
                // Clicking outside the editbox makes it lose focus
                // So lets focus it again
                editbox_focus.set(true);
                debug!(target: "app", "sending text {text}");
            }
        });
        tasks.push(task);

        drop(sg);
        let pimpl = EditBox::new(
            self.ex.clone(),
            self.sg.clone(),
            node_id,
            self.render_api.clone(),
            self.event_pub.clone(),
            self.text_shaper.clone(),
        )
        .await;
        let mut sg = self.sg.lock().await;
        let node = sg.get_node_mut(node_id).unwrap();
        node.pimpl = pimpl;

        sg.link(node_id, layer_node_id).unwrap();

        // ChatView
        let (node_id, recvr) = create_chatview(&mut sg, "chatty");
        let node = sg.get_node(node_id).unwrap();
        let prop = node.get_property("rect").unwrap();
        prop.set_f32(Role::App, 0, 0.).unwrap();
        let code =
            vec![Op::Div((Box::new(Op::LoadVar("h".to_string())), Box::new(Op::ConstFloat32(2.))))];
        prop.set_expr(Role::App, 1, code).unwrap();
        let code = vec![Op::LoadVar("w".to_string())];
        prop.set_expr(Role::App, 2, code).unwrap();
        let code = vec![Op::Sub((
            Box::new(Op::Div((
                Box::new(Op::LoadVar("h".to_string())),
                Box::new(Op::ConstFloat32(2.)),
            ))),
            Box::new(Op::ConstFloat32(200.)),
        ))];
        prop.set_expr(Role::App, 3, code).unwrap();
        node.set_property_f32(Role::App, "font_size", 20.).unwrap();
        node.set_property_f32(Role::App, "line_height", 30.).unwrap();
        node.set_property_f32(Role::App, "baseline", 20.).unwrap();
        node.set_property_u32(Role::App, "z_index", 1).unwrap();
        //node.set_property_bool(Role::App, "debug", true).unwrap();

        let prop = node.get_property("timestamp_color").unwrap();
        prop.set_f32(Role::App, 0, 0.5).unwrap();
        prop.set_f32(Role::App, 1, 0.5).unwrap();
        prop.set_f32(Role::App, 2, 0.5).unwrap();
        prop.set_f32(Role::App, 3, 0.5).unwrap();
        let prop = node.get_property("text_color").unwrap();
        if LIGHTMODE {
            prop.set_f32(Role::App, 0, 0.).unwrap();
            prop.set_f32(Role::App, 1, 0.).unwrap();
            prop.set_f32(Role::App, 2, 0.).unwrap();
            prop.set_f32(Role::App, 3, 1.).unwrap();
        } else {
            prop.set_f32(Role::App, 0, 1.).unwrap();
            prop.set_f32(Role::App, 1, 1.).unwrap();
            prop.set_f32(Role::App, 2, 1.).unwrap();
            prop.set_f32(Role::App, 3, 1.).unwrap();
        }

        let prop = node.get_property("nick_colors").unwrap();
        #[rustfmt::skip]
        let nick_colors = [
            0.00, 0.94, 1.00, 1.,
            0.36, 1.00, 0.69, 1.,
            0.29, 1.00, 0.45, 1.,
            0.00, 0.73, 0.38, 1.,
            0.21, 0.67, 0.67, 1.,
            0.56, 0.61, 1.00, 1.,
            0.84, 0.48, 1.00, 1.,
            1.00, 0.61, 0.94, 1.,
            1.00, 0.36, 0.48, 1.,
            1.00, 0.30, 0.00, 1.
        ];
        for c in nick_colors {
            prop.push_f32(Role::App, c).unwrap();
        }

        drop(sg);
        let db = sled::open(CHATDB_PATH).expect("cannot open sleddb");
        let chat_tree = db.open_tree(b"chat").unwrap();
        if chat_tree.is_empty() {
            populate_tree(&chat_tree);
        }
        debug!(target: "app", "db has {} lines", chat_tree.len());
        let pimpl = ChatView::new(
            self.ex.clone(),
            self.sg.clone(),
            node_id,
            self.render_api.clone(),
            self.event_pub.clone(),
            self.text_shaper.clone(),
            chat_tree,
            recvr,
        )
        .await;
        let mut sg = self.sg.lock().await;
        let node = sg.get_node_mut(node_id).unwrap();
        node.pimpl = pimpl;

        sg.link(node_id, layer_node_id).unwrap();

        // On android lets scale the UI up
        // TODO: add support for fractional scaling
        // This also affects mouse/touch input since coords need to be accurately translated
        // Also we need to think about nesting of layers.
        //let window_node = sg.get_node_mut(window_id).unwrap();
        //win_node.set_property_f32(Role::App, "scale", 1.6).unwrap();

        *self.tasks.lock().unwrap() = tasks;
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

impl Drop for App {
    fn drop(&mut self) {
        debug!(target: "app", "dropping app");
    }
}

// Just for testing
fn populate_tree(tree: &sled::Tree) {
    let chat_txt = include_str!("../chat.txt");
    for line in chat_txt.lines() {
        let parts: Vec<&str> = line.splitn(3, ' ').collect();
        assert_eq!(parts.len(), 3);
        let time_parts: Vec<&str> = parts[0].splitn(2, ':').collect();
        let (hour, min) = (time_parts[0], time_parts[1]);
        let hour = hour.parse::<u32>().unwrap();
        let min = min.parse::<u32>().unwrap();
        let dt: NaiveDateTime =
            NaiveDate::from_ymd_opt(2024, 8, 6).unwrap().and_hms_opt(hour, min, 0).unwrap();
        let timest = dt.and_utc().timestamp() as u64;

        let message_id = [0u8; 32];
        let nick = parts[1].to_string();
        let text = parts[2].to_string();

        // serial order is important here
        let timest = timest.to_be_bytes();
        assert_eq!(timest.len(), 8);
        let mut key = [0u8; 8 + 32];
        key[..8].clone_from_slice(&timest);

        let msg = chatview::ChatMsg { nick, text };
        let mut val = vec![];
        msg.encode(&mut val).unwrap();

        tree.insert(&key, val).unwrap();
    }
    // O(n)
    debug!(target: "app", "populated db with {} lines", tree.len());
}

pub fn create_layer(sg: &mut SceneGraph, name: &str) -> SceneNodeId {
    debug!(target: "app", "create_layer({name})");
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
    debug!(target: "app", "create_mesh({name})");
    let node = sg.add_node(name, SceneNodeType::RenderMesh);

    let mut prop = Property::new("rect", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_array_len(4);
    prop.allow_exprs();
    node.add_property(prop).unwrap();

    let prop = Property::new("z_index", PropertyType::Uint32, PropertySubType::Null);
    node.add_property(prop).unwrap();

    node.id
}

pub fn create_button(sg: &mut SceneGraph, name: &str) -> SceneNodeId {
    debug!(target: "app", "create_button({name})");
    let node = sg.add_node(name, SceneNodeType::Button);

    let mut prop = Property::new("is_active", PropertyType::Bool, PropertySubType::Null);
    prop.set_ui_text("Is Active", "An active Button can be clicked");
    node.add_property(prop).unwrap();

    let mut prop = Property::new("rect", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_array_len(4);
    prop.allow_exprs();
    node.add_property(prop).unwrap();

    node.add_signal("click", "Button clicked event", vec![]).unwrap();

    node.id
}

pub fn create_image(sg: &mut SceneGraph, name: &str) -> SceneNodeId {
    debug!(target: "app", "create_image({name})");
    let node = sg.add_node(name, SceneNodeType::RenderMesh);

    let mut prop = Property::new("rect", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_array_len(4);
    prop.allow_exprs();
    node.add_property(prop).unwrap();

    let prop = Property::new("z_index", PropertyType::Uint32, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let prop = Property::new("path", PropertyType::Str, PropertySubType::Null);
    node.add_property(prop).unwrap();

    node.id
}

fn create_text(sg: &mut SceneGraph, name: &str) -> SceneNodeId {
    debug!(target: "app", "create_text({name})");
    let node = sg.add_node(name, SceneNodeType::RenderText);

    let mut prop = Property::new("rect", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_array_len(4);
    prop.allow_exprs();
    node.add_property(prop).unwrap();

    let prop = Property::new("baseline", PropertyType::Float32, PropertySubType::Pixel);
    node.add_property(prop).unwrap();

    let prop = Property::new("font_size", PropertyType::Float32, PropertySubType::Pixel);
    node.add_property(prop).unwrap();

    let prop = Property::new("text", PropertyType::Str, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("text_color", PropertyType::Float32, PropertySubType::Color);
    prop.set_array_len(4);
    prop.set_range_f32(0., 1.);
    node.add_property(prop).unwrap();

    let prop = Property::new("z_index", PropertyType::Uint32, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let prop = Property::new("debug", PropertyType::Bool, PropertySubType::Null);
    node.add_property(prop).unwrap();

    node.id
}

fn create_editbox(sg: &mut SceneGraph, name: &str) -> SceneNodeId {
    debug!(target: "app", "create_editbox({name})");
    let node = sg.add_node(name, SceneNodeType::EditBox);

    let mut prop = Property::new("is_active", PropertyType::Bool, PropertySubType::Null);
    prop.set_ui_text("Is Active", "An active EditBox can be focused");
    node.add_property(prop).unwrap();

    let mut prop = Property::new("is_focused", PropertyType::Bool, PropertySubType::Null);
    prop.set_ui_text("Is Focused", "A focused EditBox receives input");
    node.add_property(prop).unwrap();

    let mut prop = Property::new("rect", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_array_len(4);
    prop.allow_exprs();
    node.add_property(prop).unwrap();

    let mut prop = Property::new("baseline", PropertyType::Float32, PropertySubType::Pixel);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("scroll", PropertyType::Float32, PropertySubType::Pixel);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("cursor_pos", PropertyType::Uint32, PropertySubType::Pixel);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("font_size", PropertyType::Float32, PropertySubType::Pixel);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("text", PropertyType::Str, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("text_color", PropertyType::Float32, PropertySubType::Color);
    prop.set_array_len(4);
    prop.set_range_f32(0., 1.);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("cursor_color", PropertyType::Float32, PropertySubType::Color);
    prop.set_array_len(4);
    prop.set_range_f32(0., 1.);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("hi_bg_color", PropertyType::Float32, PropertySubType::Color);
    prop.set_array_len(4);
    prop.set_range_f32(0., 1.);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("selected", PropertyType::Uint32, PropertySubType::Color);
    prop.set_array_len(2);
    prop.allow_null_values();
    node.add_property(prop).unwrap();

    let mut prop = Property::new("z_index", PropertyType::Uint32, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("debug", PropertyType::Bool, PropertySubType::Null);
    node.add_property(prop).unwrap();

    node.id
}

fn create_chatview(
    sg: &mut SceneGraph,
    name: &str,
) -> (SceneNodeId, async_channel::Receiver<Vec<u8>>) {
    debug!(target: "app", "create_chatview({name})");
    let node = sg.add_node(name, SceneNodeType::ChatView);

    let mut prop = Property::new("rect", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_array_len(4);
    prop.allow_exprs();
    node.add_property(prop).unwrap();

    let mut prop = Property::new("scroll", PropertyType::Float32, PropertySubType::Null);
    prop.set_ui_text("Scroll", "Scroll up from the bottom");
    node.add_property(prop).unwrap();

    let prop = Property::new("font_size", PropertyType::Float32, PropertySubType::Pixel);
    node.add_property(prop).unwrap();

    let prop = Property::new("line_height", PropertyType::Float32, PropertySubType::Pixel);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("timestamp_color", PropertyType::Float32, PropertySubType::Color);
    prop.set_array_len(4);
    prop.set_range_f32(0., 1.);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("text_color", PropertyType::Float32, PropertySubType::Color);
    prop.set_array_len(4);
    prop.set_range_f32(0., 1.);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("nick_colors", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_unbounded();
    prop.set_range_f32(0., 1.);
    node.add_property(prop).unwrap();

    let prop = Property::new("baseline", PropertyType::Float32, PropertySubType::Pixel);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("z_index", PropertyType::Uint32, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let mut prop = Property::new("debug", PropertyType::Bool, PropertySubType::Null);
    node.add_property(prop).unwrap();

    let mut prop =
        Property::new("mouse_scroll_start_accel", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_ui_text("Mouse Scroll Start Acceleration", "Initial acceperation when scrolling");
    prop.set_defaults_f32(vec![4.]).unwrap();
    node.add_property(prop).unwrap();

    let mut prop =
        Property::new("mouse_scroll_decel", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_ui_text(
        "Mouse Scroll Deceleration",
        "Deceleration factor for mouse scroll acceleration",
    );
    prop.set_range_f32(0., 1.);
    prop.set_defaults_f32(vec![0.5]).unwrap();
    node.add_property(prop).unwrap();

    let mut prop =
        Property::new("mouse_scroll_resist", PropertyType::Float32, PropertySubType::Pixel);
    prop.set_ui_text("Mouse Scroll Resistance", "How quickly scrolling speed is dampened");
    prop.set_range_f32(0., 1.);
    prop.set_defaults_f32(vec![0.9]).unwrap();
    node.add_property(prop).unwrap();

    let (sender, recvr) = async_channel::unbounded::<Vec<u8>>();
    let method = move |data: Vec<u8>, response_fn: MethodResponseFn| {
        if sender.try_send(data).is_err() {
            response_fn(Err(Error::ChannelClosed));
        } else {
            response_fn(Ok(vec![]));
        }
    };
    node.add_method(
        "insert_line",
        vec![
            ("timestamp", "Timestamp", CallArgType::Uint64),
            ("id", "Message ID", CallArgType::Hash),
            ("nick", "Nickname", CallArgType::Str),
            ("text", "Text", CallArgType::Str),
        ],
        vec![],
        Box::new(method),
    )
    .unwrap();

    (node.id, recvr)
}
