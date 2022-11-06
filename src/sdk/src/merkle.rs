use darkfi_serial::Encodable;

use super::{
    crypto::{ContractId, MerkleNode},
    db::DbHandle,
    error::{ContractError, GenericResult},
    util::{get_object_bytes, get_object_size},
};

pub fn merkle_add(
    db_info: DbHandle,
    db_roots: DbHandle,
    key: &[u8],
    coin: &MerkleNode,
) -> GenericResult<()> {
    let mut buf = vec![];
    let mut len = 0;
    len += db_info.encode(&mut buf)?;
    len += db_roots.encode(&mut buf)?;
    len += key.to_vec().encode(&mut buf)?;
    len += coin.encode(&mut buf)?;
    return match unsafe { merkle_add_(buf.as_ptr(), len as u32) } {
        0 => Ok(()),
        -1 => Err(ContractError::CallerAccessDenied),
        -2 => Err(ContractError::DbSetFailed),
        _ => unreachable!(),
    }
}

extern "C" {
    fn merkle_add_(ptr: *const u8, len: u32) -> i32;
}
