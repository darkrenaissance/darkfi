/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the impl FunctionParams foried warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::fmt::Write;

use darkfi_dao_contract::model as dao_model;
use pyo3::{prelude::PyDictMethods, pyclass, types::PyDict, Py, PyResult, Python};

use super::{impl_py_methods, FunctionParams};

/// [`dao_model::DaoAuthMoneyTransferParams`] python binding.
#[pyclass]
pub struct DaoAuthMoneyTransferParams(dao_model::DaoAuthMoneyTransferParams);
impl_py_methods!(DaoAuthMoneyTransferParams);

impl FunctionParams for dao_model::DaoAuthMoneyTransferParams {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item(
            "enc_attrs",
            self.enc_attrs
                .iter()
                .map(|e| e.to_pydict(py))
                .collect::<PyResult<Vec<Py<PyDict>>>>()?,
        )?;
        dict.set_item("dao_change_attrs", self.dao_change_attrs.to_pydict(py)?)?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}dao_change_attrs:").unwrap();
        self.dao_change_attrs.fmt_pretty(out, depth + 2)?;

        writeln!(out, "{prefix}enc_attrs:").unwrap();

        for attr in &self.enc_attrs {
            attr.fmt_pretty(out, depth + 2)?;
            writeln!(out).unwrap();
        }
        Ok(())
    }
}
