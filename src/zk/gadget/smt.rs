/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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
    plonk::{self, Advice, Column, ConstraintSystem, Constraints, Selector},
    poly::Rotation,
};

use super::{
    cond_select::{ConditionalSelectChip, ConditionalSelectConfig, NUM_OF_UTILITY_ADVICE_COLUMNS},
    is_equal::{AssertEqualChip, AssertEqualConfig},
};

#[derive(Clone, Debug)]
pub struct PathConfig {
    s_path: Selector,
    advices: [Column<Advice>; 2],
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

        PathConfig {
            s_path,
            advices,
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
        let path = path.transpose_array();
        // Witness values
        let (bits, path, zero) = layouter.assign_region(
            || "witness",
            |mut region| {
                let bits = pos.value().map(Self::decompose_value).transpose_vec(SMT_FP_DEPTH);
                assert_eq!(bits.len(), SMT_FP_DEPTH);

                let mut witness_bits = vec![];
                let mut witness_path = vec![];
                for (i, (bit, sibling)) in bits.into_iter().zip(path.into_iter()).enumerate() {
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
}
