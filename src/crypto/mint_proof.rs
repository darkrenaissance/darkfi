use std::io;
use std::time::Instant;

use halo2_gadgets::primitives;
use halo2_gadgets::primitives::poseidon::{ConstantLength, P128Pow5T3};
use log::debug;
use pasta_curves as pasta;
use pasta_curves::{
    arithmetic::{CurveAffine, FieldExt},
    group::Curve,
};

use super::{
    proof::{Proof, ProvingKey, VerifyingKey},
    types::*,
    util::{mod_r_p, pedersen_commitment_scalar, pedersen_commitment_u64},
};
use crate::circuit::mint_contract::MintContract;
use crate::error::Result;
use crate::serial::{Decodable, Encodable};

pub struct MintRevealedValues {
    pub value_commit: DrkValueCommit,
    pub token_commit: DrkValueCommit,
    pub coin: [u8; 32],
}

impl MintRevealedValues {
    fn compute(
        value: u64,
        token_id: DrkTokenId,
        value_blind: DrkValueBlind,
        token_blind: DrkValueBlind,
        serial: DrkSerial,
        coin_blind: DrkCoinBlind,
        public_key: DrkPublicKey,
    ) -> Self {
        let value_commit = pedersen_commitment_u64(value, value_blind);
        let token_commit = pedersen_commitment_scalar(mod_r_p(token_id), token_blind);

        let coords = public_key.to_affine().coordinates().unwrap();
        let messages = [
            [*coords.x(), *coords.y()],
            [pasta::Fp::from_u64(value), token_id],
            [serial, coin_blind],
        ];

        let mut hash = pasta::Fp::zero();
        for msg in messages.iter() {
            hash += primitives::poseidon::Hash::init(P128Pow5T3, ConstantLength::<2>).hash(*msg);
        }

        let coin = hash.to_bytes();

        MintRevealedValues {
            value_commit,
            token_commit,
            coin,
        }
    }

    fn make_outputs(&self) -> [pasta::Fp; 5] {
        let value_coords = self.value_commit.to_affine().coordinates().unwrap();
        let token_coords = self.value_commit.to_affine().coordinates().unwrap();

        vec![
            pasta::Fp::from_bytes(&self.coin).unwrap(),
            *value_coords.x(),
            *value_coords.y(),
            *token_coords.x(),
            *token_coords.y(),
        ]
        .try_into()
        .unwrap()
    }
}

impl Encodable for MintRevealedValues {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.value_commit.encode(&mut s)?;
        len += self.token_commit.encode(&mut s)?;
        len += self.coin.encode(&mut s)?;
        Ok(len)
    }
}

impl Decodable for MintRevealedValues {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        Ok(Self {
            value_commit: Decodable::decode(&mut d)?,
            token_commit: Decodable::decode(&mut d)?,
            coin: Decodable::decode(d)?,
        })
    }
}

#[allow(clippy::too_many_arguments)]
pub fn create_mint_proof(
    value: u64,
    token_id: pasta::Fp,
    value_blind: pasta::Fq,
    token_blind: pasta::Fq,
    serial: pasta::Fp,
    coin_blind: pasta::Fp,
    public_key: pasta::Ep,
) -> Result<(Proof, MintRevealedValues)> {
    let revealed = MintRevealedValues::compute(
        value,
        token_id,
        value_blind,
        token_blind,
        serial,
        coin_blind,
        public_key,
    );

    let coords = public_key.to_affine().coordinates().unwrap();

    let c = MintContract {
        pub_x: Some(*coords.x()),
        pub_y: Some(*coords.y()),
        value: Some(pasta::Fp::from_u64(value)),
        asset: Some(token_id),
        serial: Some(serial),
        coin_blind: Some(coin_blind),
        value_blind: Some(value_blind),
        asset_blind: Some(token_blind),
    };

    let start = Instant::now();
    let pk = ProvingKey::build(11, MintContract::default());
    debug!("Setup: [{:?}]", start.elapsed());

    let start = Instant::now();
    let public_inputs = revealed.make_outputs();
    let proof = Proof::create(&pk, &[c], &public_inputs)?;
    debug!("Prove: [{:?}]", start.elapsed());

    Ok((proof, revealed))
}

pub fn verify_mint_proof(proof: Proof, revealed: &MintRevealedValues) -> Result<()> {
    let public_inputs = revealed.make_outputs();

    let vk = VerifyingKey::build(11, MintContract::default());
    Ok(proof.verify(&vk, &public_inputs)?)
}
