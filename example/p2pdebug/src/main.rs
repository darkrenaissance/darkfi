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

use std::sync::Arc;

use async_executor::Executor;
use clap::Parser;
use easy_parallel::Parallel;
use rand::{rngs::OsRng, Rng, RngCore};
use simplelog::{ColorChoice, TermLogger, TerminalMode};
use url::Url;

use darkfi::{
    cli_desc, net,
    rpc::server::listen_and_serve,
    util::cli::{get_log_config, get_log_level},
    Result,
};

pub(crate) mod proto;
pub(crate) mod rpc;

use crate::proto::debugmsg::{Debugmsg, ProtocolDebugmsg, SeenDebugmsgIds};

#[derive(Parser)]
#[clap(name = "p2pdebugging", about = cli_desc!(), version)]
struct Args {
    /// Verbosity level
    #[clap(short, parse(from_occurrences))]
    verbose: u8,
    /// node number:
    /// 0-2 is for seed nodes
    /// 3-20 is for inbound connections nodes
    /// 21- is for outbound connections nodes
    #[clap(short, long, default_value = "0")]
    node: u8,
    /// broadcast messages
    #[clap(short, long)]
    broadcast: bool,
    /// communicate using tls protocol by default is tcp
    #[clap(long)]
    tls: bool,
    /// communicate using tor protocol
    #[clap(long)]
    tor: bool,
    /// peers url to connect to manually
    #[clap(long)]
    peers: Vec<Url>,
    /// communicate using tls protocol by default is tcp
    #[clap(long, default_value = "tcp://127.0.0.1:11055")]
    rpc: String,
    /// open manual connection
    #[clap(long)]
    connect: Option<String>,
}

#[derive(Debug, Clone)]
enum State {
    Seed,
    Inbound,
    Outbound,
}

struct MockP2p {
    node_number: u8,
    state: State,
    broadcast: bool,
    address: Option<Url>,
}

impl MockP2p {
    async fn new(
        node_number: u8,
        _broadcast: bool,
        scheme: &str,
        connect: Option<String>,
        peers: Vec<Url>,
    ) -> Result<(net::P2pPtr, Self)> {
        let seed_addrs: Vec<Url> = vec![
            Url::parse(&format!("{}://127.0.0.1:11001", scheme))?,
            Url::parse(&format!("{}://127.0.0.1:11002", scheme))?,
            Url::parse(&format!("{}://127.0.0.1:11003", scheme))?,
        ];

        let state: State;
        let address: Option<Url>;

        let mut broadcast = _broadcast;

        let p2p = if connect.is_none() {
            match node_number {
                0..=2 => {
                    address = Some(seed_addrs[node_number as usize].clone());

                    let net_settings =
                        net::Settings { inbound: address.clone(), peers, ..Default::default() };
                    let p2p = net::P2p::new(net_settings).await;

                    broadcast = false;
                    state = State::Seed;

                    p2p
                }
                3..=20 => {
                    let random_port: u32 = rand::thread_rng().gen_range(11007..49151);
                    address = Some(format!("{}://127.0.0.1:{}", scheme, random_port).parse()?);

                    let net_settings = net::Settings {
                        inbound: address.clone(),
                        external_addr: address.clone(),
                        seeds: seed_addrs,
                        peers,
                        ..Default::default()
                    };

                    let p2p = net::P2p::new(net_settings).await;

                    state = State::Inbound;

                    p2p
                }
                _ => {
                    address = None;

                    let net_settings = net::Settings {
                        outbound_connections: 3,
                        seeds: seed_addrs,
                        peers,
                        ..Default::default()
                    };

                    let p2p = net::P2p::new(net_settings).await;
                    state = State::Outbound;

                    p2p
                }
            }
        } else {
            address = None;

            let net_settings =
                net::Settings { peers: vec![Url::parse(&connect.unwrap())?], ..Default::default() };

            let p2p = net::P2p::new(net_settings).await;
            state = State::Outbound;

            p2p
        };

        println!("start {:?} node #{} address {:?}", state, node_number, address);

        Ok((p2p, Self { node_number, state, broadcast, address }))
    }

