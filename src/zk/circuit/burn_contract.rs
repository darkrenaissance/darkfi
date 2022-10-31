/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use darkfi_sdk::crypto::{
    constants::{
        sinsemilla::{OrchardCommitDomains, OrchardHashDomains},
        util::gen_const_array,
        NullifierK, OrchardFixedBases, OrchardFixedBasesFull, ValueCommitV, MERKLE_DEPTH_ORCHARD,
    },
    MerkleNode,
};
use halo2_gadgets::{
    ecc::{
        chip::{EccChip, EccConfig},
        FixedPoint, FixedPointBaseField, FixedPointShort, ScalarFixed, ScalarFixedShort,
    },
    poseidon::{
        primitives as poseidon, Hash as PoseidonHash, Pow5Chip as PoseidonChip,
        Pow5Config as PoseidonConfig,
    },
    sinsemilla::{
        chip::{SinsemillaChip, SinsemillaConfig},
        merkle::{
            chip::{MerkleChip, MerkleConfig},
            MerklePath,
        },
    },
    utilities::lookup_range_check::LookupRangeCheckConfig,
};
use halo2_proofs::{
    circuit::{floor_planner, AssignedCell, Layouter, Value},
    plonk::{Advice, Circuit, Column, ConstraintSystem, Error, Instance as InstanceColumn},
};
use pasta_curves::{pallas, Fp};

use crate::zk::assign_free_advice;

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct BurnConfig {
    primary: Column<InstanceColumn>,
    advices: [Column<Advice>; 10],
    ecc_config: EccConfig<OrchardFixedBases>,
    merkle_config_1: MerkleConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
    merkle_config_2: MerkleConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
    sinsemilla_config_1:
        SinsemillaConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
    sinsemilla_config_2:
        SinsemillaConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
    poseidon_config: PoseidonConfig<pallas::Base, 3, 2>,
}

impl BurnConfig {
    fn ecc_chip(&self) -> EccChip<OrchardFixedBases> {
        EccChip::construct(self.ecc_config.clone())
    }

    /*
    fn sinsemilla_chip_1(
        &self,
    ) -> SinsemillaChip<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases> {
        SinsemillaChip::construct(self.sinsemilla_config_1.clone())
    }

    fn sinsemilla_chip_2(
        &self,
    ) -> SinsemillaChip<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases> {
        SinsemillaChip::construct(self.sinsemilla_config_2.clone())
    }
    */

    fn merkle_chip_1(
        &self,
    ) -> MerkleChip<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases> {
        MerkleChip::construct(self.merkle_config_1.clone())
    }

    fn merkle_chip_2(
        &self,
    ) -> MerkleChip<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases> {
        MerkleChip::construct(self.merkle_config_2.clone())
    }

    fn poseidon_chip(&self) -> PoseidonChip<pallas::Base, 3, 2> {
        PoseidonChip::construct(self.poseidon_config.clone())
    }
}

// The public input array offsets
const BURN_NULLIFIER_OFFSET: usize = 0;
const BURN_VALCOMX_OFFSET: usize = 1;
const BURN_VALCOMY_OFFSET: usize = 2;
const BURN_TOKCOMX_OFFSET: usize = 3;
const BURN_TOKCOMY_OFFSET: usize = 4;
const BURN_MERKLEROOT_OFFSET: usize = 5;
const BURN_USERDATA_OFFSET: usize = 6;
const BURN_SIGKEYX_OFFSET: usize = 7;
const BURN_SIGKEYY_OFFSET: usize = 8;

#[derive(Default, Debug, Clone)]
pub struct BurnContract {
    pub secret_key: Value<pallas::Base>,
    pub serial: Value<pallas::Base>,
    pub value: Value<pallas::Base>,
    pub token: Value<pallas::Base>,
    pub coin_blind: Value<pallas::Base>,
    pub value_blind: Value<pallas::Scalar>,
    pub token_blind: Value<pallas::Scalar>,
    pub leaf_pos: Value<u32>,
    pub merkle_path: Value<[MerkleNode; MERKLE_DEPTH_ORCHARD]>,
    /// Allows composing this ZK proof to invoke other contracts
    pub spend_hook: Value<pallas::Base>,
    /// Data passed from this coin to the invoked contract
    pub user_data: Value<pallas::Base>,
    /// Blinding factor for the encrypted user_data
    pub user_data_blind: Value<pallas::Base>,
    pub sig_secret: Value<pallas::Base>,
}

impl Circuit<pallas::Base> for BurnContract {
    type Config = BurnConfig;
    type FloorPlanner = floor_planner::V1;

