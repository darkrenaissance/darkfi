pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    #[error("io error: `{0:?}`")]
    Io(std::io::ErrorKind),

    #[error("Infallible Error: `{0}`")]
    InfallibleError(String),

    #[error("Cannot find home directory")]
    PathNotFound,
    /// VarInt was encoded in a non-minimal way
    #[error("non-minimal varint")]
    NonMinimalVarInt,

    /// Parsing And Encode/Decode errors
    #[error("parse failed: `{0}`")]
    ParseFailed(&'static str),
    #[error(transparent)]
    ParseIntError(#[from] std::num::ParseIntError),
    #[error(transparent)]
    ParseBigIntError(#[from] num_bigint::ParseBigIntError),
    #[error(transparent)]
    ParseFloatError(#[from] std::num::ParseFloatError),
    #[error(transparent)]
    FromHexError(#[from] hex::FromHexError),
    #[error("Url parse error `{0}`")]
    UrlParseError(String),
    #[error("No url found")]
    NoUrlFound,
    #[error("Malformed packet")]
    MalformedPacket,
    #[error(transparent)]
    AddrParseError(#[from] std::net::AddrParseError),
    #[error(transparent)]
    Base58EncodeError(#[from] bs58::encode::Error),
    #[error(transparent)]
    Base58DecodeError(#[from] bs58::decode::Error),
    #[error(transparent)]
    Utf8Error(#[from] std::string::FromUtf8Error),
    #[error(transparent)]
    StrUtf8Error(#[from] std::str::Utf8Error),
    #[error("TryInto error")]
    TryIntoError,
    #[error("TryFrom error")]
    TryFromError,
    #[error(transparent)]
    TryFromBigIntError(#[from] num_bigint::TryFromBigIntError<num_bigint::BigUint>),
    #[error("Json serialization error: `{0}`")]
    SerdeJsonError(String),
    #[error(transparent)]
    TomlDeserializeError(#[from] toml::de::Error),
    #[error(transparent)]
    TomlSerializeError(#[from] toml::ser::Error),
    #[error("Bincode serialization error: `{0}`")]
    BincodeError(String),

    /// Contract
    #[error("Bad variable ref type byte")]
    BadVariableRefType,
    #[error("Bad operation type byte")]
    BadOperationType,
    #[error("Bad constraint type byte")]
    BadConstraintType,
    #[error("Invalid param name")]
    InvalidParamName,
    #[error("Invalid param type")]
    InvalidParamType,
    #[error("Missing params")]
    MissingParams,
    #[error("Contract is poorly defined")]
    BadContract,
    #[error("Operation failed")]
    OperationFailed,
    #[error("PLONK error: `{0}`")]
    PlonkError(String),
    #[error("Unable to decrypt mint note")]
    NoteDecryptionFailed,

    #[cfg(feature = "node")]
    #[error(transparent)]
    VerifyFailed(#[from] crate::node::state::VerifyFailed),
    #[error("MerkleTree is full")]
    TreeFull,

    /// Service
    #[error("Services Error: `{0}`")]
    ServicesError(&'static str),
    #[error("Client failed: `{0}`")]
    ClientFailed(String),
    #[cfg(feature = "btc")]
    #[error(transparent)]
    BtcFailed(#[from] crate::service::BtcFailed),
    #[cfg(feature = "sol")]
    #[error("Sol client failed: `{0}`")]
    SolFailed(String),
    #[cfg(feature = "eth")]
    #[error(transparent)]
    EthFailed(#[from] crate::service::EthFailed),
    #[error("BridgeError Error: `{0}`")]
    BridgeError(String),
    #[error("ZmqError: `{0}`")]
    ZmqError(String),

    /// Database/Sql errors
    #[error("Rocksdb error: `{0}`")]
    RocksdbError(String),
    #[error("sqlx error: `{0}`")]
    SqlxError(String),
    #[error("SlabsStore Error: `{0}`")]
    SlabsStore(String),

    /// RPC errors
    #[error("JsonRpc Error: `{0}`")]
    JsonRpcError(String),
    #[error("Not supported network")]
    NotSupportedNetwork,
    #[error("Not supported token")]
    NotSupportedToken,
    #[error("Could not parse token parameter")]
    TokenParseError,
    #[error("Cannot parse network parameter")]
    NetworkParseError,
    #[error("Async_Native_TLS error: `{0}`")]
    AsyncNativeTlsError(String),
    #[error("TungsteniteError: `{0}`")]
    TungsteniteError(String),

    /// Network
    #[error("Connection failed")]
    ConnectFailed,
    #[error("Connection timed out")]
    ConnectTimeout,
    #[error("Channel stopped")]
    ChannelStopped,
    #[error("Channel timed out")]
    ChannelTimeout,
    #[error("Service stopped")]
    ServiceStopped,

    /// Util
    #[error("No config file detected. Please create one.")]
    ConfigNotFound,
    #[error("No keypair file detected.")]
    KeypairPathNotFound,
    #[error("No cashier public keys detected.")]
    CashierKeysNotFound,
    #[error("SetLoggerError")]
    SetLoggerError,
    #[error("Async_channel sender error")]
    AsyncChannelSenderError,
    #[error(transparent)]
    AsyncChannelReceiverError(#[from] async_channel::RecvError),

    /// Keypari & Address
    #[error("Error converting Address to PublicKey")]
    AddressToPublicKeyError,
    #[error("Error converting bytes to PublicKey")]
    PublicKeyFromBytes,
    #[error("Error converting bytes to SecretKey")]
    SecretKeyFromBytes,
    #[error("Invalid Address")]
    InvalidAddress,
}

#[cfg(feature = "node")]
impl From<zeromq::ZmqError> for Error {
    fn from(err: zeromq::ZmqError) -> Error {
        Error::ZmqError(err.to_string())
    }
}

#[cfg(feature = "chain")]
impl From<rocksdb::Error> for Error {
    fn from(err: rocksdb::Error) -> Error {
        Error::RocksdbError(err.to_string())
    }
}

#[cfg(feature = "node")]
impl From<sqlx::error::Error> for Error {
    fn from(err: sqlx::error::Error) -> Error {
        Error::SqlxError(err.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Error {
        Error::SerdeJsonError(err.to_string())
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Error {
        Error::Io(err.kind())
    }
}

impl<T> From<async_channel::SendError<T>> for Error {
    fn from(_err: async_channel::SendError<T>) -> Error {
        Error::AsyncChannelSenderError
    }
}

impl From<async_native_tls::Error> for Error {
    fn from(err: async_native_tls::Error) -> Error {
        Error::AsyncNativeTlsError(err.to_string())
    }
}

impl From<url::ParseError> for Error {
    fn from(err: url::ParseError) -> Error {
        Error::UrlParseError(err.to_string())
    }
}

#[cfg(feature = "node")]
impl From<crate::node::client::ClientFailed> for Error {
    fn from(err: crate::node::client::ClientFailed) -> Error {
        Error::ClientFailed(err.to_string())
    }
}

impl From<log::SetLoggerError> for Error {
    fn from(_err: log::SetLoggerError) -> Error {
        Error::SetLoggerError
    }
}

impl From<tungstenite::Error> for Error {
    fn from(err: tungstenite::Error) -> Error {
        Error::TungsteniteError(err.to_string())
    }
}

#[cfg(feature = "sol")]
impl From<crate::service::SolFailed> for Error {
    fn from(err: crate::service::SolFailed) -> Error {
        Error::SolFailed(err.to_string())
    }
}

impl From<halo2::plonk::Error> for Error {
    fn from(err: halo2::plonk::Error) -> Error {
        Error::PlonkError(format!("{:?}", err))
    }
}

impl From<Box<bincode::ErrorKind>> for Error {
    fn from(err: Box<bincode::ErrorKind>) -> Error {
        Error::BincodeError(err.to_string())
    }
}

impl From<std::convert::Infallible> for Error {
    fn from(err: std::convert::Infallible) -> Error {
        Error::InfallibleError(err.to_string())
    }
}
