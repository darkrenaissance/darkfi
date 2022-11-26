use darkfi_serial::{SerialDecodable, SerialEncodable};

use crate::consensus::{EncryptedTxRcpt, TransferStx};

/// transfer transaction
#[derive(Debug, Clone, SerialDecodable, SerialEncodable)]
pub struct Tx {
    pub xfer: TransferStx,
    pub cipher: EncryptedTxRcpt,
}
