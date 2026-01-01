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
use darkfi_sdk::crypto::util::FieldElemAsStr;
use pyo3::{prelude::PyDictMethods, pyclass, types::PyDict, Py, PyResult, Python};

use super::{impl_py_methods, FunctionParams};

/// [`dao_model::DaoProposeParams`] python binding.
#[pyclass]
pub struct DaoProposeParams(dao_model::DaoProposeParams);
impl_py_methods!(DaoProposeParams);

impl FunctionParams for dao_model::DaoProposeParams {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("dao_merkle_root", self.dao_merkle_root.to_string())?;
        dict.set_item("token_commit", self.token_commit.to_string())?;
        dict.set_item("proposal_bulla", self.proposal_bulla.to_string())?;
        dict.set_item("note", self.note.to_pydict(py)?)?;
        dict.set_item(
            "inputs",
            self.inputs
                .iter()
                .map(|input| input.to_pydict(py))
                .collect::<PyResult<Vec<Py<PyDict>>>>()?,
        )?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}dao_merkle_root: {}", self.dao_merkle_root).unwrap();
        writeln!(out, "{prefix}token_commit: {}", self.dao_merkle_root).unwrap();
        writeln!(out, "{prefix}proposal_bulla: {}", self.dao_merkle_root).unwrap();
        writeln!(out, "{prefix}note:").unwrap();
        self.note.fmt_pretty(out, depth + 2)?;

        writeln!(out, "{prefix}inputs:").unwrap();

        for input in &self.inputs {
            input.fmt_pretty(out, depth + 2)?;
            writeln!(out).unwrap();
        }
        Ok(())
    }
}

/// [`dao_model::DaoProposeParamsInput`] python binding.
#[pyclass]
pub struct DaoProposeParamsInput(dao_model::DaoProposeParamsInput);
impl_py_methods!(DaoProposeParamsInput);

impl FunctionParams for dao_model::DaoProposeParamsInput {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("value_commit", format!("{:?}", self.value_commit))?;
        dict.set_item("merkle_coin_root", self.merkle_coin_root.to_string())?;
        dict.set_item("smt_null_root", self.smt_null_root.to_string())?;
        dict.set_item("signature_public", self.signature_public.to_string())?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}value_commit: {:?}", self.value_commit).unwrap();
        writeln!(out, "{prefix}merkle_coin_root: {}", self.merkle_coin_root).unwrap();
        writeln!(out, "{prefix}smt_null_root: {:?}", self.smt_null_root).unwrap();
        writeln!(out, "{prefix}signature_public: {:?}", self.signature_public).unwrap();
        Ok(())
    }
}
