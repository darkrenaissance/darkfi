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
    cell::{Cell, RefCell},
    sync::Arc,
};

use darkfi_sdk::{crypto::ContractId, entrypoint};
use log::{debug, info};
use wasmer::{
    imports, wasmparser::Operator, AsStoreRef, CompilerConfig, Function, FunctionEnv, Instance,
    Memory, MemoryView, Module, Pages, Store, Value, WASM_PAGE_SIZE,
};
use wasmer_compiler_singlepass::Singlepass;
use wasmer_middlewares::{
    metering::{get_remaining_points, MeteringPoints},
    Metering,
};

use super::{import, import::db::DbHandle, memory::MemoryManipulation};
use crate::{blockchain::Blockchain, Error, Result};

/// Name of the wasm linear memory in our guest module
const MEMORY: &str = "memory";

/// Hardcoded setup function of a contract
const INITIALIZE: &str = "__initialize";
/// Hardcoded entrypoint function of a contract
const ENTRYPOINT: &str = "__entrypoint";
/// Hardcoded apply function of a contract
const UPDATE: &str = "__update";

/// Gas limit for a contract
const GAS_LIMIT: u64 = 200000;

#[derive(Clone, Copy)]
pub enum ContractSection {
    Deploy,
    Exec,
    Update,
    Null,
}

impl ContractSection {
    fn name(&self) -> &str {
        match self {
            Self::Deploy => "__initialize",
            Self::Exec => "__entrypoint",
            Self::Update => "__update",
            Self::Null => unreachable!(),
        }
    }
}

/// The wasm vm runtime instantiated for every smart contract that runs.
pub struct Env {
    /// Blockchain access
    pub blockchain: Blockchain,
    /// sled tree handles used with `db_*`
    pub db_handles: RefCell<Vec<DbHandle>>,
    /// The contract ID being executed
    pub contract_id: ContractId,
    /// The contract section being executed
    pub contract_section: ContractSection,
    /// State update produced by a smart contract function call
    pub contract_return_data: Cell<Option<Vec<u8>>>,
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

pub struct Runtime {
    pub instance: Instance,
    pub store: Store,
    pub ctx: FunctionEnv<Env>,
}

impl Runtime {
    /// Create a new wasm runtime instance that contains the given wasm module.
    pub fn new(wasm_bytes: &[u8], blockchain: Blockchain, contract_id: ContractId) -> Result<Self> {
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

        // Initialize data
        let db_handles = RefCell::new(vec![]);
        let logs = RefCell::new(vec![]);

        debug!(target: "wasm_runtime::new", "Importing functions");

        let ctx = FunctionEnv::new(
            &mut store,
            Env {
                blockchain,
                db_handles,
                contract_id,
                contract_section: ContractSection::Null,
                contract_return_data: Cell::new(None),
                logs,
                memory: None,
            },
        );

        let imports = imports! {
            "env" => {
                "drk_log_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::util::drk_log,
                ),

                "nullifier_exists_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::chain_state::nullifier_exists,
                ),

                "is_valid_merkle_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::chain_state::is_valid_merkle,
                ),

                "set_return_data_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::util::set_return_data,
                ),

