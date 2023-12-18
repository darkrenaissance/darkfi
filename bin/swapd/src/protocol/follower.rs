use crate::protocol::traits::Follower;
use tokio::sync::mpsc::Receiver;

#[allow(dead_code)]
enum Event {
    CounterpartyFundsLocked,
    ReadyToClaim,
    CounterpartyFundsRefunded,
}

#[allow(dead_code)]
struct Swap {
    handler: Box<dyn Follower>,
    event_rx: Receiver<Event>,
}

#[allow(dead_code)]
impl Swap {
    fn new(handler: Box<dyn Follower>, event_rx: Receiver<Event>) -> Self {
        Self { handler, event_rx }
    }

    async fn run(&mut self) {
        loop {
            match self.event_rx.recv().await {
                Some(Event::CounterpartyFundsLocked) => {
                    self.handler.handle_counterparty_funds_locked();
                }
                Some(Event::ReadyToClaim) => {
                    self.handler.handle_ready_to_claim();
                }
                Some(Event::CounterpartyFundsRefunded) => {
                    self.handler.handle_counterparty_funds_refunded();
                }
                None => {
                    break;
                }
            }
        }
    }
}
