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

use darkfi::zkas::decoder;
use pyo3::prelude::*;

#[pyclass]
pub struct ZkBinary(pub(crate) decoder::ZkBinary);

/// There is no need for constants, as bindings for ec_mul_short and ec_mul_base
/// don't actually take the constants.
/// The constants are hardcoded on the Rust side.
#[pymethods]
impl ZkBinary {
    #[staticmethod]
    fn decode(bytes: Vec<u8>) -> Self {
        let bincode = decoder::ZkBinary::decode(bytes.as_slice()).unwrap();
        Self(bincode)
    }

    fn namespace(&self) -> String {
        self.0.namespace.clone()
    }

    fn literals(&self) -> Vec<(String, String)> {
        let l = self.0.literals.clone();
        l.iter().map(|(lit, value)| (format!("{lit:?}"), value.clone())).collect()
    }

    fn witnesses(&self) -> Vec<String> {
        let w = self.0.witnesses.clone();
        w.iter().map(|v| format!("{v:?}")).collect()
    }

    fn constant_count(&self) -> usize {
        self.0.constants.len()
    }

    fn opcodes(&self) -> Vec<(String, Vec<(String, usize)>)> {
        let o = self.0.opcodes.clone();
        o.iter()
            .map(|(opcode_, args_)| {
                let opcode = format!("{opcode_:?}");
                let args = args_
                    .iter()
                    .map(|(heap_type, heap_idx)| (format!("{heap_type:?}"), heap_idx.clone()))
                    .collect();
                (opcode, args)
            })
            .collect()
    }
}

pub fn create_module(py: pyo3::Python<'_>) -> pyo3::PyResult<&PyModule> {
    let submod = PyModule::new(py, "zk_binary")?;
    submod.add_class::<ZkBinary>()?;
    Ok(submod)
}
