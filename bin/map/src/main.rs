use darkfi::{
    cli::{
        cli_config::{log_config, spawn_config},
        Config, MapConfig,
    },
    error::{Error, Result},
    rpc::{jsonrpc, jsonrpc::JsonResult},
    util::{async_util, join_config_path},
};

use async_std::sync::Arc;
use easy_parallel::Parallel;
use log::debug;
use serde_json::{json, Value};
use smol::Executor;
use std::{io, io::Read, path::PathBuf};
use termion::{async_stdin, event::Key, input::TermRead, raw::IntoRawMode};
use tui::{
    backend::{Backend, TermionBackend},
    Terminal,
};

use map::{
    model::{IdList, InfoList, NodeInfo},
    ui,
    view::{IdListView, InfoListView},
    Model, View,
};

const CONFIG_FILE_CONTENTS: &[u8] = include_bytes!("../map_config.toml");

struct Map {
    url: String,
}

impl Map {
    pub fn new(url: String) -> Self {
        Self { url }
    }

    async fn request(&self, r: jsonrpc::JsonRequest) -> Result<Value> {
        let reply: JsonResult = match jsonrpc::send_request(&self.url, json!(r)).await {
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

    // --> {"jsonrpc": "2.0", "method": "say_hello", "params": [], "id": 42}
    // <-- {"jsonrpc": "2.0", "result": "hello world", "id": 42}
    async fn _say_hello(&self) -> Result<Value> {
        let req = jsonrpc::request(json!("say_hello"), json!([]));
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
    let config_path = join_config_path(&PathBuf::from("map_config.toml"))?;

    spawn_config(&config_path, CONFIG_FILE_CONTENTS)?;

    let config = Config::<MapConfig>::load(config_path)?;

    let stdout = io::stdout().into_raw_mode()?;
    let backend = TermionBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    terminal.clear()?;

    let infos = vec![NodeInfo::new()];
    let info_list = InfoList::new(infos.clone());
    let ids = vec![String::new()];
    let id_list = IdList::new(ids);

    let model = Arc::new(Model::new(id_list, info_list));

    let nthreads = num_cpus::get();
    let (signal, shutdown) = async_channel::unbounded::<()>();

    let ex = Arc::new(Executor::new());
    let ex2 = ex.clone();

    let (_, result) = Parallel::new()
        .each(0..nthreads, |_| smol::future::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            smol::future::block_on(async move {
                run_rpc(&config, ex2.clone(), model.clone()).await?;
                render(&config, &mut terminal, model.clone()).await?;
                drop(signal);
                Ok::<(), darkfi::Error>(())
            })
        });

    result
}

async fn run_rpc(config: &MapConfig, ex: Arc<Executor<'_>>, model: Arc<Model>) -> Result<()> {
    // TODO: listen to multiple nodes
    let client = Map::new(config.nodes[0].node_id.to_string());

    ex.spawn(poll(client, model)).detach();

    Ok(())
}

async fn poll(client: Map, model: Arc<Model>) -> Result<()> {
    loop {
        let reply = client.get_info().await?;

        if reply.as_object().is_some() && !reply.as_object().unwrap().is_empty() {
            let nodes = reply.as_object().unwrap().get("nodes").unwrap();

            // TODO: generalize this
            //let node1 = &nodes[0];
            //let node2 = &nodes[1];
            //let node3 = &nodes[2];

            // change NodeInfo
            // TODO: error handling
            //let infos = vec![
            //    NodeInfo {
            //        id: node1["id"].to_string(),
            //        connections: node1["connections"].as_u64().unwrap() as usize,
            //        is_active: node1["is_active"].as_bool().unwrap(),
            //        last_message: node1["message"].to_string(),
            //    },
            //    //NodeInfo {
            //    //    id: node2["id"].to_string(),
            //    //    connections: node2["connections"].as_u64().unwrap() as usize,
            //    //    is_active: node2["is_active"].as_bool().unwrap(),
            //    //    last_message: node2["message"].to_string(),
            //    //},
            //    //NodeInfo {
            //    //    id: node3["id"].to_string(),
            //    //    connections: node3["connections"].as_u64().unwrap() as usize,
            //    //    is_active: node3["is_active"].as_bool().unwrap(),
            //    //    last_message: node3["message"].to_string(),
            //    //},
            //];

            //for node in infos {
            //    // write nodes
            //    model.info_list.infos.lock().await.push(node.clone());
            //    // write node id
            //    model.id_list.node_id.lock().await.push(node.clone().id);
            //}
        } else {
            // TODO: error handling
            println!("Reply is an error");
        }

        async_util::sleep(2).await;
    }
}

async fn render<B: Backend>(
    config: &MapConfig,
    terminal: &mut Terminal<B>,
    model: Arc<Model>,
) -> io::Result<()> {
    let mut asi = async_stdin();

    terminal.clear()?;

    let mut info_vec = Vec::new();
    for info in model.info_list.infos.lock().await.clone() {
        info_vec.push(info)
    }
    let mut id_vec = Vec::new();
    for id in model.id_list.node_id.lock().await.clone() {
        id_vec.push(id)
    }

    let id_list = IdListView::new(id_vec);
    let info_list = InfoListView::new(info_vec);
    let mut view = View::new(id_list.clone(), info_list.clone());

    view.id_list.state.select(Some(0));
    view.info_list.index = 0;

    loop {
        let mut view = view.clone();
        view.update(
            model.id_list.node_id.lock().await.clone(),
            model.info_list.infos.lock().await.clone(),
        );

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
                    view.info_list.next().await;
                }
                Key::Char('k') => {
                    view.id_list.previous();
                    view.info_list.previous().await;
                }
                _ => (),
            }
        }
    }
}
