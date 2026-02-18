/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use std::io::Cursor;

use darkfi_sdk::{crypto::MerkleNode, pasta::pallas, wasm};
use darkfi_serial::Decodable;
use tracing::error;
use wasmer::{FunctionEnvMut, WasmPtr};

use super::acl::acl_allow;
use crate::runtime::vm_runtime::{ContractSection, Env};

/// Checks if a given MerkleNode root is contained in known roots.
///
/// This function expects to receive a pointer from which the MerkleNode
/// will be read.
///
/// Permissions: exec
pub(crate) fn coin_roots_contains(
    mut ctx: FunctionEnvMut<Env>,
    ptr: WasmPtr<u8>,
    ptr_len: u32,
) -> i64 {
    let (env, mut store) = ctx.data_and_store_mut();
    let cid = env.contract_id;

    if let Err(e) = acl_allow(env, &[ContractSection::Exec]) {
        error!(
            target: "runtime::tx_local::coin_roots_contains",
            "[WASM] [{cid}] coin_roots_contains(): Called in unauthorized section: {e}",
        );
        return darkfi_sdk::error::CALLER_ACCESS_DENIED
    }

    // Subtract used gas.
    env.subtract_gas(&mut store, 1);

    // Create a mem slice of the wasm VM memory
    let memory_view = env.memory_view(&store);
    let Ok(mem_slice) = ptr.slice(&memory_view, ptr_len) else {
        error!(
            target: "runtime::tx_local::coin_roots_contains",
            "[WASM] [{cid}] coin_roots_contains(): Failed to make slice from ptr",
        );
        return darkfi_sdk::error::INTERNAL_ERROR
    };

    let mut buf = vec![0u8; ptr_len as usize];
    if let Err(e) = mem_slice.read_slice(&mut buf) {
        error!(
            target: "runtime::tx_local::coin_roots_contains",
            "[WASM] [{cid}] coin_roots_contains(): Failed to read memory slice: {e}",
        );
        return darkfi_sdk::error::INTERNAL_ERROR
    }

    let mut buf_reader = Cursor::new(buf);
    let read_root: MerkleNode = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::tx_local::coin_roots_contains",
                "[WASM] [{cid}] coin_roots_contains(): Failed decoding MerkleNode: {e}",
            );
            return darkfi_sdk::error::INTERNAL_ERROR
        }
    };

    // Make sure we've read the entire buffer
    if buf_reader.position() != ptr_len as u64 {
        error!(
            target: "runtime::tx_local::coin_roots_contains",
            "[WASM] [{cid}] coin_roots_contains(): Trailing bytes in argument stream",
        );
        return darkfi_sdk::error::INTERNAL_ERROR
    }

    let Ok(tx_state) = env.tx_local.try_borrow() else {
        error!(
            target: "runtime::tx_local::coin_roots_contains",
            "[WASM] [{cid}] coin_roots_contains(): Could not borrow tx-local state",
        );
        return darkfi_sdk::error::INTERNAL_ERROR
    };

    if tx_state.coin_roots.contains(&read_root) {
        // true
        return 1
    }

    // false
    0
}

/// Checks if a given coin is contained in new coins.
///
/// This function expects to receive a pointer from which the coin will
/// be read.
///
/// Permissions: exec
pub(crate) fn new_coins_contains(
    mut ctx: FunctionEnvMut<Env>,
    ptr: WasmPtr<u8>,
    ptr_len: u32,
) -> i64 {
    let (env, mut store) = ctx.data_and_store_mut();
    let cid = env.contract_id;

    if let Err(e) = acl_allow(env, &[ContractSection::Exec]) {
        error!(
            target: "runtime::tx_local::new_coins_contains",
            "[WASM] [{cid}] new_coins_contains(): Called in unauthorized section: {e}",
        );
        return darkfi_sdk::error::CALLER_ACCESS_DENIED
    }

    // Subtract used gas.
    env.subtract_gas(&mut store, 1);

    // Create a mem slice of the wasm VM memory
    let memory_view = env.memory_view(&store);
    let Ok(mem_slice) = ptr.slice(&memory_view, ptr_len) else {
        error!(
            target: "runtime::tx_local::new_coins_contains",
            "[WASM] [{cid}] new_coins_contains(): Failed to make slice from ptr",
        );
        return darkfi_sdk::error::INTERNAL_ERROR
    };

    let mut buf = vec![0u8; ptr_len as usize];
    if let Err(e) = mem_slice.read_slice(&mut buf) {
        error!(
            target: "runtime::tx_local::new_coins_contains",
            "[WASM] [{cid}] new_coins_contains(): Failed to read memory slice: {e}",
        );
        return darkfi_sdk::error::INTERNAL_ERROR
    }

    let mut buf_reader = Cursor::new(buf);
    let read_coin: pallas::Base = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::tx_local::new_coins_contains",
                "[WASM] [{cid}] new_coins_contains(): Failed decoding Coin: {e}",
            );
            return darkfi_sdk::error::INTERNAL_ERROR
        }
    };

    // Make sure we've read the entire buffer
    if buf_reader.position() != ptr_len as u64 {
        error!(
            target: "runtime::tx_local::new_coins_contains",
            "[WASM] [{cid}] new_coins_contains(): Trailing bytes in argument stream",
        );
        return darkfi_sdk::error::INTERNAL_ERROR
    }

    let Ok(tx_state) = env.tx_local.try_borrow() else {
        error!(
            target: "runtime::tx_local::new_coins_contains",
            "[WASM] [{cid}] new_coins_contains(): Could not borrow tx-local state",
        );
        return darkfi_sdk::error::INTERNAL_ERROR
    };

    if tx_state.new_coins.contains(&read_coin) {
        // true
        return 1
    }

    // false
    0
}

