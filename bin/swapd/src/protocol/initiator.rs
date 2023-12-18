use crate::protocol::traits::{
    HandleCounterpartyKeysReceivedResult, InitiateSwapArgs, InitiationArgs, Initiator,
};
use eyre::{eyre, Result};
use tokio::sync::mpsc::Receiver;

#[derive(Debug)]
pub(crate) enum Event {
    ReceivedCounterpartyKeys(InitiateSwapArgs),
    CounterpartyFundsLocked,
    CounterpartyFundsClaimed([u8; 32]),
    AlmostTimeout1,
    PastTimeout2,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum State {
    WaitingForCounterpartyKeys,
    WaitingForCounterpartyFundsLocked,
    WaitingForCounterpartyFundsClaimed,
    Completed,
}

struct Swap {
    args: InitiationArgs,
    handler: Box<dyn Initiator + Send + Sync>,
    event_rx: Receiver<Event>,
    state: tokio::sync::watch::Sender<State>,
}

impl Swap {
    fn new(
        args: InitiationArgs,
        handler: Box<dyn Initiator + Send + Sync>,
        event_rx: Receiver<Event>,
    ) -> (Self, tokio::sync::watch::Receiver<State>) {
        let state = tokio::sync::watch::channel(State::WaitingForCounterpartyKeys);
        (Self { args, handler, event_rx, state: state.0 }, state.1)
    }

    async fn run(mut self) -> Result<()> {
        let mut contract_swap_info: Option<HandleCounterpartyKeysReceivedResult> = None;

        loop {
            match self.event_rx.recv().await {
                Some(Event::ReceivedCounterpartyKeys(args)) => {
                    println!("received counterparty keys: {:?}", args);

                    if !matches!(*self.state.borrow(), State::WaitingForCounterpartyKeys) {
                        println!(
                            "unexpected event ReceivedCounterpartyKeys, state is {:?}",
                            *self.state.borrow()
                        );
                        return Err(eyre!(
                            "unexpected event: {:?}",
                            Event::ReceivedCounterpartyKeys(args)
                        ));
                    }

                    contract_swap_info =
                        Some(self.handler.handle_counterparty_keys_received(args).await?);
                    let _ = self.state.send(State::WaitingForCounterpartyFundsLocked);
                    //.expect("state channel should not be dropped");
                }
                Some(Event::CounterpartyFundsLocked) => {
                    println!("counterparty funds locked");
                    if !matches!(*self.state.borrow(), State::WaitingForCounterpartyFundsLocked) {
                        return Err(eyre!("unexpected event: {:?}", Event::CounterpartyFundsLocked));
                    }

                    self.handler
                        .handle_counterparty_funds_locked(
                            contract_swap_info
                                .as_ref()
                                .expect("contract swap info must be set")
                                .contract_swap
                                .clone(),
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
                        self.handler
                            .handle_should_refund(
                                contract_swap_info
                                    .as_ref()
                                    .expect("contract swap info must be set")
                                    .contract_swap
                                    .clone(),
                            )
                            .await?;

                        self.state
                            .send(State::Completed)
                            .expect("state channel should not be dropped");
                    }
                }
                Some(Event::PastTimeout2) => {
                    if !matches!(*self.state.borrow(), State::WaitingForCounterpartyFundsClaimed) {
                        return Err(eyre!("unexpected event: {:?}", Event::PastTimeout2));
                    }

                    // we're past timeout 2, and the counterparty hasn't claimed,
                    // so we need to refund
                    self.handler
                        .handle_should_refund(
                            contract_swap_info
                                .as_ref()
                                .expect("contract swap info must be set")
                                .contract_swap
                                .clone(),
                        )
                        .await?;

                    self.state.send(State::Completed).expect("state channel should not be dropped");
                }
                None => {
                    println!("event channel closed, exiting");
                    break;
                }
            }

            if matches!(*self.state.borrow(), State::Completed) {
                println!("swap completed, exiting");
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

    use ethers::prelude::{Address, SignerMiddleware, U256};

    use crate::protocol::traits::{CounterpartyKeys, InitiatorEventWatcher as _};

    struct MockOtherChainClient;

    impl OtherChainClient for MockOtherChainClient {
        fn claim_funds(&self, our_secret: [u8; 32], counterparty_secret: [u8; 32]) -> Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_initiator_swap() {
        let (event_tx, event_rx) = tokio::sync::mpsc::channel(1);

        let (contract_address, provider, wallet, _anvil) =
            crate::ethereum::test_utils::deploy_swap_creator().await;
        let signer = Arc::new(SignerMiddleware::new(provider, wallet));
        let contract = SwapCreator::new(contract_address, signer.clone());

        let other_chain_client = MockOtherChainClient;
        let secret = [0; 32]; // TODO generate an actual secp256k1 private key
        let initiator = EthInitiator::new(contract.clone(), other_chain_client, secret);

        let args = InitiationArgs {
            claim_commitment: [0; 32],
            claimer: signer.address(),
            timeout_duration_1: U256::from(120),
            timeout_duration_2: U256::from(120),
            asset: Address::zero(), // ETH
            value: U256::zero(),
            nonce: U256::zero(), // arbitrary
        };

        let (swap, mut state) = Swap::new(args.clone(), Box::new(initiator), event_rx);
        assert!(*state.borrow_and_update() == State::WaitingForCounterpartyKeys);

        let swap_task = tokio::spawn(async move { swap.run().await });

        let (counterparty_keys_tx, counterparty_keys_rx) = tokio::sync::oneshot::channel();
        let join_handle = tokio::spawn(Watcher::run_received_counterparty_keys_watcher(
            event_tx.clone(),
            counterparty_keys_rx,
            args,
        ));

        counterparty_keys_tx
            .send(CounterpartyKeys { secp256k1_public_key: [0; 33] })
            .expect("should send counterparty keys");
        state.changed().await.expect("state should change");
        assert!(*state.borrow() == State::WaitingForCounterpartyFundsLocked);
        join_handle.await.expect("task should not fail").expect("task should succeed");

        Watcher::run_counterparty_funds_locked_watcher(event_tx.clone())
            .await
            .expect("watcher should run");
        state.changed().await.expect("state should change");
        assert!(*state.borrow() == State::WaitingForCounterpartyFundsClaimed);

        // TODO need to watch for swap contract ID
        Watcher::run_counterparty_funds_claimed_watcher(event_tx, contract, &[0; 32], 1)
            .await
            .expect("watcher should run");
        state.changed().await.expect("state should change");
        assert!(*state.borrow() == State::Completed);

        swap_task.await.expect("task should not fail").expect("swap should succeed");
    }
}
