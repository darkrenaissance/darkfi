use std::marker::PhantomData;

use halo2_proofs::{
    arithmetic::FieldExt,
    circuit::{AssignedCell, Chip, Layouter, Region},
    plonk::{Advice, Column, ConstraintSystem, Error, Expression, Instance, Selector},
    poly::Rotation,
};

use pasta_curves::pallas;

#[derive(Clone, Debug)]
pub struct GreaterThanConfig {
    pub advice: [Column<Advice>; 2],
    pub instance: Column<Instance>,
    s_gt: Selector,
}

pub struct GreaterThanChip<F: FieldExt, const WORD_BITS: u32> {
    config: GreaterThanConfig,
    _marker: PhantomData<F>,
}

impl<F: FieldExt, const WORD_BITS: u32> Chip<F> for GreaterThanChip<F, WORD_BITS> {
    type Config = GreaterThanConfig;
    type Loaded = ();

    fn config(&self) -> &Self::Config {
        &self.config
    }

    fn loaded(&self) -> &Self::Loaded {
        &()
    }
}

impl<F: FieldExt, const WORD_BITS: u32> GreaterThanChip<F, WORD_BITS> {
    pub fn construct(config: <Self as Chip<F>>::Config) -> Self {
        Self { config, _marker: PhantomData }
    }

    /*
    pub fn configure(meta: &mut ConstraintSystem<F>) -> <Self as Chip<F>>::Config {
        //let constant = meta.fixed_column();
        //meta.enable_constant(constant);

        let advice = [meta.advice_column(), meta.advice_column()];

        for column in &advice {
            meta.enable_equality(*column);
        }

        let s_gt = meta.selector();

        meta.create_gate("greater than", |meta| {
            let lhs = meta.query_advice(advice[0], Rotation::cur());
            let rhs = meta.query_advice(advice[1], Rotation::cur());

            // This value is `lhs - rhs` if `lhs !> rhs` and `2^W - (lhs - rhs)` if `lhs > rhs`
            let helper = meta.query_advice(advice[0], Rotation::next());

            let is_greater = meta.query_advice(advice[1], Rotation::next());
            let s_gt = meta.query_selector(s_gt);

            vec![
                s_gt * (lhs - rhs + helper -
                    Expression::Constant(F::from(2_u64.pow(WORD_BITS))) * is_greater),
            ]
        });

        GreaterThanConfig { advice, s_gt }
    }

    pub fn configure(meta: &mut ConstraintSystem<F>) -> <Self as Chip<F>>::Config {
        let advice = [meta.advice_column(), meta.advice_column()];


        let s_gt = meta.selector();

        meta.create_gate("greater than", |meta| {
            let lhs = meta.query_advice(advice[0], Rotation::cur());
            let rhs = meta.query_advice(advice[1], Rotation::cur());

            // This value is `lhs - rhs` if `lhs !> rhs` and `2^W - (lhs - rhs)` if `lhs > rhs`
            let helper = meta.query_advice(advice[0], Rotation::next());

            let is_greater = meta.query_advice(advice[1], Rotation::next());
            let s_gt = meta.query_selector(s_gt);

            vec![
                s_gt * (lhs - rhs + helper -
                    Expression::Constant(F::from(2_u64.pow(WORD_BITS))) * is_greater),
            ]
        });

        GreaterThanConfig { advice, s_gt }
    }
     */
    pub fn configure(
        meta: &mut ConstraintSystem<F>,
        advice: [Column<Advice>; 2],
        instance: Column<Instance>,
    ) -> <Self as Chip<F>>::Config {
        for column in &advice {
            meta.enable_equality(*column);
        }

        let s_gt = meta.selector();

        meta.create_gate("greater than", |meta| {
            let lhs = meta.query_advice(advice[0], Rotation::cur());
            let rhs = meta.query_advice(advice[1], Rotation::cur());

            // This value is `lhs - rhs` if `lhs !> rhs` and `2^W - (lhs - rhs)` if `lhs > rhs`
            let helper = meta.query_advice(advice[0], Rotation::next());

            let is_greater = meta.query_advice(advice[1], Rotation::next());
            let s_gt = meta.query_selector(s_gt);

            vec![
                s_gt * (lhs - rhs + helper -
                    Expression::Constant(F::from(2_u64.pow(WORD_BITS))) * is_greater),
            ]
        });

        GreaterThanConfig { advice, instance, s_gt }
    }
}

pub trait GreaterThanInstruction<F: FieldExt>: Chip<F> {
    type Word;

    fn greater_than(
        &self,
        layouter: impl Layouter<F>,
        a: Self::Word,
        b: Self::Word,
    ) -> Result<(Self::Word, Self::Word), Error>;
}

#[derive(Clone, Debug)]
pub struct Word<F: FieldExt>(pub AssignedCell<F, F>);

impl From<AssignedCell<pallas::Base, pallas::Base>> for Word<pallas::Base> {
    fn from(cell: AssignedCell<pallas::Base, pallas::Base>) -> Self {
        Self(cell)
    }
}

impl<const WORD_BITS: u32> GreaterThanInstruction<pallas::Base>
    for GreaterThanChip<pallas::Base, WORD_BITS>
{
    type Word = Word<pallas::Base>;

    fn greater_than(
        &self,
        mut layouter: impl Layouter<pallas::Base>,
        a: Self::Word,
        b: Self::Word,
    ) -> Result<(Self::Word, Self::Word), Error> {
        let config = self.config();

        layouter.assign_region(
            || "greater than",
            |mut region: Region<'_, pallas::Base>| {
                config.s_gt.enable(&mut region, 0)?;

                a.0.copy_advice(|| "lhs", &mut region, config.advice[0], 0)?;
                b.0.copy_advice(|| "rhs", &mut region, config.advice[1], 0)?;

                let helper_cell = region
                    .assign_advice(
                        || "max minus diff",
                        config.advice[0],
                        1,
                        || {
                            let is_greater = a.0.value().unwrap().get_lower_128() >
                                b.0.value().unwrap().get_lower_128();
                            a.0.value()
                                .and_then(|a| {
                                    b.0.value().map(|b| {
                                        let x = *a - *b;

                                        (if is_greater {
                                            pallas::Base::from(2_u64.pow(WORD_BITS))
                                        } else {
                                            pallas::Base::zero()
                                        }) - x
                                    })
                                })
                                .ok_or(Error::Synthesis)
                        },
                    )
                    .map(Word)?;

                let is_greater_cell = region
                    .assign_advice(
                        || "is greater",
                        config.advice[1],
                        1,
                        || {
                            let is_greater = a.0.value().unwrap().get_lower_128() >
                                b.0.value().unwrap().get_lower_128();
                            Ok(if is_greater { pallas::Base::one() } else { pallas::Base::zero() })
                        },
                    )
                    .map(Word)?;
                Ok((helper_cell, is_greater_cell))
            },
        )
    }
}
