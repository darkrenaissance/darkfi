/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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

use async_std::sync::{Arc, Mutex};
use std::path::Path;

use async_executor::Executor;
use fxhash::FxHashMap;
use log::{error, info, warn};
use smol::future;
use structopt::StructOpt;
use url::Url;

use darkfi::{
    net,
    raft::{DataStore, NetMsg, ProtocolRaft, Raft, RaftSettings},
    util::{
        cli::{get_log_config, get_log_level},
        expand_path,
        serial::{SerialDecodable, SerialEncodable},
        sleep,
    },
    Result,
};

#[derive(Clone, Debug, StructOpt)]
#[structopt(name = "raft-diag")]
pub struct Args {
    /// JSON-RPC listen URL
    #[structopt(long = "rpc", default_value = "tcp://127.0.0.1:12055")]
    pub rpc_listen: Url,
    /// Inbound listen URL
    #[structopt(long = "inbound")]
    pub inbound_url: Vec<Url>,
    /// Seed Urls
    #[structopt(long = "seeds")]
    pub seed_urls: Vec<Url>,
    /// Outbound connections
    #[structopt(long = "outbound", default_value = "0")]
    pub outbound_connections: u32,
    /// Sets Datastore Path
    #[structopt(long = "path", default_value = "test1.db")]
    pub datastore: String,
    /// Check if all datastore paths provided are synced
    #[structopt(long = "check")]
    pub check: Vec<String>,
    /// Datastore path to extract and print it
    #[structopt(long = "extract")]
    pub extract: Option<String>,
    /// Number of messages to broadcast
    #[structopt(short, default_value = "0")]
    pub broadcast: u32,
    /// Increase verbosity
    #[structopt(short, parse(from_occurrences))]
    pub verbose: u8,
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable, PartialEq, Eq)]
pub struct Message {
    payload: String,
}

fn extract(path: &str) -> Result<()> {
    if !Path::new(path).exists() {
        return Ok(())
    }

    let db = DataStore::<Message>::new(path)?;
    let commits = db.commits.get_all()?;

    println!("{:?}", commits);

    Ok(())
}

fn check(args: Args) -> Result<()> {
    let mut commits_check = vec![];

    for path in args.check {
        if !Path::new(&path).exists() {
            continue
        }
        let db = DataStore::<Message>::new(&path)?;
        let commits = db.commits.get_all()?;
        commits_check.push(commits);
    }

    let result = commits_check.windows(2).all(|w| w[0] == w[1]);

    println!("Synced: {}", result);

    Ok(())
}

async fn start_broadcasting(n: u32, sender: async_channel::Sender<Message>) -> Result<()> {
    sleep(8).await;
    info!(target: "raft", "Start broadcasting...");
    for id in 0..n {
        let msg = format!("msg_test_{}", id);
        info!(target: "raft", "Send a message {:?}", msg);
        let msg = Message { payload: msg };
        sender.send(msg).await?;
    }

    Ok(())
}

async fn receive_loop(receiver: async_channel::Receiver<Message>) -> Result<()> {
    loop {
        let msg = receiver.recv().await?;
        info!(target: "raft", "Receive new msg {:?}", msg);
    }
}

async fn start(args: Args, executor: Arc<Executor<'_>>) -> Result<()> {
    let net_settings = net::Settings {
        outbound_connections: args.outbound_connections,
        inbound: args.inbound_url.clone(),
        external_addr: args.inbound_url,
        seeds: args.seed_urls,
        ..net::Settings::default()
    };

    //
    // Raft
    //

    let datastore_raft = expand_path(&args.datastore)?;

    let seen_net_msgs = Arc::new(Mutex::new(FxHashMap::default()));

    let raft_settings = RaftSettings { datastore_path: datastore_raft, ..RaftSettings::default() };

    let mut raft = Raft::<Message>::new(raft_settings, seen_net_msgs.clone())?;

    //
    // P2p setup
    //

    let (p2p_send_channel, p2p_recv_channel) = async_channel::unbounded::<NetMsg>();

    let p2p = net::P2p::new(net_settings).await;
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
    ctrlc::set_handler(move || {
        warn!("Catch exit signal");
        // cleaning up tasks running in the background
        if let Err(e) = async_std::task::block_on(signal.send(())) {
            error!("Error on sending exit signal: {}", e);
        }
    })
    .unwrap();

    if args.broadcast != 0 {
        executor.spawn(start_broadcasting(args.broadcast, raft.sender())).detach();
    }

    executor.spawn(receive_loop(raft.receiver())).detach();

    raft.run(p2p.clone(), p2p_recv_channel.clone(), executor.clone(), shutdown.clone()).await?;
    Ok(())
}

fn main() -> Result<()> {
    let args = Args::from_args();
    let log_level = get_log_level(args.verbose.into());
    let log_config = get_log_config();

    let mut log_path = expand_path(&args.datastore)?;
    let log_name: String = log_path.file_name().as_ref().unwrap().to_str().unwrap().to_owned();
    log_path.pop();
    let log_path = log_path.join(&format!("{}.log", log_name));
    let env_log_file_path = std::fs::File::create(log_path).unwrap();

    simplelog::CombinedLogger::init(vec![
        simplelog::TermLogger::new(
            log_level,
            log_config.clone(),
            simplelog::TerminalMode::Mixed,
            simplelog::ColorChoice::Auto,
        ),
        simplelog::WriteLogger::new(log_level, log_config, env_log_file_path),
    ])?;

    if !args.check.is_empty() {
        return check(args)
    }

    if args.extract.is_some() {
        return extract(&args.extract.unwrap())
    }

    // https://docs.rs/smol/latest/smol/struct.Executor.html#examples
    let ex = Arc::new(async_executor::Executor::new());
    let (signal, shutdown) = async_channel::unbounded::<()>();
    let (_, result) = easy_parallel::Parallel::new()
        // Run four executor threads
        .each(0..4, |_| future::block_on(ex.run(shutdown.recv())))
        // Run the main future on the current thread.
        .finish(|| {
            future::block_on(async {
                start(args, ex.clone()).await?;
                drop(signal);
                Ok::<(), darkfi::Error>(())
            })
        });

    result
}
