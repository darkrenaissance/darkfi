use darkfi_serial::{Encodable, Decodable, SerialDecodable, SerialEncodable};
use crate::{
    consensus::{EncryptedTxRcpt, leadcoin::TransferStx},
};

/// transfer transaction
#[derive(Debug, Clone, SerialDecodable, SerialEncodable)]
pub struct Tx {
    pub xfer: TransferStx,
    pub cipher: EncryptedTxRcpt,
}

impl Tx {
    /// verify transfer transaction
    pub fn verify(&self) -> bool{
        //TODO: verify tx
        true
    }
}