                "db_init_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::db::db_init,
                ),

                "db_lookup_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::db::db_lookup,
                ),

                "db_get_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::db::db_get,
                ),

                "db_begin_tx_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::db::db_begin_tx,
                ),

                "db_set_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::db::db_set,
                ),

                "db_end_tx_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::db::db_end_tx,
                ),
            }
        };

        debug!(target: "wasm_runtime::new", "Instantiating module");
        let instance = Instance::new(&mut store, &module, &imports)?;

        let mut env_mut = ctx.as_mut(&mut store);
        env_mut.memory = Some(instance.exports.get_with_generics(MEMORY)?);

        Ok(Self { instance, store, ctx })
    }

    fn call(&mut self, section: ContractSection, payload: &[u8]) -> Result<Vec<u8>> {
        debug!(target: "runtime", "Calling {} method", section.name());

        let mut env_mut = self.ctx.as_mut(&mut self.store);
        env_mut.contract_section = section;
        assert!(env_mut.contract_return_data.take().is_none());
        env_mut.contract_return_data.set(None);

        // Serialize the payload for the format the wasm runtime is expecting.
        let payload = Self::serialize_payload(&env_mut.contract_id, payload);

        // Allocate enough memory for the payload and copy it into the memory.
        let pages_required = payload.len() / WASM_PAGE_SIZE + 1;
        self.set_memory_page_size(pages_required as u32)?;
        self.copy_to_memory(&payload)?;

        debug!(target: "runtime", "Getting initialize function");
        let entrypoint = self.instance.exports.get_function(section.name())?;

        debug!(target: "runtime", "Executing wasm");
        let ret = match entrypoint.call(&mut self.store, &[Value::I32(0 as i32)]) {
            Ok(retvals) => {
                self.print_logs();
                debug!(target: "runtime", "{}", self.gas_info());
                retvals
            }
            Err(e) => {
                self.print_logs();
                debug!(target: "runtime", "{}", self.gas_info());
                // WasmerRuntimeError panics are handled here. Return from run() immediately.
                return Err(e.into())
            }
        };

        debug!(target: "runtime", "wasm executed successfully");
        debug!(target: "runtime", "Contract returned: {:?}", ret[0]);

        let mut env_mut = self.ctx.as_mut(&mut self.store);
        env_mut.contract_section = ContractSection::Null;
        let retdata = match env_mut.contract_return_data.take() {
            Some(retdata) => retdata,
            None => Vec::new()
        };

        let retval = match ret[0] {
            Value::I64(v) => v as u64,
            _ => unreachable!(),
        };

        match retval {
            entrypoint::SUCCESS => Ok(retdata),
            // FIXME: we should be able to see the error returned from the contract
            // We can put sdk::Error inside of this.
            _ => Err(Error::ContractInitError(retval)),
        }
    }

    /// This function runs when a smart contract is initially deployed, or re-deployed.
    /// The runtime will look for an [`INITIALIZE`] symbol in the wasm code, and execute
    /// it if found. Optionally, it is possible to pass in a payload for any kind of special
    /// instructions the developer wants to manage in the initialize function.
    /// This process is supposed to set up the sled db trees for storing the smart contract
    /// state, and it can create, delete, modify, read, and write to databases it's allowed to.
    /// The permissions for this are handled by the `ContractId` in the sled db API so we
    /// assume that the contract is only able to do write operations on its own sled trees.
    pub fn deploy(&mut self, payload: &[u8]) -> Result<()> {
        let _ = self.call(ContractSection::Deploy, payload)?;
        Ok(())
        /*
        let mut env_mut = self.ctx.as_mut(&mut self.store);
        env_mut.contract_section = ContractSection::Deploy;
        assert!(env_mut.contract_return_data.take().is_none());
        env_mut.contract_return_data.set(None);

        // Serialize the payload for the format the wasm runtime is expecting.
        let payload = Self::serialize_payload(&env_mut.contract_id, payload);

        // Allocate enough memory for the payload and copy it into the memory.
        let pages_required = payload.len() / WASM_PAGE_SIZE + 1;
        self.set_memory_page_size(pages_required as u32)?;
        self.copy_to_memory(&payload)?;

        debug!(target: "wasm_runtime::deploy", "Getting initialize function");
        let entrypoint = self.instance.exports.get_function(INITIALIZE)?;

        debug!(target: "wasm_runtime::deploy", "Executing wasm");
        let ret = match entrypoint.call(&mut self.store, &[Value::I32(0 as i32)]) {
            Ok(retvals) => {
                self.print_logs();
                debug!(target: "wasm_runtime::deploy", "{}", self.gas_info());
                retvals
            }
            Err(e) => {
                self.print_logs();
                debug!(target: "wasm_runtime::deploy", "{}", self.gas_info());
                // WasmerRuntimeError panics are handled here. Return from run() immediately.
                return Err(e.into())
            }
        };

        debug!(target: "wasm_runtime::deploy", "wasm executed successfully");
        debug!(target: "wasm_runtime::deploy", "Contract returned: {:?}", ret[0]);

        let mut env_mut = self.ctx.as_mut(&mut self.store);
        env_mut.contract_section = ContractSection::Null;
        let retdata = env_mut.contract_return_data.take();

        let retval = match ret[0] {
            Value::I64(v) => v as u64,
            _ => unreachable!(),
        };

        match retval {
            entrypoint::SUCCESS => Ok(()),
            // FIXME: we should be able to see the error returned from the contract
            // We can put sdk::Error inside of this.
            _ => Err(Error::ContractInitError(retval)),
        }
        */
    }

    /// This funcion runs when someone wants to execute a smart contract.
    /// The runtime will look for an [`ENTRYPOINT`] symbol in the wasm code, and
    /// execute it if found. A payload is also passed as an instruction that can
    /// be used inside the vm by the runtime.
    pub fn exec(&mut self, payload: &[u8]) -> Result<Vec<u8>> {
        self.call(ContractSection::Exec, payload)
        /*
        let mut env_mut = self.ctx.as_mut(&mut self.store);
        env_mut.contract_section = ContractSection::Exec;
        assert!(env_mut.contract_return_data.take().is_none());
        env_mut.contract_return_data.set(None);

        // Serialize the payload for the format the wasm runtime is expecting.
        let payload = Self::serialize_payload(&env_mut.contract_id, payload);

        // Allocate enough memory for the payload and copy it into the memory.
        let pages_required = payload.len() / WASM_PAGE_SIZE + 1;
        self.set_memory_page_size(pages_required as u32)?;
        self.copy_to_memory(&payload)?;

        debug!(target: "wasm_runtime::exec", "Getting entrypoint function");
        let entrypoint = self.instance.exports.get_function(ENTRYPOINT)?;

        debug!(target: "wasm_runtime::exec", "Executing wasm");
        // We pass 0 to entrypoint() which is the location of the payload data in the memory
        let ret = match entrypoint.call(&mut self.store, &[Value::I32(0 as i32)]) {
            Ok(retvals) => {
                self.print_logs();
                debug!(target: "wasm_runtime::exec", "{}", self.gas_info());
                retvals
            }
            Err(e) => {
                self.print_logs();
                debug!(target: "wasm_runtime::exec", "{}", self.gas_info());
                // WasmerRuntimeError panics are handled here. Return from run() immediately.
                return Err(e.into())
            }
        };

        debug!(target: "wasm_runtime::exec", "wasm executed successfully");
        debug!(target: "wasm_runtime::exec", "Contract returned: {:?}", ret[0]);

        let retval = match ret[0] {
            Value::I64(v) => v as u64,
            _ => unreachable!(),
        };

        match retval {
            entrypoint::SUCCESS => Ok(()),
            _ => Err(Error::ContractExecError(retval)),
        }
        */
    }

    /// This function runs after successful execution of [`exec`] and tries to
    /// apply the state change to the sled databases.
    /// The runtime will lok for an [`UPDATE`] symbol in the wasm code, and execute
    /// it if found. The function does not take an arbitrary payload, but just takes
    /// a state update from `env` and passes it into the wasm runtime.
    pub fn apply(&mut self, update: &[u8]) -> Result<()> {
        let _ = self.call(ContractSection::Update, update)?;
        Ok(())
        /*
        let mut env_mut = self.ctx.as_mut(&mut self.store);
        env_mut.contract_section = ContractSection::Update;
        assert!(env_mut.contract_return_data.take().is_none());
        env_mut.contract_return_data.set(None);

        // Take the update data from env, and serialize it for the format the wasm
        // runtime is expecting.
        // FIXME: Can panic
        let update_data = env_mut.contract_update.take().unwrap();
        let mut payload = Vec::with_capacity(1 + update_data.1.len());
        payload.extend_from_slice(&[update_data.0]);
        payload.extend_from_slice(&update_data.1);
        let payload = Self::serialize_payload(&env_mut.contract_id, &payload);

        // Allocate enough memory for the payload and copy it into the memory.
        let pages_required = payload.len() / WASM_PAGE_SIZE + 1;
        self.set_memory_page_size(pages_required as u32)?;
        self.copy_to_memory(&payload)?;

        debug!(target: "wasm_runtime::apply", "Getting update function");
        let entrypoint = self.instance.exports.get_function(UPDATE)?;

        debug!(target: "wasm_runtime::apply", "Executing wasm");
        let ret = match entrypoint.call(&mut self.store, &[Value::I32(0 as i32)]) {
            Ok(retvals) => {
                self.print_logs();
                debug!(target: "wasm_runtime::apply", "{}", self.gas_info());
                retvals
            }
            Err(e) => {
                self.print_logs();
                debug!(target: "wasm_runtime::apply", "{}", self.gas_info());
                // WasmerRuntimeError panics are handled here. Return from run() immediately.
                return Err(e.into())
            }
        };

        debug!(target: "wasm_runtime::apply", "wasm executed successfully");
        debug!(target: "wasm_runtime::apply", "Contract returned: {:?}", ret[0]);

        let retval = match ret[0] {
            Value::I64(v) => v as u64,
            _ => unreachable!(),
        };

        match retval {
            entrypoint::SUCCESS => Ok(()),
            _ => Err(Error::ContractInitError(retval)),
        }
        */
    }

    fn print_logs(&self) {
        let logs = self.ctx.as_ref(&self.store).logs.borrow();
        for msg in logs.iter() {
            debug!(target: "runtime", "Contract log: {}", msg);
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
        // TODO: Maybe should write to first zero memory and return the pointer/offset?
        // Get the memory view
        let env = self.ctx.as_ref(&self.store);
        let memory_view = env.memory_view(&self.store);
        memory_view.write_slice(payload, 0)
    }

    /// Serialize contract payload to the format accepted by the runtime functions.
    /// We keep the same payload as a slice of bytes, and prepend it with a
    /// little-endian u64 to tell the payload's length.
    fn serialize_payload(cid: &ContractId, payload: &[u8]) -> Vec<u8> {
        let ser_cid = cid.to_bytes();
        let payload_len = payload.len();
        let mut out = Vec::with_capacity(ser_cid.len() + 8 + payload_len);
        out.extend_from_slice(&ser_cid);
        out.extend_from_slice(&(payload_len as u64).to_le_bytes());
        out.extend_from_slice(payload);
        out
    }
}
