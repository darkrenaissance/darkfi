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

#[cfg(target_os = "android")]
use crate::android;

use crate::{
    error::Error,
    expr::Op,
    gfx::{GraphicsEventPublisherPtr, RenderApi, Vertex},
    plugin::PluginSettings,
    prop::{
        Property, PropertyAtomicGuard, PropertyBool, PropertyStr, PropertySubType, PropertyType,
        PropertyValue, Role,
    },
    scene::{Pimpl, SceneNode, SceneNodePtr, SceneNodeType, Slot},
    text::TextShaperPtr,
    ui::{chatview, Window},
    ExecutorPtr,
};

mod node;
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

pub type AppPtr = Arc<App>;

pub struct App {
    pub sg_root: SceneNodePtr,
    pub render_api: RenderApi,
    pub text_shaper: TextShaperPtr,
    pub tasks: SyncMutex<Vec<Task<()>>>,
    pub ex: ExecutorPtr,
}

impl App {
    pub fn new(
        sg_root: SceneNodePtr,
        render_api: RenderApi,
        text_shaper: TextShaperPtr,
        ex: ExecutorPtr,
    ) -> Arc<Self> {
        Arc::new(Self { sg_root, ex, render_api, text_shaper, tasks: SyncMutex::new(vec![]) })
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

        let mut window = SceneNode::new("window", SceneNodeType::Window);

        let mut prop = Property::new("screen_size", PropertyType::Float32, PropertySubType::Pixel);
        prop.set_array_len(2);
        window.add_property(prop).unwrap();

        let setting_root = SceneNode::new("setting", SceneNodeType::SettingRoot);
        let setting_root = setting_root.setup_null();
        let settings_tree = db.open_tree("settings").unwrap();
        let settings = Arc::new(PluginSettings {
            setting_root: setting_root.clone(),
            sled_tree: settings_tree,
        });

        let mut window_scale = 1.;
        #[cfg(target_os = "android")]
        {
            window_scale = android::get_screen_density() / 2.25;
            t!("Setting window_scale to {window_scale}");
        }

        settings.add_setting("scale", PropertyValue::Float32(window_scale));
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

        schema::make(&self, window.clone()).await;

        //settings::make(&self, window, self.ex.clone()).await;

        d!("Schema loaded");

        Ok(None)
    }

    /// Begins the draw of the tree, and then starts the UI procs.
    pub async fn start(self: Arc<Self>, event_pub: GraphicsEventPublisherPtr) {
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
        self.init();
        self.trigger_draw().await;

        self.start_procs(event_pub).await;
        i!("App started");
    }

    pub fn init(&self) {
        let window_node = self.sg_root.clone().lookup_node("/window").unwrap();
        match window_node.pimpl() {
            Pimpl::Window(win) => win.init(),
            _ => panic!("wrong pimpl"),
        }
    }

    pub fn stop(&self) {
        let window_node = self.sg_root.clone().lookup_node("/window").unwrap();
        match window_node.pimpl() {
            Pimpl::Window(win) => win.stop(),
            _ => panic!("wrong pimpl"),
        }
    }

    async fn trigger_draw(&self) {
        let window_node = self.sg_root.clone().lookup_node("/window").expect("no window attached!");
        match window_node.pimpl() {
            Pimpl::Window(win) => win.draw().await,
            _ => panic!("wrong pimpl"),
        }
    }
    async fn start_procs(&self, event_pub: GraphicsEventPublisherPtr) {
        let window_node = self.sg_root.clone().lookup_node("/window").unwrap();
        match window_node.pimpl() {
            Pimpl::Window(win) => win.clone().start(event_pub, self.ex.clone()).await,
            _ => panic!("wrong pimpl"),
        }
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
