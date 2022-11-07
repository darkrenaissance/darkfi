use darkfi_sdk::{
    crypto::ContractId,
    db::{db_init, db_lookup, db_set},
    define_contract,
    error::ContractResult,
    pasta::pallas,
    util::set_return_data,
};
use darkfi_serial::{serialize, Encodable, SerialDecodable, SerialEncodable};

#[repr(u8)]
pub enum MoneyFunction {
    Foo = 0x00,
    Mint = 0x01,
}

#[derive(SerialEncodable, SerialDecodable)]
pub struct MoneyMintParams {
    pub a: u32,
    pub b: u32,
}

define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    let db_handle = db_init(cid, "wagies")?;

    Ok(())
}
fn get_metadata(_cid: ContractId, ix: &[u8]) -> ContractResult {
    let zk_public_values: Vec<(String, Vec<pallas::Base>)> = Vec::new();
    let signature_public_keys: Vec<pallas::Point> = Vec::new();

    let mut metadata = Vec::new();
    zk_public_values.encode(&mut metadata)?;
    signature_public_keys.encode(&mut metadata)?;
    set_return_data(&metadata)?;

    Ok(())
}
fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    Ok(())
}
fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    let db_handle = db_lookup(cid, "wagies")?;
    db_set(db_handle, &serialize(&"jason_gulag".to_string()), &serialize(&110))?;
    Ok(())
}
