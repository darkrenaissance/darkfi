use  ::darkfi::{
    stakeholder::Stakeholder,
    blockchain::{EpochConsensus,},
    net::{Settings,},
};

use futures::executor::block_on;
use url::Url;
use std::thread;
use vec;
use clap::Parser;

#[derive(Parser)]
struct NetCli {
    addr: String,
    path: String,
    peers: Vec<String>,
}


#[async_std::main]
async fn main()
{
    let args = NetCli::parse();
    let addr = vec!(Url::parse(args.addr.as_str()).unwrap());
    let mut peers = vec![];
    for i in 0..args.peers.len() {
        peers.push(Url::parse(args.peers[i].as_str()).unwrap());
    }
    let seeds = [Url::parse("tls://irc0.dark.fi:11001").unwrap(),
                 Url::parse("tls://irc1.dark.fi:11001").unwrap()].to_vec();
    let slots=3;
    let epochs=3;
    let ticks=10;
    let reward=1;
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
        external_addr: addr.clone(),
        peers: peers,
        seeds: seeds,
        ..Default::default()
    };
    //proof's number of rows
    let k : u32 = 13;
    let mut handles = vec!();
    let path = args.path;
    for i in 0..2 {
        let rel_path =  format!("{}{}",path, i.to_string());

        let mut stakeholder = block_on(Stakeholder::new(epoch_consensus.clone(),
                                                        settings.clone(),
                                                        &rel_path,
                                                        i,
                                                        Some(k))).unwrap();

        let handle = thread::spawn(move || {
            block_on(stakeholder.background(Some(9)));
        });
        handles.push(handle);
    }
    for handle in handles {
        handle.join().unwrap();
    }
}
