use halo2::{
    circuit::{Layouter, SimpleFloorPlanner},
    plonk,
    plonk::{Advice, Circuit, Column, ConstraintSystem, Instance as InstanceColumn},
};
use halo2_gadgets::{
    ecc::{
        chip::{EccChip, EccConfig},
        FixedPoint, Point,
    },
    poseidon::{Hash as PoseidonHash, Pow5T3Chip as PoseidonChip, Pow5T3Config as PoseidonConfig},
    primitives::poseidon::{ConstantLength, P128Pow5T3},
    sinsemilla::{
        chip::{SinsemillaChip, SinsemillaConfig},
        merkle::{
            chip::{MerkleChip, MerkleConfig},
            MerklePath,
        },
    },
    utilities::{
        lookup_range_check::LookupRangeCheckConfig, CellValue, UtilitiesInstructions, Var,
    },
};
use log::debug;
use pasta_curves::pallas;

use crate::{
    crypto::{
        constants::{
            sinsemilla::{OrchardCommitDomains, OrchardHashDomains},
            OrchardFixedBases,
        },
        merkle_node::MerkleNode,
    },
    zkas::{decoder::ZkBinary, opcode::Opcode},
};

// Stack type abstractions
#[derive(Clone)]
enum ConstantVar {
    EcFixedPoint(FixedPoint<pallas::Affine, EccChip<OrchardFixedBases>>),
}

#[derive(Clone)]
pub enum WitnessVar {
    EcPoint(Point<pallas::Affine, EccChip<OrchardFixedBases>>),
    EcFixedPoint(FixedPoint<pallas::Affine, EccChip<OrchardFixedBases>>),
    Base(pallas::Base),
    Scalar(pallas::Scalar),
    MerklePath(Vec<MerkleNode>),
    Uint32(u32),
    Uint64(u64),
}

#[derive(Clone)]
enum StackVar {
    Constant(ConstantVar),
    Witness(WitnessVar),
    Cell(CellValue<pallas::Base>),
}

impl StackVar {
    fn to_ec_point(&self) -> Point<pallas::Affine, EccChip<OrchardFixedBases>> {
        let inner = match self {
            StackVar::Witness(v) => v,
            _ => unimplemented!(),
        };

        let inner = match inner {
            WitnessVar::EcPoint(v) => v,
            _ => unimplemented!(),
        };

        inner.clone()
    }

    fn to_fixed_point(&self) -> FixedPoint<pallas::Affine, EccChip<OrchardFixedBases>> {
        let inner = match self {
            StackVar::Witness(WitnessVar::EcFixedPoint(v)) => v,
            StackVar::Constant(ConstantVar::EcFixedPoint(v)) => v,
            _ => unimplemented!(),
        };

        inner.clone()
    }

    fn to_scalar(&self) -> pallas::Scalar {
        let inner = match self {
            StackVar::Witness(v) => v,
            _ => unimplemented!(),
        };

        let inner = match inner {
            WitnessVar::Scalar(v) => v,
            _ => unimplemented!(),
        };

        *inner
    }

    fn to_base(&self) -> CellValue<pallas::Base> {
        let inner = match self {
            StackVar::Cell(v) => v,
            _ => unimplemented!(),
        };

        *inner
    }
}

impl From<StackVar> for std::option::Option<u32> {
    fn from(value: StackVar) -> Self {
        match value {
            StackVar::Witness(WitnessVar::Uint32(v)) => Some(v),
            _ => unimplemented!(),
        }
    }
}

impl From<StackVar> for std::option::Option<u64> {
    fn from(value: StackVar) -> Self {
        match value {
            StackVar::Witness(WitnessVar::Uint64(v)) => Some(v),
            _ => unimplemented!(),
        }
    }
}

impl From<StackVar> for std::option::Option<[pallas::Base; 32]> {
    fn from(value: StackVar) -> Self {
        match value {
            StackVar::Witness(WitnessVar::MerklePath(v)) => {
                let ret: Vec<pallas::Base> = v.iter().map(|x| x.0).collect();
                Some(ret.try_into().unwrap())
            }
            _ => unimplemented!(),
        }
    }
}

