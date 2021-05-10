use std::fmt;

use crate::net::error::NetError;
use crate::vm::ZKVMError;
use rusqlite;

use async_zmq::zmq;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Foo,
    CommitsDontAdd,
    InvalidCredential,
    TransactionPedersenCheckFailed,
    TokenAlreadySpent,
    InputTokenVerifyFailed,
    RangeproofPedersenMatchFailed,
    ProofsFailed,
    MissingProofs,
    Io(std::io::Error),
    /// VarInt was encoded in a non-minimal way
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
    VMError(ZKVMError),
    BadContract,
    Groth16Error(bellman::SynthesisError),
    ZMQError(zmq::Error),
    RusqliteError(rusqlite::Error),
    OperationFailed,
    ConnectFailed,
    ConnectTimeout,
    ChannelStopped,
    ChannelTimeout,
    ServiceStopped,
    Utf8Error,
    NoteDecryptionFailed,
}

impl std::error::Error for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        match *self {
            Error::Foo => f.write_str("foo"),
            Error::CommitsDontAdd => f.write_str("Commits don't add up properly"),
            Error::InvalidCredential => f.write_str("Credential is invalid"),
            Error::TransactionPedersenCheckFailed => {
                f.write_str("Transaction pedersens for input and output don't sum up")
            }
            Error::TokenAlreadySpent => f.write_str("This input token is already spent"),
            Error::InputTokenVerifyFailed => f.write_str("Input token verify of credential failed"),
            Error::RangeproofPedersenMatchFailed => {
                f.write_str("Rangeproof pedersen check for match failed")
            }
            Error::ProofsFailed => f.write_str("Proof validation failed"),
            Error::MissingProofs => f.write_str("Missing proofs"),
            Error::Io(ref err) => fmt::Display::fmt(err, f),
            Error::NonMinimalVarInt => f.write_str("non-minimal varint"),
            Error::ParseFailed(ref err) => write!(f, "parse failed: {}", err),
            Error::ParseIntError => f.write_str("Parse int error"),
            Error::AsyncChannelError => f.write_str("async_channel error"),
            Error::MalformedPacket => f.write_str("Malformed packet"),
            Error::AddrParseError => f.write_str("Unable to parse address"),
            Error::BadVariableRefType => f.write_str("Bad variable ref type byte"),
            Error::BadOperationType => f.write_str("Bad operation type byte"),
            Error::BadConstraintType => f.write_str("Bad constraint type byte"),
            Error::InvalidParamName => f.write_str("Invalid param name"),
            Error::MissingParams => f.write_str("Missing params"),
            Error::VMError(_) => f.write_str("VM error"),
            Error::BadContract => f.write_str("Contract is poorly defined"),
            Error::Groth16Error(ref err) => write!(f, "groth16 error: {}", err),
            Error::ZMQError(ref err) => write!(f, "ZMQ error: {}", err),
            Error::RusqliteError(ref err) => write!(f, "Rusqlite error: {}", err),
            Error::OperationFailed => f.write_str("Operation failed"),
            Error::ConnectFailed => f.write_str("Connection failed"),
            Error::ConnectTimeout => f.write_str("Connection timed out"),
            Error::ChannelStopped => f.write_str("Channel stopped"),
            Error::ChannelTimeout => f.write_str("Channel timed out"),
            Error::ServiceStopped => f.write_str("Service stopped"),
            Error::Utf8Error => f.write_str("Malformed UTF8"),
            Error::NoteDecryptionFailed => f.write_str("Unable to decrypt mint note"),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Error {
        Error::Io(err)
    }
}

impl From<zmq::Error> for Error {
    fn from(err: zmq::Error) -> Error {
        Error::ZMQError(err)
    }
}

impl From<rusqlite::Error> for Error {
    fn from(err: rusqlite::Error) -> Error {
        Error::RusqliteError(err)
    }
}

impl From<ZKVMError> for Error {
    fn from(err: ZKVMError) -> Error {
        Error::VMError(err)
    }
}

impl From<bellman::SynthesisError> for Error {
    fn from(err: bellman::SynthesisError) -> Error {
        Error::Groth16Error(err)
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

impl From<NetError> for Error {
    fn from(err: NetError) -> Error {
        match err {
            NetError::OperationFailed => Error::OperationFailed,
            NetError::ConnectFailed => Error::ConnectFailed,
            NetError::ConnectTimeout => Error::ConnectTimeout,
            NetError::ChannelStopped => Error::ChannelStopped,
            NetError::ChannelTimeout => Error::ChannelTimeout,
            NetError::ServiceStopped => Error::ServiceStopped,
        }
    }
}

impl From<std::string::FromUtf8Error> for Error {
    fn from(_err: std::string::FromUtf8Error) -> Error {
        Error::Utf8Error
    }
}
