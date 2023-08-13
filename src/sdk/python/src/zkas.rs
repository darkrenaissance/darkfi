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

use std::ops::Deref;

use darkfi::{
    zk::{self, empty_witnesses, halo2::Value},
    zkas::{self, decoder},
};
use darkfi_sdk::{crypto::MerkleNode, pasta::pallas};
use pyo3::{pyclass, pymethods, types::PyModule, PyCell, PyResult, Python};
use rand::rngs::OsRng;

use super::pasta::{Ep, Fp, Fq};

#[pyclass]
/// Decoded zkas bincode
pub struct ZkBinary(decoder::ZkBinary);

#[pymethods]
impl ZkBinary {
    #[new]
    fn new(filename: String, source_code: String) -> Self {
        let source = source_code.replace('\t', "    ").replace("\r\n", "\n");
        let lexer = zkas::Lexer::new(&filename, source.chars());
        let tokens = lexer.lex();
        let parser = zkas::Parser::new(&filename, source.chars(), tokens);
        let (namespace, k, constants, witnesses, statements) = parser.parse();
        let mut analyzer =
            zkas::Analyzer::new(&filename, source.chars(), constants, witnesses, statements);
        analyzer.analyze_types();

        let compiler = zkas::Compiler::new(
            &filename,
            source.chars(),
            namespace,
            k,
            analyzer.constants,
            analyzer.witnesses,
            analyzer.statements,
            analyzer.literals,
            true,
        );

        let bincode = compiler.compile();

        Self::decode(bincode)
    }

    #[staticmethod]
    fn decode(bytes: Vec<u8>) -> Self {
        let bincode = decoder::ZkBinary::decode(bytes.as_slice()).unwrap();
        Self(bincode)
    }

    fn k(&self) -> u32 {
        self.0.k
    }
}

#[pyclass]
/// Class representing a zkVM circuit, the witness values, and the zkas binary
/// defining the circuit code path.
pub struct ZkCircuit(zk::vm::ZkCircuit, Vec<zk::vm::Witness>, decoder::ZkBinary);

#[pymethods]
impl ZkCircuit {
    #[new]
    fn new(zkbin: &PyCell<ZkBinary>) -> Self {
        let zkbin = zkbin.borrow().deref().0.clone();
        let circuit = zk::vm::ZkCircuit::new(vec![], &zkbin);
        Self(circuit, vec![], zkbin)
    }

    fn prover_build(&self) -> Self {
        let circuit = zk::vm::ZkCircuit::new(self.1.clone(), &self.2);
        Self(circuit, self.1.clone(), self.2.clone())
    }

    fn verifier_build(&self) -> Self {
        let witnesses = empty_witnesses(&self.2).unwrap();
        let circuit = zk::vm::ZkCircuit::new(witnesses.clone(), &self.2);
        Self(circuit, witnesses, self.2.clone())
    }

    fn witness_ecpoint(&mut self, w: &PyCell<Ep>) {
        let w = w.borrow();
        let w = w.deref();
        self.1.push(zk::vm::Witness::EcPoint(Value::known(w.0)));
    }

    fn witness_ecnipoint(&mut self, w: &PyCell<Ep>) {
        let w = w.borrow();
        let w = w.deref();
        self.1.push(zk::vm::Witness::EcNiPoint(Value::known(w.0)));
    }

    fn witness_base(&mut self, w: &PyCell<Fp>) {
        let w = w.borrow();
        let w = w.deref();
        self.1.push(zk::vm::Witness::Base(Value::known(w.0)));
    }

    fn witness_scalar(&mut self, w: &PyCell<Fq>) {
        let w = w.borrow();
        let w = w.deref();
        self.1.push(zk::vm::Witness::Scalar(Value::known(w.0)));
    }

