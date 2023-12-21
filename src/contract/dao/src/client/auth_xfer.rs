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

use darkfi_sdk::{
    crypto::{pasta_prelude::*, pedersen_commitment_u64, SecretKey},
    pasta::pallas,
};

use log::debug;
use rand::rngs::OsRng;

use darkfi::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};

use crate::model::{Dao, DaoAuthMoneyTransferParams, DaoBlindAggregateVote, DaoProposal};

pub struct DaoAuthMoneyTransferCall {}

impl DaoAuthMoneyTransferCall {
    pub fn make(
        self,
        //_auth_xfer_zkbin: &ZkBinary,
        //_auth_xfer_pk: &ProvingKey,
    ) -> Result<(DaoAuthMoneyTransferParams, Vec<Proof>)> {
        let proofs = vec![];
        let params = DaoAuthMoneyTransferParams {};
        Ok((params, proofs))
    }
}
