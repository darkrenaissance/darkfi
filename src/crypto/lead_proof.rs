use log::error;

use rand::rngs::OsRng;

use crate::{
    crypto::{
        leadcoin::LeadCoin,
        proof::{Proof, ProvingKey, VerifyingKey},
        types::*,
    },
    Result, VerifyFailed, VerifyResult,
};

#[allow(clippy::too_many_arguments)]
pub fn create_lead_proof(pk: &ProvingKey, coin: LeadCoin) -> Result<Proof> {
    let contract = coin.create_contract();
    let public_inputs = coin.public_inputs();
    let proof = Proof::create(&pk, &[contract], &public_inputs, &mut OsRng)?;
    Ok(proof)
}

pub fn verify_lead_proof(
    vk: &VerifyingKey,
    proof: &Proof,
    public_inputs: &[DrkCircuitField],
) -> VerifyResult<()> {
    match proof.verify(vk, public_inputs) {
        Ok(()) => Ok(()),
        Err(e) => {
            error!("lead verification failed: {}", e);
            Err(VerifyFailed::InternalError("lead verification failure".to_string()))
        }
    }
}
