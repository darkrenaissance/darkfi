/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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

use darkfi_sdk::{crypto::ContractId, error::ContractResult};

darkfi_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

fn init_contract(_cid: ContractId, _ix: &[u8]) -> ContractResult {
    //panic!("ohai");
    Ok(())
}

fn get_metadata(_cid: ContractId, _ix: &[u8]) -> ContractResult {
    Ok(())
}

fn process_instruction(_cid: ContractId, _ix: &[u8]) -> ContractResult {
    Ok(())
}

fn process_update(_cid: ContractId, _ix: &[u8]) -> ContractResult {
    Ok(())
}
