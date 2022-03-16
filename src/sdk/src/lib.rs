pub mod entrypoint;
pub mod error;
pub mod log;

// Set up global allocator by default
#[cfg(target_arch = "wasm32")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;
