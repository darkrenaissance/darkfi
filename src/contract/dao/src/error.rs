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
pub enum DaoError {
    #[error("Invalid calls")]
    InvalidCalls,

    #[error("DAO already exists")]
    DaoAlreadyExists,

    #[error("Proposal inputs are empty")]
    ProposalInputsEmpty,

    #[error("Invalid input Merkle root")]
    InvalidInputMerkleRoot,

    #[error("Invalid DAO Merkle root")]
    InvalidDaoMerkleRoot,

    #[error("Proposal already exists")]
    ProposalAlreadyExists,

    #[error("Vote inputs are empty")]
    VoteInputsEmpty,

    #[error("Proposal doesn't exist")]
    ProposalNonexistent,

    #[error("Proposal ended")]
    ProposalEnded,

    #[error("Coin is already spent")]
    CoinAlreadySpent,

    #[error("Attempted double vote")]
    DoubleVote,

    #[error("Exec calls len does not match auth spec")]
    ExecCallWrongChildCallsLen,

    #[error("Child of exec call does not match proposal")]
    ExecCallWrongChildCall,

    #[error("Exec call has invalid tx format")]
    ExecCallInvalidFormat,

    #[error("Exec call value commitment mismatch")]
    ExecCallValueMismatch,

    #[error("Vote commitments mismatch")]
    VoteCommitMismatch,

    #[error("Sibling contract ID is not money::transfer()")]
    AuthXferSiblingWrongContractId,

    #[error("Sibling function code is not money::transfer()")]
    AuthXferSiblingWrongFunctionCode,

    #[error("Inputs with non-matching encrypted input user data")]
    AuthXferNonMatchingEncInputUserData,

    #[error("Auth call not found in parent")]
    AuthXferCallNotFoundInParent,

    #[error("Wrong number of outputs")]
    AuthXferWrongNumberOutputs,

    #[error("Wrong output coin")]
    AuthXferWrongOutputCoin,
}

impl From<DaoError> for ContractError {
    fn from(e: DaoError) -> Self {
        match e {
            DaoError::InvalidCalls => Self::Custom(1),
            DaoError::DaoAlreadyExists => Self::Custom(2),
            DaoError::ProposalInputsEmpty => Self::Custom(3),
            DaoError::InvalidInputMerkleRoot => Self::Custom(4),
            DaoError::InvalidDaoMerkleRoot => Self::Custom(5),
            DaoError::ProposalAlreadyExists => Self::Custom(6),
            DaoError::VoteInputsEmpty => Self::Custom(7),
            DaoError::ProposalNonexistent => Self::Custom(8),
            DaoError::ProposalEnded => Self::Custom(9),
            DaoError::CoinAlreadySpent => Self::Custom(10),
            DaoError::DoubleVote => Self::Custom(11),
            DaoError::ExecCallWrongChildCallsLen => Self::Custom(12),
            DaoError::ExecCallWrongChildCall => Self::Custom(13),
            DaoError::ExecCallInvalidFormat => Self::Custom(14),
            DaoError::ExecCallValueMismatch => Self::Custom(15),
            DaoError::VoteCommitMismatch => Self::Custom(16),
            DaoError::AuthXferSiblingWrongContractId => Self::Custom(17),
            DaoError::AuthXferSiblingWrongFunctionCode => Self::Custom(18),
            DaoError::AuthXferNonMatchingEncInputUserData => Self::Custom(19),
            DaoError::AuthXferCallNotFoundInParent => Self::Custom(20),
            DaoError::AuthXferWrongNumberOutputs => Self::Custom(21),
            DaoError::AuthXferWrongOutputCoin => Self::Custom(22),
        }
    }
}
