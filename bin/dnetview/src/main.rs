use async_std::sync::{Arc, Mutex};
use std::{collections::hash_map::Entry, fs::File, io, io::Read, path::PathBuf};

use easy_parallel::Parallel;
use fxhash::{FxHashMap, FxHashSet};
use log::{error, info};
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
    rpc::{client::RpcClient, jsonrpc::JsonRequest},
    util::{
        async_util,
        cli::{get_log_config, get_log_level, spawn_config, Config},
        join_config_path, NanoTimestamp,
    },
};

use dnetview::{
    config::{DnvConfig, IrcNode, CONFIG_FILE_CONTENTS},
    error::{DnetViewError, DnetViewResult},
    model::{ConnectInfo, Model, NodeInfo, SelectableObject, Session, SessionInfo},
    options::ProgramOptions,
    util::{is_empty_session, make_connect_id, make_empty_id, make_node_id, make_session_id},
    view::{IdListView, NodeInfoView, View},
};

use log::debug;

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
        let req = JsonRequest::new("ping", json!([]));
        self.rpc_client.request(req).await
    }

    //--> {"jsonrpc": "2.0", "method": "poll", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": {"nodeID": [], "nodeinfo" [], "id": 42}
    async fn get_info(&self) -> DnetViewResult<Value> {
        let req = JsonRequest::new("get_info", json!([]));
        match self.rpc_client.request(req).await {
            Ok(req) => Ok(req),
            Err(e) => Err(DnetViewError::Darkfi(e)),
        }
    }
}

#[async_std::main]
async fn main() -> DnetViewResult<()> {
    let options = ProgramOptions::load()?;

    let verbosity_level = options.app.occurrences_of("verbose");

    let log_level = get_log_level(verbosity_level);
    let log_config = get_log_config();

    let file = File::create(&*options.log_path).unwrap();
    WriteLogger::init(log_level, log_config, file)?;
    info!("Log level: {}", log_level);

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
                start_connect_slots(&config, ex2.clone(), model.clone()).await?;
                render_view(&mut terminal, model.clone()).await?;
                drop(signal);
                Ok(())
            })
        });

    result
}

async fn start_connect_slots(
    config: &DnvConfig,
    ex: Arc<Executor<'_>>,
    model: Arc<Model>,
) -> DnetViewResult<()> {
    for node in &config.nodes {
        ex.spawn(try_connect(ex.clone(), model.clone(), node.name.clone(), node.rpc_url.clone()))
            .detach();
    }
    Ok(())
}

async fn try_connect(
    ex: Arc<Executor<'_>>,
    model: Arc<Model>,
    node_name: String,
    rpc_url: String,
) -> DnetViewResult<()> {
    loop {
        info!("Attempting to poll {}, RPC URL: {}", node_name, rpc_url);
        match DnetView::new(Url::parse(&rpc_url)?, node_name.clone()).await {
            Ok(client) => {
                poll(client, model.clone()).await?;
            }
            Err(e) => {
                error!("{}", e);
                async_util::sleep(2).await;
            }
        }
    }
}

async fn poll(client: DnetView, model: Arc<Model>) -> DnetViewResult<()> {
    loop {
        match client.get_info().await {
            Ok(reply) => {
                if reply.as_object().is_some() && !reply.as_object().unwrap().is_empty() {
                    parse_data(reply.as_object().unwrap(), &client, model.clone()).await?;
                } else {
                    return Err(DnetViewError::EmptyRpcReply)
                }
            }
            Err(e) => {
                error!("{:?}", e);
                parse_offline(client.name.clone(), model.clone()).await?;
            }
        }
        async_util::sleep(2).await;
    }
}

async fn parse_offline(node_name: String, model: Arc<Model>) -> DnetViewResult<()> {
    let name = "Offline".to_string();
    let session_type = Session::Offline;
    let node_id = make_node_id(&node_name)?;
    let session_id = make_session_id(&node_id, &session_type)?;
    let mut connects: Vec<ConnectInfo> = Vec::new();
    let mut sessions: Vec<SessionInfo> = Vec::new();

    // initialize with empty values
    let id = make_empty_id(&node_id, &session_type, 0)?;
    let addr = "Null".to_string();
    let state = "Null".to_string();
    let parent = node_id.clone();
    let msg_log = Vec::new();
    let is_empty = true;
    let last_msg = "Null".to_string();
    let last_status = "Null".to_string();
    let remote_node_id = "Null".to_string();
    let connect_info = ConnectInfo::new(
        id,
        addr,
        state.clone(),
        parent.clone(),
        msg_log,
        is_empty,
        last_msg,
        last_status,
        remote_node_id,
    );
    connects.push(connect_info.clone());

    let accept_addr = None;
    let session_info =
        SessionInfo::new(session_id, name, is_empty, parent.clone(), connects, accept_addr);
    sessions.push(session_info);

    let node = NodeInfo::new(
        node_id.clone(),
        node_name.to_string(),
        state.clone(),
        sessions.clone(),
        None,
        true,
    );

    update_node(model.clone(), node.clone(), node_id.clone()).await;
    update_selectable_and_ids(model.clone(), sessions, node.clone()).await?;
    Ok(())
}

