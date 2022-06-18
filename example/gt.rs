use darkfi::zk::gadget::{
    arith_chip::{ArithChip, ArithConfig, ArithInstruction},
    even_bits::{EvenBitsChip, EvenBitsConfig, EvenBitsLookup},
    greater_than::{GreaterThanChip, GreaterThanConfig, GreaterThanInstruction},
};
use halo2_gadgets::utilities::UtilitiesInstructions;
use halo2_proofs::{
    circuit::{AssignedCell, Layouter, SimpleFloorPlanner},
    dev::MockProver,
    plonk,
    plonk::{Advice, Circuit, Column, ConstraintSystem, Instance as InstanceColumn},
};
use pasta_curves::{pallas, Fp};

const WORD_BITS: u32 = 24;

#[derive(Clone)]
struct ZkConfig {
    primary: Column<InstanceColumn>,
    advices: [Column<Advice>; 3],
    evenbits_config: EvenBitsConfig,
    greaterthan_config: GreaterThanConfig,
    arith_config: ArithConfig,
}

impl ZkConfig {
    fn evenbits_chip(&self) -> EvenBitsChip<pallas::Base, WORD_BITS> {
        EvenBitsChip::construct(self.evenbits_config.clone())
    }

    fn greaterthan_chip(&self) -> GreaterThanChip<pallas::Base, WORD_BITS> {
        GreaterThanChip::construct(self.greaterthan_config.clone())
    }

    fn arith_chip(&self) -> ArithChip {
        ArithChip::construct(self.arith_config.clone())
    }
}

struct ZkCircuit {
    y: Option<pallas::Base>,
    v: Option<pallas::Base>,
    f: Option<pallas::Base>,
}

impl UtilitiesInstructions<pallas::Base> for ZkCircuit {
    type Var = AssignedCell<Fp, Fp>;
}

impl Circuit<pallas::Base> for ZkCircuit {
    type Config = ZkConfig;
    type FloorPlanner = SimpleFloorPlanner;

    fn without_witnesses(&self) -> Self {
        Self { y: None, v: None, f: None }
    }

    fn configure(meta: &mut ConstraintSystem<pallas::Base>) -> Self::Config {
        let advices = [meta.advice_column(), meta.advice_column(), meta.advice_column()];

        // Instance column used for public inputs
        let primary = meta.instance_column();
        meta.enable_equality(primary);

        for advice in advices.iter() {
            meta.enable_equality(*advice);
        }

        let evenbits_config = EvenBitsChip::<pallas::Base, WORD_BITS>::configure(meta);
        let greaterthan_config = GreaterThanChip::<pallas::Base, WORD_BITS>::configure(
            meta,
            [advices[1], advices[2]],
            primary,
        );
        let arith_config = ArithChip::configure(meta, advices[1], advices[2], advices[0]);

        ZkConfig { primary, advices, evenbits_config, greaterthan_config, arith_config }
    }

    fn synthesize(
        &self,
        config: Self::Config,
        mut layouter: impl Layouter<pallas::Base>,
    ) -> Result<(), plonk::Error> {
        let eb_chip = config.evenbits_chip();
        eb_chip.alloc_table(&mut layouter.namespace(|| "alloc table"))?;

        let gt_chip = config.greaterthan_chip();

        let ar_chip = config.arith_chip();

        let y = self.load_private(layouter.namespace(|| "Witness y"), config.advices[0], self.y)?;
        let v = self.load_private(layouter.namespace(|| "Witness v"), config.advices[0], self.v)?;
        let f = self.load_private(layouter.namespace(|| "Witness t"), config.advices[0], self.f)?;

        let t = ar_chip.mul(layouter.namespace(|| "target value"), &v, &f)?;

        eb_chip.decompose(layouter.namespace(|| "y range check"), y.clone())?;
        eb_chip.decompose(layouter.namespace(|| "t range check"), t.clone())?;

        let (helper, greater_than) =
            gt_chip.greater_than(layouter.namespace(|| "y > t"), y.into(), t.into())?;

        eb_chip.decompose(layouter.namespace(|| "helper range check"), helper.0)?;

        layouter.constrain_instance(greater_than.0.cell(), config.primary, 0)?;
        Ok(())
    }
}

fn main() {
    let k = 13;
    let y = pallas::Base::from(2);
    let v = pallas::Base::from(3);
    let f = pallas::Base::from(1);
    //
    let c = pallas::Base::from(0);
    let circuit = ZkCircuit { y: Some(y), v: Some(v), f: Some(f) };

    let public_inputs: Vec<pallas::Base> = vec![c];

    let prover = MockProver::run(k, &circuit, vec![public_inputs]).unwrap();
    assert_eq!(prover.verify(), Ok(()));
}
