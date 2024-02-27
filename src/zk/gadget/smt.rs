/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
 * Copyright (C) 2022 zkMove Authors (Apache-2.0)
 * Copyright (C) 2021 Webb Technologies Inc. (Apache-2.0)
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

use std::marker::PhantomData;

use darkfi_sdk::crypto::smt::{FieldHasher, Path};
use halo2_gadgets::poseidon::{
    primitives as poseidon, Hash as PoseidonHash, Pow5Chip as PoseidonChip,
    Pow5Config as PoseidonConfig,
};
use halo2_proofs::{
    circuit::{AssignedCell, Layouter, Value},
    pasta::Fp,
    plonk::{self, Advice, Column, ConstraintSystem, Selector},
};

use super::{
    cond_select::{ConditionalSelectChip, ConditionalSelectConfig, NUM_OF_UTILITY_ADVICE_COLUMNS},
    is_equal::{AssertEqualChip, AssertEqualConfig, IsEqualChip, IsEqualConfig},
};

#[derive(Clone, Debug)]
pub struct PathConfig<const N: usize> {
    s_path: Selector,
    advices: [Column<Advice>; N],
    poseidon_config: PoseidonConfig<Fp, 3, 2>,
    is_eq_config: IsEqualConfig<Fp>,
    conditional_select_config: ConditionalSelectConfig<Fp>,
    assert_equal_config: AssertEqualConfig<Fp>,
}

impl<const N: usize> PathConfig<N> {
    fn poseidon_chip(&self) -> PoseidonChip<Fp, 3, 2> {
        PoseidonChip::construct(self.poseidon_config.clone())
    }

    fn is_eq_chip(&self) -> IsEqualChip<Fp> {
        IsEqualChip::construct(self.is_eq_config.clone())
    }

    fn conditional_select_chip(&self) -> ConditionalSelectChip<Fp> {
        ConditionalSelectChip::construct(self.conditional_select_config.clone())
    }

    fn assert_eq_chip(&self) -> AssertEqualChip<Fp> {
        AssertEqualChip::construct(self.assert_equal_config.clone())
    }
}

pub struct PathChip<H: FieldHasher<Fp, 2>, const N: usize> {
    path: [(AssignedCell<Fp, Fp>, AssignedCell<Fp, Fp>); N],
    config: PathConfig<N>,
    _hasher: PhantomData<H>,
}

impl<H: FieldHasher<Fp, 2>, const N: usize> PathChip<H, N> {
    pub fn configure(
        meta: &mut ConstraintSystem<Fp>,
        advices: [Column<Advice>; N],
        utility_advices: [Column<Advice>; NUM_OF_UTILITY_ADVICE_COLUMNS],
        poseidon_config: PoseidonConfig<Fp, 3, 2>,
    ) -> PathConfig<N> {
        let s_path = meta.selector();

        for advice in &advices {
            meta.enable_equality(*advice);
        }

        for advice in &utility_advices {
            meta.enable_equality(*advice);
        }

        PathConfig {
            s_path,
            advices,
            poseidon_config,
            is_eq_config: IsEqualChip::configure(meta, utility_advices),
            conditional_select_config: ConditionalSelectChip::configure(meta, utility_advices),
            assert_equal_config: AssertEqualChip::configure(
                meta,
                [utility_advices[0], utility_advices[1]],
            ),
        }
    }

    pub fn from_native(
        config: PathConfig<N>,
        layouter: &mut impl Layouter<Fp>,
        native: Path<Fp, H, N>,
    ) -> Result<Self, plonk::Error> {
        let path = layouter.assign_region(
            || "path",
            |mut region| {
                config.s_path.enable(&mut region, 0)?;
                let left = (0..N)
                    .map(|i| {
                        region.assign_advice(
                            || format!("path[{}][{}]", i, 0),
                            config.advices[i],
                            0,
                            || Value::known(native.path[i].0),
                        )
                    })
                    .collect::<Result<Vec<AssignedCell<Fp, Fp>>, plonk::Error>>();

                let right = (0..N)
                    .map(|i| {
                        region.assign_advice(
                            || format!("path[{}][{}]", i, 1),
                            config.advices[i],
                            1,
                            || Value::known(native.path[i].1),
                        )
                    })
                    .collect::<Result<Vec<AssignedCell<Fp, Fp>>, plonk::Error>>();

                let result = left?
                    .into_iter()
                    .zip(right?.into_iter())
                    .collect::<Vec<(AssignedCell<Fp, Fp>, AssignedCell<Fp, Fp>)>>();

                Ok(result.try_into().unwrap())
            },
        )?;

        Ok(PathChip { path, config, _hasher: PhantomData })
    }