impl From<StackVar> for CellValue<pallas::Base> {
    fn from(value: StackVar) -> Self {
        match value {
            StackVar::Cell(v) => v,
            _ => unimplemented!(),
        }
    }
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct VmConfig {
    primary: Column<InstanceColumn>,
    advices: [Column<Advice>; 10],
    ecc_config: EccConfig,
    merkle_cfg1: MerkleConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
    merkle_cfg2: MerkleConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
    sinsemilla_cfg1: SinsemillaConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
    sinsemilla_cfg2: SinsemillaConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
    poseidon_config: PoseidonConfig<pallas::Base>,
}

impl VmConfig {
    fn ecc_chip(&self) -> EccChip<OrchardFixedBases> {
        EccChip::construct(self.ecc_config.clone())
    }

    /*
    fn sinsemilla_chip_1(
        &self,
    ) -> SinsemillaChip<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases> {
        SinsemillaChip::construct(self.sinsemilla_cfg1.clone())
    }

    fn sinsemilla_chip_2(
        &self,
    ) -> SinsemillaChip<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases> {
        SinsemillaChip::construct(self.sinsemilla_cfg2.clone())
    }
    */

    fn merkle_chip_1(
        &self,
    ) -> MerkleChip<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases> {
        MerkleChip::construct(self.merkle_cfg1.clone())
    }

    fn merkle_chip_2(
        &self,
    ) -> MerkleChip<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases> {
        MerkleChip::construct(self.merkle_cfg2.clone())
    }

    fn poseidon_chip(&self) -> PoseidonChip<pallas::Base> {
        PoseidonChip::construct(self.poseidon_config.clone())
    }
}

#[derive(Default)]
pub struct ZkCircuit {
    constants: Vec<String>,
    witnesses: Vec<WitnessVar>,
    opcodes: Vec<(Opcode, Vec<u64>)>,
    pub public_inputs: Vec<pallas::Base>,
}

impl ZkCircuit {
    pub fn new(
        witnesses: Vec<WitnessVar>,
        public_inputs: Vec<pallas::Base>,
        circuit_code: ZkBinary,
    ) -> Self {
        let constants = circuit_code.constants.iter().map(|x| x.1.clone()).collect();
        Self { constants, witnesses, opcodes: circuit_code.opcodes, public_inputs }
    }
}

impl UtilitiesInstructions<pallas::Base> for ZkCircuit {
    type Var = CellValue<pallas::Base>;
}

impl Circuit<pallas::Base> for ZkCircuit {
    type Config = VmConfig;
    type FloorPlanner = SimpleFloorPlanner;

    fn without_witnesses(&self) -> Self {
        Self::default()
    }

