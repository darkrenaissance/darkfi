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

use darkfi_sdk::crypto::{poseidon_hash, PublicKey, SecretKey, TokenId};
use halo2_proofs::circuit::Value;
use pasta_curves::pallas;
use rand::rngs::OsRng;

use darkfi::{
    crypto::Proof,
    zk::vm::{Witness, ZkCircuit},
};

use crate::{
    contract::dao::{mint::validate::CallData, state::DaoBulla, CONTRACT_ID},
    util::{FuncCall, ZkContractInfo, ZkContractTable},
};

#[derive(Clone)]
pub struct DaoParams {
    // Number of governance tokens that holder must possess in order to propose a new vote.
    pub proposer_limit: u64,
    // Number of minimum votes that must be met for a proposal to pass.
    pub quorum: u64,
    // Ratio of winning to total votes for a proposal to pass.Ex- YES_VOTES_COUNT/(YES_VOTES_COUNT+NO_VOTES_COUNT) ???
    pub approval_ratio_quot: u64,
    // Ratio of winning to total votes for a proposal to pass. NO_VOTES_COUNT/(YES_VOTES_COUNT+NO_VOTES_COUNT) ???
    pub approval_ratio_base: u64,
    // Governance Token ID ???
    pub gov_token_id: TokenId,
    // Public key of the DAO for permissioned access.
    pub public_key: PublicKey,
    // Blinding factor for the DAO bulla.
    pub bulla_blind: pallas::Base,
}

pub struct Builder {
    // Number of governance tokens that holder must possess in order to propose a new vote.
    pub dao_proposer_limit: u64,
    // Number of minimum votes that must be met for a proposal to pass.
    pub dao_quorum: u64,
    // Ratio of winning to total votes for a proposal to pass.Ex- YES_VOTES_COUNT/(YES_VOTES_COUNT+NO_VOTES_COUNT) ???
    pub dao_approval_ratio_quot: u64,
    // Ratio of winning to total votes for a proposal to pass. NO_VOTES_COUNT/(YES_VOTES_COUNT+NO_VOTES_COUNT) ???
    pub dao_approval_ratio_base: u64,
    // Governance Token ID 
    pub gov_token_id: TokenId,
    // Public key of the DAO for permissioned access.
    pub dao_pubkey: PublicKey,
    // Blinding factor for the DAO bulla.
    pub dao_bulla_blind: pallas::Base,
    // signature_secret 
    pub _signature_secret: SecretKey,
}

impl Builder {
    /// Consumes self, and produces the function call
    pub fn build(self, zk_bins: &ZkContractTable) -> FuncCall {
        // Dao bulla
        let dao_proposer_limit = pallas::Base::from(self.dao_proposer_limit);
        let dao_quorum = pallas::Base::from(self.dao_quorum);
        let dao_approval_ratio_quot = pallas::Base::from(self.dao_approval_ratio_quot);
        let dao_approval_ratio_base = pallas::Base::from(self.dao_approval_ratio_base);

        let (dao_pub_x, dao_pub_y) = self.dao_pubkey.xy();

        let dao_bulla = poseidon_hash::<8>([
            dao_proposer_limit,
            dao_quorum,
            dao_approval_ratio_quot,
            dao_approval_ratio_base,
            self.gov_token_id.inner(),
            dao_pub_x,
            dao_pub_y,
            self.dao_bulla_blind,
        ]);
        let dao_bulla = DaoBulla(dao_bulla);

        // Now create the mint proof
        let zk_info = zk_bins.lookup(&"dao-mint".to_string()).unwrap();
        let zk_info = if let ZkContractInfo::Binary(info) = zk_info {
            info
        } else {
            panic!("Not binary info")
        };
        let zk_bin = zk_info.bincode.clone();
        let prover_witnesses = vec![
            Witness::Base(Value::known(dao_proposer_limit)),
            Witness::Base(Value::known(dao_quorum)),
            Witness::Base(Value::known(dao_approval_ratio_quot)),
            Witness::Base(Value::known(dao_approval_ratio_base)),
            Witness::Base(Value::known(self.gov_token_id.inner())),
            Witness::Base(Value::known(dao_pub_x)),
            Witness::Base(Value::known(dao_pub_y)),
            Witness::Base(Value::known(self.dao_bulla_blind)),
        ];
        let public_inputs = vec![dao_bulla.0];
        let circuit = ZkCircuit::new(prover_witnesses, zk_bin);

        let proving_key = &zk_info.proving_key;
        let mint_proof = Proof::create(proving_key, &[circuit], &public_inputs, &mut OsRng)
            .expect("DAO::mint() proving error!");

        let call_data = CallData { dao_bulla };
        FuncCall {
            contract_id: *CONTRACT_ID,
            func_id: *super::FUNC_ID,
            call_data: Box::new(call_data),
            proofs: vec![mint_proof],
        }
    }
}
