use crate::protocol::traits::{InitiateSwapArgs, Initiator};
use eyre::{eyre, Result};
use tokio::sync::mpsc::Receiver;

use super::traits::HandleCounterpartyKeysReceivedResult;

#[derive(Debug)]
enum Event {
    ReceivedCounterpartyKeys(InitiateSwapArgs),
    CounterpartyFundsLocked,
    CounterpartyFundsClaimed,
    ShouldRefund,
}

#[derive(Debug)]
enum State {
    WaitingForCounterpartyKeys,
    WaitingForCounterpartyFundsLocked(HandleCounterpartyKeysReceivedResult),
    WaitingForCounterpartyFundsClaimed,
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
        loop {
            match self.event_rx.recv().await {
                Some(Event::ReceivedCounterpartyKeys(args)) => {
                    match self.state {
                        State::WaitingForCounterpartyKeys => {}
                        _ => {
                            return Err(eyre!(
                                "unexpected event: {:?}",
                                Event::ReceivedCounterpartyKeys(args)
                            ));
                        }
                    }

                    let res = self.handler.handle_counterparty_keys_received(args).await?;
                    self.state = State::WaitingForCounterpartyFundsLocked(res);
                }
                Some(Event::CounterpartyFundsLocked) => {
                    let state = std::mem::replace(
                        &mut self.state,
                        State::WaitingForCounterpartyFundsClaimed,
                    );

                    let input = match state {
                        State::WaitingForCounterpartyFundsLocked(res) => res,
                        _ => {
                            return Err(eyre!(
                                "unexpected event: {:?}",
                                Event::CounterpartyFundsLocked
                            ));
                        }
                    };

                    self.handler.handle_counterparty_funds_locked(input.contract_swap).await?;
                }
                Some(Event::CounterpartyFundsClaimed) => {
                    self.handler.handle_counterparty_funds_claimed().await;
                }
                Some(Event::ShouldRefund) => {
                    self.handler.handle_should_refund().await;
                }
                None => {
                    break;
                }
            }
        }

        Ok(())
    }
}
