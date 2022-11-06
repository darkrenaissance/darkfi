use darkfi_sdk::{
    crypto::ContractId,
    db::{db_get, db_init, db_lookup, db_set},
    define_contract,
    error::ContractResult,
    msg,
    pasta::pallas,
    tx::ContractCall,
    util::{set_return_data, put_object_bytes, get_object_bytes, get_object_size},
};
use darkfi_serial::{deserialize, serialize, Encodable, SerialDecodable, SerialEncodable, WriteExt, ReadExt};

#[repr(u8)]
pub enum DaoFunction {
    Foo = 0x00,
    Mint = 0x01,
}

#[derive(SerialEncodable, SerialDecodable)]
pub struct DaoMintParams {
    pub a: u32,
    pub b: u32
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
