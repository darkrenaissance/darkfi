use halo2_proofs::{
    circuit::{AssignedCell, Chip, Layouter},
    plonk::{Advice, Column, ConstraintSystem, Error, Selector},
    poly::Rotation,
};
use pasta_curves::pallas;

type Variable = AssignedCell<pallas::Base, pallas::Base>;

#[derive(Clone, Debug)]
pub struct ArithmeticChipConfig {
    a_col: Column<Advice>,
    b_col: Column<Advice>,
    s_add: Selector,
    s_mul: Selector,
    s_sub: Selector,
}

pub struct ArithmeticChip {
    config: ArithmeticChipConfig,
}

impl Chip<pallas::Base> for ArithmeticChip {
    type Config = ArithmeticChipConfig;
    type Loaded = ();

    fn config(&self) -> &Self::Config {
        &self.config
    }

    fn loaded(&self) -> &Self::Loaded {
        &()
    }
}

impl ArithmeticChip {
    pub fn construct(config: ArithmeticChipConfig) -> Self {
        Self { config }
    }

    pub fn configure(cs: &mut ConstraintSystem<pallas::Base>) -> ArithmeticChipConfig {
        let a_col = cs.advice_column();
        let b_col = cs.advice_column();

        cs.enable_equality(a_col);
        cs.enable_equality(b_col);

        let s_add = cs.selector();
        let s_mul = cs.selector();
        let s_sub = cs.selector();

        cs.create_gate("add", |cs| {
            let lhs = cs.query_advice(a_col, Rotation::cur());
            let rhs = cs.query_advice(b_col, Rotation::cur());
            let out = cs.query_advice(a_col, Rotation::next());
            let s_add = cs.query_selector(s_add);

            vec![s_add * (lhs + rhs - out)]
        });

        cs.create_gate("mul", |cs| {
            let lhs = cs.query_advice(a_col, Rotation::cur());
            let rhs = cs.query_advice(b_col, Rotation::cur());
            let out = cs.query_advice(a_col, Rotation::next());
            let s_mul = cs.query_selector(s_mul);

            vec![s_mul * (lhs * rhs - out)]
        });

        cs.create_gate("sub", |cs| {
            let lhs = cs.query_advice(a_col, Rotation::cur());
            let rhs = cs.query_advice(b_col, Rotation::cur());
            let out = cs.query_advice(a_col, Rotation::next());
            let s_sub = cs.query_selector(s_sub);

            vec![s_sub * (lhs - rhs - out)]
        });

        ArithmeticChipConfig { a_col, b_col, s_add, s_mul, s_sub }
    }

    pub fn add(
        &self,
        mut layouter: impl Layouter<pallas::Base>,
        a: Variable,
        b: Variable,
    ) -> Result<Variable, Error> {
        let mut out = None;
        layouter.assign_region(
            || "mul",
            |mut region| {
                self.config.s_add.enable(&mut region, 0)?;

                let lhs = region.assign_advice(
                    || "lhs",
                    self.config.a_col,
                    0,
                    || Ok(*a.value().ok_or(Error::Synthesis)?),
                )?;

                let rhs = region.assign_advice(
                    || "rhs",
                    self.config.b_col,
                    0,
                    || Ok(*b.value().ok_or(Error::Synthesis)?),
                )?;

                region.constrain_equal(a.cell(), lhs.cell())?;
                region.constrain_equal(b.cell(), rhs.cell())?;

                let value = a.value().and_then(|a| b.value().map(|b| a + b));

                let cell = region.assign_advice(
                    || "lhs + rhs",
                    self.config.a_col,
                    1,
                    || value.ok_or(Error::Synthesis),
                )?;

                out = Some(cell);
                Ok(())
            },
        )?;

        Ok(out.unwrap())
    }

    pub fn mul(
        &self,
        mut layouter: impl Layouter<pallas::Base>,
        a: Variable,
        b: Variable,
    ) -> Result<Variable, Error> {
        let mut out = None;

        layouter.assign_region(
            || "mul",
            |mut region| {
                self.config.s_mul.enable(&mut region, 0)?;

                let lhs = region.assign_advice(
                    || "lhs",
                    self.config.a_col,
                    0,
                    || Ok(*a.value().ok_or(Error::Synthesis)?),
                )?;

                let rhs = region.assign_advice(
                    || "rhs",
                    self.config.b_col,
                    0,
                    || Ok(*b.value().ok_or(Error::Synthesis)?),
                )?;

                region.constrain_equal(a.cell(), lhs.cell())?;
                region.constrain_equal(b.cell(), rhs.cell())?;

                let value = a.value().and_then(|a| b.value().map(|b| a * b));
                let cell = region.assign_advice(
                    || "lhs * rhs",
                    self.config.a_col,
                    1,
                    || value.ok_or(Error::Synthesis),
                )?;

                out = Some(cell);
                Ok(())
            },
        )?;

        Ok(out.unwrap())
    }

    pub fn sub(
        &self,
        mut layouter: impl Layouter<pallas::Base>,
        a: Variable,
        b: Variable,
    ) -> Result<Variable, Error> {
        let mut out = None;

        layouter.assign_region(
            || "sub",
            |mut region| {
                self.config.s_sub.enable(&mut region, 0)?;

                let lhs = region.assign_advice(
                    || "lhs",
                    self.config.a_col,
                    0,
                    || Ok(*a.value().ok_or(Error::Synthesis)?),
                )?;

                let rhs = region.assign_advice(
                    || "rhs",
                    self.config.b_col,
                    0,
                    || Ok(*b.value().ok_or(Error::Synthesis)?),
                )?;

                region.constrain_equal(a.cell(), lhs.cell())?;
                region.constrain_equal(b.cell(), rhs.cell())?;

                let value = a.value().and_then(|a| b.value().map(|b| a - b));
                let cell = region.assign_advice(
                    || "lhs * rhs",
                    self.config.a_col,
                    1,
                    || value.ok_or(Error::Synthesis),
                )?;

                out = Some(cell);
                Ok(())
            },
        )?;

        Ok(out.unwrap())
    }
}
