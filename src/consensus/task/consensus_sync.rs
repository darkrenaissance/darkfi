use log::{info, warn};

use crate::{
    consensus::{
        state::{ConsensusRequest, ConsensusResponse},
        ValidatorStatePtr,
    },
    net::P2pPtr,
    Result,
};

/// async task used for consensus state syncing.
pub async fn consensus_sync_task(p2p: P2pPtr, state: ValidatorStatePtr) -> Result<()> {
    info!("Starting consensus state sync...");
    let channels_map = p2p.channels().lock().await;
    let values = channels_map.values();
    // Using len here beacuse is_empty() uses unstable library feature
    // called 'exact_size_is_empty'.
    if values.len() != 0 {
        // Node iterates the channel peers to ask for their consensus state
        for channel in values {
            // Communication setup
            let msg_subsystem = channel.get_message_subsystem();
            msg_subsystem.add_dispatch::<ConsensusResponse>().await;
            let response_sub = channel.subscribe_msg::<ConsensusResponse>().await?;

            // Node creates a `ConsensusRequest` and sends it
            let request = ConsensusRequest { address: state.read().await.address };
            channel.send(request).await?;

            // Node verifies response came from a participating node.
            // Extra validations can be added here.
            let response = response_sub.receive().await?;
            if response.consensus.participants.is_empty() {
                warn!("Retrieved consensus state from a new node, retrying...");
                continue
            }
            // Node stores response data.
            state.write().await.consensus = response.consensus.clone();
        }
    } else {
        warn!("Node is not connected to other nodes, resetting consensus state.");
        state.write().await.reset_consensus_state()?;
    }

    info!("Consensus state synced!");
    Ok(())
}
