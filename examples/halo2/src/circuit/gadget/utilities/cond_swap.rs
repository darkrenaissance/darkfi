use super::{copy, CellValue, UtilitiesInstructions, Var};
use halo2::{
    circuit::{Chip, Layouter},
    plonk::{Advice, Column, ConstraintSystem, Error, Expression, Selector},
    poly::Rotation,
};
use pasta_curves::arithmetic::FieldExt;
use std::{array, marker::PhantomData};

pub trait CondSwapInstructions<F: FieldExt>: UtilitiesInstructions<F> {
    #[allow(clippy::type_complexity)]
    /// Given an input pair (a,b) and a `swap` boolean flag, returns
    /// (b,a) if `swap` is set, else (a,b) if `swap` is not set.
    ///
    /// The second element of the pair is required to be a witnessed
    /// value, not a variable that already exists in the circuit.
    fn swap(
        &self,
        layouter: impl Layouter<F>,
        pair: (Self::Var, Option<F>),
        swap: Option<bool>,
    ) -> Result<(Self::Var, Self::Var), Error>;
}

/// A chip implementing a conditional swap.
#[derive(Clone, Debug)]
pub struct CondSwapChip<F> {
    config: CondSwapConfig,
    _marker: PhantomData<F>,
}

impl<F: FieldExt> Chip<F> for CondSwapChip<F> {
    type Config = CondSwapConfig;
    type Loaded = ();

    fn config(&self) -> &Self::Config {
        &self.config
    }

    fn loaded(&self) -> &Self::Loaded {
        &()
    }
}

#[derive(Clone, Debug)]
pub struct CondSwapConfig {
    pub q_swap: Selector,
    pub a: Column<Advice>,
    pub b: Column<Advice>,
    pub a_swapped: Column<Advice>,
    pub b_swapped: Column<Advice>,
    pub swap: Column<Advice>,
}

impl<F: FieldExt> UtilitiesInstructions<F> for CondSwapChip<F> {
    type Var = CellValue<F>;
}

impl<F: FieldExt> CondSwapInstructions<F> for CondSwapChip<F> {
    #[allow(clippy::type_complexity)]
    fn swap(
        &self,
        mut layouter: impl Layouter<F>,
        pair: (Self::Var, Option<F>),
        swap: Option<bool>,
    ) -> Result<(Self::Var, Self::Var), Error> {
        let config = self.config();

        layouter.assign_region(
            || "swap",
            |mut region| {
                // Enable `q_swap` selector
                config.q_swap.enable(&mut region, 0)?;

                // Copy in `a` value
                let a = copy(&mut region, || "copy a", config.a, 0, &pair.0)?;

                // Witness `b` value
                let b = {
                    let cell = region.assign_advice(
                        || "witness b",
                        config.b,
                        0,
                        || pair.1.ok_or(Error::SynthesisError),
                    )?;
                    CellValue::new(cell, pair.1)
                };

                // Witness `swap` value
                let swap_val = swap.map(|swap| F::from_u64(swap as u64));
                region.assign_advice(
                    || "swap",
                    config.swap,
                    0,
                    || swap_val.ok_or(Error::SynthesisError),
                )?;

                // Conditionally swap a
                let a_swapped = {
                    let a_swapped = a
                        .value
                        .zip(b.value)
                        .zip(swap)
                        .map(|((a, b), swap)| if swap { b } else { a });
                    let a_swapped_cell = region.assign_advice(
                        || "a_swapped",
                        config.a_swapped,
                        0,
                        || a_swapped.ok_or(Error::SynthesisError),
                    )?;
                    CellValue {
                        cell: a_swapped_cell,
                        value: a_swapped,
                    }
                };

                // Conditionally swap b
                let b_swapped = {
                    let b_swapped = a
                        .value
                        .zip(b.value)
                        .zip(swap)
                        .map(|((a, b), swap)| if swap { a } else { b });
                    let b_swapped_cell = region.assign_advice(
                        || "b_swapped",
                        config.b_swapped,
                        0,
                        || b_swapped.ok_or(Error::SynthesisError),
                    )?;
                    CellValue {
                        cell: b_swapped_cell,
                        value: b_swapped,
                    }
                };

                // Return swapped pair
                Ok((a_swapped, b_swapped))
            },
        )
    }
}

