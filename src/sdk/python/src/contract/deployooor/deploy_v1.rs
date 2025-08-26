/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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

use std::fmt::Write;

use darkfi_sdk::{deploy, hex::AsHex};
use pyo3::{prelude::PyDictMethods, pyclass, types::PyDict, Py, PyResult, Python};

use super::{impl_py_methods, FunctionParams};

/// [`deploy::DeployParamsV1`] python binding.
#[pyclass]
pub struct DeployParamsV1(deploy::DeployParamsV1);
impl_py_methods!(DeployParamsV1);

impl FunctionParams for deploy::DeployParamsV1 {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("public_key", self.public_key.to_string())?;
        dict.set_item("wasm_bindcode", self.wasm_bincode.hex())?;
        dict.set_item("ix", self.ix.hex())?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}public_key: {}", self.public_key).unwrap();
        writeln!(out, "{prefix}wasm_bincode: [{} bytes]", &self.wasm_bincode.len()).unwrap();
        writeln!(out, "{prefix}ix: [{} bytes]", &self.ix.len()).unwrap();
        Ok(())
    }
}
