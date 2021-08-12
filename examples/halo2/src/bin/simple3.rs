use halo2::{
    circuit::{SimpleFloorPlanner, Chip, Layouter},
    pasta::{EqAffine, Fp},
    plonk::{Advice, Circuit, Column, ConstraintSystem, Error, Expression, Selector, create_proof, verify_proof, keygen_vk, keygen_pk},
    poly::{commitment::Params, Rotation},
    transcript::{Blake2bRead, Blake2bWrite, Challenge255},
};
use std::time::Instant;

#[derive(Clone, Debug)]
struct CoolConfig {
    a_col: Column<Advice>,
    s_range: Selector,
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

impl CoolChip {
    fn construct(config: CoolConfig) -> Self {
        Self { config }
    }

    fn configure(cs: &mut ConstraintSystem<Fp>) -> CoolConfig {
        let a_col = cs.advice_column();
        let s_range = cs.selector();

        cs.create_gate("check", |cs| {
            let a = cs.query_advice(a_col, Rotation::cur());
            let s_range = cs.query_selector(s_range);
            vec![s_range * (a - Expression::Constant(Fp::from(2)))]
        });

        CoolConfig { a_col, s_range }
    }

    fn alloc_and_check(
        &self,
        layouter: &mut impl Layouter<Fp>,
        a: Option<Fp>,
    ) -> Result<(), Error> {
        layouter.assign_region(
            || "load private inputs",
            |mut region| {
                let row_offset = 0;
                self.config.s_range.enable(&mut region, row_offset)?;
                region.assign_advice(
                    || "private input 'a'",
                    self.config.a_col,
                    row_offset,
                    || a.ok_or(Error::SynthesisError),
                )?;
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
        chip.alloc_and_check(&mut layouter, self.a)
    }
}

fn main() {
    let start = Instant::now();
    let params: Params<EqAffine> = Params::new(4);

    let empty_circuit = CoolCircuit { a: None };
    let vk = keygen_vk(&params, &empty_circuit).expect("keygen_vk should not fail");
    let pk = keygen_pk(&params, vk, &empty_circuit).expect("keygen_pk should not fail");
    println!("Setup: [{:?}]", start.elapsed());

    let start = Instant::now();
    let circuit = CoolCircuit {
        a: Some(Fp::from(2)),
    };

    // Create a proof
    let mut transcript = Blake2bWrite::<_, _, Challenge255<_>>::init(vec![]);
    create_proof(&params, &pk, &[circuit], &[&[]], &mut transcript)
        .expect("proof generation should not fail");
    let proof = transcript.finalize();
    println!("Prove: [{:?}]", start.elapsed());

    let start = Instant::now();
    let msm = params.empty_msm();
    let mut transcript = Blake2bRead::<_, _, Challenge255<_>>::init(&proof[..]);
    let verification = verify_proof(&params, pk.get_vk(), msm, &[&[]], &mut transcript);
    if let Err(err) = verification {
        panic!("error {:?}", err);
    }
    let guard = verification.unwrap();
    let msm = guard.clone().use_challenges();
    assert!(msm.eval());
    println!("Verify: [{:?}]", start.elapsed());
}

