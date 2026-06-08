/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use darkfi_sdk::crypto::smt::SMT_FP_DEPTH;
use halo2_gadgets::poseidon::{
    primitives as poseidon, Hash as PoseidonHash, Pow5Chip as PoseidonChip,
    Pow5Config as PoseidonConfig,
};
use halo2_proofs::{
    circuit::{AssignedCell, Layouter, Value},
    pasta::{
        group::ff::{Field, PrimeFieldBits},
        Fp,
    },
    plonk::{self, Advice, Column, ConstraintSystem, Constraints, Expression, Selector},
    poly::Rotation,
};

use super::{
    cond_select::{ConditionalSelectChip, ConditionalSelectConfig, NUM_OF_UTILITY_ADVICE_COLUMNS},
    is_equal::{AssertEqualChip, AssertEqualConfig},
};

// Constants for the canonical-position (`< p`) enforcement.

/// `2^128`
#[inline]
fn two_pow_128() -> Fp {
    Fp::from_raw([0, 0, 1, 0])
}

/// `2^126`
#[inline]
fn two_pow_126() -> Fp {
    Fp::from_raw([0, 0x4000_0000_0000_0000, 0, 0])
}

/// `T - 1` where `T = p - 2^254`
#[inline]
fn p_low_tail_minus_one() -> Fp {
    Fp::from_raw([0x992d_30ed_0000_0000, 0x2246_98fc_094c_f91b, 0, 0])
}

#[derive(Clone, Debug)]
pub struct PathConfig {
    s_path: Selector,
    s_canon: Selector,
    s_bool: Selector,
    advices: [Column<Advice>; 2],
    utility_advices: [Column<Advice>; NUM_OF_UTILITY_ADVICE_COLUMNS],
    poseidon_config: PoseidonConfig<Fp, 3, 2>,
    conditional_select_config: ConditionalSelectConfig<Fp>,
    assert_equal_config: AssertEqualConfig<Fp>,
}

impl PathConfig {
    fn poseidon_chip(&self) -> PoseidonChip<Fp, 3, 2> {
        PoseidonChip::construct(self.poseidon_config.clone())
    }

    fn conditional_select_chip(&self) -> ConditionalSelectChip<Fp> {
        ConditionalSelectChip::construct(self.conditional_select_config.clone())
    }

    fn assert_eq_chip(&self) -> AssertEqualChip<Fp> {
        AssertEqualChip::construct(self.assert_equal_config.clone())
    }
}

#[derive(Clone, Debug)]
pub struct PathChip {
    config: PathConfig,
}

