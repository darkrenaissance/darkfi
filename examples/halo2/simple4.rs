use halo2::{
    circuit::{SimpleFloorPlanner, Cell, Chip, Layouter},
    pasta::{EqAffine, Fp},
    plonk::{Advice, Any, Circuit, Column, ConstraintSystem, Error, Expression, Selector, create_proof, verify_proof, keygen_vk, keygen_pk, Permutation},
    poly::{commitment::{Blind, Params}, Rotation},
    transcript::{Blake2bRead, Blake2bWrite, Challenge255},
};
use group::Curve;
use std::time::Instant;

#[derive(Clone, Debug)]
struct CoolConfig {
    a_col: Column<Advice>,
    b_col: Column<Advice>,
    permute: Permutation,
    s_range: Selector,
    s_mul: Selector,
    s_pub: Selector,
}

struct CoolChip {
    config: CoolConfig
}

impl Chip<Fp> for CoolChip {
    type Config = CoolConfig;
    type Loaded = ();

    fn config(&self) -> &Self::Config {
        &self.config
    }

    fn loaded(&self) -> &Self::Loaded {
        &()
    }
}

#[derive(Clone, Debug)]
struct Number {
    cell: Cell,
    value: Option<Fp>,
}

impl CoolChip {
    fn construct(config: CoolConfig) -> Self {
        Self { config }
    }

    fn configure(cs: &mut ConstraintSystem<Fp>) -> CoolConfig {
        let a_col = cs.advice_column();
        let b_col = cs.advice_column();

        let instance = cs.instance_column();

        let permute = {
            // Convert advice columns into an "any" columns.
            let cols: [Column<Any>; 2] = [a_col.into(), b_col.into()];
            Permutation::new(cs, &cols)
        };

        let s_range = cs.selector();
        let s_mul = cs.selector();
        let s_pub = cs.selector();

        cs.create_gate("check", |cs| {
            let a = cs.query_advice(a_col, Rotation::cur());
            let s_range = cs.query_selector(s_range);
            vec![s_range * (a - Expression::Constant(Fp::from(2)))]
        });

        cs.create_gate("mul", |cs| {
            let lhs = cs.query_advice(a_col, Rotation::cur());
            let rhs = cs.query_advice(b_col, Rotation::cur());
            let out = cs.query_advice(a_col, Rotation::next());
            let s_mul = cs.query_selector(s_mul);

            vec![s_mul * (lhs * rhs + out * -Fp::one())]
        });

        cs.create_gate("public input", |cs| {
            let a = cs.query_advice(b_col, Rotation::cur());
            let p = cs.query_instance(instance, Rotation::cur());
            let s = cs.query_selector(s_pub);

            vec![s * (p + a * -Fp::one())]
        });

        CoolConfig { a_col, b_col, permute, s_range, s_mul, s_pub }
    }

    fn alloc_left(
        &self,
        layouter: &mut impl Layouter<Fp>,
        value: Option<Fp>
    ) -> Result<Number, Error> {
        layouter.assign_region(
            || "load left private input",
            |mut region| {
                let cell = region.assign_advice(
                    || "private input 'a'",
                    self.config.a_col,
                    0,
                    || value.ok_or(Error::SynthesisError),
                )?;
                Ok(Number { cell, value })
            }
        )
    }

    fn check(
        &self,
        layouter: &mut impl Layouter<Fp>,
        number: Number
    ) -> Result<(), Error> {
        layouter.assign_region(
            || "load private inputs",
            |mut region| {
                self.config.s_range.enable(&mut region, 0)?;

                let a = region.assign_advice(
                    || "lhs",
                    self.config.a_col,
                    0,
                    || number.value.ok_or(Error::SynthesisError),
                )?;
                region.constrain_equal(&self.config.permute, number.cell, a)?;

                Ok(())
            },
        )
    }

    fn mul(
        &self,
        layouter: &mut impl Layouter<Fp>,
        a: Number,
        b: Number
    ) -> Result<Number, Error> {
        let mut out = None;
        layouter.assign_region(
            || "mul",
            |mut region| {
                self.config.s_mul.enable(&mut region, 0)?;

                let lhs = region.assign_advice(
                    || "lhs",
                    self.config.a_col,
                    0,
                    || a.value.ok_or(Error::SynthesisError),
                )?;
                let rhs = region.assign_advice(
                    || "rhs",
                    self.config.b_col,
                    0,
                    || b.value.ok_or(Error::SynthesisError),
                )?;
                region.constrain_equal(&self.config.permute, a.cell, lhs)?;
                region.constrain_equal(&self.config.permute, b.cell, rhs)?;

                let value = a.value.and_then(|a| b.value.map(|b| a * b));
                let cell = region.assign_advice(
                    || "lhs * rhs",
                    self.config.a_col,
                    1,
                    || value.ok_or(Error::SynthesisError),
                )?;

                out = Some(Number { cell, value });
                Ok(())
            },
        )?;

        Ok(out.unwrap())
    }

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
                region.constrain_equal(&self.config.permute, num.cell, out)?;

                Ok(())
            },
        )
    }
}

#[derive(Clone)]
struct CoolCircuit {
    // Private input.
    a: Option<Fp>,
}

impl Circuit<Fp> for CoolCircuit {
    type Config = CoolConfig;
    type FloorPlanner = SimpleFloorPlanner;

    fn without_witnesses(&self) -> Self {
        Self { a: None }
    }

    fn configure(cs: &mut ConstraintSystem<Fp>) -> Self::Config {
        CoolChip::configure(cs)
    }

    fn synthesize(&self, config: Self::Config, mut layouter: impl Layouter<Fp>) -> Result<(), Error> {
        let chip = CoolChip::construct(config);
        let a = chip.alloc_left(&mut layouter, self.a)?;
        chip.check(&mut layouter, a.clone())?;
        let a2 = chip.mul(&mut layouter, a.clone(), a)?;
        chip.expose_public(&mut layouter, a2)?;
        Ok(())
    }
}

fn main() {
    let k = 6;

    let start = Instant::now();
    let params: Params<EqAffine> = Params::new(k);

    let empty_circuit = CoolCircuit { a: None };
    let vk = keygen_vk(&params, &empty_circuit).expect("keygen_vk should not fail");
    let pk = keygen_pk(&params, vk, &empty_circuit).expect("keygen_pk should not fail");
    println!("Setup: [{:?}]", start.elapsed());

    let start = Instant::now();
    let circuit = CoolCircuit {
        a: Some(Fp::from(2)),
    };

    let mut public_inputs = pk.get_vk().get_domain().empty_lagrange();
    public_inputs[4] = Fp::from(4);

    // Create a proof
    let mut transcript = Blake2bWrite::<_, _, Challenge255<_>>::init(vec![]);
    create_proof(&params, &pk, &[circuit], &[&[public_inputs.clone()]], &mut transcript)
        .expect("proof generation should not fail");
    let proof = transcript.finalize();
    println!("Prove: [{:?}]", start.elapsed());

    let pubinput = params
        .commit_lagrange(&public_inputs, Blind::default())
        .to_affine();
    let pubinput_slice = &[pubinput];

    let start = Instant::now();
    let msm = params.empty_msm();
    let mut transcript = Blake2bRead::<_, _, Challenge255<_>>::init(&proof[..]);
    let verification = verify_proof(&params, pk.get_vk(), msm, &[pubinput_slice], &mut transcript);
    if let Err(err) = verification {
        panic!("error {:?}", err);
    }
    let guard = verification.unwrap();
    let msm = guard.clone().use_challenges();
    assert!(msm.eval());
    println!("Verify: [{:?}]", start.elapsed());
}

