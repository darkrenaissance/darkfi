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
use chrono::{Local, NaiveDate, NaiveDateTime, TimeZone};
use darkfi::system::CondVar;
use darkfi_serial::Encodable;
use futures::{stream::FuturesUnordered, StreamExt};
use sled_overlay::sled;
use smol::Task;
use std::{
    sync::{Arc, Mutex as SyncMutex},
    thread,
};

use crate::{
    darkirc::DarkIrcBackendPtr,
    error::Error,
    expr::Op,
    gfx::{GraphicsEventPublisherPtr, RenderApi, Vertex},
    prop::{Property, PropertyBool, PropertyStr, PropertySubType, PropertyType, Role},
    scene::{Pimpl, SceneNode as SceneNode3, SceneNodePtr, SceneNodeType as SceneNodeType3},
    text::TextShaperPtr,
    ui::{chatview, Window},
    ExecutorPtr,
};

mod node;
mod schema;

//fn print_type_of<T>(_: &T) {
//    println!("{}", std::any::type_name::<T>())
//}

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
        info!(target: "async_runtime", "Started runtime [{n_threads} threads]");
    }

    pub fn push_task(&self, task: Task<()>) {
        self.tasks.lock().unwrap().push(task);
    }

    pub fn stop(&self) {
        // Go through event graph and call stop on everything
        // Depth first
        debug!(target: "app", "Stopping async runtime...");

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
    pub sg_root: SceneNodePtr,
    pub render_api: RenderApi,
    pub event_pub: GraphicsEventPublisherPtr,
    pub text_shaper: TextShaperPtr,
    pub darkirc_evgr: SyncMutex<Option<DarkIrcBackendPtr>>,
    pub tasks: SyncMutex<Vec<Task<()>>>,
    pub ex: ExecutorPtr,
}

impl App {
    pub fn new(
        sg_root: SceneNodePtr,
        render_api: RenderApi,
        event_pub: GraphicsEventPublisherPtr,
        text_shaper: TextShaperPtr,
        ex: ExecutorPtr,
    ) -> Arc<Self> {
        Arc::new(Self {
            sg_root,
            ex,
            render_api,
            event_pub,
            text_shaper,
            darkirc_evgr: SyncMutex::new(None),
            tasks: SyncMutex::new(vec![]),
        })
    }

    /// Does not require miniquad to be init. Created the scene graph tree / schema and all
    /// the objects.
    pub async fn setup(&self) {
        debug!(target: "app", "App::setup()");

        let mut window = SceneNode3::new("window", SceneNodeType3::Window);

        let mut prop = Property::new("screen_size", PropertyType::Float32, PropertySubType::Pixel);
        prop.set_array_len(2);
        window.add_property(prop).unwrap();

        let mut prop = Property::new("scale", PropertyType::Float32, PropertySubType::Pixel);
        prop.set_defaults_f32(vec![1.]).unwrap();
        window.add_property(prop).unwrap();

        let window = window.setup(|me| Window::new(me, self.render_api.clone())).await;
        self.sg_root.clone().link(window.clone());
        schema::make(&self, window).await;

        debug!(target: "app", "Schema loaded");
    }

    /// Begins the draw of the tree, and then starts the UI procs.
    pub async fn start(self: Arc<Self>) {
        debug!(target: "app", "App::start()");

        let window_node = self.sg_root.clone().lookup_node("/window").unwrap();
        let prop = window_node.get_property("screen_size").unwrap();
        // We can only do this once the window has been created in miniquad.
        let (screen_width, screen_height) = miniquad::window::screen_size();
        prop.set_f32(Role::App, 0, screen_width);
        prop.set_f32(Role::App, 1, screen_height);

        // Access drawable in window node and call draw()
        self.trigger_draw().await;

        self.start_procs().await;
        debug!(target: "app", "App started");
    }

    pub fn stop(&self) {
        smol::future::block_on(async {
            self.async_stop().await;
        });
    }

    async fn trigger_draw(&self) {
        let window_node = self.sg_root.clone().lookup_node("/window").expect("no window attached!");
        match &window_node.pimpl {
            Pimpl::Window(win) => win.draw().await,
            _ => panic!("wrong pimpl"),
        }
    }
    async fn start_procs(&self) {
        let window_node = self.sg_root.clone().lookup_node("/window").unwrap();
        match &window_node.pimpl {
            Pimpl::Window(win) => win.clone().start(self.event_pub.clone(), self.ex.clone()).await,
            _ => panic!("wrong pimpl"),
        }
    }

    /// Shutdown code here
    async fn async_stop(&self) {
        //self.darkirc_backend.stop().await;
    }
}

impl Drop for App {
    fn drop(&mut self) {
        debug!(target: "app", "Dropping app");
        // This hangs
        //self.stop();
    }
}

// Just for testing
fn populate_tree(tree: &sled::Tree) {
    let chat_txt = include_str!("../../chat.txt");
    for line in chat_txt.lines() {
        let parts: Vec<&str> = line.splitn(3, ' ').collect();
        assert_eq!(parts.len(), 3);
        let time_parts: Vec<&str> = parts[0].splitn(2, ':').collect();
        let (hour, min) = (time_parts[0], time_parts[1]);
        let hour = hour.parse::<u32>().unwrap();
        let min = min.parse::<u32>().unwrap();
        let dt: NaiveDateTime =
            NaiveDate::from_ymd_opt(2024, 8, 6).unwrap().and_hms_opt(hour, min, 0).unwrap();
        let timest = dt.and_utc().timestamp_millis() as u64;

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
