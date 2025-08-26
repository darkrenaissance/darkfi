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

use darkfi::error::Result;
use darkfi_deployooor_contract::{model as deployooor_model, DeployFunction};
use darkfi_sdk::deploy;
use darkfi_serial::deserialize;
use pyo3::{
    prelude::{PyAnyMethods, PyModule, PyModuleMethods},
    Bound, PyResult, Python,
};

use super::{impl_py_methods, FunctionParams};

/// [`DeployFunction::DeployV1`] function call parameter's python bindings.
pub mod deploy_v1;
pub use deploy_v1::DeployParamsV1;

/// [`DeployFunction::LockV1`] function call parameter's python bindings.
pub mod lock_v1;
pub use lock_v1::LockParamsV1;

/// Decodes the parameters of a Deployooor contract function call.
pub fn decode_deployooor_function_params(
    function_index: u8,
    data: &[u8],
) -> Result<Box<dyn FunctionParams>> {
    let res: Box<dyn FunctionParams> = match DeployFunction::try_from(function_index)? {
        DeployFunction::DeployV1 => {
            let params: deploy::DeployParamsV1 = deserialize(&data[1..])?;
            Box::new(params)
        }
        DeployFunction::LockV1 => {
            let params: deployooor_model::LockParamsV1 = deserialize(&data[1..])?;
            Box::new(params)
        }
    };

    Ok(res)
}

/// Create deployooor module and provide the python bindings.
pub fn create_module(py: Python) -> PyResult<Bound<PyModule>> {
    let submod = PyModule::new(py, "deployooor")?;

    submod.add_class::<DeployParamsV1>()?;
    submod.add_class::<LockParamsV1>()?;

    py.import("sys")?.getattr("modules")?.set_item("darkfi_sdk.contract.deployooor", &submod)?;

    Ok(submod)
}
