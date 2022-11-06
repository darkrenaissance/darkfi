use darkfi_serial::Encodable;

use super::{
    crypto::ContractId,
    error::{ContractError, GenericResult},
    util::{get_object_bytes, get_object_size},
};

pub type DbHandle = u32;
type TxHandle = u32;

/// Only deploy() can call this. Creates a new database instance for this contract.
///
/// ```
///     type DbHandle = u32;
///     db_init(db_name) -> DbHandle
/// ```
pub fn db_init(contract_id: ContractId, db_name: &str) -> GenericResult<DbHandle> {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        let mut len = 0;
        let mut buf = vec![];
        len += contract_id.encode(&mut buf)?;
        len += db_name.to_string().encode(&mut buf)?;

        let ret = db_init_(buf.as_ptr(), len as u32);

        if ret < 0 {
            match ret {
                -1 => return Err(ContractError::CallerAccessDenied),
                -2 => return Err(ContractError::DbInitFailed),
                _ => unimplemented!(),
            }
        }

        return Ok(ret as u32)
    }

    #[cfg(not(target_arch = "wasm32"))]
    unimplemented!()
}

pub fn db_lookup(contract_id: ContractId, db_name: &str) -> GenericResult<DbHandle> {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        let mut len = 0;
        let mut buf = vec![];
        len += contract_id.encode(&mut buf)?;
        len += db_name.to_string().encode(&mut buf)?;

        let ret = db_lookup_(buf.as_ptr(), len as u32);

        if ret < 0 {
            match ret {
                -1 => return Err(ContractError::CallerAccessDenied),
                -2 => return Err(ContractError::DbLookupFailed),
                _ => unimplemented!(),
            }
        }

        return Ok(ret as u32)
    }

    #[cfg(not(target_arch = "wasm32"))]
    unimplemented!()
}

/// Everyone can call this. Will read a key from the key-value store.
///
/// ```
///     value = db_get(db_handle, key);
/// ```
pub fn db_get(db_handle: DbHandle, key: &[u8]) -> GenericResult<Option<Vec<u8>>> {
    #[cfg(target_arch = "wasm32")]
    {
        let mut len = 0;
        let mut buf = vec![];
        len += db_handle.encode(&mut buf)?;
        len += key.to_vec().encode(&mut buf)?;

        let ret = unsafe { db_get_(buf.as_ptr(), len as u32) };

        if ret < 0 {
            match ret {
                -1 => return Err(ContractError::CallerAccessDenied),
                -2 => return Err(ContractError::DbGetFailed),
                -3 => return Ok(None),
                _ => unimplemented!(),
            }
        }

        let obj = ret as u32;
        let obj_size = get_object_size(obj);
        let mut buf = vec![0u8; obj_size as usize];
        get_object_bytes(&mut buf, obj);

        Ok(Some(buf))
    }

    #[cfg(not(target_arch = "wasm32"))]
    unimplemented!()
}

/// Only update() can call this. Set a value within the transaction.
///
/// ```
///     db_set(tx_handle, key, value);
/// ```
pub fn db_set(db_handle: DbHandle, key: &[u8], value: &[u8]) -> GenericResult<()> {
    // Check entry for tx_handle is not None
    #[cfg(target_arch = "wasm32")]
    unsafe {
        let mut len = 0;
        let mut buf = vec![];
        len += db_handle.encode(&mut buf)?;
        len += key.to_vec().encode(&mut buf)?;
        len += value.to_vec().encode(&mut buf)?;

        return match db_set_(buf.as_ptr(), len as u32) {
            0 => Ok(()),
            -1 => Err(ContractError::CallerAccessDenied),
            -2 => Err(ContractError::DbSetFailed),
            _ => unreachable!(),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    unimplemented!()
}

#[cfg(target_arch = "wasm32")]
extern "C" {
    fn db_init_(ptr: *const u8, len: u32) -> i32;
    fn db_lookup_(ptr: *const u8, len: u32) -> i32;
    fn db_get_(ptr: *const u8, len: u32) -> i64;
    fn db_set_(ptr: *const u8, len: u32) -> i32;
}
