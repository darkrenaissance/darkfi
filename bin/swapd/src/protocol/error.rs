use crate::protocol::traits::CounterpartyKeys;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("unexpected received counterparty keys event: {0}")]
    UnexpectedReceivedCounterpartyKeysEvent(CounterpartyKeys),
    #[error("unexpected counterparty funds locked event")]
    UnexpectedCounterpartyFundsLockedEvent,
    #[error("unexpected counterparty funds claimed event")]
    UnexpectedCounterpartyFundsClaimedEvent([u8; 32]),
    #[error("unexpected almost timeout 1 event")]
    UnexpectedAlmostTimeout1Event,
    #[error("unexpected past timeout 2 event")]
    UnexpectedPastTimeout2Event,
}
