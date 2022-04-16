use std::time::Instant;

use halo2_gadgets::primitives::{
    poseidon,
    poseidon::{ConstantLength, P128Pow5T3},
};
use incrementalmerkletree::Hashable;
use log::debug;
use pasta_curves::{arithmetic::CurveAffine, group::Curve};
use rand::rngs::OsRng;

use super::{
    nullifier::Nullifier,
    proof::{Proof, ProvingKey, VerifyingKey},
    util::{mod_r_p, pedersen_commitment_scalar, pedersen_commitment_u64},
};
use crate::{
    crypto::{
        keypair::{PublicKey, SecretKey},
        merkle_node::MerkleNode,
        types::*,
    },
    util::serial::{SerialDecodable, SerialEncodable},
    zk::circuit::burn_contract::BurnContract,
    Result,
};

#[derive(Debug, Clone, PartialEq, SerialEncodable, SerialDecodable)]
pub struct BurnRevealedValues {
    pub value_commit: DrkValueCommit,
    pub token_commit: DrkValueCommit,
    pub nullifier: Nullifier,
    pub merkle_root: MerkleNode,
    pub signature_public: PublicKey,
}

impl BurnRevealedValues {
    #[allow(clippy::too_many_arguments)]
    pub fn compute(
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
            poseidon::Hash::<_, P128Pow5T3, ConstantLength<2>, 3, 2>::init().hash(nullifier);

        let public_key = PublicKey::from_secret(secret);
        let coords = public_key.0.to_affine().coordinates().unwrap();

        let messages =
            [*coords.x(), *coords.y(), DrkValue::from(value), token_id, serial, coin_blind];

        let coin = poseidon::Hash::<_, P128Pow5T3, ConstantLength<6>, 3, 2>::init().hash(messages);

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

        BurnRevealedValues {
            value_commit,
            token_commit,
            nullifier: Nullifier(nullifier),
            merkle_root,
            signature_public: PublicKey::from_secret(signature_secret),
        }
    }

    pub fn make_outputs(&self) -> [DrkCircuitField; 8] {
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

#[allow(clippy::too_many_arguments)]
pub fn create_burn_proof(
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
) -> Result<(Proof, BurnRevealedValues)> {
    let revealed = BurnRevealedValues::compute(
        value,
        token_id,
        value_blind,
        token_blind,
        serial,
        coin_blind,
        secret,
        leaf_position,
        merkle_path.clone(),
        signature_secret,
    );

    let leaf_position: u64 = leaf_position.into();

    let c = BurnContract {
        secret_key: Some(secret.0),
        serial: Some(serial),
        value: Some(DrkValue::from(value)),
        token: Some(token_id),
        coin_blind: Some(coin_blind),
        value_blind: Some(value_blind),
        token_blind: Some(token_blind),
        leaf_pos: Some(leaf_position as u32),
        merkle_path: Some(merkle_path.try_into().unwrap()),
        sig_secret: Some(signature_secret.0),
    };

    let start = Instant::now();
    let public_inputs = revealed.make_outputs();
    let proof = Proof::create(pk, &[c], &public_inputs, &mut OsRng)?;
    debug!("Prove: [{:?}]", start.elapsed());

    Ok((proof, revealed))
}

pub fn verify_burn_proof(
    vk: &VerifyingKey,
    proof: &Proof,
    revealed: &BurnRevealedValues,
) -> Result<()> {
    let public_inputs = revealed.make_outputs();
    Ok(proof.verify(vk, &public_inputs)?)
}
