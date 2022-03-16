use borsh::maybestd::io::Error as BorshIoError;
use std::result::Result as ResultGeneric;

pub type ContractResult = ResultGeneric<(), ContractError>;

#[derive(Debug, thiserror::Error)]
pub enum ContractError {
    /// Allows on-chain programs to implement contract-specific error types and
    /// see them returned by the runtime. A contract-specific error may be any
    /// type that is represented as or serialized to an u32 integer.
    #[error("Custom contract error: {0:#x}")]
    Custom(u32),

    #[error("Internal error")]
    Internal,

    #[error("IO error: {0}")]
    BorshIoError(String),
}

/// Builtin return values occupy the upper 32 bits
const BUILTIN_BIT_SHIFT: usize = 32;
macro_rules! to_builtin {
    ($error:expr) => {
        ($error as u64) << BUILTIN_BIT_SHIFT
    };
}

pub const CUSTOM_ZERO: u64 = to_builtin!(1);
pub const INTERNAL_ERROR: u64 = to_builtin!(2);
pub const BORSH_IO_ERROR: u64 = to_builtin!(3);

impl From<ContractError> for u64 {
    fn from(err: ContractError) -> Self {
        match err {
            ContractError::Internal => INTERNAL_ERROR,
            ContractError::BorshIoError(_) => BORSH_IO_ERROR,
            ContractError::Custom(error) => {
                if error == 0 {
                    CUSTOM_ZERO
                } else {
                    error as u64
                }
            }
        }
    }
}

impl From<u64> for ContractError {
    fn from(error: u64) -> Self {
        match error {
            CUSTOM_ZERO => Self::Custom(0),
            INTERNAL_ERROR => Self::Internal,
            BORSH_IO_ERROR => Self::BorshIoError("Unknown".to_string()),
            _ => Self::Custom(error as u32),
        }
    }
}

impl From<BorshIoError> for ContractError {
    fn from(error: BorshIoError) -> Self {
        Self::BorshIoError(format!("{}", error))
    }
}