    fn without_witnesses(&self) -> Self {
        Self::default()
    }

    fn configure(meta: &mut ConstraintSystem<pallas::Base>) -> Self::Config {
        // Advice columns used in the circuit
        let advices = [
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
        ];

        // Fixed columns for the Sinsemilla generator lookup table
        let table_idx = meta.lookup_table_column();
        let lookup = (table_idx, meta.lookup_table_column(), meta.lookup_table_column());

        // Instance column used for public inputs
        let primary = meta.instance_column();
        meta.enable_equality(primary);

        // Permutation over all advice columns
        for advice in advices.iter() {
            meta.enable_equality(*advice);
        }

        // Poseidon requires four advice columns, while ECC incomplete addition
        // requires six. We can reduce the proof size by sharing fixed columns
        // between the ECC and Poseidon chips.
        // TODO: For multiple invocations they could/should be configured in
        // parallel rather than sharing perhaps?
        let lagrange_coeffs = [
            meta.fixed_column(),
            meta.fixed_column(),
            meta.fixed_column(),
            meta.fixed_column(),
            meta.fixed_column(),
            meta.fixed_column(),
            meta.fixed_column(),
            meta.fixed_column(),
        ];
        let rc_a = lagrange_coeffs[2..5].try_into().unwrap();
        let rc_b = lagrange_coeffs[5..8].try_into().unwrap();

        // Also use the first Lagrange coefficient column for loading global constants.
        meta.enable_constant(lagrange_coeffs[0]);

        // Use one of the right-most advice columns for all of our range checks.
        let range_check = LookupRangeCheckConfig::configure(meta, advices[9], table_idx);

        // Configuration for curve point operations.
        // This uses 10 advice columns and spans the whole circuit.
        let ecc_config =
            EccChip::<OrchardFixedBases>::configure(meta, advices, lagrange_coeffs, range_check);

        // Configuration for the Poseidon hash
        let poseidon_config = PoseidonChip::configure::<poseidon::P128Pow5T3>(
            meta,
            advices[6..9].try_into().unwrap(),
            advices[5],
            rc_a,
            rc_b,
        );

        // Configuration for a Sinsemilla hash instantiation and a
        // Merkle hash instantiation using this Sinsemilla instance.
        // Since the Sinsemilla config uses only 5 advice columns,
        // we can fit two instances side-by-side.
        let (sinsemilla_config_1, merkle_config_1) = {
            let sinsemilla_config_1 = SinsemillaChip::configure(
                meta,
                advices[..5].try_into().unwrap(),
                advices[6],
                lagrange_coeffs[0],
                lookup,
                range_check,
            );
            let merkle_config_1 = MerkleChip::configure(meta, sinsemilla_config_1.clone());
            (sinsemilla_config_1, merkle_config_1)
        };

        // Configuration for a Sinsemilla hash instantiation and a
        // Merkle hash instantiation using this Sinsemilla instance.
        // Since the Sinsemilla config uses only 5 advice columns,
        // we can fit two instances side-by-side.
        let (sinsemilla_config_2, merkle_config_2) = {
            let sinsemilla_config_2 = SinsemillaChip::configure(
                meta,
                advices[5..].try_into().unwrap(),
                advices[7],
                lagrange_coeffs[1],
                lookup,
                range_check,
            );
            let merkle_config_2 = MerkleChip::configure(meta, sinsemilla_config_2.clone());

            (sinsemilla_config_2, merkle_config_2)
        };

        BurnConfig {
            primary,
            advices,
            ecc_config,
            merkle_config_1,
            merkle_config_2,
            sinsemilla_config_1,
            sinsemilla_config_2,
            poseidon_config,
        }
    }

