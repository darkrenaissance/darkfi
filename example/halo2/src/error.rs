use std::fmt;

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
    InvalidParamType,
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
    TryIntoError,
    TryFromError,
    JsonRpcError(String),
    RocksdbError(String),
    TreeFull,
    SerdeJsonError(String),
    SurfHttpError(String),
    EmptyPassword,
    TomlDeserializeError(String),
    TomlSerializeError(String),
    CashierNoReply,
    Base58EncodeError(String),
    Base58DecodeError(String),
    BadBTCAddress(String),
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
            Error::InvalidParamType => f.write_str("Invalid param type"),
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
            Error::TryIntoError => f.write_str("TryInto error"),
            Error::TryFromError => f.write_str("TryFrom error"),
            Error::RocksdbError(ref err) => write!(f, "Rocksdb Error: {}", err),
            Error::JsonRpcError(ref err) => write!(f, "JsonRpc Error: {}", err),
            Error::TreeFull => f.write_str("MerkleTree is full"),
            Error::SerdeJsonError(ref err) => write!(f, "Json serialization error: {}", err),
            Error::SurfHttpError(ref err) => write!(f, "Surf Http error: {}", err),
            Error::EmptyPassword => f.write_str("Password is empty. Cannot create database"),
            Error::TomlDeserializeError(ref err) => write!(f, "Toml parsing error: {}", err),
            Error::TomlSerializeError(ref err) => write!(f, "Toml parsing error: {}", err),
            Error::Base58EncodeError(ref err) => write!(f, "bs58 encode error: {}", err),
            Error::Base58DecodeError(ref err) => write!(f, "bs58 decode error: {}", err),
            Error::CashierNoReply => f.write_str("Cashier did not reply with BTC address"),
            Error::BadBTCAddress(ref err) => write!(f, "could not parse BTC address: {}", err),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Error {
        Error::Io(err.kind())
    }
}
