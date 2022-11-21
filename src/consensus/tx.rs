use crate::consensus::{EncryptedTxRcpt, TransferStx};
use darkfi_serial::{Decodable, Encodable, SerialDecodable, SerialEncodable};

/// transfer transaction
#[derive(Debug, Clone, SerialDecodable, SerialEncodable)]
pub struct Tx {
    pub xfer: TransferStx,
    pub cipher: EncryptedTxRcpt,
}

impl Tx {
    /// verify transfer transaction
    pub fn verify(&self) -> bool {
        //TODO: verify tx
        true
    }
}
