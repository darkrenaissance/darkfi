use ff::PrimeFieldBits;
use halo2::{
    circuit::{Cell, Layouter, Region},
    plonk::{Advice, Column, Error, Expression},
};
use pasta_curves::arithmetic::FieldExt;
use std::{array, convert::TryInto, ops::Range};

// pub(crate) mod cond_swap;
// pub(crate) mod decompose_running_sum;
// pub(crate) mod lookup_range_check;

/// A variable representing a field element.
#[derive(Copy, Clone, Debug)]
pub struct CellValue<F: FieldExt> {
    cell: Cell,
    value: Option<F>,
}

pub trait Var<F: FieldExt>: Copy + Clone + std::fmt::Debug {
    fn new(cell: Cell, value: Option<F>) -> Self;
    fn cell(&self) -> Cell;
    fn value(&self) -> Option<F>;
}

impl<F: FieldExt> Var<F> for CellValue<F> {
    fn new(cell: Cell, value: Option<F>) -> Self {
        Self { cell, value }
    }

    fn cell(&self) -> Cell {
        self.cell
    }

    fn value(&self) -> Option<F> {
        self.value
    }
}

pub trait UtilitiesInstructions<F: FieldExt> {
    type Var: Var<F>;

    fn load_private(
        &self,
        mut layouter: impl Layouter<F>,
        column: Column<Advice>,
        value: Option<F>,
    ) -> Result<Self::Var, Error> {
        layouter.assign_region(
            || "load private",
            |mut region| {
                let cell = region.assign_advice(
                    || "load private",
                    column,
                    0,
                    || value.ok_or(Error::SynthesisError),
                )?;
                Ok(Var::new(cell, value))
            },
        )
    }
}

/// Assigns a cell at a specific offset within the given region, constraining it
/// to the same value as another cell (which may be in any region).
///
/// Returns an error if either `column` or `copy` is not in a column that was passed to
/// [`ConstraintSystem::enable_equality`] during circuit configuration.
///
/// [`ConstraintSystem::enable_equality`]: halo2::plonk::ConstraintSystem::enable_equality
pub fn copy<A, AR, F: FieldExt>(
    region: &mut Region<'_, F>,
    annotation: A,
    column: Column<Advice>,
    offset: usize,
    copy: &CellValue<F>,
) -> Result<CellValue<F>, Error>
where
    A: Fn() -> AR,
    AR: Into<String>,
{
    let cell = region.assign_advice(annotation, column, offset, || {
        copy.value.ok_or(Error::SynthesisError)
    })?;

    region.constrain_equal(cell, copy.cell)?;

    Ok(CellValue::new(cell, copy.value))
}

pub fn transpose_option_array<T: Copy + std::fmt::Debug, const LEN: usize>(
    option_array: Option<[T; LEN]>,
) -> [Option<T>; LEN] {
    let mut ret = [None; LEN];
    if let Some(arr) = option_array {
        for (entry, value) in ret.iter_mut().zip(array::IntoIter::new(arr)) {
            *entry = Some(value);
        }
    }
    ret
}

/// Checks that an expresssion is either 1 or 0.
pub fn bool_check<F: FieldExt>(value: Expression<F>) -> Expression<F> {
    value.clone() * (Expression::Constant(F::one()) - value)
}

/// Takes a specified subsequence of the little-endian bit representation of a field element.
/// The bits are numbered from 0 for the LSB.
pub fn bitrange_subset<F: FieldExt + PrimeFieldBits>(field_elem: F, bitrange: Range<usize>) -> F {
    assert!(bitrange.end <= F::NUM_BITS as usize);

    let bits: Vec<bool> = field_elem
        .to_le_bits()
        .iter()
        .by_val()
        .skip(bitrange.start)
        .take(bitrange.end - bitrange.start)
        .chain(std::iter::repeat(false))
        .take(256)
        .collect();
    let bytearray: Vec<u8> = bits
        .chunks_exact(8)
        .map(|byte| byte.iter().rev().fold(0u8, |acc, bit| acc * 2 + *bit as u8))
        .collect();

    F::from_bytes(&bytearray.try_into().unwrap()).unwrap()
}

