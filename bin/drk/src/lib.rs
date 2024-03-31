/// Error codes
mod error;
pub use error::WalletDbError;

/// CLI wallet structure
mod drk;
pub use drk::Drk;

/// darkfid JSON-RPC related methods
mod rpc;

/// Payment methods
mod transfer;

/// Swap methods
mod swap;
pub use swap::PartialSwapData;

/// Token methods
mod token;

/// CLI utility functions
mod cli_util;
pub use cli_util::{generate_completions, kaching, parse_token_pair, parse_value_pair};

/// Wallet functionality related to Money
mod money;
pub use money::BALANCE_BASE10_DECIMALS;

/// Wallet functionality related to Dao
mod dao;
pub use dao::DaoParams;

/// Wallet functionality related to transactions history
mod txs_history;

/// Wallet database operations handler
mod walletdb;
pub(crate) use walletdb::{WalletDb, WalletPtr};
