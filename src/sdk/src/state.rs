use super::{crypto::Nullifier, error::ContractError};

pub fn nullifier_exists(nullifier: &Nullifier) -> Result<bool, ContractError> {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        // Convert to bytes, and pass pointer to first byte in slice to the function.
        let nf = nullifier.to_bytes();
        return match nullifier_exists_(&nf as *const u8, 32) {
            0 => Ok(false),
            1 => Ok(true),
            -1 => Err(ContractError::NullifierExistCheck),
            -2 => Err(ContractError::Internal),
            _ => unreachable!(),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    todo!("nullifier_exists({:?}", nullifier);
}

#[cfg(target_arch = "wasm32")]
extern "C" {
    fn nullifier_exists_(ptr: *const u8, len: u32) -> i32;
}
