use log::{info, warn};

use crate::{
    consensus2::{
        block::{ForkOrder, ForkResponse},
        ValidatorStatePtr,
    },
    net, Result,
};

/// async task used for consensus fork syncing.
pub async fn fork_sync_task(p2p: net::P2pPtr, state: ValidatorStatePtr) -> Result<()> {
    info!("Starting forks sync...");

    // Using len here beacuse is_empty() uses unstable library feature
    // called 'exact_size_is_empty'.
    if p2p.channels().lock().await.values().len() != 0 {
        // Nodes ask for the fork chains of the last channel peer
        let channel = p2p.channels().lock().await.values().last().unwrap().clone();

        // Communication setup
        let msg_subsystem = channel.get_message_subsystem();
        msg_subsystem.add_dispatch::<ForkResponse>().await;
        let response_sub = channel.subscribe_msg::<ForkResponse>().await?;

        // Node creates a `ForkOrder` and sends it
        let order = ForkOrder { id: state.read().await.id };
        channel.send(order).await?;

        // Node stores response data. Extra validations can be added here.
        let response = response_sub.receive().await?;
        state.write().await.consensus.proposals = response.proposals.clone();
    } else {
        warn!("Node is not connected to other nodes, resetting consensus state.");
        state.write().await.reset_consensus_state()?;
    }

    info!("Forks synced!");
    Ok(())
}
