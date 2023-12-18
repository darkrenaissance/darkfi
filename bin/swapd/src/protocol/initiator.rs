use crate::protocol::traits::{
    CounterpartyKeys, HandleCounterpartyKeysReceivedResult, InitiateSwapArgs, InitiationArgs,
    Initiator,
};
use eyre::{eyre, Context, Result};
use tokio::sync::{mpsc, watch};
use tracing::{info, warn};

#[derive(Debug)]
pub(crate) enum Event {
    ReceivedCounterpartyKeys(CounterpartyKeys),
    CounterpartyFundsLocked,
    CounterpartyFundsClaimed([u8; 32]),
    AlmostTimeout1,
    PastTimeout2,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
enum State {
    WaitingForCounterpartyKeys,
    WaitingForCounterpartyFundsLocked,
    WaitingForCounterpartyFundsClaimed,
    Completed,
}

#[allow(dead_code)]
struct Swap {
    // the initial parameters required for the swap
    args: InitiationArgs,

    // the chain-specific event handler
    // TODO: just make this a generic
    handler: Box<dyn Initiator + Send + Sync>,

    // the event receiver channel for the swap
    // the [`Watcher`] sends events to this channel
    event_rx: mpsc::Receiver<Event>,

    // the current state of the swap
    state: watch::Sender<State>,

    // the info of the swap within the on-chain contract
    contract_swap_info: watch::Sender<Option<HandleCounterpartyKeysReceivedResult>>,
}

#[allow(dead_code)]
impl Swap {
    fn new(
        args: InitiationArgs,
        handler: Box<dyn Initiator + Send + Sync>,
        event_rx: mpsc::Receiver<Event>,
    ) -> (Self, watch::Receiver<State>, watch::Receiver<Option<HandleCounterpartyKeysReceivedResult>>)
    {
        let state = tokio::sync::watch::channel(State::WaitingForCounterpartyKeys);
        let contract_swap_info = tokio::sync::watch::channel(None);
        (
            Self {
                args,
                handler,
                event_rx,
                state: state.0,
                contract_swap_info: contract_swap_info.0,
            },
            state.1,
            contract_swap_info.1,
        )
    }

