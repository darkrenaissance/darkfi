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

use std::{
    cell::{Cell, RefCell},
    sync::Arc,
};

use darkfi_sdk::{crypto::ContractId, entrypoint};
use darkfi_serial::serialize;
use log::{debug, error, info};
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
use crate::{
    blockchain::{contract_store::SMART_CONTRACT_ZKAS_DB_NAME, BlockchainOverlayPtr},
    util::time::TimeKeeper,
    Error, Result,
};

/// Name of the wasm linear memory in our guest module
const MEMORY: &str = "memory";

/// Gas limit for a contract
const GAS_LIMIT: u64 = 400_000_000;

#[derive(Clone, Copy, PartialEq)]
pub enum ContractSection {
    /// Setup function of a contract
    Deploy,
    /// Entrypoint function of a contract
    Exec,
    /// Apply function of a contract
    Update,
    /// Metadata
    Metadata,
    /// Placeholder state before any initialization
    Null,
}

impl ContractSection {
    pub const fn name(&self) -> &str {
        match self {
            Self::Deploy => "__initialize",
            Self::Exec => "__entrypoint",
            Self::Update => "__update",
            Self::Metadata => "__metadata",
            Self::Null => unreachable!(),
        }
    }
}

/// The wasm vm runtime instantiated for every smart contract that runs.
pub struct Env {
    /// Blockchain overlay access
    pub blockchain: BlockchainOverlayPtr,
    /// Overlay tree handles used with `db_*`
    pub db_handles: RefCell<Vec<DbHandle>>,
    /// The contract ID being executed
    pub contract_id: ContractId,
    /// The compiled wasm bincode being executed,
    pub contract_bincode: Vec<u8>,
    /// The contract section being executed
    pub contract_section: ContractSection,
    /// State update produced by a smart contract function call
    pub contract_return_data: Cell<Option<Vec<u8>>>,
    /// Logs produced by the contract
    pub logs: RefCell<Vec<String>>,
    /// Direct memory access to the VM
    pub memory: Option<Memory>,
    /// Object store for transferring memory from the host to VM
    pub objects: RefCell<Vec<Vec<u8>>>,
    /// Helper structure to calculate time related operations
    pub time_keeper: TimeKeeper,
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

/// Define a wasm runtime.
pub struct Runtime {
    /// A wasm instance
    pub instance: Instance,
    /// A wasm store (global state)
    pub store: Store,
    // Wrapper for [`Env`], defined above.
    pub ctx: FunctionEnv<Env>,
}

impl Runtime {
    /// Create a new wasm runtime instance that contains the given wasm module.
    pub fn new(
        wasm_bytes: &[u8],
        blockchain: BlockchainOverlayPtr,
        contract_id: ContractId,
        time_keeper: TimeKeeper,
    ) -> Result<Self> {
        info!(target: "runtime::vm_runtime", "Instantiating a new runtime");
        // This function will be called for each `Operator` encountered during
        // the wasm module execution. It should return the cost of the operator
        // that it received as its first argument. For now, every wasm opcode
        // has a cost of `1`.
        // https://docs.rs/wasmparser/latest/wasmparser/enum.Operator.html
        let cost_function = |_operator: &Operator| -> u64 { 1 };

        // `Metering` needs to be configured with a limit and a cost function.
        // For each `Operator`, the metering middleware will call the cost
        // function and subtract the cost from the remaining points.
        let metering = Arc::new(Metering::new(GAS_LIMIT, cost_function));

        // Define the compiler and middleware, engine, and store
        let mut compiler_config = Singlepass::new();
        compiler_config.push_middleware(metering);
        let mut store = Store::new(compiler_config);

        debug!(target: "runtime::vm_runtime", "Compiling module");
        let module = Module::new(&store, wasm_bytes)?;

        // Initialize data
        let db_handles = RefCell::new(vec![]);
        let logs = RefCell::new(vec![]);

        debug!(target: "runtime::vm_runtime", "Importing functions");

        let ctx = FunctionEnv::new(
            &mut store,
            Env {
                blockchain,
                db_handles,
                contract_id,
                contract_bincode: wasm_bytes.to_vec(),
                contract_section: ContractSection::Null,
                contract_return_data: Cell::new(None),
                logs,
                memory: None,
                objects: RefCell::new(vec![]),
                time_keeper,
            },
        );

        let imports = imports! {
            "env" => {
                "drk_log_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::util::drk_log,
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

                "db_contains_key_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::db::db_contains_key,
                ),

                "db_set_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::db::db_set,
                ),

                "db_del_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::db::db_del,
                ),

