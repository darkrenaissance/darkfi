use std::time::Duration;

use log::{debug, error, info};

use super::consensus_sync_task;
use crate::{
    consensus::{Participant, ValidatorStatePtr},
    net::P2pPtr,
    util::sleep,
};

/// async task used for participating in the consensus protocol
pub async fn proposal_task(consensus_p2p: P2pPtr, sync_p2p: P2pPtr, state: ValidatorStatePtr) {
    // Node waits just before the current or next epoch end, so it can
    // start syncing latest state.
    let mut seconds_until_next_epoch = state.read().await.next_epoch_start();
    let one_sec = Duration::new(1, 0);

    loop {
        if seconds_until_next_epoch > one_sec {
            seconds_until_next_epoch -= one_sec;
            break
        }

        info!("consensus: Waiting for next epoch ({:?} sec)", seconds_until_next_epoch);
        sleep(seconds_until_next_epoch.as_secs()).await;
        seconds_until_next_epoch = state.read().await.next_epoch_start();
    }

    info!("consensus: Waiting for next epoch ({:?} sec)", seconds_until_next_epoch);
    sleep(seconds_until_next_epoch.as_secs()).await;

    // Node syncs its consensus state
    if let Err(e) = consensus_sync_task(consensus_p2p.clone(), state.clone()).await {
        error!("Failed syncing consensus state: {}. Quitting consensus.", e);
        // TODO: Perhaps notify over a channel in order to
        // stop consensus p2p protocols.
        return
    };

    // Node signals the network that iw till start participating
    let address = state.read().await.address;
    let cur_epoch = state.read().await.current_epoch();
    let participant = Participant::new(address, cur_epoch);
    state.write().await.append_participant(participant.clone());

    match consensus_p2p.broadcast(participant).await {
        Ok(()) => info!("consensus: Participation message broadcasted successfully."),
        Err(e) => error!("Failed broadcasting consensus participation: {}", e),
    }

    // Node modifies its participating epoch to next.
    match state.write().await.set_participating() {
        Ok(()) => info!("consensus: Node will start participating in the next epoch"),
        Err(e) => error!("Failed to set participation epoch: {}", e),
    }

    loop {
        let seconds_next_epoch = state.read().await.next_epoch_start().as_secs();
        info!("consensus: Waiting for next epoch ({} sec)", seconds_next_epoch);
        sleep(seconds_next_epoch).await;

        // Node refreshes participants records
        match state.write().await.refresh_participants() {
            Ok(()) => debug!("Participants refreshed successfully."),
            Err(e) => error!("Failed refreshing consensus participants: {}", e),
        }

        // Node checks if it's the epoch leader to generate a new proposal
        // for that epoch.
        let result = if state.write().await.is_epoch_leader() {
            state.read().await.propose()
        } else {
            Ok(None)
        };

        let proposal = match result {
            Ok(prop) => {
                if prop.is_none() {
                    info!("consensus: Node is not the epoch lead");
                    continue
                }
                prop.unwrap()
            }
            Err(e) => {
                error!("consensus: Block proposal failed: {}", e);
                continue
            }
        };

        info!("consensus: Node is the epoch leader: Proposed block: {:?}", proposal);
        let vote = state.write().await.receive_proposal(&proposal);
        let vote = match vote {
            Ok(v) => {
                if v.is_none() {
                    debug!("proposal_task(): Node did not vote for the proposed block");
                    continue
                }
                v.unwrap()
            }
            Err(e) => {
                error!("consensus: Failed processing proposal: {}", e);
                continue
            }
        };

        let result = state.write().await.receive_vote(&vote).await;
        match result {
            Ok((_, to_broadcast)) => {
                info!("consensus: Vote saved successfully");
                // Broadcast finalized blocks info, if any:
                if let Some(blocks) = to_broadcast {
                    info!("consensus: Broadcasting finalized blocks");
                    for info in blocks {
                        match sync_p2p.broadcast(info).await {
                            Ok(()) => info!("consensus: Broadcasted block"),
                            Err(e) => error!("consensus: Failed broadcasting block: {}", e),
                        }
                    }
                } else {
                    info!("consensus: No finalized blocks to broadcast");
                }
            }
            Err(e) => {
                error!("consensus: Vote save failed: {}", e);
                // TODO: Is this fallthrough ok?
            }
        }

        // Broadcast block to other consensus nodes
        match consensus_p2p.broadcast(proposal).await {
            Ok(()) => info!("consensus: Proposal broadcasted successfully"),
            Err(e) => error!("consensus: Failed broadcasting proposal: {}", e),
        }

        // Broadcast leader vote
        match consensus_p2p.broadcast(vote).await {
            Ok(()) => info!("consensus: Leader vote broadcasted successfully"),
            Err(e) => error!("consensus: Failed broadcasting leader vote: {}", e),
        }
    }
}
