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

    // Using len here beacuse is_empty() uses unstable library feature
    // called 'exact_size_is_empty'.
    if p2p.channels().lock().await.values().len() != 0 {
        // Nodes ask for the consensus state of the last channel peer
        let channel = p2p.channels().lock().await.values().last().unwrap().clone();

        // Communication setup
        let msg_subsystem = channel.get_message_subsystem();
        msg_subsystem.add_dispatch::<ConsensusResponse>().await;
        let response_sub = channel.subscribe_msg::<ConsensusResponse>().await?;

        // Node creates a `ConsensusRequest` and sends it
        let request = ConsensusRequest { id: state.read().await.id };
        channel.send(request).await?;

        // Node stores response data. Extra validations can be added here.
        let response = response_sub.receive().await?;
        state.write().await.consensus = response.consensus.clone();
    } else {
        warn!("Node is not connected to other nodes, resetting consensus state.");
        state.write().await.reset_consensus_state()?;
    }

    info!("Consensus state synced!");
    Ok(())
}
