use super::{
    crypto::{MerkleNode, Nullifier},
    error::{ContractError, ContractResult},
};

pub trait Verification {
    fn verify(&self) -> ContractResult;
}

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

pub fn is_valid_merkle(merkle_root: &MerkleNode) -> Result<bool, ContractError> {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        // Convert to bytes, and pass pointer to first byte in slice to the function.
        let mr = merkle_root.to_bytes();
        return match is_valid_merkle_(&mr as *const u8, 32) {
            0 => Ok(false),
            1 => Ok(true),
            -1 => Err(ContractError::ValidMerkleCheck),
            -2 => Err(ContractError::Internal),
            _ => unreachable!(),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    todo!("is_valid_merkle({:?}", merkle_root);
}

#[cfg(target_arch = "wasm32")]
extern "C" {
    fn nullifier_exists_(ptr: *const u8, len: u32) -> i32;
    fn is_valid_merkle_(ptr: *const u8, len: u32) -> i32;
}
