use jsonrpc_core::*;
use std::fmt;

use crate::client;
use crate::state;
use crate::vm::ZkVmError;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone)]
pub enum Error {
    Io(std::io::ErrorKind),
    /// VarInt was encoded in a non-minimal way
    PathNotFound,
    NonMinimalVarInt,
    /// Parsing error
    ParseFailed(&'static str),
    ParseIntError,
    ParseFloatError,
    AsyncChannelSenderError,
    AsyncChannelReceiverError,
    MalformedPacket,
    AddrParseError,
    BadVariableRefType,
    BadOperationType,
    BadConstraintType,
    InvalidParamName,
    MissingParams,
    VmError,
    BadContract,
    Groth16Error,
    RusqliteError(String),
    OperationFailed,
    ConnectFailed,
    ConnectTimeout,
    ChannelStopped,
    ChannelTimeout,
    ServiceStopped,
    Utf8Error,
    StrUtf8Error(String),
    NoteDecryptionFailed,
    ServicesError(&'static str),
    ZmqError(String),
    VerifyFailed,
    ClientFailed(String),
    #[cfg(feature = "btc")]
    BtcFailed(String),
    #[cfg(feature = "sol")]
    SolFailed(String),
    TryIntoError,
    TryFromError,
    JsonRpcError(String),
    RocksdbError(String),
    TreeFull,
    BridgeError(String),
    SerdeJsonError(String),
    SurfHttpError(String),
    TomlDeserializeError(String),
    TomlSerializeError(String),
    CashierNoReply,
    Base58EncodeError(String),
    Base58DecodeError(String),
    ConfigNotFound,
}

impl std::error::Error for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        match *self {
            Error::PathNotFound => f.write_str("Cannot find home directory"),
            Error::Io(ref err) => write!(f, "io error:{:?}", err),
            Error::NonMinimalVarInt => f.write_str("non-minimal varint"),
            Error::ParseFailed(ref err) => write!(f, "parse failed: {}", err),
            Error::ParseIntError => f.write_str("Parse int error"),
            Error::ParseFloatError => f.write_str("Parse float error"),
            Error::AsyncChannelSenderError => f.write_str("Async_channel sender error"),
            Error::AsyncChannelReceiverError => f.write_str("Async_channel receiver error"),
            Error::MalformedPacket => f.write_str("Malformed packet"),
            Error::AddrParseError => f.write_str("Unable to parse address"),
            Error::BadVariableRefType => f.write_str("Bad variable ref type byte"),
            Error::BadOperationType => f.write_str("Bad operation type byte"),
            Error::BadConstraintType => f.write_str("Bad constraint type byte"),
            Error::InvalidParamName => f.write_str("Invalid param name"),
            Error::MissingParams => f.write_str("Missing params"),
            Error::VmError => f.write_str("VM error"),
            Error::BadContract => f.write_str("Contract is poorly defined"),
            Error::Groth16Error => f.write_str("Groth16 error"),
            Error::RusqliteError(ref err) => write!(f, "Rusqlite error {}", err),
            Error::OperationFailed => f.write_str("Operation failed"),
            Error::ConnectFailed => f.write_str("Connection failed"),
            Error::ConnectTimeout => f.write_str("Connection timed out"),
            Error::ChannelStopped => f.write_str("Channel stopped"),
            Error::ChannelTimeout => f.write_str("Channel timed out"),
            Error::ServiceStopped => f.write_str("Service stopped"),
            Error::Utf8Error => f.write_str("Malformed UTF8"),
            Error::StrUtf8Error(ref err) => write!(f, "Malformed UTF8: {}", err),
            Error::NoteDecryptionFailed => f.write_str("Unable to decrypt mint note"),
            Error::ServicesError(ref err) => write!(f, "Services error: {}", err),
            Error::ZmqError(ref err) => write!(f, "ZmqError: {}", err),
            Error::VerifyFailed => f.write_str("Verify failed"),
            Error::ClientFailed(ref err) => write!(f, "Client failed: {}", err),
            #[cfg(feature = "btc")]
            Error::BtcFailed(ref err) => write!(f, "Btc client failed: {}", err),
            #[cfg(feature = "sol")]
            Error::SolFailed(ref err) => write!(f, "Sol client failed: {}", err),
            Error::TryIntoError => f.write_str("TryInto error"),
            Error::TryFromError => f.write_str("TryFrom error"),
            Error::RocksdbError(ref err) => write!(f, "Rocksdb Error: {}", err),
            Error::JsonRpcError(ref err) => write!(f, "JsonRpc Error: {}", err),
            Error::TreeFull => f.write_str("MerkleTree is full"),
            Error::BridgeError(ref err) => write!(f, "Bridge error: {}", err),
            Error::SerdeJsonError(ref err) => write!(f, "Json serialization error: {}", err),
            Error::SurfHttpError(ref err) => write!(f, "Surf Http error: {}", err),
            Error::TomlDeserializeError(ref err) => write!(f, "Toml parsing error: {}", err),
            Error::TomlSerializeError(ref err) => write!(f, "Toml parsing error: {}", err),
            Error::Base58EncodeError(ref err) => write!(f, "bs58 encode error: {}", err),
            Error::Base58DecodeError(ref err) => write!(f, "bs58 decode error: {}", err),
            Error::CashierNoReply => f.write_str("Cashier did not reply with BTC address"),
            Error::ConfigNotFound => {
                f.write_str("No config file detected. Please create a config file")
            }
        }
    }
}

