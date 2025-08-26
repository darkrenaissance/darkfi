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

use pyo3::prelude::PyAnyMethods;

/// Pallas and Vesta curves
mod pasta;

/// Merkle tree utilities
mod merkle;

/// Cryptographic utilities
mod crypto;

/// zkas definitions
mod zkas;

/// Contract definitions
mod contract;

/// Transaction definitions
mod tx;

#[pyo3::prelude::pymodule]
fn darkfi_sdk(
    py: pyo3::Python<'_>,
    m: &pyo3::Bound<'_, pyo3::prelude::PyModule>,
) -> pyo3::PyResult<()> {
    let submodule = pasta::create_module(py)?;
    py.import("sys")?.getattr("modules")?.set_item("darkfi_sdk.pasta", &submodule)?;
    pyo3::types::PyModuleMethods::add_submodule(m, &submodule)?;

    let submodule = merkle::create_module(py)?;
    py.import("sys")?.getattr("modules")?.set_item("darkfi_sdk.merkle", &submodule)?;
    pyo3::types::PyModuleMethods::add_submodule(m, &submodule)?;

    let submodule = crypto::create_module(py)?;
    py.import("sys")?.getattr("modules")?.set_item("darkfi_sdk.crypto", &submodule)?;
    pyo3::types::PyModuleMethods::add_submodule(m, &submodule)?;

    let submodule = zkas::create_module(py)?;
    py.import("sys")?.getattr("modules")?.set_item("darkfi_sdk.zkas", &submodule)?;
    pyo3::types::PyModuleMethods::add_submodule(m, &submodule)?;

    let submodule = tx::create_module(py)?;
    py.import("sys")?.getattr("modules")?.set_item("darkfi_sdk.tx", &submodule)?;
    pyo3::types::PyModuleMethods::add_submodule(m, &submodule)?;

    let submodule = contract::create_module(py)?;
    py.import("sys")?.getattr("modules")?.set_item("darkfi_sdk.contract", &submodule)?;
    pyo3::types::PyModuleMethods::add_submodule(m, &submodule)?;

    Ok(())
}
