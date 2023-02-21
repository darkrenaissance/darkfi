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

//! This API is experimental. Have to evaluate the best approach still.

pub struct MoneyMintClient {
    pub mint_authority: SecretKey,
    pub amount: u64,
    pub recipient: PublicKey,
    pub spend_hook: pallas::Base,
    pub user_data: pallas::Base,
}

impl MoneyMintClient {
    pub fn build(&self) -> Result<MoneyMintParams> {
        debug!(target: "money::client::token_mint", "Building params");
        assert!(self.amount != 0);

        let token_id = TokenId::derive(self.mint_authority);
        let value_blind = pallas::Scalar::random(&mut OsRng);
        let token_blind = pallas::Scalar::random(&mut OsRng);

        let serial = pallas::Base::random(&mut OsRng);
        let coin_blind = pallas::Base::random(&mut OsRng);

        let (pub_x, pub_y) = self.recipient.xy();

        // Create the clear input
        let input = ClearInput {
            value: self.amount,
            token_id,
            value_blind,
            token_blind,
            signature_public: PublicKey::from_secret(mint_authority),
        };

        // Create the anonymous output
        let coin = poseidon_hash([
            pub_x,
            pub_y,
            pallas::Base::from(self.amount),
            token_id.inner(),
            serial,
            self.spend_hook,
            self.user_data,
            coin_blind,
        ]);

        let note = MoneyNote {
            serial,
            value: self.amount,
            token_id,
            spend_hook,
            user_data,
            coin_blind,
            value_blind,
            token_blind,
            memo: vec![],
        };

        let encrypted_note = note.encrypt(&self.recipient)?;

        let output = Output {
            value_commit: pedersen_commitment_u64(self.amount, value_blind),
            token_commit: pedersen_commitment_base(token_id, token_blind),
            coin,
            ciphertext: encrypted_note.ciphertext,
            ephem_public: encrypted_note.ephem_public,
        };

        Ok(MoneyMintParams { input, output })
    }

    pub fn prove(&self, params: &MoneyMintParams, zkbin: &ZkBinary, proving_key: &ProvingKey) -> Result<Proof> {
        let prover_witnesses = vec![];

        let circuit = ZkCircuit::new(prover_witnesses, zkbin.clone());
        let proof = Proof::create(proving_key, &[circuit], &public_inputs, &mut OsRng)?;

        Ok(proof)
    }
}
