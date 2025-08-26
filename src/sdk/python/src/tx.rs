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

use darkfi::tx::{MAX_TX_CALLS, MIN_TX_CALLS};
use darkfi_sdk::dark_tree::dark_forest_leaf_vec_integrity_check;
use darkfi_serial::deserialize;
use pyo3::{
    exceptions::PyValueError,
    prelude::{PyDictMethods, PyModule, PyModuleMethods},
    pyclass, pymethods,
    types::PyDict,
    Bound, Py, PyResult, Python,
};

use super::{
    contract::{ContractCall, DarkLeafContractCall},
    crypto::Signature,
    zkas::Proof,
};

/// Class representing a transaction
#[pyclass]
pub struct Transaction(darkfi::tx::Transaction);

#[pymethods]
impl Transaction {
    #[staticmethod]
    pub fn decode(data: Vec<u8>) -> PyResult<Self> {
        let tx: darkfi::tx::Transaction = deserialize(&data)?;
        dark_forest_leaf_vec_integrity_check(&tx.calls, Some(MIN_TX_CALLS), Some(MAX_TX_CALLS))
            .map_err(|e| {
                PyValueError::new_err(format!(
                    "Invalid Transaction, contract call integrity check failed: {e}"
                ))
            })?;
        Ok(Self(tx))
    }

    pub fn hash(&self) -> TransactionHash {
        TransactionHash(self.0.hash())
    }

    pub fn proofs(&self) -> Vec<Vec<Proof>> {
        self.0.proofs.iter().map(|inner| inner.iter().map(|p| Proof(p.clone())).collect()).collect()
    }

    pub fn signatures(&self) -> Vec<Vec<Signature>> {
        self.0
            .signatures
            .iter()
            .map(|inner| inner.iter().map(|s| Signature(*s)).collect())
            .collect()
    }

    pub fn calls(&self) -> Vec<DarkLeafContractCall> {
        self.0.calls.iter().map(|leaf| DarkLeafContractCall(leaf.clone())).collect()
    }

    /// Returns the transaction represented as a Python dictionary.
    #[getter]
    pub fn __dict__(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        let mut calls = vec![];
        for (i, call) in self.calls().iter().enumerate() {
            let call_dict = PyDict::new(py);
            call_dict.set_item("parent_index", call.parent_index())?;
            call_dict.set_item("children_indexes", call.children_indexes())?;

            let call_data = call.data();

            call_dict.set_item("contract_id", call_data.contract_id())?;
            call_dict.set_item("contract_name", call_data.contract_name())?;
            call_dict.set_item("function_index", call_data.function_index())?;
            call_dict.set_item("function_name", call_data.function_name())?;
            call_dict.set_item("function_params", call_data.function_params_dict(py)?)?;
            call_dict.set_item("proofs", self.0.proofs.get(i).unwrap().iter().len())?;
            call_dict.set_item(
                "signatures",
                self.0
                    .signatures
                    .get(i)
                    .unwrap()
                    .iter()
                    .map(|s| format!("{s:?}"))
                    .collect::<Vec<String>>(),
            )?;
            calls.push(call_dict);
        }

        dict.set_item("hash", self.0.hash().as_string())?;
        dict.set_item("calls", calls)?;
        Ok(dict.unbind())
    }

    /// A formatted text representation of a transaction, showing
    /// function calls in their call hierarchy.
    pub fn __str__(&self) -> PyResult<String> {
        let mut out = String::new();
        writeln!(out, "hash: {}\n", self.0.hash()).unwrap();

        let depth = self.compute_depth();
        for (i, call) in self.0.calls.iter().enumerate().rev() {
            write!(out, "{}", self.format_call(&ContractCall(call.data.clone()), depth[i])?)
                .unwrap();
        }

        Ok(out)
    }
}

impl Transaction {
    /// Calculate the depth of each node in the call tree.
    fn compute_depth(&self) -> Vec<usize> {
        let mut depth = vec![0; self.0.calls.len()];

        for (i, call) in self.0.calls.iter().enumerate().rev() {
            if let Some(parent) = call.parent_index {
                depth[i] = depth[parent] + 1;
            }
        }

        depth
    }

    fn format_call(&self, call: &ContractCall, depth: usize) -> PyResult<String> {
        let mut out = String::new();
        let prefix = format!("{}├─ ", "   ".repeat(depth + 1));
        writeln!(out, "{}⊟ Contract Call", "    ".repeat(depth)).unwrap();
        writeln!(out, "{}━━━━━━━━━━━━━━━", "    ".repeat(depth)).unwrap();

        writeln!(out, "{}contract id: {}", prefix, call.contract_id()).unwrap();
        writeln!(out, "{}contract_name: {}", prefix, call.contract_name().unwrap_or_default())
            .unwrap();
        writeln!(out, "{}function_index: {}", prefix, call.function_index()).unwrap();
        writeln!(out, "{}function_name: {}", prefix, call.function_name().unwrap_or_default())
            .unwrap();
        writeln!(out, "{}function_params:\n{}", prefix, call.function_params_str(depth + 2)?)
            .unwrap();

        Ok(out)
    }
}

#[pyclass]
pub struct TransactionHash(darkfi_sdk::tx::TransactionHash);

#[pymethods]
impl TransactionHash {
    pub fn __str__(&self) -> String {
        self.0.to_string()
    }
}

pub fn create_module(py: Python<'_>) -> PyResult<Bound<PyModule>> {
    let submod = PyModule::new(py, "tx")?;

    submod.add_class::<Transaction>()?;

    Ok(submod)
}
