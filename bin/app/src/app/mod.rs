/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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
use darkfi_serial::{deserialize, Decodable, Encodable};
use futures::{stream::FuturesUnordered, StreamExt};
use sled_overlay::sled;
use smol::Task;
use std::{
    fs::File,
    io::Cursor,
    sync::{Arc, Mutex as SyncMutex},
    thread,
};

use crate::{
    error::Error,
    expr::Op,
    gfx::{GraphicsEventPublisherPtr, RenderApi, Vertex},
    plugin::{self, PluginObject, PluginSettings},
    prop::{
        Property, PropertyAtomicGuard, PropertyBool, PropertyStr, PropertySubType, PropertyType,
        PropertyValue, Role,
    },
    scene::{Pimpl, SceneNode as SceneNode3, SceneNodePtr, SceneNodeType as SceneNodeType3, Slot},
    text::TextShaperPtr,
    ui::{chatview, Window},
    ExecutorPtr,
};

mod node;
use node::create_darkirc;
mod schema;
use schema::{get_settingsdb_path, get_window_scale_filename, settings};

macro_rules! d { ($($arg:tt)*) => { debug!(target: "app", $($arg)*); } }
macro_rules! t { ($($arg:tt)*) => { trace!(target: "app", $($arg)*); } }
macro_rules! i { ($($arg:tt)*) => { info!(target: "app", $($arg)*); } }
macro_rules! w { ($($arg:tt)*) => { warn!(target: "app", $($arg)*); } }
macro_rules! e { ($($arg:tt)*) => { error!(target: "app", $($arg)*); } }

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
        d!("Stopping async runtime...");

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
        i!("Stopped app");
    }
}

pub type AppPtr = Arc<App>;

