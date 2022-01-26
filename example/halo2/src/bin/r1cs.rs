use halo2::{
    arithmetic::FieldExt,
    circuit::{Layouter, SimpleFloorPlanner},
    dev::MockProver,
    plonk::{Advice, Circuit, Column, ConstraintSystem, Error, Selector},
    poly::Rotation,
};
use pasta_curves::Fp;

#[derive(Clone)]
struct R1csConfig {
    a: Column<Advice>,
    b: Column<Advice>,
    c: Column<Advice>,
    s: Selector,
}

#[derive(Clone, Default)]
struct R1csCircuit {
    a: Option<u64>,
    b: Option<u64>,
}

impl<F: FieldExt> Circuit<F> for R1csCircuit {
    type Config = R1csConfig;
    type FloorPlanner = SimpleFloorPlanner;

    fn without_witnesses(&self) -> Self {
        Self::default()
    }

    fn configure(meta: &mut ConstraintSystem<F>) -> R1csConfig {
        let a = meta.advice_column();
        let b = meta.advice_column();
        let c = meta.advice_column();
        let s = meta.selector();

        meta.create_gate("R1CS constraint", |meta| {
            let a = meta.query_advice(a, Rotation::cur());
            let b = meta.query_advice(b, Rotation::cur());
            let c = meta.query_advice(c, Rotation::cur());
            let s = meta.query_selector(s);

            Some(("R1CS", s * (a * b - c)))
        });

        R1csConfig { a, b, c, s }
    }

    fn synthesize(&self, config: R1csConfig, mut layouter: impl Layouter<F>) -> Result<(), Error> {
        layouter.assign_region(
            || "Example region",
            |mut region| {
                config.s.enable(&mut region, 0)?;
                region.assign_advice(
                    || "a",
                    config.a,
                    0,
                    || self.a.map(|v| F::from_u64(v)).ok_or(Error::SynthesisError),
                )?;
                region.assign_advice(
                    || "b",
                    config.b,
                    0,
                    || self.b.map(|v| F::from_u64(v)).ok_or(Error::SynthesisError),
                )?;
                region.assign_advice(
                    || "c",
                    config.c,
                    0,
                    || {
                        self.a
                            .and_then(|a| self.b.map(|b| F::from_u64(a * b)))
                            .ok_or(Error::SynthesisError)
                    },
                )?;
                Ok(())
            },
        )
    }
}

fn main() -> Result<(), Error> {
    let circuit = R1csCircuit { a: Some(2), b: Some(4) };

    let public_inputs = vec![];

    let prover = MockProver::<Fp>::run(5, &circuit, public_inputs).unwrap();
    assert_eq!(prover.verify(), Ok(()));

    Ok(())
}
