use log::{error, warn};
use wasmer::{AsStoreRef, FunctionEnvMut, WasmPtr};

use crate::runtime::{memory::MemoryManipulation, vm_runtime::Env};

/// Host function for logging strings.
/// This is injected into the runtime with wasmer's `imports!` macro.
pub(crate) fn drk_log(mut ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, len: u32) {
    let env = ctx.data();
    let memory_view = env.memory_view(&ctx);

    match ptr.read_utf8_string(&memory_view, len) {
        Ok(msg) => {
            let mut logs = env.logs.borrow_mut();
            logs.push(msg);
            std::mem::drop(logs);
        }
        Err(_) => {
            error!(target: "wasm_runtime::drk_log", "Failed to read UTF-8 string from VM memory");
        }
    }
}
