/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
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

use darkfi_sdk::crypto::{PublicKey, TokenId};
use darkfi_serial::{SerialDecodable, SerialEncodable};

use super::TransactionOutput;
use crate::crypto::{types::DrkValueBlind, BurnRevealedValues, Proof};

#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct PartialTransaction {
    pub clear_inputs: Vec<PartialTransactionClearInput>,
    pub inputs: Vec<PartialTransactionInput>,
    pub outputs: Vec<TransactionOutput>,
}

#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct PartialTransactionClearInput {
    pub value: u64,
    pub token_id: TokenId,
    pub value_blind: DrkValueBlind,
    pub token_blind: DrkValueBlind,
    pub signature_public: PublicKey,
}

#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct PartialTransactionInput {
    pub burn_proof: Proof,
    pub revealed: BurnRevealedValues,
}
