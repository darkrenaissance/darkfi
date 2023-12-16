use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::protocol::{initiator::Event, traits::InitiateSwapArgs};
use eyre::{eyre, Result};
use tokio::sync::mpsc::Sender;

struct Watcher {
    event_tx: Sender<Event>,
}

impl Watcher {
    fn new(event_tx: Sender<Event>) -> Self {
        Self { event_tx }
    }

    fn handle_counterparty_keys_received(&mut self, args: InitiateSwapArgs) {
        self.event_tx.try_send(Event::ReceivedCounterpartyKeys(args)).unwrap();
    }

    fn handle_counterparty_funds_locked(&mut self) {
        self.event_tx.try_send(Event::CounterpartyFundsLocked).unwrap();
    }

    fn handle_counterparty_funds_claimed(&mut self, counterparty_secret: [u8; 32]) {
        self.event_tx.try_send(Event::CounterpartyFundsClaimed(counterparty_secret)).unwrap();
    }

    fn handle_almost_timeout1(&mut self) {
        self.event_tx.try_send(Event::AlmostTimeout1).unwrap();
    }

    fn handle_past_timeout2(&mut self) {
        self.event_tx.try_send(Event::PastTimeout2).unwrap();
    }

    async fn run_timeout_1_watcher(&mut self, timeout_1: u64, buffer_seconds: u64) -> Result<()> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let diff = timeout_1
            .checked_sub(now)
            .ok_or(eyre!("timeout_1 is in the past"))?
            .checked_sub(buffer_seconds)
            .ok_or(eyre!("timeout_1 is too close to now"))?;
        let sleep_duration = tokio::time::Duration::from_secs(diff);

        tokio::time::sleep(sleep_duration).await;
        self.event_tx.send(Event::AlmostTimeout1).await.unwrap();
        Ok(())
    }
}
