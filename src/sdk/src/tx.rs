use darkfi_serial::{SerialDecodable, SerialEncodable};
use pasta_curves::pallas;

type ContractId = pallas::Base;
type FuncId = pallas::Base;

#[derive(SerialEncodable, SerialDecodable)]
pub struct FuncCall {
    pub contract_id: ContractId,
    pub func_id: FuncId,
    pub call_data: Vec<u8>,
}
