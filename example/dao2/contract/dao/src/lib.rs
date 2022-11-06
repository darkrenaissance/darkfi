use darkfi_sdk::{
    crypto::{ContractId, constants::MERKLE_DEPTH, MerkleNode, Nullifier},
    db::{db_get, db_init, db_lookup, db_set},
    define_contract,
    error::ContractResult,
    msg,
    merkle::merkle_add,
    pasta::pallas,
    tx::ContractCall,
    util::{get_object_bytes, get_object_size, put_object_bytes, set_return_data},
    incrementalmerkletree::{bridgetree::BridgeTree, Tree}
};
use darkfi_serial::{
    deserialize, serialize, Encodable, ReadExt, SerialDecodable, SerialEncodable, WriteExt,
};

type MerkleTree = BridgeTree<MerkleNode, { MERKLE_DEPTH }>;

#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct DaoBulla(pub pallas::Base);

#[repr(u8)]
pub enum DaoFunction {
    Foo = 0x00,
    Mint = 0x01,
}

impl From<u8> for DaoFunction {
    fn from(b: u8) -> Self {
        match b {
            0x00 => Self::Foo,
            0x01 => Self::Mint,
            _ => panic!("Invalid function ID: {:#04x?}", b),
        }
    }
}

#[derive(SerialEncodable, SerialDecodable)]
pub struct DaoMintParams {
    pub dao_bulla: DaoBulla,
}
#[derive(SerialEncodable, SerialDecodable)]
pub struct DaoMintUpdate {
    pub dao_bulla: DaoBulla,
}

define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    let db_handle = db_init(cid, "info")?;

    let dao_tree = MerkleTree::new(100);
    let dao_tree_data = serialize(&dao_tree);
    db_set(db_handle, &serialize(&"dao_tree".to_string()), &dao_tree_data)?;

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
    let (call_idx, call): (u32, Vec<ContractCall>) = deserialize(ix)?;

    assert!(call_idx < call.len() as u32);
    let self_ = &call[call_idx as usize];

    match DaoFunction::from(self_.data[0]) {
        DaoFunction::Mint => {
            let data = &self_.data[1..];
            let params: DaoMintParams = deserialize(data)?;

            // No checks in Mint. Just return the update.

            let update = DaoMintUpdate { dao_bulla: params.dao_bulla };

            let mut update_data = Vec::new();
            update_data.write_u8(DaoFunction::Mint as u8);
            update.encode(&mut update_data);
            set_return_data(&update_data)?;
            msg!("update is set!");
        }
        DaoFunction::Foo => {
            unimplemented!();
        }
    }

    Ok(())
}
fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    match DaoFunction::from(update_data[0]) {
        DaoFunction::Mint => {
            let data = &update_data[1..];
            let update: DaoMintUpdate = deserialize(data)?;

            let db_handle = db_lookup(cid, "info")?;
            let node = MerkleNode::new(update.dao_bulla.0);
            merkle_add(db_handle, &serialize(&"dao_tree".to_string()), &node)?;
        }
        DaoFunction::Foo => {
            unimplemented!();
        }
    }

    Ok(())
}
