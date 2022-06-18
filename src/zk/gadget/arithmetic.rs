use halo2_proofs::{
    arithmetic::FieldExt,
    circuit::{AssignedCell, Chip, Layouter},
    plonk,
    plonk::{Advice, Column, ConstraintSystem, Constraints, Selector},
    poly::Rotation,
};
use pasta_curves::pallas;

pub trait ArithInstruction<F: FieldExt>: Chip<F> {
    fn add(
        &self,
        layouter: impl Layouter<F>,
        a: &AssignedCell<F, F>,
        b: &AssignedCell<F, F>,
    ) -> Result<AssignedCell<F, F>, plonk::Error>;

    fn sub(
        &self,
        layouter: impl Layouter<F>,
        a: &AssignedCell<F, F>,
        b: &AssignedCell<F, F>,
    ) -> Result<AssignedCell<F, F>, plonk::Error>;

    fn mul(
        &self,
        layouter: impl Layouter<F>,
        a: &AssignedCell<F, F>,
        b: &AssignedCell<F, F>,
    ) -> Result<AssignedCell<F, F>, plonk::Error>;
}

#[derive(Clone, Debug)]
pub struct ArithConfig {
    a: Column<Advice>,
    b: Column<Advice>,
    c: Column<Advice>,
    q_add: Selector,
    q_sub: Selector,
    q_mul: Selector,
}

pub struct ArithChip {
    config: ArithConfig,
}

impl Chip<pallas::Base> for ArithChip {
    type Config = ArithConfig;
    type Loaded = ();

    fn config(&self) -> &Self::Config {
        &self.config
    }

    fn loaded(&self) -> &Self::Loaded {
        &()
    }
}

impl ArithChip {
    pub fn configure(
        meta: &mut ConstraintSystem<pallas::Base>,
        a: Column<Advice>,
        b: Column<Advice>,
        c: Column<Advice>,
    ) -> ArithConfig {
        let q_add = meta.selector();
        let q_sub = meta.selector();
        let q_mul = meta.selector();

        meta.create_gate("Field element addition: c = a + b", |meta| {
            let q_add = meta.query_selector(q_add);
            let a = meta.query_advice(a, Rotation::cur());
            let b = meta.query_advice(b, Rotation::cur());
            let c = meta.query_advice(c, Rotation::cur());

            Constraints::with_selector(q_add, Some(a + b - c))
        });

        meta.create_gate("Field element substitution: c = a - b", |meta| {
            let q_sub = meta.query_selector(q_sub);
            let a = meta.query_advice(a, Rotation::cur());
            let b = meta.query_advice(b, Rotation::cur());
            let c = meta.query_advice(c, Rotation::cur());

            Constraints::with_selector(q_sub, Some(a - b - c))
        });

        meta.create_gate("Field element multiplication: c = a * b", |meta| {
            let q_mul = meta.query_selector(q_mul);
            let a = meta.query_advice(a, Rotation::cur());
            let b = meta.query_advice(b, Rotation::cur());
            let c = meta.query_advice(c, Rotation::cur());

            Constraints::with_selector(q_mul, Some(a * b - c))
        });

        ArithConfig { a, b, c, q_add, q_sub, q_mul }
    }

    pub fn construct(config: ArithConfig) -> Self {
        Self { config }
    }
}

impl ArithInstruction<pallas::Base> for ArithChip {
    fn add(
        &self,
        mut layouter: impl Layouter<pallas::Base>,
        a: &AssignedCell<pallas::Base, pallas::Base>,
        b: &AssignedCell<pallas::Base, pallas::Base>,
    ) -> Result<AssignedCell<pallas::Base, pallas::Base>, plonk::Error> {
        layouter.assign_region(
            || "c = a + b",
            |mut region| {
                self.config.q_add.enable(&mut region, 0)?;

                a.copy_advice(|| "copy a", &mut region, self.config.a, 0)?;
                b.copy_advice(|| "copy b", &mut region, self.config.b, 0)?;

                let scalar_val = a.value().zip(b.value()).map(|(a, b)| a + b);
                region.assign_advice(
                    || "c",
                    self.config.c,
                    0,
                    || scalar_val.ok_or(plonk::Error::Synthesis),
                )
            },
        )
    }

    fn sub(
        &self,
        mut layouter: impl Layouter<pallas::Base>,
        a: &AssignedCell<pallas::Base, pallas::Base>,
        b: &AssignedCell<pallas::Base, pallas::Base>,
    ) -> Result<AssignedCell<pallas::Base, pallas::Base>, plonk::Error> {
        layouter.assign_region(
            || "c = a - b",
            |mut region| {
                self.config.q_sub.enable(&mut region, 0)?;

                a.copy_advice(|| "copy a", &mut region, self.config.a, 0)?;
                b.copy_advice(|| "copy b", &mut region, self.config.b, 0)?;

                let scalar_val = a.value().zip(b.value()).map(|(a, b)| a - b);
                region.assign_advice(
                    || "c",
                    self.config.c,
                    0,
                    || scalar_val.ok_or(plonk::Error::Synthesis),
                )
            },
        )
    }

    fn mul(
        &self,
        mut layouter: impl Layouter<pallas::Base>,
        a: &AssignedCell<pallas::Base, pallas::Base>,
        b: &AssignedCell<pallas::Base, pallas::Base>,
    ) -> Result<AssignedCell<pallas::Base, pallas::Base>, plonk::Error> {
        layouter.assign_region(
            || "c = a * b",
            |mut region| {
                self.config.q_mul.enable(&mut region, 0)?;

                a.copy_advice(|| "copy a", &mut region, self.config.a, 0)?;
                b.copy_advice(|| "copy b", &mut region, self.config.b, 0)?;

                let scalar_val = a.value().zip(b.value()).map(|(a, b)| a * b);
                region.assign_advice(
                    || "c",
                    self.config.c,
                    0,
                    || scalar_val.ok_or(plonk::Error::Synthesis),
                )
            },
        )
    }
}
