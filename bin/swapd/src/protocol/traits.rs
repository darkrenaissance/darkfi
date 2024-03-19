use crate::ethereum::swap_creator::Swap; // TODO: shouldn't depend on this
use crate::{error::Error, ethereum::swap_creator::SwapCreator, protocol::initiator::Event};
use darkfi_serial::async_trait;
use ethers::{prelude::*, utils::hex};
use smol::channel;
use std::{
    fmt,
    fmt::{Display, Formatter},
};

// Initial parameters required by the swap initiator.
// TODO: make Address/U256 generic; these are ethers-specific right now
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct InitiationArgs {
    pub(crate) claim_commitment: [u8; 32],
    pub(crate) claimer: Address,
    pub(crate) timeout_duration_1: U256,
    pub(crate) timeout_duration_2: U256,
    pub(crate) asset: Address,
    pub(crate) value: U256,
    pub(crate) nonce: U256,
}

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

// TODO: make this generic for both chains
#[allow(dead_code)]
#[derive(Debug)]
pub(crate) struct CounterpartyKeys {
    pub(crate) secp256k1_public_key: [u8; 33],
}

impl Display for CounterpartyKeys {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "CounterpartyKeys {{ secp256k1_public_key: {:?} }}",
            hex::encode(self.secp256k1_public_key)
        )
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct HandleCounterpartyKeysReceivedResult {
    // the ID of the swap within the on-chain contract
    pub(crate) contract_swap_id: [u8; 32],

    // the details of the swap within the on-chain contract
    pub(crate) contract_swap: Swap,

    // the block number at which the swap was initiated
    pub(crate) block_number: u64,
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
    ) -> Result<HandleCounterpartyKeysReceivedResult, Error>;

    // handles the counterparty locking funds
    async fn handle_counterparty_funds_locked(
        &self,
        swap: Swap,
        swap_id: [u8; 32],
    ) -> Result<(), Error>;

    // handles the counterparty claiming funds
    async fn handle_counterparty_funds_claimed(
        &self,
        counterparty_secret: [u8; 32],
    ) -> Result<(), Error>;

    // handles the timeout cases where we need to refund funds
    async fn handle_should_refund(&self, swap: Swap) -> Result<(), Error>;
}

#[async_trait]
pub(crate) trait InitiatorEventWatcher {
    async fn run_received_counterparty_keys_watcher(
        event_tx: channel::Sender<Event>,
        counterparty_keys_rx: channel::Receiver<CounterpartyKeys>,
    ) -> Result<(), Error>;

    async fn run_counterparty_funds_locked_watcher(
        event_tx: channel::Sender<Event>,
    ) -> Result<(), Error>;

    // TODO: make this generic for both chains
    async fn run_counterparty_funds_claimed_watcher<M: Middleware>(
        event_tx: channel::Sender<Event>,
        contract: SwapCreator<M>,
        contract_swap_id: &[u8; 32],
        from_block: u64,
    ) -> Result<(), Error>;

    async fn run_timeout_1_watcher(
        event_tx: channel::Sender<Event>,
        timeout_1: u64,
        buffer_seconds: u64,
    ) -> Result<(), Error>;

    async fn run_timeout_2_watcher(
        event_tx: channel::Sender<Event>,
        timeout_2: u64,
    ) -> Result<(), Error>;
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
