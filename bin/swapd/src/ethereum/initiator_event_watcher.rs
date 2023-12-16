use std::time::{SystemTime, UNIX_EPOCH};

use crate::{
    ethereum::swap_creator::SwapCreator,
    protocol::{initiator::Event, traits::InitiateSwapArgs},
};
use ethers::prelude::Middleware;
use eyre::{eyre, Result};
use smol::stream::StreamExt as _;
use tokio::sync::mpsc::Sender;

struct Watcher<M: Middleware> {
    event_tx: Sender<Event>,
    contract: SwapCreator<M>,
}

impl<M: Middleware> Watcher<M> {
    fn new(event_tx: Sender<Event>, contract: SwapCreator<M>) -> Self {
        Self { event_tx, contract }
    }

    async fn run_received_counterparty_keys_watcher(
        &mut self,
        args: InitiateSwapArgs,
    ) -> Result<()> {
        // TODO: from p2p
        self.event_tx.send(Event::ReceivedCounterpartyKeys(args)).await.unwrap();
        Ok(())
    }

    async fn run_counterparty_funds_locked_watcher(&mut self) -> Result<()> {
        self.event_tx.send(Event::CounterpartyFundsLocked).await.unwrap();
        Ok(())
    }

    async fn run_counterparty_funds_claimed_watcher(&mut self) -> Result<()> {
        // TODO filter for swap ID also
        let events =
            self.contract.event::<crate::ethereum::swap_creator::ClaimedFilter>().from_block(1);
        let mut stream = events.stream().await.unwrap().with_meta();

        let Some(Ok((event, _meta))) = stream.next().await else {
            panic!("listening to Claimed event stream failed");
        };

        self.event_tx.send(Event::CounterpartyFundsClaimed(event.s)).await.unwrap();
        Ok(())
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

    async fn run_timeout_2_watcher(&mut self, timeout_2: u64) -> Result<()> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let diff = timeout_2.checked_sub(now).ok_or(eyre!("timeout_2 is in the past"))?;
        let sleep_duration = tokio::time::Duration::from_secs(diff);

        tokio::time::sleep(sleep_duration).await;
        self.event_tx.send(Event::PastTimeout2).await.unwrap();
        Ok(())
    }
}