    pub fn calculate_root(
        &self,
        layouter: &mut impl Layouter<Fp>,
        leaf: AssignedCell<Fp, Fp>,
    ) -> Result<AssignedCell<Fp, Fp>, plonk::Error> {
        // Check levels between leaf level and root
        let mut previous_hash = leaf;

        let iseq_chip = self.config.is_eq_chip();
        let condselect_chip = self.config.conditional_select_chip();
        let asserteq_chip = self.config.assert_eq_chip();

        for (left_hash, right_hash) in self.path.iter() {
            // Check if previous_hash matches the correct current hash
            let previous_is_left =
                iseq_chip.is_eq_with_output(layouter, previous_hash.clone(), left_hash.clone())?;

            let left_or_right = condselect_chip.conditional_select(
                layouter,
                left_hash.clone(),
                right_hash.clone(),
                previous_is_left,
            )?;

            asserteq_chip.assert_equal(layouter, previous_hash, left_or_right)?;

            // Update previous_hash
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

            previous_hash = hasher.hash(
                layouter.namespace(|| "SmtPoseidonHash hash"),
                [left_hash.clone(), right_hash.clone()],
            )?;
        }

        Ok(previous_hash)
    }

    pub fn check_membership(
        &self,
        layouter: &mut impl Layouter<Fp>,
        root_hash: AssignedCell<Fp, Fp>,
        leaf: AssignedCell<Fp, Fp>,
    ) -> Result<AssignedCell<Fp, Fp>, plonk::Error> {
        let computed_root = self.calculate_root(layouter, leaf)?;

        self.config.is_eq_chip().is_eq_with_output(layouter, computed_root, root_hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use darkfi_sdk::crypto::smt::{Poseidon, SparseMerkleTree};
    use halo2_proofs::{arithmetic::Field, circuit::floor_planner, plonk::Circuit};

    const HEIGHT: usize = 3;

    struct TestCircuit {
        root: Fp,
        path: Path<Fp, Poseidon<Fp, 2>, HEIGHT>,
        leaf: Fp,
    }

    impl Circuit<Fp> for TestCircuit {
        type Config = PathConfig<HEIGHT>;
        type FloorPlanner = floor_planner::V1;
        type Params = ();

        fn without_witnesses(&self) -> Self {
            todo!()
        }

        fn configure(meta: &mut ConstraintSystem<Fp>) -> Self::Config {
            let advices = [(); HEIGHT].map(|_| meta.advice_column());
            let utility_advices = [(); NUM_OF_UTILITY_ADVICE_COLUMNS].map(|_| meta.advice_column());
            let poseidon_advices = [(); 5].map(|_| meta.advice_column());

            for advice in &advices {
                meta.enable_equality(*advice);
            }

            for advice in &utility_advices {
                meta.enable_equality(*advice);
            }

            for advice in &poseidon_advices {
                meta.enable_equality(*advice);
            }

            let rc_a = [(); 3].map(|_| meta.fixed_column());
            let rc_b = [(); 3].map(|_| meta.fixed_column());

            let poseidon_config = PoseidonChip::configure::<poseidon::P128Pow5T3>(
                meta,
                poseidon_advices[1..5].try_into().unwrap(),
                poseidon_advices[0],
                rc_a,
                rc_b,
            );

            PathChip::<Poseidon<Fp, 2>, HEIGHT>::configure(
                meta,
                advices,
                utility_advices,
                poseidon_config,
            )
        }

        fn synthesize(
            &self,
            config: Self::Config,
            mut layouter: impl Layouter<Fp>,
        ) -> Result<(), plonk::Error> {
            let (root_cell, leaf_cell, one) = layouter.assign_region(
                || "test circuit",
                |mut region| {
                    let root_cell = region.assign_advice(
                        || "root",
                        config.advices[0],
                        0,
                        || Value::known(self.root),
                    )?;

                    let leaf_cell = region.assign_advice(
                        || "leaf",
                        config.advices[1],
                        0,
                        || Value::known(self.leaf),
                    )?;

                    let one = region.assign_advice(
                        || "one",
                        config.advices[2],
                        0,
                        || Value::known(Fp::ONE),
                    )?;
                    Ok((root_cell, leaf_cell, one))
                },
            )?;

            let path_chip =
                PathChip::from_native(config.clone(), &mut layouter, self.path.clone())?;

            let res = path_chip.check_membership(&mut layouter, root_cell, leaf_cell)?;

            let assert_eq_chip = config.assert_eq_chip();
            assert_eq_chip.assert_equal(&mut layouter, res, one)?;

            Ok(())
        }
    }

    #[test]
    fn test_smt_circuit() {
        let hasher = Poseidon::<Fp, 2>::hasher();
        let leaves: [Fp; HEIGHT] = [Fp::ZERO, Fp::ZERO, Fp::ZERO];
        let empty_leaf = [0u8; 64];

        let smt = SparseMerkleTree::<Fp, Poseidon<Fp, 2>, HEIGHT>::new_sequential(
            &leaves,
            &hasher.clone(),
            &empty_leaf,
        )
        .unwrap();

        let path = smt.generate_membership_proof(0);
        let root = path.calculate_root(&leaves[0], &hasher.clone()).unwrap();

        let _circuit = TestCircuit { root, path, leaf: leaves[0] };
    }
}
