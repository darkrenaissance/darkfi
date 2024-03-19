#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("counterparty keys channel closed")]
    CounterpartyKeysChannelClosed,
    #[error("listening to Claimed event stream failed")]
    ClaimedEventStreamFailed,
    #[error("timeout_1 is in the past")]
    Timeout1Passed,
    #[error("timeout_1 is too close to now")]
    Timeout1TooClose,
    #[error("timeout_2 is in the past")]
    Timeout2Passed,
    #[error("ERC20 not supported yet")]
    ERC20NotSupported,
    #[error("failed to submit `{0}` transaction: {1}")]
    FailedToSubmitTransaction(String, String),
    #[error("failed to await pending `{0}` transaction")]
    FailedToAwaitPendingTransaction(String, #[source] ethers::providers::ProviderError),
    #[error("no receipt received for transaction")]
    NoReceipt,
    #[error("`{0}` transaction failed: {1:?}")]
    TransactionFailed(String, ethers::types::TransactionReceipt),
    #[error("failed to decode log")]
    NewSwapLogDecodingFailed(#[source] ethers::abi::Error),
    #[error("expected exactly one log, got {0}")]
    NewSwapUnexpectedLogCount(usize),
    #[error("expected exactly one topic, got {0}")]
    NewSwapUnexpectedTopicCount(usize),
    #[error("expected five tokens, got {0}")]
    NewSwapUnexpectedLogTokenCount(usize),
    #[error("expected exactly 32 bytes, got {0}")]
    FixedBytesDecodingError(usize),
    #[error("expected FixedBytes, got another token type: {0}")]
    ExpectedFixedBytes(ethers::abi::Token),
    #[error("expected two U256s, got something else")]
    ExpectedTwoU256s,
}
