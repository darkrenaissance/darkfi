use crate::protocol::traits::{InitiateSwapArgs, Initiator};
use eyre::{eyre, Result};
use tokio::sync::mpsc::Receiver;

use super::traits::HandleCounterpartyKeysReceivedResult;

#[derive(Debug)]
enum Event {
    ReceivedCounterpartyKeys(InitiateSwapArgs),
    CounterpartyFundsLocked,
    CounterpartyFundsClaimed([u8; 32]),
    ShouldRefund,
}

#[derive(Debug)]
enum State {
    WaitingForCounterpartyKeys,
    WaitingForCounterpartyFundsLocked,
    WaitingForCounterpartyFundsClaimed,
    Completed,
}

struct Swap {
    handler: Box<dyn Initiator>,
    event_rx: Receiver<Event>,
    state: State,
}

impl Swap {
    fn new(handler: Box<dyn Initiator>, event_rx: Receiver<Event>) -> Self {
        Self { handler, event_rx, state: State::WaitingForCounterpartyKeys }
    }

    async fn run(&mut self) -> Result<()> {
        let mut contract_swap_info: Option<HandleCounterpartyKeysReceivedResult> = None;

        loop {
            match self.event_rx.recv().await {
                Some(Event::ReceivedCounterpartyKeys(args)) => {
                    if !matches!(self.state, State::WaitingForCounterpartyKeys) {
                        return Err(eyre!(
                            "unexpected event: {:?}",
                            Event::ReceivedCounterpartyKeys(args)
                        ));
                    }

                    contract_swap_info =
                        Some(self.handler.handle_counterparty_keys_received(args).await?);
                    self.state = State::WaitingForCounterpartyFundsLocked;
                }
                Some(Event::CounterpartyFundsLocked) => {
                    let state = std::mem::replace(
                        &mut self.state,
                        State::WaitingForCounterpartyFundsClaimed,
                    );

                    if !matches!(state, State::WaitingForCounterpartyFundsLocked) {
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
                }
                Some(Event::CounterpartyFundsClaimed(counterparty_secret)) => {
                    let state = std::mem::replace(&mut self.state, State::Completed);

                    if !matches!(state, State::WaitingForCounterpartyFundsClaimed) {
                        return Err(eyre!(
                            "unexpected event: {:?}",
                            Event::CounterpartyFundsClaimed(counterparty_secret)
                        ));
                    }

                    self.handler.handle_counterparty_funds_claimed(counterparty_secret).await?;
                }
                Some(Event::ShouldRefund) => {
                    let state = std::mem::replace(&mut self.state, State::Completed);

                    match state {
                        State::WaitingForCounterpartyFundsLocked |
                        State::WaitingForCounterpartyFundsClaimed => {}
                        _ => {
                            return Err(eyre!("unexpected event: {:?}", Event::ShouldRefund));
                        }
                    }
                    self.handler
                        .handle_should_refund(
                            contract_swap_info
                                .as_ref()
                                .expect("contract swap info must be set")
                                .contract_swap
                                .clone(),
                        )
                        .await?;
                }
                None => {
                    break;
                }
            }
        }

        Ok(())
    }
}