impl PathChip {
    pub fn configure(
        meta: &mut ConstraintSystem<Fp>,
        advices: [Column<Advice>; 2],
        utility_advices: [Column<Advice>; NUM_OF_UTILITY_ADVICE_COLUMNS],
        poseidon_config: PoseidonConfig<Fp, 3, 2>,
    ) -> PathConfig {
        let s_path = meta.selector();
        let s_canon = meta.selector();
        let s_bool = meta.selector();

        for advice in &advices {
            meta.enable_equality(*advice);
        }

        for advice in &utility_advices {
            meta.enable_equality(*advice);
        }

        meta.create_gate("Path builder", |meta| {
            let s_path = meta.query_selector(s_path);
            let current_path = meta.query_advice(advices[0], Rotation::cur());
            let bit = meta.query_advice(advices[1], Rotation::cur());
            let next_path = meta.query_advice(advices[0], Rotation::next());

            Constraints::with_selector(s_path, Some(next_path - (current_path * Fp::from(2) + bit)))
        });

        // Boolean constraint for an auxiliary witnessed bit in `advices[1]`.
        // The position bits get their booleanity from the cond-select chip.
        // This gate is only used to range-check the helper value `k`.
        meta.create_gate("aux bit is boolean", |meta| {
            let s_bool = meta.query_selector(s_bool);
            let bit = meta.query_advice(advices[1], Rotation::cur());
            let one = Expression::Constant(Fp::ONE);
            Constraints::with_selector(s_bool, Some(bit.clone() * (one - bit)))
        });

        // Canonicity of the position decomposition (`pos < p`)
        // Constraints:
        // a) pos == high * 2^128 + low    (defines `low`, the low 128 bits)
        // b) top == 1 -> high == 2^126    (bit254=1 forces bits[128..254]=0)
        // c) top == 1 -> low + k == T-1   (with k in [0,2^128): forces low<T)
        //
        // Together these force the 255-bit value to be < p.
        meta.create_gate("canonical SMT position (< p)", |meta| {
            let s_canon = meta.query_selector(s_canon);
            let pos = meta.query_advice(advices[0], Rotation::cur());
            let high = meta.query_advice(advices[1], Rotation::cur());
            let low = meta.query_advice(utility_advices[0], Rotation::cur());
            let k = meta.query_advice(utility_advices[1], Rotation::cur());
            let top = meta.query_advice(utility_advices[2], Rotation::cur());

            let two_128 = Expression::Constant(two_pow_128());
            let two_126 = Expression::Constant(two_pow_126());
            let t_minus_1 = Expression::Constant(p_low_tail_minus_one());

            Constraints::with_selector(
                s_canon,
                [
                    // a) binding: pos == high * 2^128 + low
                    high.clone() * two_128 + low.clone() - pos,
                    // b) top == 1 -> high == 2^126
                    top.clone() * (high - two_126),
                    // c) top == 1 -> low + k == T-1
                    top * (low + k - t_minus_1),
                ],
            )
        });

        PathConfig {
            s_path,
            s_canon,
            s_bool,
            advices,
            utility_advices,
            poseidon_config,
            conditional_select_config: ConditionalSelectChip::configure(meta, utility_advices),
            assert_equal_config: AssertEqualChip::configure(
                meta,
                [utility_advices[0], utility_advices[1]],
            ),
        }
    }

    pub fn construct(config: PathConfig) -> Self {
        Self { config }
    }

    fn decompose_value(value: &Fp) -> Vec<Fp> {
        // Returns 256 bits, but the last bit is uneeded
        let bits: Vec<bool> = value.to_le_bits().into_iter().collect();

        let mut bits: Vec<Fp> = bits[..SMT_FP_DEPTH].iter().map(|x| Fp::from(*x)).collect();
        bits.resize(SMT_FP_DEPTH, Fp::from(0));
        bits
    }

    pub fn check_membership(
        &self,
        layouter: &mut impl Layouter<Fp>,
        pos: AssignedCell<Fp, Fp>,
        path: Value<[Fp; SMT_FP_DEPTH]>,
        leaf: AssignedCell<Fp, Fp>,
    ) -> Result<AssignedCell<Fp, Fp>, plonk::Error> {
        // The honest prover decomposes `pos` into its canonical little-endian
        // bits. The constraint system additionally enforces canonicity, so
        // a malicious prover cannot substitute the non-canonical `pos+p`
        // decomposition to walk a different leaf.
        let bits = pos.value().map(Self::decompose_value);
        self.membership_inner(layouter, pos, bits, path, leaf)
    }

