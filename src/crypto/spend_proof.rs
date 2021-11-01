use std::io;
use std::time::Instant;

use halo2_gadgets::{
    primitives,
    primitives::poseidon::{ConstantLength, P128Pow5T3},
};
use log::debug;
use pasta_curves::{
    arithmetic::{CurveAffine, FieldExt},
    group::Curve,
};

use super::{
    proof::{Proof, ProvingKey, VerifyingKey},
    util::{mod_r_p, pedersen_commitment_scalar, pedersen_commitment_u64},
};
use crate::{
    circuit::spend_contract::SpendContract,
    serial::{Decodable, Encodable},
    types::*,
    Result,
};

pub struct SpendRevealedValues {
    pub value_commit: DrkValueCommit,
    pub token_commit: DrkValueCommit,
    pub nullifier: DrkNullifier,
    //pub merkle_root: MerkleNode,
    pub signature_public: DrkPublicKey,
}

impl SpendRevealedValues {
    #[allow(clippy::too_many_arguments)]
    fn compute(
        value: u64,
        token_id: DrkTokenId,
        value_blind: DrkValueBlind,
        token_blind: DrkValueBlind,
        serial: DrkSerial,
        coin_blind: DrkCoinBlind,
        secret: DrkSecretKey,
        merkle_path: Vec<DrkCoin>,
        signature_secret: DrkSecretKey,
    ) -> Self {
        let nullifier = [secret, serial];
        let nullifier =
            primitives::poseidon::Hash::init(P128Pow5T3, ConstantLength::<2>).hash(nullifier);

        let public_key = derive_publickey(secret);
        let coords = public_key.to_affine().coordinates().unwrap();
        let messages = [
            [*coords.x(), *coords.y()],
            [DrkValue::from_u64(value), token_id],
            [serial, coin_blind],
        ];

        let mut coin = DrkCoin::zero();
        for msg in messages.iter() {
            coin += primitives::poseidon::Hash::init(P128Pow5T3, ConstantLength::<2>).hash(*msg);
        }

        // TODO: Merkle root

        let value_commit = pedersen_commitment_u64(value, value_blind);
        let token_commit = pedersen_commitment_scalar(mod_r_p(token_id), token_blind);

        let signature_public = derive_publickey(signature_secret);

        SpendRevealedValues {
            value_commit,
            token_commit,
            nullifier,
            signature_public,
        }
    }

    fn make_outputs(&self) -> [DrkCircuitField; 8] {
        let value_coords = self.value_commit.to_affine().coordinates().unwrap();
        let token_coords = self.token_commit.to_affine().coordinates().unwrap();
        let sig_coords = self.signature_public.to_affine().coordinates().unwrap();

        // TODO: merkle
        vec![
            self.nullifier,
            *value_coords.x(),
            *value_coords.y(),
            *token_coords.x(),
            *token_coords.y(),
            // merkleroot,
            *sig_coords.x(),
            *sig_coords.y(),
        ]
        .try_into()
        .unwrap()
    }
}

impl Encodable for SpendRevealedValues {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.value_commit.encode(&mut s)?;
        len += self.token_commit.encode(&mut s)?;
        len += self.nullifier.encode(&mut s)?;
        //len += self.merkle_root.encode(&mut s)?;
        len += self.signature_public.encode(s)?;
        Ok(len)
    }
}

impl Decodable for SpendRevealedValues {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        Ok(Self {
            value_commit: Decodable::decode(&mut d)?,
            token_commit: Decodable::decode(&mut d)?,
            nullifier: Decodable::decode(&mut d)?,
            //merkle_root: Decodable::decode(&mut d)?,
            signature_public: Decodable::decode(d)?,
        })
    }
}

#[allow(clippy::too_many_arguments)]
pub fn create_spend_proof(
    value: u64,
    token_id: DrkTokenId,
    value_blind: DrkValueBlind,
    token_blind: DrkValueBlind,
    serial: DrkSerial,
    coin_blind: DrkCoinBlind,
    secret: DrkSecretKey,
    merkle_path: Vec<DrkCoin>,
    signature_secret: DrkSecretKey,
) -> Result<(Proof, SpendRevealedValues)> {
    let revealed = SpendRevealedValues::compute(
        value,
        token_id,
        value_blind,
        token_blind,
        serial,
        coin_blind,
        secret,
        merkle_path.clone(),
        signature_secret,
    );

    let c = SpendContract {
        secret_key: Some(secret),
        serial: Some(serial),
        value: Some(DrkValue::from_u64(value)),
        asset: Some(token_id),
        coin_blind: Some(coin_blind),
        value_blind: Some(value_blind),
        asset_blind: Some(token_blind),
        leaf: Some(pasta_curves::Fp::one()), // TODO:
        leaf_pos: Some(0),                   // TODO:
        merkle_path: Some(merkle_path.try_into().unwrap()),
        sig_secret: Some(mod_r_p(signature_secret)),
    };

    let start = Instant::now();
    // TODO: Don't always build this
    let pk = ProvingKey::build(11, SpendContract::default());
    debug!("Setup: [{:?}]", start.elapsed());

    let start = Instant::now();
    let public_inputs = revealed.make_outputs();
    let proof = Proof::create(&pk, &[c], &public_inputs)?;
    debug!("Prove: [{:?}]", start.elapsed());

    Ok((proof, revealed))
}

pub fn verify_spend_proof(proof: Proof, revealed: &SpendRevealedValues) -> Result<()> {
    let public_inputs = revealed.make_outputs();

    // TODO: Don't always build this
    let vk = VerifyingKey::build(11, SpendContract::default());
    Ok(proof.verify(&vk, &public_inputs)?)
}
