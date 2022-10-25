use async_std::sync::Arc;
use darkfi_serial::serialize;
use log::{debug, error};
use rand::Rng;
use smol::Executor;

use crate::{
    consensus::{state::QUARANTINE_DURATION, KeepAlive, ValidatorStatePtr},
    crypto::schnorr::SchnorrSecret,
    net::P2pPtr,
    util::async_util::sleep,
    Result,
};

/// async task used for sending keep alive messages in background.
pub async fn keep_alive_task(
    p2p: P2pPtr,
    state: ValidatorStatePtr,
    ex: Arc<Executor<'_>>,
) -> Result<()> {
    ex.spawn(async move {
        loop {
            // Pick a random slot in range: next slot + QUARANTINE_DURATION, exluding first and last
            let slot = rand::thread_rng().gen_range(2..QUARANTINE_DURATION);
            let seconds = state.read().await.next_n_slot_start(slot).as_secs();
            debug!("keep_alive_task: Waiting for next {} slots ({} sec)", slot, seconds);

            // Sleep until that slot
            sleep(seconds).await;

            // TODO: [PLACEHOLDER] Add balance proof creation

            // Create keep alive message
            let secret = state.read().await.secret;
            let address = state.read().await.address;
            let slot = state.read().await.current_slot();
            let serialized = serialize(&slot);
            let signature = secret.sign(&serialized);
            let keep_alive = KeepAlive { address, slot, signature };

            // Broadcast keep alive message
            match p2p.broadcast(keep_alive).await {
                Ok(()) => debug!("keep_alive_task: Keep alive message broadcasted successfully."),
                Err(e) => error!("keep_alive_task: Failed broadcasting keep alive message: {}", e),
            }
        }
    })
    .detach();

    Ok(())
}
