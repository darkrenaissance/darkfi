use ::darkfi::{
    consensus::ouroboros::{EpochConsensus, Stakeholder},
    net::Settings,
    util::time::Timestamp,
};

use clap::Parser;
use futures::executor::block_on;
use std::thread;
use url::Url;

#[derive(Parser)]
struct NetCli {
    #[clap(long, value_parser, default_value = "tls://127.0.0.1:12003")]
    addr: String,
    #[clap(long, value_parser, default_value = "/tmp/db")]
    path: String,
    #[clap(long, value_parser, default_value = "tls://127.0.0.1:12004")]
    peers: Vec<String>,
    #[clap(long, value_parser, default_value = "tls://lilith.dark.fi:25551")]
    seeds: Vec<String>,
}

#[async_std::main]
async fn main() {
    env_logger::init();
    let args = NetCli::parse();
    let addr = vec![Url::parse(args.addr.as_str()).unwrap()];
    let mut peers = vec![];
    for i in 0..args.peers.len() {
        peers.push(Url::parse(args.peers[i].as_str()).unwrap());
    }
    let mut seeds = vec![];
    for i in 0..args.seeds.len() {
        seeds.push(Url::parse(args.seeds[i].as_str()).unwrap());
    }
    let slots = 3;
    let epochs = 3;
    let ticks = 10;
    let reward = 1;
    let epoch_consensus = EpochConsensus::new(Some(slots), Some(epochs), Some(ticks), Some(reward));
    // initialize n stakeholders
    let settings = Settings {
        inbound: addr.clone(),
        outbound_connections: 4,
        manual_attempt_limit: 0,
        seed_query_timeout_seconds: 8,
        connect_timeout_seconds: 10,
        channel_handshake_seconds: 4,
        channel_heartbeat_seconds: 10,
        external_addr: addr,
        peers,
        seeds,
        ..Default::default()
    };
    //proof's number of rows
    let k: u32 = 13;
    let path = args.path;
    let id = Timestamp::current_time().0;

    let mut stakeholder =
        block_on(Stakeholder::new(epoch_consensus, settings, &path, id, Some(k))).unwrap();

    let handle = thread::spawn(move || {
        block_on(stakeholder.background(Some(100)));
    });
    handle.join().unwrap();
}
