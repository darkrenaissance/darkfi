use async_std::sync::{Arc, Mutex};
use std::{collections::hash_map::Entry, fs::File, io, io::Read, path::PathBuf};

use easy_parallel::Parallel;
use fxhash::{FxHashMap, FxHashSet};
use log::info;
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
    error::Result,
    rpc::{jsonrpc, rpcclient::RpcClient},
    util::{
        async_util,
        cli::{log_config, spawn_config, Config},
        join_config_path,
    },
};

use dnetview::{
    config::{DnvConfig, CONFIG_FILE_CONTENTS},
    error::{DnetViewError, DnetViewResult},
    model::{ConnectInfo, Model, NodeInfo, SelectableObject, Session, SessionInfo},
    options::ProgramOptions,
    util::{is_empty_session, make_connect_id, make_empty_id, make_node_id, make_session_id},
    view::{IdListView, NodeInfoView, View},
};

struct DnetView {
    name: String,
    rpc_client: RpcClient,
}

impl DnetView {
    async fn new(url: Url, name: String) -> Result<Self> {
        let rpc_client = RpcClient::new(url).await?;
        Ok(Self { name, rpc_client })
    }

    // --> {"jsonrpc": "2.0", "method": "ping", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": "pong", "id": 42}
    async fn _ping(&self) -> Result<Value> {
        let req = jsonrpc::request(json!("ping"), json!([]));
        Ok(self.rpc_client.request(req).await?)
    }

    //--> {"jsonrpc": "2.0", "method": "poll", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": {"nodeID": [], "nodeinfo" [], "id": 42}
    async fn get_info(&self) -> Result<Value> {
        let req = jsonrpc::request(json!("get_info"), json!([]));
        Ok(self.rpc_client.request(req).await?)
    }
}

#[async_std::main]
async fn main() -> DnetViewResult<()> {
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
    let nodes = Mutex::new(FxHashMap::default());
    let selectables = Mutex::new(FxHashMap::default());
    let msg_log = Mutex::new(FxHashMap::default());
    let model = Arc::new(Model::new(ids, nodes, selectables, msg_log));

    let nthreads = num_cpus::get();
    let (signal, shutdown) = async_channel::unbounded::<()>();

    let ex = Arc::new(Executor::new());
    let ex2 = ex.clone();

    let (_, result) = Parallel::new()
        .each(0..nthreads, |_| smol::future::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            smol::future::block_on(async move {
                poll_and_update_model(&config, ex2.clone(), model.clone()).await?;
                render_view(&mut terminal, model.clone()).await?;
                drop(signal);
                Ok(())
            })
        });

    result
}

// create a new RPC instance for every node in the config file
// spawn poll() and detach in the background
async fn poll_and_update_model(
    config: &DnvConfig,
    ex: Arc<Executor<'_>>,
    model: Arc<Model>,
) -> DnetViewResult<()> {
    for node in &config.nodes {
        let client = DnetView::new(Url::parse(&node.rpc_url)?, node.name.clone()).await?;
        ex.spawn(poll(client, model.clone())).detach();
    }
    Ok(())
}

async fn poll(client: DnetView, model: Arc<Model>) -> DnetViewResult<()> {
    loop {
        let reply = client.get_info().await?;

        if reply.as_object().is_some() && !reply.as_object().unwrap().is_empty() {
            parse_data(reply.as_object().unwrap(), &client, model.clone()).await?;
        } else {
            return Err(DnetViewError::EmptyRpcReply)
        }
        async_util::sleep(2).await;
    }
}

async fn parse_data(
    reply: &serde_json::Map<String, Value>,
    client: &DnetView,
    model: Arc<Model>,
) -> DnetViewResult<()> {
    let addr = &reply.get("external_addr");
    let inbound = &reply["session_inbound"];
    let manual = &reply["session_manual"];
    let outbound = &reply["session_outbound"];

    let mut sessions: Vec<SessionInfo> = Vec::new();

    let node_name = &client.name;
    let node_id = make_node_id(node_name)?;
    //let external_addr = ext_addr.unwrap().as_str().unwrap();

    let ext_addr = parse_external_addr(addr).await?;
    let in_session = parse_inbound(inbound, &node_id).await?;
    let out_session = parse_outbound(outbound, &node_id).await?;
    let man_session = parse_manual(manual, &node_id).await?;

    sessions.push(in_session.clone());
    sessions.push(out_session.clone());
    sessions.push(man_session.clone());

    let node = NodeInfo::new(node_id.clone(), node_name.to_string(), sessions.clone(), ext_addr);

    update_node(model.clone(), node.clone(), node_id.clone()).await;
    update_selectable_and_ids(model.clone(), sessions.clone(), node.clone()).await?;
    update_msgs(model.clone(), sessions.clone()).await?;

    //debug!("IDS: {:?}", model.ids.lock().await);
    //debug!("INFOS: {:?}", model.infos.lock().await);

    Ok(())
}

