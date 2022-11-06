use darkfi_serial::{SerialDecodable, SerialEncodable};
use pasta_curves::pallas;

use super::crypto::ContractId;

#[derive(SerialEncodable, SerialDecodable)]
pub struct ContractCall {
    pub contract_id: ContractId,
    pub data: Vec<u8>,
}
