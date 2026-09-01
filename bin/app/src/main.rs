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
use darkfi::{system::CondVar, tx::Transaction, util::parse::encode_base10};
use darkfi_money_contract::model::DARK_TOKEN_ID;
use darkfi_serial::{deserialize, Decodable, Encodable};
use smol::Task;
use std::sync::{Arc, OnceLock};

#[macro_use]
extern crate tracing;

#[cfg(target_os = "android")]
mod android;
mod app;
mod build_info;
mod db;
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
mod setting;
mod sfx;
mod shape;
mod text;
mod ui;
mod util;

use crate::{
    app::{App, AppPtr},
    db::{AppDb, AppDbPtr},
    gfx::EpochIndex,
    prop::{Property, PropertySubType, PropertyType},
    scene::{CallArgType, SceneNode, SceneNodePtr, SceneNodeType},
    ui::RedrawTrigger,
    util::AsyncRuntime,
};
#[cfg(feature = "enable-netdebug")]
use net::ZeroMQAdapter;
use {
    app::schema::get_main_db_path,
    // Local imports
    db::get_app_db_path,
    gfx::Renderer,
    // Global imports
    kvdb_overlay::Database as KvDb,
    prop::{PropertyBool, PropertyStr, Role},
    scene::Slot,
    std::io::Cursor,
    ui::chatview,
    url::Url,
};

// This is historical, but ideally we can fix the entire project and remove this import.
pub use util::ExecutorPtr;

macro_rules! t { ($($arg:tt)*) => { trace!(target: "main", $($arg)*); } }
macro_rules! d { ($($arg:tt)*) => { trace!(target: "main", $($arg)*); } }
macro_rules! i { ($($arg:tt)*) => { trace!(target: "main", $($arg)*); } }

