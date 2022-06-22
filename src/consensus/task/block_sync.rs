use crate::{
    consensus::{
        block::{BlockOrder, BlockResponse},
        ValidatorState, ValidatorStatePtr,
    },
    net,
    node::MemoryState,
    Result,
};
use log::{debug, info, warn};

/// async task used for block syncing.
pub async fn block_sync_task(p2p: net::P2pPtr, state: ValidatorStatePtr) -> Result<()> {
    info!("Starting blockchain sync...");

    // we retrieve p2p network connected channels, so we can use it to
    // parallelize downloads.
    // Using len here because is_empty() uses unstable library feature
    // called 'exact_size_is_empty'.
    if p2p.channels().lock().await.values().len() != 0 {
        // Currently we will just use the last channel
        let channel = p2p.channels().lock().await.values().last().unwrap().clone();

        // Communication setup
        let msg_subsystem = channel.get_message_subsystem();
        msg_subsystem.add_dispatch::<BlockResponse>().await;
        let response_sub = channel.subscribe_msg::<BlockResponse>().await?;

        // Node sends the last known block hash of the canonical blockchain
        // and loops until the response is the same block (used to utilize
        // batch requests).
        let mut last = state.read().await.blockchain.last()?;
        info!("Last known block: {:?} - {:?}", last.0, last.1);

        loop {
            // Node creates a `BlockOrder` and sends it
            let order = BlockOrder { slot: last.0, block: last.1 };
            channel.send(order).await?;

            // Node stores response data.
            let resp = response_sub.receive().await?;

            // Verify state transitions for all blocks and their respective transactions.
            debug!("block_sync_task(): Starting state transition validations");
            let mut canon_updates = vec![];
            let canon_state_clone = state.read().await.state_machine.lock().await.clone();
            let mut mem_state = MemoryState::new(canon_state_clone);
            for block in &resp.blocks {
                let mut state_updates =
                    ValidatorState::validate_state_transitions(mem_state.clone(), &block.txs)?;

                for update in &state_updates {
                    mem_state.apply(update.clone());
                }

                canon_updates.append(&mut state_updates);
            }
            debug!("block_sync_task(): All state transitions passed");

            debug!("block_sync_task(): Updating canon state");
            state.write().await.update_canon_state(canon_updates, None).await?;

            debug!("block_sync_task(): Appending blocks to ledger");
            state.write().await.blockchain.add(&resp.blocks)?;

            let last_received = state.read().await.blockchain.last()?;
            info!("Last received block: {:?} - {:?}", last_received.0, last_received.1);

            if last == last_received {
                break
            }

            last = last_received;
        }
    } else {
        warn!("Node is not connected to other nodes");
    }

    info!("Blockchain synced!");
    Ok(())
}
