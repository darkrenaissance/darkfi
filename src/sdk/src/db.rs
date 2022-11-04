use super::error::GenericResult;

type DbHandle = u32;
type TxHandle = u32;

/// Only deploy() can call this. Creates a new database instance for this contract.
///
/// ```
///     type DbHandle = u32;
///     db_init(db_name) -> DbHandle
/// ```
pub fn db_init(db_name: &str) -> GenericResult<()> {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        return match db_init_(db_name.as_ptr(), db_name.len() as u32) {
            0 => Ok(()),
            -1 => Err(ContractError::CallerAccessDenied),
            -2 => Err(ContractError::DbInitFailed),
            _ => unreachable!(),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    todo!("{}", db_name);
}

pub fn db_lookup(db_name: &str) -> GenericResult<DbHandle> {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        return match db_lookup_(db_name.as_ptr(), db_name.len() as u32) {
            handle => {
                if handle < 0 {
                    unreachable!();
                }
                Ok(handle as u32)
            }
            -1 => Err(ContractError::CallerAccessDenied),
            -2 => Err(ContractError::DbNotFound),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    todo!("{}", db_name);
}

/// Everyone can call this. Will read a key from the key-value store.
///
/// ```
///     value = db_get(db_handle, key);
/// ```
pub fn db_get(db_handle: DbHandle, key: &[u8]) -> GenericResult<Vec<u8>> {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        return match db_get_() {
            0 => Ok(Vec::new()),
            -1 => Err(ContractError::CallerAccessDenied),
            _ => unreachable!(),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    todo!("db_get");
}

/// Only update() can call this. Starts an atomic transaction.
///
/// ```
///     tx_handle = db_begin_tx();
/// ```
pub fn db_begin_tx() -> GenericResult<TxHandle> {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        return match db_begin_tx_() {
            0 => Ok(4),
            -1 => Err(ContractError::CallerAccessDenied),
            _ => unreachable!(),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    todo!("db_begin_tx");
}

/// Only update() can call this. Set a value within the transaction.
///
/// ```
///     db_set(tx_handle, key, value);
/// ```
pub fn db_set(tx_handle: TxHandle, key: &[u8], value: Vec<u8>) -> GenericResult<()> {
    // Check entry for tx_handle is not None
    #[cfg(target_arch = "wasm32")]
    unsafe {
        return match db_set_() {
            0 => Ok(()),
            -1 => Err(ContractError::CallerAccessDenied),
            _ => unreachable!(),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    todo!("db_set");
}

/// Only update() can call this. This writes the atomic tx to the database.
///
/// ```
///     db_end_tx(db_handle, tx_handle);
/// ```
pub fn db_end_tx(db_handle: DbHandle, tx_handle: TxHandle) -> GenericResult<()> {
    // Don't forget to set the entry for the tx in the table to empty.
    #[cfg(target_arch = "wasm32")]
    unsafe {
        return match db_end_tx_() {
            0 => Ok(()),
            -1 => Err(ContractError::CallerAccessDenied),
            _ => unreachable!(),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    todo!("db_end_tx");
}

#[cfg(target_arch = "wasm32")]
extern "C" {
    fn get_update_() -> i32;
    fn set_update_(ptr: *const u8, len: u32) -> i32;
    fn nullifier_exists_(ptr: *const u8, len: u32) -> i32;
    fn is_valid_merkle_(ptr: *const u8, len: u32) -> i32;

    fn db_init_(ptr: *const u8, len: u32) -> i32;
    fn db_lookup_(ptr: *const u8, len: u32) -> i32;
    fn db_get_() -> i32;
    fn db_begin_tx_() -> i32;
    fn db_set_() -> i32;
    fn db_end_tx_() -> i32;
}
