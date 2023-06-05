/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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

/// Block definition
pub mod block;
pub use block::{Block, BlockInfo, BlockProposal, Header};

/// Constants
pub mod constants;
pub use constants::{
    TESTNET_BOOTSTRAP_TIMESTAMP, TESTNET_GENESIS_HASH_BYTES, TESTNET_GENESIS_TIMESTAMP,
    TESTNET_INITIAL_DISTRIBUTION,
};

/// Consensus block leader information
pub mod lead_info;
pub use lead_info::{LeadInfo, LeadProof};

/// Consensus state
pub mod state;
pub use state::SlotCheckpoint;

/// Consensus validator state
pub mod validator;
pub use validator::{ValidatorState, ValidatorStatePtr};

/// Fee calculations
pub mod fees;

/// P2P net protocols
pub mod proto;

/// async tasks to utilize the protocols
pub mod task;

/// Lamport clock
pub mod clock;
pub use clock::{Clock, Ticks};

/// Consensus participation coin functions and definitions
pub mod lead_coin;
pub use lead_coin::LeadCoin;

/// Utility types
pub mod types;
pub use types::Float10;

/// Utility functions
pub mod utils;

/// Wallet functions
pub mod wallet;

/// transfered tx proof with public inputs.
pub mod stx;
pub use stx::TransferStx;

/// encrypted receipient coin info
pub mod rcpt;
pub use rcpt::{EncryptedTxRcpt, TxRcpt};

/// transfer transaction
pub mod tx;
pub use tx::Tx;
