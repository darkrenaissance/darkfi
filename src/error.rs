/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

// Hello developer. Please add your error to the according subsection
// that is commented, or make a new subsection. Keep it clean.

/// Main result type used throughout the codebase.
pub type Result<T> = std::result::Result<T, Error>;

/// Result type used in the Client module
pub type ClientResult<T> = std::result::Result<T, ClientFailed>;

/// General library errors used throughout the codebase.
#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    // ==============
    // Parsing errors
    // ==============
    #[error("Parse failed: {0}")]
    ParseFailed(&'static str),

    #[error(transparent)]
    ParseIntError(#[from] std::num::ParseIntError),

    #[error(transparent)]
    ParseFloatError(#[from] std::num::ParseFloatError),

    #[cfg(feature = "url")]
    #[error(transparent)]
    UrlParseError(#[from] url::ParseError),

    #[error("URL parse error: {0}")]
    UrlParse(String),

    #[error(transparent)]
    AddrParseError(#[from] std::net::AddrParseError),

    #[error("Could not parse token parameter")]
    TokenParseError,

    #[error(transparent)]
    TryFromSliceError(#[from] std::array::TryFromSliceError),

    #[cfg(feature = "semver")]
    #[error("semver parse error: {0}")]
    SemverError(String),

    // ===============
    // Encoding errors
    // ===============
    #[error("decode failed: {0}")]
    DecodeError(&'static str),

    #[error("encode failed: {0}")]
    EncodeError(&'static str),

    #[error("VarInt was encoded in a non-minimal way")]
    NonMinimalVarInt,

    #[error(transparent)]
    Utf8Error(#[from] std::string::FromUtf8Error),

    #[error(transparent)]
    StrUtf8Error(#[from] std::str::Utf8Error),

    #[cfg(feature = "tinyjson")]
    #[error("JSON parse error: {0}")]
    JsonParseError(String),

    #[cfg(feature = "tinyjson")]
    #[error("JSON generate error: {0}")]
    JsonGenerateError(String),

    #[cfg(feature = "toml")]
    #[error(transparent)]
    TomlDeserializeError(#[from] toml::de::Error),

    #[cfg(feature = "bs58")]
    #[error(transparent)]
    Bs58DecodeError(#[from] bs58::decode::Error),

    #[cfg(feature = "hex")]
    #[error(transparent)]
    HexDecodeError(#[from] hex::FromHexError),

    #[error("Bad operation type byte")]
    BadOperationType,

    // ======================
    // Network-related errors
    // ======================
    #[error("Invalid Dialer scheme")]
    InvalidDialerScheme,

    #[error("Invalid Listener scheme")]
    InvalidListenerScheme,

    #[error("Unsupported network transport: {0}")]
    UnsupportedTransport(String),

    #[error("Unsupported network transport upgrade: {0}")]
    UnsupportedTransportUpgrade(String),

    #[error("Transport request exceeds number of accepted transports")]
    InvalidTransportRequest,

    #[error("Connection failed: {0}")]
    ConnectFailed(String),

    #[cfg(feature = "system")]
    #[error(transparent)]
    TimeoutError(#[from] crate::system::timeout::TimeoutError),

    #[error("Connection timed out")]
    ConnectTimeout,

    #[error("Channel stopped")]
    ChannelStopped,

    #[error("Channel timed out")]
    ChannelTimeout,

    #[error("Failed to reach any seeds")]
    SeedFailed,

    #[error("Network service stopped")]
    NetworkServiceStopped,

    #[error("Create listener bound to {0} failed")]
    BindFailed(String),

    #[error("Accept a new incoming connection from the listener {0} failed")]
    AcceptConnectionFailed(String),

    #[error("Accept a new tls connection from the listener {0} failed")]
    AcceptTlsConnectionFailed(String),

    #[error("Connector stopped: {0}")]
    ConnectorStopped(String),

    #[error("Network operation failed")]
    NetworkOperationFailed,

    #[error("Missing P2P message dispatcher")]
    MissingDispatcher,

    #[error("P2P message is invalid")]
    MessageInvalid,

    #[error("P2P message subsystem over metering limit")]
    MeteringLimitExceeded,

    #[cfg(feature = "arti-client")]
    #[error(transparent)]
    ArtiError(#[from] arti_client::Error),

    #[error("Malformed packet")]
    MalformedPacket,

    #[error("Error decoding packet: {0}")]
    DecodePacket(String),

    #[error("Socks proxy error: {0}")]
    SocksError(String),

    #[error("No Socks5 URL found")]
    NoSocks5UrlFound,

    #[error("No URL found")]
    NoUrlFound,

    #[error("Tor error: {0}")]
    TorError(String),

    #[error("Node is not connected to other nodes.")]
    NetworkNotConnected,

    #[error("P2P network stopped")]
    P2PNetworkStopped,

    #[error("No such host color exists")]
    InvalidHostColor,

    #[error("No matching hostlist entry")]
    HostDoesNotExist,

    #[error("Invalid state transition: current_state={0}, end_state={1}")]
    HostStateBlocked(String, String),

    // =============
    // Crypto errors
    // =============
    #[cfg(feature = "halo2_proofs")]
    #[error("halo2 plonk error: {0}")]
    PlonkError(String),

    #[error("Wrong witness type at index: {0}")]
    WrongWitnessType(usize),

    #[error("Wrong witnesses count")]
    WrongWitnessesCount,

    #[error("Wrong public inputs count")]
    WrongPublicInputsCount,

    #[error("Unable to decrypt mint note: {0}")]
    NoteDecryptionFailed(String),

    #[error("No keypair file detected")]
    KeypairPathNotFound,

    #[error("Failed converting bytes to PublicKey")]
    PublicKeyFromBytes,

    #[error("Failed converting bytes to Coin")]
    CoinFromBytes,

    #[error("Failed converting bytes to SecretKey")]
    SecretKeyFromBytes,

    #[error("Failed converting b58 string to PublicKey")]
    PublicKeyFromStr,

    #[error("Failed converting bs58 string to SecretKey")]
    SecretKeyFromStr,

    #[error("Invalid DarkFi address")]
    InvalidAddress,

    #[error("unable to decrypt rcpt")]
    TxRcptDecryptionError,

    #[cfg(feature = "blake3")]
    #[error(transparent)]
    Blake3FromHexError(#[from] blake3::HexError),

    // =======================
    // Protocol-related errors
    // =======================
    #[error("Unsupported chain")]
    UnsupportedChain,

    #[error("JSON-RPC error: {0:?}")]
    JsonRpcError((i32, String)),

    #[cfg(feature = "rpc")]
    #[error(transparent)]
    RpcServerError(RpcError),

    #[cfg(feature = "rpc")]
    #[error("JSON-RPC connections exhausted")]
    RpcConnectionsExhausted,

    #[cfg(feature = "rpc")]
    #[error("JSON-RPC server stopped")]
    RpcServerStopped,

    #[cfg(feature = "rpc")]
    #[error("JSON-RPC client stopped")]
    RpcClientStopped,

    #[error("Unexpected JSON-RPC data received: {0}")]
    UnexpectedJsonRpc(String),

    #[error("Received proposal from unknown node")]
    UnknownNodeError,

    #[error("Public inputs are invalid")]
    InvalidPublicInputsError,

    #[error("Signature could not be verified")]
    InvalidSignature,

    #[error("State transition failed")]
    StateTransitionError,

    #[error("No forks exist")]
    ForksNotFound,

    #[error("Check if proposal extends any existing fork chains failed")]
    ExtendedChainIndexNotFound,

    #[error("Proposal contains missmatched hashes")]
    ProposalHashesMissmatchError,

    #[error("Proposal contains missmatched headers")]
    ProposalHeadersMissmatchError,

    #[error("Unable to verify transfer transaction")]
    TransferTxVerification,

    #[error("Erroneous transactions detected")]
    ErroneousTxsDetected,

    #[error("Proposal task stopped")]
    ProposalTaskStopped,

    #[error("Proposal already exists")]
    ProposalAlreadyExists,

    #[error("Consensus task stopped")]
    ConsensusTaskStopped,

    #[error("Miner task stopped")]
    MinerTaskStopped,

    #[error("Garbage collection task stopped")]
    GarbageCollectionTaskStopped,

    #[error("Calculated total work is zero")]
    PoWTotalWorkIsZero,

    #[error("Erroneous cutoff calculation")]
    PoWCuttofCalculationError,

    #[error("Provided timestamp is invalid")]
    PoWInvalidTimestamp,

    #[error("Provided output hash is greater than current target")]
    PoWInvalidOutHash,

    #[error("Contracts states monotree root missing")]
    ContractsStatesRootNotFoundError,

    #[error("Contracts states monotree root missmatch: {0} - {1}")]
    ContractsStatesRootError(String, String),

    #[error("Hashing of Monero data failed: {0}")]
    MoneroHashingError(String),

    #[error("Cannot have zero merge-mining chains")]
    MoneroNumberOfChainZero,

    #[error("MergeMineError: {0}")]
    MoneroMergeMineError(String),

    #[cfg(feature = "randomx")]
    #[error("RandomX Error: {0}")]
    RandomXError(String),

    // ===============
    // Database errors
    // ===============
    #[error("Database error: {0}")]
    DatabaseError(String),

    #[cfg(feature = "sled-overlay")]
    #[error(transparent)]
    SledError(#[from] sled_overlay::sled::Error),

    #[cfg(feature = "sled-overlay")]
    #[error(transparent)]
    SledTransactionError(#[from] sled_overlay::sled::transaction::TransactionError),

    #[error("Transaction {0} not found in database")]
    TransactionNotFound(String),

    #[error("Transaction already seen")]
    TransactionAlreadySeen,

    #[error("Input vectors have different length")]
    InvalidInputLengths,

    #[error("Header {0} not found in database")]
    HeaderNotFound(String),

    #[error("Block {0} is invalid")]
    BlockIsInvalid(String),

    #[error("Block version {0} is invalid")]
    BlockVersionIsInvalid(u8),

    #[error("Block {0} already in database")]
    BlockAlreadyExists(String),

    #[error("Block {0} not found in database")]
    BlockNotFound(String),

    #[error("Block with height number {0} not found in database")]
    BlockHeightNotFound(u32),

    #[error("Block difficulty for height number {0} not found in database")]
    BlockDifficultyNotFound(u32),

    #[error("Block state inverse diff for height number {0} not found in database")]
    BlockStateInverseDiffNotFound(u32),

    #[error("Block {0} contains 0 transactions")]
    BlockContainsNoTransactions(String),

    #[error("Contract {0} not found in database")]
    ContractNotFound(String),

    #[error("Contract state tree not found")]
    ContractStateNotFound,

    #[error("Contract already initialized")]
    ContractAlreadyInitialized,

    #[error("zkas bincode not found in sled database")]
    ZkasBincodeNotFound,

    // ===================
    // wasm runtime errors
    // ===================
    #[cfg(feature = "wasm-runtime")]
    #[error("Wasmer compile error: {0}")]
    WasmerCompileError(String),

    #[cfg(feature = "wasm-runtime")]
    #[error("Wasmer export error: {0}")]
    WasmerExportError(String),

    #[cfg(feature = "wasm-runtime")]
    #[error("Wasmer runtime error: {0}")]
    WasmerRuntimeError(String),

    #[cfg(feature = "wasm-runtime")]
    #[error("Wasmer instantiation error: {0}")]
    WasmerInstantiationError(String),

    #[cfg(feature = "wasm-runtime")]
    #[error("wasm memory error")]
    WasmerMemoryError(String),

    #[cfg(feature = "wasm-runtime")]
    #[error("wasm runtime out of memory")]
    WasmerOomError(String),

    #[cfg(feature = "darkfi-sdk")]
    #[error("Contract execution failed: {0}")]
    ContractError(darkfi_sdk::error::ContractError),

    #[cfg(feature = "darkfi-sdk")]
    #[error("Invalid DarkTree: {0}")]
    DarkTreeError(darkfi_sdk::error::DarkTreeError),

    #[cfg(feature = "blockchain")]
    #[error("contract wasm bincode not found")]
    WasmBincodeNotFound,

    #[cfg(feature = "wasm-runtime")]
    #[error("contract initialize error")]
    ContractInitError(u64),

    #[cfg(feature = "wasm-runtime")]
    #[error("contract execution error")]
    ContractExecError(u64),

    #[cfg(feature = "wasm-runtime")]
    #[error("wasm function ACL denied")]
    WasmFunctionAclDenied,

    // ====================
    // Event Graph errors
    // ====================
    #[error("Event is not found in tree: {0}")]
    EventNotFound(String),

    #[error("Event is invalid")]
    EventIsInvalid,

    // ====================
    // Miscellaneous errors
    // ====================
    #[error("IO error: {0}")]
    Io(std::io::ErrorKind),

    #[error("Infallible error: {0}")]
    InfallibleError(String),

    #[cfg(feature = "smol")]
    #[error("async_channel sender error: {0}")]
    AsyncChannelSendError(String),

    #[cfg(feature = "smol")]
    #[error("async_channel receiver error: {0}")]
    AsyncChannelRecvError(String),

    #[cfg(feature = "util")]
    #[error("tracing-subscriber init failed: {0}")]
    LogInitError(String),

    #[error("ValueIsNotObject")]
    ValueIsNotObject,

    #[error("No config file detected")]
    ConfigNotFound,

    #[error("Invalid config file detected")]
    ConfigInvalid,

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Failed decoding bincode: {0}")]
    ZkasDecoderError(String),

    #[cfg(feature = "util")]
    #[error("System clock is not correct!")]
    InvalidClock,

    #[error("Unsupported OS")]
    UnsupportedOS,

    #[error("System clock went backwards")]
    BackwardsTime(std::time::SystemTimeError),

    #[error("Detached task stopped")]
    DetachedTaskStopped,

    #[error("Addition overflow")]
    AdditionOverflow,

    #[error("Subtraction underflow")]
    SubtractionUnderflow,

    // ==============================================
    // Wrappers for other error types in this library
    // ==============================================
    #[error(transparent)]
    ClientFailed(#[from] ClientFailed),

    #[cfg(feature = "tx")]
    #[error(transparent)]
    TxVerifyFailed(#[from] TxVerifyFailed),

    //=============
    // clock
    //=============
    #[error("clock out of sync with peers: {0}")]
    ClockOutOfSync(String),

    // ================
    // DHT/Geode errors
    // ================
    #[error("Geode needs garbage collection")]
    GeodeNeedsGc,

    #[error("Geode file not found")]
    GeodeFileNotFound,

    #[error("Geode chunk not found")]
    GeodeChunkNotFound,

    #[error("Geode file route not found")]
    GeodeFileRouteNotFound,

    #[error("Geode chunk route not found")]
    GeodeChunkRouteNotFound,

    // ==================
    // Event Graph errors
    // ==================
    #[error("DAG sync failed")]
    DagSyncFailed,

    #[error("Malicious flood detected")]
    MaliciousFlood,

    // =========
    // Catch-all
    // =========
    #[error("{0}")]
    Custom(String),
}

#[cfg(feature = "tx")]
impl Error {
    /// Auxiliary function to retrieve the vector of erroneous
    /// transactions from a TxVerifyFailed error.
    /// In any other case, we return the error itself.
    pub fn retrieve_erroneous_txs(&self) -> Result<Vec<crate::tx::Transaction>> {
        if let Self::TxVerifyFailed(TxVerifyFailed::ErroneousTxs(erroneous_txs)) = self {
            return Ok(erroneous_txs.clone())
        };

        Err(self.clone())
    }
}

#[cfg(feature = "tx")]
/// Transaction verification errors
#[derive(Debug, Clone, thiserror::Error)]
pub enum TxVerifyFailed {
    #[error("Transaction {0} already exists")]
    AlreadySeenTx(String),

    #[error("Invalid transaction signature")]
    InvalidSignature,

    #[error("Missing signatures in transaction")]
    MissingSignatures,

    #[error("Missing contract calls in transaction")]
    MissingCalls,

    #[error("Invalid ZK proof in transaction")]
    InvalidZkProof,

    #[error("Missing Money::Fee call in transaction")]
    MissingFee,

    #[error("Invalid Money::Fee call in transaction")]
    InvalidFee,

    #[error("Insufficient fee paid")]
    InsufficientFee,

    #[error("Erroneous transactions found")]
    ErroneousTxs(Vec<crate::tx::Transaction>),
}

/// Client module errors
#[derive(Debug, Clone, thiserror::Error)]
pub enum ClientFailed {
    #[error("IO error: {0}")]
    Io(std::io::ErrorKind),

    #[error("Not enough value: {0}")]
    NotEnoughValue(u64),

    #[error("Invalid address: {0}")]
    InvalidAddress(String),

    #[error("Invalid amount: {0}")]
    InvalidAmount(u64),

    #[error("Invalid token ID: {0}")]
    InvalidTokenId(String),

    #[error("Internal error: {0}")]
    InternalError(String),

    #[error("Verify error: {0}")]
    VerifyError(String),
}

#[cfg(feature = "rpc")]
#[derive(Clone, Debug, thiserror::Error)]
pub enum RpcError {
    #[error("Connection closed: {0}")]
    ConnectionClosed(String),

    #[error("Invalid JSON: {0}")]
    InvalidJson(String),

    #[error("IO Error: {0}")]
    IoError(std::io::ErrorKind),

    #[error("Method not found: {0}")]
    MethodNotFound(String),

    #[error("Server-side Error: {0}")]
    ServerError(#[from] std::sync::Arc<dyn std::error::Error + Send + Sync + 'static>),
}

#[cfg(feature = "rpc")]
impl From<std::io::Error> for RpcError {
    fn from(err: std::io::Error) -> Self {
        Self::IoError(err.kind())
    }
}

#[cfg(feature = "rpc")]
impl From<RpcError> for Error {
    fn from(err: RpcError) -> Self {
        Self::RpcServerError(err)
    }
}

impl From<Error> for ClientFailed {
    fn from(err: Error) -> Self {
        Self::InternalError(err.to_string())
    }
}

impl From<std::io::Error> for ClientFailed {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err.kind())
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err.kind())
    }
}

impl From<std::time::SystemTimeError> for Error {
    fn from(err: std::time::SystemTimeError) -> Self {
        Self::BackwardsTime(err)
    }
}

impl From<std::convert::Infallible> for Error {
    fn from(err: std::convert::Infallible) -> Self {
        Self::InfallibleError(err.to_string())
    }
}

impl From<()> for Error {
    fn from(_err: ()) -> Self {
        Self::InfallibleError("Infallible".into())
    }
}

#[cfg(feature = "net")]
impl From<std::collections::TryReserveError> for Error {
    fn from(err: std::collections::TryReserveError) -> Self {
        Self::DecodePacket(err.to_string())
    }
}
#[cfg(feature = "smol")]
impl<T> From<smol::channel::SendError<T>> for Error {
    fn from(err: smol::channel::SendError<T>) -> Self {
        Self::AsyncChannelSendError(err.to_string())
    }
}

#[cfg(feature = "smol")]
impl From<smol::channel::RecvError> for Error {
    fn from(err: smol::channel::RecvError) -> Self {
        Self::AsyncChannelRecvError(err.to_string())
    }
}

#[cfg(feature = "halo2_proofs")]
impl From<halo2_proofs::plonk::Error> for Error {
    fn from(err: halo2_proofs::plonk::Error) -> Self {
        Self::PlonkError(err.to_string())
    }
}

#[cfg(feature = "semver")]
impl From<semver::Error> for Error {
    fn from(err: semver::Error) -> Self {
        Self::SemverError(err.to_string())
    }
}

#[cfg(feature = "tinyjson")]
impl From<tinyjson::JsonParseError> for Error {
    fn from(err: tinyjson::JsonParseError) -> Self {
        Self::JsonParseError(err.to_string())
    }
}

#[cfg(feature = "tinyjson")]
impl From<tinyjson::JsonGenerateError> for Error {
    fn from(err: tinyjson::JsonGenerateError) -> Self {
        Self::JsonGenerateError(err.to_string())
    }
}

#[cfg(feature = "wasm-runtime")]
impl From<wasmer::CompileError> for Error {
    fn from(err: wasmer::CompileError) -> Self {
        Self::WasmerCompileError(err.to_string())
    }
}

#[cfg(feature = "wasm-runtime")]
impl From<wasmer::ExportError> for Error {
    fn from(err: wasmer::ExportError) -> Self {
        Self::WasmerExportError(err.to_string())
    }
}

#[cfg(feature = "wasm-runtime")]
impl From<wasmer::RuntimeError> for Error {
    fn from(err: wasmer::RuntimeError) -> Self {
        Self::WasmerRuntimeError(err.to_string())
    }
}

#[cfg(feature = "wasm-runtime")]
impl From<wasmer::InstantiationError> for Error {
    fn from(err: wasmer::InstantiationError) -> Self {
        Self::WasmerInstantiationError(err.to_string())
    }
}

#[cfg(feature = "wasm-runtime")]
impl From<wasmer::MemoryAccessError> for Error {
    fn from(err: wasmer::MemoryAccessError) -> Self {
        Self::WasmerMemoryError(err.to_string())
    }
}

#[cfg(feature = "wasm-runtime")]
impl From<wasmer::MemoryError> for Error {
    fn from(err: wasmer::MemoryError) -> Self {
        Self::WasmerOomError(err.to_string())
    }
}

#[cfg(feature = "darkfi-sdk")]
impl From<darkfi_sdk::error::ContractError> for Error {
    fn from(err: darkfi_sdk::error::ContractError) -> Self {
        Self::ContractError(err)
    }
}

#[cfg(feature = "darkfi-sdk")]
impl From<darkfi_sdk::error::DarkTreeError> for Error {
    fn from(err: darkfi_sdk::error::DarkTreeError) -> Self {
        Self::DarkTreeError(err)
    }
}

#[cfg(feature = "util")]
impl From<tracing_subscriber::util::TryInitError> for Error {
    fn from(err: tracing_subscriber::util::TryInitError) -> Self {
        Self::LogInitError(err.to_string())
    }
}

#[cfg(feature = "randomx")]
impl From<randomx::RandomXError> for Error {
    fn from(err: randomx::RandomXError) -> Self {
        Self::RandomXError(err.to_string())
    }
}
