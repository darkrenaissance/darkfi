#[cfg(target_arch = "wasm32")]
use super::error::ContractError;

#[cfg(target_arch = "wasm32")]
pub fn set_return_data(data: &[u8]) -> Result<(), ContractError> {
    unsafe {
        return match set_return_data_(data.as_ptr(), data.len() as u32) {
            0 => Ok(()),
            errcode => Err(ContractError::from(errcode)),
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub fn put_object_bytes(data: &[u8]) -> i64 {
    unsafe { return put_object_bytes_(data.as_ptr(), data.len() as u32) }
}

#[cfg(target_arch = "wasm32")]
pub fn get_object_bytes(data: &mut [u8], object_index: u32) -> i64 {
    unsafe { return get_object_bytes_(data.as_mut_ptr(), object_index as u32) }
}

#[cfg(target_arch = "wasm32")]
pub fn get_object_size(object_index: u32) -> i64 {
    unsafe { return get_object_size_(object_index as u32) }
}

#[cfg(target_arch = "wasm32")]
extern "C" {
    fn set_return_data_(ptr: *const u8, len: u32) -> i64;
    fn put_object_bytes_(ptr: *const u8, len: u32) -> i64;
    fn get_object_bytes_(ptr: *mut u8, len: u32) -> i64;
    fn get_object_size_(len: u32) -> i64;
}
