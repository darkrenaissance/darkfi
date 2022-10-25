/// Main wasm vm runtime implementation
pub mod vm_runtime;

/// VM memory access (read/write)
pub(crate) mod memory;

/// Utility functions
pub mod util;

/// Host functions for querying blockchain state through `MemoryState`
pub(crate) mod chain_state;
