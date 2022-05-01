use async_std::sync::{Arc, Mutex};
use std::{fs::File, io, io::Read, path::PathBuf};

use easy_parallel::Parallel;
use fxhash::{FxHashMap, FxHashSet};
use log::{debug, info};
use serde_json::{json, Value};
use simplelog::*;
use smol::Executor;

use termion::{async_stdin, event::Key, input::TermRead, raw::IntoRawMode};
use tui::{
    backend::{Backend, TermionBackend},
    Terminal,
};
use url::Url;

use darkfi::{
    error::{Error, Result},
    rpc::{jsonrpc, jsonrpc::JsonResult},
    util::{
        async_util,
        cli::{log_config, spawn_config, Config},
        join_config_path,
    },
};

use dnetview::{
    config::{DnvConfig, CONFIG_FILE_CONTENTS},
    model::{ConnectInfo, Model, NodeInfo, SelectableObject, Session, SessionInfo},
    options::ProgramOptions,
    util::{is_empty_session, make_connect_id, make_empty_id, make_node_id, make_session_id},
    view::{IdListView, NodeInfoView, View},
};

struct DNetView {
    url: Url,
    name: String,
}

impl DNetView {
    pub fn new(url: Url, name: String) -> Self {
        Self { url, name }
    }

    async fn request(&self, r: jsonrpc::JsonRequest) -> Result<Value> {
        let reply: JsonResult = match jsonrpc::send_request(&self.url, json!(r), None).await {
            Ok(v) => v,
            Err(e) => return Err(e),
        };

        match reply {
            JsonResult::Resp(r) => {
                debug!(target: "RPC", "<-- {}", serde_json::to_string(&r)?);
                Ok(r.result)
            }

            JsonResult::Err(e) => {
                debug!(target: "RPC", "<-- {}", serde_json::to_string(&e)?);
                Err(Error::JsonRpcError(e.error.message.to_string()))
            }

            JsonResult::Notif(n) => {
                debug!(target: "RPC", "<-- {}", serde_json::to_string(&n)?);
                Err(Error::JsonRpcError("Unexpected reply".to_string()))
            }
        }
    }

    // --> {"jsonrpc": "2.0", "method": "ping", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": "pong", "id": 42}
    async fn _ping(&self) -> Result<Value> {
        let req = jsonrpc::request(json!("ping"), json!([]));
        Ok(self.request(req).await?)
    }

    //--> {"jsonrpc": "2.0", "method": "poll", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": {"nodeID": [], "nodeinfo" [], "id": 42}
    async fn get_info(&self) -> Result<Value> {
        let req = jsonrpc::request(json!("get_info"), json!([]));
        Ok(self.request(req).await?)
    }
}

#[async_std::main]
async fn main() -> Result<()> {
    let options = ProgramOptions::load()?;

    let verbosity_level = options.app.occurrences_of("verbose");

    let (lvl, cfg) = log_config(verbosity_level)?;

    let file = File::create(&*options.log_path).unwrap();
    WriteLogger::init(lvl, cfg, file)?;
    info!("Log level: {}", lvl);

    let config_path = join_config_path(&PathBuf::from("dnetview_config.toml"))?;

    spawn_config(&config_path, CONFIG_FILE_CONTENTS)?;

    let config = Config::<DnvConfig>::load(config_path)?;

    let stdout = io::stdout().into_raw_mode()?;
    let backend = TermionBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    terminal.clear()?;

    let ids = Mutex::new(FxHashSet::default());
    let node_info = Mutex::new(FxHashMap::default());
    let select_info = Mutex::new(FxHashMap::default());

    let model = Arc::new(Model::new(ids, node_info, select_info));

    let nthreads = num_cpus::get();
    let (signal, shutdown) = async_channel::unbounded::<()>();

    let ex = Arc::new(Executor::new());
    let ex2 = ex.clone();

    let (_, result) = Parallel::new()
        .each(0..nthreads, |_| smol::future::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            smol::future::block_on(async move {
                run_rpc(&config, ex2.clone(), model.clone()).await?;
                render(&mut terminal, model.clone()).await?;
                drop(signal);
                Ok::<(), darkfi::Error>(())
            })
        });

    result
}

async fn run_rpc(config: &DnvConfig, ex: Arc<Executor<'_>>, model: Arc<Model>) -> Result<()> {
    for node in config.nodes.clone() {
        let client = DNetView::new(Url::parse(&node.rpc_url)?, node.name);
        ex.spawn(poll(client, model.clone())).detach();
    }
    Ok(())
}

async fn poll(client: DNetView, model: Arc<Model>) -> Result<()> {
    loop {
        let reply = client.get_info().await?;

        if reply.as_object().is_some() && !reply.as_object().unwrap().is_empty() {
            parse_data(reply.as_object().unwrap(), &client, model.clone()).await?;
        } else {
            // TODO: error handling
            //debug!("Reply is empty");
        }
        async_util::sleep(2).await;
    }
}

