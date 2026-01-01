/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{crypto::SecretKey, pasta::pallas};
use rand::rngs::OsRng;
use tracing::debug;

use crate::model::{Dao, DaoMintParams};

#[allow(clippy::too_many_arguments)]
pub fn make_mint_call(
    dao: &Dao,
    dao_notes_secret_key: &SecretKey,
    dao_proposer_secret_key: &SecretKey,
    dao_proposals_secret_key: &SecretKey,
    dao_votes_secret_key: &SecretKey,
    dao_exec_secret_key: &SecretKey,
    dao_early_exec_secret_key: &SecretKey,
    dao_mint_zkbin: &ZkBinary,
    dao_mint_pk: &ProvingKey,
) -> Result<(DaoMintParams, Vec<Proof>)> {
    debug!(target: "contract::dao::client::mint", "Building DAO contract mint transaction call");

    let proposer_limit = pallas::Base::from(dao.proposer_limit);
    let quorum = pallas::Base::from(dao.quorum);
    let early_exec_quorum = pallas::Base::from(dao.early_exec_quorum);
    let approval_ratio_quot = pallas::Base::from(dao.approval_ratio_quot);
    let approval_ratio_base = pallas::Base::from(dao.approval_ratio_base);

    // NOTE: It's important to keep these in the same order as the zkas code.
    let prover_witnesses = vec![
        Witness::Base(Value::known(proposer_limit)),
        Witness::Base(Value::known(quorum)),
        Witness::Base(Value::known(early_exec_quorum)),
        Witness::Base(Value::known(approval_ratio_quot)),
        Witness::Base(Value::known(approval_ratio_base)),
        Witness::Base(Value::known(dao.gov_token_id.inner())),
        Witness::Base(Value::known(dao_notes_secret_key.inner())),
        Witness::Base(Value::known(dao_proposer_secret_key.inner())),
        Witness::Base(Value::known(dao_proposals_secret_key.inner())),
        Witness::Base(Value::known(dao_votes_secret_key.inner())),
        Witness::Base(Value::known(dao_exec_secret_key.inner())),
        Witness::Base(Value::known(dao_early_exec_secret_key.inner())),
        Witness::Base(Value::known(dao.bulla_blind.inner())),
    ];

    let (pub_x, pub_y) = dao.notes_public_key.xy();
    let dao_bulla = dao.to_bulla();
    let public = vec![pub_x, pub_y, dao_bulla.inner()];

    //darkfi::zk::export_witness_json("proof/witness/mint.json", &prover_witnesses, &public);
    let circuit = ZkCircuit::new(prover_witnesses, dao_mint_zkbin);
    let proof = Proof::create(dao_mint_pk, &[circuit], &public, &mut OsRng)?;

    let dao_mint_params = DaoMintParams { dao_bulla, dao_pubkey: dao.notes_public_key };

    Ok((dao_mint_params, vec![proof]))
}
