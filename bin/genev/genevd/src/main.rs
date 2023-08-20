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

use async_std::{
    stream::StreamExt,
    sync::{Arc, Mutex},
};
use darkfi::{
    async_daemonize, cli_desc,
    event_graph::{
        events_queue::EventsQueue,
        model::{Event, EventId, Model},
        protocol_event::{ProtocolEvent, Seen, SeenPtr},
        view::{View, ViewPtr},
    },
    net::{self, settings::SettingsOpt},
    rpc::server::listen_and_serve,
    Result,
};
use genevd::GenEvent;
use log::info;
use structopt_toml::{serde::Deserialize, structopt::StructOpt, StructOptToml};
use url::Url;

mod rpc;
use rpc::JsonRpcInterface;

const CONFIG_FILE: &str = "genev_config.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../../genev_config.toml");

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "genev", about = cli_desc!())]
struct Args {
    #[structopt(short, long)]
    /// Configuration file to use
    config: Option<String>,

    /// JSON-RPC listen URL
    #[structopt(long = "rpc", default_value = "tcp://127.0.0.1:28880")]
    pub rpc_listen: Url,

    #[structopt(flatten)]
    pub net: SettingsOpt,

    #[structopt(short, long)]
    /// Set log file to ouput into
    log: Option<String>,

    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,
}

async fn start_sync_loop(
    view: ViewPtr<GenEvent>,
    seen: SeenPtr<EventId>,
    missed_events: Arc<Mutex<Vec<Event<GenEvent>>>>,
) -> Result<()> {
    loop {
        let event = view.lock().await.process().await?;
        if !seen.push(&event.hash()).await {
            continue
        }

        info!("new event: {:?}", event);
        missed_events.lock().await.push(event.clone());
    }
}

async_daemonize!(realmain);
async fn realmain(args: Args, executor: Arc<smol::Executor<'_>>) -> Result<()> {
    ////////////////////
    // Initialize the base structures
    ////////////////////
    let events_queue = EventsQueue::<GenEvent>::new();
    let model = Arc::new(Mutex::new(Model::new(events_queue.clone())));
    let view = Arc::new(Mutex::new(View::new(events_queue)));
    let model_clone = model.clone();

    ////////////////////
    // P2p setup
    ////////////////////
    // Buffers
    let seen_event = Seen::new();
    let seen_inv = Seen::new();

    // Check the version
    let net_settings = args.net.clone();

    // New p2p
    let p2p = net::P2p::new(net_settings.into()).await;
    let p2p2 = p2p.clone();

    // Register the protocol_event
    let registry = p2p.protocol_registry();
    registry
        .register(net::SESSION_ALL, move |channel, p2p| {
            let seen_event = seen_event.clone();
            let seen_inv = seen_inv.clone();
            let model = model.clone();
            async move { ProtocolEvent::init(channel, p2p, model, seen_event, seen_inv).await }
        })
        .await;

    // Start
    p2p.clone().start(executor.clone()).await?;

    // Run
    let executor_cloned = executor.clone();
    executor_cloned.spawn(p2p.clone().run(executor.clone())).detach();

    ////////////////////
    // Listner
    ////////////////////
    let seen_ids = Seen::new();
    let missed_events = Arc::new(Mutex::new(vec![]));

    executor.spawn(start_sync_loop(view, seen_ids.clone(), missed_events.clone())).detach();

    //
    // RPC interface
    //
    let rpc_interface = Arc::new(JsonRpcInterface::new(
        "Alolymous".to_string(),
        missed_events.clone(),
        model_clone,
        seen_ids.clone(),
        p2p.clone(),
    ));
    let _ex = executor.clone();
    executor.spawn(listen_and_serve(args.rpc_listen.clone(), rpc_interface, _ex)).detach();

    // Signal handling for graceful termination.
    let (signals_handler, signals_task) = SignalHandler::new()?;
    signals_handler.wait_termination(signals_task).await?;
    info!("Caught termination signal, cleaning up and exiting...");

    // stop p2p
    p2p2.stop().await;

    Ok(())
}
