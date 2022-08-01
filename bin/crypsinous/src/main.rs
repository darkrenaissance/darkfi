use  ::darkfi::{
    stakeholder::Stakeholder,
    blockchain::{EpochConsensus,},
    net::{Settings,},
};

use futures::executor::block_on;
use url::Url;
use std::thread;

fn main()
{
    let slots=22;
    let epochs=3;
    let ticks=22;
    let reward=1;
    let epoch_consensus = EpochConsensus::new(Some(slots), Some(epochs), Some(ticks), Some(reward));
    /// read n from the cmd
    let n = 3;
    /// initialize n stakeholders
    //let mut stakeholders = vec!();
    let settings = Settings{
        inbound: Some(Url::parse("tls://127.0.0.1:12002").unwrap()),
        outbound_connections: 4,
        manual_attempt_limit: 0,
        seed_query_timeout_seconds: 8,
        connect_timeout_seconds: 10,
        channel_handshake_seconds: 4,
        channel_heartbeat_seconds: 10,
        external_addr: Some(Url::parse("tls://127.0.0.1:12002").unwrap()),
        peers: [Url::parse("tls://127.0.0.1:12003").unwrap()].to_vec(),
        seeds: [Url::parse("tls://irc0.dark.fi:11001").unwrap(),
                Url::parse("tls://irc1.dark.fi:11001").unwrap()
        ].to_vec(),
    };
    let k : u32 = 13; //proof's number of rows
    let mut handles = vec!();
    for i in 0..n {
        let mut stakeholder = block_on(Stakeholder::new(epoch_consensus.clone(), settings.clone(), Some(k))).unwrap();
        let handle = thread::spawn(move || {
            block_on(stakeholder.background());
        });
        //stakeholders.push(stakeholder);
        handles.push(handle);
    }
    for handle in handles {
        handle.join().unwrap();
    }
}
