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

use darkfi::{
    zk::{proof::ProvingKey, vm::ZkCircuit, vm_stack::Witness, Proof},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::{
    crypto::{poseidon_hash, PublicKey, SecretKey, TokenId},
    pasta::pallas,
};
use halo2_proofs::circuit::Value;
use log::debug;
use rand::rngs::OsRng;

use crate::state::{DaoBulla, DaoMintParams};

struct DaoMintRevealed {
    pub bulla: DaoBulla,
}

impl DaoMintRevealed {
    pub fn compute(
        dao_proposer_limit: pallas::Base,
        dao_quorum: pallas::Base,
        dao_approval_ratio_quot: pallas::Base,
        dao_approval_ratio_base: pallas::Base,
        gov_token_id: TokenId,
        dao_pubkey: &PublicKey,
        dao_bulla_blind: pallas::Base,
    ) -> Self {
        let (pub_x, pub_y) = dao_pubkey.xy();

        let dao_bulla = poseidon_hash([
            dao_proposer_limit,
            dao_quorum,
            dao_approval_ratio_quot,
            dao_approval_ratio_base,
            gov_token_id.inner(),
            pub_x,
            pub_y,
            dao_bulla_blind,
        ]);

        Self { bulla: DaoBulla::from(dao_bulla) }
    }

    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.bulla.inner()]
    }
}

fn create_dao_mint_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    dao_proposer_limit: pallas::Base,
    dao_quorum: pallas::Base,
    dao_approval_ratio_quot: pallas::Base,
    dao_approval_ratio_base: pallas::Base,
    gov_token_id: TokenId,
    dao_pubkey: &PublicKey,
    dao_bulla_blind: pallas::Base,
) -> Result<(Proof, DaoMintRevealed)> {
    let revealed = DaoMintRevealed::compute(
        dao_proposer_limit,
        dao_quorum,
        dao_approval_ratio_quot,
        dao_approval_ratio_base,
        gov_token_id,
        dao_pubkey,
        dao_bulla_blind,
    );

    let (pub_x, pub_y) = dao_pubkey.xy();

    // NOTE: It's important to keep these in the same order as the zkas code.
    let prover_witnesses = vec![
        Witness::Base(Value::known(dao_proposer_limit)),
        Witness::Base(Value::known(dao_quorum)),
        Witness::Base(Value::known(dao_approval_ratio_quot)),
        Witness::Base(Value::known(dao_approval_ratio_base)),
        Witness::Base(Value::known(gov_token_id.inner())),
        Witness::Base(Value::known(pub_x)),
        Witness::Base(Value::known(pub_y)),
        Witness::Base(Value::known(dao_bulla_blind)),
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin.clone());
    let proof = Proof::create(pk, &[circuit], &revealed.to_vec(), &mut OsRng)?;

    Ok((proof, revealed))
}

pub fn build_dao_mint_tx(
    dao_proposer_limit: u64,
    dao_quorum: u64,
    dao_approval_ratio_quot: u64,
    dao_approval_ratio_base: u64,
    gov_token_id: TokenId,
    dao_pubkey: &PublicKey,
    dao_bulla_blind: pallas::Base,
    signature_secret: &SecretKey,
    dao_mint_zkbin: &ZkBinary,
    dao_mint_pk: &ProvingKey,
) -> Result<(DaoMintParams, Vec<Proof>)> {
    debug!("Building DAO contract mint transaction");

    let (proof, revealed) = create_dao_mint_proof(
        dao_mint_zkbin,
        dao_mint_pk,
        pallas::Base::from(dao_proposer_limit),
        pallas::Base::from(dao_quorum),
        pallas::Base::from(dao_approval_ratio_quot),
        pallas::Base::from(dao_approval_ratio_base),
        gov_token_id,
        dao_pubkey,
        dao_bulla_blind,
    )?;

    let dao_bulla = revealed.bulla;
    let dao_mint_params = DaoMintParams { dao_bulla };

    Ok((dao_mint_params, vec![proof]))
}
