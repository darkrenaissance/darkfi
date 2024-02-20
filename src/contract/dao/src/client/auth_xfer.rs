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

use darkfi_money_contract::model::CoinAttributes;
use darkfi_sdk::{
    crypto::{note::ElGamalEncryptedNote, poseidon_hash, BaseBlind, PublicKey, SecretKey},
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
    pub proposal_coinattrs: Vec<CoinAttributes>,
    pub dao: Dao,
    pub input_user_data_blind: BaseBlind,
    pub dao_coin_attrs: CoinAttributes,
}

impl DaoAuthMoneyTransferCall {
    pub fn make(
        self,
        auth_xfer_zkbin: &ZkBinary,
        auth_xfer_pk: &ProvingKey,
        auth_xfer_enc_coin_zkbin: &ZkBinary,
        auth_xfer_enc_coin_pk: &ProvingKey,
    ) -> Result<(DaoAuthMoneyTransferParams, Vec<Proof>)> {
        let mut proofs = vec![];

        // Proof for each coin of verifiable encryption

        let mut enc_attrs = vec![];
        let mut proposal_coinattrs = self.proposal_coinattrs;
        proposal_coinattrs.push(self.dao_coin_attrs.clone());
        for coin_attrs in proposal_coinattrs {
            let coin = coin_attrs.to_coin();

            let ephem_secret = SecretKey::random(&mut OsRng);
            let ephem_pubkey = PublicKey::from_secret(ephem_secret);
            let (ephem_x, ephem_y) = ephem_pubkey.xy();

            let value_base = pallas::Base::from(coin_attrs.value);

            let note = [
                value_base,
                coin_attrs.token_id.inner(),
                coin_attrs.spend_hook.inner(),
                coin_attrs.user_data,
                coin_attrs.blind.inner(),
            ];
            let enc_note =
                ElGamalEncryptedNote::encrypt_unsafe(note, &ephem_secret, &coin_attrs.public_key)?;

            let prover_witnesses = vec![
                Witness::EcNiPoint(Value::known(coin_attrs.public_key.inner())),
                Witness::Base(Value::known(value_base)),
                Witness::Base(Value::known(coin_attrs.token_id.inner())),
                Witness::Base(Value::known(coin_attrs.spend_hook.inner())),
                Witness::Base(Value::known(coin_attrs.user_data)),
                Witness::Base(Value::known(coin_attrs.blind.inner())),
                Witness::Base(Value::known(ephem_secret.inner())),
            ];

            let public_inputs = vec![
                coin.inner(),
                ephem_x,
                ephem_y,
                enc_note.encrypted_values[0],
                enc_note.encrypted_values[1],
                enc_note.encrypted_values[2],
                enc_note.encrypted_values[3],
                enc_note.encrypted_values[4],
            ];

            let circuit = ZkCircuit::new(prover_witnesses, auth_xfer_enc_coin_zkbin);
            let proof =
                Proof::create(auth_xfer_enc_coin_pk, &[circuit], &public_inputs, &mut OsRng)?;
            proofs.push(proof);

            enc_attrs.push(enc_note);
        }

        // Build the main proof

        let ephem_secret = SecretKey::random(&mut OsRng);
        let change_ephem_pubkey = PublicKey::from_secret(ephem_secret);
        let (ephem_x, ephem_y) = change_ephem_pubkey.xy();

        let dao_public_key = self.dao.public_key.inner();
        let dao_change_value = pallas::Base::from(self.dao_coin_attrs.value);

        let note = [
            dao_change_value,
            self.dao_coin_attrs.token_id.inner(),
            self.dao_coin_attrs.blind.inner(),
        ];

        let dao_change_attrs =
            ElGamalEncryptedNote::encrypt_unsafe(note, &ephem_secret, &self.dao.public_key)?;

        let params = DaoAuthMoneyTransferParams { enc_attrs, dao_change_attrs };

        let dao_proposer_limit = pallas::Base::from(self.dao.proposer_limit);
        let dao_quorum = pallas::Base::from(self.dao.quorum);
        let dao_approval_ratio_quot = pallas::Base::from(self.dao.approval_ratio_quot);
        let dao_approval_ratio_base = pallas::Base::from(self.dao.approval_ratio_base);

        let input_user_data_enc =
            poseidon_hash([self.dao.to_bulla().inner(), self.input_user_data_blind.inner()]);

        let prover_witnesses = vec![
            // proposal params
            Witness::Base(Value::known(self.proposal.auth_calls.commit())),
            Witness::Base(Value::known(pallas::Base::from(self.proposal.creation_day))),
            Witness::Base(Value::known(pallas::Base::from(self.proposal.duration_days))),
            Witness::Base(Value::known(self.proposal.user_data)),
            Witness::Base(Value::known(self.proposal.blind.inner())),
            // DAO params
            Witness::Base(Value::known(dao_proposer_limit)),
            Witness::Base(Value::known(dao_quorum)),
            Witness::Base(Value::known(dao_approval_ratio_quot)),
            Witness::Base(Value::known(dao_approval_ratio_base)),
            Witness::Base(Value::known(self.dao.gov_token_id.inner())),
            Witness::EcNiPoint(Value::known(dao_public_key)),
            Witness::Base(Value::known(self.dao.bulla_blind.inner())),
            // Dao input user data blind
            Witness::Base(Value::known(self.input_user_data_blind.inner())),
            // Dao output coin attrs
            Witness::Base(Value::known(dao_change_value)),
            Witness::Base(Value::known(self.dao_coin_attrs.token_id.inner())),
            Witness::Base(Value::known(self.dao_coin_attrs.blind.inner())),
            // DAO::exec() func ID
            Witness::Base(Value::known(self.dao_coin_attrs.spend_hook.inner())),
            // Encrypted change DAO output
            Witness::Base(Value::known(ephem_secret.inner())),
        ];

        let public_inputs = vec![
            self.proposal.to_bulla().inner(),
            input_user_data_enc,
            self.dao_coin_attrs.to_coin().inner(),
            self.dao_coin_attrs.spend_hook.inner(),
            self.proposal.auth_calls.commit(),
            ephem_x,
            ephem_y,
            dao_change_attrs.encrypted_values[0],
            dao_change_attrs.encrypted_values[1],
            dao_change_attrs.encrypted_values[2],
        ];

        let circuit = ZkCircuit::new(prover_witnesses, auth_xfer_zkbin);
        let proof = Proof::create(auth_xfer_pk, &[circuit], &public_inputs, &mut OsRng)?;
        proofs.push(proof);

        Ok((params, proofs))
    }
}
