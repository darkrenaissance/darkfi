use log::debug;
use rand::rngs::OsRng;

use halo2_proofs::circuit::Value;
use pasta_curves::pallas;

use darkfi::{
    crypto::Proof,
    zk::vm::{Witness, ZkCircuit},
};

use crate::{
    demo::{FuncCall, ZkContractInfo, ZkContractTable},
    example_contract::foo::validate::{CallData, Header},
};

pub struct Foo {
    pub a: u64,
    pub b: u64,
}

pub struct Builder {
    pub foo: Foo,
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

        let call_data = CallData { public_value: c };

        FuncCall {
            contract_id: "Example".to_string(),
            func_id: "Example::foo()".to_string(),
            call_data: Box::new(call_data),
            proofs,
        }
    }
}
