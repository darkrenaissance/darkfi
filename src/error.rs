// TODO: Add support for rusqlite error
use rusqlite;
use std::fmt;

use crate::state;
use crate::vm::ZKVMError;

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
    AsyncChannelError,
    MalformedPacket,
    AddrParseError,
    BadVariableRefType,
    BadOperationType,
    BadConstraintType,
    InvalidParamName,
    MissingParams,
    VMError,
    BadContract,
    Groth16Error,
    RusqliteError,
    OperationFailed,
    ConnectFailed,
    ConnectTimeout,
    ChannelStopped,
    ChannelTimeout,
    ServiceStopped,
    Utf8Error,
    NoteDecryptionFailed,
    ServicesError(&'static str),
    ZMQError(String),
    VerifyFailed,
    TryIntoError,
    TryFromError,
    JsonRpcError(String),
    RocksdbError(String),
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
            Error::AsyncChannelError => f.write_str("Async_channel error"),
            Error::MalformedPacket => f.write_str("Malformed packet"),
            Error::AddrParseError => f.write_str("Unable to parse address"),
            Error::BadVariableRefType => f.write_str("Bad variable ref type byte"),
            Error::BadOperationType => f.write_str("Bad operation type byte"),
            Error::BadConstraintType => f.write_str("Bad constraint type byte"),
            Error::InvalidParamName => f.write_str("Invalid param name"),
            Error::MissingParams => f.write_str("Missing params"),
            Error::VMError => f.write_str("VM error"),
            Error::BadContract => f.write_str("Contract is poorly defined"),
            Error::Groth16Error => f.write_str("Groth16 error"),
            Error::RusqliteError => f.write_str("Rusqlite error"),
            Error::OperationFailed => f.write_str("Operation failed"),

            Error::ConnectFailed => f.write_str("Connection failed"),
            Error::ConnectTimeout => f.write_str("Connection timed out"),
            Error::ChannelStopped => f.write_str("Channel stopped"),
            Error::ChannelTimeout => f.write_str("Channel timed out"),
            Error::ServiceStopped => f.write_str("Service stopped"),
            Error::Utf8Error => f.write_str("Malformed UTF8"),
            Error::NoteDecryptionFailed => f.write_str("Unable to decrypt mint note"),
            Error::ServicesError(ref err) => write!(f, "Services error: {}", err),
            Error::ZMQError(ref err) => write!(f, "ZMQError: {}", err),
            Error::VerifyFailed => f.write_str("Verify failed"),
            Error::TryIntoError => f.write_str("TryInto error"),
            Error::TryFromError => f.write_str("TryFrom error"),
            Error::RocksdbError(ref err) => write!(f, "Rocksdb Error: {}", err),
            Error::JsonRpcError(ref err) => write!(f, "JsonRpc Error: {}", err),
        }
    }
}

// TODO: Match statement to parse external errors into strings.
impl From<zeromq::ZmqError> for Error {
    fn from(err: zeromq::ZmqError) -> Error {
        Error::ZMQError(err.to_string())
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

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Error {
        Error::Io(err.kind())
    }
}

impl From<rusqlite::Error> for Error {
    fn from(_err: rusqlite::Error) -> Error {
        Error::RusqliteError
    }
}

impl From<ZKVMError> for Error {
    fn from(_err: ZKVMError) -> Error {
        Error::VMError
    }
}

impl From<bellman::SynthesisError> for Error {
    fn from(_err: bellman::SynthesisError) -> Error {
        Error::Groth16Error
    }
}

impl<T> From<async_channel::SendError<T>> for Error {
    fn from(_err: async_channel::SendError<T>) -> Error {
        Error::AsyncChannelError
    }
}

impl From<async_channel::RecvError> for Error {
    fn from(_err: async_channel::RecvError) -> Error {
        Error::AsyncChannelError
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

impl From<std::string::FromUtf8Error> for Error {
    fn from(_err: std::string::FromUtf8Error) -> Error {
        Error::Utf8Error
    }
}

impl From<state::VerifyFailed> for Error {
    fn from(_err: state::VerifyFailed) -> Error {
        Error::VerifyFailed
    }
}