async fn update_msgs(model: Arc<Model>, sessions: Vec<SessionInfo>) -> DnetViewResult<()> {
    for session in sessions {
        for connection in session.children {
            if !model.msg_log.lock().await.contains_key(&connection.id) {
                model.msg_log.lock().await.insert(connection.id, connection.msg_log);
            } else {
                match model.msg_log.lock().await.entry(connection.id) {
                    Entry::Vacant(e) => {
                        e.insert(connection.msg_log);
                    }
                    Entry::Occupied(mut e) => {
                        for msg in connection.msg_log {
                            e.get_mut().push(msg);
                        }
                    }
                }
            }
        }
    }
    //debug!("MSGS: {:?}", model.msg_log.lock().await);
    Ok(())
}

async fn update_ids(model: Arc<Model>, id: String) {
    model.ids.lock().await.insert(id);
}

async fn update_node(model: Arc<Model>, node: NodeInfo, id: String) {
    model.nodes.lock().await.insert(id, node);
}

async fn update_selectable_and_ids(
    model: Arc<Model>,
    sessions: Vec<SessionInfo>,
    node: NodeInfo,
) -> DnetViewResult<()> {
    let node_obj = SelectableObject::Node(node.clone());
    model.selectables.lock().await.insert(node.id.clone(), node_obj);
    update_ids(model.clone(), node.id.clone()).await;
    for session in sessions.clone() {
        let session_obj = SelectableObject::Session(session.clone());
        model.selectables.lock().await.insert(session.clone().id, session_obj);
        update_ids(model.clone(), session.clone().id).await;
        for connect in session.children {
            let connect_obj = SelectableObject::Connect(connect.clone());
            model.selectables.lock().await.insert(connect.clone().id, connect_obj);
            update_ids(model.clone(), connect.clone().id).await;
        }
    }
    Ok(())
}

async fn parse_external_addr(addr: &Option<&Value>) -> DnetViewResult<String> {
    match addr {
        Some(addr) => match addr.as_str() {
            Some(addr) => return Ok(addr.to_string()),
            None => return Ok("null".to_string()),
        },
        None => Err(DnetViewError::NoExternalAddr),
    }
}

async fn parse_inbound(inbound: &Value, node_id: &String) -> DnetViewResult<SessionInfo> {
    let name = "Inbound".to_string();
    let session_type = Session::Inbound;
    let parent = node_id.to_string();
    let id = make_session_id(&parent, &session_type)?;
    let mut connects: Vec<ConnectInfo> = Vec::new();
    let connections = &inbound["connected"];
    let mut connect_count = 0;

    match connections.as_object() {
        Some(connect) => {
            match connect.is_empty() {
                true => {
                    connect_count += 1;
                    // channel is empty. initialize with empty values
                    let id = make_empty_id(&node_id, &session_type, connect_count)?;
                    let addr = "Null".to_string();
                    let state = "Null".to_string();
                    let parent = parent.clone();
                    let msg_log = Vec::new();
                    let is_empty = true;
                    let last_msg = "Null".to_string();
                    let last_status = "Null".to_string();
                    let connect_info = ConnectInfo::new(
                        id,
                        addr,
                        state,
                        parent,
                        msg_log,
                        is_empty,
                        last_msg,
                        last_status,
                    );
                    connects.push(connect_info.clone());
                }
                false => {
                    // channel is not empty. initialize with whole values
                    for k in connect.keys() {
                        let node = connect.get(k);
                        let addr = k.to_string();
                        let id = node.unwrap().get("random_id").unwrap().as_u64().unwrap();
                        let id = make_connect_id(&id)?;
                        let state = "state".to_string();
                        let parent = parent.clone();
                        let msg_values = node.unwrap().get("log").unwrap().as_array().unwrap();
                        let mut msg_log: Vec<(u64, String, String)> = Vec::new();
                        for msg in msg_values {
                            let msg: (u64, String, String) = serde_json::from_value(msg.clone())?;
                            msg_log.push(msg);
                        }
                        let is_empty = false;
                        let last_msg =
                            node.unwrap().get("last_msg").unwrap().as_str().unwrap().to_string();
                        let last_status =
                            node.unwrap().get("last_status").unwrap().as_str().unwrap().to_string();
                        let connect_info = ConnectInfo::new(
                            id,
                            addr,
                            state,
                            parent,
                            msg_log,
                            is_empty,
                            last_msg,
                            last_status,
                        );
                        connects.push(connect_info.clone());
                    }
                }
            }
            let is_empty = is_empty_session(&connects);

            let session_info = SessionInfo::new(id, name, is_empty, parent, connects);
            Ok(session_info)
        }
        None => Err(DnetViewError::ValueIsNotObject),
    }
}

