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

// Hides the cmd.exe terminal on Windows.
// Enable this when making release builds.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use clap::Parser;
use darkfi::system::CondVar;
use std::sync::{Arc, OnceLock};

#[macro_use]
extern crate tracing;

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
#[cfg(feature = "enable-netdebug")]
mod net;
mod plugin;
mod prop;
mod pubsub;
//mod py;
//mod ringbuf;
mod scene;
mod shape;
mod text;
mod text2;
mod ui;
mod util;
mod video;

use crate::{
    app::{App, AppPtr},
    gfx::EpochIndex,
    prop::{Property, PropertySubType, PropertyType},
    scene::{CallArgType, SceneNode, SceneNodeType},
    text::TextShaper,
    util::AsyncRuntime,
};
#[cfg(feature = "enable-netdebug")]
use net::ZeroMQAdapter;
#[cfg(feature = "enable-plugins")]
use {
    darkfi_serial::{deserialize, Decodable, Encodable},
    gfx::RenderApi,
    prop::{PropertyBool, PropertyStr, Role},
    scene::{SceneNodePtr, Slot},
    std::io::Cursor,
    ui::chatview,
};

// This is historical, but ideally we can fix the entire project and remove this import.
pub use util::ExecutorPtr;

macro_rules! t { ($($arg:tt)*) => { trace!(target: "main", $($arg)*); } }
#[cfg(feature = "enable-plugins")]
macro_rules! d { ($($arg:tt)*) => { trace!(target: "main", $($arg)*); } }
#[cfg(feature = "enable-plugins")]
macro_rules! i { ($($arg:tt)*) => { trace!(target: "main", $($arg)*); } }

fn panic_hook(panic_info: &std::panic::PanicHookInfo) {
    error!("panic occurred: {panic_info}");
    error!("{}", std::backtrace::Backtrace::force_capture().to_string());
    std::process::abort()
}

/// Contains values which persist between app restarts. For example on Android, we are
/// running a foreground service. Everytime the UI restarts main() is called again.
/// However the global state remains intact.
struct God {
    _bg_runtime: AsyncRuntime,
    _bg_ex: ExecutorPtr,

    pub fg_runtime: AsyncRuntime,
    pub fg_ex: ExecutorPtr,

    /// App must fully finish setup() before start() is allowed to begin.
    cv_app_is_setup: Arc<CondVar>,
    app: AppPtr,

    /// This is the main rendering API used to send commands to the gfx subsystem.
    /// We have a ref here so the gfx subsystem can increment the epoch counter.
    render_api: gfx::RenderApi,
    /// This is how the gfx subsystem receives messages from the render API.
    method_recv: async_channel::Receiver<(gfx::EpochIndex, gfx::GraphicsMethod)>,
    /// Publisher to send input and window events to subscribers.
    event_pub: gfx::GraphicsEventPublisherPtr,

    /// A WorkerGuard for file logging used to ensure buffered logs are flushed
    /// to their output in the case of abrupt terminations of a process.
    _file_logging_guard: Option<tracing_appender::non_blocking::WorkerGuard>,
}

impl God {
    fn new() -> Self {
        // Abort the application on panic right away
        std::panic::set_hook(Box::new(panic_hook));

        text2::init_txt_ctx();
        let file_logging_guard = logger::setup_logging();

        info!(target: "main", "Creating the app");

        #[cfg(target_os = "android")]
        {
            use crate::android::get_appdata_path;

            // Workaround for this bug
            // https://gitlab.torproject.org/tpo/core/arti/-/issues/999
            unsafe {
                std::env::set_var("HOME", get_appdata_path().as_os_str());
            }
        }

        #[cfg(target_os = "ios")]
        {
            let home = std::env::var("HOME").unwrap_or(".".to_string());
            let doc_path = std::path::PathBuf::from(home).join("Documents");
            unsafe {
                std::env::set_var("HOME", doc_path);
            }
        }

        let exe_path = std::env::current_exe().unwrap();
        let basename = exe_path.parent().unwrap();
        std::env::set_current_dir(basename).unwrap();

        let bg_ex = Arc::new(smol::Executor::new());
        let fg_ex = Arc::new(smol::Executor::new());
        let sg_root = SceneNode::root();

        let bg_runtime = AsyncRuntime::new(bg_ex.clone(), "bg");
        bg_runtime.start();

        let fg_runtime = AsyncRuntime::new(fg_ex.clone(), "fg");

        let (method_send, method_recv) = async_channel::unbounded();
        // The UI actually needs to be running for this to reply back.
        // Otherwise calls will just hang.
        let render_api = gfx::RenderApi::new(method_send);
        let event_pub = gfx::GraphicsEventPublisher::new();

        let text_shaper = TextShaper::new();

        let app = App::new(sg_root.clone(), render_api.clone(), text_shaper, fg_ex.clone());

        let app2 = app.clone();
        let cv_app_is_setup = Arc::new(CondVar::new());
        let cv = cv_app_is_setup.clone();
        let app_task = fg_ex.spawn(async move {
            app2.setup().await.unwrap();
            cv.notify();
        });
        fg_runtime.push_task(app_task);

        #[cfg(feature = "enable-netdebug")]
        {
            let sg_root = sg_root.clone();
            let ex = bg_ex.clone();
            let render_api = render_api.clone();
            let zmq_task = bg_ex.spawn(async {
                let zmq_rpc = ZeroMQAdapter::new(sg_root, render_api, ex).await;
                zmq_rpc.run().await;
            });
            bg_runtime.push_task(zmq_task);
        }

        #[cfg(feature = "enable-plugins")]
        {
            let ex = bg_ex.clone();
            let cv = cv_app_is_setup.clone();
            let render_api = render_api.clone();
            let plug_task = bg_ex.spawn(async move {
                load_plugins(ex, sg_root, render_api, cv).await;
            });
            bg_runtime.push_task(plug_task);
        }

        #[cfg(not(feature = "enable-plugins"))]
        warn!(target: "main", "Plugins are disabled in this build");

        Self {
            _bg_runtime: bg_runtime,
            _bg_ex: bg_ex,

            fg_runtime,
            fg_ex,
            cv_app_is_setup,
            app,

            render_api,
            method_recv,
            event_pub,
            _file_logging_guard: file_logging_guard,
        }
    }

