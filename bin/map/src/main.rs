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
use log::{debug, info};
use serde_json::{json, Value};
use simplelog::*;
use smol::Executor;
use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io,
    io::Read,
    path::PathBuf,
};
use termion::{async_stdin, event::Key, input::TermRead, raw::IntoRawMode};
use tui::{
    backend::{Backend, TermionBackend},
    Terminal,
};
use url::Url;

use map::{
    model::{Connection, IdList, InfoList, NodeInfo},
    options::ProgramOptions,
    ui,
    view::{IdListView, InfoListView},
    Model, View,
};

const CONFIG_FILE_CONTENTS: &[u8] = include_bytes!("../map_config.toml");

struct Map {
    url: Url,
}

impl Map {
    pub fn new(url: Url) -> Self {
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
    let (lvl, cfg) = log_config(options.app.clone())?;

    let file = File::create(&*options.log_path).unwrap();
    WriteLogger::init(lvl, cfg, file)?;
    info!("Log level: {}", lvl);

    let config_path = join_config_path(&PathBuf::from("map_config.toml"))?;

    spawn_config(&config_path, CONFIG_FILE_CONTENTS)?;

    let config = Config::<MapConfig>::load(config_path)?;

    let stdout = io::stdout().into_raw_mode()?;
    let backend = TermionBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    terminal.clear()?;

    let info_list = InfoList::new();
    let ids = HashSet::new();
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
                render(&mut terminal, model.clone()).await?;
                drop(signal);
                Ok::<(), darkfi::Error>(())
            })
        });

    result
}

async fn run_rpc(config: &MapConfig, ex: Arc<Executor<'_>>, model: Arc<Model>) -> Result<()> {
    let mut rpc_vec = Vec::new();
    for node in config.nodes.clone() {
        rpc_vec.push(node);
    }
    for node in rpc_vec {
        debug!("Created client: {}", node.node_id);
        let client = Map::new(Url::parse(&node.node_id)?);
        ex.spawn(poll(client, model.clone())).detach();
    }

    Ok(())
}

async fn poll(client: Map, model: Arc<Model>) -> Result<()> {
    debug!("Attemping to poll: {}", client.url);
    // TODO: fix this! this can lag forever
    // check connect() on net/connector.rs
    //let reply = client.ping().await?;
    //if reply.as_str().is_some() {
    let mut index = 0;
    loop {
        //debug!("Connected to: {}", client.url);
        let reply = client.get_info().await?;

        if reply.as_object().is_some() && !reply.as_object().unwrap().is_empty() {
            let id = reply.as_object().unwrap().get("id").unwrap();

            let connections = reply.as_object().unwrap().get("connections").unwrap();
            let outgoing = connections.get("outgoing").unwrap();
            let incoming = connections.get("incoming").unwrap();

            let mut outconnects = Vec::new();
            let mut inconnects = Vec::new();

            // here we are simulating new messages by scrolling through a vector
            let msgs = outgoing[1].get("message").unwrap();
            if index == 0 {
                index += 1;
            } else if index >= 5 {
                index = 0
            } else {
                index = index + 1;
            }

            let out0 = Connection::new(
                outgoing[0].get("id").unwrap().as_str().unwrap().to_string(),
                msgs[index].as_str().unwrap().to_string(),
            );
            let out1 = Connection::new(
                outgoing[1].get("id").unwrap().as_str().unwrap().to_string(),
                msgs[index].as_str().unwrap().to_string(),
            );

            let in0 = Connection::new(
                incoming[0].get("id").unwrap().as_str().unwrap().to_string(),
                msgs[index].as_str().unwrap().to_string(),
            );
            let in1 = Connection::new(
                incoming[1].get("id").unwrap().as_str().unwrap().to_string(),
                msgs[index].as_str().unwrap().to_string(),
            );

            outconnects.push(out0);
            outconnects.push(out1);

            inconnects.push(in0);
            inconnects.push(in1);

            let infos = NodeInfo { outgoing: outconnects, incoming: inconnects };

            let mut node_info = HashMap::new();
            node_info.insert(id.as_str().unwrap().to_string(), infos);

            for (id, value) in node_info.clone() {
                model.id_list.node_id.lock().await.insert(id.clone());
                model.info_list.infos.lock().await.insert(id, value);
            }
        } else {
            // TODO: error handling
            debug!("Reply is empty");
        }
        async_util::sleep(2).await;
    }
    //} else {
    //    async_util::sleep(10).await;
    //    Err(Error::ConnectTimeout)
    //}
}

async fn render<B: Backend>(terminal: &mut Terminal<B>, model: Arc<Model>) -> io::Result<()> {
    let mut asi = async_stdin();

    terminal.clear()?;

    let id_list = IdListView::new(HashSet::new());
    let info_list = InfoListView::new(HashMap::new());
    let mut view = View::new(id_list.clone(), info_list.clone());

    view.id_list.state.select(Some(0));
    view.info_list.index = 0;
    //let mut counter = 0;

    loop {
        //counter = counter + 1;
        view.update(model.info_list.infos.lock().await.clone());
        //if view.id_list.node_id.is_empty() {
        //    // TODO: delete this and display empty data
        //    if counter == 1 {
        //        let mut progress = 0;
        //        while progress < 100 {
        //            terminal.draw(|f| {
        //                ui::init_panel(f, progress);
        //            })?;
        //            Timer::after(Duration::from_millis(1)).await;
        //            progress = progress + 1;
        //        }
        //    } else if counter == 2 {
        //        terminal.clear()?;
        //        // TODO: continue to display not, mark as offline
        //        println!("Could not connect to node. Are you sure RPC is running?");
        //    }
        //} else {
        terminal.draw(|f| {
            ui::ui(f, view.clone());
        })?;
        //}
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
