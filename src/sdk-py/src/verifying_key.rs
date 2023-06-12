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

use crate::zk_circuit::ZkCircuit;
use darkfi::zk::proof;
use pyo3::prelude::*;
use std::ops::Deref;

#[pyclass]
pub struct VerifyingKey(pub(crate) proof::VerifyingKey);

#[pymethods]
impl VerifyingKey {
    #[staticmethod]
    fn build(k: u32, circuit: &PyCell<ZkCircuit>) -> Self {
        let circuit_ref = circuit.borrow();
        let circuit = &circuit_ref.deref().0;
        let proving_key = proof::VerifyingKey::build(k, circuit);
        Self(proving_key)
    }
}

pub fn create_module(py: pyo3::Python<'_>) -> pyo3::PyResult<&PyModule> {
    let submod = PyModule::new(py, "verifying_key")?;
    submod.add_class::<VerifyingKey>()?;
    Ok(submod)
}
