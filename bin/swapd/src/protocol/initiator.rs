use crate::protocol::traits::{ChainA, ChainB};
use tokio::sync::mpsc::{channel, Receiver, Sender};

enum Event {
    ReceivedCounterpartyKeys,
    CounterpartyFundsLocked,
    CounterpartyFundsClaimed,
    ShouldRefund,
}

struct Swap {
    chain_a: Box<dyn ChainA>,
    chain_b: Box<dyn ChainB>,
    event_rx: Receiver<Event>,
}

impl Swap {
    fn new(chain_a: Box<dyn ChainA>, chain_b: Box<dyn ChainB>, event_rx: Receiver<Event>) -> Self {
        Self { chain_a, chain_b, event_rx }
    }

    fn run(&mut self) {
        loop {}
    }
}