    /// Core membership/walk logic. `bits_value` is the little-endian position
    /// decomposition supplied by the prover. All of the constraints live here.
    fn membership_inner(
        &self,
        layouter: &mut impl Layouter<Fp>,
        pos: AssignedCell<Fp, Fp>,
        bits_value: Value<Vec<Fp>>,
        path: Value<[Fp; SMT_FP_DEPTH]>,
        leaf: AssignedCell<Fp, Fp>,
    ) -> Result<AssignedCell<Fp, Fp>, plonk::Error> {
        let path = path.transpose_array();
        // Witness values
        let (bits, path, zero) = layouter.assign_region(
            || "witness",
            |mut region| {
                let bits = bits_value.clone().transpose_vec(SMT_FP_DEPTH);
                assert_eq!(bits.len(), SMT_FP_DEPTH);

                let mut witness_bits = vec![];
                let mut witness_path = vec![];
                for (i, (bit, sibling)) in bits.into_iter().zip(path).enumerate() {
                    let bit = region.assign_advice(
                        || "witness pos bit",
                        self.config.advices[0],
                        i,
                        || bit,
                    )?;
                    witness_bits.push(bit);

                    let sibling = region.assign_advice(
                        || "witness path sibling",
                        self.config.advices[1],
                        i,
                        || sibling,
                    )?;
                    witness_path.push(sibling);
                }

                let zero = region.assign_advice(
                    || "witness zero",
                    self.config.advices[0],
                    SMT_FP_DEPTH,
                    || Value::known(Fp::ZERO),
                )?;
                region.constrain_constant(zero.cell(), Fp::ZERO)?;

                Ok((witness_bits, witness_path, zero))
            },
        )?;
        assert_eq!(bits.len(), path.len());
        assert_eq!(bits.len(), SMT_FP_DEPTH);

        let condselect_chip = self.config.conditional_select_chip();
        let asserteq_chip = self.config.assert_eq_chip();

        // Check path construction
        let mut current_path = zero;
        for bit in bits.iter().rev() {
            current_path = layouter.assign_region(
                || "pᵢ₊₁ = 2pᵢ + bᵢ",
                |mut region| {
                    self.config.s_path.enable(&mut region, 0)?;

                    current_path.copy_advice(
                        || "current path",
                        &mut region,
                        self.config.advices[0],
                        0,
                    )?;
                    bit.copy_advice(|| "path bit", &mut region, self.config.advices[1], 0)?;

                    let next_path =
                        current_path.value().zip(bit.value()).map(|(p, b)| p * Fp::from(2) + b);
                    region.assign_advice(|| "next path", self.config.advices[0], 1, || next_path)
                },
            )?;
        }

        // Force the position decomposition to be the canonical one.
        self.enforce_canonical_position(layouter, &bits, pos.clone())?;

        // Check tree construction
        let mut current_node = leaf.clone();
        for (bit, sibling) in bits.into_iter().zip(path.into_iter().rev()) {
            // Conditional select also constraints the bit ∈ {0, 1}
            let left = condselect_chip.conditional_select(
                layouter,
                sibling.clone(),
                current_node.clone(),
                bit.clone(),
            )?;
            let right = condselect_chip.conditional_select(
                layouter,
                current_node.clone(),
                sibling,
                bit.clone(),
            )?;
            //println!("bit: {:?}", bit);
            //println!("left: {:?}, right: {:?}", left, right);

            let hasher = PoseidonHash::<
                _,
                _,
                poseidon::P128Pow5T3,
                poseidon::ConstantLength<2>,
                3,
                2,
            >::init(
                self.config.poseidon_chip(),
                layouter.namespace(|| "SmtPoseidonHash init"),
            )?;

            current_node =
                hasher.hash(layouter.namespace(|| "SmtPoseidonHash hash"), [left, right])?;
        }

        asserteq_chip.assert_equal(layouter, current_path, pos)?;

        let root = current_node;
        Ok(root)
    }

    /// Enforce that the witnessed position bits are the canonical
    /// little-endian decomposition, rejecting the non-canonical `pos+p`
    /// representation.
    fn enforce_canonical_position(
        &self,
        layouter: &mut impl Layouter<Fp>,
        bits: &[AssignedCell<Fp, Fp>],
        pos: AssignedCell<Fp, Fp>,
    ) -> Result<(), plonk::Error> {
        assert_eq!(bits.len(), SMT_FP_DEPTH);

        let high = self.running_sum_from_bits(layouter, &bits[128..SMT_FP_DEPTH])?;
        let top = bits[SMT_FP_DEPTH - 1].clone();
        let low_val = pos.value().zip(high.value()).map(|(p, h)| *p - *h * two_pow_128());

        let k_val = top.value().zip(low_val).map(|(t, l)| {
            if *t == Fp::ONE {
                p_low_tail_minus_one() - l
            } else {
                Fp::ZERO
            }
        });

        let k = self.range_check_value(layouter, k_val, 128)?;

        // Bind the values and fire the canonicity gate.
        layouter.assign_region(
            || "canonical SMT position",
            |mut region| {
                self.config.s_canon.enable(&mut region, 0)?;
                pos.copy_advice(|| "pos", &mut region, self.config.advices[0], 0)?;
                high.copy_advice(|| "high", &mut region, self.config.advices[1], 0)?;
                region.assign_advice(|| "low", self.config.utility_advices[0], 0, || low_val)?;
                k.copy_advice(|| "k", &mut region, self.config.utility_advices[1], 0)?;
                top.copy_advice(|| "top", &mut region, self.config.utility_advices[2], 0)?;
                Ok(())
            },
        )?;

        Ok(())
    }

