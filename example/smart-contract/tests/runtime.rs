/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
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

use darkfi::{
    blockchain::Blockchain,
    consensus::{TESTNET_GENESIS_HASH_BYTES, TESTNET_GENESIS_TIMESTAMP},
    runtime::vm_runtime::Runtime,
    Result,
};
use darkfi_sdk::{crypto::ContractId, pasta::pallas, tx::FuncCall};
use darkfi_serial::{serialize, Encodable, WriteExt};

use smart_contract::{FooCallData, Function};

#[test]
fn run_contract() -> Result<()> {
    // Debug log configuration
    let mut cfg = simplelog::ConfigBuilder::new();
    cfg.add_filter_ignore("sled".to_string());
    simplelog::TermLogger::init(
        simplelog::LevelFilter::Debug,
        cfg.build(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    )?;

    // =============================
    // Initialize a dummy blockchain
    // =============================
    // TODO: This blockchain interface should perhaps be ValidatorState and Mutex/RwLock.
    let db = sled::Config::new().temporary(true).open()?;
    let blockchain = Blockchain::new(&db, *TESTNET_GENESIS_TIMESTAMP, *TESTNET_GENESIS_HASH_BYTES)?;

    // ================================================================
    // Load the wasm binary into memory and create an execution runtime
    // ================================================================
    let wasm_bytes = std::fs::read("contract.wasm")?;
    let contract_id = ContractId::new(pallas::Base::from(1));
    let mut runtime = Runtime::new(&wasm_bytes, blockchain, contract_id)?;

    // Deploy function to initialize the smart contract state.
    // Here we pass an empty payload, but it's possible to feed in arbitrary data.
    runtime.deploy(&[])?;

    // =============================================
    // Build some kind of payload to show an example
    // =============================================
    let func_calls = vec![FuncCall {
        contract_id: pallas::Base::from(110),
        func_id: pallas::Base::from(4),
        call_data: serialize(&FooCallData { a: 777, b: 666 }),
    }];
    let func_call_index: u32 = 0;

    let mut payload = Vec::new();
    // Selects which path executes in the contract.
    payload.write_u8(Function::Foo as u8)?;
    // Write the actual payload data
    payload.write_u32(func_call_index)?;
    func_calls.encode(&mut payload)?;

    // ============================================================
    // Serialize the payload into the runtime format and execute it
    // ============================================================
    runtime.exec(&payload)?;

    // =====================================================
    // If exec was successful, try to apply the state change
    // =====================================================
    runtime.apply()?;

    Ok(())
}
