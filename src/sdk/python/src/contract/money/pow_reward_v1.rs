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
 * but WITHOUT ANY WARRANTY; without even the impl FunctionParams foried warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::fmt::Write;

use darkfi_money_contract::model as money_model;
use pyo3::{prelude::PyDictMethods, pyclass, types::PyDict, Py, PyResult, Python};

use super::{impl_py_methods, FunctionParams};

/// [`money_model::MoneyPoWRewardParamsV1`] python binding.
#[pyclass]
pub struct MoneyPoWRewardParamsV1(money_model::MoneyPoWRewardParamsV1);
impl_py_methods!(MoneyPoWRewardParamsV1);

impl FunctionParams for money_model::MoneyPoWRewardParamsV1 {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("input", self.input.to_pydict(py)?)?;
        dict.set_item("output", self.output.to_pydict(py)?)?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}input:").unwrap();
        self.input.fmt_pretty(out, depth + 2)?;
        writeln!(out, "{prefix}output:").unwrap();
        self.output.fmt_pretty(out, depth + 2)?;
        Ok(())
    }
}
