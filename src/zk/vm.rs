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
        gen_const_array, lookup_range_check::LookupRangeCheckConfig, CellValue,
        UtilitiesInstructions, Var,
    },
};
use log::debug;
use pasta_curves::{group::Curve, pallas};

pub use super::vm_stack::{StackVar, Witness};
use crate::{
    crypto::constants::{
        sinsemilla::{OrchardCommitDomains, OrchardHashDomains},
        OrchardFixedBases,
    },
    zkas::{decoder::ZkBinary, opcode::Opcode},
};

#[derive(Clone)]
pub struct VmConfig {
    primary: Column<InstanceColumn>,
    advices: [Column<Advice>; 10],
    ecc_config: EccConfig,
    merkle_cfg1: MerkleConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
    merkle_cfg2: MerkleConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
    sinsemilla_cfg1: SinsemillaConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
    _sinsemilla_cfg2: SinsemillaConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
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

#[derive(Clone, Default)]
pub struct ZkCircuit {
    constants: Vec<String>,
    witnesses: Vec<Witness>,
    opcodes: Vec<(Opcode, Vec<usize>)>,
}

impl ZkCircuit {
    pub fn new(witnesses: Vec<Witness>, circuit_code: ZkBinary) -> Self {
        let constants = circuit_code.constants.iter().map(|x| x.1.clone()).collect();
        Self { constants, witnesses, opcodes: circuit_code.opcodes }
    }
}

impl UtilitiesInstructions<pallas::Base> for ZkCircuit {
    type Var = CellValue<pallas::Base>;
}

impl Circuit<pallas::Base> for ZkCircuit {
    type Config = VmConfig;
    type FloorPlanner = SimpleFloorPlanner;

    fn without_witnesses(&self) -> Self {
        Self {
            constants: self.constants.clone(),
            witnesses: self.witnesses.clone(),
            opcodes: self.opcodes.clone(),
        }
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

        let (_sinsemilla_cfg2, merkle_cfg2) = {
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
            _sinsemilla_cfg2,
            poseidon_config,
        }
    }

