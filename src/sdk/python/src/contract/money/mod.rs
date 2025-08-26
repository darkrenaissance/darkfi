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

use darkfi_money_contract::{model as money_model, MoneyFunction};
use darkfi_sdk::crypto::util::FieldElemAsStr;
use darkfi_serial::deserialize;
use pyo3::{
    prelude::{PyAnyMethods, PyDictMethods, PyModule, PyModuleMethods},
    pyclass,
    types::PyDict,
    Bound, Py, PyResult, Python,
};

use crate::crypto::AeadEncryptedNote;

use super::{impl_py_methods, FunctionParams};

/// [`MoneyFunction::AuthTokenFreezeV1`] function call parameter's python bindings.
pub mod auth_token_freeze_v1;
pub use auth_token_freeze_v1::MoneyAuthTokenFreezeParamsV1;

/// [`MoneyFunction::AuthTokenMintV1`] function call parameter's python bindings.
pub mod auth_token_mint_v1;
pub use auth_token_mint_v1::MoneyAuthTokenMintParamsV1;

/// [`MoneyFunction::FeeV1`] function call parameter's python bindings.
pub mod fee_v1;
pub use fee_v1::MoneyFeeParamsV1;

/// [`MoneyFunction::GenesisMintV1`] function call parameter's python bindings.
pub mod genesis_mint_v1;
pub use genesis_mint_v1::MoneyGenesisMintParamsV1;

/// [`MoneyFunction::PoWRewardV1`] function call parameter's python bindings.
pub mod pow_reward_v1;
pub use pow_reward_v1::MoneyPoWRewardParamsV1;

/// [`MoneyFunction::TokenMintV1`] function call parameter's bindings.
pub mod token_mint_v1;
pub use token_mint_v1::MoneyTokenMintParamsV1;

/// [`MoneyFunction::TransferV1`] function call parameter's bindings.
pub mod transfer_v1;
pub use transfer_v1::MoneyTransferParamsV1;

/// Decodes the parameters of a Money contract function call.
pub fn decode_money_function_params(
    function_index: u8,
    data: &[u8],
) -> darkfi::Result<Box<dyn FunctionParams>> {
    let res: Box<dyn FunctionParams> = match MoneyFunction::try_from(function_index)? {
        MoneyFunction::FeeV1 => {
            let params: money_model::MoneyFeeParamsV1 = deserialize(&data[9..])?;
            Box::new(params)
        }
        MoneyFunction::GenesisMintV1 => {
            let params: money_model::MoneyGenesisMintParamsV1 = deserialize(&data[1..])?;
            Box::new(params)
        }
        MoneyFunction::PoWRewardV1 => {
            let params: money_model::MoneyPoWRewardParamsV1 = deserialize(&data[1..])?;
            Box::new(params)
        }
        MoneyFunction::TransferV1 | MoneyFunction::OtcSwapV1 => {
            let params: money_model::MoneyTransferParamsV1 = deserialize(&data[1..])?;
            Box::new(params)
        }
        MoneyFunction::AuthTokenMintV1 => {
            let params: money_model::MoneyAuthTokenMintParamsV1 = deserialize(&data[1..])?;
            Box::new(params)
        }
        MoneyFunction::AuthTokenFreezeV1 => {
            let params: money_model::MoneyAuthTokenFreezeParamsV1 = deserialize(&data[1..])?;
            Box::new(params)
        }
        MoneyFunction::TokenMintV1 => {
            let params: money_model::MoneyTokenMintParamsV1 = deserialize(&data[1..])?;
            Box::new(params)
        }
    };

    Ok(res)
}

/// [`money_model::Input`] python binding
#[pyclass]
pub struct Input(money_model::Input);
impl_py_methods!(Input);

