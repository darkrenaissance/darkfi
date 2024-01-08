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

use darkfi::{
    blockchain::{Blockchain, BlockchainOverlay},
    runtime::vm_runtime::Runtime,
    util::time::{TimeKeeper, Timestamp},
};
use darkfi_sdk::{crypto::ContractId, pasta::pallas};

fn init_logger() {
    let mut cfg = simplelog::ConfigBuilder::new();
    cfg.add_filter_ignore("sled".to_string());

    simplelog::TermLogger::init(
        simplelog::LevelFilter::Debug,
        cfg.build(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    )
    .unwrap();
}

fn main() {
    init_logger();

    let sled_db = sled::Config::new().temporary(true).open().unwrap();
    let blockchain = Blockchain::new(&sled_db).unwrap();
    let overlay = BlockchainOverlay::new(&blockchain).unwrap();

    let timekeeper = TimeKeeper::new(Timestamp::current_time(), 10, 90, 0);

    let wasm_bytes =
        include_bytes!("../target/wasm32-unknown-unknown/release/darkfi_dummy_contract.wasm");

    let contract_id = ContractId::from(pallas::Base::from(69));

    let mut runtime = Runtime::new(wasm_bytes, overlay, contract_id, timekeeper).unwrap();
    match runtime.deploy(&[]) {
        Ok(()) => {}
        Err(e) => println!("Error running deploy: {:#?}", e),
    }

    match runtime.exec(&[]) {
        Ok(_) => {}
        Err(e) => println!("Error running exec: {:#?}", e),
    }
}
