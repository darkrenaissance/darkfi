use async_std::sync::{Arc, Mutex};

use chrono::Utc;
use log::{debug, error};

use crate::{net, util, Result};

mod consensus;
mod consensus_candidate;
mod consensus_follower;
mod consensus_leader;
mod datastore;
mod primitives;
mod protocol_raft;
mod settings;

pub use consensus::Raft;
pub use datastore::DataStore;
pub use primitives::NetMsg;
pub use protocol_raft::ProtocolRaft;
pub use settings::RaftSettings;

use primitives::NodeId;

// Auxilary function to periodically prun items, based on when they were received.
async fn prune_map<T: Clone + Eq + std::hash::Hash>(
    map: Arc<Mutex<fxhash::FxHashMap<T, i64>>>,
    seen_duration: i64,
) {
    loop {
        util::sleep(seen_duration as u64).await;
        debug!(target: "raft", "Pruning item in map");

        let now = Utc::now().timestamp();

        let mut map = map.lock().await;
        for (k, v) in map.clone().iter() {
            if now - v > seen_duration {
                map.remove(k);
            }
        }
    }
}

async fn p2p_send_loop(receiver: async_channel::Receiver<NetMsg>, p2p: net::P2pPtr) -> Result<()> {
    loop {
        let msg: NetMsg = receiver.recv().await?;
        if let Err(e) = p2p.broadcast(msg).await {
            error!(target: "raft", "error occurred during broadcasting a msg: {}", e);
            continue
        }
    }
}

fn gen_id() -> NodeId {
    let timestamp = Utc::now().timestamp();
    let hash: String = blake3::hash(&timestamp.to_be_bytes()).to_hex().to_string();
    NodeId(hash)
}