impl FunctionParams for money_model::Input {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("value_commit", format!("{:?}", self.value_commit))?;
        dict.set_item("token_commit", self.token_commit.to_string())?;
        dict.set_item("nullifier", self.nullifier.to_string())?;
        dict.set_item("merkle_root", self.merkle_root.to_string())?;
        dict.set_item("user_data_enc", self.user_data_enc.to_string())?;
        dict.set_item("signature_public", self.signature_public.to_string())?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}value_commit: {:?}", self.value_commit).unwrap();
        writeln!(out, "{prefix}token_commit: {}", self.token_commit.to_string()).unwrap();
        writeln!(out, "{prefix}nullifier: {}", self.nullifier).unwrap();
        writeln!(out, "{prefix}merkle_root: {}", self.merkle_root).unwrap();
        writeln!(out, "{prefix}user_data_enc: {}", self.user_data_enc.to_string()).unwrap();
        writeln!(out, "{prefix}signature_public: {}", self.signature_public).unwrap();
        Ok(())
    }
}

/// [`money_model::Output`] python binding
#[pyclass]
pub struct Output(money_model::Output);
impl_py_methods!(Output);

impl FunctionParams for money_model::Output {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("value_commit", format!("{:?}", self.value_commit))?;
        dict.set_item("token_commit", self.token_commit.to_string())?;
        dict.set_item("coin", self.coin.to_string())?;
        dict.set_item("note", self.note.to_pydict(py)?)?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}value_commit: {:?}", self.value_commit).unwrap();
        writeln!(out, "{prefix}token_commit: {}", self.token_commit.to_string()).unwrap();
        writeln!(out, "{prefix}coin: {}", self.coin).unwrap();
        writeln!(out, "{prefix}note:").unwrap();
        self.note.fmt_pretty(out, depth + 2)?;
        Ok(())
    }
}

/// [`money_model::ClearInput`] python binding
#[pyclass]
pub struct ClearInput(money_model::ClearInput);
impl_py_methods!(ClearInput);

impl FunctionParams for money_model::ClearInput {
    fn to_pydict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("value", self.value)?;
        dict.set_item("token_id", self.token_id.to_string())?;
        dict.set_item("value_blind", self.value_blind.to_string())?;
        dict.set_item("token_blind", self.token_blind.to_string())?;
        dict.set_item("signature_public", self.signature_public.to_string())?;
        Ok(dict.unbind())
    }

    fn fmt_pretty(&self, out: &mut String, depth: usize) -> PyResult<()> {
        let prefix = format!("{}├─ ", "   ".repeat(depth));
        writeln!(out, "{prefix}value: {}", self.value).unwrap();
        writeln!(out, "{prefix}token_id: {}", self.token_id).unwrap();
        writeln!(out, "{prefix}value_blind: {}", self.value_blind).unwrap();
        writeln!(out, "{prefix}token_blind: {}", self.token_blind).unwrap();
        writeln!(out, "{prefix}signature_public: {}", self.signature_public).unwrap();
        Ok(())
    }
}

/// Create money module and provide the python bindings.
pub fn create_module(py: Python) -> PyResult<Bound<PyModule>> {
    let submod = PyModule::new(py, "money")?;

    submod.add_class::<MoneyAuthTokenFreezeParamsV1>()?;
    submod.add_class::<MoneyAuthTokenMintParamsV1>()?;
    submod.add_class::<MoneyFeeParamsV1>()?;
    submod.add_class::<MoneyGenesisMintParamsV1>()?;
    submod.add_class::<MoneyPoWRewardParamsV1>()?;
    submod.add_class::<MoneyTokenMintParamsV1>()?;
    submod.add_class::<MoneyTransferParamsV1>()?;
    submod.add_class::<Input>()?;
    submod.add_class::<Output>()?;
    submod.add_class::<ClearInput>()?;
    submod.add_class::<AeadEncryptedNote>()?;

    py.import("sys")?.getattr("modules")?.set_item("darkfi_sdk.contract.money", &submod)?;

    Ok(submod)
}
