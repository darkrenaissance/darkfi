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

use crate::{
    base::Base, proving_key::ProvingKey, verifying_key::VerifyingKey, zk_circuit::ZkCircuit,
};
use darkfi::zk::{proof, vm};
use pasta_curves::pallas;
use pyo3::prelude::*;
use rand::rngs::OsRng;
use std::ops::Deref;

#[pyclass]
pub struct Proof(pub(crate) proof::Proof);

#[pymethods]
impl Proof {
    #[staticmethod]
    fn create(
        pk: &PyCell<ProvingKey>,
        circuits: Vec<&PyCell<ZkCircuit>>,
        instances: Vec<&PyCell<Base>>,
    ) -> Self {
        let pk = pk.borrow().deref().0.clone();
        let circuits: Vec<vm::ZkCircuit> =
            circuits.iter().map(|c| c.borrow().deref().0.clone()).collect();
        let instances: Vec<pallas::Base> = instances.iter().map(|i| i.borrow().deref().0).collect();
        let proof =
            proof::Proof::create(&pk, circuits.as_slice(), instances.as_slice(), &mut OsRng);
        let proof = proof.unwrap();
        Self(proof)
    }

    fn verify(&self, vk: &PyCell<VerifyingKey>, instances: Vec<&PyCell<Base>>) {
        let vk = vk.borrow().deref().0.clone();
        let proof = self.0.clone();
        let instances: Vec<pallas::Base> = instances.iter().map(|i| i.borrow().deref().0).collect();
        proof.verify(&vk, instances.as_slice()).unwrap();
    }
}

pub fn create_module(py: pyo3::Python<'_>) -> pyo3::PyResult<&PyModule> {
    let submod = PyModule::new(py, "proof")?;
    submod.add_class::<Proof>()?;
    Ok(submod)
}
