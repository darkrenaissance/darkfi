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

use crate::crypto::merkle_node::SAPLING_COMMITMENT_TREE_DEPTH;

pub struct SpendContract {
    pub value: Option<u64>,
    pub asset_id: Option<jubjub::Fr>,
    pub randomness_value: Option<jubjub::Fr>,
    pub randomness_asset: Option<jubjub::Fr>,
    pub serial: Option<jubjub::Fr>,
    pub randomness_coin: Option<jubjub::Fr>,
    pub secret: Option<jubjub::Fr>,
    pub branch: [Option<bls12_381::Scalar>; SAPLING_COMMITMENT_TREE_DEPTH],
    pub is_right: [Option<bool>; SAPLING_COMMITMENT_TREE_DEPTH],
    pub signature_secret: Option<jubjub::Fr>,
}
impl Circuit<bls12_381::Scalar> for SpendContract {
    fn synthesize<CS: ConstraintSystem<bls12_381::Scalar>>(
        self,
        cs: &mut CS,
    ) -> Result<(), SynthesisError> {
        // Line 40: u64_as_binary_le value param:value
        let value = boolean::u64_into_boolean_vec_le(
            cs.namespace(|| "Line 40: u64_as_binary_le value param:value"),
            self.value,
        )?;

        // Line 41: u64_as_binary_le asset_id param:asset_id
        let asset_id = boolean::field_into_boolean_vec_le(
            cs.namespace(|| "Line 41: u64_as_binary_le value param:value"),
            self.asset_id,
        )?;

        // Line 41: fr_as_binary_le randomness_value param:randomness_value
        let randomness_value = boolean::field_into_boolean_vec_le(
            cs.namespace(|| "Line 41: fr_as_binary_le randomness_value param:randomness_value"),
            self.randomness_value,
        )?;

        // Line 41: fr_as_binary_le randomness_asset param:randomness_asset
        let randomness_asset = boolean::field_into_boolean_vec_le(
            cs.namespace(|| "Line 41: fr_as_binary_le randomness_asset param:randomness_asset"),
            self.randomness_asset,
        )?;

        // Line 46: ec_mul_const vcv value G_VCV
        let vcv = ecc::fixed_base_multiplication(
            cs.namespace(|| "Line 46: ec_mul_const vcv value G_VCV"),
            &zcash_proofs::constants::VALUE_COMMITMENT_VALUE_GENERATOR,
            &value,
        )?;

        // Line 47: ec_mul_const rcv randomness_value G_VCR
        let rcv = ecc::fixed_base_multiplication(
            cs.namespace(|| "Line 47: ec_mul_const rcv randomness_value G_VCR"),
            &zcash_proofs::constants::VALUE_COMMITMENT_RANDOMNESS_GENERATOR,
            &randomness_value,
        )?;

        // Line 48: ec_add cv vcv rcv
        let cv = vcv.add(cs.namespace(|| "Line 48: ec_add cv vcv rcv"), &rcv)?;

        // Line 50: emit_ec cv
        cv.inputize(cs.namespace(|| "Line 50: emit_ec cv"))?;

        // Line 46: ec_mul_const vca asset_id G_VCV
        let vca = ecc::fixed_base_multiplication(
            cs.namespace(|| "Line 46: ec_mul_const vca asset_id G_VCV"),
            &zcash_proofs::constants::VALUE_COMMITMENT_VALUE_GENERATOR,
            &asset_id,
        )?;

        // Line 47: ec_mul_const rca randomness_asset G_VCR
        let rca = ecc::fixed_base_multiplication(
            cs.namespace(|| "Line 47: ec_mul_const rca randomness_asset G_VCR"),
            &zcash_proofs::constants::VALUE_COMMITMENT_RANDOMNESS_GENERATOR,
            &randomness_asset,
        )?;

        // Line 48: ec_add ca vca rca
        let ca = vca.add(cs.namespace(|| "Line 48: ec_add ca vca rca"), &rca)?;

        // Line 50: emit_ec ca
        ca.inputize(cs.namespace(|| "Line 50: emit_ec ca"))?;

        // Line 54: fr_as_binary_le serial param:serial
        let serial = boolean::field_into_boolean_vec_le(
            cs.namespace(|| "Line 54: fr_as_binary_le serial param:serial"),
            self.serial,
        )?;

        // Line 55: fr_as_binary_le secret param:secret
        let secret = boolean::field_into_boolean_vec_le(
            cs.namespace(|| "Line 55: fr_as_binary_le secret param:secret"),
            self.secret,
        )?;

        // Line 57: alloc_binary nf_preimage
        let mut nf_preimage = vec![];

        // Line 64: binary_clone secret2 secret
        let mut secret2: Vec<_> = secret.iter().cloned().collect();

        // Line 65: binary_extend nf_preimage secret2
        nf_preimage.extend(secret2);

        // Line 67: alloc_const_bit zero_bit false
        let zero_bit = Boolean::constant(false);

        // Line 68: binary_push nf_preimage zero_bit
        nf_preimage.push(zero_bit);

        // Line 70: alloc_const_bit zero_bit false
        let zero_bit = Boolean::constant(false);

        // Line 71: binary_push nf_preimage zero_bit
        nf_preimage.push(zero_bit);

        // Line 73: alloc_const_bit zero_bit false
        let zero_bit = Boolean::constant(false);

        // Line 74: binary_push nf_preimage zero_bit
        nf_preimage.push(zero_bit);

        // Line 76: alloc_const_bit zero_bit false
        let zero_bit = Boolean::constant(false);

        // Line 77: binary_push nf_preimage zero_bit
        nf_preimage.push(zero_bit);

        // Line 81: binary_clone serial2 serial
        let mut serial2: Vec<_> = serial.iter().cloned().collect();

        // Line 82: binary_extend nf_preimage serial2
        nf_preimage.extend(serial2);

        // Line 84: alloc_const_bit zero_bit false
        let zero_bit = Boolean::constant(false);

        // Line 85: binary_push nf_preimage zero_bit
        nf_preimage.push(zero_bit);

        // Line 87: alloc_const_bit zero_bit false
        let zero_bit = Boolean::constant(false);

        // Line 88: binary_push nf_preimage zero_bit
        nf_preimage.push(zero_bit);

        // Line 90: alloc_const_bit zero_bit false
        let zero_bit = Boolean::constant(false);

        // Line 91: binary_push nf_preimage zero_bit
        nf_preimage.push(zero_bit);

        // Line 93: alloc_const_bit zero_bit false
        let zero_bit = Boolean::constant(false);

        // Line 94: binary_push nf_preimage zero_bit
        nf_preimage.push(zero_bit);

        // Line 100: static_assert_binary_size nf_preimage 512
        assert_eq!(nf_preimage.len(), 512);

        // Line 101: blake2s nf nf_preimage PRF_NF
        let mut nf = blake2s::blake2s(
            cs.namespace(|| "Line 101: blake2s nf nf_preimage PRF_NF"),
            &nf_preimage,
            zcash_primitives::constants::PRF_NF_PERSONALIZATION,
        )?;

        // Line 102: emit_binary nf
        multipack::pack_into_inputs(cs.namespace(|| "Line 102: emit_binary nf"), &nf)?;

        // Line 106: ec_mul_const public secret G_SPEND
        let public = ecc::fixed_base_multiplication(
            cs.namespace(|| "Line 106: ec_mul_const public secret G_SPEND"),
            &zcash_proofs::constants::SPENDING_KEY_GENERATOR,
            &secret,
        )?;

        // Line 110: fr_as_binary_le randomness_coin param:randomness_coin
        let randomness_coin = boolean::field_into_boolean_vec_le(
            cs.namespace(|| "Line 110: fr_as_binary_le randomness_coin param:randomness_coin"),
            self.randomness_coin,
        )?;

        // Line 110: fr_as_binary_le asset_id param:asset_id
        let asset_id = boolean::field_into_boolean_vec_le(
            cs.namespace(|| "Line 109: fr_as_binary_le asset_id param:asset_id"),
            self.randomness_coin,
        )?;
        // Line 113: alloc_binary preimage
        let mut preimage = vec![];

        // Line 116: ec_repr repr_public public
        let repr_public = public.repr(cs.namespace(|| "Line 116: ec_repr repr_public public"))?;

        // Line 117: binary_extend preimage repr_public
        preimage.extend(repr_public);

        // Line 120: binary_extend preimage value
        preimage.extend(value);

        // Line 123: binary_extend preimage serial
        preimage.extend(serial);

        // Line 125: alloc_const_bit zero_bit false
        let zero_bit = Boolean::constant(false);

        // Line 126: binary_push preimage zero_bit
        preimage.push(zero_bit);

        // Line 128: alloc_const_bit zero_bit false
        let zero_bit = Boolean::constant(false);

        // Line 129: binary_push preimage zero_bit
        preimage.push(zero_bit);

        // Line 131: alloc_const_bit zero_bit false
        let zero_bit = Boolean::constant(false);

        // Line 132: binary_push preimage zero_bit
        preimage.push(zero_bit);

        // Line 134: alloc_const_bit zero_bit false
        let zero_bit = Boolean::constant(false);

        // Line 135: binary_push preimage zero_bit
        preimage.push(zero_bit);

        // Line 139: binary_extend preimage randomness_coin
        preimage.extend(randomness_coin);

        // Line 141: alloc_const_bit zero_bit false
        let zero_bit = Boolean::constant(false);

        // Line 142: binary_push preimage zero_bit
        preimage.push(zero_bit);

        // Line 144: alloc_const_bit zero_bit false
        let zero_bit = Boolean::constant(false);

        // Line 145: binary_push preimage zero_bit
        preimage.push(zero_bit);

        // Line 147: alloc_const_bit zero_bit false
        let zero_bit = Boolean::constant(false);

        // Line 148: binary_push preimage zero_bit
        preimage.push(zero_bit);

        // Line 150: alloc_const_bit zero_bit false
        let zero_bit = Boolean::constant(false);

        // Line 151: binary_push preimage zero_bit
        preimage.push(zero_bit);

        // Line 109: binary_extend preimage asset_id
        preimage.extend(asset_id);

        // Line 109: alloc_const_bit zero_bit false
        let zero_bit = Boolean::constant(false);

        // Line 109: binary_push preimage zero_bit
        preimage.push(zero_bit);

        // Line 109: alloc_const_bit zero_bit false
        let zero_bit = Boolean::constant(false);

        // Line 109: binary_push preimage zero_bit
        preimage.push(zero_bit);

        // Line 109: alloc_const_bit zero_bit false
        let zero_bit = Boolean::constant(false);

        // Line 109: binary_push preimage zero_bit
        preimage.push(zero_bit);

        // Line 109: alloc_const_bit zero_bit false
        let zero_bit = Boolean::constant(false);

        // Line 109: binary_push preimage zero_bit
        preimage.push(zero_bit);

        // Line 159: static_assert_binary_size preimage 1088
        assert_eq!(preimage.len(), 1088);

        // Line 160: blake2s coin preimage CRH_IVK
        let mut coin = blake2s::blake2s(
            cs.namespace(|| "Line 160: blake2s coin preimage CRH_IVK"),
            &preimage,
            zcash_primitives::constants::CRH_IVK_PERSONALIZATION,
        )?;

        // Line 166: pedersen_hash cm coin NOTE_COMMIT
        let mut cm = pedersen_hash::pedersen_hash(
            cs.namespace(|| "Line 166: pedersen_hash cm coin NOTE_COMMIT"),
            pedersen_hash::Personalization::NoteCommitment,
            &coin,
        )?;

        // Line 168: ec_get_u current cm
        let mut current = cm.get_u().clone();

        for i in 0..SAPLING_COMMITMENT_TREE_DEPTH {
            // Line 174: alloc_scalar branch param:branch_0
            let branch = num::AllocatedNum::alloc(
                cs.namespace(|| "Line 174: alloc_scalar branch param:branch_0"),
                || Ok(*self.branch[i].get()?),
            )?;

            // Line 177: alloc_bit is_right param:is_right_0
            let is_right = boolean::Boolean::from(boolean::AllocatedBit::alloc(
                cs.namespace(|| "Line 177: alloc_bit is_right param:is_right_0"),
                self.is_right[i],
            )?);

            // Line 180: conditionally_reverse left right current branch is_right
            let (left, right) = num::AllocatedNum::conditionally_reverse(
                cs.namespace(|| {
                    "Line 180: conditionally_reverse left right current branch is_right"
                }),
                &current,
                &branch,
                &is_right,
            )?;

            // Line 183: scalar_as_binary left left
            let left = left.to_bits_le(cs.namespace(|| "Line 183: scalar_as_binary left left"))?;

            // Line 184: scalar_as_binary right right
            let right =
                right.to_bits_le(cs.namespace(|| "Line 184: scalar_as_binary right right"))?;

            // Line 185: alloc_binary preimage
            let mut preimage = vec![];

            // Line 186: binary_extend preimage left
            preimage.extend(left);

            // Line 187: binary_extend preimage right
            preimage.extend(right);

            // Line 188: pedersen_hash cm preimage MERKLE_0
            let mut cm = pedersen_hash::pedersen_hash(
                cs.namespace(|| "Line 188: pedersen_hash cm preimage MERKLE_0"),
                pedersen_hash::Personalization::MerkleTree(i),
                &preimage,
            )?;

            // Line 190: ec_get_u current cm
            current = cm.get_u().clone();
        }

        /*
        // Line 174: alloc_scalar branch param:branch_0
        let branch = num::AllocatedNum::alloc(
            cs.namespace(|| "Line 174: alloc_scalar branch param:branch_0"),
            || Ok(*self.branch_0.get()?),
        )?;

        // Line 177: alloc_bit is_right param:is_right_0
        let is_right = boolean::Boolean::from(boolean::AllocatedBit::alloc(
            cs.namespace(|| "Line 177: alloc_bit is_right param:is_right_0"),
            self.is_right_0,
        )?);

        // Line 180: conditionally_reverse left right current branch is_right
        let (left, right) = num::AllocatedNum::conditionally_reverse(
            cs.namespace(|| "Line 180: conditionally_reverse left right current branch is_right"),
            &current,
            &branch,
            &is_right,
        )?;

        // Line 183: scalar_as_binary left left
        let left = left.to_bits_le(cs.namespace(|| "Line 183: scalar_as_binary left left"))?;

        // Line 184: scalar_as_binary right right
        let right = right.to_bits_le(cs.namespace(|| "Line 184: scalar_as_binary right right"))?;

        // Line 185: alloc_binary preimage
        let mut preimage = vec![];

        // Line 186: binary_extend preimage left
        preimage.extend(left);

        // Line 187: binary_extend preimage right
        preimage.extend(right);

        // Line 188: pedersen_hash cm preimage MERKLE_0
        let mut cm = pedersen_hash::pedersen_hash(
            cs.namespace(|| "Line 188: pedersen_hash cm preimage MERKLE_0"),
            pedersen_hash::Personalization::MerkleTree(0),
            &preimage,
        )?;

        // Line 190: ec_get_u current cm
        let mut current = cm.get_u().clone();

        // Line 194: alloc_scalar branch param:branch_1
        let branch = num::AllocatedNum::alloc(
            cs.namespace(|| "Line 194: alloc_scalar branch param:branch_1"),
            || Ok(*self.branch_1.get()?),
        )?;

        // Line 197: alloc_bit is_right param:is_right_1
        let is_right = boolean::Boolean::from(boolean::AllocatedBit::alloc(
            cs.namespace(|| "Line 197: alloc_bit is_right param:is_right_1"),
            self.is_right_1,
        )?);

        // Line 200: conditionally_reverse left right current branch is_right
        let (left, right) = num::AllocatedNum::conditionally_reverse(
            cs.namespace(|| "Line 200: conditionally_reverse left right current branch is_right"),
            &current,
            &branch,
            &is_right,
        )?;

        // Line 203: scalar_as_binary left left
        let left = left.to_bits_le(cs.namespace(|| "Line 203: scalar_as_binary left left"))?;

        // Line 204: scalar_as_binary right right
        let right = right.to_bits_le(cs.namespace(|| "Line 204: scalar_as_binary right right"))?;

        // Line 205: alloc_binary preimage
        let mut preimage = vec![];

        // Line 206: binary_extend preimage left
        preimage.extend(left);

        // Line 207: binary_extend preimage right
        preimage.extend(right);

        // Line 208: pedersen_hash cm preimage MERKLE_1
        let mut cm = pedersen_hash::pedersen_hash(
            cs.namespace(|| "Line 208: pedersen_hash cm preimage MERKLE_1"),
            pedersen_hash::Personalization::MerkleTree(1),
            &preimage,
        )?;

        // Line 210: ec_get_u current cm
        let mut current = cm.get_u().clone();

        // Line 214: alloc_scalar branch param:branch_2
        let branch = num::AllocatedNum::alloc(
            cs.namespace(|| "Line 214: alloc_scalar branch param:branch_2"),
            || Ok(*self.branch_2.get()?),
        )?;

        // Line 217: alloc_bit is_right param:is_right_2
        let is_right = boolean::Boolean::from(boolean::AllocatedBit::alloc(
            cs.namespace(|| "Line 217: alloc_bit is_right param:is_right_2"),
            self.is_right_2,
        )?);

        // Line 220: conditionally_reverse left right current branch is_right
        let (left, right) = num::AllocatedNum::conditionally_reverse(
            cs.namespace(|| "Line 220: conditionally_reverse left right current branch is_right"),
            &current,
            &branch,
            &is_right,
        )?;

        // Line 223: scalar_as_binary left left
        let left = left.to_bits_le(cs.namespace(|| "Line 223: scalar_as_binary left left"))?;

        // Line 224: scalar_as_binary right right
        let right = right.to_bits_le(cs.namespace(|| "Line 224: scalar_as_binary right right"))?;

        // Line 225: alloc_binary preimage
        let mut preimage = vec![];

        // Line 226: binary_extend preimage left
        preimage.extend(left);

        // Line 227: binary_extend preimage right
        preimage.extend(right);

        // Line 228: pedersen_hash cm preimage MERKLE_2
        let mut cm = pedersen_hash::pedersen_hash(
            cs.namespace(|| "Line 228: pedersen_hash cm preimage MERKLE_2"),
            pedersen_hash::Personalization::MerkleTree(2),
            &preimage,
        )?;

        // Line 230: ec_get_u current cm
        let mut current = cm.get_u().clone();

        // Line 234: alloc_scalar branch param:branch_3
        let branch = num::AllocatedNum::alloc(
            cs.namespace(|| "Line 234: alloc_scalar branch param:branch_3"),
            || Ok(*self.branch_3.get()?),
        )?;

        // Line 237: alloc_bit is_right param:is_right_3
        let is_right = boolean::Boolean::from(boolean::AllocatedBit::alloc(
            cs.namespace(|| "Line 237: alloc_bit is_right param:is_right_3"),
            self.is_right_3,
        )?);

        // Line 240: conditionally_reverse left right current branch is_right
        let (left, right) = num::AllocatedNum::conditionally_reverse(
            cs.namespace(|| "Line 240: conditionally_reverse left right current branch is_right"),
            &current,
            &branch,
            &is_right,
        )?;

        // Line 243: scalar_as_binary left left
        let left = left.to_bits_le(cs.namespace(|| "Line 243: scalar_as_binary left left"))?;

        // Line 244: scalar_as_binary right right
        let right = right.to_bits_le(cs.namespace(|| "Line 244: scalar_as_binary right right"))?;

        // Line 245: alloc_binary preimage
        let mut preimage = vec![];

        // Line 246: binary_extend preimage left
        preimage.extend(left);

        // Line 247: binary_extend preimage right
        preimage.extend(right);

        // Line 248: pedersen_hash cm preimage MERKLE_3
        let mut cm = pedersen_hash::pedersen_hash(
            cs.namespace(|| "Line 248: pedersen_hash cm preimage MERKLE_3"),
            pedersen_hash::Personalization::MerkleTree(3),
            &preimage,
        )?;

        // Line 250: ec_get_u current cm
        let mut current = cm.get_u().clone();
        */

        // Line 253: emit_scalar current
        current.inputize(cs.namespace(|| "Line 253: emit_scalar current"))?;

        let signature_secret = boolean::field_into_boolean_vec_le(
            cs.namespace(|| "Signature secret"),
            self.signature_secret,
        )?;
        let signature_public = ecc::fixed_base_multiplication(
            cs.namespace(|| "Signature public"),
            &zcash_proofs::constants::SPENDING_KEY_GENERATOR,
            &signature_secret,
        )?;
        signature_public.inputize(cs.namespace(|| "Signature public inputize"))?;

        Ok(())
    }
}
