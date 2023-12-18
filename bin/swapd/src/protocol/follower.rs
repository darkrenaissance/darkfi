use crate::protocol::traits::Follower;
use smol::channel;

#[allow(dead_code)]
enum Event {
    CounterpartyFundsLocked,
    ReadyToClaim,
    CounterpartyFundsRefunded,
}

#[allow(dead_code)]
struct Swap {
    handler: Box<dyn Follower>,
    event_rx: channel::Receiver<Event>,
}

#[allow(dead_code)]
impl Swap {
    fn new(handler: Box<dyn Follower>, event_rx: channel::Receiver<Event>) -> Self {
        Self { handler, event_rx }
    }

    async fn run(&mut self) {
        loop {
            match self.event_rx.recv().await {
                Ok(Event::CounterpartyFundsLocked) => {
                    self.handler.handle_counterparty_funds_locked();
                }
                Ok(Event::ReadyToClaim) => {
                    self.handler.handle_ready_to_claim();
                }
                Ok(Event::CounterpartyFundsRefunded) => {
                    self.handler.handle_counterparty_funds_refunded();
                }
                Err(_) => {
                    break;
                }
            }
        }
    }
}
