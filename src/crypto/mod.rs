pub mod mint_proof;
pub mod spend_proof;

use bellman::groth16;
use bls12_381::Bls12;

use crate::error::Result;
pub use mint_proof::{create_mint_proof, setup_mint_prover, verify_mint_proof, MintRevealedValues};
pub use spend_proof::{
    create_spend_proof, setup_spend_prover, verify_spend_proof, SpendRevealedValues,
};

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
