pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Debug, thiserror::Error)]
pub enum Error {
    #[error("io error: `{0:?}`")]
    Io(std::io::ErrorKind),

    #[error("Infallible Error: `{0}`")]
    InfallibleError(String),

    #[cfg(feature = "util")]
    #[error("VarInt was encoded in a non-minimal way")]
    NonMinimalVarInt,

    #[cfg(feature = "util")]
    #[error("parse failed: `{0}`")]
    ParseFailed(&'static str),

    #[error("decode failed: `{0}`")]
    DecodeError(&'static str),

    #[error("encode failed: `{0}`")]
    EncodeError(&'static str),

    #[error(transparent)]
    ParseIntError(#[from] std::num::ParseIntError),

    #[cfg(feature = "util")]
    #[error(transparent)]
    ParseBigIntError(#[from] num_bigint::ParseBigIntError),

    #[cfg(any(feature = "rpc", feature = "node"))]
    #[error("Url parse error `{0}`")]
    UrlParseError(String),

    #[cfg(any(feature = "rpc"))]
    #[error("Socks error `{0}`")]
    SocksError(String),

    #[error("No url found")]
    NoUrlFound,

    #[error("No socks5 url found")]
    NoSocks5UrlFound,

    #[error(transparent)]
    AddrParseError(#[from] std::net::AddrParseError),

    #[error(transparent)]
    Utf8Error(#[from] std::string::FromUtf8Error),

    #[error(transparent)]
    StrUtf8Error(#[from] std::str::Utf8Error),

    #[error("TryFrom error")]
    TryFromError,

    #[cfg(feature = "util")]
    #[error(transparent)]
    TryFromBigIntError(#[from] num_bigint::TryFromBigIntError<num_bigint::BigUint>),

    #[cfg(feature = "util")]
    #[error("Json serialization error: `{0}`")]
    SerdeJsonError(String),

    #[cfg(feature = "util")]
    #[error(transparent)]
    TomlDeserializeError(#[from] toml::de::Error),

    #[cfg(feature = "util")]
    #[error("Bincode serialization error: `{0}`")]
    BincodeError(String),

    #[error("Bad operation type byte")]
    BadOperationType,

    #[error("Invalid param name")]
    InvalidParamName,

    #[error("Invalid param type")]
    InvalidParamType,

    #[error("Missing params")]
    MissingParams,

    #[cfg(feature = "crypto")]
    #[error("Plonk error: `{0}`")]
    PlonkError(String),

    #[cfg(feature = "crypto")]
    #[error("Unable to decrypt mint note")]
    NoteDecryptionFailed,

    #[cfg(feature = "node")]
    #[error(transparent)]
    VerifyFailed(#[from] crate::node::state::VerifyFailed),

    #[error("Services Error: `{0}`")]
    ServicesError(&'static str),

    #[error("Client failed: `{0}`")]
    ClientFailed(String),

    #[error("Cashier failed: `{0}`")]
    CashierError(String),

    #[error("ZmqError: `{0}`")]
    ZmqError(String),

    #[cfg(feature = "blockchain")]
    #[error("Rocksdb error: `{0}`")]
    RocksdbError(String),

    #[cfg(feature = "node")]
    #[error("sqlx error: `{0}`")]
    SqlxError(String),

    #[cfg(feature = "node")]
    #[error("SlabsStore Error: `{0}`")]
    SlabsStore(String),

    #[error("JsonRpc Error: `{0}`")]
    JsonRpcError(String),

    #[error("Not supported network")]
    NotSupportedNetwork,

    #[error("Not supported token")]
    NotSupportedToken,

    #[error("Could not parse token parameter")]
    TokenParseError,

    #[cfg(feature = "async-net")]
    #[error("Async_Native_TLS error: `{0}`")]
    AsyncNativeTlsError(String),

    #[cfg(feature = "websockets")]
    #[error("TungsteniteError: `{0}`")]
    TungsteniteError(String),

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

    #[error("Operation failed")]
    OperationFailed,

    #[error("Malformed packet")]
    MalformedPacket,

    #[error("No config file detected. Please create one.")]
    ConfigNotFound,

    #[cfg(feature = "util")]
    #[error("No keypair file detected.")]
    KeypairPathNotFound,

    #[error("No cashier public keys detected.")]
    CashierKeysNotFound,

    #[error("SetLoggerError")]
    SetLoggerError,

    #[cfg(feature = "async-runtime")]
    #[error("Async_channel sender error")]
    AsyncChannelSenderError,

    #[cfg(feature = "async-runtime")]
    #[error(transparent)]
    AsyncChannelReceiverError(#[from] async_channel::RecvError),

    #[cfg(feature = "crypto")]
    #[error("Error converting bytes to PublicKey")]
    PublicKeyFromBytes,

    #[cfg(feature = "crypto")]
    #[error("Error converting bytes to SecretKey")]
    SecretKeyFromBytes,

    #[cfg(feature = "crypto")]
    #[error("Invalid Address")]
    InvalidAddress,

    #[error("Invalid bincode: {0}")]
    ZkasDecoderError(&'static str),

    #[cfg(feature = "wasm-runtime")]
    #[error("wasm export error: {0}")]
    WasmerExportError(String),

    #[cfg(feature = "wasm-runtime")]
    #[error("wasm runtime error: {0}")]
    WasmerRuntimeError(String),

    #[cfg(feature = "wasm-runtime")]
    #[error("wasm instantiation error: {0}")]
    WasmerInstantiationError(String),

    #[cfg(feature = "wasm-runtime")]
    #[error("wasm compile error: {0}")]
    WasmerCompileError(String),

    #[cfg(feature = "wasm-runtime")]
    #[error("wasm runtime out of memory")]
    WasmerOomError,
}

#[cfg(feature = "node")]
impl From<zeromq::ZmqError> for Error {
    fn from(err: zeromq::ZmqError) -> Error {
        Error::ZmqError(err.to_string())
    }
}

#[cfg(feature = "blockchain")]
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

#[cfg(feature = "util")]
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

#[cfg(feature = "async-runtime")]
impl<T> From<async_channel::SendError<T>> for Error {
    fn from(_err: async_channel::SendError<T>) -> Error {
        Error::AsyncChannelSenderError
    }
}

#[cfg(feature = "async-net")]
impl From<async_native_tls::Error> for Error {
    fn from(err: async_native_tls::Error) -> Error {
        Error::AsyncNativeTlsError(err.to_string())
    }
}

#[cfg(feature = "rpc")]
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

#[cfg(feature = "websockets")]
impl From<tungstenite::Error> for Error {
    fn from(err: tungstenite::Error) -> Error {
        Error::TungsteniteError(err.to_string())
    }
}

#[cfg(feature = "util")]
impl From<Box<bincode::ErrorKind>> for Error {
    fn from(err: Box<bincode::ErrorKind>) -> Error {
        Error::BincodeError(err.to_string())
    }
}

#[cfg(feature = "rpc")]
impl From<fast_socks5::SocksError> for Error {
    fn from(err: fast_socks5::SocksError) -> Error {
        Error::SocksError(err.to_string())
    }
}

impl From<std::convert::Infallible> for Error {
    fn from(err: std::convert::Infallible) -> Error {
        Error::InfallibleError(err.to_string())
    }
}

#[cfg(feature = "crypto")]
impl From<halo2_proofs::plonk::Error> for Error {
    fn from(err: halo2_proofs::plonk::Error) -> Error {
        Error::PlonkError(err.to_string())
    }
}

#[cfg(feature = "wasm-runtime")]
impl From<wasmer::CompileError> for Error {
    fn from(err: wasmer::CompileError) -> Error {
        Error::WasmerCompileError(err.to_string())
    }
}

#[cfg(feature = "wasm-runtime")]
impl From<wasmer::ExportError> for Error {
    fn from(err: wasmer::ExportError) -> Error {
        Error::WasmerExportError(err.to_string())
    }
}

#[cfg(feature = "wasm-runtime")]
impl From<wasmer::RuntimeError> for Error {
    fn from(err: wasmer::RuntimeError) -> Error {
        Error::WasmerRuntimeError(err.to_string())
    }
}

#[cfg(feature = "wasm-runtime")]
impl From<wasmer::InstantiationError> for Error {
    fn from(err: wasmer::InstantiationError) -> Error {
        Error::WasmerInstantiationError(err.to_string())
    }
}
