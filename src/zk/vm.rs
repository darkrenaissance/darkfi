/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use darkfi_sdk::crypto::constants::{
    sinsemilla::{OrchardCommitDomains, OrchardHashDomains},
    util::gen_const_array,
    NullifierK, OrchardFixedBases, OrchardFixedBasesFull, ValueCommitV, MERKLE_DEPTH_ORCHARD,
};
use halo2_gadgets::{
    ecc::{
        chip::{EccChip, EccConfig},
        FixedPoint, FixedPointBaseField, FixedPointShort, Point, ScalarFixed, ScalarFixedShort,
    },
    poseidon::{
        primitives as poseidon, Hash as PoseidonHash, Pow5Chip as PoseidonChip,
        Pow5Config as PoseidonConfig,
    },
    sinsemilla::{
        chip::{SinsemillaChip, SinsemillaConfig},
        merkle::{
            chip::{MerkleChip, MerkleConfig},
            MerklePath,
        },
    },
    utilities::lookup_range_check::LookupRangeCheckConfig,
};
use halo2_proofs::{
    circuit::{floor_planner, AssignedCell, Layouter, Value},
    pasta::{group::Curve, pallas, Fp},
    plonk,
    plonk::{Advice, Circuit, Column, ConstraintSystem, Instance as InstanceColumn},
};
use log::{error, trace};

pub use super::vm_stack::{StackVar, Witness};
use super::{
    assign_free_advice,
    gadget::{
        arithmetic::{ArithChip, ArithConfig, ArithInstruction},
        less_than::{LessThanChip, LessThanConfig},
        native_range_check::{NativeRangeCheckChip, NativeRangeCheckConfig},
        small_range_check::{SmallRangeCheckChip, SmallRangeCheckConfig},
    },
};
use crate::zkas::{
    types::{LitType, StackType},
    Opcode, ZkBinary,
};

#[derive(Clone)]
pub struct VmConfig {
    primary: Column<InstanceColumn>,
    advices: [Column<Advice>; 10],
    ecc_config: EccConfig<OrchardFixedBases>,
    merkle_cfg1: MerkleConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
    merkle_cfg2: MerkleConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
    sinsemilla_cfg1: SinsemillaConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
    _sinsemilla_cfg2: SinsemillaConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
    poseidon_config: PoseidonConfig<pallas::Base, 3, 2>,
    arith_config: ArithConfig,

    native_64_range_check_config: NativeRangeCheckConfig<3, 64, 22>,
    native_253_range_check_config: NativeRangeCheckConfig<3, 253, 85>,
    lessthan_config: LessThanConfig<3, 253, 85>,
    boolcheck_config: SmallRangeCheckConfig,
}

impl VmConfig {
    fn ecc_chip(&self) -> EccChip<OrchardFixedBases> {
        EccChip::construct(self.ecc_config.clone())
    }

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

    fn poseidon_chip(&self) -> PoseidonChip<pallas::Base, 3, 2> {
        PoseidonChip::construct(self.poseidon_config.clone())
    }

    fn arithmetic_chip(&self) -> ArithChip {
        ArithChip::construct(self.arith_config.clone())
    }
}

pub struct ZkCircuit {
    constants: Vec<String>,
    witnesses: Vec<Witness>,
    literals: Vec<(LitType, String)>,
    opcodes: Vec<(Opcode, Vec<(StackType, usize)>)>,
}

impl ZkCircuit {
    pub fn new(witnesses: Vec<Witness>, circuit_code: ZkBinary) -> Self {
        let constants = circuit_code.constants.iter().map(|x| x.1.clone()).collect();
        #[allow(clippy::map_clone)]
        let literals = circuit_code.literals.iter().map(|x| x.clone()).collect();
        Self { constants, witnesses, literals, opcodes: circuit_code.opcodes }
    }
}

impl Circuit<pallas::Base> for ZkCircuit {
    type Config = VmConfig;
    type FloorPlanner = floor_planner::V1;