impl From<zeromq::ZmqError> for Error {
    fn from(err: zeromq::ZmqError) -> Error {
        Error::ZmqError(err.to_string())
    }
}

impl From<rocksdb::Error> for Error {
    fn from(err: rocksdb::Error) -> Error {
        Error::RocksdbError(err.to_string())
    }
}

impl From<jsonrpc_core::Error> for Error {
    fn from(err: jsonrpc_core::Error) -> Error {
        Error::JsonRpcError(err.to_string())
    }
}

// err.fmt();
impl From<Error> for jsonrpc_core::Error {
    fn from(err: Error) -> jsonrpc_core::Error {
        jsonrpc_core::Error::invalid_params(err.to_string())
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

impl From<rusqlite::Error> for Error {
    fn from(err: rusqlite::Error) -> Error {
        Error::RusqliteError(err.to_string())
    }
}

impl From<ZkVmError> for Error {
    fn from(_err: ZkVmError) -> Error {
        Error::VmError
    }
}

impl From<bellman::SynthesisError> for Error {
    fn from(_err: bellman::SynthesisError) -> Error {
        Error::Groth16Error
    }
}

impl<T> From<async_channel::SendError<T>> for Error {
    fn from(_err: async_channel::SendError<T>) -> Error {
        Error::AsyncChannelSenderError
    }
}

impl From<async_channel::RecvError> for Error {
    fn from(_err: async_channel::RecvError) -> Error {
        Error::AsyncChannelReceiverError
    }
}

impl From<std::net::AddrParseError> for Error {
    fn from(_err: std::net::AddrParseError) -> Error {
        Error::AddrParseError
    }
}

impl From<std::num::ParseIntError> for Error {
    fn from(_err: std::num::ParseIntError) -> Error {
        Error::ParseIntError
    }
}

impl From<std::num::ParseFloatError> for Error {
    fn from(_err: std::num::ParseFloatError) -> Error {
        Error::ParseFloatError
    }
}

impl From<std::string::FromUtf8Error> for Error {
    fn from(_err: std::string::FromUtf8Error) -> Error {
        Error::Utf8Error
    }
}

impl From<std::str::Utf8Error> for Error {
    fn from(err: std::str::Utf8Error) -> Error {
        Error::StrUtf8Error(err.to_string())
    }
}

impl From<state::VerifyFailed> for Error {
    fn from(_err: state::VerifyFailed) -> Error {
        Error::VerifyFailed
    }
}

impl From<client::ClientFailed> for Error {
    fn from(err: client::ClientFailed) -> Error {
        Error::ClientFailed(err.to_string())
    }
}

#[cfg(feature = "btc")]
impl From<crate::service::BtcFailed> for Error {
    fn from(err: crate::service::BtcFailed) -> Error {
        Error::BtcFailed(err.to_string())
    }
}

#[cfg(feature = "sol")]
impl From<crate::service::SolFailed> for Error {
    fn from(err: crate::service::SolFailed) -> Error {
        Error::SolFailed(err.to_string())
    }
}

impl From<surf::Error> for Error {
    fn from(err: surf::Error) -> Error {
        Error::SurfHttpError(err.to_string())
    }
}

impl From<toml::de::Error> for Error {
    fn from(err: toml::de::Error) -> Error {
        Error::TomlDeserializeError(err.to_string())
    }
}

impl From<toml::ser::Error> for Error {
    fn from(err: toml::ser::Error) -> Error {
        Error::TomlSerializeError(err.to_string())
    }
}

impl From<bs58::encode::Error> for Error {
    fn from(err: bs58::encode::Error) -> Error {
        Error::Base58EncodeError(err.to_string())
    }
}

impl From<bs58::decode::Error> for Error {
    fn from(err: bs58::decode::Error) -> Error {
        Error::Base58DecodeError(err.to_string())
    }
}
