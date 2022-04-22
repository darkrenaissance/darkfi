use std::time::Duration;

use log::{debug, error, info};

use super::consensus_sync_task;
use crate::{
    consensus2::{state::ValidatorStatePtr, Participant},
    net,
    util::async_util::sleep,
};

/// async task used for participating in the consensus protocol
pub async fn proposal_task(p2p: net::P2pPtr, state: ValidatorStatePtr) {
    // Node waits just before the current or next epoch end,
    // so it can start syncing latest state.
    let mut seconds_until_next_epoch = state.read().await.next_epoch_start();
    let one_sec = Duration::new(1, 0);
    loop {
        if seconds_until_next_epoch > one_sec {
            seconds_until_next_epoch -= one_sec;
            break
        }
        info!("Waiting for next epoch ({:?} sec)...", seconds_until_next_epoch);
        sleep(seconds_until_next_epoch.as_secs()).await;
        seconds_until_next_epoch = state.read().await.next_epoch_start();
    }
    info!("Waiting for next epoch ({:?} sec)...", seconds_until_next_epoch);
    sleep(seconds_until_next_epoch.as_secs()).await;

    // Node syncs its consensus state
    match consensus_sync_task(p2p.clone(), state.clone()).await {
        Ok(()) => {}
        Err(e) => {
            error!("Failed syncing consensus state: {}. Quitting consensus.", e);
            return
        }
    }

    // Node signals the network that it will start participating
    let participant = Participant::new(state.read().await.id, state.read().await.current_epoch());
    state.write().await.append_participant(participant.clone());

    match p2p.broadcast(participant).await {
        Ok(()) => info!("Consensus participation message broadcasted successfully."),
        Err(e) => error!("Failed broadcasting consensus participation: {}", e),
    }

    // Note modifies its participating flag to true.
    state.write().await.participating = true;

    loop {
        let seconds_until_next_epoch = state.read().await.next_epoch_start().as_secs();
        info!(target: "consensus", "Waiting for next epoch ({:?} sec)...", seconds_until_next_epoch);
        sleep(seconds_until_next_epoch).await;

        // Node refreshes participants records
        state.write().await.refresh_participants();

        // Node checks if it's the epoch leader to generate a new proposal
        // for that epoch.
        let result = if state.write().await.is_epoch_leader() {
            state.read().await.propose()
        } else {
            Ok(None)
        };

        match result {
            Ok(proposal) => {
                if proposal.is_none() {
                    info!(target: "consensus", "Node is not the epoch leader. Sleeping till next epoch...");
                    continue
                }
                // Leader creates a vote for the proposal and broadcasts them both
                let proposal = proposal.unwrap();
                info!(target: "consensus", "Node is the epoch leader: Proposed block: {:?}", proposal);
                let vote = state.write().await.receive_proposal(&proposal);
                match vote {
                    Ok(v) => {
                        if v.is_none() {
                            debug!("proposal_task(): Node did not vote for the proposed block");
                        } else {
                            let vote = v.unwrap();
                            let result = state.write().await.receive_vote(&vote);
                            match result {
                                Ok(_) => info!(target: "consensus", "Vote saved successfully."),
                                Err(e) => error!(target: "consensus", "Vote save failed: {}", e),
                            }

                            // Broadcast block
                            let result = p2p.broadcast(proposal).await;
                            match result {
                                Ok(()) => {
                                    info!(target: "consensus", "Proposal broadcasted successfully.")
                                }
                                Err(e) => {
                                    error!(target: "consensus", "Failed broadcasting proposal: {}", e)
                                }
                            }

                            // Broadcast leader vote
                            let result = p2p.broadcast(vote).await;
                            match result {
                                Ok(()) => {
                                    info!(target: "consensus", "Leader vote broadcasted successfully.")
                                }
                                Err(e) => {
                                    error!(target: "consensus", "Failed broadcasting leader vote: {}", e)
                                }
                            }
                        }
                    }
                    Err(e) => error!(target: "consensus", "Failed processing proposal: {}", e),
                }
            }
            Err(e) => error!("Block proposal failed: {}", e),
        }
    }
}