    fn witness_merklepath(&mut self, w: Vec<&PyCell<Fp>>) {
        assert!(w.len() == 32);
        let path: Vec<MerkleNode> =
            w.iter().map(|x| MerkleNode::from(x.borrow().deref().0)).collect();
        self.1.push(zk::vm::Witness::MerklePath(Value::known(path.try_into().unwrap())));
    }

    fn witness_uint32(&mut self, w: u32) {
        self.1.push(zk::vm::Witness::Uint32(Value::known(w)));
    }

    fn witness_uint64(&mut self, w: u64) {
        self.1.push(zk::vm::Witness::Uint64(Value::known(w)));
    }
}

#[pyclass]
/// Verifying key for a zkVM proof
pub struct VerifyingKey(zk::proof::VerifyingKey);

#[pymethods]
impl VerifyingKey {
    #[staticmethod]
    fn build(k: u32, circuit: &PyCell<ZkCircuit>) -> Self {
        let circuit_ref = circuit.borrow();
        let circuit = &circuit_ref.deref().0;
        let vk = zk::proof::VerifyingKey::build(k, circuit);
        Self(vk)
    }
}

#[pyclass]
/// Proving key for a zkVM proof
pub struct ProvingKey(zk::proof::ProvingKey);

#[pymethods]
impl ProvingKey {
    #[staticmethod]
    fn build(k: u32, circuit: &PyCell<ZkCircuit>) -> Self {
        let circuit_ref = circuit.borrow();
        let circuit = &circuit_ref.deref().0;
        let pk = zk::proof::ProvingKey::build(k, circuit);
        Self(pk)
    }
}

#[pyclass]
/// A zkVM proof
pub struct Proof(zk::proof::Proof);

#[pymethods]
impl Proof {
    #[staticmethod]
    fn create(
        pk: &PyCell<ProvingKey>,
        circuits: Vec<&PyCell<ZkCircuit>>,
        instances: Vec<&PyCell<Fp>>,
    ) -> Self {
        let pk = pk.borrow().deref().0.clone();
        let circuits: Vec<zk::vm::ZkCircuit> =
            circuits.iter().map(|c| c.borrow().deref().0.clone()).collect();
        let instances: Vec<pallas::Base> = instances.iter().map(|i| i.borrow().deref().0).collect();

        let proof =
            zk::proof::Proof::create(&pk, circuits.as_slice(), instances.as_slice(), &mut OsRng)
                .unwrap();
        Self(proof)
    }

    fn verify(&self, vk: &PyCell<VerifyingKey>, instances: Vec<&PyCell<Fp>>) {
        let vk = vk.borrow().deref().0.clone();
        let instances: Vec<pallas::Base> = instances.iter().map(|i| i.borrow().deref().0).collect();
        self.0.verify(&vk, instances.as_slice()).unwrap();
    }
}

#[pyclass]
/// MockProver class used for fast proof creation and verification.
/// Doesn't offer any security and should not be used in production.
pub struct MockProver(zk::halo2::dev::MockProver<pallas::Base>);

#[pymethods]
impl MockProver {
    #[staticmethod]
    fn run(k: u32, circuit: &PyCell<ZkCircuit>, instances: Vec<&PyCell<Fp>>) -> Self {
        let circuit = circuit.borrow().deref().0.clone();
        let instances: Vec<pallas::Base> = instances.iter().map(|i| i.borrow().deref().0).collect();
        let prover = zk::halo2::dev::MockProver::run(k, &circuit, vec![instances]).unwrap();
        Self(prover)
    }

    fn verify(&self) {
        self.0.assert_satisfied();
    }
}

pub fn create_module(py: Python<'_>) -> PyResult<&PyModule> {
    let submod = PyModule::new(py, "zkas")?;

    submod.add_class::<ZkBinary>()?;
    submod.add_class::<ZkCircuit>()?;
    submod.add_class::<VerifyingKey>()?;
    submod.add_class::<ProvingKey>()?;
    submod.add_class::<Proof>()?;
    submod.add_class::<MockProver>()?;

    Ok(submod)
}