    fn configure(meta: &mut ConstraintSystem<pallas::Base>) -> Self::Config {
        // Advice columns used in the circuit
        let advices = [
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
        ];

        // Fixed columns for the Sinsemilla generator lookup table
        let table_idx = meta.lookup_table_column();
        let lookup = (table_idx, meta.lookup_table_column(), meta.lookup_table_column());

        // Instance column used for public inputs
        let primary = meta.instance_column();
        meta.enable_equality(primary.into());

        // Permutation over all advice columns
        for advice in advices.iter() {
            meta.enable_equality((*advice).into());
        }

        // Poseidon requires four advice columns, while ECC incomplete addition
        // requires six. We can reduce the proof size by sharing fixed columns
        // between the ECC and Poseidon chips.
        // TODO: For multiple invocations perhaps they could/should be configured
        // in parallel rather than sharing?
        let lagrange_coeffs = [
            meta.fixed_column(),
            meta.fixed_column(),
            meta.fixed_column(),
            meta.fixed_column(),
            meta.fixed_column(),
            meta.fixed_column(),
            meta.fixed_column(),
            meta.fixed_column(),
        ];
        let rc_a = lagrange_coeffs[2..5].try_into().unwrap();
        let rc_b = lagrange_coeffs[5..8].try_into().unwrap();

        // Also use the first Lagrange coefficient column for loading global constants.
        meta.enable_constant(lagrange_coeffs[0]);

        // Use one of the right-most advice columns for all of our range checks.
        let range_check = LookupRangeCheckConfig::configure(meta, advices[9], table_idx);

        // Configuration for curve point operations.
        // This uses 10 advice columns and spans the whole circuit.
        let ecc_config = EccChip::<OrchardFixedBases>::configure(
            meta,
            advices,
            lagrange_coeffs,
            range_check.clone(),
        );

        // Configuration for the Poseidon hash
        let poseidon_config = PoseidonChip::configure(
            meta,
            P128Pow5T3,
            advices[6..9].try_into().unwrap(),
            advices[5],
            rc_a,
            rc_b,
        );

        // Configuration for a Sinsemilla hash instantiation and a
        // Merkle hash instantiation using this Sinsemilla instance.
        // Since the Sinsemilla config uses only 5 advice columns,
        // we can fit two instances side-by-side.
        let (sinsemilla_cfg1, merkle_cfg1) = {
            let sinsemilla_cfg1 = SinsemillaChip::configure(
                meta,
                advices[..5].try_into().unwrap(),
                advices[6],
                lagrange_coeffs[0],
                lookup,
                range_check.clone(),
            );
            let merkle_cfg1 = MerkleChip::configure(meta, sinsemilla_cfg1.clone());
            (sinsemilla_cfg1, merkle_cfg1)
        };

        let (sinsemilla_cfg2, merkle_cfg2) = {
            let sinsemilla_cfg2 = SinsemillaChip::configure(
                meta,
                advices[5..].try_into().unwrap(),
                advices[7],
                lagrange_coeffs[1],
                lookup,
                range_check,
            );
            let merkle_cfg2 = MerkleChip::configure(meta, sinsemilla_cfg2.clone());
            (sinsemilla_cfg2, merkle_cfg2)
        };

        VmConfig {
            primary,
            advices,
            ecc_config,
            merkle_cfg1,
            merkle_cfg2,
            sinsemilla_cfg1,
            sinsemilla_cfg2,
            poseidon_config,
        }
    }

