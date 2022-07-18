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
        leadcoin::LeadCoin,
        proof::{Proof, ProvingKey, VerifyingKey},
        util::mod_r_p,
    },
    zk::circuit::lead_contract::LeadContract,
    Result, VerifyResult, VerifyFailed,
};

use rand::{thread_rng, Rng};

#[allow(clippy::too_many_arguments)]
pub fn create_lead_proof(pk: ProvingKey, coin: LeadCoin) -> Result<Proof> {
    //
    //let mut rng = thread_rng();
    //let yu64: u64 = rng.gen();
    //let rhou64: u64 = rng.gen();
    //let mau_y: pallas::Base = pallas::Base::from(yu64);
    //let mau_rho: pallas::Base = pallas::Base::from(rhou64);
    let contract = coin.create_contract();
    //let start = Instant::now();
    let public_inputs = coin.public_inputs();
    println!("creating proof");
    let proof = Proof::create(&pk, &[contract], &public_inputs, &mut OsRng)?;
    println!("proof created");
    //debug!("Prove lead: [{:?}]", start.elapsed());
    Ok(proof)
}

pub fn verify_lead_proof(vk: &VerifyingKey, proof: &Proof, coin: LeadCoin) -> VerifyResult<()> {
    let start = Instant::now();
    let public_inputs = coin.public_inputs();
    match proof.verify(vk, &public_inputs) {
        Ok(()) => {Ok(())},
        Err(e) => {
            error!("lead verification failed: {}", e);
            Err(VerifyFailed::InternalError("lead verification failure".to_string()))
        }
    }
}
