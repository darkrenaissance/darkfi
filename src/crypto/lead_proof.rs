use std::time::Instant;

use halo2_gadgets::primitives::{
    poseidon,
    poseidon::{ConstantLength, P128Pow5T3},
};
use log::debug;
use pasta_curves::{arithmetic::CurveAffine, group::Curve, pallas};
use rand::rngs::OsRng;

use crate::{
    crypto::{
        leadcoin::{LeadCoin},
        keypair::PublicKey,
        proof::{Proof, ProvingKey, VerifyingKey},
        types::{DrkCoinBlind, DrkSerial, DrkTokenId, DrkValue, DrkValueBlind, DrkValueCommit},
        util::{mod_r_p, pedersen_commitment_scalar, pedersen_commitment_u64},
    },
    zk::circuit::lead_contract::LeadContract,
    util::serial::{SerialDecodable, SerialEncodable},
    Result,
};


use rand::{thread_rng, Rng};


#[allow(clippy::too_many_arguments)]
pub fn create_lead_proof(
    pk: ProvingKey,
    coin : LeadCoin,
) -> Result<Proof> {
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
        path_sk: coin.path_sk,
        coin_timestamp: coin.tau, //
        coin_nonce: coin.nonce,
        coin_opening_1: Some(mod_r_p(coin.opening1.unwrap())),
        value: coin.value,
        coin_opening_2: Some(mod_r_p(coin.opening2.unwrap())),
        cm_pos: Some(coin.idx),
        //sn_c1: Some(coin.sn.unwrap()),
        slot: Some(coin.sl.unwrap()),
        mau_rho: Some(mau_rho.clone()),
        mau_y: Some(mau_y.clone()),
        root_cm: Some(coin.root_cm.unwrap()),
    };

    let start = Instant::now();
    let public_inputs = coin.public_inputs();
    let proof = Proof::create(&pk, &[contract], &public_inputs, &mut OsRng)?;
    debug!("Prove lead: [{:?}]", start.elapsed());
    Ok((proof))
}

pub fn verify_lead_proof(
    vk: &VerifyingKey,
    proof: &Proof,
    coin: LeadCoin
) -> Result<()> {
    let start = Instant::now();
    let public_inputs = coin.public_inputs();
    proof.verify(vk, &public_inputs)?;
    debug!("Verify lead: [{:?}]", start.elapsed());
    Ok(())
}
