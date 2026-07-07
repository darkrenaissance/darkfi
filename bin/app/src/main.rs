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

// Hides the cmd.exe terminal on Windows.
// Enable this when making release builds.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use clap::Parser;
use darkfi::system::CondVar;
use darkfi::tx::Transaction;
use darkfi::util::parse::encode_base10;
use darkfi_money_contract::model::DARK_TOKEN_ID;
use darkfi_serial::{Decodable, Encodable, deserialize};
use std::sync::{Arc, OnceLock};

#[macro_use]
extern crate tracing;

#[cfg(target_os = "android")]
mod android;
mod app;
mod build_info;
mod clipboard;
mod error;
mod expr;
mod gfx;
mod logger;
mod mesh;
#[cfg(feature = "enable-netdebug")]
mod net;
#[cfg(feature = "enable-plugins")]
mod plugin;
mod prop;
mod pubsub;
//mod py;
//mod ringbuf;
mod scene;
mod shape;
mod text;
mod ui;
mod util;

use crate::{
    app::{App, AppPtr},
    gfx::EpochIndex,
    prop::{Property, PropertySubType, PropertyType},
    scene::{CallArgType, SceneNode, SceneNodePtr, SceneNodeType},
    util::AsyncRuntime,
};
#[cfg(feature = "enable-netdebug")]
use net::ZeroMQAdapter;
#[cfg(feature = "enable-plugins")]
use {
    // Local imports
    gfx::Renderer,
    prop::{PropertyBool, PropertyStr, Role},
    scene::Slot,
    std::io::Cursor,
    ui::chatview,
    // Global imports
    url::Url,
};

// This is historical, but ideally we can fix the entire project and remove this import.
pub use util::ExecutorPtr;

