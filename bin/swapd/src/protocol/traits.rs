use crate::ethereum::swap_creator::Swap; // TODO: shouldn't depend on this
use darkfi_serial::async_trait;
use ethers::prelude::*;
use eyre::Result;

// TODO: make Address/U256 generic; these are ethers-specific right now
#[derive(Debug)]
pub(crate) struct InitiateSwapArgs {
    pub(crate) claim_commitment: [u8; 32],
    pub(crate) refund_commitment: [u8; 32],
    pub(crate) claimer: Address,
    pub(crate) timeout_duration_1: U256,
    pub(crate) timeout_duration_2: U256,
    pub(crate) asset: Address,
    pub(crate) value: U256,
    pub(crate) nonce: U256,
}

#[derive(Debug)]
pub(crate) struct HandleCounterpartyKeysReceivedResult {
    pub(crate) contract_swap_id: [u8; 32],
    pub(crate) contract_swap: Swap,
}

/// the chain that initiates the swap; ie. the first-mover
///
/// the implementation of this trait must hold a signing key for
/// chain A and chain B.
///
/// TODO: [`Swap`] should be a non-chain-specific type
#[async_trait]
pub(crate) trait Initiator {
    // initiates the swap by locking funds on chain A
    async fn handle_counterparty_keys_received(
        &self,
        args: InitiateSwapArgs,
    ) -> Result<HandleCounterpartyKeysReceivedResult>;

    // handles the counterparty locking funds
    async fn handle_counterparty_funds_locked(&self, swap: Swap) -> Result<()>;

    // handles the counterparty claiming funds
    async fn handle_counterparty_funds_claimed(&self, counterparty_secret: [u8; 32]) -> Result<()>;

    // handles the timeout cases where we need to refund funds
    async fn handle_should_refund(&self, swap: Swap) -> Result<()>;
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
