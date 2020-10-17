use std::fmt;

use crate::vm::ZKVMError;

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
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Error {
        Error::Io(err)
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