pub struct App {
    pub sg_root: SceneNodePtr,
    pub render_api: RenderApi,
    pub event_pub: GraphicsEventPublisherPtr,
    pub text_shaper: TextShaperPtr,
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
            tasks: SyncMutex::new(vec![]),
        })
    }

    /// Does not require miniquad to be init. Created the scene graph tree / schema and all
    /// the objects.
    pub async fn setup(&self) -> Result<Option<i32>, Error> {
        t!("App::setup()");

        let db_path = get_settingsdb_path();
        let db = match sled::open(&db_path) {
            Ok(db) => db,
            Err(err) => {
                e!("Sled database '{}' failed to open: {err}!", db_path.display());
                return Err(Error::SledDbErr)
            }
        };

        let mut window = SceneNode3::new("window", SceneNodeType3::Window);

        let mut prop = Property::new("screen_size", PropertyType::Float32, PropertySubType::Pixel);
        prop.set_array_len(2);
        window.add_property(prop).unwrap();

        let setting_root = SceneNode3::new("setting", SceneNodeType3::SettingRoot);
        let setting_root = setting_root.setup_null();
        let settings_tree = db.open_tree("settings").unwrap();
        let settings = Arc::new(PluginSettings {
            setting_root: setting_root.clone(),
            sled_tree: settings_tree,
        });

        settings.add_setting("scale", PropertyValue::Float32(1.));
        settings.load_settings();

        // Save app settings in sled when they change
        for setting_node in settings.setting_root.get_children().iter() {
            let setting_sub = setting_node.get_property("value").unwrap().subscribe_modify();
            let settings2 = settings.clone();
            let setting_task = self.ex.spawn(async move {
                while let Ok(_) = setting_sub.receive().await {
                    settings2.save_settings();
                }
            });
            self.tasks.lock().unwrap().push(setting_task);
        }

        let window =
            window.setup(|me| Window::new(me, self.render_api.clone(), setting_root.clone())).await;

        self.sg_root.clone().link(window.clone());
        self.sg_root.clone().link(setting_root.clone());

        schema::test::make(&self, window.clone()).await;

        d!("Schema loaded");

        let plugin = SceneNode3::new("plugin", SceneNodeType3::PluginRoot);
        let plugin = plugin.setup_null();
        self.sg_root.clone().link(plugin.clone());

        #[cfg(feature = "enable-plugins")]
        self.load_plugins(plugin).await;

        #[cfg(not(feature = "enable-plugins"))]
        w!("Plugins are disabled in this build");

        settings::make(&self, window, self.ex.clone()).await;

        Ok(None)
    }

    #[cfg(feature = "enable-plugins")]
    async fn load_plugins(&self, plugin: SceneNodePtr) {
        let darkirc = create_darkirc("darkirc");
        let darkirc = darkirc
            .setup(|me| async {
                plugin::DarkIrc::new(me, self.ex.clone()).await.expect("DarkIrc pimpl setup")
            })
            .await;

        let (slot, recvr) = Slot::new("recvmsg");
        darkirc.register("recv", slot).unwrap();
        let sg_root2 = self.sg_root.clone();
        let darkirc_nick = PropertyStr::wrap(&darkirc, Role::App, "nick", 0).unwrap();
        let listen_recv = self.ex.spawn(async move {
            while let Ok(data) = recvr.recv().await {
                let atom = &mut PropertyAtomicGuard::new();

                let mut cur = Cursor::new(&data);
                let channel = String::decode(&mut cur).unwrap();
                let timestamp = chatview::Timestamp::decode(&mut cur).unwrap();
                let id = chatview::MessageId::decode(&mut cur).unwrap();
                let nick = String::decode(&mut cur).unwrap();
                let msg = String::decode(&mut cur).unwrap();

                let node_path = format!("/window/{channel}_chat_layer/content/chatty");
                t!("Attempting to relay message to {node_path}");
                let Some(chatview) = sg_root2.clone().lookup_node(&node_path) else {
                    d!("Ignoring message since {node_path} doesn't exist");
                    continue
                };

                // I prefer to just re-encode because the code is clearer.
                let mut data = vec![];
                timestamp.encode(&mut data).unwrap();
                id.encode(&mut data).unwrap();
                nick.encode(&mut data).unwrap();
                msg.encode(&mut data).unwrap();
                if let Err(err) = chatview.call_method("insert_line", data).await {
                    error!(
                        target: "app",
                        "Call method {node_path}::insert_line({timestamp}, {id}, {nick}, '{msg}'): {err:?}"
                    );
                }

                // Apply coloring when you get a message
                let chat_path = format!("/window/{channel}_chat_layer");
                let chat_layer = sg_root2.clone().lookup_node(chat_path).unwrap();
                if chat_layer.get_property_bool("is_visible").unwrap() {
                    continue
                }

                let node_path = format!("/window/menu_layer/{channel}_channel_label");
                let menu_label = sg_root2.clone().lookup_node(&node_path).unwrap();
                let prop = menu_label.get_property("text_color").unwrap();
                if msg.contains(&darkirc_nick.get()) {
                    // Nick highlight
                    prop.clone().set_f32(atom, Role::App, 0, 0.56).unwrap();
                    prop.clone().set_f32(atom, Role::App, 1, 0.61).unwrap();
                    prop.clone().set_f32(atom, Role::App, 2, 1.).unwrap();
                    prop.clone().set_f32(atom, Role::App, 3, 1.).unwrap();
                } else {
                    // Normal channel activity
                    prop.clone().set_f32(atom, Role::App, 0, 0.36).unwrap();
                    prop.clone().set_f32(atom, Role::App, 1, 1.).unwrap();
                    prop.clone().set_f32(atom, Role::App, 2, 0.51).unwrap();
                    prop.clone().set_f32(atom, Role::App, 3, 1.).unwrap();
                }
            }
        });
        self.tasks.lock().unwrap().push(listen_recv);

        let (slot, recvr) = Slot::new("connect");
        darkirc.register("connect", slot).unwrap();
        let sg_root2 = self.sg_root.clone();
        let listen_connect = self.ex.spawn(async move {
            let net0 = sg_root2.clone().lookup_node("/window/netstatus_layer/net0").unwrap();
            let net1 = sg_root2.clone().lookup_node("/window/netstatus_layer/net1").unwrap();
            let net2 = sg_root2.clone().lookup_node("/window/netstatus_layer/net2").unwrap();
            let net3 = sg_root2.clone().lookup_node("/window/netstatus_layer/net3").unwrap();

            let net0_is_visible = PropertyBool::wrap(&net0, Role::App, "is_visible", 0).unwrap();
            let net1_is_visible = PropertyBool::wrap(&net1, Role::App, "is_visible", 0).unwrap();
            let net2_is_visible = PropertyBool::wrap(&net2, Role::App, "is_visible", 0).unwrap();
            let net3_is_visible = PropertyBool::wrap(&net3, Role::App, "is_visible", 0).unwrap();

            while let Ok(data) = recvr.recv().await {
                let (peers_count, is_dag_synced): (u32, bool) = deserialize(&data).unwrap();

                let atom = &mut PropertyAtomicGuard::new();

                if peers_count == 0 {
                    net0_is_visible.set(atom, true);
                    net1_is_visible.set(atom, false);
                    net2_is_visible.set(atom, false);
                    net3_is_visible.set(atom, false);
                    continue
                }

                assert!(peers_count > 0);
                if !is_dag_synced {
                    net0_is_visible.set(atom, false);
                    net1_is_visible.set(atom, true);
                    net2_is_visible.set(atom, false);
                    net3_is_visible.set(atom, false);
                    continue
                }

                assert!(peers_count > 0 && is_dag_synced);
                if peers_count == 1 {
                    net0_is_visible.set(atom, false);
                    net1_is_visible.set(atom, false);
                    net2_is_visible.set(atom, true);
                    net3_is_visible.set(atom, false);
                    continue
                }

                net0_is_visible.set(atom, false);
                net1_is_visible.set(atom, false);
                net2_is_visible.set(atom, false);
                net3_is_visible.set(atom, true);
            }
        });
        self.tasks.lock().unwrap().push(listen_connect);

        plugin.link(darkirc);

        i!("Plugins loaded");
    }

    /// Begins the draw of the tree, and then starts the UI procs.
    pub async fn start(self: Arc<Self>) {
        d!("Starting app");
        let atom = &mut PropertyAtomicGuard::new();

        let window_node = self.sg_root.clone().lookup_node("/window").unwrap();
        let prop = window_node.get_property("screen_size").unwrap();
        // We can only do this once the window has been created in miniquad.
        let (screen_width, screen_height) = miniquad::window::screen_size();
        prop.clone().set_f32(atom, Role::App, 0, screen_width);
        prop.clone().set_f32(atom, Role::App, 1, screen_height);

        drop(atom);

        // Access drawable in window node and call draw()
        self.trigger_draw().await;

        self.start_procs().await;
        i!("App started");
    }

    pub fn stop(&self) {
        smol::future::block_on(async {
            self.async_stop().await;
        });
    }

    async fn trigger_draw(&self) {
        let window_node = self.sg_root.clone().lookup_node("/window").expect("no window attached!");
        match window_node.pimpl() {
            Pimpl::Window(win) => win.draw().await,
            _ => panic!("wrong pimpl"),
        }
    }
    async fn start_procs(&self) {
        let window_node = self.sg_root.clone().lookup_node("/window").unwrap();
        match window_node.pimpl() {
            Pimpl::Window(win) => win.clone().start(self.event_pub.clone(), self.ex.clone()).await,
            _ => panic!("wrong pimpl"),
        }

        let plugins = self.sg_root.clone().lookup_node("/plugin").unwrap();
        for plugin in plugins.get_children() {
            match plugin.pimpl() {
                Pimpl::DarkIrc(darkirc) => darkirc.clone().start(self.ex.clone()).await,
                _ => panic!("wrong pimpl"),
            }
        }
    }

    /// Shutdown code here
    async fn async_stop(&self) {
        //self.darkirc_backend.stop().await;
    }
}

impl Drop for App {
    fn drop(&mut self) {
        t!("Dropping app");
        // This hangs
        //self.stop();
    }
}

// Just for testing
fn populate_tree(tree: &sled::Tree) {
    let chat_txt = include_str!("../../data/chat.txt");
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
    d!("populated db with {} lines", tree.len());
}
