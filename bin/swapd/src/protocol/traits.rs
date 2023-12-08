/// the chain that initiates the swap; ie. the first-mover
///
/// the implementation of this trait must hold a signing key for
/// chain A and chain B.
pub(crate) trait Initiator {
    // initiates the swap by locking funds on chain A
    fn handle_counterparty_keys_received(&self);

    // handles the counterparty locking funds
    fn handle_counterparty_funds_locked(&self);

    // handles the counterparty claiming funds
    fn handle_counterparty_funds_claimed(&self);

    // handles the timeout cases where we need to refund funds
    fn handle_should_refund(&self);
}

/// the chain that is the counterparty to the swap; ie. the second-mover
pub(crate) trait Follower {
    // handle the swap initiation by locking funds on chain B
    fn handle_counterparty_funds_locked(&self);

    // handle the funds being ready to be claimed by us
    fn handle_ready_to_claim(&self);

    // handle the counterparty refunding their funds, in case of a timeout
    fn handle_counterparty_funds_refunded(&self);
}