macro_rules! t { ($($arg:tt)*) => { trace!(target: "main", $($arg)*); } }
#[cfg(feature = "enable-plugins")]
macro_rules! d { ($($arg:tt)*) => { trace!(target: "main", $($arg)*); } }
#[cfg(any(feature = "enable-plugins", feature = "enable-netdebug"))]
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
    _fg_ex: ExecutorPtr,

    /// App must fully finish setup() before start() is allowed to begin.
    cv_app_is_setup: Arc<CondVar>,
    app: AppPtr,

    /// This is the main rendering API used to send commands to the gfx subsystem.
    /// We have a ref here so the gfx subsystem can increment the epoch counter.
    renderer: gfx::Renderer,
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
        let renderer = gfx::Renderer::new(method_send);
        let event_pub = gfx::GraphicsEventPublisher::new();

        let app = App::new(sg_root.clone(), renderer.clone(), fg_ex.clone());

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
            let renderer = renderer.clone();
            let zmq_task = bg_ex.spawn(async {
                i!("Enabled net debugging backend in this build");
                let zmq_rpc = ZeroMQAdapter::new(sg_root, renderer, ex).await;
                zmq_rpc.run().await;
            });
            bg_runtime.push_task(zmq_task);
        }

        #[cfg(feature = "enable-plugins")]
        {
            let ex = bg_ex.clone();
            let cv = cv_app_is_setup.clone();
            let renderer = renderer.clone();
            let plug_task = bg_ex.spawn(async move {
                load_plugins(ex, sg_root, renderer, cv).await;
            });
            bg_runtime.push_task(plug_task);
        }

        #[cfg(not(feature = "enable-plugins"))]
        warn!(target: "main", "Plugins are disabled in this build");

        Self {
            _bg_runtime: bg_runtime,
            _bg_ex: bg_ex,

            fg_runtime,
            _fg_ex: fg_ex,
            cv_app_is_setup,
            app,

            renderer,
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

        self.app.notify_start();
    }

    /// Put the app to sleep until the next restart.
    pub fn stop_app(&self) {
        self.app.notify_stop();
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
    renderer: Renderer,
    cv: Arc<CondVar>,
) {
    let plugin = SceneNode::new("plugin", SceneNodeType::PluginRoot);
    let plugin = plugin.setup_null();
    sg_root.link(plugin.clone());

    // DarkIrc needs /window to start
    cv.wait().await;
    let darkirc = create_darkirc("darkirc");
    let darkirc = darkirc
        .setup(|me| async {
            plugin::DarkIrc::new(me, sg_root.clone(), ex.clone())
                .await
                .expect("DarkIrc pimpl setup")
        })
        .await;

    let (slot, recvr) = Slot::new("recvmsg");
    darkirc.register("recv", slot).unwrap();
    let sg_root2 = sg_root.clone();
    let darkirc_nick = PropertyStr::wrap(&darkirc, Role::App, "nick", 0).unwrap();
    let renderer2 = renderer.clone();
    let listen_recv = ex.spawn(async move {
        while let Ok(data) = recvr.recv().await {
            let atom = &mut renderer2.make_guard(gfxtag!("darkirc msg recv"));

            let mut cur = Cursor::new(&data);
            let channel = String::decode(&mut cur).unwrap();
            let timestamp = chatview::Timestamp::decode(&mut cur).unwrap();
            let id = chatview::MessageId::decode(&mut cur).unwrap();
            let nick = String::decode(&mut cur).unwrap();
            let msg = String::decode(&mut cur).unwrap();

            let node_path = format!("/window/content/{channel}_chat_layer/content/chatty");
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
            let chat_path = format!("/window/content/{channel}_chat_layer");
            let chat_layer = sg_root2.lookup_node(chat_path).unwrap();
            if chat_layer.get_property_bool("is_visible").unwrap() {
                continue
            }

            let node_path = format!("/window/content/menu_layer/{channel}_channel_label");
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
    let renderer2 = renderer.clone();
    let listen_connect = ex.spawn(async move {
        let net0 = sg_root2.lookup_node("/window/content/netstatus_layer/net0").unwrap();
        let net1 = sg_root2.lookup_node("/window/content/netstatus_layer/net1").unwrap();
        let net2 = sg_root2.lookup_node("/window/content/netstatus_layer/net2").unwrap();
        let net3 = sg_root2.lookup_node("/window/content/netstatus_layer/net3").unwrap();

        let net0_is_visible = PropertyBool::wrap(&net0, Role::App, "is_visible", 0).unwrap();
        let net1_is_visible = PropertyBool::wrap(&net1, Role::App, "is_visible", 0).unwrap();
        let net2_is_visible = PropertyBool::wrap(&net2, Role::App, "is_visible", 0).unwrap();
        let net3_is_visible = PropertyBool::wrap(&net3, Role::App, "is_visible", 0).unwrap();

        while let Ok(data) = recvr.recv().await {
            let (peers_count, is_dag_synced): (u32, bool) = deserialize(&data).unwrap();

            let atom = &mut renderer2.make_guard(gfxtag!("netstatus change"));

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

    let fud = create_fud("fud");
    let sg_root2 = sg_root.clone();
    let fud = fud
        .setup(|me| async {
            plugin::FudPlugin::new(me, sg_root2, ex.clone()).await.expect("Fud pimpl setup")
        })
        .await;

    let (slot, recv) = Slot::new("file_status_update");
    let _ = fud.register("file_status_updated", slot);
    let sg_root2 = sg_root.clone();
    let listen_file_status = ex.spawn(async move {
        while let Ok(data) = recv.recv().await {
            let window = sg_root2.lookup_node("/window/content").unwrap();
            let mut cur = Cursor::new(&data);
            let url = Url::decode(&mut cur).unwrap();
            let status = chatview::FileMessageStatus::decode(&mut cur).unwrap();
            for child in window.get_children() {
                if let Some(chatty) = child.lookup_node("/content/chatty") {
                    let mut data = vec![];
                    url.encode(&mut data).unwrap();
                    status.encode(&mut data).unwrap();
                    let _ = chatty.call_method("set_file_status", data).await;
                }
            }
        }
    });

    plugin.link(fud);

    let drk = create_drk("drk");
    let sg_root2 = sg_root.clone();
    let ex2 = ex.clone();
    let (drk_pimpl_send, drk_pimpl_recv) = smol::channel::bounded(1);
    let drk = drk
        .setup(move |me| async move {
            // Drk uses rusqlite which is not Send, so we run it on a dedicated thread
            let handle = std::thread::spawn(move || {
                let (pimpl, local_ex) = smol::block_on(plugin::DrkPlugin::new(me, sg_root2, ex2))
                    .expect("Drk pimpl setup");

                // Send Pimpl back to the setup closure
                smol::block_on(drk_pimpl_send.send(pimpl)).unwrap();

                // Block on local executor to process spawned tasks forever
                smol::block_on(local_ex.run(async { futures::future::pending::<()>().await }));
            });

            // Wait for Pimpl to be created
            let pimpl = drk_pimpl_recv.recv().await.unwrap();

            drop(handle);

            pimpl
        })
        .await;

    let (slot, recvr) = Slot::new("connect");
    drk.register("connect", slot).unwrap();
    let sg_root2 = sg_root.clone();
    let renderer2 = renderer.clone();
    let listen_connect = ex.spawn(async move {
        let net0 = sg_root2.lookup_node("/window/content/wallet/netstatus_layer/net0").unwrap();
        let net1 = sg_root2.lookup_node("/window/content/wallet/netstatus_layer/net1").unwrap();
        let net2 = sg_root2.lookup_node("/window/content/wallet/netstatus_layer/net2").unwrap();
        let net3 = sg_root2.lookup_node("/window/content/wallet/netstatus_layer/net3").unwrap();

        let net0_is_visible = PropertyBool::wrap(&net0, Role::App, "is_visible", 0).unwrap();
        let net1_is_visible = PropertyBool::wrap(&net1, Role::App, "is_visible", 0).unwrap();
        let net2_is_visible = PropertyBool::wrap(&net2, Role::App, "is_visible", 0).unwrap();
        let net3_is_visible = PropertyBool::wrap(&net3, Role::App, "is_visible", 0).unwrap();

        while let Ok(data) = recvr.recv().await {
            let status: u8 = deserialize(&data).unwrap();
            let atom = &mut renderer2.make_guard(gfxtag!("blockchain netstatus change"));

            match status {
                1 => {
                    net0_is_visible.set(atom, false);
                    net1_is_visible.set(atom, true);
                    net2_is_visible.set(atom, false);
                    net3_is_visible.set(atom, false);
                },
                2 => {
                    net0_is_visible.set(atom, false);
                    net1_is_visible.set(atom, false);
                    net2_is_visible.set(atom, true);
                    net3_is_visible.set(atom, false);
                },
                3 => {
                    net0_is_visible.set(atom, false);
                    net1_is_visible.set(atom, false);
                    net2_is_visible.set(atom, false);
                    net3_is_visible.set(atom, true);
                },
                _ => {
                    net0_is_visible.set(atom, true);
                    net1_is_visible.set(atom, false);
                    net2_is_visible.set(atom, false);
                    net3_is_visible.set(atom, false);
                }
            }
        }
    });

    let (slot, recv) = Slot::new("balances_update");
    let _ = drk.register("balances_updated", slot);
    let sg_root2 = sg_root.clone();
    let renderer2 = renderer.clone();
    let drk_node2 = drk.clone();
    let listen_balances = ex.spawn(async move {
        use crate::ui::TokenRow;
        use darkfi_money_contract::model::TokenId;
        use darkfi_serial::Encodable;

        let update = async || {
            d!("drk balances_updated signal received");

            // Fetch and update main wallet tokens table
            if let Ok(Some(response_data)) = drk_node2.call_method("get_balances", vec![]).await {
                let atom = &mut renderer2.make_guard(gfxtag!("wallet - refresh tokens"));

                let mut cur = std::io::Cursor::new(response_data);
                if let Ok(balances) = Vec::<(String, TokenId, u64)>::decode(&mut cur) {
                    let token_rows: Vec<TokenRow> = balances
                        .iter()
                        .enumerate()
                        .map(|(i, (symbol, token_id, balance))| {
                            TokenRow {
                                id: *token_id,
                                symbol: symbol.clone(),
                                balance: encode_base10(*balance, 8),
                            }
                        })
                        .collect();

                    let mut data: Vec<u8> = vec![];
                    for row in &token_rows {
                        let _ = TokenRow::encode(row, &mut data);
                    }

                    if let Some(tokens_table) = sg_root2.lookup_node("/window/content/wallet/main_layer/tokens_table") {
                        let _ = tokens_table.call_method("set_tokens", data.clone()).await;
                    }

                    if let Some(send_tokens_table) = sg_root2.lookup_node("/window/content/wallet/send_step1_layer/tokens_table") {
                        let _ = send_tokens_table.call_method("set_tokens", data).await;
                    }

                    // Update main wallet balance
                    if let Some(drk_row) = token_rows.iter().find(|row| row.id == *DARK_TOKEN_ID) {
                        if let Some(balance_node) = sg_root2.lookup_node("/window/content/wallet/main_layer/wallet_balance") {
                            balance_node.set_property_str(atom, Role::App, "text", format!("DRK {}", drk_row.balance)).unwrap();
                        }
                    }

                    if let Some(tx_status_layer) = sg_root2.lookup_node("/window/content/wallet/tx_status_layer") {
                        let tx_id = tx_status_layer.get_property_str("tx_id").unwrap();
                        if !tx_id.is_empty() {
                            let mut tx_id_data = vec![];
                            tx_id.encode(&mut tx_id_data).unwrap();
                            if let Ok(Some(data)) = drk_node2.call_method("get_tx_status", tx_id_data).await {
                                let mut cur = std::io::Cursor::new(data);
                                let status_text = String::decode(&mut cur).unwrap();
                                if let Some(status_node) = tx_status_layer.lookup_node("/status") {
                                    status_node.set_property_str(atom, Role::App, "text", status_text).unwrap();
                                }
                            }
                        }
                    }
                }
            }
        };

        update().await;
        while let Ok(_) = recv.recv().await {
            update().await;
        }
    });

    let (slot, recv) = Slot::new("tx_updated");
    let _ = drk.register("tx_updated", slot);
    let sg_root2 = sg_root.clone();
    let listen_tx = ex.spawn(async move {
        while let Ok(data) = recv.recv().await {
            if let Some(tx_status_layer) = sg_root2.lookup_node("/window/content/wallet/tx_status_layer") {
                let _ = tx_status_layer.call_method("set_tx_status", data).await;
            }
        }
    });

    // Listen for tx_built signal - emitted when transaction is built (non-blocking)
    let (slot, recv) = Slot::new("tx_built");
    let _ = drk.register("tx_built", slot);
    let sg_root2 = sg_root.clone();
    let renderer2 = renderer.clone();
    let listen_tx_built = ex.spawn(async move {
        while let Ok(data) = recv.recv().await {
            let mut cur = std::io::Cursor::new(data);
            let amount = String::decode(&mut cur).unwrap();
            let token_symbol = String::decode(&mut cur).unwrap();
            let recipient_str = String::decode(&mut cur).unwrap();

            // Decode transaction and pass to wallet schema
            let tx = Transaction::decode(&mut cur).unwrap();

            // Update tx_status_layer with built transaction
            let atom = &mut renderer2.make_guard(gfxtag!("tx built"));
            if let Some(tx_status) = sg_root2.lookup_node("/window/content/wallet/tx_status_layer") {
                let mut tx_status_data = vec![];
                None::<String>.encode(&mut tx_status_data).unwrap();
                Some("Broadcasting transaction...".to_string()).encode(&mut tx_status_data).unwrap();
                Some(amount).encode(&mut tx_status_data).unwrap();
                Some(token_symbol).encode(&mut tx_status_data).unwrap();
                Some(recipient_str).encode(&mut tx_status_data).unwrap();
                let _ = tx_status.call_method("set_tx_status", tx_status_data).await;

                // Call set_built_tx to store transaction for later broadcast
                let mut set_built_tx_data = vec![];
                tx.encode(&mut set_built_tx_data).unwrap();
                let _ = tx_status.call_method("set_built_tx", set_built_tx_data).await;
            }

            // Hide step3 layer
            if let Some(step4_layer) = sg_root2.lookup_node("/window/content/wallet/send_step3_layer") {
                step4_layer.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
            }

            // Show step4 layer
            if let Some(step4_layer) = sg_root2.lookup_node("/window/content/wallet/send_step4_layer") {
                step4_layer.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
            }
        }
    });

    // Listen for tx_built_error signal - emitted when transaction building fails
    let (slot, recv) = Slot::new("tx_built_error");
    let _ = drk.register("tx_built_error", slot);
    let sg_root2 = sg_root.clone();
    let renderer2 = renderer.clone();
    let listen_tx_built_error = ex.spawn(async move {
        while let Ok(data) = recv.recv().await {
            let mut cur = std::io::Cursor::new(data);
            let error_message = String::decode(&mut cur).unwrap();
            let atom = &mut renderer2.make_guard(gfxtag!("tx built error"));

            // TODO: display error somewhere

            // Reset button state
            if let Some(btn_node) = sg_root2.lookup_node("/window/content/wallet/send_step3_layer/send_amount_button") {
                btn_node.set_property_bool(atom, Role::App, "is_active", true).unwrap();
                if let Some(label_node) = sg_root2.lookup_node("/window/content/wallet/send_step3_layer/send_amount_button_label") {
                    label_node.set_property_str(atom, Role::App, "text", "add amount").unwrap();
                }
            }
        }
    });

    plugin.link(drk);

    i!("Plugins loaded");
    futures::join!(listen_recv, listen_connect, listen_file_status, listen_balances, listen_tx, listen_tx_built, listen_tx_built_error);
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

    node.add_method("reconnect", vec![], None).unwrap();

    node
}

pub fn create_fud(name: &str) -> SceneNode {
    t!("create_fud({name})");
    let mut node = SceneNode::new(name, SceneNodeType::Plugin);

    let mut prop = Property::new("ready", PropertyType::Bool, PropertySubType::Null);
    prop.set_defaults_bool(vec![false]).unwrap();
    node.add_property(prop).unwrap();

    node.add_signal(
        "file_status_updated",
        "File download status updated",
        vec![("url", "File URL", CallArgType::Str), ("status", "File status", CallArgType::Str)],
    )
    .unwrap();

    node.add_method("get", vec![("url", "Url", CallArgType::Str)], None).unwrap();
    node.add_method("track_file", vec![("url", "Url", CallArgType::Str)], None).unwrap();

    node
}

pub fn create_drk(name: &str) -> SceneNode {
    t!("create_drk({name})");
    let mut node = SceneNode::new(name, SceneNodeType::Plugin);

    node.add_signal(
        "connect",
        "Connections and disconnects",
        vec![
            ("connected", "Is darkfid connected", CallArgType::Bool),
        ],
    )
    .unwrap();

    node.add_method(
        "get_default_address",
        vec![],
        Some(vec![("address", "Default address", CallArgType::Str)]),
    ).unwrap();

    node.add_method(
        "get_balances",
        vec![],
        Some(vec![("balances", "Token balances", CallArgType::Hash)]),
    ).unwrap();

    node.add_method(
        "get_tx_status",
        vec![("tx_id", "Transaction hash", CallArgType::Str)],
        Some(vec![("status_text", "Status text", CallArgType::Str)]),
    ).unwrap();

    node.add_method(
        "build_tx",
        vec![
            ("amount", "Amount", CallArgType::Str),
            ("token_id", "Token ID", CallArgType::Hash),
            ("recipient", "Recipient address", CallArgType::Str),
        ],
        Some(vec![("tx", "Transaction", CallArgType::Hash)]),
    ).unwrap();

    node.add_method(
        "broadcast_tx",
        vec![("tx", "Transaction", CallArgType::Hash)],
        Some(vec![("status_text", "Status text", CallArgType::Str)]),
    ).unwrap();

    node.add_signal("balances_updated", "Balances changed", vec![]).unwrap();

    node.add_signal(
        "tx_updated",
        "Transaction status updated",
        vec![
            ("tx_id", "Transaction ID", CallArgType::Str),
            ("status_text", "Transaction status text", CallArgType::Str),
        ],
    ).unwrap();

    node.add_signal(
        "tx_built",
        "Transaction built - for wallet send flow",
        vec![
            ("amount", "Amount", CallArgType::Str),
            ("token_symbol", "Token symbol", CallArgType::Str),
            ("recipient_str", "Recipient address", CallArgType::Str),
        ],
    ).unwrap();

    node.add_signal(
        "tx_built_error",
        "Transaction build error",
        vec![
            ("error_message", "Error message", CallArgType::Str),
        ],
    ).unwrap();

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

    // Reuse renderer and event_pub
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
