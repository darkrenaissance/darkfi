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

use std::{
    cell::RefCell,
    sync::{Arc, Mutex},
};

use darkfi_sdk::entrypoint;
use log::{debug, info};
use wasmer::{
    imports, wasmparser::Operator, AsStoreRef, CompilerConfig, Function, FunctionEnv, Instance,
    Memory, MemoryView, Module, Pages, Store, Value, WasmPtr, WASM_PAGE_SIZE,
};
use wasmer_compiler_singlepass::Singlepass;
use wasmer_middlewares::{
    metering::{get_remaining_points, MeteringPoints},
    Metering,
};

use super::{
    chain_state::{is_valid_merkle, nullifier_exists},
    memory::MemoryManipulation,
    util::drk_log,
};
use crate::{Error, Result};

/// Name of the wasm linear memory in our guest module
const MEMORY: &str = "memory";
/// Hardcoded entrypoint function of a contract
pub const ENTRYPOINT: &str = "entrypoint";
/// Gas limit for a contract
const GAS_LIMIT: u64 = 200000;

/// The wasm vm runtime instantiated for every smart contract that runs.
pub struct Env {
    /// Logs produced by the contract
    pub logs: RefCell<Vec<String>>,
    /// Direct memory access to the VM
    pub memory: Option<Memory>,
}

impl Env {
    /// Provide safe access to the memory
    /// (it must be initialized before it can be used)
    ///
    ///     // ctx: FunctionEnvMut<Env>
    ///     let env = ctx.data();
    ///     let memory = env.memory_view(&ctx);
    ///
    pub fn memory_view<'a>(&'a self, store: &'a impl AsStoreRef) -> MemoryView<'a> {
        self.memory().view(store)
    }

    /// Get memory, that needs to have been set fist
    pub fn memory(&self) -> &Memory {
        self.memory.as_ref().unwrap()
    }
}

/// The result of the VM execution
pub struct ExecutionResult {
    /// The exit code returned by the wasm program
    pub exitcode: u8,
    /// Logs written from the wasm program
    pub logs: Vec<String>,
}

pub struct Runtime {
    pub instance: Instance,
    pub store: Store,
    pub ctx: FunctionEnv<Env>,
}

impl Runtime {
    /// Create a new wasm runtime instance that contains the given wasm module.
    pub fn new(wasm_bytes: &[u8]) -> Result<Self> {
        info!(target: "warm_runtime::new", "Instantiating a new runtime");
        // This function will be called for each `Operator` encountered during
        // the wasm module execution. It should return the cost of the operator
        // that it received as its first argument.
        // https://docs.rs/wasmparser/latest/wasmparser/enum.Operator.html
        let cost_function = |operator: &Operator| -> u64 {
            match operator {
                Operator::LocalGet { .. } => 1,
                Operator::I32Const { .. } => 1,
                Operator::I32Add { .. } => 2,
                _ => 0,
            }
        };

        // `Metering` needs to be conigured with a limit and a cost function.
        // For each `Operator`, the metering middleware will call the cost
        // function and subtract the cost from the remaining points.
        let metering = Arc::new(Metering::new(GAS_LIMIT, cost_function));

        // Define the compiler and middleware, engine, and store
        let mut compiler_config = Singlepass::new();
        compiler_config.push_middleware(metering);
        let mut store = Store::new(compiler_config);

        debug!(target: "wasm_runtime::new", "Compiling module");
        let module = Module::new(&store, wasm_bytes)?;

        // This section will need changing
        debug!(target: "wasm_runtime::new", "Importing functions");
        let logs = RefCell::new(vec![]);

        let ctx = FunctionEnv::new(&mut store, Env { logs, memory: None });

        let imports = imports! {
            "env" => {
                "drk_log_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    drk_log,
                ),

                "nullifier_exists_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    nullifier_exists,
                ),

                "is_valid_merkle_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    is_valid_merkle,
                ),
            }
        };

        debug!(target: "wasm_runtime::new", "Instantiating module");
        let instance = Instance::new(&mut store, &module, &imports)?;

        let mut env_mut = ctx.as_mut(&mut store);
        env_mut.memory = Some(instance.exports.get_with_generics(MEMORY)?);

        Ok(Self { instance, store, ctx })
    }

    /// Run the hardcoded `ENTRYPOINT` function with the given payload as input.
    pub fn run(&mut self, payload: &[u8]) -> Result<()> {
        let pages_required = payload.len() / WASM_PAGE_SIZE + 1;
        self.set_memory_page_size(pages_required as u32)?;

        self.copy_to_memory(payload)?;

        debug!(target: "wasm_runtime::run", "Getting entrypoint function");
        let entrypoint = self.instance.exports.get_function(ENTRYPOINT)?;

        debug!(target: "wasm_runtime::run", "Executing wasm");
        // We pass 0 to entrypoint() which is the location of the payload data in the memory
        let ret = match entrypoint.call(&mut self.store, &[Value::I32(0 as i32)]) {
            Ok(retvals) => {
                self.print_logs();
                debug!(target: "wasm_runtime::run", "{}", self.gas_info());
                retvals
            }
            Err(e) => {
                self.print_logs();
                debug!(target: "wasm_runtime::run", "{}", self.gas_info());
                // WasmerRuntimeError panics are handled here. Return from run() immediately.
                return Err(e.into())
            }
        };

        debug!(target: "wasm_runtime::run", "wasm executed successfully");
        debug!(target: "wasm_runtime::run", "Contract returned: {:?}", ret[0]);

        let retval = match ret[0] {
            Value::I64(v) => v as u64,
            _ => unreachable!(),
        };

        match retval {
            entrypoint::SUCCESS => Ok(()),
            _ => Err(Error::ContractExecError(retval)),
        }
    }

    fn print_logs(&self) {
        let logs = self.ctx.as_ref(&self.store).logs.borrow();
        for msg in logs.iter() {
            debug!(target: "wasm_runtime::run", "Contract log: {}", msg);
        }
    }

    fn gas_info(&mut self) -> String {
        let remaining_points = get_remaining_points(&mut self.store, &self.instance);

        match remaining_points {
            MeteringPoints::Remaining(rem) => {
                format!("Gas used: {}/{}", GAS_LIMIT - rem, GAS_LIMIT)
            }
            MeteringPoints::Exhausted => {
                format!("Gas fully exhausted: {}/{}", GAS_LIMIT + 1, GAS_LIMIT)
            }
        }
    }

    /// Set the memory page size
    fn set_memory_page_size(&mut self, pages: u32) -> Result<()> {
        // Grab memory by value
        let memory = self.take_memory();
        // Modify the memory
        memory.grow(&mut self.store, Pages(pages))?;
        // Replace the memory back again
        self.ctx.as_mut(&mut self.store).memory = Some(memory);
        Ok(())
    }
    /// Take Memory by value. Needed to modify the Memory object
    /// Will panic if memory isn't set.
    fn take_memory(&mut self) -> Memory {
        let env_memory = &mut self.ctx.as_mut(&mut self.store).memory;
        let memory = std::mem::replace(env_memory, None);
        memory.expect("memory should be set")
    }

    /// Copy payload to the start of the memory
    fn copy_to_memory(&self, payload: &[u8]) -> Result<()> {
        // Get the memory view
        let env = self.ctx.as_ref(&self.store);
        let memory_view = env.memory_view(&self.store);
        memory_view.write_slice(payload, 0)
    }
}