/// Append a coin to the transaction-local state
///
/// This function expects to receive a pointer from which the Coin will
/// be read.
///
/// Permissions: exec
pub(crate) fn append_coin(mut ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, ptr_len: u32) -> i64 {
    let (env, mut store) = ctx.data_and_store_mut();
    let cid = env.contract_id;

    if let Err(e) = acl_allow(env, &[ContractSection::Exec]) {
        error!(
            target: "runtime::tx_local::append_coin",
            "[WASM] [{cid}] append_coin(): Called in unauthorized section: {e}",
        );
        return darkfi_sdk::error::CALLER_ACCESS_DENIED
    }

    // Subtract used gas.
    // 1 for opcode, 99 for 3 times 33 bytes (tree, root, and coin)
    env.subtract_gas(&mut store, 100);

    // Create a mem slice of the wasm VM memory
    let memory_view = env.memory_view(&store);
    let Ok(mem_slice) = ptr.slice(&memory_view, ptr_len) else {
        error!(
            target: "runtime::tx_local::append_coin",
            "[WASM] [{cid}] append_coin(): Failed to make slice from ptr",
        );
        return darkfi_sdk::error::INTERNAL_ERROR
    };

    let mut buf = vec![0u8; ptr_len as usize];
    if let Err(e) = mem_slice.read_slice(&mut buf) {
        error!(
            target: "runtime::tx_local::append_coin",
            "[WASM] [{cid}] append_coin(): Failed to read memory slice: {e}",
        );
        return darkfi_sdk::error::INTERNAL_ERROR
    }

    let mut buf_reader = Cursor::new(buf);
    let read_coin: pallas::Base = match Decodable::decode(&mut buf_reader) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "runtime::tx_local::append_coin",
                "[WASM] [{cid}] append_coin(): Failed decoding Coin: {e}",
            );
            return darkfi_sdk::error::INTERNAL_ERROR
        }
    };

    // Make sure we've read the entire buffer
    if buf_reader.position() != ptr_len as u64 {
        error!(
            target: "runtime::tx_local::append_coin",
            "[WASM] [{cid}] append_coin(): Trailing bytes in argument stream",
        );
        return darkfi_sdk::error::INTERNAL_ERROR
    }

    let Ok(mut tx_state) = env.tx_local.try_borrow_mut() else {
        error!(
            target: "runtime::tx_local::append_coin",
            "[WASM] [{cid}] append_coin(): Could not borrow tx-local state",
        );
        return darkfi_sdk::error::INTERNAL_ERROR
    };

    // Check if we've added this coin already.
    if tx_state.new_coins.contains(&read_coin) {
        return darkfi_sdk::error::DB_SET_FAILED
    }

    // Add it to the new coins set
    tx_state.new_coins.push(read_coin);

    // Add it to the Merkle tree
    tx_state.coins_tree.append(MerkleNode::from(read_coin));

    // Append the Merkle root
    let Some(latest_root) = tx_state.coins_tree.root(0) else {
        error!(
            target: "runtime::tx_local::append_coin",
            "[WASM] [{cid}] append_coin(): Unable to read Merkle tree root",
        );
        return darkfi_sdk::error::INTERNAL_ERROR
    };

    // A bit redundant, but we should check.
    if tx_state.coin_roots.contains(&latest_root) {
        error!(
            target: "runtime::tx_local::append_coin",
            "[WASM] [{cid}] append_coin(): Found duplicate Merkle root",
        );
        return darkfi_sdk::error::INTERNAL_ERROR
    }

    // Push to new known roots
    tx_state.coin_roots.push(latest_root);

    // Return success
    wasm::entrypoint::SUCCESS
}
