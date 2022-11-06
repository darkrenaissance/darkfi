use darkfi_sdk::{
    crypto::ContractId,
    db::{db_get, db_init, db_lookup, db_set},
    define_contract,
    error::ContractResult,
    msg,
    pasta::pallas,
    tx::FuncCall,
    util::{set_return_data, put_object_bytes, get_object_bytes, get_object_size},
};
use darkfi_serial::{deserialize, serialize, Encodable, SerialDecodable, SerialEncodable, WriteExt, ReadExt};

#[repr(u8)]
pub enum DaoFunction {
    Foo = 0x00,
}

fn foo() {
    println!("foo");
}

define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    Ok(())
}
fn get_metadata(_cid: ContractId, ix: &[u8]) -> ContractResult {
    let zk_public_values = vec![
        (
            "DaoProposeInput".to_string(),
            vec![pallas::Base::from(110), pallas::Base::from(4)],
        ),
        ("DaoProposeInput".to_string(), vec![pallas::Base::from(7), pallas::Base::from(4)]),
        (
            "DaoProposeMain".to_string(),
            vec![
                pallas::Base::from(1),
                pallas::Base::from(3),
                pallas::Base::from(5),
                pallas::Base::from(7),
            ],
        ),
    ];

    let signature_public_keys: Vec<pallas::Point> = vec![
        //pallas::Point::identity()
    ];

    let mut metadata = Vec::new();
    zk_public_values.encode(&mut metadata)?;
    signature_public_keys.encode(&mut metadata)?;
    set_return_data(&metadata)?;

    Ok(())
}
fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    Ok(())
}
fn process_update(_cid: ContractId, update_data: &[u8]) -> ContractResult {
    Ok(())
}