fn panic_hook(panic_info: &std::panic::PanicHookInfo) {
    error!("panic occurred: {panic_info}");
    let backtrace = std::backtrace::Backtrace::force_capture().to_string();
    error!("{backtrace}");

    if let Some(logfile_path) = logger::cached_logfile_path() {
        let timestamp = chrono::Utc::now().to_rfc3339();
        let report = format!("[{timestamp}] PANIC: {panic_info}\n{backtrace}\n");
        let _ = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(logfile_path)
            .and_then(|mut file| std::io::Write::write_all(&mut file, report.as_bytes()));
    }

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
        #[cfg(feature = "enable-filelog")]
        logger::init_logfile_path();

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

        let db_path = get_main_db_path();
        let kv_db = KvDb::open_default(&db_path).expect("KVDB failed to open");
        let app_db_path = get_app_db_path();
        let app_db = smol::block_on(AppDb::new(app_db_path.to_str().unwrap()))
            .expect("turso app db failed to open");

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
        let kv_db2 = kv_db.clone();
        let app_db2 = app_db.clone();
        let app_task = fg_ex.spawn(async move {
            app2.setup(kv_db2, app_db2).await;
            cv.notify();
        });
        fg_runtime.push_task(app_task);

        #[cfg(feature = "enable-netdebug")]
        {
            let sg_root = sg_root.clone();
            let ex = bg_ex.clone();
            let renderer = app.renderer.clone();
            let redraw = app.redraw_trigger.clone();
            let zmq_task = bg_ex.spawn(async {
                i!("Enabled net debugging backend in this build");
                let zmq_rpc = ZeroMQAdapter::new(sg_root, renderer, redraw, ex).await;
                zmq_rpc.run().await;
            });
            bg_runtime.push_task(zmq_task);
        }

        {
            let ex = bg_ex.clone();
            let cv = cv_app_is_setup.clone();
            let redraw = app.redraw_trigger.clone();
            let plug_task = bg_ex.spawn(async move {
                load_plugins(ex, sg_root, redraw, cv, kv_db, app_db).await;
            });
            bg_runtime.push_task(plug_task);
        }

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

async fn load_plugins(
    ex: ExecutorPtr,
    sg_root: SceneNodePtr,
    redraw: RedrawTrigger,
    cv: Arc<CondVar>,
    kv_db: KvDb,
    app_db: AppDbPtr,
) {
    let plugin = SceneNode::new("plugin", SceneNodeType::PluginRoot);
    let plugin = plugin.setup_null();
    sg_root.link(plugin.clone());

    // DarkIrc needs /window to start
    cv.wait().await;

    let mut listeners: Vec<Task<()>> = vec![];

    #[cfg(feature = "enable-plugin-darkirc")]
    {
        let darkirc = create_darkirc("darkirc");
        let darkirc = darkirc
            .setup(|me| async {
                plugin::DarkIrc::new(me, sg_root.clone(), ex.clone(), kv_db, app_db)
                    .await
                    .expect("DarkIrc pimpl setup")
            })
            .await;

        let (slot, recvr) = Slot::new("recvmsg");
        darkirc.register("recv", slot).unwrap();
        let sg_root2 = sg_root.clone();
        let darkirc_nick = PropertyStr::wrap(&darkirc, Role::App, "nick", 0).unwrap();
        let redraw2 = redraw.clone();
        let listen_recv = ex.spawn(async move {
        while let Ok(data) = recvr.recv().await {
            let atom = &mut redraw2.make_guard(gfxtag!("darkirc msg recv"));

            let mut cur = Cursor::new(&data);
            let channel = String::decode(&mut cur).unwrap();
            let timestamp = chatview::Timestamp::decode(&mut cur).unwrap();
            let id = chatview::MessageId::decode(&mut cur).unwrap();
            let nick = String::decode(&mut cur).unwrap();
            let msg = String::decode(&mut cur).unwrap();

            let node_path = format!("/window/content/chat/{channel}_chat_layer/content/chatty");
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
            let chat_path = format!("/window/content/chat/{channel}_chat_layer");
            let chat_layer = sg_root2.lookup_node(chat_path).unwrap();
            if chat_layer.get_property_bool("is_visible").unwrap() {
                continue
            }

            let menu_node =
                sg_root2.lookup_node("/window/content/chat/menu_layer/main_menu").unwrap();
            let group_name = if msg.contains(&darkirc_nick.get()) { "role2_group" } else { "role1_group" };
            let group = menu_node.get_property(group_name).unwrap();
            if !group.get_str_vec().unwrap().contains(&channel) {
                group.push_str(atom, Role::App, &channel).unwrap();
            }
        }
    });

        let (slot, recvr) = Slot::new("connect");
        darkirc.register("connect", slot).unwrap();
        let sg_root2 = sg_root.clone();
        let redraw2 = redraw.clone();
        let listen_connect = ex.spawn(async move {
            let net0 = sg_root2.lookup_node("/window/content/chat/netstatus_layer/net0").unwrap();
            let net1 = sg_root2.lookup_node("/window/content/chat/netstatus_layer/net1").unwrap();
            let net2 = sg_root2.lookup_node("/window/content/chat/netstatus_layer/net2").unwrap();
            let net3 = sg_root2.lookup_node("/window/content/chat/netstatus_layer/net3").unwrap();

            let net0_is_visible = PropertyBool::wrap(&net0, Role::App, "is_visible", 0).unwrap();
            let net1_is_visible = PropertyBool::wrap(&net1, Role::App, "is_visible", 0).unwrap();
            let net2_is_visible = PropertyBool::wrap(&net2, Role::App, "is_visible", 0).unwrap();
            let net3_is_visible = PropertyBool::wrap(&net3, Role::App, "is_visible", 0).unwrap();

            while let Ok(data) = recvr.recv().await {
                let (peers_count, is_dag_synced): (u32, bool) = deserialize(&data).unwrap();

                let atom = &mut redraw2.make_guard(gfxtag!("netstatus change"));

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

        listeners.push(listen_recv);
        listeners.push(listen_connect);
    }

    #[cfg(feature = "enable-plugin-fud")]
    {
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

        listeners.push(listen_file_status);
    }

    #[cfg(feature = "enable-plugin-drk")]
    {
        let drk = create_drk("drk");
        let drk = drk
            .setup(|me| async {
                plugin::DrkPlugin::new(me, sg_root.clone(), ex.clone())
                    .await
                    .expect("Drk pimpl setup")
            })
            .await;

        let (slot, recvr) = Slot::new("connect");
        drk.register("connect", slot).unwrap();
        let sg_root2 = sg_root.clone();
        let redraw2 = redraw.clone();
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
                let (status, desc): (u8, String) = deserialize(&data).unwrap();
                let atom = &mut redraw2.make_guard(gfxtag!("blockchain netstatus change"));

                if let Some(progress_node) =
                    sg_root2.lookup_node("/window/content/wallet/netstatus_layer/progress")
                {
                    progress_node.set_property_str(atom, Role::App, "text", &desc).unwrap();
                }

                match status {
                    1 => {
                        net0_is_visible.set(atom, false);
                        net1_is_visible.set(atom, true);
                        net2_is_visible.set(atom, false);
                        net3_is_visible.set(atom, false);
                    }
                    2 => {
                        net0_is_visible.set(atom, false);
                        net1_is_visible.set(atom, false);
                        net2_is_visible.set(atom, true);
                        net3_is_visible.set(atom, false);
                    }
                    3 => {
                        net0_is_visible.set(atom, false);
                        net1_is_visible.set(atom, false);
                        net2_is_visible.set(atom, false);
                        net3_is_visible.set(atom, true);
                    }
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
        let redraw2 = redraw.clone();
        let drk_node2 = drk.clone();
        let listen_balances = ex.spawn(async move {
            use crate::ui::TokenRow;
            use darkfi_money_contract::model::TokenId;
            use darkfi_serial::Encodable;

            let update = async |data: Vec<u8>| {
                d!("drk balances_updated signal received");

                let mut cur = std::io::Cursor::new(data);
                if let Ok(balances) = Vec::<(String, TokenId, u64)>::decode(&mut cur) {
                    let atom = &mut redraw2.make_guard(gfxtag!("wallet - refresh tokens"));

                    let token_rows: Vec<TokenRow> = balances
                        .iter()
                        .map(|(symbol, token_id, balance)| TokenRow {
                            id: *token_id,
                            symbol: symbol.clone(),
                            balance: encode_base10(*balance, 8),
                        })
                        .collect();

                    let mut rows_data: Vec<u8> = vec![];
                    for row in &token_rows {
                        let _ = TokenRow::encode(row, &mut rows_data);
                    }

                    let tokens_table = sg_root2
                        .lookup_node("/window/content/wallet/main_layer/tokens_table")
                        .unwrap();
                    let send_tokens_table = sg_root2
                        .lookup_node("/window/content/wallet/send_step1_layer/tokens_table")
                        .unwrap();

                    tokens_table.call_method("set_tokens", rows_data.clone()).await.unwrap();
                    send_tokens_table.call_method("set_tokens", rows_data).await.unwrap();

                    // Update main wallet balance
                    if let Some(drk_row) = token_rows.iter().find(|row| row.id == *DARK_TOKEN_ID) {
                        let balance_node = sg_root2
                            .lookup_node("/window/content/wallet/main_layer/wallet_balance")
                            .unwrap();
                        balance_node
                            .set_property_str(
                                atom,
                                Role::App,
                                "text",
                                format!("DRK {}", drk_row.balance),
                            )
                            .unwrap();
                    }

                    let tx_status_layer =
                        sg_root2.lookup_node("/window/content/wallet/tx_status_layer").unwrap();
                    let tx_id = tx_status_layer.get_property_str("tx_id").unwrap();
                    if !tx_id.is_empty() {
                        let mut tx_id_data = vec![];
                        tx_id.encode(&mut tx_id_data).unwrap();
                        let status_data = drk_node2
                            .call_method("get_tx_status", tx_id_data)
                            .await
                            .unwrap()
                            .unwrap();

                        let mut cur = std::io::Cursor::new(status_data);
                        let status_text = String::decode(&mut cur).unwrap();
                        let status_node = tx_status_layer.lookup_node("/status").unwrap();
                        status_node.set_property_str(atom, Role::App, "text", status_text).unwrap();
                    }
                }
            };

            let response_data =
                drk_node2.call_method("get_balances", vec![]).await.unwrap().unwrap();
            update(response_data).await;
            while let Ok(data) = recv.recv().await {
                update(data).await;
            }
        });

        let (slot, recv) = Slot::new("tx_updated");
        let _ = drk.register("tx_updated", slot);
        let sg_root2 = sg_root.clone();
        let listen_tx = ex.spawn(async move {
            while let Ok(data) = recv.recv().await {
                if let Some(tx_status_layer) =
                    sg_root2.lookup_node("/window/content/wallet/tx_status_layer")
                {
                    let _ = tx_status_layer.call_method("set_tx_status", data).await;
                }
            }
        });

        // Listen for tx_built signal - emitted when transaction is built (non-blocking)
        let (slot, recv) = Slot::new("tx_built");
        let _ = drk.register("tx_built", slot);
        let sg_root2 = sg_root.clone();
        let redraw2 = redraw.clone();
        let listen_tx_built = ex.spawn(async move {
            while let Ok(data) = recv.recv().await {
                let mut cur = std::io::Cursor::new(data);
                let amount = String::decode(&mut cur).unwrap();
                let token_symbol = String::decode(&mut cur).unwrap();
                let recipient_str = String::decode(&mut cur).unwrap();

                // Decode transaction and pass to wallet schema
                let tx = Transaction::decode(&mut cur).unwrap();

                // Update tx_status_layer with built transaction
                let atom = &mut redraw2.make_guard(gfxtag!("tx built"));
                if let Some(tx_status) =
                    sg_root2.lookup_node("/window/content/wallet/tx_status_layer")
                {
                    let mut tx_status_data = vec![];
                    None::<String>.encode(&mut tx_status_data).unwrap();
                    Some("Broadcasting transaction...".to_string())
                        .encode(&mut tx_status_data)
                        .unwrap();
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
                if let Some(step4_layer) =
                    sg_root2.lookup_node("/window/content/wallet/send_step3_layer")
                {
                    step4_layer.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
                }

                // Show step4 layer
                if let Some(step4_layer) =
                    sg_root2.lookup_node("/window/content/wallet/send_step4_layer")
                {
                    step4_layer.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
                }
            }
        });

        // Listen for tx_built_error signal - emitted when transaction building fails
        let (slot, recv) = Slot::new("tx_built_error");
        let _ = drk.register("tx_built_error", slot);
        let sg_root2 = sg_root.clone();
        let redraw2 = redraw.clone();
        let listen_tx_built_error = ex.spawn(async move {
            while let Ok(data) = recv.recv().await {
                let mut cur = std::io::Cursor::new(data);
                let error_message = String::decode(&mut cur).unwrap();
                let atom = &mut redraw2.make_guard(gfxtag!("tx built error"));

                // Display error message in step3
                if let Some(error_node) =
                    sg_root2.lookup_node("/window/content/wallet/send_step3_layer/error")
                {
                    error_node.set_property_str(atom, Role::App, "text", error_message).unwrap();
                }

                // Reset step4 send button to disabled state
                if let Some(send_label_node) =
                    sg_root2.lookup_node("/window/content/wallet/send_step4_layer/send_btn_label")
                {
                    send_label_node.set_property_str(atom, Role::App, "text", "send").unwrap();
                    let prop = send_label_node.get_property("text_color").unwrap();
                    prop.set_f32(atom, Role::App, 0, 0.5).unwrap();
                    prop.set_f32(atom, Role::App, 1, 0.5).unwrap();
                    prop.set_f32(atom, Role::App, 2, 0.5).unwrap();
                    prop.set_f32(atom, Role::App, 3, 1.).unwrap();
                }
                if let Some(send_bg_grey_node) =
                    sg_root2.lookup_node("/window/content/wallet/send_step4_layer/send_btn_bg_grey")
                {
                    send_bg_grey_node
                        .set_property_bool(atom, Role::App, "is_visible", true)
                        .unwrap();
                }
                if let Some(send_bg_node) =
                    sg_root2.lookup_node("/window/content/wallet/send_step4_layer/send_btn_bg")
                {
                    send_bg_node.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
                }

                // Go back to step3
                if let Some(step4) = sg_root2.lookup_node("/window/content/wallet/send_step4_layer")
                {
                    step4.set_property_bool(atom, Role::App, "is_visible", false).unwrap();
                }
                if let Some(step3) = sg_root2.lookup_node("/window/content/wallet/send_step3_layer")
                {
                    step3.set_property_bool(atom, Role::App, "is_visible", true).unwrap();
                }

                // Focus the amount input
                if let Some(amount_input) = sg_root2.lookup_node(
                    "/window/content/wallet/send_step3_layer/send_amount_wrapper/send_amount_input",
                ) {
                    let _ = amount_input.call_method("focus", vec![]).await;
                }
            }
        });

        plugin.link(drk);

        listeners.push(listen_connect);
        listeners.push(listen_balances);
        listeners.push(listen_tx);
        listeners.push(listen_tx_built);
        listeners.push(listen_tx_built_error);
    }

    i!("Plugins loaded");
    futures::future::join_all(listeners).await;
}

pub fn create_darkirc(name: &str) -> SceneNode {
    t!("create_darkirc({name})");
    let mut node = SceneNode::new(name, SceneNodeType::Plugin);

    let mut prop = Property::new("nick", PropertyType::Str, PropertySubType::Null);
    prop.set_ui_text("Nick", "Nickname");
    prop.set_defaults_str(vec!["anon".to_string()]).unwrap();
    node.add_property(prop).unwrap();

    let mut prop = Property::new("dm_public", PropertyType::Str, PropertySubType::Null);
    prop.set_ui_text("DM Public Key", "Your DM public key (share with contacts)");
    prop.allow_null_values();
    prop.set_defaults_null().unwrap();
    node.add_property(prop).unwrap();

    let mut prop = Property::new("outbound_peers", PropertyType::Str, PropertySubType::Null);
    prop.set_ui_text("Outbound Peers", "Connected outbound peers");
    #[cfg(feature = "enable-plugin-darkirc")]
    prop.set_array_len(plugin::darkirc::P2P_OUTBOUND_ACTIVE);
    prop.allow_null_values();
    prop.set_defaults_null().unwrap();
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

    node.add_method("rescan", vec![("channel", "Channel", CallArgType::Str)], None).unwrap();

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
        "Darkfid connection update",
        vec![
            ("status", "Connection status", CallArgType::Uint32),
            ("description", "Description", CallArgType::Str),
        ],
    )
    .unwrap();

    node.add_method(
        "get_default_address",
        vec![],
        Some(vec![("address", "Default address", CallArgType::Str)]),
    )
    .unwrap();

    node.add_method(
        "get_balances",
        vec![],
        Some(vec![("balances", "Token balances", CallArgType::Hash)]),
    )
    .unwrap();

    node.add_method(
        "get_tx_status",
        vec![("tx_id", "Transaction hash", CallArgType::Str)],
        Some(vec![("status_text", "Status text", CallArgType::Str)]),
    )
    .unwrap();

    node.add_method(
        "build_tx",
        vec![
            ("amount", "Amount", CallArgType::Str),
            ("token_id", "Token ID", CallArgType::Hash),
            ("recipient", "Recipient address", CallArgType::Str),
        ],
        Some(vec![("tx", "Transaction", CallArgType::Hash)]),
    )
    .unwrap();

    node.add_method(
        "broadcast_tx",
        vec![("tx", "Transaction", CallArgType::Hash)],
        Some(vec![("status_text", "Status text", CallArgType::Str)]),
    )
    .unwrap();

    node.add_signal(
        "balances_updated",
        "Balances changed",
        vec![
            ("symbol", "Token symbol", CallArgType::Str),
            ("token_id", "Token ID", CallArgType::Hash),
            ("balance", "Token balance", CallArgType::Uint64),
        ],
    )
    .unwrap();

    node.add_signal(
        "tx_updated",
        "Transaction status updated",
        vec![
            ("tx_id", "Transaction ID", CallArgType::Str),
            ("status_text", "Transaction status text", CallArgType::Str),
        ],
    )
    .unwrap();

    node.add_signal(
        "tx_built",
        "Transaction built - for wallet send flow",
        vec![
            ("amount", "Amount", CallArgType::Str),
            ("token_symbol", "Token symbol", CallArgType::Str),
            ("recipient_str", "Recipient address", CallArgType::Str),
        ],
    )
    .unwrap();

    node.add_signal(
        "tx_built_error",
        "Transaction build error",
        vec![("error_message", "Error message", CallArgType::Str)],
    )
    .unwrap();

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
