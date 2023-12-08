use crate::protocol::traits::Initiator;
use tokio::sync::mpsc::Receiver;

enum Event {
    ReceivedCounterpartyKeys,
    CounterpartyFundsLocked,
    CounterpartyFundsClaimed,
    ShouldRefund,
}

struct Swap {
    handler: Box<dyn Initiator>,
    event_rx: Receiver<Event>,
}

impl Swap {
    fn new(handler: Box<dyn Initiator>, event_rx: Receiver<Event>) -> Self {
        Self { handler, event_rx }
    }

    async fn run(&mut self) {
        loop {
            match self.event_rx.recv().await {
                Some(Event::ReceivedCounterpartyKeys) => {
                    self.handler.handle_counterparty_keys_received();
                }
                Some(Event::CounterpartyFundsLocked) => {
                    self.handler.handle_counterparty_funds_locked();
                }
                Some(Event::CounterpartyFundsClaimed) => {
                    self.handler.handle_counterparty_funds_claimed();
                }
                Some(Event::ShouldRefund) => {
                    self.handler.handle_should_refund();
                }
                None => {
                    break;
                }
            }
        }
    }
}
