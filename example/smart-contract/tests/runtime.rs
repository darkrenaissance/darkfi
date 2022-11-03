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
    crypto::contract_id::ContractId,
    runtime::{util::serialize_payload, vm_runtime::Runtime},
    Result,
};
use darkfi_sdk::{crypto::nullifier::Nullifier, pasta::pallas};
use darkfi_serial::serialize;

use smart_contract::FooArgs;

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
    // =============================================================
    // Build a ledger state so the runtime has something to work on
    // =============================================================
    //let state_machine = State::dummy()?;

    // Add a nullifier to the nullifier set. (This is checked by the contract)
    //state_machine.nullifiers.insert(&[Nullifier::from(pallas::Base::from(0x10))])?;

    // ================================================================
    // Load the wasm binary into memory and create an execution runtime
    // ================================================================
    let wasm_bytes = std::fs::read("contract.wasm")?;
    let contract_id = ContractId::new(pallas::Base::from(1));
    let mut runtime = Runtime::new(&wasm_bytes, contract_id)?;

    runtime.deploy()?;

    // =============================================
    // Build some kind of payload to show an example
    // =============================================
    let args = FooArgs { a: 777, b: 666 };
    // Prepend the func id
    let mut payload = vec![0x00];
    payload.extend_from_slice(&serialize(&args));

    // ============================================================
    // Serialize the payload into the runtime format and execute it
    // ============================================================
    runtime.exec(&serialize_payload(&payload))?;

    runtime.apply()?;
    //Ok(())

    //runtime.exec(&serialize_payload(&payload))?;

    //runtime.apply()?;

    Ok(())
}
