use super::error::ContractError;

pub fn set_return_data(data: &[u8]) -> Result<(), ContractError> {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        return match set_return_data_(data.as_ptr(), data.len() as u32) {
            0 => Ok(()),
            errcode => Err(ContractError::from(errcode)),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    unimplemented!();
}

#[cfg(target_arch = "wasm32")]
extern "C" {
    fn set_return_data_(ptr: *const u8, len: u32) -> i64;
}