    async fn run(mut self) -> Result<()> {
        loop {
            match self.event_rx.recv().await {
                Some(Event::ReceivedCounterpartyKeys(counterparty_keys)) => {
                    info!("received counterparty keys");

                    if !matches!(*self.state.borrow(), State::WaitingForCounterpartyKeys) {
                        warn!(
                            "unexpected event ReceivedCounterpartyKeys, state is {:?}",
                            *self.state.borrow()
                        );
                        return Err(eyre!(
                            "unexpected event: {:?}",
                            Event::ReceivedCounterpartyKeys(counterparty_keys)
                        ));
                    }

                    let refund_commitment =
                        ethers::utils::keccak256(&counterparty_keys.secp256k1_public_key);

                    let args = InitiateSwapArgs {
                        claim_commitment: self.args.claim_commitment,
                        refund_commitment,
                        claimer: self.args.claimer,
                        timeout_duration_1: self.args.timeout_duration_1,
                        timeout_duration_2: self.args.timeout_duration_2,
                        asset: self.args.asset,
                        value: self.args.value,
                        nonce: self.args.nonce,
                    };

                    let contract_swap_info = Some(
                        self.handler
                            .handle_counterparty_keys_received(args)
                            .await
                            .wrap_err("failed to handle receiving counterparty keys")?,
                    );

                    let _ = self.contract_swap_info.send(contract_swap_info.clone());
                    self.state
                        .send(State::WaitingForCounterpartyFundsLocked)
                        .expect("state channel should not be dropped");
                }
                Some(Event::CounterpartyFundsLocked) => {
                    info!("counterparty funds locked");
                    if !matches!(*self.state.borrow(), State::WaitingForCounterpartyFundsLocked) {
                        return Err(eyre!("unexpected event: {:?}", Event::CounterpartyFundsLocked));
                    }

                    let contract_swap_info = self
                        .contract_swap_info
                        .borrow()
                        .clone()
                        .expect("contract swap info must be set");

                    self.handler
                        .handle_counterparty_funds_locked(
                            contract_swap_info.contract_swap,
                            contract_swap_info.contract_swap_id,
                        )
                        .await?;

                    self.state
                        .send(State::WaitingForCounterpartyFundsClaimed)
                        .expect("state channel should not be dropped");
                }
                Some(Event::CounterpartyFundsClaimed(counterparty_secret)) => {
                    if !matches!(*self.state.borrow(), State::WaitingForCounterpartyFundsClaimed) {
                        return Err(eyre!(
                            "unexpected event: {:?}",
                            Event::CounterpartyFundsClaimed(counterparty_secret)
                        ));
                    }

                    self.handler.handle_counterparty_funds_claimed(counterparty_secret).await?;
                    self.state.send(State::Completed).expect("state channel should not be dropped");
                }
                Some(Event::AlmostTimeout1) => {
                    match *self.state.borrow() {
                        State::WaitingForCounterpartyFundsLocked |
                        State::WaitingForCounterpartyFundsClaimed => {}
                        _ => {
                            return Err(eyre!("unexpected event: {:?}", Event::AlmostTimeout1));
                        }
                    }

                    // we're almost at timeout 1, and the counterparty hasn't locked,
                    // so we need to refund
                    if matches!(*self.state.borrow(), State::WaitingForCounterpartyFundsLocked) {
                        let contract_swap_info = self
                            .contract_swap_info
                            .borrow()
                            .clone()
                            .expect("contract swap info must be set");

                        self.handler.handle_should_refund(contract_swap_info.contract_swap).await?;

                        self.state
                            .send(State::Completed)
                            .expect("state channel should not be dropped");
                    }
                }
                Some(Event::PastTimeout2) => {
                    if !matches!(*self.state.borrow(), State::WaitingForCounterpartyFundsClaimed) {
                        return Err(eyre!("unexpected event: {:?}", Event::PastTimeout2));
                    }

                    let contract_swap_info = self
                        .contract_swap_info
                        .borrow()
                        .clone()
                        .expect("contract swap info must be set");

                    // we're past timeout 2, and the counterparty hasn't claimed,
                    // so we need to refund
                    self.handler
                        .handle_should_refund(contract_swap_info.contract_swap.clone())
                        .await?;

                    self.state.send(State::Completed).expect("state channel should not be dropped");
                }
                None => {
                    info!("event channel closed, exiting");
                    break;
                }
            }

            if matches!(*self.state.borrow(), State::Completed) {
                info!("swap completed, exiting");
                break;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use std::sync::Arc;

    use crate::ethereum::{
        initiator::OtherChainClient, swap_creator::SwapCreator, EthInitiator, Watcher,
    };

    use ethers::{
        core::k256::elliptic_curve::sec1::ToEncodedPoint,
        prelude::{Address, SignerMiddleware, U256},
    };

    use crate::protocol::traits::{CounterpartyKeys, InitiatorEventWatcher as _};
    // use ethers::{
    //     core::k256::elliptic_curve::{PublicKey, SecretKey},
    //     prelude::rand,
    // };

    struct MockOtherChainClient;

    impl OtherChainClient for MockOtherChainClient {
        fn claim_funds(&self, _our_secret: [u8; 32], _counterparty_secret: [u8; 32]) -> Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_initiator_swap_success() {
        let (event_tx, event_rx) = tokio::sync::mpsc::channel(1);

        let (contract_address, provider, wallet, anvil) =
            crate::ethereum::test_utils::deploy_swap_creator().await;
        let signer = Arc::new(SignerMiddleware::new(provider, wallet));
        let contract = SwapCreator::new(contract_address, signer.clone());

        let other_chain_client = MockOtherChainClient;
        let refund_secret = [0; 32]; // TODO generate an actual secp256k1 private key for refund testing
        let initiator =
            EthInitiator::new(contract.clone(), signer.clone(), other_chain_client, refund_secret);

        // let counterparty_secret = SecretKey::<Secp256k1>::random(&mut rand::thread_rng());
        // let counterparty_public_key = PublicKey::<Secp256k1>::from_secret_scalar(&counterparty_secret.to_nonzero_scalar());

        // TODO: this is the same key as the initiator right now.
        let counterparty_secret: [u8; 32] = anvil.keys()[0].to_bytes().try_into().unwrap();
        let counterparty_public_key = anvil.keys()[0].public_key();
        let pubkey_bytes: [u8; 64] =
            counterparty_public_key.to_encoded_point(false).as_bytes()[1..].try_into().unwrap();
        let claim_commitment = ethers::utils::keccak256(pubkey_bytes);

        let args = InitiationArgs {
            claim_commitment,
            claimer: signer.address(),
            timeout_duration_1: U256::from(120),
            timeout_duration_2: U256::from(120),
            asset: Address::zero(),                      // ETH
            value: 1_000_000_000_000_000_000u128.into(), // 1 ETH
            nonce: U256::zero(),                         // arbitrary
        };

        let (swap, mut state, contract_swap_id) =
            Swap::new(args.clone(), Box::new(initiator), event_rx);
        assert!(*state.borrow_and_update() == State::WaitingForCounterpartyKeys);

        let swap_task = tokio::spawn(async move { swap.run().await });

        let (counterparty_keys_tx, counterparty_keys_rx) = tokio::sync::oneshot::channel();
        let join_handle = tokio::spawn(Watcher::run_received_counterparty_keys_watcher(
            event_tx.clone(),
            counterparty_keys_rx,
        ));

        counterparty_keys_tx
            .send(CounterpartyKeys { secp256k1_public_key: [0; 33] })
            .expect("should send counterparty keys");
        state.changed().await.expect("state should change");
        assert!(*state.borrow() == State::WaitingForCounterpartyFundsLocked);
        join_handle.abort();

        Watcher::run_counterparty_funds_locked_watcher(event_tx.clone())
            .await
            .expect("watcher should run");
        state.changed().await.expect("state should change");
        assert!(*state.borrow() == State::WaitingForCounterpartyFundsClaimed);

        let contract_swap = contract_swap_id.borrow().as_ref().unwrap().contract_swap.clone();

        let contract_clone = contract.clone();
        tokio::spawn(async move {
            let tx = contract_clone.claim(contract_swap, counterparty_secret);

            let receipt = tx
                .send()
                .await
                .expect("failed to submit transaction")
                .await
                .expect("failed to await pending transaction")
                .expect("no receipt found");

            assert!(
                receipt.status == Some(ethers::types::U64::from(1)),
                "`claim` transaction failed: {:?}",
                receipt
            );
        });

        Watcher::run_counterparty_funds_claimed_watcher(
            event_tx,
            contract,
            &contract_swap_id.borrow().as_ref().unwrap().contract_swap_id,
            contract_swap_id.borrow().as_ref().unwrap().block_number,
        )
        .await
        .expect("watcher should run");
        state.changed().await.expect("state should change");
        assert!(*state.borrow() == State::Completed);

        swap_task.await.expect("task should not fail").expect("swap should succeed");
    }
}
