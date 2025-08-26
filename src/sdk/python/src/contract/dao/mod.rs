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

use darkfi::Result;
use darkfi_dao_contract::{model as dao_model, DaoFunction};
use darkfi_serial::deserialize;
use pyo3::{
    prelude::{PyAnyMethods, PyModule, PyModuleMethods},
    Bound, PyResult, Python,
};

use crate::crypto::{ElGamalEncryptedNote3, ElGamalEncryptedNote4, ElGamalEncryptedNote5};

use super::{impl_py_methods, FunctionParams};

/// [`DaoFunction::Mint`] function call parameter's python bindings.
pub mod mint;

/// [`DaoFunction::Propose`] function call parameter's python bindings.
pub mod propose;

/// [`DaoFunction::Vote`] function call parameter's python bindings.
pub mod vote;

/// [`DaoFunction::Exec`] function call parameter's python bindings.
pub mod exec;

/// [`DaoFunction::AuthMoneyTransfer`] function call parameter's python bindings.
pub mod auth_xfer;

/// Decodes the parameters of a DAO contract function call.
pub fn decode_dao_function_params(
    function_index: u8,
    data: &[u8],
) -> Result<Box<dyn FunctionParams>> {
    let res: Box<dyn FunctionParams> = match DaoFunction::try_from(function_index)? {
        DaoFunction::Mint => {
            let params: dao_model::DaoMintParams = deserialize(&data[1..])?;
            Box::new(params)
        }
        DaoFunction::Propose => {
            let params: dao_model::DaoProposeParams = deserialize(&data[1..])?;
            Box::new(params)
        }
        DaoFunction::Vote => {
            let params: dao_model::DaoVoteParams = deserialize(&data[1..])?;
            Box::new(params)
        }
        DaoFunction::Exec => {
            let params: dao_model::DaoExecParams = deserialize(&data[1..])?;
            Box::new(params)
        }
        DaoFunction::AuthMoneyTransfer => {
            let params: dao_model::DaoAuthMoneyTransferParams = deserialize(&data[1..])?;
            Box::new(params)
        }
    };

    Ok(res)
}

/// Create dao module and provide the python bindings.
pub fn create_module(py: Python) -> PyResult<Bound<PyModule>> {
    let submod = PyModule::new(py, "dao")?;

    submod.add_class::<mint::DaoMintParams>()?;
    submod.add_class::<propose::DaoProposeParams>()?;
    submod.add_class::<propose::DaoProposeParamsInput>()?;
    submod.add_class::<vote::DaoVoteParams>()?;
    submod.add_class::<vote::DaoVoteParamsInput>()?;
    submod.add_class::<exec::DaoExecParams>()?;
    submod.add_class::<exec::DaoAuthCall>()?;
    submod.add_class::<exec::DaoBlindAggregateVote>()?;
    submod.add_class::<auth_xfer::DaoAuthMoneyTransferParams>()?;
    submod.add_class::<ElGamalEncryptedNote3>()?;
    submod.add_class::<ElGamalEncryptedNote4>()?;
    submod.add_class::<ElGamalEncryptedNote5>()?;

    py.import("sys")?.getattr("modules")?.set_item("darkfi_sdk.contract.dao", &submod)?;

    Ok(submod)
}
