use std::time::Instant;

use bellman::{gadgets::multipack, groth16, Circuit};
use blake2s_simd::Params as Blake2sParams;
use bls12_381::Bls12;
use ff::PrimeField;
use group::{Curve, GroupEncoding};
use log::debug;
use rand::rngs::OsRng;

use crate::circuit::{mint_contract::MintContract, spend_contract::SpendContract};
use crate::crypto::merkle_node::{merkle_hash, MerkleNode, SAPLING_COMMITMENT_TREE_DEPTH};
use crate::crypto::nullifier::Nullifier;
use crate::crypto_new::{
    types::*,
    util::{pedersen_commitment_scalar, pedersen_commitment_u64},
};
use crate::{Error, Result};

pub struct Proof(groth16::Proof<Bls12>);

impl Proof {
    pub fn create(
        pk: &groth16::Parameters<Bls12>,
        circuits: impl Circuit<bls12_381::Scalar>,
        _pubinputs: &[bls12_381::Scalar],
    ) -> Result<Self> {
        let start = Instant::now();
        let proof = groth16::create_random_proof(circuits, pk, &mut OsRng).unwrap();
        debug!("Prove: [{:?}]", start.elapsed());
        Ok(Proof(proof))
    }

    pub fn verify(
        &self,
        vk: &groth16::PreparedVerifyingKey<Bls12>,
        pubinputs: &[bls12_381::Scalar],
    ) -> Result<()> {
        let start = Instant::now();
        let result = groth16::verify_proof(vk, &self.0, pubinputs);
        debug!("Verify: [{:?}]", start);
        if result.is_ok() {
            Ok(())
        } else {
            Err(Error::VerifyFailed)
        }
    }
}

pub const MINT_COIN0_OFFSET: usize = 0;
pub const MINT_COIN1_OFFSET: usize = 1;
pub const MINT_VALCOMX_OFFSET: usize = 2;
pub const MINT_VALCOMY_OFFSET: usize = 3;
pub const MINT_TOKCOMX_OFFSET: usize = 4;
pub const MINT_TOKCOMY_OFFSET: usize = 5;
pub const MINT_PUBINPUTS_LEN: usize = 6;

pub fn setup_mint_prover() -> groth16::Parameters<Bls12> {
    debug!("Mint: Making random params...");
    let start = Instant::now();
    let params = {
        let c = MintContract::default();
        groth16::generate_random_parameters::<Bls12, _, _>(c, &mut OsRng).unwrap()
    };
    debug!("Setup: [{:?}]", start.elapsed());
    params
}

pub fn create_mint_proof(
    params: &groth16::Parameters<Bls12>,
    value: u64,
    token_id: DrkTokenId,
    randomness_value: DrkValueBlind,
    randomness_token: DrkTokenBlind,
    serial: DrkSerial,
    randomness_coin: DrkCoinBlind,
    public: DrkPublicKey,
) -> Result<(Proof, Vec<DrkPublicInput>)> {
    let mut public_inputs = vec![DrkPublicInput::zero(); MINT_PUBINPUTS_LEN];

    let mut coin = [0; 32];
    coin.copy_from_slice(
        Blake2sParams::new()
            .hash_length(32)
            .personal(zcash_primitives::constants::CRH_IVK_PERSONALIZATION)
            .to_state()
            .update(&public.to_bytes())
            .update(&value.to_le_bytes())
            .update(&token_id.to_bytes())
            .update(&serial.to_bytes())
            .update(&randomness_coin.to_bytes())
            .finalize()
            .as_bytes(),
    );

    let hash = multipack::bytes_to_bits_le(&coin);
    let hash = multipack::compute_multipacking(&hash);
    assert_eq!(hash.len(), 2);
    public_inputs[MINT_COIN0_OFFSET] = hash[0];
    public_inputs[MINT_COIN1_OFFSET] = hash[1];

    let value_commit = pedersen_commitment_u64(value, randomness_value);
    let affine = jubjub::ExtendedPoint::from(value_commit).to_affine();
    public_inputs[MINT_VALCOMX_OFFSET] = affine.get_u();
    public_inputs[MINT_VALCOMY_OFFSET] = affine.get_v();

    let token_commit = pedersen_commitment_scalar(token_id, randomness_token);
    let affine = jubjub::ExtendedPoint::from(token_commit).to_affine();
    public_inputs[MINT_TOKCOMX_OFFSET] = affine.get_u();
    public_inputs[MINT_TOKCOMY_OFFSET] = affine.get_v();

    let c = MintContract {
        value: Some(value),
        token_id: Some(token_id),
        randomness_value: Some(randomness_value),
        randomness_token: Some(randomness_token),
        serial: Some(serial),
        randomness_coin: Some(randomness_coin),
        public: Some(public),
    };

    let proof = Proof::create(params, c, &public_inputs)?;
    Ok((proof, public_inputs))
}

pub const SPEND_VALCOMX_OFFSET: usize = 0;
pub const SPEND_VALCOMY_OFFSET: usize = 1;
pub const SPEND_TOKCOMX_OFFSET: usize = 2;
pub const SPEND_TOKCOMY_OFFSET: usize = 3;
pub const SPEND_NULLIFIER0_OFFSET: usize = 4;
pub const SPEND_NULLIFIER1_OFFSET: usize = 5;
pub const SPEND_MERKLEROOT_OFFSET: usize = 6;
pub const SPEND_SIGPUBX_OFFSET: usize = 7;
pub const SPEND_SIGPUBY_OFFSET: usize = 8;
pub const SPEND_PUBINPUTS_LEN: usize = 9;

