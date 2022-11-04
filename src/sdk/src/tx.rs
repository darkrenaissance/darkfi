use darkfi_serial::{SerialDecodable, SerialEncodable};
use pasta_curves::{
    group::ff::{Field, PrimeField},
    pallas,
};

type ContractId = pallas::Base;
type FuncId = pallas::Base;

#[derive(SerialEncodable, SerialDecodable)]
pub struct Transaction {
    pub func_calls: Vec<FuncCall>,
    // This should also be bytes?
    //pub signatures: Vec<Signature>,
    pub signatures: Vec<Vec<u8>>,
}

#[derive(SerialEncodable, SerialDecodable)]
pub struct FuncCall {
    pub contract_id: ContractId,
    pub func_id: FuncId,
    pub call_data: Vec<u8>,
    // This should also be bytes?
    //pub proofs: Vec<Proof>,
    pub proofs: Vec<Vec<u8>>,
}
