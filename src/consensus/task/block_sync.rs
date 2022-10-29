use crate::{
    consensus::{
        block::{BlockOrder, BlockResponse},
        ValidatorStatePtr,
    },
    net, Result,
};
use log::{debug, info, warn};

/// async task used for block syncing.
pub async fn block_sync_task(p2p: net::P2pPtr, state: ValidatorStatePtr) -> Result<()> {
    info!("Starting blockchain sync...");
    // Getting a random connected channel to ask for peers
    match p2p.clone().random_channel().await {
        Some(channel) => {
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

                // Verify and store retrieved blocks
                debug!("block_sync_task(): Processing received blocks");
                state.write().await.receive_sync_blocks(&resp.blocks).await?;

                let last_received = state.read().await.blockchain.last()?;
                info!("Last received block: {:?} - {:?}", last_received.0, last_received.1);

                if last == last_received {
                    break
                }

                last = last_received;
            }
        },
        None => warn!("Node is not connected to other nodes"),
    };

    info!("Blockchain synced!");
    Ok(())
}