impl<F: FieldExt> CondSwapChip<F> {
    /// Configures this chip for use in a circuit.
    ///
    /// # Side-effects
    ///
    /// `advices[0]` will be equality-enabled.
    pub fn configure(
        meta: &mut ConstraintSystem<F>,
        advices: [Column<Advice>; 5],
    ) -> CondSwapConfig {
        let a = advices[0];
        // Only column a is used in an equality constraint directly by this chip.
        meta.enable_equality(a.into());

        let q_swap = meta.selector();

        let config = CondSwapConfig {
            q_swap,
            a,
            b: advices[1],
            a_swapped: advices[2],
            b_swapped: advices[3],
            swap: advices[4],
        };

        // TODO: optimise shape of gate for Merkle path validation

        meta.create_gate("a' = b ⋅ swap + a ⋅ (1-swap)", |meta| {
            let q_swap = meta.query_selector(q_swap);

            let a = meta.query_advice(config.a, Rotation::cur());
            let b = meta.query_advice(config.b, Rotation::cur());
            let a_swapped = meta.query_advice(config.a_swapped, Rotation::cur());
            let b_swapped = meta.query_advice(config.b_swapped, Rotation::cur());
            let swap = meta.query_advice(config.swap, Rotation::cur());

            let one = Expression::Constant(F::one());

            // a_swapped - b ⋅ swap - a ⋅ (1-swap) = 0
            // This checks that `a_swapped` is equal to `y` when `swap` is set,
            // but remains as `a` when `swap` is not set.
            let a_check =
                a_swapped - b.clone() * swap.clone() - a.clone() * (one.clone() - swap.clone());

            // b_swapped - a ⋅ swap - b ⋅ (1-swap) = 0
            // This checks that `b_swapped` is equal to `a` when `swap` is set,
            // but remains as `b` when `swap` is not set.
            let b_check = b_swapped - a * swap.clone() - b * (one.clone() - swap.clone());

            // Check `swap` is boolean.
            let bool_check = swap.clone() * (one - swap);

            array::IntoIter::new([a_check, b_check, bool_check])
                .map(move |poly| q_swap.clone() * poly)
        });

        config
    }

    pub fn construct(config: CondSwapConfig) -> Self {
        CondSwapChip {
            config,
            _marker: PhantomData,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::UtilitiesInstructions;
    use super::{CondSwapChip, CondSwapConfig, CondSwapInstructions};
    use halo2::{
        circuit::{Layouter, SimpleFloorPlanner},
        dev::MockProver,
        plonk::{Circuit, ConstraintSystem, Error},
    };
    use pasta_curves::{arithmetic::FieldExt, pallas::Base};

    #[test]
    fn cond_swap() {
        #[derive(Default)]
        struct MyCircuit<F: FieldExt> {
            a: Option<F>,
            b: Option<F>,
            swap: Option<bool>,
        }

        impl<F: FieldExt> Circuit<F> for MyCircuit<F> {
            type Config = CondSwapConfig;
            type FloorPlanner = SimpleFloorPlanner;

            fn without_witnesses(&self) -> Self {
                Self::default()
            }

            fn configure(meta: &mut ConstraintSystem<F>) -> Self::Config {
                let advices = [
                    meta.advice_column(),
                    meta.advice_column(),
                    meta.advice_column(),
                    meta.advice_column(),
                    meta.advice_column(),
                ];

                CondSwapChip::<F>::configure(meta, advices)
            }

            fn synthesize(
                &self,
                config: Self::Config,
                mut layouter: impl Layouter<F>,
            ) -> Result<(), Error> {
                let chip = CondSwapChip::<F>::construct(config.clone());

                // Load the pair and the swap flag into the circuit.
                let a = chip.load_private(layouter.namespace(|| "a"), config.a, self.a)?;
                // Return the swapped pair.
                let swapped_pair =
                    chip.swap(layouter.namespace(|| "swap"), (a, self.b), self.swap)?;

                if let Some(swap) = self.swap {
                    if swap {
                        // Check that `a` and `b` have been swapped
                        assert_eq!(swapped_pair.0.value.unwrap(), self.b.unwrap());
                        assert_eq!(swapped_pair.1.value.unwrap(), a.value.unwrap());
                    } else {
                        // Check that `a` and `b` have not been swapped
                        assert_eq!(swapped_pair.0.value.unwrap(), a.value.unwrap());
                        assert_eq!(swapped_pair.1.value.unwrap(), self.b.unwrap());
                    }
                }

                Ok(())
            }
        }

        // Test swap case
        {
            let circuit: MyCircuit<Base> = MyCircuit {
                a: Some(Base::rand()),
                b: Some(Base::rand()),
                swap: Some(true),
            };
            let prover = MockProver::<Base>::run(3, &circuit, vec![]).unwrap();
            assert_eq!(prover.verify(), Ok(()));
        }

        // Test non-swap case
        {
            let circuit: MyCircuit<Base> = MyCircuit {
                a: Some(Base::rand()),
                b: Some(Base::rand()),
                swap: Some(false),
            };
            let prover = MockProver::<Base>::run(3, &circuit, vec![]).unwrap();
            assert_eq!(prover.verify(), Ok(()));
        }
    }
}