async fn parse_data(
    reply: &serde_json::Map<String, Value>,
    client: &DNetView,
    model: Arc<Model>,
) -> Result<()> {
    let _ext_addr = reply.get("external_addr");
    let inbound = &reply["session_inbound"];
    let manual = &reply["session_manual"];
    let outbound = &reply["session_outbound"];

    let mut sessions: Vec<SessionInfo> = Vec::new();

    let node_name = &client.name;
    let node_id = make_node_id(node_name)?;

    let in_session = parse_inbound(inbound, node_id.clone()).await?;
    let out_session = parse_outbound(outbound, node_id.clone()).await?;
    let man_session = parse_manual(manual, node_id.clone()).await?;

    sessions.push(in_session.clone());
    sessions.push(out_session.clone());
    sessions.push(man_session.clone());

    let node_info = NodeInfo::new(node_id.clone(), node_name.to_string(), sessions.clone());

    update_node_info(model.clone(), node_info.clone(), node_id.clone()).await;
    update_selectable_and_ids(model.clone(), sessions.clone(), node_info.clone()).await?;

    //debug!("IDS: {:?}", model.ids.lock().await);
    //debug!("INFOS: {:?}", model.infos.lock().await);

    Ok(())
}

async fn update_ids(model: Arc<Model>, id: String) {
    model.ids.lock().await.insert(id);
}

async fn update_node_info(model: Arc<Model>, node: NodeInfo, id: String) {
    model.node_info.lock().await.insert(id, node);
}

async fn update_selectable_and_ids(
    model: Arc<Model>,
    sessions: Vec<SessionInfo>,
    node_info: NodeInfo,
) -> Result<()> {
    let node_obj = SelectableObject::Node(node_info.clone());
    model.select_info.lock().await.insert(node_info.node_id.clone(), node_obj);
    update_ids(model.clone(), node_info.node_id.clone()).await;
    for session in sessions.clone() {
        let session_obj = SelectableObject::Session(session.clone());
        model.select_info.lock().await.insert(session.clone().session_id, session_obj);
        update_ids(model.clone(), session.clone().session_id).await;
        for connect in session.children {
            let connect_obj = SelectableObject::Connect(connect.clone());
            model.select_info.lock().await.insert(connect.clone().connect_id, connect_obj);
            update_ids(model.clone(), connect.clone().connect_id).await;
        }
    }
    Ok(())
}

async fn parse_inbound(inbound: &Value, node_id: String) -> Result<SessionInfo> {
    let session_name = "Inbound".to_string();
    let session_type = Session::Inbound;
    let session_id = make_session_id(node_id.clone(), &session_type)?;
    let mut connects: Vec<ConnectInfo> = Vec::new();
    let connections = &inbound["connected"];
    let mut connect_count = 0;

    match connections.as_object() {
        Some(connect) => {
            match connect.is_empty() {
                true => {
                    connect_count += 1;
                    // channel is empty. initialize with empty values
                    // TODO: fix this
                    let connect_id = make_empty_id(node_id.clone(), &session_type, connect_count)?;
                    let addr = "Null".to_string();
                    let msg = "Null".to_string();
                    let status = "Null".to_string();
                    let is_empty = true;
                    let parent = session_id.clone();
                    let state = "Null".to_string();
                    let msg_log = Vec::new();
                    let connect_info = ConnectInfo::new(
                        connect_id, addr, is_empty, msg, status, state, msg_log, parent,
                    );
                    connects.push(connect_info.clone());
                }
                false => {
                    // channel is not empty. initialize with whole values
                    for k in connect.keys() {
                        let node = connect.get(k);
                        let addr = k.to_string();
                        let msg =
                            node.unwrap().get("last_msg").unwrap().as_str().unwrap().to_string();
                        let status =
                            node.unwrap().get("last_status").unwrap().as_str().unwrap().to_string();
                        // TODO: state
                        let id = node.unwrap().get("random_id").unwrap().as_u64().unwrap();
                        let connect_id = make_connect_id(id)?;
                        let state = "state".to_string();
                        let is_empty = false;
                        let parent = session_id.clone();
                        let msg_values = node.unwrap().get("log").unwrap().as_array().unwrap();
                        let mut msgs: Vec<(String, String)> = Vec::new();
                        for msg in msg_values {
                            let msg: (String, String) = serde_json::from_value(msg.clone())?;
                            msgs.push(msg);
                        }
                        let connect_info = ConnectInfo::new(
                            connect_id, addr, is_empty, msg, status, state, msgs, parent,
                        );
                        connects.push(connect_info.clone());
                    }
                }
            }
            let is_empty = is_empty_session(connects.clone());

            let session_info = SessionInfo::new(
                session_name,
                session_id.clone(),
                node_id.clone(),
                connects.clone(),
                is_empty,
            );
            Ok(session_info)
        }
        None => Err(Error::ValueIsNotObject),
    }
}

