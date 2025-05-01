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

// Use these to incrementally fix warnings with cargo fix
#![allow(warnings, unused)]
//#![deny(unused_imports)]

use async_lock::{Mutex as AsyncMutex, RwLock as AsyncRwLock};
use darkfi::system::CondVar;
use file_rotate::{compression::Compression, suffix::AppendCount, ContentLimit, FileRotate};
use std::sync::{mpsc, Arc, OnceLock};

#[macro_use]
extern crate log;
#[allow(unused_imports)]
use log::LevelFilter;

#[derive(Debug)]
pub enum AndroidSuggestEvent {
    Init,
    CreateInputConnect,
    Compose { text: String, cursor_pos: i32, is_commit: bool },
    ComposeRegion { start: usize, end: usize },
    FinishCompose,
    DeleteSurroundingText { left: usize, right: usize },
}

#[cfg(target_os = "android")]
mod android;
mod app;
mod build_info;
mod error;
mod expr;
mod gfx;
mod logger;
mod mesh;
mod net;
mod plugin;
mod prop;
mod pubsub;
//mod py;
mod ringbuf;
mod scene;
mod shape;
use scene::SceneNode as SceneNode3;
mod text;
mod text2;
mod ui;
mod util;

use crate::{app::{App, AppPtr}, net::ZeroMQAdapter, text::TextShaper, util::AsyncRuntime};

// This is historical, but ideally we can fix the entire project and remove this import.
pub use util::ExecutorPtr;

// Hides the cmd.exe terminal on Windows.
// Enable this when making release builds.
//#![windows_subsystem = "windows"]

fn panic_hook(panic_info: &std::panic::PanicHookInfo) {
    error!("panic occurred: {panic_info}");
    error!("{}", std::backtrace::Backtrace::force_capture().to_string());
    std::process::abort()
}

/// Contains values which persist between app restarts. For example on Android, we are
/// running a foreground service. Everytime the UI restarts main() is called again.
/// However the global state remains intact.
struct God {
    bg_runtime: AsyncRuntime,
    bg_ex: ExecutorPtr,

    fg_runtime: AsyncRuntime,
    fg_ex: ExecutorPtr,

    /// App must fully finish setup() before start() is allowed to begin.
    cv_app_is_setup: Arc<CondVar>,
    app: AppPtr,

    /// This is the main rendering API used to send commands to the gfx subsystem.
    /// We have a ref here so the gfx subsystem can increment the epoch counter.
    render_api: gfx::RenderApi,
    /// This is how the gfx subsystem receives messages from the render API.
    method_rep: async_channel::Receiver<(gfx::EpochIndex, gfx::GraphicsMethod)>,
    /// Publisher to send input and window events to subscribers.
    event_pub: gfx::GraphicsEventPublisherPtr,
}

impl God {
    fn new() -> Self {
        info!(target: "main", "Creating the app");

        // Abort the application on panic right away
        std::panic::set_hook(Box::new(panic_hook));

        text2::init_txt_ctx();
        logger::setup_logging();

        #[cfg(target_os = "android")]
        {
            use crate::android::get_appdata_path;

            // Workaround for this bug
            // https://gitlab.torproject.org/tpo/core/arti/-/issues/999
            unsafe {
                std::env::set_var("HOME", get_appdata_path().as_os_str());
            }
        }

        let exe_path = std::env::current_exe().unwrap();
        let basename = exe_path.parent().unwrap();
        std::env::set_current_dir(basename);

        let bg_ex = Arc::new(smol::Executor::new());
        let fg_ex = Arc::new(smol::Executor::new());
        let sg_root = SceneNode3::root();

        let bg_runtime = AsyncRuntime::new(bg_ex.clone(), "bg");
        bg_runtime.start();

        let fg_runtime = AsyncRuntime::new(fg_ex.clone(), "fg");

        let (method_req, method_rep) = async_channel::unbounded();
        // The UI actually needs to be running for this to reply back.
        // Otherwise calls will just hang.
        let render_api = gfx::RenderApi::new(method_req);
        let event_pub = gfx::GraphicsEventPublisher::new();

        let text_shaper = TextShaper::new();

        let app = App::new(sg_root, render_api.clone(), text_shaper, fg_ex.clone());

        Self {
            bg_runtime,
            bg_ex,

            fg_runtime,
            fg_ex,
            cv_app_is_setup: Arc::new(CondVar::new()),
            app,

            render_api,
            method_rep,
            event_pub
        }
    }

    /// Restart the app but leave the backends intact.
    fn setup_app(&self) {
        info!(target: "main", "Restarting the app");
        #[cfg(target_os = "android")]
        {
            use crate::android::{get_appdata_path, get_external_storage_path};

            info!("App internal data path: {:?}", get_appdata_path());
            info!("App external storage path: {:?}", get_external_storage_path());

            //let paths = std::fs::read_dir("/data/data/darkfi.darkfi/").unwrap();
            //for path in paths {
            //    debug!("{}", path.unwrap().path().display())
            //}
        }

        info!("Target OS: {}", build_info::TARGET_OS);
        info!("Target arch: {}", build_info::TARGET_ARCH);
        let cwd = std::env::current_dir().unwrap();
        info!("Current dir: {}", cwd.display());

        self.fg_runtime.start_with_count(2);

        /*
        #[cfg(feature = "enable-netdebug")]
        {
            let sg_root2 = sg_root.clone();
            let ex2 = ex.clone();
            let zmq_task = ex.spawn(async {
                let zmq_rpc = ZeroMQAdapter::new(sg_root2, ex2).await;
                zmq_rpc.run().await;
            });
            async_runtime.push_task(zmq_task);
        }
        */

        let app = self.app.clone();
        let cv = self.cv_app_is_setup.clone();
        let app_task = self.fg_ex.spawn(async move {
            app.setup().await;
            cv.notify();
        });
        self.fg_runtime.push_task(app_task);
    }