    fn synthesize(
        &self,
        config: Self::Config,
        mut layouter: impl Layouter<pallas::Base>,
    ) -> std::result::Result<(), plonk::Error> {
        debug!("Entering synthesize()");

        // Our stack which holds everything we reference.
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

        // Lookup and push the constants onto the stack
        for constant in &self.constants {
            debug!("Pushing constant `{}` to stack index {}", constant.as_str(), stack.len());
            match constant.as_str() {
                "VALUE_COMMIT_VALUE" => {
                    let vcv = OrchardFixedBases::ValueCommitV;
                    let vcv = FixedPoint::from_inner(ecc_chip.clone(), vcv);
                    stack.push(StackVar::EcFixedPoint(vcv));
                }
                "VALUE_COMMIT_RANDOM" => {
                    let vcr = OrchardFixedBases::ValueCommitR;
                    let vcr = FixedPoint::from_inner(ecc_chip.clone(), vcr);
                    stack.push(StackVar::EcFixedPoint(vcr));
                }
                "NULLIFIER_K" => {
                    let nfk = OrchardFixedBases::NullifierK;
                    let nfk = FixedPoint::from_inner(ecc_chip.clone(), nfk);
                    stack.push(StackVar::EcFixedPoint(nfk));
                }
                _ => unimplemented!(),
            }
        }

        // Push the witnesses onto the stack, and potentially, if the witness
        // is in the Base field (like the entire circuit is), load it into a
        // table cell.
        for witness in &self.witnesses {
            match witness {
                Witness::EcPoint(w) => {
                    debug!("Witnessing EcPoint into circuit");
                    let point = Point::new(
                        ecc_chip.clone(),
                        layouter.namespace(|| "Witness EcPoint"),
                        w.as_ref().map(|cm| cm.to_affine()),
                    )?;

                    debug!("Pushing EcPoint to stack index {}", stack.len());
                    stack.push(StackVar::EcPoint(point));
                }

                Witness::EcFixedPoint(_) => {
                    unimplemented!()
                }

                Witness::Base(w) => {
                    debug!("Witnessing Base into circuit");
                    let base = self.load_private(
                        layouter.namespace(|| "Witness Base"),
                        config.advices[0],
                        *w,
                    )?;

                    debug!("Pushing Base to stack index {}", stack.len());
                    stack.push(StackVar::Base(base));
                }

                Witness::Scalar(w) => {
                    debug!("Pushing Scalar to stack index {}", stack.len());
                    stack.push(StackVar::Scalar(*w));
                }

                Witness::MerklePath(w) => {
                    debug!("Witnessing MerklePath into circuit");
                    let path = w.map(|typed_path| gen_const_array(|i| typed_path[i].inner()));

                    debug!("Pushing MerklePath to stack index {}", stack.len());
                    stack.push(StackVar::MerklePath(path));
                }

                Witness::Uint32(w) => {
                    debug!("Pushing Uint32 to stack index {}", stack.len());
                    stack.push(StackVar::Uint32(*w));
                }

                Witness::Uint64(w) => {
                    debug!("Pushing Uint64 to stack index {}", stack.len());
                    stack.push(StackVar::Uint64(*w));
                }
            }
        }

        // And now, work through opcodes
        for opcode in &self.opcodes {
            match opcode.0 {
                Opcode::EcAdd => {
                    debug!("Executing `EcAdd{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let lhs: Point<pallas::Affine, EccChip<OrchardFixedBases>> =
                        stack[args[0]].clone().into();

                    let rhs: Point<pallas::Affine, EccChip<OrchardFixedBases>> =
                        stack[args[1]].clone().into();

                    let ret = lhs.add(layouter.namespace(|| "EcAdd()"), &rhs)?;

                    debug!("Pushing result to stack index {}", stack.len());
                    stack.push(StackVar::EcPoint(ret));
                }

                Opcode::EcMul => {
                    debug!("Executing `EcMul{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let lhs: FixedPoint<pallas::Affine, EccChip<OrchardFixedBases>> =
                        stack[args[1]].clone().into();

                    let rhs: Option<pallas::Scalar> = stack[args[0]].clone().into();

                    let (ret, _) = lhs.mul(layouter.namespace(|| "EcMul()"), rhs)?;

                    debug!("Pushing result to stack index {}", stack.len());
                    stack.push(StackVar::EcPoint(ret));
                }

                Opcode::EcMulBase => {
                    debug!("Executing `EcMulBase{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let lhs: FixedPoint<pallas::Affine, EccChip<OrchardFixedBases>> =
                        stack[args[1]].clone().into();

                    let rhs: CellValue<pallas::Base> = stack[args[0]].clone().into();

                    let ret = lhs.mul_base_field(layouter.namespace(|| "EcMulBase()"), rhs)?;

                    debug!("Pushing result to stack index {}", stack.len());
                    stack.push(StackVar::EcPoint(ret));
                }

                Opcode::EcMulShort => {
                    debug!("Executing `EcMulShort{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let lhs: FixedPoint<pallas::Affine, EccChip<OrchardFixedBases>> =
                        stack[args[1]].clone().into();

                    let rhs: CellValue<pallas::Base> = stack[args[0]].clone().into();

                    let (ret, _) =
                        lhs.mul_short(layouter.namespace(|| "EcMulShort()"), (rhs, one))?;

                    debug!("Pushing result to stack index {}", stack.len());
                    stack.push(StackVar::EcPoint(ret));
                }

                Opcode::EcGetX => {
                    debug!("Executing `EcGetX{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let point: Point<pallas::Affine, EccChip<OrchardFixedBases>> =
                        stack[args[0]].clone().into();

                    let ret = point.inner().x();

                    debug!("Pushing result to stack index {}", stack.len());
                    stack.push(StackVar::Base(ret));
                }

                Opcode::EcGetY => {
                    debug!("Executing `EcGetY{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let point: Point<pallas::Affine, EccChip<OrchardFixedBases>> =
                        stack[args[0]].clone().into();

                    let ret = point.inner().y();

                    debug!("Pushing result to stack index {}", stack.len());
                    stack.push(StackVar::Base(ret));
                }

                Opcode::PoseidonHash => {
                    debug!("Executing `PoseidonHash{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let mut poseidon_message: Vec<CellValue<pallas::Base>> =
                        Vec::with_capacity(args.len());

                    for idx in args {
                        poseidon_message.push(stack[*idx].clone().into());
                    }

                    macro_rules! poseidon_hash {
                        ($len:expr, $hasher:ident, $output:ident, $cell:ident) => {
                            let $hasher = PoseidonHash::<_, _, P128Pow5T3, _, 3, 2>::init(
                                config.poseidon_chip(),
                                layouter.namespace(|| "PoseidonHash init"),
                                ConstantLength::<$len>,
                            )?;

                            let $output = $hasher.hash(
                                layouter.namespace(|| "PoseidonHash hash"),
                                poseidon_message.try_into().unwrap(),
                            )?;

                            let $cell: CellValue<pallas::Base> = $output.inner().into();

                            debug!("Pushing hash to stack index {}", stack.len());
                            stack.push(StackVar::Base($cell));
                        };
                    }

                    macro_rules! vla {
                        ($args:ident, $a: ident, $b:ident, $c:ident, $($num:tt)*) => {
                            match $args.len() {
                                $($num => {
                                    poseidon_hash!($num, $a, $b, $c);
                                })*
                                _ => unimplemented!()
                            }
                        };
                    }

                    vla!(args, a, b, c, 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16);
                }

                Opcode::CalculateMerkleRoot => {
                    debug!("Executing `CalculateMerkleRoot{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let leaf_pos = stack[args[0]].clone().into();
                    let merkle_path = stack[args[1]].clone().into();
                    let leaf = stack[args[2]].clone().into();

                    let path = MerklePath {
                        chip_1: merkle_chip_1.clone(),
                        chip_2: merkle_chip_2.clone(),
                        domain: OrchardHashDomains::MerkleCrh,
                        leaf_pos,
                        path: merkle_path,
                    };

                    let root =
                        path.calculate_root(layouter.namespace(|| "CalculateMerkleRoot()"), leaf)?;

                    debug!("Pushing merkle root to stack index {}", stack.len());
                    stack.push(StackVar::Base(root));
                }

                Opcode::ConstrainInstance => {
                    debug!("Executing `ConstrainInstance{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let var: CellValue<pallas::Base> = stack[args[0]].clone().into();

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

        debug!("Exiting synthesize()");
        Ok(())
    }
}
