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
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex as SyncMutex,
};

#[cfg(target_os = "android")]
use crate::android;

#[cfg(any(feature = "enable-plugin-darkirc", feature = "enable-plugin-fud"))]
use crate::plugin::PluginSettings;
use crate::{
    error::Error,
    gfx::{EpochIndex, GraphicsEventPublisherPtr, Renderer},
    prop::{PropertyAtomicGuard, PropertyValue, Role},
    scene::{Pimpl, SceneNode, SceneNodePtr, SceneNodeType},
    sfx,
    ui::{RedrawTrigger, Window},
    util::i18n::I18nBabelFish,
    ExecutorPtr,
};

pub mod locale;
use locale::read_locale_ftl;
mod node;
use node::create_window;
pub mod schema;
use schema::get_settingsdb_path;

macro_rules! d { ($($arg:tt)*) => { debug!(target: "app", $($arg)*); } }
macro_rules! t { ($($arg:tt)*) => { trace!(target: "app", $($arg)*); } }
macro_rules! i { ($($arg:tt)*) => { info!(target: "app", $($arg)*); } }
//macro_rules! w { ($($arg:tt)*) => { warn!(target: "app", $($arg)*); } }
macro_rules! e { ($($arg:tt)*) => { error!(target: "app", $($arg)*); } }

const IS_FIRST_TIME_KEY: &[u8] = b"is_first_time";

//fn print_type_of<T>(_: &T) {
//    println!("{}", std::any::type_name::<T>())
//}

pub type AppPtr = Arc<App>;

pub struct App {
    pub sg_root: SceneNodePtr,
    pub renderer: Renderer,
    pub tasks: SyncMutex<Vec<Task<()>>>,
    pub ex: ExecutorPtr,
    /// Handle for requesting a serialized draw pass from the window's
    /// draw loop. Passed to `Window::new` (with the receiver) and to
    /// widget constructors during migration to the draw-pass model.
    pub redraw_trigger: RedrawTrigger,
    /// Receiver side of the redraw queue, handed to the window in `setup()`.
    redraw_rx: async_channel::Receiver<()>,
    /// True if this is the first time the app has ever been run.
    /// Loaded from the sled DB in `setup()`.
    pub is_first_time: AtomicBool,
}

impl App {
    pub fn new(sg_root: SceneNodePtr, renderer: Renderer, ex: ExecutorPtr) -> Arc<Self> {
        let (redraw_trigger, redraw_rx) = RedrawTrigger::new();
        Arc::new(Self {
            sg_root,
            ex,
            renderer,
            tasks: SyncMutex::new(vec![]),
            redraw_trigger,
            redraw_rx,
            is_first_time: AtomicBool::new(false),
        })
    }

    /// Does not require miniquad to be init. Created the scene graph tree / schema and all
    /// the objects.
    pub async fn setup(&self, db: sled::Db) -> Result<Option<i32>, Error> {
        t!("App::setup()");

        let setting_root = SceneNode::new("setting", SceneNodeType::SettingRoot);
        let setting_root = setting_root.setup_null();
        let settings_tree = db.open_tree("settings").unwrap();

        let flags_tree = db.open_tree("app_flags").unwrap();
        let is_first_time = !flags_tree.contains_key(IS_FIRST_TIME_KEY).unwrap();
        if is_first_time {
            flags_tree.insert(IS_FIRST_TIME_KEY, b"").unwrap();
            flags_tree.flush().unwrap();
        }
        self.is_first_time.store(is_first_time, Ordering::Relaxed);
        // Commenting this out since it doesnt compile when enable-plugins isnt enabled.
        /*
        let settings = Arc::new(PluginSettings {
            setting_root: setting_root.clone(),
            sled_tree: settings_tree,
        });
        */

        let i18n_fish = self.setup_locale();

        let window = create_window("window");
        #[cfg(target_os = "android")]
        let window_scale = {
            let screen_density = miniquad::window::dpi_scale();
            i!("Android screen density: {screen_density}");
            screen_density / 2.8
        };
        #[cfg(not(target_os = "android"))]
        let window_scale = 1.;

        d!("Setting window scale to {window_scale}");
        let prop = window.get_property("scale").unwrap();
        let atom = &mut PropertyAtomicGuard::none();
        prop.set_f32(atom, Role::App, 0, window_scale).unwrap();

        #[cfg(target_os = "android")]
        {
            let insets = android::insets::get_insets();
            d!("Setting window insets to {insets:?}");
            let prop = window.get_property("insets").unwrap();
            for i in 0..4 {
                prop.set_f32(atom, Role::App, i, insets[i]).unwrap();
            }
        }
        let window = window
            .setup(|me| {
                Window::new(
                    me,
                    self.renderer.clone(),
                    i18n_fish.clone(),
                    setting_root.clone(),
                    self.redraw_trigger.clone(),
                    self.redraw_rx.clone(),
                )
            })
            .await;

        self.sg_root.link(window.clone());
        self.sg_root.link(setting_root.clone());

        #[cfg(feature = "schema-app")]
        schema::make(&self, window.clone(), &i18n_fish, db).await;

        #[cfg(feature = "schema-test")]
        schema::test::make(&self, window.clone(), &i18n_fish).await;

        #[cfg(feature = "schema-test-edit")]
        schema::test_edit::make(&self, window.clone(), &i18n_fish).await;

        #[cfg(feature = "schema-test-scroll-layer")]
        schema::test_scroll_layer::make(&self, window.clone(), &i18n_fish).await;

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
        if self.is_first_time.load(Ordering::Relaxed) {
            sfx::play_commup();
        }
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
        // Enqueue a draw pass on the window's serialized draw loop.
        // The bounded(1) queue buffers this until the listener task in
        // Window::start() is running, so calling before start is safe.
        self.redraw_trigger.trigger();

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

    async fn start_procs(&self, event_pub: GraphicsEventPublisherPtr) {
        let window_node = self.sg_root.lookup_node("/window").unwrap();
        match window_node.pimpl() {
            Pimpl::Window(win) => win.clone().start(event_pub, self.ex.clone()).await,
            _ => panic!("wrong pimpl"),
        }
    }
}
