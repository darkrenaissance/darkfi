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
 * You should have received a copy of the GNU Affero General Public
 * License along with this program.
 * If not, see <https://www.gnu.org/licenses/>.
 */

use darkfi::{
    zk::{
        halo2::Value,
        Proof,
        ProvingKey,
        ZkCircuit,
        Witness
    },
    zkas::ZkBinary,
    Result,
};

use darkfi_sdk::{
    crypto::{
        poseidon_hash,
        SecretKey,
    },
    pasta::pallas,
};

use log::debug;

use rand::rngs::OsRng;

use crate::model::SetParamsV1;

pub struct SetCallBuilder {
    pub secret:     SecretKey,
    pub lock:       pallas::Base,
    pub car:        pallas::Base,
    pub key:        pallas::Base,
    pub value:      pallas::Base,
    pub zkbin:      ZkBinary,
    pub prove_key:  ProvingKey,
}

pub struct SetCallDebris {
    pub params: SetParamsV1,
    pub proofs: Vec<Proof>,
    pub signature_secrets: Vec<SecretKey>,
}

impl SetCallBuilder {
    pub fn build(&self) -> Result<SetCallDebris> {
        debug!("Building Map::SetV1 contract call");

        let params = SetParamsV1 { 
            // !!!!private computation done in rust!!!!
            account: poseidon_hash([self.secret.inner()]), 
            lock :self.lock,
            car :self.car,
            key: self.key,
            value: self.value,
        };

        Ok(
            SetCallDebris {
                params: params.clone(),
                proofs: vec![self.create_set_proof(params.clone())?],
                signature_secrets: vec![self.secret],
        })
    }

    pub fn create_set_proof(
        &self,
        public_inputs: SetParamsV1
    ) -> Result<Proof> {
        debug!("Creating map set proof");

        let witness       = vec![
            Witness::Base(Value::known(self.secret.inner())),
            Witness::Base(Value::known(self.car)),
            Witness::Base(Value::known(self.lock)),
            Witness::Base(Value::known(self.key)),
            Witness::Base(Value::known(self.value)),
        ];
        let circuit       = ZkCircuit::new(witness, self.zkbin.clone());
        let proof         = Proof::create(
            &self.prove_key,
            &[circuit],
            &public_inputs.to_vec(),
            &mut OsRng
        )?;

        Ok(proof)
    }
}