    fn running_sum_from_bits(
        &self,
        layouter: &mut impl Layouter<Fp>,
        bits: &[AssignedCell<Fp, Fp>],
    ) -> Result<AssignedCell<Fp, Fp>, plonk::Error> {
        let mut acc = layouter.assign_region(
            || "running-sum: zero",
            |mut region| {
                let z = region.assign_advice(
                    || "zero",
                    self.config.advices[0],
                    0,
                    || Value::known(Fp::ZERO),
                )?;
                region.constrain_constant(z.cell(), Fp::ZERO)?;
                Ok(z)
            },
        )?;

        for bit in bits.iter().rev() {
            acc = layouter.assign_region(
                || "running-sum: acc = 2*acc + bit",
                |mut region| {
                    self.config.s_path.enable(&mut region, 0)?;
                    acc.copy_advice(|| "acc", &mut region, self.config.advices[0], 0)?;
                    bit.copy_advice(|| "bit", &mut region, self.config.advices[1], 0)?;
                    let next = acc.value().zip(bit.value()).map(|(a, b)| *a * Fp::from(2) + *b);
                    region.assign_advice(|| "next acc", self.config.advices[0], 1, || next)
                },
            )?;
        }

        Ok(acc)
    }

    /// Witness `value` as `n_bits` fresh boolean bits
    fn range_check_value(
        &self,
        layouter: &mut impl Layouter<Fp>,
        value: Value<Fp>,
        n_bits: usize,
    ) -> Result<AssignedCell<Fp, Fp>, plonk::Error> {
        let mut acc = layouter.assign_region(
            || "range-check: zero",
            |mut region| {
                let z = region.assign_advice(
                    || "zero",
                    self.config.advices[0],
                    0,
                    || Value::known(Fp::ZERO),
                )?;
                region.constrain_constant(z.cell(), Fp::ZERO)?;
                Ok(z)
            },
        )?;

        for i in (0..n_bits).rev() {
            let bit_val = value.map(|v| {
                let le: Vec<bool> = v.to_le_bits().into_iter().collect();
                if le[i] {
                    Fp::ONE
                } else {
                    Fp::ZERO
                }
            });

            acc = layouter.assign_region(
                || "range-check: acc = 2*acc + bit (bit boolean)",
                |mut region| {
                    self.config.s_path.enable(&mut region, 0)?;
                    self.config.s_bool.enable(&mut region, 0)?;
                    acc.copy_advice(|| "acc", &mut region, self.config.advices[0], 0)?;
                    region.assign_advice(|| "bit", self.config.advices[1], 0, || bit_val)?;
                    let next = acc.value().zip(bit_val).map(|(a, b)| *a * Fp::from(2) + b);
                    region.assign_advice(|| "next acc", self.config.advices[0], 1, || next)
                },
            )?;
        }

        Ok(acc)
    }