    fn synthesize(
        &self,
        config: Self::Config,
        mut layouter: impl Layouter<pallas::Base>,
    ) -> std::result::Result<(), plonk::Error> {
        debug!("Entering synthesize()");
        // Our stack which holds everything we reference
        let mut stack: Vec<StackVar> = vec![];

        // Offset for public inputs
        let mut public_inputs_offset = 0;

        // Load the Sinsemilla generator lookup table used by the whole circuit.
        SinsemillaChip::load(config.sinsemilla_cfg1.clone(), &mut layouter)?;

        // Construct the ECC chip.
        let ecc_chip = config.ecc_chip();

        // Construct the Merkle chips
        let merkle_chip_1 = config.merkle_chip_1();
        let merkle_chip_2 = config.merkle_chip_2();

        // This constant one is used for short multiplication
        let one = self.load_private(
            layouter.namespace(|| "Load constant one"),
            config.advices[0],
            Some(pallas::Base::one()),
        )?;

        for constant in &self.constants {
            debug!("Pushing constant `{}` to stack index {}", constant.as_str(), stack.len());
            match constant.as_str() {
                "VALUE_COMMIT_VALUE" => {
                    let vcv = OrchardFixedBases::ValueCommitV;
                    let vcv = FixedPoint::from_inner(ecc_chip.clone(), vcv);
                    stack.push(StackVar::Constant(ConstantVar::EcFixedPoint(vcv)));
                }
                "VALUE_COMMIT_RANDOM" => {
                    let vcr = OrchardFixedBases::ValueCommitR;
                    let vcr = FixedPoint::from_inner(ecc_chip.clone(), vcr);
                    stack.push(StackVar::Constant(ConstantVar::EcFixedPoint(vcr)));
                }
                "NULLIFIER_K" => {
                    let nfk = OrchardFixedBases::NullifierK;
                    let nfk = FixedPoint::from_inner(ecc_chip.clone(), nfk);
                    stack.push(StackVar::Constant(ConstantVar::EcFixedPoint(nfk)));
                }
                _ => unimplemented!(),
            }
        }

        for witness in &self.witnesses {
            match witness {
                WitnessVar::EcPoint(_) => {
                    debug!("Pushing EcPoint to stack index {}", stack.len());
                    stack.push(StackVar::Witness(witness.clone()));
                }

                WitnessVar::EcFixedPoint(_) => {
                    debug!("Pushing EcFixedPoint to stack index {}", stack.len());
                    stack.push(StackVar::Witness(witness.clone()));
                }

                WitnessVar::Base(v) => {
                    debug!("Loading Base element into cell");
                    let w = self.load_private(
                        layouter.namespace(|| "Load witness into cell"),
                        config.advices[0],
                        Some(*v),
                    )?;

                    debug!("Pushing Base to stack index {}", stack.len());
                    stack.push(StackVar::Cell(w));
                }

                WitnessVar::Scalar(_) => {
                    debug!("Pushing Scalar to stack index {}", stack.len());
                    stack.push(StackVar::Witness(witness.clone()));
                }

                WitnessVar::MerklePath(_) => {
                    debug!("Pushing MerklePath to stack index {}", stack.len());
                    stack.push(StackVar::Witness(witness.clone()));
                }

                WitnessVar::Uint32(_) => {
                    debug!("Pushing Uint32 to stack index {}", stack.len());
                    stack.push(StackVar::Witness(witness.clone()));
                }

                WitnessVar::Uint64(_) => {
                    debug!("Pushing Uint64 to stack index {}", stack.len());
                    stack.push(StackVar::Witness(witness.clone()));
                }
            }
        }

        // TODO: Guard usize casts
        for opcode in &self.opcodes {
            match opcode.0 {
                Opcode::EcAdd => {
                    debug!("Executing `EcAdd{:?}` opcode", opcode.1);
                    let args = opcode.1.clone();

                    let lhs = stack[args[0] as usize].to_ec_point();
                    let rhs = stack[args[1] as usize].to_ec_point();

                    let result = lhs.add(layouter.namespace(|| "EcAdd"), &rhs)?;

                    debug!("Pushing result to stack index {}", stack.len());
                    stack.push(StackVar::Witness(WitnessVar::EcPoint(result)));
                }

                Opcode::EcMul => {
                    debug!("Executing `EcMul{:?}` opcode", opcode.1);
                    let args = opcode.1.clone();

                    let lhs = stack[args[0] as usize].to_scalar();
                    let rhs = stack[args[1] as usize].to_fixed_point();

                    let (result, _) = rhs.mul(layouter.namespace(|| "EcMul"), Some(lhs))?;

                    debug!("Pushing result to stack index {}", stack.len());
                    stack.push(StackVar::Witness(WitnessVar::EcPoint(result)));
                }

                Opcode::EcMulBase => {
                    debug!("Executing `EcMulBase{:?}` opcode", opcode.1);
                    let args = opcode.1.clone();

                    let lhs = stack[args[0] as usize].to_base();
                    let rhs = stack[args[1] as usize].to_fixed_point();

                    let result = rhs.mul_base_field(layouter.namespace(|| "EcMulBase"), lhs)?;

                    debug!("Pushing result to stack index {}", stack.len());
                    stack.push(StackVar::Witness(WitnessVar::EcPoint(result)));
                }

                Opcode::EcMulShort => {
                    debug!("Executing `EcMulShort{:?}` opcode", opcode.1);
                    let args = opcode.1.clone();

                    let lhs = stack[args[0] as usize].to_base();
                    let rhs = stack[args[1] as usize].to_fixed_point();

                    let (result, _) =
                        rhs.mul_short(layouter.namespace(|| "EcMulShort"), (lhs, one))?;

                    debug!("Pushing result to stack index {}", stack.len());
                    stack.push(StackVar::Witness(WitnessVar::EcPoint(result)));
                }

                Opcode::EcGetX => {
                    debug!("Executing `EcGetX{:?}` opcode", opcode.1);
                    let args = opcode.1.clone();

                    let point = stack[args[0] as usize].to_ec_point();
                    let result = point.inner().x();

                    debug!("Pushing result to stack index {}", stack.len());
                    stack.push(StackVar::Cell(result));
                }

                Opcode::EcGetY => {
                    debug!("Executing `EcGetY{:?}` opcode", opcode.1);
                    let args = opcode.1.clone();

                    let point = stack[args[0] as usize].to_ec_point();
                    let result = point.inner().y();

                    debug!("Pushing result to stack index {}", stack.len());
                    stack.push(StackVar::Cell(result));
                }

                Opcode::PoseidonHash => {
                    debug!("Executing `PoseidonHash{:?}` opcode", opcode.1);
                    let args = opcode.1.clone();
                    let mut poseidon_message: Vec<CellValue<pallas::Base>> = vec![];
                    for idx in &args {
                        let index = *idx as usize; // TODO: Guard this usize cast
                        poseidon_message.push(stack[index].clone().into());
                    }

                    macro_rules! poseidon_hash {
                        ($len:expr, $poseidon_hasher:ident, $poseidon_output:ident, $cell_output:ident) => {
                            let $poseidon_hasher = PoseidonHash::<_, _, P128Pow5T3, _, 3, 2>::init(
                                config.poseidon_chip(),
                                layouter.namespace(|| "PoseidonHash ($len msgs) init"),
                                ConstantLength::<$len>,
                            )?;

                            let $poseidon_output = $poseidon_hasher.hash(
                                layouter.namespace(|| "PoseidonHash ($len msgs) hash"),
                                poseidon_message.try_into().unwrap(),
                            )?;

                            let $cell_output: CellValue<pallas::Base> =
                                $poseidon_output.inner().into();

                            debug!("Pushing hash to stack index {}", stack.len());
                            stack.push(StackVar::Cell($cell_output));
                        };
                    }

                    // I can't find a better way to do this.
                    match args.len() {
                        1 => {
                            poseidon_hash!(1, poseidon_hasher, poseidon_output, cell_output);
                        }
                        2 => {
                            poseidon_hash!(2, poseidon_hasher, poseidon_output, cell_output);
                        }
                        3 => {
                            poseidon_hash!(3, poseidon_hasher, poseidon_output, cell_output);
                        }
                        4 => {
                            poseidon_hash!(4, poseidon_hasher, poseidon_output, cell_output);
                        }
                        5 => {
                            poseidon_hash!(5, poseidon_hasher, poseidon_output, cell_output);
                        }
                        6 => {
                            poseidon_hash!(6, poseidon_hasher, poseidon_output, cell_output);
                        }
                        7 => {
                            poseidon_hash!(7, poseidon_hasher, poseidon_output, cell_output);
                        }
                        8 => {
                            poseidon_hash!(8, poseidon_hasher, poseidon_output, cell_output);
                        }
                        _ => unimplemented!(),
                    };
                }

                Opcode::CalculateMerkleRoot => {
                    debug!("Executing `CalculateMerkleRoot{:?}` opcode", opcode.1);
                    let args = opcode.1.clone();

                    let leaf_pos = stack[args[0] as usize].clone().into();
                    let merkle_path = stack[args[1] as usize].clone().into();
                    let leaf = stack[args[2] as usize].clone().into();

                    let path = MerklePath {
                        chip_1: merkle_chip_1.clone(),
                        chip_2: merkle_chip_2.clone(),
                        domain: OrchardHashDomains::MerkleCrh,
                        leaf_pos,
                        path: merkle_path,
                    };

                    let root =
                        path.calculate_root(layouter.namespace(|| "CalculateMerkleRoot"), leaf)?;

                    debug!("Pushing hash to stack index {}", stack.len());
                    stack.push(StackVar::Cell(root));
                }

                Opcode::ConstrainInstance => {
                    debug!("Executing `ConstrainInstance{:?}` opcode", opcode.1);
                    // TODO: Guard this usize cast
                    let var = match stack[opcode.1[0] as usize] {
                        StackVar::Cell(v) => v,
                        _ => panic!("Incorrect stack variable type"),
                    };

                    layouter.constrain_instance(
                        var.cell(),
                        config.primary,
                        public_inputs_offset,
                    )?;

                    public_inputs_offset += 1;
                }

                _ => unimplemented!(),
            }
        }

        // If we haven't exploded until now, we're golden.
        debug!("Exiting synthesize()");
        Ok(())
    }
}