pub fn setup_spend_prover() -> groth16::Parameters<Bls12> {
    debug!("Spend: Making random params...");
    let start = Instant::now();
    let params = {
        let c = SpendContract::default();
        groth16::generate_random_parameters::<Bls12, _, _>(c, &mut OsRng).unwrap()
    };
    debug!("Setup: [{:?}]", start.elapsed());
    params
}

pub fn create_spend_proof(
    params: &groth16::Parameters<Bls12>,
    value: u64,
    token_id: DrkTokenId,
    randomness_value: DrkValueBlind,
    randomness_token: DrkTokenBlind,
    serial: DrkSerial,
    randomness_coin: DrkCoinBlind,
    secret: DrkSecretKey,
    merkle_path: [(bls12_381::Scalar, bool); SAPLING_COMMITMENT_TREE_DEPTH],
    signature_secret: DrkSecretKey,
) -> Result<(Proof, Vec<DrkPublicInput>)> {
    let mut public_inputs = vec![DrkPublicInput::zero(); SPEND_PUBINPUTS_LEN];

    let value_commit = pedersen_commitment_u64(value, randomness_value);
    let affine = jubjub::ExtendedPoint::from(value_commit).to_affine();
    public_inputs[SPEND_VALCOMX_OFFSET] = affine.get_u();
    public_inputs[SPEND_VALCOMY_OFFSET] = affine.get_v();

    let token_commit = pedersen_commitment_scalar(token_id, randomness_token);
    let affine = jubjub::ExtendedPoint::from(token_commit).to_affine();
    public_inputs[SPEND_TOKCOMX_OFFSET] = affine.get_u();
    public_inputs[SPEND_TOKCOMY_OFFSET] = affine.get_v();

    let mut nullifier = [0; 32];
    nullifier.copy_from_slice(
        Blake2sParams::new()
            .hash_length(32)
            .personal(zcash_primitives::constants::PRF_NF_PERSONALIZATION)
            .to_state()
            .update(&secret.to_bytes())
            .update(&serial.to_bytes())
            .finalize()
            .as_bytes(),
    );
    let nullifier = Nullifier::new(nullifier);
    let hash = multipack::bytes_to_bits_le(&nullifier.repr);
    let hash = multipack::compute_multipacking(&hash);
    assert_eq!(hash.len(), 2);
    public_inputs[SPEND_NULLIFIER0_OFFSET] = hash[0];
    public_inputs[SPEND_NULLIFIER1_OFFSET] = hash[1];

    let sig_pub = zcash_primitives::constants::SPENDING_KEY_GENERATOR * signature_secret;
    let affine = jubjub::ExtendedPoint::from(sig_pub).to_affine();
    public_inputs[SPEND_SIGPUBX_OFFSET] = affine.get_u();
    public_inputs[SPEND_TOKCOMY_OFFSET] = affine.get_v();

    let public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;

    let mut coin = [0; 32];
    coin.copy_from_slice(
        Blake2sParams::new()
            .hash_length(32)
            .personal(zcash_primitives::constants::CRH_IVK_PERSONALIZATION)
            .to_state()
            .update(&public.to_bytes())
            .update(&value.to_le_bytes())
            .update(&token_id.to_bytes())
            .update(&serial.to_bytes())
            .update(&randomness_coin.to_bytes())
            .finalize()
            .as_bytes(),
    );

    use zcash_primitives::pedersen_hash::pedersen_hash;
    use zcash_primitives::pedersen_hash::Personalization::NoteCommitment;
    let merkle_root = jubjub::ExtendedPoint::from(pedersen_hash(
        NoteCommitment,
        multipack::bytes_to_bits_le(&coin),
    ));
    let affine = merkle_root.to_affine();
    let mut merkle_root = affine.get_u();

    let mut branch: [_; SAPLING_COMMITMENT_TREE_DEPTH] = Default::default();
    let mut is_right: [_; SAPLING_COMMITMENT_TREE_DEPTH] = Default::default();

    for (i, (branch_i, is_right_i)) in merkle_path.iter().enumerate() {
        branch[i] = Some(*branch_i);
        is_right[i] = Some(*is_right_i);
        if *is_right_i {
            merkle_root = merkle_hash(i, &branch_i.to_repr(), &merkle_root.to_repr());
        } else {
            merkle_root = merkle_hash(i, &merkle_root.to_repr(), &branch_i.to_repr());
        }
    }

    let merkle_root = MerkleNode::new(merkle_root.to_repr());
    public_inputs[SPEND_MERKLEROOT_OFFSET] = merkle_root.into();

    let c = SpendContract {
        value: Some(value),
        token_id: Some(token_id),
        randomness_value: Some(randomness_value),
        randomness_token: Some(randomness_token),
        serial: Some(serial),
        randomness_coin: Some(randomness_coin),
        secret: Some(secret),
        branch,
        is_right,
        signature_secret: Some(signature_secret),
    };

    let proof = Proof::create(params, c, &public_inputs)?;
    Ok((proof, public_inputs))
}