    fn without_witnesses(&self) -> Self {
        Self {
            constants: self.constants.clone(),
            witnesses: self.witnesses.clone(),
            literals: self.literals.clone(),
            opcodes: self.opcodes.clone(),
        }
    }

    fn configure(meta: &mut ConstraintSystem<pallas::Base>) -> Self::Config {
        //  Advice columns used in the circuit
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
        meta.enable_equality(primary);

        // Permutation over all advice columns
        for advice in advices.iter() {
            meta.enable_equality(*advice);
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
        let ecc_config =
            EccChip::<OrchardFixedBases>::configure(meta, advices, lagrange_coeffs, range_check);

        // Configuration for the Poseidon hash
        let poseidon_config = PoseidonChip::configure::<poseidon::P128Pow5T3>(
            meta,
            advices[6..9].try_into().unwrap(),
            advices[5],
            rc_a,
            rc_b,
        );

        // Configuration for the Arithmetic chip
        let arith_config = ArithChip::configure(meta, advices[7], advices[8], advices[6]);

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
                range_check,
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

        // K-table for 64 bit range check lookups
        let k_values_table_64 = meta.lookup_table_column();
        let native_64_range_check_config =
            NativeRangeCheckChip::<3, 64, 22>::configure(meta, advices[8], k_values_table_64);

        // K-table for 253 bit range check lookups
        let k_values_table_253 = meta.lookup_table_column();
        let native_253_range_check_config =
            NativeRangeCheckChip::<3, 253, 85>::configure(meta, advices[8], k_values_table_253);

        // TODO: FIXME: Configure these better, this is just a stop-gap
        let z1 = meta.advice_column();
        let z2 = meta.advice_column();

        let lessthan_config = LessThanChip::<3, 253, 85>::configure(
            meta,
            advices[6],
            advices[7],
            advices[8],
            z1,
            z2,
            k_values_table_253,
        );

        // Configuration for boolean checks, it uses the small_range_check
        // chip with a range of 2, which enforces one bit, i.e. 0 or 1.
        let boolcheck_config = SmallRangeCheckChip::configure(meta, advices[9], 2);

        VmConfig {
            primary,
            advices,
            ecc_config,
            merkle_cfg1,
            merkle_cfg2,
            sinsemilla_cfg1,
            _sinsemilla_cfg2,
            poseidon_config,
            arith_config,
            native_64_range_check_config,
            native_253_range_check_config,
            lessthan_config,
            boolcheck_config,
        }
    }

    fn synthesize(
        &self,
        config: Self::Config,
        mut layouter: impl Layouter<pallas::Base>,
    ) -> std::result::Result<(), plonk::Error> {
        trace!(target: "zkvm", "Entering synthesize()");

        // ===================
        // VM Setup
        //====================

        // Our stack which holds every variable we reference and create.
        let mut stack: Vec<StackVar> = vec![];

        // Our stack which holds all the literal values we have in the circuit.
        // For now, we only support u64.
        let mut litstack: Vec<u64> = vec![];

        // Offset for public inputs
        let mut public_inputs_offset = 0;

        // Offset for literals
        let mut literals_offset = 0;

        // Load the Sinsemilla generator lookup table used by the whole circuit.
        SinsemillaChip::load(config.sinsemilla_cfg1.clone(), &mut layouter)?;

        // Construct the 64-bit NativeRangeCheck and LessThan chips
        let rangecheck64_chip = NativeRangeCheckChip::<3, 64, 22>::construct(
            config.native_64_range_check_config.clone(),
        );
        NativeRangeCheckChip::<3, 64, 22>::load_k_table(
            &mut layouter,
            config.native_64_range_check_config.k_values_table,
        )?;

        // Construct the 253-bit NativeRangeCheck and LessThan chips.
        let rangecheck253_chip = NativeRangeCheckChip::<3, 253, 85>::construct(
            config.native_253_range_check_config.clone(),
        );
        let lessthan_chip = LessThanChip::<3, 253, 85>::construct(config.lessthan_config.clone());
        NativeRangeCheckChip::<3, 253, 85>::load_k_table(
            &mut layouter,
            config.native_253_range_check_config.k_values_table,
        )?;

        // Construct the ECC chip.
        let ecc_chip = config.ecc_chip();

        // Construct the Arithmetic chip.
        let arith_chip = config.arithmetic_chip();

        // Construct the boolean check chip.
        let boolcheck_chip = SmallRangeCheckChip::construct(config.boolcheck_config.clone());

        // ==========================
        // Constants setup
        // ==========================

        // This constant one is used for short multiplication
        let one = assign_free_advice(
            layouter.namespace(|| "Load constant one"),
            config.advices[0],
            Value::known(pallas::Base::one()),
        )?;

        // Lookup and push constants onto the stack
        for constant in &self.constants {
            trace!(target: "zkvm", "Pushing constant `{}` to stack index {}", constant.as_str(), stack.len());
            match constant.as_str() {
                "VALUE_COMMIT_VALUE" => {
                    let vcv = ValueCommitV;
                    let vcv = FixedPointShort::from_inner(ecc_chip.clone(), vcv);
                    stack.push(StackVar::EcFixedPointShort(vcv));
                }
                "VALUE_COMMIT_RANDOM" => {
                    let vcr = OrchardFixedBasesFull::ValueCommitR;
                    let vcr = FixedPoint::from_inner(ecc_chip.clone(), vcr);
                    stack.push(StackVar::EcFixedPoint(vcr));
                }
                "NULLIFIER_K" => {
                    let nfk = NullifierK;
                    let nfk = FixedPointBaseField::from_inner(ecc_chip.clone(), nfk);
                    stack.push(StackVar::EcFixedPointBase(nfk));
                }

                _ => {
                    error!("Invalid constant name: {}", constant.as_str());
                    return Err(plonk::Error::Synthesis)
                }
            }
        }

        // Load the literals onto the literal stack.
        // N.B. Only uint64 is supported right now.
        for literal in &self.literals {
            match literal.0 {
                LitType::Uint64 => match literal.1.parse::<u64>() {
                    Ok(v) => litstack.push(v),
                    Err(e) => {
                        error!("Failed converting u64 literal: {}", e);
                        return Err(plonk::Error::Synthesis)
                    }
                },
                _ => {
                    error!("Invalid literal: {:?}", literal);
                    return Err(plonk::Error::Synthesis)
                }
            }
        }

        // Push the witnesses onto the stack, and potentially, if the witness
        // is in the Base field (like the entire circuit is), load it into a
        // table cell.
        for witness in &self.witnesses {
            match witness {
                Witness::EcPoint(w) => {
                    trace!(target: "zkvm", "Witnessing EcPoint into circuit");
                    let point = Point::new(
                        ecc_chip.clone(),
                        layouter.namespace(|| "Witness EcPoint"),
                        w.as_ref().map(|cm| cm.to_affine()),
                    )?;

                    trace!(target: "zkvm", "Pushing EcPoint to stack index {}", stack.len());
                    stack.push(StackVar::EcPoint(point));
                }

                Witness::EcFixedPoint(_) => {
                    error!("Unable to witness EcFixedPoint, this is unimplemented.");
                    return Err(plonk::Error::Synthesis)
                }

                Witness::Base(w) => {
                    trace!(target: "zkvm", "Witnessing Base into circuit");
                    let base = assign_free_advice(
                        layouter.namespace(|| "Witness Base"),
                        config.advices[0],
                        *w,
                    )?;

                    trace!(target: "zkvm", "Pushing Base to stack index {}", stack.len());
                    stack.push(StackVar::Base(base));
                }

                Witness::Scalar(w) => {
                    // NOTE: Because the type in `halo2_gadgets` does not have a `Clone`
                    //       impl, we push scalars as-is to the stack. They get witnessed
                    //       when they get used.
                    trace!(target: "zkvm", "Pushing Scalar to stack index {}", stack.len());
                    stack.push(StackVar::Scalar(*w));
                }

                Witness::MerklePath(w) => {
                    trace!(target: "zkvm", "Witnessing MerklePath into circuit");
                    let path: Value<[pallas::Base; MERKLE_DEPTH_ORCHARD]> =
                        w.map(|typed_path| gen_const_array(|i| typed_path[i].inner()));

                    trace!(target: "zkvm", "Pushing MerklePath to stack index {}", stack.len());
                    stack.push(StackVar::MerklePath(path));
                }

                Witness::Uint32(w) => {
                    trace!(target: "zkvm", "Pushing Uint32 to stack index {}", stack.len());
                    stack.push(StackVar::Uint32(*w));
                }

                Witness::Uint64(w) => {
                    trace!(target: "zkvm", "Pushing Uint64 to stack index {}", stack.len());
                    stack.push(StackVar::Uint64(*w));
                }
            }
        }

        // =============================
        // And now, work through opcodes
        // =============================
        // TODO: Copy constraints
        for opcode in &self.opcodes {
            match opcode.0 {
                Opcode::EcAdd => {
                    trace!(target: "zkvm", "Executing `EcAdd{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let lhs: Point<pallas::Affine, EccChip<OrchardFixedBases>> =
                        stack[args[0].1].clone().into();

                    let rhs: Point<pallas::Affine, EccChip<OrchardFixedBases>> =
                        stack[args[1].1].clone().into();

                    let ret = lhs.add(layouter.namespace(|| "EcAdd()"), &rhs)?;

                    trace!(target: "zkvm", "Pushing result to stack index {}", stack.len());
                    stack.push(StackVar::EcPoint(ret));
                }

                Opcode::EcMul => {
                    trace!(target: "zkvm", "Executing `EcMul{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let lhs: FixedPoint<pallas::Affine, EccChip<OrchardFixedBases>> =
                        stack[args[1].1].clone().into();

                    let rhs = ScalarFixed::new(
                        ecc_chip.clone(),
                        layouter.namespace(|| "EcMul: ScalarFixed::new()"),
                        stack[args[0].1].clone().into(),
                    )?;

                    let (ret, _) = lhs.mul(layouter.namespace(|| "EcMul()"), rhs)?;

                    trace!(target: "zkvm", "Pushing result to stack index {}", stack.len());
                    stack.push(StackVar::EcPoint(ret));
                }

                Opcode::EcMulBase => {
                    trace!(target: "zkvm", "Executing `EcMulBase{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let lhs: FixedPointBaseField<pallas::Affine, EccChip<OrchardFixedBases>> =
                        stack[args[1].1].clone().into();

                    let rhs: AssignedCell<Fp, Fp> = stack[args[0].1].clone().into();

                    let ret = lhs.mul(layouter.namespace(|| "EcMulBase()"), rhs)?;

                    trace!(target: "zkvm", "Pushing result to stack index {}", stack.len());
                    stack.push(StackVar::EcPoint(ret));
                }

                Opcode::EcMulShort => {
                    trace!(target: "zkvm", "Executing `EcMulShort{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let lhs: FixedPointShort<pallas::Affine, EccChip<OrchardFixedBases>> =
                        stack[args[1].1].clone().into();

                    let rhs = ScalarFixedShort::new(
                        ecc_chip.clone(),
                        layouter.namespace(|| "EcMulShort: ScalarFixedShort::new()"),
                        (stack[args[0].1].clone().into(), one.clone()),
                    )?;

                    let (ret, _) = lhs.mul(layouter.namespace(|| "EcMulShort()"), rhs)?;

                    trace!(target: "zkvm", "Pushing result to stack index {}", stack.len());
                    stack.push(StackVar::EcPoint(ret));
                }

                Opcode::EcGetX => {
                    trace!(target: "zkvm", "Executing `EcGetX{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let point: Point<pallas::Affine, EccChip<OrchardFixedBases>> =
                        stack[args[0].1].clone().into();

                    let ret = point.inner().x();

                    trace!(target: "zkvm", "Pushing result to stack index {}", stack.len());
                    stack.push(StackVar::Base(ret));
                }

                Opcode::EcGetY => {
                    trace!(target: "zkvm", "Executing `EcGetY{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let point: Point<pallas::Affine, EccChip<OrchardFixedBases>> =
                        stack[args[0].1].clone().into();

                    let ret = point.inner().y();

                    trace!(target: "zkvm", "Pushing result to stack index {}", stack.len());
                    stack.push(StackVar::Base(ret));
                }

                Opcode::PoseidonHash => {
                    trace!(target: "zkvm", "Executing `PoseidonHash{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let mut poseidon_message: Vec<AssignedCell<Fp, Fp>> =
                        Vec::with_capacity(args.len());

                    for idx in args {
                        poseidon_message.push(stack[idx.1].clone().into());
                    }

                    macro_rules! poseidon_hash {
                        ($len:expr, $hasher:ident, $output:ident, $cell:ident) => {
                            let $hasher = PoseidonHash::<
                                _,
                                _,
                                poseidon::P128Pow5T3,
                                poseidon::ConstantLength<$len>,
                                3,
                                2,
                            >::init(
                                config.poseidon_chip(),
                                layouter.namespace(|| "PoseidonHash init"),
                            )?;

                            let $output = $hasher.hash(
                                layouter.namespace(|| "PoseidonHash hash"),
                                poseidon_message.try_into().unwrap(),
                            )?;

                            let $cell: AssignedCell<Fp, Fp> = $output.into();

                            trace!(target: "zkvm", "Pushing hash to stack index {}", stack.len());
                            stack.push(StackVar::Base($cell));
                        };
                    }

                    macro_rules! vla {
                        ($args:ident, $a:ident, $b:ident, $c:ident, $($num:tt)*) => {
                            match $args.len() {
                                $($num => {
                                    poseidon_hash!($num, $a, $b, $c);
                                })*
                                _ => {
                                    error!("Unsupported poseidon hash for {} elements", $args.len());
                                    return Err(plonk::Error::Synthesis)
                                }
                            }
                        };
                    }

                    vla!(args, a, b, c, 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16);
                }

                Opcode::MerkleRoot => {
                    trace!(target: "zkvm", "Executing `MerkleRoot{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let leaf_pos = stack[args[0].1].clone().into();
                    let merkle_path = stack[args[1].1].clone().into();
                    let leaf = stack[args[2].1].clone().into();

                    let merkle_inputs = MerklePath::construct(
                        [config.merkle_chip_1(), config.merkle_chip_2()],
                        OrchardHashDomains::MerkleCrh,
                        leaf_pos,
                        merkle_path,
                    );

                    let root = merkle_inputs
                        .calculate_root(layouter.namespace(|| "MerkleRoot()"), leaf)?;

                    trace!(target: "zkvm", "Pushing merkle root to stack index {}", stack.len());
                    stack.push(StackVar::Base(root));
                }

                Opcode::BaseAdd => {
                    trace!(target: "zkvm", "Executing `BaseAdd{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let lhs = &stack[args[0].1].clone().into();
                    let rhs = &stack[args[1].1].clone().into();

                    let sum = arith_chip.add(layouter.namespace(|| "BaseAdd()"), lhs, rhs)?;

                    trace!(target: "zkvm", "Pushing sum to stack index {}", stack.len());
                    stack.push(StackVar::Base(sum));
                }

                Opcode::BaseMul => {
                    trace!(target: "zkvm", "Executing `BaseSub{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let lhs = &stack[args[0].1].clone().into();
                    let rhs = &stack[args[1].1].clone().into();

                    let product = arith_chip.mul(layouter.namespace(|| "BaseMul()"), lhs, rhs)?;

                    trace!(target: "zkvm", "Pushing product to stack index {}", stack.len());
                    stack.push(StackVar::Base(product));
                }

                Opcode::BaseSub => {
                    trace!(target: "zkvm", "Executing `BaseSub{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let lhs = &stack[args[0].1].clone().into();
                    let rhs = &stack[args[1].1].clone().into();

                    let difference =
                        arith_chip.sub(layouter.namespace(|| "BaseSub()"), lhs, rhs)?;

                    trace!(target: "zkvm", "Pushing difference to stack index {}", stack.len());
                    stack.push(StackVar::Base(difference));
                }

                Opcode::WitnessBase => {
                    trace!(target: "zkvm", "Executing `WitnessBase{:?}` opcode", opcode.1);
                    //let args = &opcode.1;

                    let lit = litstack[literals_offset];
                    literals_offset += 1;

                    let witness = assign_free_advice(
                        layouter.namespace(|| "Witness literal"),
                        config.advices[0],
                        Value::known(pallas::Base::from(lit)),
                    )?;

                    trace!(target: "zkvm", "Pushing assignment to stack index {}", stack.len());
                    stack.push(StackVar::Base(witness));
                }

                Opcode::RangeCheck => {
                    trace!(target: "zkvm", "Executing `RangeCheck{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let lit = litstack[literals_offset];
                    literals_offset += 1;

                    let arg = stack[args[1].1].clone();

                    match lit {
                        64 => {
                            rangecheck64_chip.copy_range_check(
                                layouter.namespace(|| "copy range check 64"),
                                arg.into(),
                                true,
                            )?;
                        }
                        253 => {
                            rangecheck253_chip.copy_range_check(
                                layouter.namespace(|| "copy range check 253"),
                                arg.into(),
                                true,
                            )?;
                        }
                        x => {
                            error!("Unsupported bit-range {} for range_check", x);
                            return Err(plonk::Error::Synthesis)
                        }
                    }
                }

                Opcode::LessThanStrict => {
                    trace!(target: "zkvm", "Executing `LessThanStrict{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let a = stack[args[0].1].clone().into();
                    let b = stack[args[1].1].clone().into();

                    lessthan_chip.copy_less_than(
                        layouter.namespace(|| "copy a<b check"),
                        a,
                        b,
                        0,
                        true,
                    )?;
                }

                Opcode::LessThanLoose => {
                    trace!(target: "zkvm", "Executing `LessThanLoose{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let a = stack[args[0].1].clone().into();
                    let b = stack[args[1].1].clone().into();

                    lessthan_chip.copy_less_than(
                        layouter.namespace(|| "copy a<b check"),
                        a,
                        b,
                        0,
                        false,
                    )?;
                }

                Opcode::BoolCheck => {
                    trace!(target: "zkvm", "Executing `BoolCheck{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let w = stack[args[0].1].clone().into();

                    boolcheck_chip
                        .small_range_check(layouter.namespace(|| "copy boolean check"), w)?;
                }

                Opcode::ConstrainEqualBase => {
                    trace!(target: "zkvm", "Executing `ConstrainEqualBase{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let lhs: AssignedCell<Fp, Fp> = stack[args[0].1].clone().into();
                    let rhs: AssignedCell<Fp, Fp> = stack[args[1].1].clone().into();

                    layouter.assign_region(
                        || "constrain witnessed base equality",
                        |mut region| region.constrain_equal(lhs.cell(), rhs.cell()),
                    )?;
                }

                Opcode::ConstrainEqualPoint => {
                    trace!(target: "zkvm", "Executing `ConstrainEqualPoint{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let lhs: Point<pallas::Affine, EccChip<OrchardFixedBases>> =
                        stack[args[0].1].clone().into();

                    let rhs: Point<pallas::Affine, EccChip<OrchardFixedBases>> =
                        stack[args[1].1].clone().into();

                    lhs.constrain_equal(
                        layouter.namespace(|| "constrain ec point equality"),
                        &rhs,
                    )?;
                }

                Opcode::ConstrainInstance => {
                    trace!(target: "zkvm", "Executing `ConstrainInstance{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let var: AssignedCell<Fp, Fp> = stack[args[0].1].clone().into();

                    layouter.constrain_instance(
                        var.cell(),
                        config.primary,
                        public_inputs_offset,
                    )?;

                    public_inputs_offset += 1;
                }

                _ => {
                    error!("Unsupported opcode");
                    return Err(plonk::Error::Synthesis)
                }
            }
        }

        trace!(target: "zkvm", "Exiting synthesize() successfully");
        Ok(())
    }
}
