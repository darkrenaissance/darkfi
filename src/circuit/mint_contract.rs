#![allow(unused_imports)]
#![allow(unused_mut)]
use crate::crypto::merkle_node::SAPLING_COMMITMENT_TREE_DEPTH;
use bellman::{
    gadgets::{
        blake2s, boolean,
        boolean::{AllocatedBit, Boolean},
        multipack, num, Assignment,
    },
    groth16, Circuit, ConstraintSystem, SynthesisError,
};
use bls12_381::Bls12;
use ff::{Field, PrimeField};
use group::Curve;
use zcash_proofs::circuit::{ecc, pedersen_hash};

pub struct MintContract {
    pub value: Option<u64>,
    pub token_id: Option<jubjub::Fr>,
    pub randomness_value: Option<jubjub::Fr>,
    pub randomness_asset: Option<jubjub::Fr>,
    pub serial: Option<jubjub::Fr>,
    pub randomness_coin: Option<jubjub::Fr>,
    pub public: Option<jubjub::SubgroupPoint>,
}
impl Circuit<bls12_381::Scalar> for MintContract {
    fn synthesize<CS: ConstraintSystem<bls12_381::Scalar>>(
        self,
        cs: &mut CS,
    ) -> Result<(), SynthesisError> {
        // Line 20: u64_as_binary_le value param:value
        let value = boolean::u64_into_boolean_vec_le(
            cs.namespace(|| "Line 20: u64_as_binary_le value param:value"),
            self.value,
        )?;

        // Line 21: fr_as_binary_le token_id param:token_id
        let token_id = boolean::field_into_boolean_vec_le(
            cs.namespace(|| "Line 21: fr_as_binary_le token_id param:token_id"),
            self.token_id,
        )?;

        // Line 22: fr_as_binary_le randomness_value param:randomness_value
        let randomness_value = boolean::field_into_boolean_vec_le(
            cs.namespace(|| "Line 22: fr_as_binary_le randomness_value param:randomness_value"),
            self.randomness_value,
        )?;

        // Line 23: fr_as_binary_le randomness_asset param:randomness_asset
        let randomness_asset = boolean::field_into_boolean_vec_le(
            cs.namespace(|| "Line 23: fr_as_binary_le randomness_asset param:randomness_asset"),
            self.randomness_asset,
        )?;

        // Line 24: fr_as_binary_le serial param:serial
        let serial = boolean::field_into_boolean_vec_le(
            cs.namespace(|| "Line 24: fr_as_binary_le serial param:serial"),
            self.serial,
        )?;

        // Line 25: fr_as_binary_le randomness_coin param:randomness_coin
        let randomness_coin = boolean::field_into_boolean_vec_le(
            cs.namespace(|| "Line 25: fr_as_binary_le randomness_coin param:randomness_coin"),
            self.randomness_coin,
        )?;

        // Line 27: witness public param:public
        let public = ecc::EdwardsPoint::witness(
            cs.namespace(|| "Line 27: witness public param:public"),
            self.public.map(jubjub::ExtendedPoint::from),
        )?;

        // Line 28: assert_not_small_order public
        public.assert_not_small_order(cs.namespace(|| "Line 28: assert_not_small_order public"))?;

        // Line 33: ec_mul_const vcv value G_VCV
        let vcv = ecc::fixed_base_multiplication(
            cs.namespace(|| "Line 33: ec_mul_const vcv value G_VCV"),
            &zcash_proofs::constants::VALUE_COMMITMENT_VALUE_GENERATOR,
            &value,
        )?;

        // Line 34: ec_mul_const rcv randomness_value G_VCR
        let rcv = ecc::fixed_base_multiplication(
            cs.namespace(|| "Line 34: ec_mul_const rcv randomness_value G_VCR"),
            &zcash_proofs::constants::VALUE_COMMITMENT_RANDOMNESS_GENERATOR,
            &randomness_value,
        )?;

        // Line 35: ec_add cv vcv rcv
        let cv = vcv.add(cs.namespace(|| "Line 35: ec_add cv vcv rcv"), &rcv)?;

        // Line 37: emit_ec cv
        cv.inputize(cs.namespace(|| "Line 37: emit_ec cv"))?;

        // Line 42: ec_mul_const vca token_id G_VCV
        let vca = ecc::fixed_base_multiplication(
            cs.namespace(|| "Line 42: ec_mul_const vca token_id G_VCV"),
            &zcash_proofs::constants::VALUE_COMMITMENT_VALUE_GENERATOR,
            &token_id,
        )?;

        // Line 43: ec_mul_const rca randomness_asset G_VCR
        let rca = ecc::fixed_base_multiplication(
            cs.namespace(|| "Line 43: ec_mul_const rca randomness_asset G_VCR"),
            &zcash_proofs::constants::VALUE_COMMITMENT_RANDOMNESS_GENERATOR,
            &randomness_asset,
        )?;

        // Line 44: ec_add ca vca rca
        let ca = vca.add(cs.namespace(|| "Line 44: ec_add ca vca rca"), &rca)?;

        // Line 46: emit_ec ca
        ca.inputize(cs.namespace(|| "Line 46: emit_ec ca"))?;

        // Line 53: alloc_binary preimage
        let mut preimage = vec![];

        // Line 56: ec_repr repr_public public
        let repr_public = public.repr(cs.namespace(|| "Line 56: ec_repr repr_public public"))?;

        // Line 57: binary_extend preimage repr_public
        preimage.extend(repr_public);

        // Line 60: binary_extend preimage value
        preimage.extend(value);

        // Line 99: binary_extend preimage token_id
        preimage.extend(token_id);

        // add 4 zero bits
        for _ in 0..4 {
            // Line 71: alloc_const_bit zero_bit false
            let zero_bit = Boolean::constant(false);

            // Line 72: binary_push preimage zero_bit
            preimage.push(zero_bit);
        }

        // Line 67: binary_extend preimage serial
        preimage.extend(serial);

        // add 4 zero bits
        for _ in 0..4 {
            // Line 71: alloc_const_bit zero_bit false
            let zero_bit = Boolean::constant(false);

            // Line 72: binary_push preimage zero_bit
            preimage.push(zero_bit);
        }

        // Line 83: binary_extend preimage randomness_coin
        preimage.extend(randomness_coin);

        // add 4 zero bits
        for _ in 0..4 {
            // Line 71: alloc_const_bit zero_bit false
            let zero_bit = Boolean::constant(false);

            // Line 72: binary_push preimage zero_bit
            preimage.push(zero_bit);
        }

        // Line 120: static_assert_binary_size preimage 1088
        assert_eq!(preimage.len(), 1088);

        // Line 121: blake2s coin preimage CRH_IVK
        let mut coin = blake2s::blake2s(
            cs.namespace(|| "Line 121: blake2s coin preimage CRH_IVK"),
            &preimage,
            zcash_primitives::constants::CRH_IVK_PERSONALIZATION,
        )?;

        // Line 122: emit_binary coin
        multipack::pack_into_inputs(cs.namespace(|| "Line 122: emit_binary coin"), &coin)?;

        Ok(())
    }
}
