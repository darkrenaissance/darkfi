use group::ff::PrimeField;

use super::types::DrkTokenId;
use crate::{util::net_name::NetworkName, Error, Result};

pub fn generate_id(network: &NetworkName, token_str: &str) -> Result<DrkTokenId> {
    let mut net_bytes: Vec<u8> = network.to_string().as_bytes().to_vec();
    // TODO: Check for fixed length token_str
    let mut token_bytes = match network {
        NetworkName::DarkFi => bs58::decode(token_str).into_vec()?,
        NetworkName::Bitcoin => bs58::decode(token_str).into_vec()?,
        NetworkName::Ethereum => hex::decode(token_str.strip_prefix("0x").unwrap())?,
        NetworkName::Solana => bs58::decode(token_str).into_vec()?,
    };

    net_bytes.append(&mut token_bytes);

    // Since our circuit is built to take a 2^64-1 range, we take the first 64
    // bits of the hash and make an unsigned integer from it, which we can then
    // cast into pallas::Base.
    let data: [u8; 8] = blake3::hash(&net_bytes).as_bytes()[0..8].try_into()?;

    Ok(DrkTokenId::from(u64::from_le_bytes(data)))
}

/// Parse a `DrkTokenId` from a base58-encoded string
pub fn parse_b58(s: &str) -> Result<DrkTokenId> {
    let bytes = bs58::decode(s).into_vec()?;
    if bytes.len() != 32 {
        return Err(Error::ParseFailed("Failed parsing DrkTokenId from base58 string"))
    }

    let ret = DrkTokenId::from_repr(bytes.try_into().unwrap());
    if ret.is_some().unwrap_u8() == 1 {
        return Ok(ret.unwrap())
    }

    Err(Error::ParseFailed("Failed parsing DrkTokenId from base58 string"))
}
