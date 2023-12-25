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

use darkfi_money_contract::model::CoinAttributes;
use darkfi_sdk::{
    crypto::{poseidon_hash, DAO_CONTRACT_ID},
    pasta::pallas,
};

use rand::rngs::OsRng;

use darkfi::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};

use crate::model::{Dao, DaoAuthMoneyTransferParams, DaoProposal, VecAuthCallCommit};

pub struct DaoAuthMoneyTransferCall {
    pub proposal: DaoProposal,
    pub dao: Dao,
    pub input_user_data_blind: pallas::Base,
    pub dao_coin_attrs: CoinAttributes,
}

impl DaoAuthMoneyTransferCall {
    pub fn make(
        self,
        auth_xfer_zkbin: &ZkBinary,
        auth_xfer_pk: &ProvingKey,
    ) -> Result<(DaoAuthMoneyTransferParams, Vec<Proof>)> {
        let mut proofs = vec![];
        let params = DaoAuthMoneyTransferParams {};

        let dao_proposer_limit = pallas::Base::from(self.dao.proposer_limit);
        let dao_quorum = pallas::Base::from(self.dao.quorum);
        let dao_approval_ratio_quot = pallas::Base::from(self.dao.approval_ratio_quot);
        let dao_approval_ratio_base = pallas::Base::from(self.dao.approval_ratio_base);

        let (dao_pub_x, dao_pub_y) = self.dao.public_key.xy();

        let input_user_data_enc =
            poseidon_hash([self.dao.to_bulla().inner(), self.input_user_data_blind]);

        let prover_witnesses = vec![
            // proposal params
            Witness::Base(Value::known(self.proposal.auth_calls.commit())),
            Witness::Base(Value::known(pallas::Base::from(self.proposal.creation_day))),
            Witness::Base(Value::known(pallas::Base::from(self.proposal.duration_days))),
            Witness::Base(Value::known(self.proposal.user_data)),
            Witness::Base(Value::known(self.proposal.blind)),
            // DAO params
            Witness::Base(Value::known(dao_proposer_limit)),
            Witness::Base(Value::known(dao_quorum)),
            Witness::Base(Value::known(dao_approval_ratio_quot)),
            Witness::Base(Value::known(dao_approval_ratio_base)),
            Witness::Base(Value::known(self.dao.gov_token_id.inner())),
            Witness::Base(Value::known(dao_pub_x)),
            Witness::Base(Value::known(dao_pub_y)),
            Witness::Base(Value::known(self.dao.bulla_blind)),
            // Dao input user data blind
            Witness::Base(Value::known(self.input_user_data_blind)),
            // Dao output coin attrs
            Witness::Base(Value::known(pallas::Base::from(self.dao_coin_attrs.value))),
            Witness::Base(Value::known(self.dao_coin_attrs.token_id.inner())),
            Witness::Base(Value::known(self.dao_coin_attrs.serial)),
            // DAO_CONTRACT_ID
            Witness::Base(Value::known(DAO_CONTRACT_ID.inner())),
        ];

        let public_inputs = vec![
            self.proposal.to_bulla().inner(),
            input_user_data_enc,
            self.dao_coin_attrs.to_coin().inner(),
            DAO_CONTRACT_ID.inner(),
            self.proposal.auth_calls.commit(),
        ];

        let circuit = ZkCircuit::new(prover_witnesses, auth_xfer_zkbin);
        let proof = Proof::create(auth_xfer_pk, &[circuit], &public_inputs, &mut OsRng)
            .expect("DAO::exec() proving error!)");
        proofs.push(proof);

        Ok((params, proofs))
    }
}
