use std::time::Instant;


use halo2_proofs::circuit::Value;
use log::{
    debug,
    error
};
use pasta_curves::pallas;
use rand::rngs::OsRng;

use crate::{
    crypto::{
        types::*,
        leadcoin::LeadCoin,
        proof::{Proof, ProvingKey, VerifyingKey},
        util::mod_r_p,
    },
    zk::circuit::lead_contract::LeadContract,
    Result, VerifyResult, VerifyFailed,
};

use rand::{thread_rng, Rng};

//#[allow(clippy::too_many_arguments)]
pub fn create_lead_proof(pk: &ProvingKey, coin: LeadCoin) -> Result<Proof> {
    println!("[creating lead proof]");
    let contract = coin.create_contract();
    //let start = Instant::now();
    println!("[creating lead proof] public inputs");
    let public_inputs = coin.public_inputs();
    println!("[creating lead proof] Proof create");
    let proof = Proof::create(&pk, &[contract], &public_inputs, &mut OsRng)?;
    println!("[creating lead proof] proof successfully created");
    //debug!("Prove lead: [{:?}]", start.elapsed());
    Ok(proof)
}

pub fn verify_lead_proof(vk: &VerifyingKey,
                         proof: &Proof,
                         public_inputs: &[DrkCircuitField]) -> VerifyResult<()> {
    let start = Instant::now();
    match proof.verify(vk, public_inputs) {
        Ok(()) => {Ok(())},
        Err(e) => {
            error!("lead verification failed: {}", e);
            Err(VerifyFailed::InternalError("lead verification failure".to_string()))
        }
    }
}