    /// Start the app. Can only happen once the window is ready.
    pub fn start_app(&self) {
        let app = self.app.clone();
        let cv = self.cv_app_is_setup.clone();
        let event_pub = self.event_pub.clone();
        let app_task = self.fg_ex.spawn(async move {
            cv.wait().await;
            app.start(event_pub).await;
        });
        self.fg_runtime.push_task(app_task);
    }

    /// Put the app to sleep until the next restart.
    pub fn stop_app(&self) {
        self.fg_runtime.stop();
        self.app.stop();
        self.cv_app_is_setup.reset();

        #[cfg(target_os = "android")]
        android::clear_state();

        info!(target: "main", "App stopped");
    }
}

impl std::fmt::Debug for God {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "God")
    }
}

pub static GOD: OnceLock<God> = OnceLock::new();

fn main() {
    if GOD.get().is_none() {
        let god = God::new();
        GOD.set(god).unwrap();
    }

    // Reuse render_api, event_pub and text_shaper
    // No need for setup(), just wait for gfx start then call .start()
    // ZMQ, darkirc stay running

    {
        let god = GOD.get().unwrap();
        god.setup_app();
    }

    /*
    // Nice to see which events exist
    let ev_sub = event_pub.subscribe_key_down();
    let ev_relay_task = ex.spawn(async move {
        debug!(target: "main", "event relayer started");
        loop {
            let Ok((key, mods, repeat)) = ev_sub.receive().await else {
                debug!(target: "main", "Event relayer closed");
                break
            };
            // Ignore keys which get stuck repeating when switching windows
            match key {
                miniquad::KeyCode::LeftShift | miniquad::KeyCode::LeftSuper => continue,
                _ => {}
            }
            if !repeat {
                debug!(target: "main", "key_down event: {:?} {:?} {}", key, mods, repeat);
            }
        }
    });
    async_runtime.push_task(ev_relay_task);
    let ev_sub = event_pub.subscribe_key_up();
    let ev_relay_task = ex.spawn(async move {
        debug!(target: "main", "event relayer started");
        loop {
            let Ok((key, mods)) = ev_sub.receive().await else {
                debug!(target: "main", "Event relayer closed");
                break
            };
            // Ignore keys which get stuck repeating when switching windows
            match key {
                miniquad::KeyCode::LeftShift | miniquad::KeyCode::LeftSuper => continue,
                _ => {}
            }
            debug!(target: "main", "key_up event: {:?} {:?}", key, mods);
        }
    });
    async_runtime.push_task(ev_relay_task);
    let ev_sub = event_pub.subscribe_char();
    let ev_relay_task = ex.spawn(async move {
        debug!(target: "main", "event relayer started");
        loop {
            let Ok((key, mods, repeat)) = ev_sub.receive().await else {
                debug!(target: "main", "Event relayer closed");
                break
            };
            debug!(target: "main", "char event: {:?} {:?} {}", key, mods, repeat);
        }
    });
    async_runtime.push_task(ev_relay_task);
    */

    //let stage = gfx::Stage::new(method_rep, event_pub);
    gfx::run_gui();
    debug!(target: "main", "Started GFX backend");
}

/*
use rustpython_vm::{self as pyvm, convert::ToPyObject};

fn main() {
    let module = pyvm::Interpreter::without_stdlib(Default::default()).enter(|vm| {
        let source = r#"
def foo():
    open("hihi", "w")
    return 110
#max(1 + lw/3, 4*10) + foo(2, True)
"#;
        //let code_obj = vm
        //    .compile(source, pyvm::compiler::Mode::Exec, "<embedded>".to_owned())
        //    .map_err(|err| vm.new_syntax_error(&err, Some(source))).unwrap();
        //code_obj
        pyvm::import::import_source(vm, "lain", source).unwrap()
    });

    fn foo(x: u32, y: bool) -> u32 {
        if y {
            2 * x
        } else {
            x
        }
    }

    let res = pyvm::Interpreter::without_stdlib(Default::default()).enter(|vm| {
        let globals = vm.ctx.new_dict();
        globals.set_item("lw", vm.ctx.new_int(110).to_pyobject(vm), vm).unwrap();
        globals.set_item("lh", vm.ctx.new_int(4).to_pyobject(vm), vm).unwrap();
        globals.set_item("foo", vm.new_function("foo", foo).into(), vm).unwrap();

        let scope = pyvm::scope::Scope::new(None, globals);

        let foo_fn = module.get_attr("foo", vm).unwrap();
        foo_fn.call((), vm).unwrap()

        //vm.run_code_obj(code_obj, scope).unwrap()
    });
    println!("{:?}", res);
}
*/
