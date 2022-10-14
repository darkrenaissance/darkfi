use std::sync::{Arc, Mutex};

use darkfi_sdk::entrypoint;
use log::{debug, info};
use wasmer::{
    imports, wasmparser::Operator, CompilerConfig, Function, HostEnvInitError, Instance, LazyInit,
    Memory, Module, Store, Universal, Value, WasmerEnv,
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
use crate::{
    node::{state::StateUpdate, MemoryState},
    Result,
};

/// Function name in our wasm module that allows us to allocate some memory.
const WASM_MEM_ALLOC: &str = "__drkruntime_mem_alloc";
/// Name of the wasm linear memory in our guest module
const MEMORY: &str = "memory";
/// Hardcoded entrypoint function of a contract
pub const ENTRYPOINT: &str = "entrypoint";
/// Gas limit for a contract
const GAS_LIMIT: u64 = 200000;

/// The wasm vm runtime instantiated for every smart contract that runs.
#[derive(Clone)]
pub struct Env {
    /// Logs produced by the contract
    pub logs: Arc<Mutex<Vec<String>>>,
    /// Direct memory access to the VM
    pub memory: LazyInit<Memory>,
    /// Cloned state machine living in memory
    pub state_machine: Arc<MemoryState>,
    /// State updates produced by the contract
    pub state_updates: Arc<Mutex<Vec<StateUpdate>>>,
}

impl WasmerEnv for Env {
    fn init_with_instance(
        &mut self,
        instance: &Instance,
    ) -> std::result::Result<(), HostEnvInitError> {
        let memory: Memory = instance.exports.get_with_generics_weak(MEMORY)?;
        self.memory.initialize(memory);
        Ok(())
    }
}

/// The result of the VM execution
pub struct ExecutionResult {
    /// The exit code returned by the wasm program
    pub exitcode: u8,
    /// Logs written from the wasm program
    pub logs: Vec<String>,
    /// State machine updates produced by the wasm program
    pub state_updates: Vec<StateUpdate>,
}

pub struct Runtime {
    pub instance: Instance,
    pub env: Env,
}

impl Runtime {
    /// Create a new wasm runtime instance that contains the given wasm module.
    pub fn new(wasm_bytes: &[u8], state_machine: MemoryState) -> Result<Self> {
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
        let mut compiler = Singlepass::new();
        compiler.push_middleware(metering);
        let store = Store::new(&Universal::new(compiler).engine());

        debug!(target: "wasm_runtime::new", "Compiling module");
        let module = Module::new(&store, wasm_bytes)?;

        debug!(target: "wasm_runtime::new", "Importing functions");
        let memory = LazyInit::new();
        let logs = Arc::new(Mutex::new(vec![]));
        let state_machine = Arc::new(state_machine);
        let state_updates = Arc::new(Mutex::new(vec![]));

        let env = Env { logs, memory, state_machine, state_updates };

        let import_object = imports! {
            "env" => {
                "drk_log_" => Function::new_native_with_env(
                    &store,
                    env.clone(),
                    drk_log,
                ),

                "nullifier_exists_" => Function::new_native_with_env(
                    &store,
                    env.clone(),
                    nullifier_exists,
                ),

                "is_valid_merkle_" => Function::new_native_with_env(
                    &store,
                    env.clone(),
                    is_valid_merkle,
                ),
            }
        };

        debug!(target: "wasm_runtime::new", "Instantiating module");
        let instance = Instance::new(&module, &import_object)?;

        Ok(Self { instance, env })
    }

    /// Run the hardcoded `ENTRYPOINT` function with the given payload as input.
    pub fn run(&mut self, payload: &[u8]) -> Result<()> {
        // Get module linear memory
        let memory = self.memory()?;

        // Retrieve ptr to pass data, and write the payload into the vm memory
        let mem_offset = self.guest_mem_alloc(payload.len())?;
        memory.write(mem_offset, payload)?;

        debug!(target: "wasm_runtime::run", "Getting entrypoint function");
        let entrypoint = self.instance.exports.get_function(ENTRYPOINT)?;

        debug!(target: "wasm_runtime::run", "Executing wasm");
        let ret = match entrypoint.call(&[Value::I32(mem_offset as i32)]) {
            Ok(v) => {
                self.print_logs();
                debug!(target: "wasm_runtime::run", "{}", self.gas_info());
                v
            }
            Err(e) => {
                self.print_logs();
                debug!(target: "wasm_runtime::run", "{}", self.gas_info());
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
            // _ => Err(ContractError(retval)),
            _ => todo!(),
        }
    }

    fn print_logs(&self) {
        let logs = self.env.logs.lock().unwrap();
        for msg in logs.iter() {
            debug!(target: "wasm_runtime::run", "Contract log: {}", msg);
        }
    }

    fn gas_info(&self) -> String {
        let remaining_points = get_remaining_points(&self.instance);

        match remaining_points {
            MeteringPoints::Remaining(rem) => {
                format!("Gas used: {}/{}", GAS_LIMIT - rem, GAS_LIMIT)
            }
            MeteringPoints::Exhausted => {
                format!("Gas fully exhausted: {}/{}", GAS_LIMIT + 1, GAS_LIMIT)
            }
        }
    }

    /// Allocate some memory space on a wasm linear memory to allow direct rw.
    fn guest_mem_alloc(&self, size: usize) -> Result<u32> {
        let mem_alloc = self.instance.exports.get_function(WASM_MEM_ALLOC)?;
        let res_target_ptr = mem_alloc.call(&[Value::I32(size as i32)])?.to_vec();
        Ok(res_target_ptr[0].unwrap_i32() as u32)
    }

    /// Retrieve linear memory from a wasm module and return its reference.
    fn memory(&self) -> Result<&Memory> {
        Ok(self.instance.exports.get_memory(MEMORY)?)
    }
}
