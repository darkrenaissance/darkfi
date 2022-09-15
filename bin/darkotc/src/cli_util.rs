use std::process::exit;

use halo2_proofs::pasta::group::ff::PrimeField;

use darkfi::{crypto::types::DrkTokenId, util::decode_base10, Result};

pub fn parse_value_pair(s: &str) -> Result<(u64, u64)> {
    let v: Vec<&str> = s.split(':').collect();
    if v.len() != 2 {
        eprintln!("Invalid value pair. Use a pair such as '13.37:11.0'.");
        exit(1);
    }

    // TODO: We shouldn't be hardcoding everything to 8 decimals.
    let val0 = decode_base10(v[0], 8, true);
    let val1 = decode_base10(v[1], 8, true);

    if val0.is_err() || val1.is_err() {
        eprintln!("Invalid value pair. Use a pair such as '13.37:11.0'.");
        exit(1);
    }

    Ok((val0.unwrap(), val1.unwrap()))
}

pub fn parse_token_pair(s: &str) -> Result<(String, String)> {
    let v: Vec<&str> = s.split(':').collect();
    if v.len() != 2 {
        eprintln!("Invalid token pair. Use a pair such as:");
        eprintln!("A7f1RKsCUUHrSXA7a9ogmwg8p3bs6F47ggsW826HD4yd:FCuoMii64H5Ee4eVWBjP18WTFS8iLUJmGi16Qti1xFQ2");
        exit(1);
    }

    let tok0 = bs58::decode(v[0]).into_vec();
    let tok1 = bs58::decode(v[1]).into_vec();

    if tok0.is_err() || tok1.is_err() {
        eprintln!("Invalid token pair. Use a pair such as:");
        eprintln!("A7f1RKsCUUHrSXA7a9ogmwg8p3bs6F47ggsW826HD4yd:FCuoMii64H5Ee4eVWBjP18WTFS8iLUJmGi16Qti1xFQ2");
        exit(1);
    }

    if tok0.as_ref().unwrap().len() != 32 ||
        DrkTokenId::from_repr(tok0.unwrap().try_into().unwrap()).is_some().unwrap_u8() == 0
    {
        eprintln!("Error: {} is not a valid token ID", v[0]);
        exit(1);
    }

    if tok1.as_ref().unwrap().len() != 32 ||
        DrkTokenId::from_repr(tok1.unwrap().try_into().unwrap()).is_some().unwrap_u8() == 0
    {
        eprintln!("Error: {} is not a valid token ID", v[1]);
        exit(1);
    }

    Ok((v[0].to_string(), v[1].to_string()))
}