    /// Start the app. Can only happen once the window is ready.
    pub fn start_app(&self, epoch: EpochIndex) {
        info!(target: "main", "Starting the app");
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

        let app = self.app.clone();
        let cv = self.cv_app_is_setup.clone();
        let event_pub = self.event_pub.clone();
        smol::block_on(async move {
            cv.wait().await;
            app.start(event_pub, epoch).await;
        });
    }

    /// Put the app to sleep until the next restart.
    pub fn stop_app(&self) {
        self.fg_runtime.stop();
        self.app.stop();
        info!(target: "main", "App stopped");
    }
}

impl std::fmt::Debug for God {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "God")
    }
}

static GOD: OnceLock<God> = OnceLock::new();

#[cfg(feature = "enable-plugins")]
async fn load_plugins(
    ex: ExecutorPtr,
    sg_root: SceneNodePtr,
    render_api: RenderApi,
    cv: Arc<CondVar>,
) {
    let plugin = SceneNode::new("plugin", SceneNodeType::PluginRoot);
    let plugin = plugin.setup_null();
    sg_root.link(plugin.clone());

    let darkirc = create_darkirc("darkirc");
    let darkirc = darkirc
        .setup(|me| async {
            plugin::DarkIrc::new(me, ex.clone()).await.expect("DarkIrc pimpl setup")
        })
        .await;

    let fud = create_fud("fud");
    let sg_root2 = sg_root.clone();
    let fud = fud
        .setup(|me| async {
            plugin::Fud::new(me, sg_root2, ex.clone()).await.expect("Fud pimpl setup")
        })
        .await;

    let (slot, recvr) = Slot::new("recvmsg");
    darkirc.register("recv", slot).unwrap();
    let sg_root2 = sg_root.clone();
    let darkirc_nick = PropertyStr::wrap(&darkirc, Role::App, "nick", 0).unwrap();
    let render_api2 = render_api.clone();
    let listen_recv = ex.spawn(async move {
        while let Ok(data) = recvr.recv().await {
            let atom = &mut render_api2.make_guard(gfxtag!("darkirc msg recv"));

            let mut cur = Cursor::new(&data);
            let channel = String::decode(&mut cur).unwrap();
            let timestamp = chatview::Timestamp::decode(&mut cur).unwrap();
            let id = chatview::MessageId::decode(&mut cur).unwrap();
            let nick = String::decode(&mut cur).unwrap();
            let msg = String::decode(&mut cur).unwrap();

            let node_path = format!("/window/{channel}_chat_layer/content/chatty");
            t!("Attempting to relay message to {node_path}");
            let Some(chatview) = sg_root2.lookup_node(&node_path) else {
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
            let chat_layer = sg_root2.lookup_node(chat_path).unwrap();
            if chat_layer.get_property_bool("is_visible").unwrap() {
                continue
            }

            let node_path = format!("/window/menu_layer/{channel}_channel_label");
            let menu_label = sg_root2.lookup_node(&node_path).unwrap();
            let prop = menu_label.get_property("text_color").unwrap();
            if msg.contains(&darkirc_nick.get()) {
                // Nick highlight
                prop.set_f32(atom, Role::App, 0, 0.56).unwrap();
                prop.set_f32(atom, Role::App, 1, 0.61).unwrap();
                prop.set_f32(atom, Role::App, 2, 1.).unwrap();
                prop.set_f32(atom, Role::App, 3, 1.).unwrap();
            } else {
                // Normal channel activity
                prop.set_f32(atom, Role::App, 0, 0.36).unwrap();
                prop.set_f32(atom, Role::App, 1, 1.).unwrap();
                prop.set_f32(atom, Role::App, 2, 0.51).unwrap();
                prop.set_f32(atom, Role::App, 3, 1.).unwrap();
            }
        }
    });

    let (slot, recvr) = Slot::new("connect");
    darkirc.register("connect", slot).unwrap();
    let sg_root2 = sg_root.clone();
    let listen_connect = ex.spawn(async move {
        cv.wait().await;
        let net0 = sg_root2.lookup_node("/window/netstatus_layer/net0").unwrap();
        let net1 = sg_root2.lookup_node("/window/netstatus_layer/net1").unwrap();
        let net2 = sg_root2.lookup_node("/window/netstatus_layer/net2").unwrap();
        let net3 = sg_root2.lookup_node("/window/netstatus_layer/net3").unwrap();

        let net0_is_visible = PropertyBool::wrap(&net0, Role::App, "is_visible", 0).unwrap();
        let net1_is_visible = PropertyBool::wrap(&net1, Role::App, "is_visible", 0).unwrap();
        let net2_is_visible = PropertyBool::wrap(&net2, Role::App, "is_visible", 0).unwrap();
        let net3_is_visible = PropertyBool::wrap(&net3, Role::App, "is_visible", 0).unwrap();

        while let Ok(data) = recvr.recv().await {
            let (peers_count, is_dag_synced): (u32, bool) = deserialize(&data).unwrap();

            let atom = &mut render_api.make_guard(gfxtag!("netstatus change"));

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

    plugin.link(darkirc);
    plugin.link(fud);

    i!("Plugins loaded");
    futures::join!(listen_recv, listen_connect);
}

pub fn create_darkirc(name: &str) -> SceneNode {
    t!("create_darkirc({name})");
    let mut node = SceneNode::new(name, SceneNodeType::Plugin);

    let mut prop = Property::new("nick", PropertyType::Str, PropertySubType::Null);
    prop.set_ui_text("Nick", "Nickname");
    prop.set_defaults_str(vec!["anon".to_string()]).unwrap();
    node.add_property(prop).unwrap();

    node.add_signal(
        "recv",
        "Message received",
        vec![
            ("channel", "Channel", CallArgType::Str),
            ("timestamp", "Timestamp", CallArgType::Uint64),
            ("id", "ID", CallArgType::Hash),
            ("nick", "Nick", CallArgType::Str),
            ("msg", "Message", CallArgType::Str),
        ],
    )
    .unwrap();

    node.add_signal(
        "connect",
        "Connections and disconnects",
        vec![
            ("peers_count", "Peers Count", CallArgType::Uint32),
            ("dag_synced", "Is DAG Synced", CallArgType::Bool),
        ],
    )
    .unwrap();

    node.add_method(
        "send",
        vec![("channel", "Channel", CallArgType::Str), ("msg", "Message", CallArgType::Str)],
        None,
    )
    .unwrap();

    node
}

pub fn create_fud(name: &str) -> SceneNode {
    t!("create_fud({name})");
    let mut node = SceneNode::new(name, SceneNodeType::Plugin);

    let mut prop = Property::new("ready", PropertyType::Bool, PropertySubType::Null);
    prop.set_defaults_bool(vec![false]).unwrap();
    node.add_property(prop).unwrap();

    node.add_method("get", vec![("hash", "Hash", CallArgType::Str)], None).unwrap();

    node
}

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// On Linux use the X11 backend
    #[arg(long)]
    linux_x11_backend: bool,

    /// On Linux use the wayland backend
    #[arg(long)]
    linux_wayland_backend: bool,
}

fn main() {
    let args = Args::parse();

    GOD.get_or_init(God::new);

    // Reuse render_api, event_pub and text_shaper
    // No need for setup(), just wait for gfx start then call .start()
    // ZMQ, darkirc stay running

    let linux_backend = if args.linux_wayland_backend {
        if args.linux_x11_backend {
            miniquad::conf::LinuxBackend::WaylandWithX11Fallback
        } else {
            miniquad::conf::LinuxBackend::WaylandOnly
        }
    } else if args.linux_x11_backend {
        miniquad::conf::LinuxBackend::X11Only
    } else {
        miniquad::conf::LinuxBackend::WaylandWithX11Fallback
    };

    gfx::run_gui(linux_backend);
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
