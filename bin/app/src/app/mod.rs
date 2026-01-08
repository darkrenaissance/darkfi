/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use sled_overlay::sled;
use smol::Task;
use std::sync::{Arc, Mutex as SyncMutex};

#[cfg(target_os = "android")]
use crate::android;

use crate::{
    error::Error,
    gfx::{gfxtag, EpochIndex, GraphicsEventPublisherPtr, RenderApi},
    plugin::PluginSettings,
    prop::{PropertyAtomicGuard, PropertyValue, Role},
    scene::{Pimpl, SceneNode, SceneNodePtr, SceneNodeType},
    ui::Window,
    util::i18n::I18nBabelFish,
    ExecutorPtr,
};

pub mod locale;
use locale::read_locale_ftl;
mod node;
use node::create_window;
mod schema;
use schema::get_settingsdb_path;

macro_rules! d { ($($arg:tt)*) => { debug!(target: "app", $($arg)*); } }
macro_rules! t { ($($arg:tt)*) => { trace!(target: "app", $($arg)*); } }
macro_rules! i { ($($arg:tt)*) => { info!(target: "app", $($arg)*); } }
//macro_rules! w { ($($arg:tt)*) => { warn!(target: "app", $($arg)*); } }
macro_rules! e { ($($arg:tt)*) => { error!(target: "app", $($arg)*); } }

//fn print_type_of<T>(_: &T) {
//    println!("{}", std::any::type_name::<T>())
//}

pub type AppPtr = Arc<App>;

pub struct App {
    pub sg_root: SceneNodePtr,
    pub render_api: RenderApi,
    pub tasks: SyncMutex<Vec<Task<()>>>,
    pub ex: ExecutorPtr,
}

impl App {
    pub fn new(sg_root: SceneNodePtr, render_api: RenderApi, ex: ExecutorPtr) -> Arc<Self> {
        Arc::new(Self { sg_root, ex, render_api, tasks: SyncMutex::new(vec![]) })
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

        let setting_root = SceneNode::new("setting", SceneNodeType::SettingRoot);
        let setting_root = setting_root.setup_null();
        let settings_tree = db.open_tree("settings").unwrap();
        let settings = Arc::new(PluginSettings {
            setting_root: setting_root.clone(),
            sled_tree: settings_tree,
        });

        #[cfg(target_os = "android")]
        let window_scale = {
            let screen_density = android::get_screen_density();
            i!("Android screen density: {screen_density}");
            screen_density / 2.625
        };
        #[cfg(not(target_os = "android"))]
        let window_scale = 1.2;

        d!("Setting window scale to {window_scale}");

        settings.add_setting("scale", PropertyValue::Float32(window_scale));
        //settings.load_settings();

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

        let i18n_fish = self.setup_locale();

        let window = create_window("window");
        #[cfg(target_os = "android")]
        {
            let insets = android::insets::get_insets();
            d!("Setting window insets to {insets:?}");
            let prop = window.get_property("insets").unwrap();
            let atom = &mut PropertyAtomicGuard::none();
            for i in 0..4 {
                prop.set_f32(atom, Role::App, i, insets[i]).unwrap();
            }
        }
        let window = window
            .setup(|me| {
                Window::new(me, self.render_api.clone(), i18n_fish.clone(), setting_root.clone())
            })
            .await;

        self.sg_root.link(window.clone());
        self.sg_root.link(setting_root.clone());

        #[cfg(feature = "schema-app")]
        schema::make(&self, window.clone(), &i18n_fish).await;

        #[cfg(feature = "schema-test")]
        schema::test::make(&self, window.clone(), &i18n_fish).await;

        #[cfg(all(feature = "schema-app", feature = "schema-test"))]
        compile_error!("Only one schema can be selected");

        //settings::make(&self, window, self.ex.clone()).await;

        d!("Schema loaded");

        Ok(None)
    }

    fn setup_locale(&self) -> I18nBabelFish {
        /*
        let i18n_src = indoc::indoc! {"
            hello-world = Hello, world!
            channels-label = CHANNELS
        "}
        .to_owned();
        */
        let locale = "en-US";
        let i18n_src = read_locale_ftl(locale);
        // Will be managed by settings eventually
        let i18n_fish = I18nBabelFish::new(i18n_src, locale);

        // sys-locale = "0.3"
        // fluent-langneg = "0.14"
        /*
        use fluent_langneg::{
            negotiate_languages,
            NegotiationStrategy,
            convert_vec_str_to_langids_lossy,
            LanguageIdentifier
        };
        let mut locales: Vec<_> = sys_locale::get_locales().collect();
        let en_US = "en-US".to_string();
        if !locales.contains(&en_US) {
            locales.push(en_US);
        }
        info!(target: "app", "Locale: {:?}", locales);
        */

        i18n_fish
    }

    /// Begins the draw of the tree, and then starts the UI procs.
    pub async fn start(self: Arc<Self>, event_pub: GraphicsEventPublisherPtr, epoch: EpochIndex) {
        d!("Starting app epoch={epoch}");
        let mut atom = PropertyAtomicGuard::none();

        let window_node = self.sg_root.lookup_node("/window").unwrap();
        let prop = window_node.get_property("screen_size").unwrap();
        // We can only do this once the window has been created in miniquad.
        let (screen_width, screen_height) = miniquad::window::screen_size();
        prop.set_f32(&mut atom, Role::App, 0, screen_width).unwrap();
        prop.set_f32(&mut atom, Role::App, 1, screen_height).unwrap();

        drop(atom);

        // Access drawable in window node and call draw()
        self.init();
        //if epoch == 1 {
        self.trigger_draw().await;
        //}

        self.start_procs(event_pub).await;
        i!("App started");
    }

    pub fn init(&self) {
        let window_node = self.sg_root.lookup_node("/window").unwrap();
        match window_node.pimpl() {
            Pimpl::Window(win) => win.init(),
            _ => panic!("wrong pimpl"),
        }
    }

    pub fn stop(&self) {
        let window_node = self.sg_root.lookup_node("/window").unwrap();
        match window_node.pimpl() {
            Pimpl::Window(win) => win.stop(),
            _ => panic!("wrong pimpl"),
        }
    }

    async fn trigger_draw(&self) {
        let atom = &mut self.render_api.make_guard(gfxtag!("App::trigger_draw"));
        let window_node = self.sg_root.lookup_node("/window").expect("no window attached!");
        match window_node.pimpl() {
            Pimpl::Window(win) => win.draw(atom).await,
            _ => panic!("wrong pimpl"),
        }
    }
    async fn start_procs(&self, event_pub: GraphicsEventPublisherPtr) {
        let window_node = self.sg_root.lookup_node("/window").unwrap();
        match window_node.pimpl() {
            Pimpl::Window(win) => win.clone().start(event_pub, self.ex.clone()).await,
            _ => panic!("wrong pimpl"),
        }
    }

    pub fn notify_start(&self) {
        let window = self.sg_root.lookup_node("/window").unwrap();
        smol::block_on(async {
            window.trigger("start", vec![]).await.unwrap();
        });
    }

    pub fn notify_stop(&self) {
        let window = self.sg_root.lookup_node("/window").unwrap();
        smol::block_on(async {
            window.trigger("stop", vec![]).await.unwrap();
        });
    }
}