    #[cfg(test)]
    pub(crate) fn check_membership_with_bits(
        &self,
        layouter: &mut impl Layouter<Fp>,
        pos: AssignedCell<Fp, Fp>,
        bits: Value<Vec<Fp>>,
        path: Value<[Fp; SMT_FP_DEPTH]>,
        leaf: AssignedCell<Fp, Fp>,
    ) -> Result<AssignedCell<Fp, Fp>, plonk::Error> {
        self.membership_inner(layouter, pos, bits, path, leaf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use darkfi_sdk::crypto::smt::{MemoryStorageFp, PoseidonFp, SmtMemoryFp, EMPTY_NODES_FP};
    use halo2_proofs::{circuit::floor_planner, dev::MockProver, plonk::Circuit};
    use rand::rngs::OsRng;

    struct TestCircuit {
        path: Value<[Fp; SMT_FP_DEPTH]>,
        leaf: Value<Fp>,
        root: Value<Fp>,
    }

    impl Circuit<Fp> for TestCircuit {
        type Config = PathConfig;
        type FloorPlanner = floor_planner::V1;
        type Params = ();

        fn without_witnesses(&self) -> Self {
            Self { root: Value::unknown(), path: Value::unknown(), leaf: Value::unknown() }
        }

        fn configure(meta: &mut ConstraintSystem<Fp>) -> Self::Config {
            // Advice wires required by PathChip
            let advices = [(); 2].map(|_| meta.advice_column());
            let utility_advices = [(); NUM_OF_UTILITY_ADVICE_COLUMNS].map(|_| meta.advice_column());

            // Setup poseidon config
            let poseidon_advices = [(); 4].map(|_| meta.advice_column());
            for advice in &poseidon_advices {
                meta.enable_equality(*advice);
            }

            // Needed for poseidon hash
            let col_const = meta.fixed_column();
            meta.enable_constant(col_const);

            let rc_a = [(); 3].map(|_| meta.fixed_column());
            let rc_b = [(); 3].map(|_| meta.fixed_column());

            let poseidon_config = PoseidonChip::configure::<poseidon::P128Pow5T3>(
                meta,
                poseidon_advices[1..4].try_into().unwrap(),
                poseidon_advices[0],
                rc_a,
                rc_b,
            );

            PathChip::configure(meta, advices, utility_advices, poseidon_config)
        }

        fn synthesize(
            &self,
            config: Self::Config,
            mut layouter: impl Layouter<Fp>,
        ) -> Result<(), plonk::Error> {
            // Initialize the Path chip
            let path_chip = PathChip::construct(config.clone());

            // Initialize the AssertEqual chip
            let assert_eq_chip = config.assert_eq_chip();

            // Witness values
            let (leaf, root) = layouter.assign_region(
                || "witness",
                |mut region| {
                    let leaf = region.assign_advice(
                        || "witness leaf",
                        config.advices[0],
                        0,
                        || self.leaf,
                    )?;
                    let root = region.assign_advice(
                        || "witness root",
                        config.advices[1],
                        0,
                        || self.root,
                    )?;
                    Ok((leaf, root))
                },
            )?;

            let calc_root =
                path_chip.check_membership(&mut layouter, leaf.clone(), self.path, leaf.clone())?;
            // Normally we just reveal it as a public input.
            // But I'm too lazy to make a separate config for this unit test so
            // do this instead.
            assert_eq_chip.assert_equal(&mut layouter, calc_root, root)?;

            Ok(())
        }
    }

    #[test]
    fn test_smt_circuit() {
        let hasher = PoseidonFp::new();
        let store = MemoryStorageFp::new();
        let mut smt = SmtMemoryFp::new(store, hasher.clone(), &EMPTY_NODES_FP);

        let leaves = vec![Fp::random(&mut OsRng), Fp::random(&mut OsRng), Fp::random(&mut OsRng)];
        // Use the leaf value as its position in the SMT
        // Therefore we need an additional constraint that leaf == pos
        let leaves: Vec<_> = leaves.into_iter().map(|l| (l, l)).collect();
        smt.insert_batch(leaves.clone()).unwrap();

        let (pos, leaf) = leaves[2];
        assert_eq!(pos, leaf);
        assert_eq!(smt.get_leaf(&pos), leaf);

        let root = smt.root();
        let path = smt.prove_membership(&pos);
        assert!(path.verify(&root, &leaf, &pos));

        let circuit = TestCircuit {
            path: Value::known(path.path),
            leaf: Value::known(leaf),
            root: Value::known(root),
        };

        const K: u32 = 14;
        let prover = MockProver::run(K, &circuit, vec![]).unwrap();
        prover.assert_satisfied();

        //use halo2_proofs::dev::CircuitLayout;
        //use plotters::prelude::*;
        //let root = BitMapBackend::new("target/smt.png", (3840, 2160)).into_drawing_area();
        //CircuitLayout::default().render(K, &circuit, &root).unwrap();
    }

    #[test]
    /// SMT non-membership forgery via non-canonical decomposition
    fn smt_exclusion_forgery() {
        use darkfi_sdk::crypto::{
            pasta_prelude::PrimeField,
            smt::{util::FieldHasher, StorageAdapter},
        };
        use halo2_proofs::plonk::Instance;
        use num_bigint::BigUint;

        #[derive(Clone)]
        struct ForgeryConfig {
            path_config: PathConfig,
            instance: Column<Instance>,
        }

        #[derive(Clone)]
        struct ForgeryCircuit {
            pos: Value<Fp>,
            bits: Value<Vec<Fp>>,
            path: Value<[Fp; SMT_FP_DEPTH]>,
            leaf: Value<Fp>,
        }

        impl Circuit<Fp> for ForgeryCircuit {
            type Config = ForgeryConfig;
            type FloorPlanner = floor_planner::V1;
            type Params = ();

            fn without_witnesses(&self) -> Self {
                Self {
                    pos: Value::unknown(),
                    bits: Value::unknown(),
                    path: Value::unknown(),
                    leaf: Value::unknown(),
                }
            }

            fn configure(meta: &mut ConstraintSystem<Fp>) -> Self::Config {
                let advices = [(); 2].map(|_| meta.advice_column());
                let utility_advices =
                    [(); NUM_OF_UTILITY_ADVICE_COLUMNS].map(|_| meta.advice_column());
                let poseidon_advices = [(); 4].map(|_| meta.advice_column());

                for advice in &poseidon_advices {
                    meta.enable_equality(*advice);
                }

                let col_const = meta.fixed_column();
                meta.enable_constant(col_const);

                let rc_a = [(); 3].map(|_| meta.fixed_column());
                let rc_b = [(); 3].map(|_| meta.fixed_column());
                let poseidon_config = PoseidonChip::configure::<poseidon::P128Pow5T3>(
                    meta,
                    poseidon_advices[1..4].try_into().unwrap(),
                    poseidon_advices[0],
                    rc_a,
                    rc_b,
                );

                let path_config =
                    PathChip::configure(meta, advices, utility_advices, poseidon_config);

                let instance = meta.instance_column();
                meta.enable_equality(instance);
                ForgeryConfig { path_config, instance }
            }

            fn synthesize(
                &self,
                config: Self::Config,
                mut layouter: impl Layouter<Fp>,
            ) -> Result<(), plonk::Error> {
                let path_chip = PathChip::construct(config.path_config.clone());

                let (pos, leaf) = layouter.assign_region(
                    || "witness",
                    |mut region| {
                        let pos = region.assign_advice(
                            || "pos",
                            config.path_config.advices[0],
                            0,
                            || self.pos,
                        )?;
                        let leaf = region.assign_advice(
                            || "leaf",
                            config.path_config.advices[1],
                            0,
                            || self.leaf,
                        )?;
                        Ok((pos, leaf))
                    },
                )?;

                let root = path_chip.check_membership_with_bits(
                    &mut layouter,
                    pos,
                    self.bits.clone(),
                    self.path,
                    leaf,
                )?;

                layouter.constrain_instance(root.cell(), config.instance, 0)?;
                Ok(())
            }
        }

        fn native_walk(
            leaf: Fp,
            bits: &[Fp],
            path: &[Fp; SMT_FP_DEPTH],
            hasher: &PoseidonFp,
        ) -> Fp {
            let mut cur = leaf;
            for (bit, sibling) in bits.iter().zip(path.iter().rev()) {
                let (l, r) = if *bit == Fp::ONE { (*sibling, cur) } else { (cur, *sibling) };
                cur = hasher.hash([l, r]);
            }

            cur
        }

        fn le_bits(x: &BigUint) -> Vec<Fp> {
            (0..SMT_FP_DEPTH).map(|i| if x.bit(i as u64) { Fp::ONE } else { Fp::ZERO }).collect()
        }

        let hasher = PoseidonFp::new();
        let p: BigUint =
            BigUint::from_bytes_le((-Fp::ONE).to_repr().as_ref()) + BigUint::from(1u32);
        assert_eq!(p.bits(), 255);

        // Build the nullifier tree and "spend" N (inserted as (N, N), like Money).
        // N is in the LEFT half (bit 254 == 0) so the alias N+p lands in the empty
        // RIGHT half and fits in 255 bits. ~all nullifiers qualify (P[fail] ~= 2^-128).
        let n = Fp::from_raw([0x0123456789abcdef, 0xfedcba9876543210, 0x0011223344556677, 1]);
        let n_big = BigUint::from_bytes_le(n.to_repr().as_ref());
        assert!(!n_big.bit(254));

        let store = MemoryStorageFp::new();
        let mut smt = SmtMemoryFp::new(store, hasher.clone(), &EMPTY_NODES_FP);
        smt.insert_batch(vec![(n, n)]).unwrap();
        let root = smt.root();
        assert_eq!(smt.get_leaf(&n), n, "N must be present (spent)");

        // Forge non-canonical bits = LE bits of (N + p).
        let q = &n_big + &p;
        assert!(q < (BigUint::from(1u32) << 255u32), "N+p must fit in 255 bits");
        assert_eq!(&q % &p, n_big, "N+p reduces to N mod p");
        let forged_bits = le_bits(&q);
        assert_eq!(forged_bits[254], Fp::ONE, "the alias flips bit 254");

        // The forged bits still sum to N over the field, so the gadget's
        // assert_equal(position_sum, pos) passes — only canonicity rejects them.
        let mut recomposed = Fp::ZERO;
        for b in forged_bits.iter().rev() {
            recomposed = recomposed.double() + b;
        }
        assert_eq!(recomposed, n, "sum(forged_bit_i * 2^i) == N over the field");

        // Forge the auth path of the (empty) leaf at index N+p: the one top
        // sibling is the real left-subtree root; deeper siblings are empty defaults.
        let left_subtree_root =
            smt.store.get(&BigUint::from(1u32)).expect("left subtree root must be stored");
        let mut forged_path = [Fp::ZERO; SMT_FP_DEPTH];
        forged_path[0] = left_subtree_root;
        for i in 1..SMT_FP_DEPTH {
            forged_path[i] = EMPTY_NODES_FP[i + 1];
        }

        // NON-VACUOUSNESS ANCHOR: outside the circuit, the forged witness really
        // does authenticate an EMPTY leaf against the genuine root. So *without*
        // the canonicity constraint this proof WOULD verify - that is the bug.
        // With this asserted, a later rejection can only be the canonicity gate.
        assert_eq!(
            native_walk(Fp::ZERO, &forged_bits, &forged_path, &hasher),
            root,
            "forged path must reconstruct the real root (else the test is vacuous)"
        );

        let forged = ForgeryCircuit {
            pos: Value::known(n),
            bits: Value::known(forged_bits),
            path: Value::known(forged_path),
            leaf: Value::known(Fp::ZERO),
        };

        let failures = MockProver::run(14, &forged, vec![vec![root]])
            .unwrap()
            .verify()
            .expect_err("non-canonical (N+p) exclusion proof must be rejected");

        let report = format!("{failures:?}");
        assert!(
            report.contains("canonical SMT position"),
            "expected the canonicity gate to reject the forgery; failures were:\n{report}"
        );
    }
}
