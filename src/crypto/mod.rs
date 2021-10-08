pub mod coin;
pub mod diffie_hellman;
pub mod fr_serial;
pub mod merkle;
pub mod merkle_node;
pub mod mint_proof;
pub mod note;
pub mod nullifier;
pub mod schnorr;
pub mod spend_proof;
pub mod util;

use bellman::groth16;
use bls12_381::Bls12;

use crate::error::Result;
pub use mint_proof::{create_mint_proof, setup_mint_prover, verify_mint_proof, MintRevealedValues};
pub use spend_proof::{
    create_spend_proof, setup_spend_prover, verify_spend_proof, SpendRevealedValues,
};

#[derive(Clone)]
pub struct OwnCoin {
    pub coin: coin::Coin,
    pub note: note::Note,
    pub secret: jubjub::Fr,
    pub witness: merkle::IncrementalWitness<merkle_node::MerkleNode>,
}

pub type OwnCoins = Vec<(u64, OwnCoin)>;

pub fn save_params(filename: &str, params: &groth16::Parameters<Bls12>) -> Result<()> {
    let buffer = std::fs::File::create(filename)?;
    params.write(buffer)?;
    Ok(())
}

pub fn load_params(
    filename: &str,
) -> Result<(
    groth16::Parameters<Bls12>,
    groth16::PreparedVerifyingKey<Bls12>,
)> {
    let buffer = std::fs::File::open(filename)?;
    let params = groth16::Parameters::<Bls12>::read(buffer, false)?;
    let pvk = groth16::prepare_verifying_key(&params.vk);
    Ok((params, pvk))
}
