// TODO
// money-contract/
// state.apply()
//      transfer/
//          Builder
//          Partial *
//          FuncCall
use std::io;

use log::error;
use pasta_curves::group::Group;

use darkfi::{
    crypto::{
        burn_proof::verify_burn_proof,
        keypair::PublicKey,
        mint_proof::verify_mint_proof,
        note::EncryptedNote,
        proof::VerifyingKey,
        schnorr,
        schnorr::SchnorrPublic,
        types::{DrkTokenId, DrkValueBlind, DrkValueCommit},
        util::{pedersen_commitment_base, pedersen_commitment_u64},
        BurnRevealedValues, MintRevealedValues, Proof,
    },
    util::serial::{Encodable, SerialDecodable, SerialEncodable, VarInt},
    Result, VerifyFailed, VerifyResult,
};

pub mod transfer;
