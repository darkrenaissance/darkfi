use async_std::sync::{Arc, Mutex};
use std::{fs::File, io, io::Read, path::PathBuf};

use easy_parallel::Parallel;
use fxhash::{FxHashMap, FxHashSet};
use log::{debug, info};
use rand::{thread_rng, Rng};
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
    model::{ConnectInfo, Model, NodeInfo, SelectableObject, SessionInfo},
    options::ProgramOptions,
    ui,
    view::{IdListView, InfoListView, View},
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
    let infos = Mutex::new(FxHashMap::default());

    let model = Arc::new(Model::new(ids, infos));

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
        async_util::sleep(10).await;
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

    let node_id = generate_id();
    let node_name = &client.name;

    let in_session = parse_inbound(inbound, node_id).await?;
    let out_session = parse_outbound(outbound, node_id).await?;
    let man_session = parse_manual(manual, node_id).await?;

    sessions.push(in_session);
    sessions.push(out_session);
    sessions.push(man_session);

    let node_info = NodeInfo::new(node_id, node_name.to_string(), sessions);
    let node = SelectableObject::Node(node_info.clone());

    // TODO: model keeps expanding-- should stop
    model.ids.lock().await.insert(node_id);
    model.infos.lock().await.insert(node_id, node);

    //debug!("IDS: {:?}", model.ids.lock().await);
    //debug!("INFOS: {:?}", model.infos.lock().await);

    Ok(())
}

async fn parse_inbound(inbound: &Value, node_id: u32) -> Result<SessionInfo> {
    let mut connects: Vec<ConnectInfo> = Vec::new();
    let connections = &inbound["connected"];
    let session_id = generate_id();

    match connections.as_object() {
        Some(connect) => {
            match connect.is_empty() {
                true => {
                    // channel is empty. initialize with empty values
                    let connect_id = generate_id();
                    let addr = "Null".to_string();
                    let msg = "Null".to_string();
                    let status = "Null".to_string();
                    let is_empty = true;
                    let parent = session_id;
                    let state = "Null".to_string();
                    let msg_log = Vec::new();
                    let connect_info = ConnectInfo::new(
                        connect_id, addr, is_empty, msg, status, state, msg_log, parent,
                    );
                    connects.push(connect_info.clone());
                }
                false => {
                    // channel is not empty. initialize with whole values
                    // TODO: we are not saving the connect id
                    let connect_id = generate_id();
                    for k in connect.keys() {
                        let node = connect.get(k);
                        let addr = k.to_string();
                        let msg =
                            node.unwrap().get("last_msg").unwrap().as_str().unwrap().to_string();
                        let status =
                            node.unwrap().get("last_status").unwrap().as_str().unwrap().to_string();
                        // TODO: state, msg log
                        let state = "state".to_string();
                        let is_empty = false;
                        let parent = session_id;
                        let msg_log = Vec::new();
                        let connect_info = ConnectInfo::new(
                            connect_id, addr, is_empty, msg, status, state, msg_log, parent,
                        );
                        connects.push(connect_info.clone());
                    }
                }
            }
            let session_info = SessionInfo::new(session_id, node_id, connects.clone());
            Ok(session_info)
        }
        None => Err(Error::ValueIsNotObject),
    }
}

// TODO: placeholder for now
async fn parse_manual(_manual: &Value, node_id: u32) -> Result<SessionInfo> {
    let mut connects: Vec<ConnectInfo> = Vec::new();

    let session_id = generate_id();
    let connect_id = generate_id();
    let addr = "Null".to_string();
    let msg = "Null".to_string();
    let status = "Null".to_string();
    let is_empty = true;
    let parent = session_id;
    let state = "Null".to_string();
    let msg_log = Vec::new();
    let connect_info =
        ConnectInfo::new(connect_id, addr, is_empty, msg, status, state, msg_log, parent);
    connects.push(connect_info.clone());
    let session_info = SessionInfo::new(session_id, node_id, connects.clone());

    Ok(session_info)
}

async fn parse_outbound(outbound: &Value, node_id: u32) -> Result<SessionInfo> {
    let mut connects: Vec<ConnectInfo> = Vec::new();
    let slots = &outbound["slots"];
    let session_id = generate_id();

    match slots.as_array() {
        Some(slots) => {
            for slot in slots {
                match slot["channel"].is_null() {
                    true => {
                        // channel is empty. initialize with empty values
                        let connect_id = generate_id();
                        let is_empty = true;
                        let addr = "Null".to_string();
                        let state = &slot["state"];
                        let msg = "Null".to_string();
                        let status = "Null".to_string();
                        // TODO: msg log
                        let msg_log = Vec::new();
                        let parent = session_id;
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
                        let connect_id = generate_id();
                        let is_empty = false;
                        let addr = &slot["addr"];
                        let state = &slot["state"];
                        // TODO: msg and status
                        let msg = "msg";
                        let status = "status";
                        //let status = &slot["last_status"];
                        let parent = session_id;
                        // TODO
                        let msg_log = Vec::new();
                        let connect_info = ConnectInfo::new(
                            connect_id,
                            addr.as_str().unwrap().to_string(),
                            is_empty,
                            msg.to_string(),
                            status.to_string(),
                            //status.as_str().unwrap().to_string(),
                            state.as_str().unwrap().to_string(),
                            msg_log,
                            parent,
                        );
                        connects.push(connect_info.clone());
                    }
                }
            }
            let session_info = SessionInfo::new(session_id, node_id, connects.clone());
            Ok(session_info)
        }
        None => Err(Error::ValueIsNotObject),
    }
}

// create id if not exists
fn generate_id() -> u32 {
    let mut rng = thread_rng();
    let id: u32 = rng.gen();
    id
}

//fn is_empty_outbound(slots: Vec<Slot>) -> bool {
//    return slots.iter().all(|slot| slot.is_empty);
//}

async fn render<B: Backend>(terminal: &mut Terminal<B>, model: Arc<Model>) -> Result<()> {
    let mut asi = async_stdin();

    terminal.clear()?;

    let id_list = IdListView::new(FxHashSet::default());
    let info_list = InfoListView::new(FxHashMap::default());
    let mut view = View::new(id_list.clone(), info_list.clone());

    view.id_list.state.select(Some(0));
    view.info_list.index = 0;

    loop {
        //view.update(model.info_list.infos.lock().await.clone());
        terminal.draw(|f| {
            ui::ui(f, view.clone());
        })?;
        for k in asi.by_ref().keys() {
            match k.unwrap() {
                Key::Char('q') => {
                    terminal.clear()?;
                    return Ok(())
                }
                Key::Char('j') => {
                    view.id_list.next();
                }
                Key::Char('k') => {
                    view.id_list.previous();
                }
                _ => (),
            }
        }
    }
}
