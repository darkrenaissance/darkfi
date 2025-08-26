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

use darkfi::Result;
use darkfi_dao_contract::{model as dao_model, DaoFunction};
use darkfi_money_contract::{model as money_model, MoneyFunction};
use darkfi_sdk::crypto::{ContractId, DAO_CONTRACT_ID, MONEY_CONTRACT_ID};
use darkfi_serial::deserialize;
use pyo3::{
    exceptions::PyValueError, prelude::PyDictMethods, pyclass, pymethods, types::PyDict, Py,
    PyResult, Python,
};

use super::{impl_py_methods, FunctionParams};

/// [`dao_model::DaoExecParams`] python binding.
#[pyclass]
pub struct DaoExecParams(dao_model::DaoExecParams);
impl_py_methods!(DaoExecParams);

impl FunctionParams for dao_model::DaoExecParams {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("proposal_bulla", self.proposal_bulla.to_string())?;
        dict.set_item(
            "proposal_auth_calls",
            self.proposal_auth_calls
                .iter()
                .map(|auth_call| {
                    DaoAuthCallDecoded::new(auth_call)
                        .map_err(|e| PyValueError::new_err(e.to_string()))?
                        .to_pydict(py)
                })
                .collect::<PyResult<Vec<Py<PyDict>>>>()?,
        )?;
        dict.set_item("blind_total_vote", self.blind_total_vote.to_pydict(py)?)?;
        dict.set_item("early_exec", self.early_exec)?;
        dict.set_item("signature_public", self.signature_public.to_string())?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}proposal_bulla: {}", self.proposal_bulla).unwrap();
        writeln!(out, "{prefix}early_exec: {}", self.early_exec).unwrap();
        writeln!(out, "{prefix}signature_public: {}", self.signature_public).unwrap();
        writeln!(out, "{prefix}blind_total_vote:").unwrap();
        self.blind_total_vote.fmt_pretty(out, depth + 2)?;

        writeln!(out, "{prefix}proposal_auth_calls:").unwrap();
        for auth_call in &self.proposal_auth_calls {
            DaoAuthCallDecoded::new(auth_call)
                .map_err(|e| PyValueError::new_err(e.to_string()))?
                .fmt_pretty(out, depth + 2)?;
            writeln!(out).unwrap();
        }
        Ok(())
    }
}

/// [`dao_model::DaoBlindAggregateVote`] python binding.
#[pyclass]
pub struct DaoBlindAggregateVote(dao_model::DaoBlindAggregateVote);
impl_py_methods!(DaoBlindAggregateVote);

impl FunctionParams for dao_model::DaoBlindAggregateVote {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("yes_vote_commit", format!("{:?}", self.yes_vote_commit))?;
        dict.set_item("all_vote_commit", format!("{:?}", self.all_vote_commit))?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}yes_vote_commit: {:?}", self.yes_vote_commit).unwrap();
        writeln!(out, "{prefix}all_vote_commit: {:?}", self.all_vote_commit).unwrap();
        Ok(())
    }
}

/// [`dao_model::DaoAuthCall`] python binding.
#[pyclass]
pub struct DaoAuthCall(dao_model::DaoAuthCall);

#[pymethods]
impl DaoAuthCall {
    #[getter]
    pub fn __dict__(&self, py: Python) -> PyResult<Py<PyDict>> {
        DaoAuthCallDecoded::new(&self.0)
            .map_err(|e| PyValueError::new_err(e.to_string()))?
            .to_pydict(py)
    }

    pub fn __str__(&self) -> PyResult<String> {
        let mut out = String::new();
        DaoAuthCallDecoded::new(&self.0)
            .map_err(|e| PyValueError::new_err(e.to_string()))?
            .fmt_pretty(&mut out, 0)?;
        Ok(out)
    }
}

/// Decoded representation of [`dao_model::DaoAuthCall`].
pub struct DaoAuthCallDecoded {
    contract_id: ContractId,
    contract_name: String,
    function_code: u8,
    function_name: String,
    auth_data: Vec<money_model::Coin>,
}

impl DaoAuthCallDecoded {
    fn new(call: &dao_model::DaoAuthCall) -> Result<Self> {
        let (contract_name, function_name) = if call.contract_id == *MONEY_CONTRACT_ID {
            (
                "Money".to_string(),
                MoneyFunction::try_from(call.function_code).map(|f| format!("{f:?}"))?,
            )
        } else if call.contract_id == *DAO_CONTRACT_ID {
            (
                "Dao".to_string(),
                DaoFunction::try_from(call.function_code).map(|f| format!("{f:?}"))?,
            )
        } else {
            // TODO: Add support for decoding custom contract calls
            ("Unknown".to_string(), "Unknown".to_string())
        };

        let proposal_coins =
            if !call.auth_data.is_empty() { deserialize(&call.auth_data[..])? } else { vec![] };

        Ok(Self {
            contract_id: call.contract_id,
            contract_name,
            function_code: call.function_code,
            function_name,
            auth_data: proposal_coins,
        })
    }
}

impl FunctionParams for DaoAuthCallDecoded {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);

        dict.set_item("contract_id", self.contract_id.to_string())?;
        dict.set_item("contract_name", &self.contract_name)?;
        dict.set_item("function_code", self.function_code)?;
        dict.set_item("function_name", &self.function_name)?;
        dict.set_item(
            "auth_data",
            self.auth_data.iter().map(|c| c.to_string()).collect::<Vec<_>>(),
        )?;

        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}contract_id: {}", self.contract_id).unwrap();
        writeln!(out, "{prefix}contract_name: {}", self.contract_name).unwrap();
        writeln!(out, "{prefix}function_code: {}", self.function_code).unwrap();
        writeln!(out, "{prefix}function_name: {}", self.function_name).unwrap();

        if !self.auth_data.is_empty() {
            writeln!(out, "{prefix}auth_data:").unwrap();

            for coin in &self.auth_data {
                writeln!(out, "   {prefix}{coin}").unwrap();
            }
        }
        Ok(())
    }
}