                "zkas_db_set_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::db::zkas_db_set,
                ),

                "put_object_bytes_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::util::put_object_bytes,
                ),

                "get_object_bytes_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::util::get_object_bytes,
                ),

                "get_object_size_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::util::get_object_size,
                ),

                "merkle_add_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::merkle::merkle_add,
                ),

                "get_current_epoch_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::util::get_current_epoch,
                ),

                "get_current_slot_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::util::get_current_slot,
                ),

                "get_verifying_slot_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::util::get_verifying_slot,
                ),

                "get_verifying_slot_epoch_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::util::get_verifying_slot_epoch,
                ),

                "get_slot_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::util::get_slot,
                ),

                "get_blockchain_time_" => Function::new_typed_with_env(
                    &mut store,
                    &ctx,
                    import::util::get_blockchain_time,
                ),
            }
        };

        debug!(target: "runtime::vm_runtime", "Instantiating module");
        let instance = Instance::new(&mut store, &module, &imports)?;

        let env_mut = ctx.as_mut(&mut store);
        env_mut.memory = Some(instance.exports.get_with_generics(MEMORY)?);

        Ok(Self { instance, store, ctx })
    }

    /// Perform a sanity check of the WASM bincode. In particular, ensure that it contains
    /// all the necessary symbols for executing contracts, including sections for
    /// Deploy, Exec, Update, and Metadata.
    pub fn sanity_check(&self) -> Result<()> {
        debug!(target: "runtime::vm_runtime", "Performing sanity check on wasm bincode");

        // Check that we have all the necessary symbols;
        let _ = self.instance.exports.get_function(ContractSection::Deploy.name())?;
        let _ = self.instance.exports.get_function(ContractSection::Exec.name())?;
        let _ = self.instance.exports.get_function(ContractSection::Update.name())?;
        let _ = self.instance.exports.get_function(ContractSection::Metadata.name())?;

        Ok(())
    }

    /// Call a contract method using a supplied payload. Returns a Vector of bytes
    /// corresponding to the result data of the call. For calls that do not return
    /// any data, an empty Vector is returned.
    fn call(&mut self, section: ContractSection, payload: &[u8]) -> Result<Vec<u8>> {
        debug!(target: "runtime::vm_runtime", "Calling {} method", section.name());

        let env_mut = self.ctx.as_mut(&mut self.store);
        env_mut.contract_section = section;
        // Verify contract's return data is empty, or quit.
        assert!(env_mut.contract_return_data.take().is_none());

        // This should already be clear, or the assert call above
        // would prevent the code from reaching this point.
        // Clear anyway, to be safe.
        env_mut.contract_return_data.set(None);
        // Clear the logs
        let _ = env_mut.logs.take();

        // Serialize the payload for the format the wasm runtime is expecting.
        let payload = Self::serialize_payload(&env_mut.contract_id, payload);

        // Allocate enough memory for the payload and copy it into the memory.
        let pages_required = payload.len() / WASM_PAGE_SIZE + 1;
        self.set_memory_page_size(pages_required as u32)?;
        self.copy_to_memory(&payload)?;

        debug!(target: "runtime::vm_runtime", "Getting {} function", section.name());
        let entrypoint = self.instance.exports.get_function(section.name())?;

        debug!(target: "runtime::vm_runtime", "Executing wasm");
        let ret = match entrypoint.call(&mut self.store, &[Value::I32(0_i32)]) {
            Ok(retvals) => {
                self.print_logs();
                info!(target: "runtime::vm_runtime", "{}", self.gas_info());
                retvals
            }
            Err(e) => {
                self.print_logs();
                info!(target: "runtime::vm_runtime", "{}", self.gas_info());
                // WasmerRuntimeError panics are handled here. Return from run() immediately.
                error!(target: "runtime::vm_runtime", "Wasmer Runtime Error: {:#?}", e);
                return Err(e.into())
            }
        };

        debug!(target: "runtime::vm_runtime", "wasm executed successfully");

        let env_mut = self.ctx.as_mut(&mut self.store);
        env_mut.contract_section = ContractSection::Null;
        let retdata = match env_mut.contract_return_data.take() {
            Some(retdata) => retdata,
            None => Vec::new(),
        };

        // Determine the return value of the wasm function. At this stage,
        // it is assumed that the contract completed successful given
        // that validation has been performed above.
        let retval: i64 = match ret.len() {
            0 => {
                // Return a success value if there is no return value from
                // the contract.
                debug!(target: "runtime::vm_runtime", "Contract has no return value (expected)");
                entrypoint::SUCCESS
            }
            _ => {
                match ret[0] {
                    Value::I64(v) => {
                        debug!(target: "runtime::vm_runtime", "Contract returned: {:?}", ret[0]);
                        v
                    }
                    // The only supported return type is i64, so panic if another
                    // value is returned.
                    _ => unreachable!("Got unexpected result return value: {:?}", ret),
                }
            }
        };

        // Check the integer return value of the call. A value of `entrypoint::SUCCESS` (i.e. zero)
        // corresponds to a successful contract call; in this case, we return the contract's
        // result data. Otherwise, map the integer return value to a [`ContractError`].
        match retval {
            entrypoint::SUCCESS => Ok(retdata),
            // FIXME: we should be able to see the error returned from the contract
            // We can put sdk::Error inside of this.
            _ => {
                let err = darkfi_sdk::error::ContractError::from(retval);
                Err(Error::ContractError(err))
            }
        }
    }

    /// This function runs when a smart contract is initially deployed, or re-deployed.
    /// The runtime will look for an `INITIALIZE` symbol in the wasm code, and execute
    /// it if found. Optionally, it is possible to pass in a payload for any kind of special
    /// instructions the developer wants to manage in the initialize function.
    /// This process is supposed to set up the overlay trees for storing the smart contract
    /// state, and it can create, delete, modify, read, and write to databases it's allowed to.
    /// The permissions for this are handled by the `ContractId` in the overlay db API so we
    /// assume that the contract is only able to do write operations on its own overlay trees.
    /// TODO: This should also be in sled-overlay!
    pub fn deploy(&mut self, payload: &[u8]) -> Result<()> {
        info!(target: "runtime::vm_runtime", "[wasm-runtime] Running deploy");

        // Scoped for borrows
        {
            let env_mut = self.ctx.as_mut(&mut self.store);

            // We always want to have the zkas db as index 0 in db handles and batches when
            // deploying.
            let contracts = &env_mut.blockchain.lock().unwrap().contracts;

            let zkas_tree_handle =
                match contracts.lookup(&env_mut.contract_id, SMART_CONTRACT_ZKAS_DB_NAME) {
                    Ok(v) => v,
                    Err(_) => {
                        // FIXME: All this is deploy code is "vulnerable" and able to init a
                        // tree regardless of execution success. We can easily delete the db
                        // if execution fails though, and we should charge gas for db_init.
                        // and perhaps also for the zkas database in this specific case.
                        contracts.init(&env_mut.contract_id, SMART_CONTRACT_ZKAS_DB_NAME)?
                    }
                };

            let mut db_handles = env_mut.db_handles.borrow_mut();
            db_handles.push(DbHandle::new(env_mut.contract_id, zkas_tree_handle));
        }

        debug!(target: "runtime::vm_runtime", "[wasm-runtime] payload: {:?}", payload);
        let _ = self.call(ContractSection::Deploy, payload)?;

        // Update the wasm bincode in the WasmStore
        let env_mut = self.ctx.as_mut(&mut self.store);
        env_mut
            .blockchain
            .lock()
            .unwrap()
            .wasm_bincode
            .insert(env_mut.contract_id, &env_mut.contract_bincode)?;

        Ok(())
    }

    /// This function runs when someone wants to execute a smart contract.
    /// The runtime will look for an `ENTRYPOINT` symbol in the wasm code, and
    /// execute it if found. A payload is also passed as an instruction that can
    /// be used inside the vm by the runtime.
    pub fn exec(&mut self, payload: &[u8]) -> Result<Vec<u8>> {
        debug!(target: "runtime::vm_runtime", "exec: {:?}", payload);
        self.call(ContractSection::Exec, payload)
    }

    /// This function runs after successful execution of `exec` and tries to
    /// apply the state change to the overlay databases.
    /// The runtime will lok for an `UPDATE` symbol in the wasm code, and execute
    /// it if found. The function does not take an arbitrary payload, but just takes
    /// a state update from `env` and passes it into the wasm runtime.
    pub fn apply(&mut self, update: &[u8]) -> Result<()> {
        debug!(target: "runtime::vm_runtime", "apply: {:?}", update);
        let _ = self.call(ContractSection::Update, update)?;

        Ok(())
    }

    /// This function runs first in the entire scheme of executing a smart contract.
    /// It is supposed to correctly extract public inputs for any ZK proofs included
    /// in the contract calls, and also extract the public keys used to verify the
    /// call/transaction signatures.
    pub fn metadata(&mut self, payload: &[u8]) -> Result<Vec<u8>> {
        self.call(ContractSection::Metadata, payload)
    }

    /// Prints the wasm contract logs.
    fn print_logs(&self) {
        let logs = self.ctx.as_ref(&self.store).logs.borrow();
        for msg in logs.iter() {
            info!(target: "runtime::vm_runtime", "[WASM] Contract log: {}", msg);
        }
    }

    /// Calculate the remaining gas using wasm's concept
    /// of metering points.
    fn gas_used(&mut self) -> u64 {
        let remaining_points = get_remaining_points(&mut self.store, &self.instance);

        match remaining_points {
            MeteringPoints::Remaining(rem) => {
                if rem > GAS_LIMIT {
                    // This should never occur, but catch it explicitly to avoid
                    // potential underflow issues when calculating `remaining_points`.
                    unreachable!("Remaining wasm points exceed GAS_LIMIT");
                }
                GAS_LIMIT - rem
            }
            MeteringPoints::Exhausted => GAS_LIMIT + 1,
        }
    }

    // Return a message informing the user whether there is any
    // gas remaining. Values equal to GAS_LIMIT are not considered
    // to be exhausted. e.g. Using 100/100 gas should not give a
    // 'gas exhausted' message.
    fn gas_info(&mut self) -> String {
        let gas_used = self.gas_used();

        if gas_used > GAS_LIMIT {
            format!("Gas fully exhausted: {}/{}", gas_used, GAS_LIMIT)
        } else {
            format!("Gas used: {}/{}", gas_used, GAS_LIMIT)
        }
    }

    /// Set the memory page size. Returns the previous memory size.
    fn set_memory_page_size(&mut self, pages: u32) -> Result<Pages> {
        // Grab memory by value
        let memory = self.take_memory();
        // Modify the memory
        let ret = memory.grow(&mut self.store, Pages(pages))?;
        // Replace the memory back again
        self.ctx.as_mut(&mut self.store).memory = Some(memory);
        Ok(ret)
    }

    /// Take Memory by value. Needed to modify the Memory object
    /// Will panic if memory isn't set.
    fn take_memory(&mut self) -> Memory {
        let env_memory = &mut self.ctx.as_mut(&mut self.store).memory;
        let memory = env_memory.take();
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
    /// We keep the same payload as a slice of bytes, and prepend it with a [`ContractId`],
    /// and then a little-endian u64 to tell the payload's length.
    fn serialize_payload(cid: &ContractId, payload: &[u8]) -> Vec<u8> {
        let ser_cid = serialize(cid);
        let payload_len = payload.len();
        let mut out = Vec::with_capacity(ser_cid.len() + 8 + payload_len);
        out.extend_from_slice(&ser_cid);
        out.extend_from_slice(&(payload_len as u64).to_le_bytes());
        out.extend_from_slice(payload);
        out
    }
}
