use crate::{
    consensus::{
        block::{BlockOrder, BlockResponse},
        ValidatorStatePtr,
    },
    net, Result,
};
use log::{info, warn};

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
        let mut last = state.read().await.blockchain.last()?.unwrap();
        info!("Last known block: {:?} - {:?}", last.0, last.1);

        loop {
            // Node creates a `BlockOrder` and sends it
            let order = BlockOrder { sl: last.0, block: last.1 };
            channel.send(order).await?;

            // Node stores response data. Extra validations can be added here.
            let response = response_sub.receive().await?;
            state.write().await.blockchain.add(&response.blocks)?;

            let last_received = state.read().await.blockchain.last()?.unwrap();
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
