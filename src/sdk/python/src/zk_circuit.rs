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

use darkfi::zk::{halo2::Value, vm, vm_heap::empty_witnesses};
use darkfi_sdk::crypto::MerkleNode;
use pyo3::prelude::*;

use super::{base::Base, point::Point, scalar::Scalar, zk_binary::ZkBinary};

#[pyclass]
pub struct ZkCircuit(pub(crate) vm::ZkCircuit, pub(crate) Vec<vm::Witness>);

/// Like Builder Object
#[pymethods]
impl ZkCircuit {
    #[new]
    fn new(circuit_code: &PyCell<ZkBinary>) -> Self {
        let circuit_code = circuit_code.borrow().deref().0.clone();
        // DUMMY CIRCUIT
        let circuit = vm::ZkCircuit::new(vec![], &circuit_code);
        Self(circuit, vec![])
    }

    fn build(&self, circuit_code: &PyCell<ZkBinary>) -> Self {
        let circuit_code = circuit_code.borrow().deref().0.clone();
        let circuit = vm::ZkCircuit::new(self.1.clone(), &circuit_code);
        Self(circuit, self.1.clone())
    }

    fn verifier_build(&self, circuit_code: &PyCell<ZkBinary>) -> Self {
        let circuit_code = circuit_code.borrow().deref().0.clone();
        let circuit = vm::ZkCircuit::new(empty_witnesses(&circuit_code), &circuit_code);
        Self(circuit, self.1.clone())
    }

    fn witness_point(&mut self, v: &PyCell<Point>) {
        let v = v.borrow();
        let v = v.deref();
        self.1.push(vm::Witness::EcPoint(Value::known(v.0)));
    }

    fn witness_ni_point(&mut self, v: &PyCell<Point>) {
        let v = v.borrow();
        let v = v.deref();
        self.1.push(vm::Witness::EcNiPoint(Value::known(v.0)));
    }

    fn witness_fixed_point(&mut self, v: &PyCell<Point>) {
        let v = v.borrow();
        let v = v.deref();
        self.1.push(vm::Witness::EcFixedPoint(Value::known(v.0)));
    }

    fn witness_scalar(&mut self, v: &PyCell<Scalar>) {
        let v = v.borrow();
        let v = v.deref();
        self.1.push(vm::Witness::Scalar(Value::known(v.0)));
    }

    fn witness_base(&mut self, v: &PyCell<Base>) {
        let v = v.borrow();
        let v = v.deref();
        self.1.push(vm::Witness::Base(Value::known(v.0)));
    }

    fn witness_merkle_path(&mut self, v: Vec<&PyCell<Base>>) {
        let v: Vec<MerkleNode> = v.iter().map(|v| MerkleNode::new(v.borrow().deref().0)).collect();
        let v: [MerkleNode; 32] = v.try_into().unwrap();
        let v = Value::known(v);
        self.1.push(vm::Witness::MerklePath(v));
    }

    fn witness_u32(&mut self, v: u32) {
        self.1.push(vm::Witness::Uint32(Value::known(v)));
    }

    fn witness_u64(&mut self, v: u64) {
        self.1.push(vm::Witness::Uint64(Value::known(v)));
    }
}

pub fn create_module(py: pyo3::Python<'_>) -> pyo3::PyResult<&PyModule> {
    let submod = PyModule::new(py, "zk_circuit")?;
    submod.add_class::<ZkCircuit>()?;
    Ok(submod)
}
