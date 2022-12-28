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

use darkfi_sdk::crypto::{PublicKey, SecretKey};
use halo2_proofs::circuit::Value;
use log::debug;
use pasta_curves::pallas;
use rand::rngs::OsRng;

use darkfi::{
    crypto::Proof,
    zk::vm::{Witness, ZkCircuit},
};

use crate::{
    contract::example::{foo::validate::CallData, CONTRACT_ID},
    util::{FuncCall, ZkContractInfo, ZkContractTable},
};

pub struct Foo {
    pub a: u64,
    pub b: u64,
}

pub struct Builder {
    pub foo: Foo,
    pub signature_secret: SecretKey,
}

impl Builder {
    pub fn build(self, zk_bins: &ZkContractTable) -> FuncCall {
        debug!(target: "example_contract::foo::wallet::Builder", "build()");
        let mut proofs = vec![];

        let zk_info = zk_bins.lookup(&"example-foo".to_string()).unwrap();
        let zk_info = if let ZkContractInfo::Binary(info) = zk_info {
            info
        } else {
            panic!("Not binary info")
        };

        let zk_bin = zk_info.bincode.clone();

        let prover_witnesses = vec![
            Witness::Base(Value::known(pallas::Base::from(self.foo.a))),
            Witness::Base(Value::known(pallas::Base::from(self.foo.b))),
        ];

        let a = pallas::Base::from(self.foo.a);
        let b = pallas::Base::from(self.foo.b);

        let c = a + b;

        let public_inputs = vec![c];

        let circuit = ZkCircuit::new(prover_witnesses, zk_bin);
        debug!(target: "example_contract::foo::wallet::Builder", "input_proof Proof::create()");
        let proving_key = &zk_info.proving_key;
        let input_proof = Proof::create(proving_key, &[circuit], &public_inputs, &mut OsRng)
            .expect("Example::foo() proving error!)");
        proofs.push(input_proof);

        let signature_public = PublicKey::from_secret(self.signature_secret);

        let call_data = CallData { public_value: c, signature_public };

        FuncCall {
            contract_id: *CONTRACT_ID,
            func_id: *super::FUNC_ID,
            call_data: Box::new(call_data),
            proofs,
        }
    }
}