    fn synthesize(
        &self,
        config: Self::Config,
        mut layouter: impl Layouter<pallas::Base>,
    ) -> Result<(), Error> {
        // Load the Sinsemilla generator lookup table used by the whole circuit.
        SinsemillaChip::load(config.sinsemilla_config_1.clone(), &mut layouter)?;

        // Construct the ECC chip.
        let ecc_chip = config.ecc_chip();

        // =========
        // Nullifier
        // =========
        let secret_key = assign_free_advice(
            layouter.namespace(|| "load sinsemilla(secret key)"),
            config.advices[0],
            self.secret_key,
        )?;

        let serial = assign_free_advice(
            layouter.namespace(|| "load serial"),
            config.advices[0],
            self.serial,
        )?;

        let spend_hook = assign_free_advice(
            layouter.namespace(|| "load spend_hook"),
            config.advices[0],
            self.spend_hook,
        )?;

        let user_data = assign_free_advice(
            layouter.namespace(|| "load user_data"),
            config.advices[0],
            self.user_data,
        )?;

        let user_data_blind = assign_free_advice(
            layouter.namespace(|| "load user_data_blind"),
            config.advices[0],
            self.user_data_blind,
        )?;

        let hash = {
            let poseidon_message = [secret_key.clone(), serial.clone()];

            let poseidon_hasher = PoseidonHash::<
                _,
                _,
                poseidon::P128Pow5T3,
                poseidon::ConstantLength<2>,
                3,
                2,
            >::init(
                config.poseidon_chip(), layouter.namespace(|| "Poseidon init")
            )?;

            let poseidon_output =
                poseidon_hasher.hash(layouter.namespace(|| "Poseidon hash"), poseidon_message)?;

            let poseidon_output: AssignedCell<Fp, Fp> = poseidon_output;
            poseidon_output
        };

        layouter.constrain_instance(hash.cell(), config.primary, BURN_NULLIFIER_OFFSET)?;

        // let nullifier_k = FixedPointBaseField::from_inner(ecc_chip.clone(), NullifierK);
        //     nullifier_k.mul(
        //         layouter.namespace(|| "[poseidon_output + psi_old] NullifierK"),
        //         scalar,
        //     )?

        let value =
            assign_free_advice(layouter.namespace(|| "load value"), config.advices[0], self.value)?;

        let token =
            assign_free_advice(layouter.namespace(|| "load token"), config.advices[0], self.token)?;

        let coin_blind = assign_free_advice(
            layouter.namespace(|| "load coin_blind"),
            config.advices[0],
            self.coin_blind,
        )?;

        let public_key = {
            let nullifier_k = NullifierK;
            let nullifier_k = FixedPointBaseField::from_inner(ecc_chip.clone(), nullifier_k);
            nullifier_k.mul(layouter.namespace(|| "[x_s] Nullifier"), secret_key)?
        };

        let (pub_x, pub_y) = (public_key.inner().x(), public_key.inner().y());

        // =========
        // Coin hash
        // =========
        let coin = {
            let poseidon_message =
                [pub_x, pub_y, value, token, serial, spend_hook, user_data.clone(), coin_blind];

            let poseidon_hasher = PoseidonHash::<
                _,
                _,
                poseidon::P128Pow5T3,
                poseidon::ConstantLength<8>,
                3,
                2,
            >::init(
                config.poseidon_chip(), layouter.namespace(|| "Poseidon init")
            )?;

            let poseidon_output =
                poseidon_hasher.hash(layouter.namespace(|| "Poseidon hash"), poseidon_message)?;

            let poseidon_output: AssignedCell<Fp, Fp> = poseidon_output;
            poseidon_output
        };

        // ===========
        // Merkle root
        // ===========

        let path: Value<[pallas::Base; MERKLE_DEPTH_ORCHARD]> =
            self.merkle_path.map(|typed_path| gen_const_array(|i| typed_path[i].inner()));

        let merkle_inputs = MerklePath::construct(
            [config.merkle_chip_1(), config.merkle_chip_2()],
            OrchardHashDomains::MerkleCrh,
            self.leaf_pos,
            path,
        );

        let computed_final_root =
            merkle_inputs.calculate_root(layouter.namespace(|| "calculate root"), coin)?;

        layouter.constrain_instance(
            computed_final_root.cell(),
            config.primary,
            BURN_MERKLEROOT_OFFSET,
        )?;

        // ===========
        // Export user_data
        // ===========
        let user_data_enc = {
            let poseidon_message = [user_data, user_data_blind];

            let poseidon_hasher = PoseidonHash::<
                _,
                _,
                poseidon::P128Pow5T3,
                poseidon::ConstantLength<2>,
                3,
                2,
            >::init(
                config.poseidon_chip(), layouter.namespace(|| "Poseidon init")
            )?;

            let poseidon_output =
                poseidon_hasher.hash(layouter.namespace(|| "Poseidon hash"), poseidon_message)?;

            let poseidon_output: AssignedCell<Fp, Fp> = poseidon_output;
            poseidon_output
        };

        // Constrain the coin C
        layouter.constrain_instance(user_data_enc.cell(), config.primary, BURN_USERDATA_OFFSET)?;

        // ================
        // Value commitment
        // ================

        // This constant one is used for multiplication
        let one = assign_free_advice(
            layouter.namespace(|| "load constant one"),
            config.advices[0],
            Value::known(pallas::Base::one()),
        )?;

        let value =
            assign_free_advice(layouter.namespace(|| "load value"), config.advices[0], self.value)?;

        // v * G_1
        let (commitment, _) = {
            let value_commit_v = FixedPointShort::from_inner(ecc_chip.clone(), ValueCommitV);
            let value = ScalarFixedShort::new(
                ecc_chip.clone(),
                layouter.namespace(|| "value"),
                (value, one),
            )?;
            value_commit_v.mul(layouter.namespace(|| "[value] ValueCommitV"), value)?
        };

        // r_V * G_2
        let (blind, _rcv) = {
            let rcv = ScalarFixed::new(
                ecc_chip.clone(),
                layouter.namespace(|| "value_blind"),
                self.value_blind,
            )?;
            let value_commit_r =
                FixedPoint::from_inner(ecc_chip.clone(), OrchardFixedBasesFull::ValueCommitR);
            value_commit_r.mul(layouter.namespace(|| "[value_blind] ValueCommitR"), rcv)?
        };

        // Constrain the value commitment coordinates
        let value_commit = commitment.add(layouter.namespace(|| "valuecommit"), &blind)?;
        layouter.constrain_instance(
            value_commit.inner().x().cell(),
            config.primary,
            BURN_VALCOMX_OFFSET,
        )?;
        layouter.constrain_instance(
            value_commit.inner().y().cell(),
            config.primary,
            BURN_VALCOMY_OFFSET,
        )?;

        // ================
        // Token commitment
        // ================

        let token =
            assign_free_advice(layouter.namespace(|| "load token"), config.advices[0], self.token)?;

        // a * G_1
        let commitment = {
            let token_commit_v = FixedPointBaseField::from_inner(ecc_chip.clone(), NullifierK);
            token_commit_v.mul(layouter.namespace(|| "[token] NullifierK"), token)?
        };

        // r_A * G_2
        let (blind, _rca) = {
            let rca = ScalarFixed::new(
                ecc_chip.clone(),
                layouter.namespace(|| "token_blind"),
                self.token_blind,
            )?;
            let token_commit_r =
                FixedPoint::from_inner(ecc_chip.clone(), OrchardFixedBasesFull::ValueCommitR);
            token_commit_r.mul(layouter.namespace(|| "[token_blind] ValueCommitR"), rca)?
        };

        // Constrain the token commitment coordinates
        let token_commit = commitment.add(layouter.namespace(|| "tokencommit"), &blind)?;

        layouter.constrain_instance(
            token_commit.inner().x().cell(),
            config.primary,
            BURN_TOKCOMX_OFFSET,
        )?;

        layouter.constrain_instance(
            token_commit.inner().y().cell(),
            config.primary,
            BURN_TOKCOMY_OFFSET,
        )?;

        // ========================
        // Signature key derivation
        // ========================
        let sig_secret = assign_free_advice(
            layouter.namespace(|| "load sig_secret"),
            config.advices[0],
            self.sig_secret,
        )?;

        let sig_pub = {
            let nullifier_k = NullifierK;
            let nullifier_k = FixedPointBaseField::from_inner(ecc_chip, nullifier_k);
            nullifier_k.mul(layouter.namespace(|| "[x_s] Nullifier"), sig_secret)?
        };

        layouter.constrain_instance(
            sig_pub.inner().x().cell(),
            config.primary,
            BURN_SIGKEYX_OFFSET,
        )?;
        layouter.constrain_instance(
            sig_pub.inner().y().cell(),
            config.primary,
            BURN_SIGKEYY_OFFSET,
        )?;

        // At this point we've enforced all of our public inputs.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        crypto::{
            keypair::{PublicKey, SecretKey},
            proof::{ProvingKey, VerifyingKey},
            util::{pedersen_commitment_base, pedersen_commitment_u64},
            Proof,
        },
        Result,
    };
    use halo2_gadgets::poseidon::{
        primitives as poseidon,
        primitives::{ConstantLength, P128Pow5T3},
    };
    use halo2_proofs::dev::{CircuitLayout, MockProver};
    use incrementalmerkletree::{bridgetree::BridgeTree, Tree};
    use pasta_curves::{
        arithmetic::CurveAffine,
        group::{ff::Field, Curve},
    };
    use rand::rngs::OsRng;
    use std::time::Instant;

    #[test]
    fn burn_circuit_assert() -> Result<()> {
        let value = 42;
        let token_id = pallas::Base::random(&mut OsRng);
        let value_blind = pallas::Scalar::random(&mut OsRng);
        let token_blind = pallas::Scalar::random(&mut OsRng);
        let serial = pallas::Base::random(&mut OsRng);
        let coin_blind = pallas::Base::random(&mut OsRng);
        let secret = SecretKey::random(&mut OsRng);
        let spend_hook = pallas::Base::random(&mut OsRng);
        let user_data = pallas::Base::random(&mut OsRng);
        let user_data_blind = pallas::Base::random(&mut OsRng);
        let sig_secret = SecretKey::random(&mut OsRng);

        let coin2 = {
            let coords = PublicKey::from_secret(secret).0.to_affine().coordinates().unwrap();

            let msg = [
                *coords.x(),
                *coords.y(),
                pallas::Base::from(value),
                token_id,
                serial,
                spend_hook,
                user_data,
                coin_blind,
            ];
            poseidon::Hash::<_, P128Pow5T3, ConstantLength<8>, 3, 2>::init().hash(msg)
        };

        let mut tree = BridgeTree::<MerkleNode, 32>::new(100);
        let coin0 = pallas::Base::random(&mut OsRng);
        let coin1 = pallas::Base::random(&mut OsRng);
        let coin3 = pallas::Base::random(&mut OsRng);

        tree.append(&MerkleNode::from(coin0));
        tree.witness();
        tree.append(&MerkleNode::from(coin1));
        tree.append(&MerkleNode::from(coin2));
        let leaf_pos = tree.witness().unwrap();
        tree.append(&MerkleNode::from(coin3));
        tree.witness();

        let merkle_root = tree.root(0).unwrap();
        let merkle_path = tree.authentication_path(leaf_pos, &merkle_root).unwrap();
        let leaf_pos: u64 = leaf_pos.into();

        let nullifier = [secret.inner(), serial];
        let nullifier =
            poseidon::Hash::<_, P128Pow5T3, ConstantLength<2>, 3, 2>::init().hash(nullifier);

        let value_commit = pedersen_commitment_u64(value, value_blind);
        let value_coords = value_commit.to_affine().coordinates().unwrap();

        let token_commit = pedersen_commitment_base(token_id, token_blind);
        let token_coords = token_commit.to_affine().coordinates().unwrap();

        let user_data_enc = [user_data, user_data_blind];
        let user_data_enc =
            poseidon::Hash::<_, P128Pow5T3, ConstantLength<2>, 3, 2>::init().hash(user_data_enc);

        let sig_pubkey = PublicKey::from_secret(sig_secret);
        let sig_coords = sig_pubkey.0.to_affine().coordinates().unwrap();

        let public_inputs = vec![
            nullifier,
            *value_coords.x(),
            *value_coords.y(),
            *token_coords.x(),
            *token_coords.y(),
            merkle_root.inner(),
            user_data_enc,
            *sig_coords.x(),
            *sig_coords.y(),
        ];

        let circuit = BurnContract {
            secret_key: Value::known(secret.inner()),
            serial: Value::known(serial),
            value: Value::known(pallas::Base::from(value)),
            token: Value::known(token_id),
            coin_blind: Value::known(coin_blind),
            value_blind: Value::known(value_blind),
            token_blind: Value::known(token_blind),
            leaf_pos: Value::known(leaf_pos.try_into().unwrap()),
            merkle_path: Value::known(merkle_path.try_into().unwrap()),
            spend_hook: Value::known(spend_hook),
            user_data: Value::known(user_data),
            user_data_blind: Value::known(user_data_blind),
            sig_secret: Value::known(sig_secret.inner()),
        };

        use plotters::prelude::*;
        let root =
            BitMapBackend::new("target/burn_circuit_layout.png", (3840, 2160)).into_drawing_area();
        root.fill(&WHITE).unwrap();
        let root = root.titled("Burn Circuit Layout", ("sans-serif", 60)).unwrap();
        CircuitLayout::default().render(11, &circuit, &root).unwrap();

        let prover = MockProver::run(11, &circuit, vec![public_inputs.clone()])?;
        prover.assert_satisfied();

        let now = Instant::now();
        let proving_key = ProvingKey::build(11, &circuit);
        println!("ProvingKey built [{:?}]", now.elapsed());
        let now = Instant::now();
        let proof = Proof::create(&proving_key, &[circuit], &public_inputs, &mut OsRng)?;
        println!("Proof created [{:?}]", now.elapsed());

        let circuit = BurnContract::default();
        let now = Instant::now();
        let verifying_key = VerifyingKey::build(11, &circuit);
        println!("VerifyingKey built [{:?}]", now.elapsed());
        let now = Instant::now();
        proof.verify(&verifying_key, &public_inputs)?;
        println!("Proof verified [{:?}]", now.elapsed());

        println!("Proof size [{} kB]", proof.as_ref().len() as f64 / 1024.0);

        Ok(())
    }
}
