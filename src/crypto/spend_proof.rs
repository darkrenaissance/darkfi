use std::{io, time::Instant};

use halo2_gadgets::{
    primitives,
    primitives::poseidon::{ConstantLength, P128Pow5T3},
};
use incrementalmerkletree::Hashable;
use log::debug;
use pasta_curves::{
    arithmetic::{CurveAffine, FieldExt},
    group::Curve,
    pallas,
};

use super::{
    nullifier::Nullifier,
    proof::{Proof, ProvingKey, VerifyingKey},
    util::{mod_r_p, pedersen_commitment_scalar, pedersen_commitment_u64},
};
use crate::{
    circuit::spend_contract::SpendContract,
    crypto::{
        keypair::{PublicKey, SecretKey},
        merkle_node::MerkleNode,
    },
    serial::{Decodable, Encodable},
    types::*,
    Result,
};

pub struct SpendRevealedValues {
    pub value_commit: DrkValueCommit,
    pub token_commit: DrkValueCommit,
    pub nullifier: Nullifier,
    pub merkle_root: MerkleNode,
    pub signature_public: PublicKey,
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
        secret: SecretKey,
        leaf_position: incrementalmerkletree::Position,
        merkle_path: Vec<MerkleNode>,
        signature_secret: SecretKey,
    ) -> Self {
        let nullifier = [secret.0, serial];
        let nullifier =
            primitives::poseidon::Hash::init(P128Pow5T3, ConstantLength::<2>).hash(nullifier);

        let public_key = PublicKey::from_secret(secret);
        let coords = public_key.0.to_affine().coordinates().unwrap();

        let messages =
            [*coords.x(), *coords.y(), DrkValue::from_u64(value), token_id, serial, coin_blind];

        let coin = primitives::poseidon::Hash::init(P128Pow5T3, ConstantLength::<6>).hash(messages);

        let merkle_root = {
            let position: u64 = leaf_position.into();
            let mut current = MerkleNode(coin);
            for (level, sibling) in merkle_path.iter().enumerate() {
                let level = level as u8;
                current = if position & (1 << level) == 0 {
                    MerkleNode::combine(level.into(), &current, sibling)
                } else {
                    MerkleNode::combine(level.into(), sibling, &current)
                };
            }
            current
        };

        let value_commit = pedersen_commitment_u64(value, value_blind);
        let token_commit = pedersen_commitment_scalar(mod_r_p(token_id), token_blind);

        SpendRevealedValues {
            value_commit,
            token_commit,
            nullifier: Nullifier(nullifier),
            merkle_root,
            signature_public: PublicKey::from_secret(signature_secret),
        }
    }

    fn make_outputs(&self) -> [DrkCircuitField; 8] {
        let value_coords = self.value_commit.to_affine().coordinates().unwrap();
        let token_coords = self.token_commit.to_affine().coordinates().unwrap();
        let merkle_root = self.merkle_root.0;
        let sig_coords = self.signature_public.0.to_affine().coordinates().unwrap();

        vec![
            self.nullifier.inner(),
            *value_coords.x(),
            *value_coords.y(),
            *token_coords.x(),
            *token_coords.y(),
            merkle_root,
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
        len += self.merkle_root.encode(&mut s)?;
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
            merkle_root: Decodable::decode(&mut d)?,
            signature_public: Decodable::decode(d)?,
        })
    }
}

#[allow(clippy::too_many_arguments)]
pub fn create_spend_proof(
    pk: &ProvingKey,
    value: u64,
    token_id: DrkTokenId,
    value_blind: DrkValueBlind,
    token_blind: DrkValueBlind,
    serial: DrkSerial,
    coin_blind: DrkCoinBlind,
    secret: SecretKey,
    leaf_position: incrementalmerkletree::Position,
    merkle_path: Vec<MerkleNode>,
    signature_secret: SecretKey,
) -> Result<(Proof, SpendRevealedValues)> {
    let revealed = SpendRevealedValues::compute(
        value,
        token_id,
        value_blind,
        token_blind,
        serial,
        coin_blind,
        secret,
        leaf_position,
        merkle_path.clone(),
        signature_secret.clone(),
    );

    let merkle_path: Vec<pallas::Base> = merkle_path.iter().map(|node| node.0).collect();
    let leaf_position: u64 = leaf_position.into();

    let c = SpendContract {
        secret_key: Some(secret.0),
        serial: Some(serial),
        value: Some(DrkValue::from_u64(value)),
        asset: Some(token_id),
        coin_blind: Some(coin_blind),
        value_blind: Some(value_blind),
        asset_blind: Some(token_blind),
        leaf_pos: Some(leaf_position as u32),
        merkle_path: Some(merkle_path.try_into().unwrap()),
        sig_secret: Some(signature_secret.0),
    };

    let start = Instant::now();
    let public_inputs = revealed.make_outputs();
    let proof = Proof::create(&pk, &[c], &public_inputs)?;
    debug!("Prove: [{:?}]", start.elapsed());

    Ok((proof, revealed))
}

pub fn verify_spend_proof(
    vk: &VerifyingKey,
    proof: Proof,
    revealed: &SpendRevealedValues,
) -> Result<()> {
    let public_inputs = revealed.make_outputs();
    Ok(proof.verify(vk, &public_inputs)?)
}
