use darkfi_serial::{SerialDecodable, SerialEncodable};
use pasta_curves::pallas;

use super::{
    keypair::{PublicKey, SecretKey},
    util::poseidon_hash,
};

/// Contract ID used to reference smart contracts on the ledger.
#[repr(C)]
#[derive(Debug, Copy, Clone, SerialEncodable, SerialDecodable)]
pub struct ContractId(pallas::Base);

/// Derive a ContractId given a secret deploy key.
pub fn derive_contract_id(deploy_key: SecretKey) -> ContractId {
    let public_key = PublicKey::from_secret(deploy_key);
    let (x, y) = public_key.xy();
    let hash = poseidon_hash::<2>([x, y]);
    ContractId(hash)
}
