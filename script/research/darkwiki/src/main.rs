use async_std::sync::{Arc, Mutex};
use std::{
    fs::{create_dir_all, read_dir},
    path::{Path, PathBuf},
};

use async_executor::Executor;
use futures::{select, FutureExt};
use fxhash::FxHashMap;
use log::{error, warn};
use serde::Deserialize;
use smol::future;
use structopt::StructOpt;
use structopt_toml::StructOptToml;
use url::Url;

use darkfi::{
    async_daemonize,
    net::{self, settings::SettingsOpt},
    raft::{NetMsg, ProtocolRaft, Raft, RaftSettings},
    rpc::server::listen_and_serve,
    util::{
        cli::{get_log_config, get_log_level, spawn_config},
        expand_path,
        file::{load_file, load_json_file, save_file, save_json_file},
        gen_id,
        path::get_config_path,
    },
    Error, Result,
};

mod error;
mod jsonrpc;
mod sequence;

use error::DarkWikiResult;
use jsonrpc::JsonRpcInterface;
use sequence::{Operation, Sequence};

pub const CONFIG_FILE: &str = "darkwiki.toml";
pub const CONFIG_FILE_CONTENTS: &str = include_str!("../darkwiki.toml");
pub const DOCS_PATH: &str = "~/darkwiki";

/// darkwiki cli
#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "darkwiki")]
pub struct Args {
    /// Sets a custom config file
    #[structopt(long)]
    pub config: Option<String>,
    /// Sets Datastore Path
    #[structopt(long, default_value = "~/.config/darkfi/darkwiki")]
    pub datastore: String,
    /// JSON-RPC listen URL
    #[structopt(long = "rpc", default_value = "tcp://127.0.0.1:13055")]
    pub rpc_listen: Url,
    #[structopt(flatten)]
    pub net: SettingsOpt,
    /// Increase verbosity
    #[structopt(short, parse(from_occurrences))]
    pub verbose: u8,
}

fn on_receive_operation(op: Operation, datastore_path: &Path) -> DarkWikiResult<()> {
    let json_files_path = datastore_path.join("files");
    let docs_path = PathBuf::from(expand_path(&DOCS_PATH)?);

    let id_path = json_files_path.join(op.id());

    let json_file = load_json_file::<Sequence>(&id_path);
    let mut seq: Sequence = if let Ok(file) = json_file { file } else { Sequence::new(&op.id()) };

    seq.add_op(&op)?;
    save_json_file::<Sequence>(&id_path, &seq)?;

    //let st = seq.apply();
    //save_file(&docs_path.join(op.id()), &st)?;

    Ok(())
}

fn on_receive_update(datastore_path: &Path) -> DarkWikiResult<Vec<Operation>> {
    let ret = vec![];

    let json_files_path = datastore_path.join("files");
    let docs_path = PathBuf::from(expand_path(&DOCS_PATH)?);

    let files = read_dir(&docs_path).unwrap();

    for file in files {
        let file_path = file.unwrap().path();
        let path = docs_path.join(&file_path);
        let edit = load_file(&path)?;

        if let Ok(seq) = load_json_file::<Sequence>(&json_files_path.join(&file_path)) {
            //
            // TODO the transformation should happen here
            //
        } else {
            let mut seq = Sequence::new(&gen_id(30));
            //seq.insert(0, &edit)?;
            save_json_file(&json_files_path.join(file_path), &seq)?;
        }
    }

    Ok(ret)
}

async fn start(
    update_notifier_rv: async_channel::Receiver<()>,
    raft_sender: async_channel::Sender<Operation>,
    raft_receiver: async_channel::Receiver<Operation>,
    datastore_path: PathBuf,
) -> DarkWikiResult<()> {
    loop {
        select! {
            _ = update_notifier_rv.recv().fuse() => {
                let ops = on_receive_update(&datastore_path)?;
                for op in ops {
                    raft_sender.send(op).await.map_err(Error::from)?;
                }
            }
            op = raft_receiver.recv().fuse() => {
                let op = op.map_err(Error::from)?;
                on_receive_operation(op, &datastore_path)?;
            }

        }
    }
}

async_daemonize!(realmain);
async fn realmain(settings: Args, executor: Arc<Executor<'_>>) -> Result<()> {
    let datastore_path = expand_path(&settings.datastore)?;

    create_dir_all(expand_path(&DOCS_PATH)?)?;
    create_dir_all(datastore_path.join("files"))?;

    let (update_notifier_sx, update_notifier_rv) = async_channel::unbounded::<()>();

    //
    // RPC
    //
    let rpc_interface = Arc::new(JsonRpcInterface::new(update_notifier_sx));
    executor.spawn(listen_and_serve(settings.rpc_listen.clone(), rpc_interface)).detach();

    //
    // Raft
    //
    let net_settings = settings.net;
    let seen_net_msgs = Arc::new(Mutex::new(FxHashMap::default()));

    let datastore_raft = datastore_path.join("darkwiki.db");
    let raft_settings = RaftSettings { datastore_path: datastore_raft, ..RaftSettings::default() };

    let mut raft = Raft::<Operation>::new(raft_settings, seen_net_msgs.clone())?;

    executor
        .spawn(start(update_notifier_rv, raft.sender(), raft.receiver(), datastore_path.clone()))
        .detach();

    //
    // P2p setup
    //
    let (p2p_send_channel, p2p_recv_channel) = async_channel::unbounded::<NetMsg>();

    let p2p = net::P2p::new(net_settings.into()).await;
    let p2p = p2p.clone();

    let registry = p2p.protocol_registry();

    let raft_node_id = raft.id();
    registry
        .register(net::SESSION_ALL, move |channel, p2p| {
            let raft_node_id = raft_node_id.clone();
            let sender = p2p_send_channel.clone();
            let seen_net_msgs_cloned = seen_net_msgs.clone();
            async move {
                ProtocolRaft::init(raft_node_id, channel, sender, p2p, seen_net_msgs_cloned).await
            }
        })
        .await;

    p2p.clone().start(executor.clone()).await?;

    executor.spawn(p2p.clone().run(executor.clone())).detach();

    //
    // Waiting Exit signal
    //
    let (signal, shutdown) = async_channel::bounded::<()>(1);
    ctrlc_async::set_async_handler(async move {
        warn!(target: "darkwiki", "Catch exit signal");
        // cleaning up tasks running in the background
        if let Err(e) = signal.send(()).await {
            error!("Error on sending exit signal: {}", e);
        }
    })
    .unwrap();

    raft.run(p2p.clone(), p2p_recv_channel.clone(), executor.clone(), shutdown.clone()).await?;

    Ok(())
}
