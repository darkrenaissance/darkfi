use super::{
    crypto::{MerkleNode, Nullifier},
    error::{ContractError, GenericResult},
};

type DbHandle = u32;
type TxHandle = u32;

/// Only deploy() can call this. Creates a new database instance for this contract.
///
/// ```
///     type DbHandle = u32;
///     db_init(db_name) -> DbHandle
/// ```
pub fn db_init(db_name: &str) -> GenericResult<DbHandle> {
    // FIXME: how do I return the u32 db handle from db_init?
    // I also want the status (whether an error occurred or success).
    #[cfg(target_arch = "wasm32")]
    unsafe {
        return match db_init_(message.as_ptr(), message.len() as u32) {
            0 => Ok(110),
            -1 => Err(ContractError::CallerAccessDenied),
            -2 => Err(ContractError::DbInitFailed)
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
    Ok(Vec::new())
}

/// Only update() can call this. Starts an atomic transaction.
///
/// ```
///     tx_handle = db_begin_tx();
/// ```
pub fn db_begin_tx() -> GenericResult<TxHandle> {
    Ok(4)
}

/// Only update() can call this. Set a value within the transaction.
///
/// ```
///     db_set(tx_handle, key, value);
/// ```
pub fn db_set(tx_handle: TxHandle, key: &[u8], value: Vec<u8>) -> GenericResult<()> {
    // Check entry for tx_handle is not None
    Ok(())
}

/// Only update() can call this. This writes the atomic tx to the database.
///
/// ```
///     db_end_tx(db_handle, tx_handle);
/// ```
pub fn db_end_tx(db_handle: DbHandle, tx_handle: TxHandle) -> GenericResult<()> {
    // Don't forget to set the entry for the tx in the table to empty.
    Ok(())
}

#[cfg(target_arch = "wasm32")]
extern "C" {
    fn get_update_() -> i32;
    fn set_update_(ptr: *const u8, len: u32) -> i32;
    fn nullifier_exists_(ptr: *const u8, len: u32) -> i32;
    fn is_valid_merkle_(ptr: *const u8, len: u32) -> i32;

    fn db_init_(ptr: *const u8, len: usize) -> i32;
    fn db_get_() -> i32;
    fn db_begin_tx_() -> i32;
    fn db_set_() -> i32;
    fn db_end_tx_() -> i32;
}
