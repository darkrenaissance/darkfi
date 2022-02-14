use halo2_proofs::{
    circuit::{AssignedCell, Chip, Layouter},
    plonk::{Advice, Column, ConstraintSystem, Error, Selector},
    poly::Rotation,
};
use pasta_curves::pallas;

type Variable = AssignedCell<pallas::Base, pallas::Base>;

// Replace with use pasta::Fp and pasta::Fq
type Fp = pallas::Base;
//type Fq = pallas::Scalar;

#[derive(Clone, Debug)]
pub struct ArithmeticChipConfig {
    a_col: Column<Advice>,
    b_col: Column<Advice>,
    //permute: Permutation,
    s_add: Selector,
    s_mul: Selector,
    s_sub: Selector,
    //s_pub: Selector,
}

pub struct ArithmeticChip {
    config: ArithmeticChipConfig,
}

impl Chip<Fp> for ArithmeticChip {
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

    pub fn configure(cs: &mut ConstraintSystem<Fp>) -> ArithmeticChipConfig {
        let a_col = cs.advice_column();
        let b_col = cs.advice_column();

        cs.enable_equality(a_col);
        cs.enable_equality(b_col);

        //let instance = cs.instance_column();

        /*let permute = {
            // Convert advice columns into an "any" columns.
            let cols: [Column<Any>; 2] = [a_col.into(), b_col.into()];
            Permutation::new(cs, &cols)
        };*/

        let s_add = cs.selector();
        let s_mul = cs.selector();
        let s_sub = cs.selector();
        //let s_pub = cs.selector();

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

        /*
        cs.create_gate("pub", |cs| {
            let a = cs.query_advice(a_col, Rotation::cur());
            let p = cs.query_instance(instance, Rotation::cur());
            let s_pub = cs.query_selector(s_pub);

            vec![s_pub * (p - a)]
        });
        */

        ArithmeticChipConfig {
            a_col,
            b_col,
            /* permute, */ s_add,
            s_mul,
            s_sub, /* , s_pub */
        }
    }

    pub fn add(
        &self,
        mut layouter: impl Layouter<Fp>,
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
        mut layouter: impl Layouter<Fp>,
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
        mut layouter: impl Layouter<Fp>,
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

    /*
    fn expose_public(&self, layouter: &mut impl Layouter<Fp>, num: Number) -> Result<(), Error> {
        layouter.assign_region(
            || "expose public",
            |mut region| {
                self.config.s_pub.enable(&mut region, 0)?;

                let out = region.assign_advice(
                    || "public advice",
                    self.config.b_col,
                    0,
                    || num.value.ok_or(Error::SynthesisError),
                )?;
                region.constrain_equal(num.cell, out)?;

                Ok(())
            },
        )
    }
    */
}
