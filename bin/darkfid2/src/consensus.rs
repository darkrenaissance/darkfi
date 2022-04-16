use log::{debug, error, info};

use darkfi::{
    consensus2::{state::ValidatorStatePtr, Participant},
    net::P2pPtr,
    util::async_util::sleep,
};

pub async fn proposal_task(p2p: P2pPtr, state: ValidatorStatePtr) {
    // Node signals the network that it starts participating
    let participant = Participant::new(state.read().await.id, state.read().await.current_epoch());
    state.write().await.append_participant(participant.clone());

    match p2p.broadcast(participant).await {
        Ok(()) => info!("Consensus participation message broadcasted successfully."),
        Err(e) => error!("Failed broadcasting consensus participation: {}", e),
    }

    // After initialization, node should wait for next epoch
    let seconds_until_next_epoch = state.read().await.next_epoch_start().as_secs();
    info!("Waiting for next epoch({} sec)...", seconds_until_next_epoch);
    sleep(seconds_until_next_epoch).await;

    loop {
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
                    info!("Node is not the epoch leader. Sleeping till next epoch...");
                } else {
                    // Leader creates a vote for the proposal and broadcasts them both
                    let proposal = proposal.unwrap();
                    info!("Node is the epoch leader: Proposed block: {:?}", proposal);
                    let vote = state.write().await.receive_proposal(&proposal);
                    match vote {
                        Ok(v) => {
                            if v.is_none() {
                                debug!("Node did not vote for the proposed block");
                            } else {
                                let vote = v.unwrap();
                                let result = state.write().await.receive_vote(&vote);
                                match result {
                                    Ok(_) => info!("Vote saved successfully."),
                                    Err(e) => error!("Vote save failed: {}", e),
                                }

                                // Broadcast block
                                let result = p2p.broadcast(proposal).await;
                                match result {
                                    Ok(()) => info!("Proposal broadcasted successfully."),
                                    Err(e) => error!("Failed broadcasting proposal: {}", e),
                                }

                                // Broadcast leader vote
                                let result = p2p.broadcast(vote).await;
                                match result {
                                    Ok(()) => info!("Leader vote broadcasted successfully."),
                                    Err(e) => error!("Failed broadcasting leader vote: {}", e),
                                }
                            }
                        }
                        Err(e) => error!("Failed processing proposal: {}", e),
                    }
                }
            }
            Err(e) => error!("Block proposal failed: {}", e),
        }

        // Node waits until next epoch
        let seconds_until_next_epoch = state.read().await.next_epoch_start().as_secs();
        info!("Waiting for next epoch ({} sec)...", seconds_until_next_epoch);
        sleep(seconds_until_next_epoch).await;
    }
}
