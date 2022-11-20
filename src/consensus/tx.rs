use crate::{
    consensus::{EncryptedTxRcpt, TransferStx},
    serial::darkfi_derive::{SerialDecodable, SerialEncodable};
};

/// transfer transaction
#[derive(Debug, Clone, Copy, SerialDecodable, SerialEncodable)]
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

    pub fn leadcoin(&self)  -> LeadCoin {
        //
    }
}
