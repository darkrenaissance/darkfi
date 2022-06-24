use std::time::Instant;

use halo2_proofs::circuit::Value;
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
        path: Value::known(coin.path.unwrap()),
        coin_pk_x: Value::known(coin.pk_x.unwrap()),
        coin_pk_y: Value::known(coin.pk_y.unwrap()),
        root_sk: Value::known(coin.root_sk.unwrap()),
        sf_root_sk: Value::known(mod_r_p(coin.root_sk.unwrap())),
        path_sk: Value::known(coin.path_sk.unwrap()),
        coin_timestamp: Value::known(coin.tau.unwrap()),
        coin_nonce: Value::known(coin.nonce.unwrap()),
        coin1_blind: Value::known(coin.c1_blind.unwrap()),
        value: Value::known(coin.value.unwrap()),
        coin2_blind: Value::known(coin.c2_blind.unwrap()),
        cm_pos: Value::known(coin.idx),
        //sn_c1: Value::known(coin.sn.unwrap()),
        slot: Value::known(coin.sl.unwrap()),
        mau_rho: Value::known(mod_r_p(mau_rho)),
        mau_y: Value::known(mod_r_p(mau_y)),
        root_cm: Value::known(coin.root_cm.unwrap()),
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
