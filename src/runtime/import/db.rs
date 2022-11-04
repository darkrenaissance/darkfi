use log::error;
use wasmer::{FunctionEnvMut, WasmPtr};

use crate::runtime::vm_runtime::{ContractSection, Env};

/// Only deploy() can call this. Creates a new database instance for this contract.
///
/// ```
///     type DbHandle = u32;
///     db_init(db_name) -> DbHandle
/// ```
pub(crate) fn db_init(mut ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, len: u32) -> i32 {
    let env = ctx.data();
    match env.contract_section {
        ContractSection::Deploy => {
            let env = ctx.data();
            let memory_view = env.memory_view(&ctx);

            match ptr.read_utf8_string(&memory_view, len) {
                Ok(db_name) => {
                    // TODO:
                    // * db_name = blake3_hash(contract_id, db_name)
                    // * create db_name sled database
                }
                Err(_) => {
                    error!(target: "wasm_runtime::drk_log", "Failed to read UTF-8 string from VM memory");
                    return -2
                }
            }
            0
        }
        _ => -1,
    }
}

/// Everyone can call this. Lookups up a database handle from its name.
///
/// ```
///     type DbHandle = u32;
///     db_lookup(db_name) -> DbHandle
/// ```
pub(crate) fn db_lookup(mut ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, len: u32) -> i32 {
    let env = ctx.data();
    match env.contract_section {
        ContractSection::Deploy | ContractSection::Exec | ContractSection::Update => {
            let env = ctx.data();
            let memory_view = env.memory_view(&ctx);

            match ptr.read_utf8_string(&memory_view, len) {
                Ok(db_name) => {
                    // db_name = blake3_hash(contract_id, db_name)
                    return 110
                }
                Err(_) => {
                    error!(target: "wasm_runtime::drk_log", "Failed to read UTF-8 string from VM memory");
                    return -2
                }
            }
            0
        }
        _ => -1,
    }
}

/// Everyone can call this. Will read a key from the key-value store.
///
/// ```
///     value = db_get(db_handle, key);
/// ```
pub(crate) fn db_get(mut ctx: FunctionEnvMut<Env>) -> i32 {
    let env = ctx.data();
    match env.contract_section {
        ContractSection::Exec => 0,
        _ => -1,
    }
}

/// Only update() can call this. Starts an atomic transaction.
///
/// ```
///     tx_handle = db_begin_tx();
/// ```
pub(crate) fn db_begin_tx(mut ctx: FunctionEnvMut<Env>) -> i32 {
    let env = ctx.data();
    match env.contract_section {
        ContractSection::Deploy | ContractSection::Update => 0,
        _ => -1,
    }
}

/// Only update() can call this. Set a value within the transaction.
///
/// ```
///     db_set(tx_handle, key, value);
/// ```
pub(crate) fn db_set(mut ctx: FunctionEnvMut<Env>) -> i32 {
    let env = ctx.data();
    match env.contract_section {
        ContractSection::Deploy | ContractSection::Update => 0,
        _ => -1,
    }
}

/// Only update() can call this. This writes the atomic tx to the database.
///
/// ```
///     db_end_tx(db_handle, tx_handle);
/// ```
pub(crate) fn db_end_tx(mut ctx: FunctionEnvMut<Env>) -> i32 {
    let env = ctx.data();
    match env.contract_section {
        ContractSection::Deploy | ContractSection::Update => 0,
        _ => -1,
    }
}
