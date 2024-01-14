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

use darkfi::{
    zk::{halo2, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{
    crypto::{PublicKey, SecretKey, TokenId},
    pasta::pallas,
};
use log::debug;
use rand::rngs::OsRng;

use crate::model::{Dao, DaoMintParams};

#[derive(Clone)]
pub struct DaoInfo {
    pub proposer_limit: u64,
    pub quorum: u64,
    pub approval_ratio_quot: u64,
    pub approval_ratio_base: u64,
    pub gov_token_id: TokenId,
    pub public_key: PublicKey,
    pub bulla_blind: pallas::Base,
}

pub fn make_mint_call(
    dao: &Dao,
    dao_secret_key: &SecretKey,
    dao_mint_zkbin: &ZkBinary,
    dao_mint_pk: &ProvingKey,
) -> Result<(DaoMintParams, Vec<Proof>)> {
    debug!(target: "dao", "Building DAO contract mint transaction");

    let dao_proposer_limit = pallas::Base::from(dao.proposer_limit);
    let dao_quorum = pallas::Base::from(dao.quorum);
    let dao_approval_ratio_quot = pallas::Base::from(dao.approval_ratio_quot);
    let dao_approval_ratio_base = pallas::Base::from(dao.approval_ratio_base);

    // NOTE: It's important to keep these in the same order as the zkas code.
    let prover_witnesses = vec![
        Witness::Base(halo2::Value::known(dao_proposer_limit)),
        Witness::Base(halo2::Value::known(dao_quorum)),
        Witness::Base(halo2::Value::known(dao_approval_ratio_quot)),
        Witness::Base(halo2::Value::known(dao_approval_ratio_base)),
        Witness::Base(halo2::Value::known(dao.gov_token_id.inner())),
        Witness::Base(halo2::Value::known(dao_secret_key.inner())),
        Witness::Base(halo2::Value::known(dao.bulla_blind)),
    ];

    let (pub_x, pub_y) = dao.public_key.xy();
    let dao_bulla = dao.to_bulla();
    let public = vec![pub_x, pub_y, dao_bulla.inner()];

    let circuit = ZkCircuit::new(prover_witnesses, dao_mint_zkbin);
    let proof = Proof::create(dao_mint_pk, &[circuit], &public, &mut OsRng)?;

    let dao_mint_params = DaoMintParams { dao_bulla, dao_pubkey: dao.public_key };

    Ok((dao_mint_params, vec![proof]))
}
