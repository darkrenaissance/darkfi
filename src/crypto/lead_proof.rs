use std::time::Instant;

use log::debug;
use pasta_curves::pallas;
use rand::rngs::OsRng;

use crate::{
    crypto::{
        leadcoin::LeadCoin,
        proof::{Proof, ProvingKey, VerifyingKey},
        util::mod_r_p,
    },
    zk::circuit::lead_contract::LeadContract,
    Result,
};

use rand::{thread_rng, Rng};

#[allow(clippy::too_many_arguments)]
pub fn create_lead_proof(pk: ProvingKey, coin: LeadCoin) -> Result<Proof> {
    //
    let mut rng = thread_rng();
    let yu64: u64 = rng.gen();
    let rhou64: u64 = rng.gen();
    let mau_y: pallas::Base = pallas::Base::from(yu64);
    let mau_rho: pallas::Base = pallas::Base::from(rhou64);
    let contract = LeadContract {
        path: coin.path,
        coin_pk_x: coin.pk_x,
        coin_pk_y: coin.pk_y,
        root_sk: coin.root_sk,
        sf_root_sk: Some(mod_r_p(coin.root_sk.unwrap())),
        path_sk: coin.path_sk,
        coin_timestamp: coin.tau, //
        coin_nonce: coin.nonce,
        coin_opening_1: Some(mod_r_p(coin.opening1.unwrap())),
        value: coin.value,
        coin_opening_2: Some(mod_r_p(coin.opening2.unwrap())),
        cm_pos: Some(coin.idx),
        //sn_c1: Some(coin.sn.unwrap()),
        slot: Some(coin.sl.unwrap()),
        mau_rho: Some(mod_r_p(mau_rho)),
        mau_y: Some(mod_r_p(mau_y)),
        root_cm: Some(coin.root_cm.unwrap()),
    };

    let start = Instant::now();
    let public_inputs = coin.public_inputs();
    let proof = Proof::create(&pk, &[contract], &public_inputs, &mut OsRng)?;
    debug!("Prove lead: [{:?}]", start.elapsed());
    Ok(proof)
}

pub fn verify_lead_proof(vk: &VerifyingKey, proof: &Proof, coin: LeadCoin) -> Result<()> {
    let start = Instant::now();
    let public_inputs = coin.public_inputs();
    proof.verify(vk, &public_inputs)?;
    debug!("Verify lead: [{:?}]", start.elapsed());
    Ok(())
}
