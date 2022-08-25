use std::time::Instant;

use halo2_gadgets::poseidon::primitives as poseidon;
use halo2_proofs::circuit::Value;
use log::debug;
use pasta_curves::{arithmetic::CurveAffine, group::Curve};
use rand::rngs::OsRng;

use crate::{
    crypto::{
        coin::Coin,
        keypair::PublicKey,
        proof::{Proof, ProvingKey, VerifyingKey},
        types::{
            DrkCircuitField, DrkCoinBlind, DrkSerial, DrkSpendHook, DrkTokenId, DrkUserData,
            DrkValue, DrkValueBlind, DrkValueCommit,
        },
        util::{pedersen_commitment_base, pedersen_commitment_u64},
    },
    util::serial::{SerialDecodable, SerialEncodable},
    zk::circuit::mint_contract::MintContract,
    Result,
};

#[derive(Debug, Clone, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub struct MintRevealedValues {
    pub value_commit: DrkValueCommit,
    pub token_commit: DrkValueCommit,
    pub coin: Coin,
}

impl MintRevealedValues {
    #[allow(clippy::too_many_arguments)]
    pub fn compute(
        value: u64,
        token_id: DrkTokenId,
        value_blind: DrkValueBlind,
        token_blind: DrkValueBlind,
        serial: DrkSerial,
        spend_hook: DrkSpendHook,
        user_data: DrkUserData,
        coin_blind: DrkCoinBlind,
        public_key: PublicKey,
    ) -> Self {
        let value_commit = pedersen_commitment_u64(value, value_blind);
        let token_commit = pedersen_commitment_base(token_id, token_blind);

        let coords = public_key.0.to_affine().coordinates().unwrap();
        let messages = [
            *coords.x(),
            *coords.y(),
            DrkValue::from(value),
            token_id,
            serial,
            spend_hook,
            user_data,
            coin_blind,
        ];

        let coin =
            poseidon::Hash::<_, poseidon::P128Pow5T3, poseidon::ConstantLength<8>, 3, 2>::init()
                .hash(messages);

        MintRevealedValues { value_commit, token_commit, coin: Coin(coin) }
    }

    pub fn make_outputs(&self) -> Vec<DrkCircuitField> {
        let value_coords = self.value_commit.to_affine().coordinates().unwrap();
        let token_coords = self.token_commit.to_affine().coordinates().unwrap();

        vec![
            self.coin.0,
            *value_coords.x(),
            *value_coords.y(),
            *token_coords.x(),
            *token_coords.y(),
        ]
    }
}

#[allow(clippy::too_many_arguments)]
pub fn create_mint_proof(
    pk: &ProvingKey,
    value: u64,
    token_id: DrkTokenId,
    value_blind: DrkValueBlind,
    token_blind: DrkValueBlind,
    serial: DrkSerial,
    spend_hook: DrkSpendHook,
    user_data: DrkUserData,
    coin_blind: DrkCoinBlind,
    public_key: PublicKey,
) -> Result<(Proof, MintRevealedValues)> {
    let revealed = MintRevealedValues::compute(
        value,
        token_id,
        value_blind,
        token_blind,
        serial,
        spend_hook,
        user_data,
        coin_blind,
        public_key,
    );

    let coords = public_key.0.to_affine().coordinates().unwrap();

    let c = MintContract {
        pub_x: Value::known(*coords.x()),
        pub_y: Value::known(*coords.y()),
        value: Value::known(DrkValue::from(value)),
        token: Value::known(token_id),
        serial: Value::known(serial),
        coin_blind: Value::known(coin_blind),
        spend_hook: Value::known(spend_hook),
        user_data: Value::known(user_data),
        value_blind: Value::known(value_blind),
        token_blind: Value::known(token_blind),
    };

    let start = Instant::now();
    let public_inputs = revealed.make_outputs();
    let proof = Proof::create(pk, &[c], &public_inputs, &mut OsRng)?;
    debug!("Prove mint: [{:?}]", start.elapsed());

    Ok((proof, revealed))
}

pub fn verify_mint_proof(
    vk: &VerifyingKey,
    proof: &Proof,
    revealed: &MintRevealedValues,
) -> Result<()> {
    let start = Instant::now();
    let public_inputs = revealed.make_outputs();
    proof.verify(vk, &public_inputs)?;
    debug!("Verify mint: [{:?}]", start.elapsed());
    Ok(())
}