// TODO: placeholder for now
async fn parse_manual(_manual: &Value, node_id: &String) -> DnetViewResult<SessionInfo> {
    let name = "Manual".to_string();
    let session_type = Session::Manual;
    let mut connects: Vec<ConnectInfo> = Vec::new();
    let parent = node_id.to_string();

    let session_id = make_session_id(&parent, &session_type)?;
    let id: u64 = 0;
    let connect_id = make_connect_id(&id)?;
    let addr = "Null".to_string();
    let state = "Null".to_string();
    let msg_log = Vec::new();
    let is_empty = true;
    let msg = "Null".to_string();
    let status = "Null".to_string();
    let connect_info =
        ConnectInfo::new(connect_id.clone(), addr, state, parent, msg_log, is_empty, msg, status);
    connects.push(connect_info.clone());
    let parent = connect_id.clone();
    let is_empty = is_empty_session(&connects);
    let session_info = SessionInfo::new(session_id, name, is_empty, parent, connects.clone());

    Ok(session_info)
}

async fn parse_outbound(outbound: &Value, node_id: &String) -> DnetViewResult<SessionInfo> {
    let name = "Outbound".to_string();
    let session_type = Session::Outbound;
    let parent = node_id.to_string();
    let id = make_session_id(&parent, &session_type)?;
    let mut connects: Vec<ConnectInfo> = Vec::new();
    let slots = &outbound["slots"];
    let mut slot_count = 0;

    match slots.as_array() {
        Some(slots) => {
            for slot in slots {
                slot_count += 1;
                match slot["channel"].is_null() {
                    true => {
                        // channel is empty. initialize with empty values
                        let id = make_empty_id(&node_id, &session_type, slot_count)?;
                        let addr = "Null".to_string();
                        let state = &slot["state"];
                        let state = state.as_str().unwrap().to_string();
                        let parent = parent.clone();
                        let msg_log = Vec::new();
                        let is_empty = true;
                        let last_msg = "Null".to_string();
                        let last_status = "Null".to_string();
                        let connect_info = ConnectInfo::new(
                            id,
                            addr,
                            state,
                            parent,
                            msg_log,
                            is_empty,
                            last_msg,
                            last_status,
                        );
                        connects.push(connect_info.clone());
                    }
                    false => {
                        // channel is not empty. initialize with whole values
                        let channel = &slot["channel"];
                        let id = channel["random_id"].as_u64().unwrap();
                        let id = make_connect_id(&id)?;
                        let addr = &slot["addr"];
                        let addr = addr.as_str().unwrap().to_string();
                        let state = &slot["state"];
                        let state = state.as_str().unwrap().to_string();
                        let parent = parent.clone();
                        let msg_values = channel["log"].as_array().unwrap();
                        let mut msg_log: Vec<(u64, String, String)> = Vec::new();
                        for msg in msg_values {
                            let msg: (u64, String, String) = serde_json::from_value(msg.clone())?;
                            msg_log.push(msg);
                        }
                        let is_empty = false;
                        let last_msg = channel["last_msg"].as_str().unwrap().to_string();
                        let last_status = channel["last_status"].as_str().unwrap().to_string();
                        let connect_info = ConnectInfo::new(
                            id,
                            addr,
                            state,
                            parent,
                            msg_log,
                            is_empty,
                            last_msg,
                            last_status,
                        );
                        connects.push(connect_info.clone());
                    }
                }
            }

            let is_empty = is_empty_session(&connects);

            let session_info = SessionInfo::new(id, name, is_empty, parent, connects);
            Ok(session_info)
        }
        None => Err(DnetViewError::ValueIsNotObject),
    }
}

async fn render_view<B: Backend>(
    terminal: &mut Terminal<B>,
    model: Arc<Model>,
) -> DnetViewResult<()> {
    let mut asi = async_stdin();

    terminal.clear()?;

    let nodes = NodeInfoView::new(FxHashMap::default());
    let msg_log = FxHashMap::default();
    let active_ids = IdListView::new(FxHashSet::default());
    let selectables = FxHashMap::default();

    let mut view = View::new(nodes, msg_log, active_ids, selectables);
    view.active_ids.state.select(Some(0));

    loop {
        view.update(
            model.nodes.lock().await.clone(),
            model.msg_log.lock().await.clone(),
            model.selectables.lock().await.clone(),
        );

        let mut err: Option<DnetViewError> = None;

        terminal.draw(|f| match view.render(f) {
            Ok(()) => {}
            Err(e) => {
                err = Some(e);
            }
        })?;

        match err {
            Some(e) => return Err(e),
            None => {}
        }

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