// TODO: placeholder for now
async fn parse_manual(_manual: &Value, node_id: String) -> Result<SessionInfo> {
    let session_name = "Manual".to_string();
    let session_type = Session::Manual;
    let mut connects: Vec<ConnectInfo> = Vec::new();

    let session_id = make_session_id(node_id.clone(), &session_type)?;
    let id: u64 = 0;
    let connect_id = make_connect_id(id)?;
    let addr = "Null".to_string();
    let msg = "Null".to_string();
    let status = "Null".to_string();
    let is_empty = true;
    let parent = session_id.clone();
    let state = "Null".to_string();
    let msg_log = Vec::new();
    let connect_info =
        ConnectInfo::new(connect_id, addr, is_empty, msg, status, state, msg_log, parent);
    connects.push(connect_info.clone());
    let is_empty = is_empty_session(connects.clone());
    //let is_empty = false;
    let session_info =
        SessionInfo::new(session_name, session_id, node_id, connects.clone(), is_empty);

    Ok(session_info)
}

async fn parse_outbound(outbound: &Value, node_id: String) -> Result<SessionInfo> {
    let session_name = "Outbound".to_string();
    let session_type = Session::Outbound;
    let mut connects: Vec<ConnectInfo> = Vec::new();
    let slots = &outbound["slots"];
    let session_id = make_session_id(node_id.clone(), &session_type)?;
    let mut slot_count = 0;

    match slots.as_array() {
        Some(slots) => {
            for slot in slots {
                slot_count += 1;
                match slot["channel"].is_null() {
                    true => {
                        // channel is empty. initialize with empty values
                        // TODO: fix this
                        let connect_id = make_empty_id(node_id.clone(), &session_type, slot_count)?;
                        let is_empty = true;
                        let addr = "Null".to_string();
                        let state = &slot["state"];
                        let msg = "Null".to_string();
                        let status = "Null".to_string();
                        // TODO: msg log
                        let msg_log = Vec::new();
                        let parent = session_id.clone();
                        let connect_info = ConnectInfo::new(
                            connect_id,
                            addr,
                            is_empty,
                            msg,
                            status,
                            state.as_str().unwrap().to_string(),
                            msg_log,
                            parent,
                        );
                        connects.push(connect_info.clone());
                    }
                    false => {
                        // channel is not empty. initialize with whole values
                        let channel = &slot["channel"];
                        let last_msg = channel["last_msg"].as_str().unwrap().to_string();
                        let last_status = channel["last_status"].as_str().unwrap().to_string();
                        let id = channel["random_id"].as_u64().unwrap();
                        let msg_values = channel["log"].as_array().unwrap();
                        let connect_id = make_connect_id(id)?;
                        let is_empty = false;
                        let addr = &slot["addr"];
                        let state = &slot["state"];
                        let parent = session_id.clone();
                        let mut msgs: Vec<(String, String)> = Vec::new();
                        for msg in msg_values {
                            let msg: (String, String) = serde_json::from_value(msg.clone())?;
                            msgs.push(msg);
                        }
                        let connect_info = ConnectInfo::new(
                            connect_id,
                            addr.as_str().unwrap().to_string(),
                            is_empty,
                            last_msg,
                            last_status,
                            state.as_str().unwrap().to_string(),
                            msgs,
                            parent,
                        );
                        connects.push(connect_info.clone());
                    }
                }
            }

            let is_empty = is_empty_session(connects.clone());

            let session_info =
                SessionInfo::new(session_name, session_id, node_id, connects.clone(), is_empty);
            Ok(session_info)
        }
        None => Err(Error::ValueIsNotObject),
    }
}

async fn render<B: Backend>(terminal: &mut Terminal<B>, model: Arc<Model>) -> Result<()> {
    let mut asi = async_stdin();

    terminal.clear()?;

    let active_ids = IdListView::new(FxHashSet::default());
    let info_list = NodeInfoView::new(FxHashMap::default());
    let selectable = FxHashMap::default();

    let mut view = View::new(active_ids.clone(), info_list.clone(), selectable);
    view.active_ids.state.select(Some(0));

    loop {
        view.update(model.node_info.lock().await.clone(), model.select_info.lock().await.clone());

        terminal.draw(|f| {
            view.clone().render(f);
        })?;
        for k in asi.by_ref().keys() {
            match k.unwrap() {
                Key::Char('q') => {
                    terminal.clear()?;
                    return Ok(())
                }
                Key::Char('j') => {
                    view.active_ids.next();
                }
                Key::Char('k') => {
                    view.active_ids.previous();
                }
                _ => (),
            }
        }
    }
}