/// Check that an expression is in the small range [0..range),
/// i.e. 0 ≤ word < range.
pub fn range_check<F: FieldExt>(word: Expression<F>, range: usize) -> Expression<F> {
    (1..range).fold(word.clone(), |acc, i| {
        acc * (word.clone() - Expression::Constant(F::from_u64(i as u64)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bigint::U256;
    use ff::PrimeField;
    use halo2::{
        circuit::{Layouter, SimpleFloorPlanner},
        dev::{MockProver, VerifyFailure},
        plonk::{Circuit, ConstraintSystem, Error, Selector},
        poly::Rotation,
    };
    use pasta_curves::pallas;

    #[test]
    fn test_range_check() {
        struct MyCircuit<const RANGE: usize>(u8);

        impl<const RANGE: usize> UtilitiesInstructions<pallas::Base> for MyCircuit<RANGE> {
            type Var = CellValue<pallas::Base>;
        }

        #[derive(Clone)]
        struct Config {
            selector: Selector,
            advice: Column<Advice>,
        }

        impl<const RANGE: usize> Circuit<pallas::Base> for MyCircuit<RANGE> {
            type Config = Config;
            type FloorPlanner = SimpleFloorPlanner;

            fn without_witnesses(&self) -> Self {
                MyCircuit(self.0)
            }

            fn configure(meta: &mut ConstraintSystem<pallas::Base>) -> Self::Config {
                let selector = meta.selector();
                let advice = meta.advice_column();

                meta.create_gate("range check", |meta| {
                    let selector = meta.query_selector(selector);
                    let advice = meta.query_advice(advice, Rotation::cur());

                    vec![selector * range_check(advice, RANGE)]
                });

                Config { selector, advice }
            }

            fn synthesize(
                &self,
                config: Self::Config,
                mut layouter: impl Layouter<pallas::Base>,
            ) -> Result<(), Error> {
                layouter.assign_region(
                    || "range constrain",
                    |mut region| {
                        config.selector.enable(&mut region, 0)?;
                        region.assign_advice(
                            || format!("witness {}", self.0),
                            config.advice,
                            0,
                            || Ok(pallas::Base::from_u64(self.0.into())),
                        )?;

                        Ok(())
                    },
                )
            }
        }

        for i in 0..8 {
            let circuit: MyCircuit<8> = MyCircuit(i);
            let prover = MockProver::<pallas::Base>::run(3, &circuit, vec![]).unwrap();
            assert_eq!(prover.verify(), Ok(()));
        }

        {
            let circuit: MyCircuit<8> = MyCircuit(8);
            let prover = MockProver::<pallas::Base>::run(3, &circuit, vec![]).unwrap();
            assert_eq!(
                prover.verify(),
                Err(vec![VerifyFailure::ConstraintNotSatisfied {
                    constraint: ((0, "range check").into(), 0, "").into(),
                    row: 0
                }])
            );
        }
    }

    #[test]
    fn test_bitrange_subset() {
        // Subset full range.
        {
            let field_elem = pallas::Base::rand();
            let bitrange = 0..(pallas::Base::NUM_BITS as usize);
            let subset = bitrange_subset(field_elem, bitrange);
            assert_eq!(field_elem, subset);
        }

        // Subset zero bits
        {
            let field_elem = pallas::Base::rand();
            let bitrange = 0..0;
            let subset = bitrange_subset(field_elem, bitrange);
            assert_eq!(pallas::Base::zero(), subset);
        }

        // Closure to decompose field element into pieces using consecutive ranges,
        // and check that we recover the original.
        let decompose = |field_elem: pallas::Base, ranges: &[Range<usize>]| {
            assert_eq!(
                ranges.iter().map(|range| range.len()).sum::<usize>(),
                pallas::Base::NUM_BITS as usize
            );
            assert_eq!(ranges[0].start, 0);
            assert_eq!(ranges.last().unwrap().end, pallas::Base::NUM_BITS as usize);

            // Check ranges are contiguous
            #[allow(unused_assignments)]
            {
                let mut ranges = ranges.iter();
                let mut range = ranges.next().unwrap();
                if let Some(next_range) = ranges.next() {
                    assert_eq!(range.end, next_range.start);
                    range = next_range;
                }
            }

            let subsets = ranges
                .iter()
                .map(|range| bitrange_subset(field_elem, range.clone()))
                .collect::<Vec<_>>();

            let mut sum = subsets[0];
            let mut num_bits = 0;
            for (idx, subset) in subsets.iter().skip(1).enumerate() {
                // 2^num_bits
                let range_shift: [u8; 32] = {
                    num_bits += ranges[idx].len();
                    let mut range_shift = [0u8; 32];
                    U256([2, 0, 0, 0])
                        .pow(U256([num_bits as u64, 0, 0, 0]))
                        .to_little_endian(&mut range_shift);
                    range_shift
                };
                sum += subset * pallas::Base::from_bytes(&range_shift).unwrap();
            }
            assert_eq!(field_elem, sum);
        };

        decompose(pallas::Base::rand(), &[0..255]);
        decompose(pallas::Base::rand(), &[0..1, 1..255]);
        decompose(pallas::Base::rand(), &[0..254, 254..255]);
        decompose(pallas::Base::rand(), &[0..127, 127..255]);
        decompose(pallas::Base::rand(), &[0..128, 128..255]);
        decompose(
            pallas::Base::rand(),
            &[0..50, 50..100, 100..150, 150..200, 200..255],
        );
    }
}
