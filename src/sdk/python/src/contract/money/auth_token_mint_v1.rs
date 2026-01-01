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

use darkfi_money_contract::model as money_model;
use pyo3::{prelude::PyDictMethods, pyclass, types::PyDict, Py, PyResult, Python};

use super::{impl_py_methods, FunctionParams};

/// [`money_model::MoneyAuthTokenMintParamsV1`] python binding.
#[pyclass]
pub struct MoneyAuthTokenMintParamsV1(money_model::MoneyAuthTokenMintParamsV1);
impl_py_methods!(MoneyAuthTokenMintParamsV1);

impl FunctionParams for money_model::MoneyAuthTokenMintParamsV1 {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("token_id", self.token_id.to_string())?;
        dict.set_item("enc_note", self.enc_note.to_pydict(py)?)?;
        dict.set_item("mint_pubkey", self.mint_pubkey.to_string())?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}token_id: {}", self.token_id).unwrap();
        writeln!(out, "{prefix}mint_pubkey: {}", self.mint_pubkey).unwrap();
        writeln!(out, "{prefix}enc_note:").unwrap();
        self.enc_note.fmt_pretty(out, depth + 2)?;
        Ok(())
    }
}