async fn parse_data(
    reply: &serde_json::Map<String, Value>,
    client: &DnetView,
    model: Arc<Model>,
) -> DnetViewResult<()> {
    let addr = &reply.get("external_addr");
    let inbound = &reply["session_inbound"];
    let _manual = &reply["session_manual"];
    let outbound = &reply["session_outbound"];
    let state = &reply["state"];

    let mut sessions: Vec<SessionInfo> = Vec::new();

    let node_name = &client.name;
    let node_id = make_node_id(node_name)?;
    //let external_addr = ext_addr.unwrap().as_str().unwrap();

    let ext_addr = parse_external_addr(addr).await?;
    let in_session = parse_inbound(inbound, &node_id).await?;
    let out_session = parse_outbound(outbound, &node_id).await?;
    //let man_session = parse_manual(manual, &node_id).await?;

    sessions.push(in_session.clone());
    sessions.push(out_session.clone());
    //sessions.push(man_session.clone());

    let node = NodeInfo::new(
        node_id.clone(),
        node_name.to_string(),
        state.as_str().unwrap().to_string(),
        sessions.clone(),
        ext_addr,
        false,
    );

    update_node(model.clone(), node.clone(), node_id.clone()).await;
    update_selectable_and_ids(model.clone(), sessions.clone(), node.clone()).await?;
    update_msgs(model.clone(), sessions.clone()).await?;

    //debug!("IDS: {:?}", model.ids.lock().await);
    //debug!("INFOS: {:?}", model.nodes.lock().await);

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
    for session in sessions {
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

async fn parse_external_addr(addr: &Option<&Value>) -> DnetViewResult<Option<String>> {
    match addr {
        Some(addr) => match addr.as_str() {
            Some(addr) => Ok(Some(addr.to_string())),
            None => Ok(None),
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
    let mut accept_vec = Vec::new();

    match connections.as_object() {
        Some(connect) => {
            match connect.is_empty() {
                true => {
                    connect_count += 1;
                    // channel is empty. initialize with empty values
                    let id = make_empty_id(node_id, &session_type, connect_count)?;
                    let addr = "Null".to_string();
                    let state = "Null".to_string();
                    let parent = parent.clone();
                    let msg_log = Vec::new();
                    let is_empty = true;
                    let last_msg = "Null".to_string();
                    let last_status = "Null".to_string();
                    let remote_node_id = "Null".to_string();
                    let connect_info = ConnectInfo::new(
                        id,
                        addr,
                        state,
                        parent,
                        msg_log,
                        is_empty,
                        last_msg,
                        last_status,
                        remote_node_id,
                    );
                    connects.push(connect_info);
                }
                false => {
                    // channel is not empty. initialize with whole values
                    for k in connect.keys() {
                        let node = connect.get(k);
                        let addr = k.to_string();
                        let info = node.unwrap().as_array();
                        // get the accept address
                        let accept_addr = info.unwrap().get(0);
                        let acc_addr = accept_addr
                            .unwrap()
                            .get("accept_addr")
                            .unwrap()
                            .as_str()
                            .unwrap()
                            .to_string();
                        accept_vec.push(acc_addr);
                        let info2 = info.unwrap().get(1);
                        let id = info2.unwrap().get("random_id").unwrap().as_u64().unwrap();
                        let id = make_connect_id(&id)?;
                        let state = "state".to_string();
                        let parent = parent.clone();
                        let msg_values = info2.unwrap().get("log").unwrap().as_array().unwrap();
                        let mut msg_log: Vec<(NanoTimestamp, String, String)> = Vec::new();
                        for msg in msg_values {
                            let msg: (NanoTimestamp, String, String) =
                                serde_json::from_value(msg.clone())?;
                            msg_log.push(msg);
                        }
                        let is_empty = false;
                        let last_msg =
                            info2.unwrap().get("last_msg").unwrap().as_str().unwrap().to_string();
                        let last_status = info2
                            .unwrap()
                            .get("last_status")
                            .unwrap()
                            .as_str()
                            .unwrap()
                            .to_string();
                        let remote_node_id = info2
                            .unwrap()
                            .get("remote_node_id")
                            .unwrap()
                            .as_str()
                            .unwrap()
                            .to_string();
                        let r_node_id: String = match remote_node_id.is_empty() {
                            true => "no remote id".to_string(),
                            false => remote_node_id,
                        };
                        let connect_info = ConnectInfo::new(
                            id,
                            addr,
                            state,
                            parent,
                            msg_log,
                            is_empty,
                            last_msg,
                            last_status,
                            r_node_id,
                        );
                        connects.push(connect_info.clone());
                    }
                }
            }
            let is_empty = is_empty_session(&connects);

            // TODO: clean this up
            if accept_vec.is_empty() {
                let accept_addr = None;
                let session_info =
                    SessionInfo::new(id, name, is_empty, parent, connects, accept_addr);
                Ok(session_info)
            } else {
                let accept_addr = Some(accept_vec[0].clone());
                let session_info =
                    SessionInfo::new(id, name, is_empty, parent, connects, accept_addr);
                Ok(session_info)
            }
        }
        None => Err(DnetViewError::ValueIsNotObject),
    }
}

// TODO: placeholder for now
async fn _parse_manual(_manual: &Value, node_id: &String) -> DnetViewResult<SessionInfo> {
    let name = "Manual".to_string();
    let session_type = Session::Manual;
    let mut connects: Vec<ConnectInfo> = Vec::new();
    let parent = node_id.to_string();

    let session_id = make_session_id(&parent, &session_type)?;
    //let id: u64 = 0;
    let connect_id = make_empty_id(node_id, &session_type, 0)?;
    //let connect_id = make_connect_id(&id)?;
    let addr = "Null".to_string();
    let state = "Null".to_string();
    let msg_log = Vec::new();
    let is_empty = true;
    let msg = "Null".to_string();
    let status = "Null".to_string();
    let remote_node_id = "Null".to_string();
    let connect_info = ConnectInfo::new(
        connect_id.clone(),
        addr,
        state,
        parent,
        msg_log,
        is_empty,
        msg,
        status,
        remote_node_id,
    );
    connects.push(connect_info);
    let parent = connect_id;
    let is_empty = is_empty_session(&connects);
    let accept_addr = None;
    let session_info =
        SessionInfo::new(session_id, name, is_empty, parent, connects.clone(), accept_addr);

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
                        let id = make_empty_id(node_id, &session_type, slot_count)?;
                        let addr = "Null".to_string();
                        let state = &slot["state"];
                        let state = state.as_str().unwrap().to_string();
                        let parent = parent.clone();
                        let msg_log = Vec::new();
                        let is_empty = true;
                        let last_msg = "Null".to_string();
                        let last_status = "Null".to_string();
                        let remote_node_id = "Null".to_string();
                        let connect_info = ConnectInfo::new(
                            id,
                            addr,
                            state,
                            parent,
                            msg_log,
                            is_empty,
                            last_msg,
                            last_status,
                            remote_node_id,
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
                        let mut msg_log: Vec<(NanoTimestamp, String, String)> = Vec::new();
                        for msg in msg_values {
                            let msg: (NanoTimestamp, String, String) =
                                serde_json::from_value(msg.clone())?;
                            msg_log.push(msg);
                        }
                        let is_empty = false;
                        let last_msg = channel["last_msg"].as_str().unwrap().to_string();
                        let last_status = channel["last_status"].as_str().unwrap().to_string();
                        let remote_node_id =
                            channel["remote_node_id"].as_str().unwrap().to_string();
                        let r_node_id: String = match remote_node_id.is_empty() {
                            true => "no remote id".to_string(),
                            false => remote_node_id,
                        };
                        let connect_info = ConnectInfo::new(
                            id,
                            addr,
                            state,
                            parent,
                            msg_log,
                            is_empty,
                            last_msg,
                            last_status,
                            r_node_id,
                        );
                        connects.push(connect_info.clone());
                    }
                }
            }

            let is_empty = is_empty_session(&connects);

            let accept_addr = None;
            let session_info = SessionInfo::new(id, name, is_empty, parent, connects, accept_addr);
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
