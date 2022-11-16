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
        NullifierK, OrchardFixedBases,
        OrchardFixedBasesFull::ValueCommitR,
        MERKLE_DEPTH_ORCHARD,
    },
    MerkleNode,
};
use halo2_gadgets::{
    ecc::{
        chip::{EccChip, EccConfig},
        FixedPoint, FixedPointBaseField, NonIdentityPoint, ScalarFixed,
    },
    poseidon::{
        primitives::{ConstantLength, P128Pow5T3},
        Hash as PoseidonHash, Pow5Chip as PoseidonChip, Pow5Config as PoseidonConfig,
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
    pasta::{group::Curve, pallas},
    plonk,
    plonk::{Advice, Circuit, Column, ConstraintSystem, Instance as InstanceColumn},
};

use crate::zk::{
    assign_free_advice,
    gadget::{
        arithmetic::{ArithChip, ArithConfig, ArithInstruction},
        less_than::{LessThanChip, LessThanConfig},
        native_range_check::NativeRangeCheckChip,
    },
};
use log::info;

/// Public input offset for the lead coin C2 nonce
const LEADCOIN_C2_NONCE_OFFSET: usize = 0;
/// Public input offset for lead coin public key X coordinate
const LEADCOIN_PK_X_OFFSET: usize = 1;
/// Public input offset for lead coin public key Y coordinate
const LEADCOIN_PK_Y_OFFSET: usize = 2;
/// Public input offset for the lottery target lhs
const LEADCOIN_Y_BASE_OFFSET: usize = 3;
/// Derivation prefix for the nullifier PRF
const PRF_NULLIFIER_PREFIX: u64 = 0;

/// Circuit configuration for the crypsinous leader proof
#[derive(Clone, Debug)]
pub struct LeadConfig {
    primary: Column<InstanceColumn>,
    advices: [Column<Advice>; 14],
    ecc_config: EccConfig<OrchardFixedBases>,
    poseidon_config: PoseidonConfig<pallas::Base, 3, 2>,
    sinsemilla_config_1:
        SinsemillaConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
    _sinsemilla_config_2:
        SinsemillaConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
    merkle_config_1: MerkleConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
    merkle_config_2: MerkleConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
    lessthan_config: LessThanConfig<3, 253, 85>,
    arith_config: ArithConfig,
}

impl LeadConfig {
    fn ecc_chip(&self) -> EccChip<OrchardFixedBases> {
        EccChip::construct(self.ecc_config.clone())
    }

    fn poseidon_chip(&self) -> PoseidonChip<pallas::Base, 3, 2> {
        PoseidonChip::construct(self.poseidon_config.clone())
    }

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

    fn lessthan_chip(&self) -> LessThanChip<3, 253, 85> {
        LessThanChip::construct(self.lessthan_config.clone())
    }

    fn arith_chip(&self) -> ArithChip {
        ArithChip::construct(self.arith_config.clone())
    }
}

/// Circuit implementation for the crypsinous leader proof
#[derive(Default, Clone, Debug)]
pub struct LeadContract {
    /// Merkle path to the commitment for `coin_1`
    pub coin1_commit_merkle_path: Value<[MerkleNode; MERKLE_DEPTH_ORCHARD]>,
    /// Merkle root to the commitment of `coin_1` in the Merkle tree of commitments
    pub coin1_commit_root: Value<pallas::Base>,
    /// `coin_1` leaf position in the Merkle tree of coin commitments
    pub coin1_commit_leaf_pos: Value<u32>,
    /// `coin_1` secret key.
    pub coin1_sk: Value<pallas::Base>,
    /// Merkle root of the `coin_1` secret key in the Merkle tree of secret keys.
    pub coin1_sk_root: Value<pallas::Base>,
    /// Merkle path to the secret key of `coin_1` in the Merkle tree of secret keys.
    pub coin1_sk_merkle_path: Value<[MerkleNode; MERKLE_DEPTH_ORCHARD]>,
    /// $\tau$ (in the crypsinous paper), can be slot index, or coin timestamp.
    /// Used only in the public key calculation.
    pub coin1_timestamp: Value<pallas::Base>,
    /// `coin_1` nonce, a random sampled value. `coin_2` nonce is calculated
    /// inside the circuit as `Hash(coin1_nonce || coin1_sk_root)`, assuming
    /// `Hash` is a function that satisfies a PRF definition.
    pub coin1_nonce: Value<pallas::Base>,
    /// Blinding factor for the commitment of `coin_1`
    pub coin1_blind: Value<pallas::Scalar>,
    /// Serial number for `coin_1`
    pub coin1_serial: Value<pallas::Base>,
    /// Value of `coin_1`
    pub coin1_value: Value<pallas::Base>,
    /// Blinding factor for the commitment of `coin_2`
    pub coin2_blind: Value<pallas::Scalar>,
    /// `coin_2` pedersen commitment point
    pub coin2_commit: Value<pallas::Point>,
    /// Random value derived from `eta` used for constraining `rho`
    pub rho_mu: Value<pallas::Scalar>,
    /// Random value derived from `eta` used for calculating `y`.
    pub y_mu: Value<pallas::Scalar>,
    /// First coefficient in 1-term T (target function) approximation.
    /// sigma1 and sigma2 is not the capital sigma from the paper, but
    /// the whole coefficient multiplied with the absolute stake.
    pub sigma1: Value<pallas::Base>,
    /// Second coefficient in 2-term T (target function) approximation
    pub sigma2: Value<pallas::Base>,
    /// Constrained nonce `rho`.
    pub rho: Value<pallas::Point>,
}

impl Circuit<pallas::Base> for LeadContract {
    type Config = LeadConfig;
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

        // Configuration for curve point operations. This uses 10 advice columns.
        let ecc_config = EccChip::<OrchardFixedBases>::configure(
            meta,
            advices[..10].try_into().unwrap(),
            lagrange_coeffs,
            range_check,
        );

        let poseidon_config = PoseidonChip::configure::<P128Pow5T3>(
            meta,
            advices[10..13].try_into().unwrap(),
            advices[13],
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

        let (sinsemilla_config_2, merkle_config_2) = {
            let sinsemilla_config_2 = SinsemillaChip::configure(
                meta,
                advices[5..10].try_into().unwrap(),
                advices[7],
                lagrange_coeffs[1],
                lookup,
                range_check,
            );
            let merkle_config_2 = MerkleChip::configure(meta, sinsemilla_config_2.clone());
            (sinsemilla_config_2, merkle_config_2)
        };

        // Lookup table for native range checks and less-than check
        let k_values_table = meta.lookup_table_column();
        let lessthan_config = {
            /*
            let a = advices[10];
            let b = advices[11];
            let a_offset = advices[12];
            let z1 = advices[13];
            let z2 = advices[14];
            */
            let a = advices[9];
            let b = advices[10];
            let a_offset = advices[11];
            let z1 = advices[12];
            let z2 = advices[13];
            let constants = meta.fixed_column();
            meta.enable_constant(constants);
            meta.enable_equality(a);
            meta.enable_equality(b);
            meta.enable_equality(a_offset);

            LessThanChip::<3, 253, 85>::configure(meta, a, b, a_offset, z1, z2, k_values_table)
        };

        // Configuration for the arithmetic chip
        let arith_config = ArithChip::configure(meta, advices[7], advices[8], advices[6]);

        LeadConfig {
            primary,
            advices,
            ecc_config,
            poseidon_config,
            sinsemilla_config_1,
            _sinsemilla_config_2: sinsemilla_config_2,
            merkle_config_1,
            merkle_config_2,
            lessthan_config,
            arith_config,
        }
    }

    fn synthesize(
        &self,
        config: Self::Config,
        mut layouter: impl Layouter<pallas::Base>,
    ) -> Result<(), plonk::Error> {
        // Initialize necessary chips
        let lessthan_chip = config.lessthan_chip();
        NativeRangeCheckChip::<3, 253, 85>::load_k_table(
            &mut layouter,
            config.lessthan_config.k_values_table,
        )?;
        SinsemillaChip::load(config.sinsemilla_config_1.clone(), &mut layouter)?;
        let ecc_chip = config.ecc_chip();
        let arith_chip = config.arith_chip();

        // ====================================
        // Load witness values into the circuit
        // ====================================
        let prf_nullifier_prefix_base = assign_free_advice(
            layouter.namespace(|| "witness nullifier prefix"),
            config.advices[8],
            Value::known(pallas::Base::from(PRF_NULLIFIER_PREFIX)),
        )?;

        let coin1_commit_merkle_path: Value<[pallas::Base; MERKLE_DEPTH_ORCHARD]> = self
            .coin1_commit_merkle_path
            .map(|typed_path| gen_const_array(|i| typed_path[i].inner()));

        let coin1_commit_root = assign_free_advice(
            layouter.namespace(|| "witness coin_commitment_root"),
            config.advices[8],
            self.coin1_commit_root,
        )?;

        let coin1_sk = assign_free_advice(
            layouter.namespace(|| "witness coin1_sk"),
            config.advices[8],
            self.coin1_sk,
        )?;

        let coin1_sk_root = assign_free_advice(
            layouter.namespace(|| "witness coin1_sk_root"),
            config.advices[8],
            self.coin1_sk_root,
        )?;

        let _coin1_sk_merkle_path: Value<[pallas::Base; MERKLE_DEPTH_ORCHARD]> =
            self.coin1_sk_merkle_path.map(|typed_path| gen_const_array(|i| typed_path[i].inner()));

        let _coin1_timestamp = assign_free_advice(
            layouter.namespace(|| "witness coin1_timestamp"),
            config.advices[8],
            self.coin1_timestamp,
        )?;

        let coin1_nonce = assign_free_advice(
            layouter.namespace(|| "witness coin1_nonce"),
            config.advices[8],
            self.coin1_nonce,
        )?;

        let coin1_blind = ScalarFixed::new(
            ecc_chip.clone(),
            layouter.namespace(|| "witness coin1_blind"),
            self.coin1_blind,
        )?;

        let coin1_serial = assign_free_advice(
            layouter.namespace(|| "witness coin1_serial"),
            config.advices[8],
            self.coin1_serial,
        )?;

        let coin1_value = assign_free_advice(
            layouter.namespace(|| "witness coin1_value"),
            config.advices[8],
            self.coin1_value,
        )?;

        let coin2_blind = ScalarFixed::new(
            ecc_chip.clone(),
            layouter.namespace(|| "witness coin2_blind"),
            self.coin2_blind,
        )?;

        let coin2_commit = NonIdentityPoint::new(
            ecc_chip.clone(),
            layouter.namespace(|| "witness coin2_commit"),
            self.coin2_commit.as_ref().map(|cm| cm.to_affine()),
        )?;

        let rho_mu = ScalarFixed::new(
            ecc_chip.clone(),
            layouter.namespace(|| "witness rho_mu"),
            self.rho_mu,
        )?;

        let y_mu =
            ScalarFixed::new(ecc_chip.clone(), layouter.namespace(|| "witness y_mu"), self.y_mu)?;

        let sigma1 = assign_free_advice(
            layouter.namespace(|| "witness sigma1"),
            config.advices[8],
            self.sigma1,
        )?;

        let sigma2 = assign_free_advice(
            layouter.namespace(|| "witness sigma2"),
            config.advices[8],
            self.sigma2,
        )?;

        let rho = NonIdentityPoint::new(
            ecc_chip.clone(),
            layouter.namespace(|| "witness rho"),
            self.rho.as_ref().map(|cm| cm.to_affine()),
        )?;

        let zero = assign_free_advice(
            layouter.namespace(|| "witness constant zero"),
            config.advices[8],
            Value::known(pallas::Base::zero()),
        )?;

        let one = assign_free_advice(
            layouter.namespace(|| "witness constant one"),
            config.advices[8],
            Value::known(pallas::Base::one()),
        )?;

        // ========================
        // Derive coin's public key
        // ========================
        let coin_pk = {
            let coin_pk_commit_v = FixedPointBaseField::from_inner(ecc_chip.clone(), NullifierK);
            coin_pk_commit_v.mul(layouter.namespace(|| "coin_1sk * NullifierK"), coin1_sk)?
        };

        // Coin `c1` serial number:
        // sn=PRF_{root_sk}(nonce)
        // Coin's serial number is derived from coin nonce (sampled at random)
        // and root of the coin's secret key sampled at random.
        let sn_commit: AssignedCell<pallas::Base, pallas::Base> = {
            // For derivation here, we append one 0 and one 1 to the hashed message.
            // TODO: Add these constants to ouroboros/consts.rs
            let poseidon_message = [coin1_nonce.clone(), coin1_sk_root.clone(), zero, one.clone()];
            let poseidon_hasher = PoseidonHash::<_, _, P128Pow5T3, ConstantLength<4>, 3, 2>::init(
                config.poseidon_chip(),
                layouter.namespace(|| "sn_commit poseidon init"),
            )?;

            poseidon_hasher
                .hash(layouter.namespace(|| "sn_commit poseidon hash"), poseidon_message)?
        };

        // ==============================
        // Commitment to the staking coin
        // ==============================
        // coin commitment H=Commit(PRF(prefix||pk||V||nonce), r)
        let coin_commitment_v = {
            // Coin c1 nullifier is a commitment of the following:
            let nullifier_msg: AssignedCell<pallas::Base, pallas::Base> = {
                let poseidon_message = [
                    prf_nullifier_prefix_base.clone(),
                    coin_pk.inner().x(),
                    coin_pk.inner().y(),
                    coin1_value.clone(),
                    coin1_nonce.clone(),
                    one.clone(), // One is here because of poseidon odd-n bug
                ];
                let poseidon_hasher =
                    PoseidonHash::<_, _, P128Pow5T3, ConstantLength<6>, 3, 2>::init(
                        config.poseidon_chip(),
                        layouter.namespace(|| "nullifier poseidon init"),
                    )?;

                poseidon_hasher
                    .hash(layouter.namespace(|| "nullifier poseidon hash"), poseidon_message)?
            };

            let v = FixedPointBaseField::from_inner(ecc_chip.clone(), NullifierK);
            v.mul(layouter.namespace(|| "nullifier_msg * NullifierK"), nullifier_msg)?
        };

        let (coin_commitment_r, _) = {
            let r = FixedPoint::from_inner(ecc_chip.clone(), ValueCommitR);
            r.mul(layouter.namespace(|| "coin1_blind * ValueCommitR"), coin1_blind)?
        };

        let coin_commitment = coin_commitment_v.add(
            layouter.namespace(|| "coin_commitment_v + coin_commitment_r"),
            &coin_commitment_r,
        )?;

        // ================================================
        // Validate Merkle path to staked coin's commitment
        // ================================================
        let merkle_inputs = MerklePath::construct(
            [config.merkle_chip_1(), config.merkle_chip_2()],
            OrchardHashDomains::MerkleCrh,
            self.coin1_commit_leaf_pos,
            coin1_commit_merkle_path,
        );

        let coin1_commit_hash: AssignedCell<pallas::Base, pallas::Base> = {
            let poseidon_message = [coin_commitment.inner().x(), coin_commitment.inner().y()];
            let poseidon_hasher = PoseidonHash::<_, _, P128Pow5T3, ConstantLength<2>, 3, 2>::init(
                config.poseidon_chip(),
                layouter.namespace(|| "coin1_commit_hash poseidon init"),
            )?;

            poseidon_hasher
                .hash(layouter.namespace(|| "coin1_commit_hash poseidon hash"), poseidon_message)?
        };

        let coin1_cm_root = merkle_inputs.calculate_root(
            layouter.namespace(|| "calculate merkle root for coin1 commitment"),
            coin1_commit_hash,
        )?;

        // ===========================
        // Derivation of coin2's nonce
        // ===========================
        let coin2_nonce: AssignedCell<pallas::Base, pallas::Base> = {
            // For derivation here, we append 1 two times to the hashed message.
            // TODO: Add these constants to ouroboros/consts.rs
            let poseidon_message =
                [coin1_nonce.clone(), coin1_sk_root.clone(), one.clone(), one.clone()];
            let poseidon_hasher = PoseidonHash::<_, _, P128Pow5T3, ConstantLength<4>, 3, 2>::init(
                config.poseidon_chip(),
                layouter.namespace(|| "coin2_nonce poseidon init"),
            )?;

            poseidon_hasher
                .hash(layouter.namespace(|| "coin2_nonce poseidon hash"), poseidon_message)?
        };

        // ================
        // Coin2 commitment
        // ================
        // H=Commit(PRF(pk||V||nonce2), r2)
        // Poured coin's commitment is a nullifier
        let coin2_commitment_v = {
            // coin2's commitment input body as a poseidon hash of input
            // concatenation of public key, stake, and poured coin's nonce.
            let nullifier2: AssignedCell<pallas::Base, pallas::Base> = {
                let poseidon_message = [
                    prf_nullifier_prefix_base,
                    coin_pk.inner().x(),
                    coin_pk.inner().y(),
                    coin1_value.clone(),
                    coin2_nonce.clone(),
                    one, // Used here because of poseidon odd-n bug
                ];
                let poseidon_hasher =
                    PoseidonHash::<_, _, P128Pow5T3, ConstantLength<6>, 3, 2>::init(
                        config.poseidon_chip(),
                        layouter.namespace(|| "coin2_commitment_v poseidon init"),
                    )?;

                poseidon_hasher.hash(
                    layouter.namespace(|| "coin2_commitment_v poseidon hash"),
                    poseidon_message,
                )?
            };

            let v = FixedPointBaseField::from_inner(ecc_chip.clone(), NullifierK);
            v.mul(layouter.namespace(|| "nullifier2 * NullifierK"), nullifier2)?
        };

        let (coin2_commitment_r, _) = {
            let r = FixedPoint::from_inner(ecc_chip.clone(), ValueCommitR);
            r.mul(layouter.namespace(|| "coin2_blind * ValueCommitR"), coin2_blind)?
        };

        let coin2_commitment = coin2_commitment_v.add(
            layouter.namespace(|| "coin2_commitment_v + coin2_commitment_r"),
            &coin2_commitment_r,
        )?;

        // ==================================
        // lhs of the leader election lottery
        // ==================================
        // * y as Commit(root_sk||nonce, y_mu)
        // Commitment to the coin's secret key, coin's nonce, and random value
        // derived from the epoch sampled random eta.
        let lottery_commit_msg: AssignedCell<pallas::Base, pallas::Base> = {
            let poseidon_message = [coin1_sk_root, coin1_nonce];
            let poseidon_hasher = PoseidonHash::<_, _, P128Pow5T3, ConstantLength<2>, 3, 2>::init(
                config.poseidon_chip(),
                layouter.namespace(|| "lottery_commit_msg poseidon init"),
            )?;

            poseidon_hasher
                .hash(layouter.namespace(|| "lottery_commit_msg poseidon hash"), poseidon_message)?
        };

        let lottery_commit_v = {
            let v = FixedPointBaseField::from_inner(ecc_chip.clone(), NullifierK);
            v.mul(layouter.namespace(|| "lottery_commit_msg * NullifierK"), lottery_commit_msg)?
        };

        let (lottery_commit_r, _) = {
            let r = FixedPoint::from_inner(ecc_chip.clone(), ValueCommitR);
            r.mul(layouter.namespace(|| "y_mu * ValueCommitR"), y_mu)?
        };

        let y_commit = lottery_commit_v
            .add(layouter.namespace(|| "lottery_commit_v + lottery_commit_r"), &lottery_commit_r)?;

        // Hash the coordinates to get a base field element
        let y_commit_base: AssignedCell<pallas::Base, pallas::Base> = {
            let poseidon_message = [y_commit.inner().x(), y_commit.inner().y()];
            let poseidon_hasher = PoseidonHash::<_, _, P128Pow5T3, ConstantLength<2>, 3, 2>::init(
                config.poseidon_chip(),
                layouter.namespace(|| "lottery_commit coords poseidon init"),
            )?;

            poseidon_hasher.hash(
                layouter.namespace(|| "lottery_commit coords poseidon hash"),
                poseidon_message,
            )?
        };

        // y_commit also becomes V of the following pedersen commitment for rho
        let (rho_cm, _) = {
            let rho_commit_r = FixedPoint::from_inner(ecc_chip, ValueCommitR);
            rho_commit_r.mul(layouter.namespace(|| "coin serial number commit R"), rho_mu)?
        };
        let rho_commit = lottery_commit_v.add(layouter.namespace(|| "nonce commit"), &rho_cm)?;

        // Calculate term1 and term2 for the lottery
        let term1 = arith_chip.mul(
            layouter.namespace(|| "term1 = sigma1 * coin1_value"),
            &sigma1,
            &coin1_value,
        )?;
        let term2_1 = arith_chip.mul(
            layouter.namespace(|| "term2_1 = sigma2 * coin1_value"),
            &sigma2,
            &coin1_value,
        )?;
        let term2 = arith_chip.mul(
            layouter.namespace(|| "term2 = term2_1 * coin1_value"),
            &term2_1,
            &coin1_value,
        )?;

        // Calculate lottery target
        let target =
            arith_chip.add(layouter.namespace(|| "target = term1 + term2"), &term1, &term2)?;
        let t: Value<pallas::Base> = target.value().cloned();
        let y: Value<pallas::Base> = y_commit_base.value().cloned();

        info!("y: {:?}", y);
        info!("T: {:?}", t);

        // Constrain derived `sn_commit` to be equal to witnessed `coin1_serial`.
        info!("coin1 cm root LHS: {:?}", coin1_cm_root.value());
        info!("coin1 cm root RHS: {:?}", coin1_commit_root.value());
        layouter.assign_region(
            || "coin1_cm_root equality",
            |mut region| region.constrain_equal(coin1_cm_root.cell(), coin1_commit_root.cell()),
        )?;
        info!("coin1 serial commit LHS: {:?}", sn_commit.value());
        info!("coin1 serial commit RHS: {:?}", coin1_serial.value());
        layouter.assign_region(
            || "sn_commit equality",
            |mut region| region.constrain_equal(sn_commit.cell(), coin1_serial.cell()),
        )?;

        info!("coin2_commit LHS: x {:?}", coin2_commitment.inner().x());
        info!("coin2_commit LHS: y {:?}", coin2_commitment.inner().y());
        info!("coin2_commit RHS: x {:?}", coin2_commit.inner().x());
        info!("coin2_commit RHS: y {:?}", coin2_commit.inner().y());
        // Constrain equality between witnessed and derived commitment
        coin2_commitment
            .constrain_equal(layouter.namespace(|| "coin2_commit equality"), &coin2_commit)?;

        info!("rho commit LHS: x {:?}", rho_commit.inner().x());
        info!("rho commit LHS: y {:?}", rho_commit.inner().y());
        info!("rho commit RHS: x {:?}", rho.inner().x());
        info!("rho commit RHS: y {:?}", rho.inner().y());
        // Constrain derived rho_commit to witnessed rho
        rho_commit.constrain_equal(layouter.namespace(|| "rho equality"), &rho)?;

        info!("coin pk: x {:?}", coin_pk.inner().x());
        info!("coin pk: y {:?}", coin_pk.inner().y());
        // Constrain coin's public key coordinates with public inputs
        layouter.constrain_instance(
            coin_pk.inner().x().cell(),
            config.primary,
            LEADCOIN_PK_X_OFFSET,
        )?;
        layouter.constrain_instance(
            coin_pk.inner().y().cell(),
            config.primary,
            LEADCOIN_PK_Y_OFFSET,
        )?;
        info!("coin2 nonce: {:?}", coin2_nonce);
        // Constrain coin2_nonce with associated public input
        layouter.constrain_instance(
            coin2_nonce.cell(),
            config.primary,
            LEADCOIN_C2_NONCE_OFFSET,
        )?;

        // Constrain y to its respective public input
        layouter.constrain_instance(
            y_commit_base.cell(),
            config.primary,
            LEADCOIN_Y_BASE_OFFSET,
        )?;

        // Constrain y < target
        lessthan_chip.copy_less_than(
            layouter.namespace(|| "y < target"),
            y_commit_base,
            target,
            0,
            true,
        )?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Result;
    use halo2_proofs::dev::CircuitLayout;
    use plotters::prelude::*;

    #[test]
    fn test_leader_circuit() -> Result<()> {
        let k = 11;
        let circuit = LeadContract::default();

        let root = BitMapBackend::new("target/leader_circuit_layout.png", (3840, 2160))
            .into_drawing_area();
        root.fill(&WHITE).unwrap();
        let root = root.titled("Lead Circuit Layout", ("sans-serif", 60)).unwrap();
        CircuitLayout::default()
            //.view_width(0..10)
            .render(k, &circuit, &root)
            .unwrap();

        Ok(())
    }
}
