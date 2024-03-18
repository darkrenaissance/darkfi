/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

use darkfi_sdk::error::ContractError;

#[derive(Debug, Clone, thiserror::Error)]
// TODO: Make generic contract common errors like
// ParentCallFunctionMismatch
pub enum MoneyError {
    #[error("Missing inputs in transfer call")]
    TransferMissingInputs,

    #[error("Missing outputs in transfer call")]
    TransferMissingOutputs,

    #[error("Clear input used non-native token")]
    TransferClearInputNonNativeToken,

    #[error("Clear input used unauthorised pubkey")]
    TransferClearInputUnauthorised,

    #[error("Merkle root not found in previous state")]
    TransferMerkleRootNotFound,

    #[error("Duplicate nullifier found")]
    DuplicateNullifier,

    #[error("Duplicate coin found")]
    DuplicateCoin,

    #[error("Value commitment mismatch")]
    ValueMismatch,

    #[error("Token commitment mismatch")]
    TokenMismatch,

    #[error("Invalid number of inputs")]
    InvalidNumberOfInputs,

    #[error("Invalid number of outputs")]
    InvalidNumberOfOutputs,

    #[error("Spend hook is not zero")]
    SpendHookNonZero,

    #[error("Merkle root not found in previous state")]
    SwapMerkleRootNotFound,

    #[error("Token ID does not derive from mint authority")]
    TokenIdDoesNotDeriveFromMint,

    #[error("Token mint is frozen")]
    TokenMintFrozen,

    #[error("Parent call function mismatch")]
    ParentCallFunctionMismatch,

    #[error("Parent call input mismatch")]
    ParentCallInputMismatch,

    #[error("Child call function mismatch")]
    ChildCallFunctionMismatch,

    #[error("Child call input mismatch")]
    ChildCallInputMismatch,

    #[error("Call is not executed on genesis block")]
    GenesisCallNonGenesisBlock,

    #[error("Missing nullifier in set")]
    MissingNullifier,

    #[error("Call is executed on genesis block height")]
    PoWRewardCallOnGenesisBlock,

    #[error("Could not retrieve last block height from db")]
    PoWRewardRetrieveLastBlockHeightError,

    #[error("Call is not executed on next block height")]
    PoWRewardCallNotOnNextBlockHeight,

    #[error("No inputs in fee call")]
    FeeMissingInputs,

    #[error("Insufficient fee paid")]
    InsufficientFee,

    // TODO: This should catch-all (TransferMerkle../SwapMerkle...)
    #[error("Coin merkle root not found")]
    CoinMerkleRootNotFound,
}

impl From<MoneyError> for ContractError {
    fn from(e: MoneyError) -> Self {
        match e {
            MoneyError::TransferMissingInputs => Self::Custom(1),
            MoneyError::TransferMissingOutputs => Self::Custom(2),
            // 3 was removed
            MoneyError::TransferClearInputNonNativeToken => Self::Custom(4),
            MoneyError::TransferClearInputUnauthorised => Self::Custom(5),
            MoneyError::TransferMerkleRootNotFound => Self::Custom(6),
            MoneyError::DuplicateNullifier => Self::Custom(7),
            // 8 was removed
            // 9 was removed
            MoneyError::DuplicateCoin => Self::Custom(10),
            MoneyError::ValueMismatch => Self::Custom(11),
            MoneyError::TokenMismatch => Self::Custom(12),
            MoneyError::InvalidNumberOfInputs => Self::Custom(13),
            MoneyError::InvalidNumberOfOutputs => Self::Custom(14),
            MoneyError::SpendHookNonZero => Self::Custom(15),
            MoneyError::SwapMerkleRootNotFound => Self::Custom(16),
            MoneyError::TokenIdDoesNotDeriveFromMint => Self::Custom(17),
            MoneyError::TokenMintFrozen => Self::Custom(18),
            MoneyError::ParentCallFunctionMismatch => Self::Custom(19),
            MoneyError::ParentCallInputMismatch => Self::Custom(20),
            MoneyError::ChildCallFunctionMismatch => Self::Custom(21),
            MoneyError::ChildCallInputMismatch => Self::Custom(22),
            MoneyError::GenesisCallNonGenesisBlock => Self::Custom(23),
            MoneyError::MissingNullifier => Self::Custom(24),
            MoneyError::PoWRewardCallOnGenesisBlock => Self::Custom(25),
            MoneyError::PoWRewardRetrieveLastBlockHeightError => Self::Custom(26),
            MoneyError::PoWRewardCallNotOnNextBlockHeight => Self::Custom(27),
            MoneyError::FeeMissingInputs => Self::Custom(28),
            MoneyError::InsufficientFee => Self::Custom(29),
            MoneyError::CoinMerkleRootNotFound => Self::Custom(30),
        }
    }
}
