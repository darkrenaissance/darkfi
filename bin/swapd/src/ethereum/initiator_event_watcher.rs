use std::time::{SystemTime, UNIX_EPOCH};

use crate::{
    ethereum::swap_creator::SwapCreator,
    protocol::{
        initiator::Event,
        traits::{CounterpartyKeys, InitiatorEventWatcher},
    },
};
use ethers::prelude::Middleware;
use eyre::{eyre, Result};
use smol::{channel, stream::StreamExt as _};

pub(crate) struct Watcher;

#[darkfi_serial::async_trait]
impl InitiatorEventWatcher for Watcher {
    async fn run_received_counterparty_keys_watcher(
        event_tx: channel::Sender<Event>,
        counterparty_keys_rx: async_oneshot::Receiver<CounterpartyKeys>,
    ) -> Result<()> {
        let counterparty_keys =
            counterparty_keys_rx.await.map_err(|_| eyre!("counterparty_keys channel closed"))?;
        event_tx.send(Event::ReceivedCounterpartyKeys(counterparty_keys)).await.unwrap();
        Ok(())
    }

    async fn run_counterparty_funds_locked_watcher(event_tx: channel::Sender<Event>) -> Result<()> {
        // TODO: from watching counterchain swap wallet
        event_tx.send(Event::CounterpartyFundsLocked).await.unwrap();
        Ok(())
    }

    async fn run_counterparty_funds_claimed_watcher<M: Middleware>(
        event_tx: channel::Sender<Event>,
        contract: SwapCreator<M>,
        contract_swap_id: &[u8; 32],
        from_block: u64,
    ) -> Result<()> {
        let topic1: ethers::types::U256 = contract_swap_id.into();
        let events = contract
            .claimed_filter() // claimed event sig is topic0
            .from_block(from_block)
            .address(contract.address().into())
            .topic1(topic1);

        let mut stream = events.stream().await.unwrap().with_meta();

        // we listen for the first event, as there can only be one event
        // that matches the filter (ie. has the same swap_id)
        let Some(Ok((event, _meta))) = stream.next().await else {
            eyre::bail!("listening to Claimed event stream failed")
        };

        event_tx.send(Event::CounterpartyFundsClaimed(event.s)).await.unwrap();
        Ok(())
    }

    async fn run_timeout_1_watcher(
        event_tx: channel::Sender<Event>,
        timeout_1: u64,
        buffer_seconds: u64,
    ) -> Result<()> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let diff = timeout_1
            .checked_sub(now)
            .ok_or(eyre!("timeout_1 is in the past"))?
            .checked_sub(buffer_seconds)
            .ok_or(eyre!("timeout_1 is too close to now"))?;
        let sleep_duration = std::time::Duration::from_secs(diff);

        smol::Timer::after(sleep_duration).await;
        event_tx.send(Event::AlmostTimeout1).await.unwrap();
        Ok(())
    }

    async fn run_timeout_2_watcher(event_tx: channel::Sender<Event>, timeout_2: u64) -> Result<()> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let diff = timeout_2.checked_sub(now).ok_or(eyre!("timeout_2 is in the past"))?;
        let sleep_duration = std::time::Duration::from_secs(diff);

        smol::Timer::after(sleep_duration).await;
        event_tx.send(Event::PastTimeout2).await.unwrap();
        Ok(())
    }
}
