#![allow(unused_imports)]
#![allow(unused_mut)]
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
    pub asset_id: Option<u64>,
    pub randomness_value: Option<jubjub::Fr>,
    pub serial: Option<jubjub::Fr>,
    pub randomness_coin: Option<jubjub::Fr>,
    pub public: Option<jubjub::SubgroupPoint>,
}
impl Circuit<bls12_381::Scalar> for MintContract {
    fn synthesize<CS: ConstraintSystem<bls12_381::Scalar>>(
        self,
        cs: &mut CS,
    ) -> Result<(), SynthesisError> {
        // Line 18: u64_as_binary_le value param:value
        let value = boolean::u64_into_boolean_vec_le(
            cs.namespace(|| "Line 18: u64_as_binary_le value param:value"),
            self.value,
        )?;

        // Line 19: u64_as_binary_le asset_id param:asset_id
        let asset_id = boolean::u64_into_boolean_vec_le(
            cs.namespace(|| "Line 19: u64_as_binary_le value param:asset_id"),
            self.asset_id,
        )?;

        // Line 19: fr_as_binary_le randomness_value param:randomness_value
        let randomness_value = boolean::field_into_boolean_vec_le(
            cs.namespace(|| "Line 19: fr_as_binary_le randomness_value param:randomness_value"),
            self.randomness_value,
        )?;

        // Line 20: fr_as_binary_le serial param:serial
        let serial = boolean::field_into_boolean_vec_le(
            cs.namespace(|| "Line 20: fr_as_binary_le serial param:serial"),
            self.serial,
        )?;

        // Line 21: fr_as_binary_le randomness_coin param:randomness_coin
        let randomness_coin = boolean::field_into_boolean_vec_le(
            cs.namespace(|| "Line 21: fr_as_binary_le randomness_coin param:randomness_coin"),
            self.randomness_coin,
        )?;

        // Line 23: witness public param:public
        let public = ecc::EdwardsPoint::witness(
            cs.namespace(|| "Line 23: witness public param:public"),
            self.public.map(jubjub::ExtendedPoint::from),
        )?;

        // Line 24: assert_not_small_order public
        public.assert_not_small_order(cs.namespace(|| "Line 24: assert_not_small_order public"))?;

        // Line 29: ec_mul_const vcv value G_VCV
        let vcv = ecc::fixed_base_multiplication(
            cs.namespace(|| "Line 29: ec_mul_const vcv value G_VCV"),
            &zcash_proofs::constants::VALUE_COMMITMENT_VALUE_GENERATOR,
            &value,
        )?;

        // Line 30: ec_mul_const rcv randomness_value G_VCR
        let rcv = ecc::fixed_base_multiplication(
            cs.namespace(|| "Line 30: ec_mul_const rcv randomness_value G_VCR"),
            &zcash_proofs::constants::VALUE_COMMITMENT_RANDOMNESS_GENERATOR,
            &randomness_value,
        )?;

        // Line 31: ec_add cv vcv rcv
        let cv = vcv.add(cs.namespace(|| "Line 31: ec_add cv vcv rcv"), &rcv)?;

        // Line 33: emit_ec cv
        cv.inputize(cs.namespace(|| "Line 33: emit_ec cv"))?;

        // Line 39: alloc_binary preimage
        let mut preimage = vec![];

        // Line 42: ec_repr repr_public public
        let repr_public = public.repr(cs.namespace(|| "Line 42: ec_repr repr_public public"))?;

        // Line 43: binary_extend preimage repr_public
        preimage.extend(repr_public);

        // Line 46: binary_extend preimage value
        preimage.extend(value);

        // Line 47: binary_extend preimage asset_id
        preimage.extend(asset_id);

        // Line 53: binary_extend preimage serial
        preimage.extend(serial);

        for _ in 0..4 {
            // Line 55: alloc_const_bit zero_bit false
            let zero_bit = Boolean::constant(false);

            // Line 56: binary_push preimage zero_bit
            preimage.push(zero_bit);
        }

        // Line 69: binary_extend preimage randomness_coin
        preimage.extend(randomness_coin);

        for _ in 0..4 {
            // Line 71: alloc_const_bit zero_bit false
            let zero_bit = Boolean::constant(false);

            // Line 72: binary_push preimage zero_bit
            preimage.push(zero_bit);
        }

        // Line 89: static_assert_binary_size preimage 896
        assert_eq!(preimage.len(), 896);

        // Line 90: blake2s coin preimage CRH_IVK
        let mut coin = blake2s::blake2s(
            cs.namespace(|| "Line 90: blake2s coin preimage CRH_IVK"),
            &preimage,
            zcash_primitives::constants::CRH_IVK_PERSONALIZATION,
        )?;

        // Line 91: emit_binary coin
        multipack::pack_into_inputs(cs.namespace(|| "Line 91: emit_binary coin"), &coin)?;

        Ok(())
    }
}