    async fn run(
        &self,
        p2p: net::P2pPtr,
        rpc_addr: Url,
        executor: Arc<Executor<'_>>,
    ) -> Result<()> {
        let state = self.state.clone();
        let node_number = self.node_number;
        let address = self.address.clone();

        let (sender, receiver) = async_channel::unbounded();
        let sender_clone = sender.clone();

        let seen_debugmsg_ids = SeenDebugmsgIds::new();
        let seen_debugmsg_ids_clone = seen_debugmsg_ids.clone();

        let registry = p2p.protocol_registry();
        registry
            .register(net::SESSION_ALL, move |channel, p2p| {
                let sender = sender_clone.clone();
                let seen_debugmsg_ids = seen_debugmsg_ids_clone.clone();
                async move { ProtocolDebugmsg::init(channel, sender, seen_debugmsg_ids, p2p).await }
            })
            .await;

        if self.broadcast {
            println!("start broadcast {:?} node #{} address {:?}", state, node_number, address);
            let sleep_time = 10;
            let p2p_clone = p2p.clone();
            let executor_clone = executor.clone();
            let address_cloned = address.clone();
            let state = state.clone();
            executor_clone
                .spawn(async move {
                    loop {
                        darkfi::util::sleep(sleep_time).await;

                        println!(
                            "broadcast sleep for {} {:?} node #{} address {:?}",
                            sleep_time,
                            state,
                            node_number,
                            address_cloned.clone()
                        );

                        let random_id = OsRng.next_u32();

                        let msg = Debugmsg { id: random_id, message: "hello".to_string() };

                        println!(
                            "send {:?} {:?} node #{} address {:?}",
                            msg, state, node_number, address_cloned
                        );

                        p2p_clone.broadcast(msg).await.unwrap();
                    }
                })
                .detach();
        }

        let seen_debugmsg_ids_clone = seen_debugmsg_ids.clone();
        let address_cloned = address.clone();
        executor
            .spawn(async move {
                loop {
                    let msg = receiver.recv().await.unwrap();
                    println!(
                        "receive {:?} {:?} node #{} address {:?}",
                        msg, state, node_number, address_cloned
                    );
                    seen_debugmsg_ids_clone.add_seen(msg.id).await;
                }
            })
            .detach();

        // RPC

        let rpc_interface =
            Arc::new(rpc::JsonRpcInterface { addr: rpc_addr.clone(), p2p: p2p.clone() });
        let _ex = executor.clone();
        executor.spawn(async move { listen_and_serve(rpc_addr, rpc_interface, _ex).await }).detach();

        p2p.clone().start(executor.clone()).await?;
        p2p.run(executor).await
    }
}

async fn start(executor: Arc<Executor<'_>>, args: Args) -> Result<()> {
    let mut scheme = (if args.tor { "tor" } else { "tcp" }).to_string();

    if args.tls {
        scheme = format!("{}+tls", scheme);
    }

    let rpc_addr = Url::parse(&args.rpc)?;
    let (p2p, mock_p2p) =
        MockP2p::new(args.node, args.broadcast, &scheme, args.connect, args.peers).await?;
    mock_p2p.run(p2p.clone(), rpc_addr, executor).await
}

fn main() -> Result<()> {
    let args = Args::parse();

    let log_level = get_log_level(args.verbose.into());
    let log_config = get_log_config();
    TermLogger::init(log_level, log_config, TerminalMode::Mixed, ColorChoice::Auto)?;

    let ex = Arc::new(Executor::new());
    let ex_clone = ex.clone();
    let (signal, shutdown) = async_channel::unbounded::<()>();
    let (_, result) = Parallel::new()
        .each(0..4, |_| smol::future::block_on(ex.run(shutdown.recv())))
        // Run the main future on the current thread.
        .finish(|| {
            smol::future::block_on(async move {
                start(ex_clone.clone(), args).await?;
                drop(signal);
                Ok::<(), darkfi::Error>(())
            })
        });

    result
}
