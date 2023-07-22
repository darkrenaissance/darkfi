/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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
        FixedPoint, FixedPointBaseField, FixedPointShort, NonIdentityPoint, Point, ScalarFixed,
        ScalarFixedShort, ScalarVar,
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
    arithmetic::Field,
    circuit::{floor_planner, AssignedCell, Layouter, Value},
    pasta::{group::Curve, pallas, Fp},
    plonk,
    plonk::{Advice, Circuit, Column, ConstraintSystem, Instance as InstanceColumn},
};
use log::{error, trace};

pub use super::vm_heap::{HeapVar, Witness};
use super::{
    assign_free_advice,
    gadget::{
        arithmetic::{ArithChip, ArithConfig, ArithInstruction},
        cond_select::{ConditionalSelectChip, ConditionalSelectConfig},
        less_than::{LessThanChip, LessThanConfig},
        native_range_check::{NativeRangeCheckChip, NativeRangeCheckConfig},
        small_range_check::{SmallRangeCheckChip, SmallRangeCheckConfig},
        zero_cond::{ZeroCondChip, ZeroCondConfig},
    },
};
use crate::zkas::{
    types::{HeapType, LitType},
    Opcode, ZkBinary,
};

/// Available chips/gadgets in the zkvm
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
enum VmChip {
    /// ECC Chip
    Ecc(EccConfig<OrchardFixedBases>),

    /// Merkle tree chip (using Sinsemilla)
    Merkle(
        (
            MerkleConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
            MerkleConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
        ),
    ),

    /// Sinsemilla chip
    Sinsemilla(
        (
            SinsemillaConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
            SinsemillaConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
        ),
    ),

    /// Poseidon hash chip
    Poseidon(PoseidonConfig<pallas::Base, 3, 2>),

    /// Base field arithmetic chip
    Arithmetic(ArithConfig),

    /// 64 bit native range check
    NativeRange64(NativeRangeCheckConfig<3, 64, 22>),

    /// 253 bit native range check
    NativeRange253(NativeRangeCheckConfig<3, 253, 85>),

    /// 253 bit `a < b` check
    LessThan(LessThanConfig<3, 253, 85>),

    /// Boolean check
    BoolCheck(SmallRangeCheckConfig),

    /// Conditional selection
    CondSelect(ConditionalSelectConfig<pallas::Base>),

    /// Zero-Cond selection
    ZeroCond(ZeroCondConfig<pallas::Base>),
}

/// zkvm configuration
#[derive(Clone)]
pub struct VmConfig {
    /// Chips used in the circuit
    chips: Vec<VmChip>,
    /// Instance column used for public inputs
    primary: Column<InstanceColumn>,
    advices: [Column<Advice>; 10],
}

impl VmConfig {
    fn ecc_chip(&self) -> EccChip<OrchardFixedBases> {
        let Some(VmChip::Ecc(ecc_config)) =
            self.chips.iter().find(|&c| matches!(c, VmChip::Ecc(_)))
        else {
            unreachable!();
        };

        EccChip::construct(ecc_config.clone())
    }

    fn merkle_chip_1(
        &self,
    ) -> MerkleChip<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases> {
        let Some(VmChip::Merkle((merkle_cfg1, _))) =
            self.chips.iter().find(|&c| matches!(c, VmChip::Merkle(_)))
        else {
            unreachable!();
        };

        MerkleChip::construct(merkle_cfg1.clone())
    }

    fn merkle_chip_2(
        &self,
    ) -> MerkleChip<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases> {
        let Some(VmChip::Merkle((_, merkle_cfg2))) =
            self.chips.iter().find(|&c| matches!(c, VmChip::Merkle(_)))
        else {
            unreachable!();
        };

        MerkleChip::construct(merkle_cfg2.clone())
    }

    fn poseidon_chip(&self) -> PoseidonChip<pallas::Base, 3, 2> {
        let Some(VmChip::Poseidon(poseidon_config)) =
            self.chips.iter().find(|&c| matches!(c, VmChip::Poseidon(_)))
        else {
            unreachable!();
        };

        PoseidonChip::construct(poseidon_config.clone())
    }

    fn arithmetic_chip(&self) -> ArithChip<pallas::Base> {
        let Some(VmChip::Arithmetic(arith_config)) =
            self.chips.iter().find(|&c| matches!(c, VmChip::Arithmetic(_)))
        else {
            unreachable!();
        };

        ArithChip::construct(arith_config.clone())
    }

    fn condselect_chip(&self) -> ConditionalSelectChip<pallas::Base> {
        let Some(VmChip::CondSelect(condselect_config)) =
            self.chips.iter().find(|&c| matches!(c, VmChip::CondSelect(_)))
        else {
            unreachable!();
        };

        ConditionalSelectChip::construct(condselect_config.clone(), ())
    }

    fn zerocond_chip(&self) -> ZeroCondChip<pallas::Base> {
        let Some(VmChip::ZeroCond(zerocond_config)) =
            self.chips.iter().find(|&c| matches!(c, VmChip::ZeroCond(_)))
        else {
            unreachable!();
        };

        ZeroCondChip::construct(zerocond_config.clone())
    }

    fn rangecheck64_chip(&self) -> NativeRangeCheckChip<3, 64, 22> {
        let Some(VmChip::NativeRange64(range_config)) =
            self.chips.iter().find(|&c| matches!(c, VmChip::NativeRange64(_)))
        else {
            unreachable!();
        };

        NativeRangeCheckChip::construct(range_config.clone())
    }

    fn rangecheck253_chip(&self) -> NativeRangeCheckChip<3, 253, 85> {
        let Some(VmChip::NativeRange253(range_config)) =
            self.chips.iter().find(|&c| matches!(c, VmChip::NativeRange253(_)))
        else {
            unreachable!();
        };

        NativeRangeCheckChip::construct(range_config.clone())
    }

    fn lessthan_chip(&self) -> LessThanChip<3, 253, 85> {
        let Some(VmChip::LessThan(lessthan_config)) =
            self.chips.iter().find(|&c| matches!(c, VmChip::LessThan(_)))
        else {
            unreachable!();
        };

        LessThanChip::construct(lessthan_config.clone())
    }

    fn boolcheck_chip(&self) -> SmallRangeCheckChip {
        let Some(VmChip::BoolCheck(boolcheck_config)) =
            self.chips.iter().find(|&c| matches!(c, VmChip::BoolCheck(_)))
        else {
            unreachable!();
        };

        SmallRangeCheckChip::construct(boolcheck_config.clone())
    }
}

#[derive(Clone)]
pub struct ZkCircuit {
    constants: Vec<String>,
    witnesses: Vec<Witness>,
    literals: Vec<(LitType, String)>,
    opcodes: Vec<(Opcode, Vec<(HeapType, usize)>)>,
}

impl ZkCircuit {
    pub fn new(witnesses: Vec<Witness>, circuit_code: &ZkBinary) -> Self {
        let constants = circuit_code.constants.iter().map(|x| x.1.clone()).collect();
        #[allow(clippy::map_clone)]
        let literals = circuit_code.literals.iter().map(|x| x.clone()).collect();
        Self { constants, witnesses, literals, opcodes: circuit_code.opcodes.clone() }
    }
}

impl Circuit<pallas::Base> for ZkCircuit {
    type Config = VmConfig;
    type FloorPlanner = floor_planner::V1;
    type Params = ();

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

        // Configuration for the conditional selection chip
        let condselect_config =
            ConditionalSelectChip::configure(meta, advices[1..5].try_into().unwrap());

        // Configuration for the zero_cond selection chip
        let zerocond_config = ZeroCondChip::configure(meta, advices[1..5].try_into().unwrap());

        // Later we'll use this for optimisation
        let chips = vec![
            VmChip::Ecc(ecc_config),
            VmChip::Merkle((merkle_cfg1, merkle_cfg2)),
            VmChip::Sinsemilla((sinsemilla_cfg1, sinsemilla_cfg2)),
            VmChip::Poseidon(poseidon_config),
            VmChip::Arithmetic(arith_config),
            VmChip::NativeRange64(native_64_range_check_config),
            VmChip::NativeRange253(native_253_range_check_config),
            VmChip::LessThan(lessthan_config),
            VmChip::BoolCheck(boolcheck_config),
            VmChip::CondSelect(condselect_config),
            VmChip::ZeroCond(zerocond_config),
        ];

        VmConfig { primary, advices, chips }
    }

    fn synthesize(
        &self,
        config: Self::Config,
        mut layouter: impl Layouter<pallas::Base>,
    ) -> std::result::Result<(), plonk::Error> {
        trace!(target: "zk::vm", "Entering synthesize()");

        // ===================
        // VM Setup
        //====================

        // Our heap which holds every variable we reference and create.
        let mut heap: Vec<HeapVar> = vec![];

        // Our heap which holds all the literal values we have in the circuit.
        // For now, we only support u64.
        let mut litheap: Vec<u64> = vec![];

        // Offset for public inputs
        let mut public_inputs_offset = 0;

        // Offset for literals
        let mut literals_offset = 0;

        // Load the Sinsemilla generator lookup table used by the whole circuit.
        let Some(VmChip::Sinsemilla((sinsemilla_cfg1, _))) =
            config.chips.iter().find(|&c| matches!(c, VmChip::Sinsemilla(_)))
        else {
            unreachable!();
        };
        SinsemillaChip::load(sinsemilla_cfg1.clone(), &mut layouter)?;

        // Construct the 64-bit NativeRangeCheck chip
        let rangecheck64_chip = config.rangecheck64_chip();
        let Some(VmChip::NativeRange64(rangecheck64_config)) =
            config.chips.iter().find(|&c| matches!(c, VmChip::NativeRange64(_)))
        else {
            unreachable!();
        };
        NativeRangeCheckChip::<3, 64, 22>::load_k_table(
            &mut layouter,
            rangecheck64_config.k_values_table,
        )?;

        // Construct the 253-bit NativeRangeCheck and LessThan chips.
        let rangecheck253_chip = config.rangecheck253_chip();
        let lessthan_chip = config.lessthan_chip();

        let Some(VmChip::NativeRange253(rangecheck253_config)) =
            config.chips.iter().find(|&c| matches!(c, VmChip::NativeRange253(_)))
        else {
            unreachable!();
        };

        NativeRangeCheckChip::<3, 253, 85>::load_k_table(
            &mut layouter,
            rangecheck253_config.k_values_table,
        )?;

        // Construct the ECC chip.
        let ecc_chip = config.ecc_chip();

        // Construct the Arithmetic chip.
        let arith_chip = config.arithmetic_chip();

        // Construct the boolean check chip.
        let boolcheck_chip = config.boolcheck_chip();

        // Construct the conditional selection chip
        let condselect_chip = config.condselect_chip();

        // Construct the zero_cond selection chip
        let zerocond_chip = config.zerocond_chip();

        // ==========================
        // Constants setup
        // ==========================

        // This constant one is used for short multiplication
        let one = assign_free_advice(
            layouter.namespace(|| "Load constant one"),
            config.advices[0],
            Value::known(pallas::Base::ONE),
        )?;
        layouter.assign_region(
            || "constrain constant",
            |mut region| region.constrain_constant(one.cell(), pallas::Base::ONE),
        )?;

        // ANCHOR: constant_init
        // Lookup and push constants onto the heap
        for constant in &self.constants {
            trace!(
                target: "zk::vm",
                "Pushing constant `{}` to heap address {}",
                constant.as_str(),
                heap.len()
            );
            match constant.as_str() {
                "VALUE_COMMIT_VALUE" => {
                    let vcv = ValueCommitV;
                    let vcv = FixedPointShort::from_inner(ecc_chip.clone(), vcv);
                    heap.push(HeapVar::EcFixedPointShort(vcv));
                }
                "VALUE_COMMIT_RANDOM" => {
                    let vcr = OrchardFixedBasesFull::ValueCommitR;
                    let vcr = FixedPoint::from_inner(ecc_chip.clone(), vcr);
                    heap.push(HeapVar::EcFixedPoint(vcr));
                }
                "NULLIFIER_K" => {
                    let nfk = NullifierK;
                    let nfk = FixedPointBaseField::from_inner(ecc_chip.clone(), nfk);
                    heap.push(HeapVar::EcFixedPointBase(nfk));
                }

                _ => {
                    error!(target: "zk::vm", "Invalid constant name: {}", constant.as_str());
                    return Err(plonk::Error::Synthesis)
                }
            }
        }
        // ANCHOR_END: constant_init

        // ANCHOR: literals_init
        // Load the literals onto the literal heap
        // N.B. Only uint64 is supported right now.
        for literal in &self.literals {
            match literal.0 {
                LitType::Uint64 => match literal.1.parse::<u64>() {
                    Ok(v) => litheap.push(v),
                    Err(e) => {
                        error!(target: "zk::vm", "Failed converting u64 literal: {}", e);
                        return Err(plonk::Error::Synthesis)
                    }
                },
                _ => {
                    error!(target: "zk::vm", "Invalid literal: {:?}", literal);
                    return Err(plonk::Error::Synthesis)
                }
            }
        }
        // ANCHOR_END: literals_init

        // ANCHOR: witness_init
        // Push the witnesses onto the heap, and potentially, if the witness
        // is in the Base field (like the entire circuit is), load it into a
        // table cell.
        for witness in &self.witnesses {
            match witness {
                Witness::EcPoint(w) => {
                    trace!(target: "zk::vm", "Witnessing EcPoint into circuit");
                    let point = Point::new(
                        ecc_chip.clone(),
                        layouter.namespace(|| "Witness EcPoint"),
                        w.as_ref().map(|cm| cm.to_affine()),
                    )?;

                    trace!(target: "zk::vm", "Pushing EcPoint to heap address {}", heap.len());
                    heap.push(HeapVar::EcPoint(point));
                }

                Witness::EcNiPoint(w) => {
                    trace!(target: "zk::vm", "Witnessing EcNiPoint into circuit");
                    let point = NonIdentityPoint::new(
                        ecc_chip.clone(),
                        layouter.namespace(|| "Witness EcNiPoint"),
                        w.as_ref().map(|cm| cm.to_affine()),
                    )?;

                    trace!(target: "zk::vm", "Pushing EcNiPoint to heap address {}", heap.len());
                    heap.push(HeapVar::EcNiPoint(point));
                }

                Witness::EcFixedPoint(_) => {
                    error!(target: "zk::vm", "Unable to witness EcFixedPoint, this is unimplemented.");
                    return Err(plonk::Error::Synthesis)
                }

                Witness::Base(w) => {
                    trace!(target: "zk::vm", "Witnessing Base into circuit");
                    let base = assign_free_advice(
                        layouter.namespace(|| "Witness Base"),
                        config.advices[0],
                        *w,
                    )?;

                    trace!(target: "zk::vm", "Pushing Base to heap address {}", heap.len());
                    heap.push(HeapVar::Base(base));
                }

                Witness::Scalar(w) => {
                    // NOTE: Because the type in `halo2_gadgets` does not have a `Clone`
                    //       impl, we push scalars as-is to the heap. They get witnessed
                    //       when they get used.
                    trace!(target: "zk::vm", "Pushing Scalar to heap address {}", heap.len());
                    heap.push(HeapVar::Scalar(*w));
                }

                Witness::MerklePath(w) => {
                    trace!(target: "zk::vm", "Witnessing MerklePath into circuit");
                    let path: Value<[pallas::Base; MERKLE_DEPTH_ORCHARD]> =
                        w.map(|typed_path| gen_const_array(|i| typed_path[i].inner()));

                    trace!(target: "zk::vm", "Pushing MerklePath to heap address {}", heap.len());
                    heap.push(HeapVar::MerklePath(path));
                }

                Witness::Uint32(w) => {
                    trace!(target: "zk::vm", "Pushing Uint32 to heap address {}", heap.len());
                    heap.push(HeapVar::Uint32(*w));
                }

                Witness::Uint64(w) => {
                    trace!(target: "zk::vm", "Pushing Uint64 to heap address {}", heap.len());
                    heap.push(HeapVar::Uint64(*w));
                }
            }
        }
        // ANCHOR_END: witness_init

        // =============================
        // And now, work through opcodes
        // =============================
        // TODO: Copy constraints
        // ANCHOR: opcode_begin
        for opcode in &self.opcodes {
            match opcode.0 {
                Opcode::EcAdd => {
                    trace!(target: "zk::vm", "Executing `EcAdd{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let lhs: Point<pallas::Affine, EccChip<OrchardFixedBases>> =
                        heap[args[0].1].clone().into();

                    let rhs: Point<pallas::Affine, EccChip<OrchardFixedBases>> =
                        heap[args[1].1].clone().into();

                    let ret = lhs.add(layouter.namespace(|| "EcAdd()"), &rhs)?;

                    trace!(target: "zk::vm", "Pushing result to heap address {}", heap.len());
                    heap.push(HeapVar::EcPoint(ret));
                }
                // ANCHOR_END: opcode_begin
                Opcode::EcMul => {
                    trace!(target: "zk::vm", "Executing `EcMul{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let lhs: FixedPoint<pallas::Affine, EccChip<OrchardFixedBases>> =
                        heap[args[1].1].clone().into();

                    let rhs = ScalarFixed::new(
                        ecc_chip.clone(),
                        layouter.namespace(|| "EcMul: ScalarFixed::new()"),
                        heap[args[0].1].clone().into(),
                    )?;

                    let (ret, _) = lhs.mul(layouter.namespace(|| "EcMul()"), rhs)?;

                    trace!(target: "zk::vm", "Pushing result to heap address {}", heap.len());
                    heap.push(HeapVar::EcPoint(ret));
                }

                Opcode::EcMulVarBase => {
                    trace!(target: "zk::vm", "Executing `EcMulVarBase{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let lhs: NonIdentityPoint<pallas::Affine, EccChip<OrchardFixedBases>> =
                        heap[args[1].1].clone().into();

                    let rhs: AssignedCell<Fp, Fp> = heap[args[0].1].clone().into();
                    let rhs = ScalarVar::from_base(
                        ecc_chip.clone(),
                        layouter.namespace(|| "EcMulVarBase::from_base()"),
                        &rhs,
                    )?;

                    let (ret, _) = lhs.mul(layouter.namespace(|| "EcMulVarBase()"), rhs)?;

                    trace!(target: "zk::vm", "Pushing result to heap address {}", heap.len());
                    heap.push(HeapVar::EcPoint(ret));
                }

                Opcode::EcMulBase => {
                    trace!(target: "zk::vm", "Executing `EcMulBase{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let lhs: FixedPointBaseField<pallas::Affine, EccChip<OrchardFixedBases>> =
                        heap[args[1].1].clone().into();

                    let rhs: AssignedCell<Fp, Fp> = heap[args[0].1].clone().into();

                    let ret = lhs.mul(layouter.namespace(|| "EcMulBase()"), rhs)?;

                    trace!(target: "zk::vm", "Pushing result to heap address {}", heap.len());
                    heap.push(HeapVar::EcPoint(ret));
                }

                Opcode::EcMulShort => {
                    trace!(target: "zk::vm", "Executing `EcMulShort{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let lhs: FixedPointShort<pallas::Affine, EccChip<OrchardFixedBases>> =
                        heap[args[1].1].clone().into();

                    let rhs = ScalarFixedShort::new(
                        ecc_chip.clone(),
                        layouter.namespace(|| "EcMulShort: ScalarFixedShort::new()"),
                        (heap[args[0].1].clone().into(), one.clone()),
                    )?;

                    let (ret, _) = lhs.mul(layouter.namespace(|| "EcMulShort()"), rhs)?;

                    trace!(target: "zk::vm", "Pushing result to heap address {}", heap.len());
                    heap.push(HeapVar::EcPoint(ret));
                }

                Opcode::EcGetX => {
                    trace!(target: "zk::vm", "Executing `EcGetX{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let point: Point<pallas::Affine, EccChip<OrchardFixedBases>> =
                        heap[args[0].1].clone().into();

                    let ret = point.inner().x();

                    trace!(target: "zk::vm", "Pushing result to heap address {}", heap.len());
                    heap.push(HeapVar::Base(ret));
                }

                Opcode::EcGetY => {
                    trace!(target: "zk::vm", "Executing `EcGetY{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let point: Point<pallas::Affine, EccChip<OrchardFixedBases>> =
                        heap[args[0].1].clone().into();

                    let ret = point.inner().y();

                    trace!(target: "zk::vm", "Pushing result to heap address {}", heap.len());
                    heap.push(HeapVar::Base(ret));
                }

                Opcode::PoseidonHash => {
                    trace!(target: "zk::vm", "Executing `PoseidonHash{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let mut poseidon_message: Vec<AssignedCell<Fp, Fp>> =
                        Vec::with_capacity(args.len());

                    for idx in args {
                        poseidon_message.push(heap[idx.1].clone().into());
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

                            trace!(target: "zk::vm", "Pushing hash to heap address {}", heap.len());
                            heap.push(HeapVar::Base($cell));
                        };
                    }

                    macro_rules! vla {
                        ($args:ident, $a:ident, $b:ident, $c:ident, $($num:tt)*) => {
                            match $args.len() {
                                $($num => {
                                    poseidon_hash!($num, $a, $b, $c);
                                })*
                                _ => {
                                    error!(target: "zk::vm", "Unsupported poseidon hash for {} elements", $args.len());
                                    return Err(plonk::Error::Synthesis)
                                }
                            }
                        };
                    }

                    vla!(args, a, b, c, 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16);
                }

                Opcode::MerkleRoot => {
                    trace!(target: "zk::vm", "Executing `MerkleRoot{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let leaf_pos = heap[args[0].1].clone().into();
                    let merkle_path = heap[args[1].1].clone().into();
                    let leaf = heap[args[2].1].clone().into();

                    let merkle_inputs = MerklePath::construct(
                        [config.merkle_chip_1(), config.merkle_chip_2()],
                        OrchardHashDomains::MerkleCrh,
                        leaf_pos,
                        merkle_path,
                    );

                    let root = merkle_inputs
                        .calculate_root(layouter.namespace(|| "MerkleRoot()"), leaf)?;

                    trace!(target: "zk::vm", "Pushing merkle root to heap address {}", heap.len());
                    heap.push(HeapVar::Base(root));
                }

                Opcode::BaseAdd => {
                    trace!(target: "zk::vm", "Executing `BaseAdd{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let lhs = &heap[args[0].1].clone().into();
                    let rhs = &heap[args[1].1].clone().into();

                    let sum = arith_chip.add(layouter.namespace(|| "BaseAdd()"), lhs, rhs)?;

                    trace!(target: "zk::vm", "Pushing sum to heap address {}", heap.len());
                    heap.push(HeapVar::Base(sum));
                }

                Opcode::BaseMul => {
                    trace!(target: "zk::vm", "Executing `BaseSub{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let lhs = &heap[args[0].1].clone().into();
                    let rhs = &heap[args[1].1].clone().into();

                    let product = arith_chip.mul(layouter.namespace(|| "BaseMul()"), lhs, rhs)?;

                    trace!(target: "zk::vm", "Pushing product to heap address {}", heap.len());
                    heap.push(HeapVar::Base(product));
                }

                Opcode::BaseSub => {
                    trace!(target: "zk::vm", "Executing `BaseSub{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let lhs = &heap[args[0].1].clone().into();
                    let rhs = &heap[args[1].1].clone().into();

                    let difference =
                        arith_chip.sub(layouter.namespace(|| "BaseSub()"), lhs, rhs)?;

                    trace!(target: "zk::vm", "Pushing difference to heap address {}", heap.len());
                    heap.push(HeapVar::Base(difference));
                }

                Opcode::WitnessBase => {
                    trace!(target: "zk::vm", "Executing `WitnessBase{:?}` opcode", opcode.1);
                    //let args = &opcode.1;

                    let lit = litheap[literals_offset];
                    literals_offset += 1;

                    let witness = assign_free_advice(
                        layouter.namespace(|| "Witness literal"),
                        config.advices[0],
                        Value::known(pallas::Base::from(lit)),
                    )?;
                    layouter.assign_region(
                        || "constrain constant",
                        |mut region| {
                            region.constrain_constant(witness.cell(), pallas::Base::from(lit))
                        },
                    )?;

                    trace!(target: "zk::vm", "Pushing assignment to heap address {}", heap.len());
                    heap.push(HeapVar::Base(witness));
                }

                Opcode::RangeCheck => {
                    trace!(target: "zk::vm", "Executing `RangeCheck{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let lit = litheap[literals_offset];
                    literals_offset += 1;

                    let arg = heap[args[1].1].clone();

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
                            error!(target: "zk::vm", "Unsupported bit-range {} for range_check", x);
                            return Err(plonk::Error::Synthesis)
                        }
                    }
                }

                Opcode::LessThanStrict => {
                    trace!(target: "zk::vm", "Executing `LessThanStrict{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let a = heap[args[0].1].clone().into();
                    let b = heap[args[1].1].clone().into();

                    lessthan_chip.copy_less_than(
                        layouter.namespace(|| "copy a<b check"),
                        a,
                        b,
                        0,
                        true,
                    )?;
                }

                Opcode::LessThanLoose => {
                    trace!(target: "zk::vm", "Executing `LessThanLoose{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let a = heap[args[0].1].clone().into();
                    let b = heap[args[1].1].clone().into();

                    lessthan_chip.copy_less_than(
                        layouter.namespace(|| "copy a<b check"),
                        a,
                        b,
                        0,
                        false,
                    )?;
                }

                Opcode::BoolCheck => {
                    trace!(target: "zk::vm", "Executing `BoolCheck{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let w = heap[args[0].1].clone().into();

                    boolcheck_chip
                        .small_range_check(layouter.namespace(|| "copy boolean check"), w)?;
                }

                Opcode::CondSelect => {
                    trace!(target: "zk::vm", "Executing `CondSelect{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let cond: AssignedCell<Fp, Fp> = heap[args[0].1].clone().into();
                    let lhs: AssignedCell<Fp, Fp> = heap[args[1].1].clone().into();
                    let rhs: AssignedCell<Fp, Fp> = heap[args[2].1].clone().into();

                    let out: AssignedCell<Fp, Fp> = condselect_chip.conditional_select(
                        &mut layouter.namespace(|| "cond_select"),
                        lhs,
                        rhs,
                        cond,
                    )?;

                    trace!(target: "zk::vm", "Pushing assignment to heap address {}", heap.len());
                    heap.push(HeapVar::Base(out));
                }

                Opcode::ZeroCondSelect => {
                    trace!(target: "zk::vm", "Executing `ZeroCondSelect{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let lhs: AssignedCell<Fp, Fp> = heap[args[0].1].clone().into();
                    let rhs: AssignedCell<Fp, Fp> = heap[args[1].1].clone().into();

                    let out: AssignedCell<Fp, Fp> =
                        zerocond_chip.assign(layouter.namespace(|| "zero_cond"), lhs, rhs)?;

                    trace!(target: "zk::vm", "Pushing assignment to heap address {}", heap.len());
                    heap.push(HeapVar::Base(out));
                }

                Opcode::ConstrainEqualBase => {
                    trace!(target: "zk::vm", "Executing `ConstrainEqualBase{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let lhs: AssignedCell<Fp, Fp> = heap[args[0].1].clone().into();
                    let rhs: AssignedCell<Fp, Fp> = heap[args[1].1].clone().into();

                    layouter.assign_region(
                        || "constrain witnessed base equality",
                        |mut region| region.constrain_equal(lhs.cell(), rhs.cell()),
                    )?;
                }

                Opcode::ConstrainEqualPoint => {
                    trace!(target: "zk::vm", "Executing `ConstrainEqualPoint{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let lhs: Point<pallas::Affine, EccChip<OrchardFixedBases>> =
                        heap[args[0].1].clone().into();

                    let rhs: Point<pallas::Affine, EccChip<OrchardFixedBases>> =
                        heap[args[1].1].clone().into();

                    lhs.constrain_equal(
                        layouter.namespace(|| "constrain ec point equality"),
                        &rhs,
                    )?;
                }

                Opcode::ConstrainInstance => {
                    trace!(target: "zk::vm", "Executing `ConstrainInstance{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let var: AssignedCell<Fp, Fp> = heap[args[0].1].clone().into();

                    layouter.constrain_instance(
                        var.cell(),
                        config.primary,
                        public_inputs_offset,
                    )?;

                    public_inputs_offset += 1;
                }

                Opcode::DebugPrint => {
                    trace!(target: "zk::vm", "Executing `DebugPrint{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    println!("[ZKVM DEBUG] HEAP INDEX: {}", args[0].1);
                    println!("[ZKVM DEBUG] {:#?}", heap[args[0].1]);
                }

                Opcode::Noop => {
                    error!(target: "zk::vm", "Unsupported opcode");
                    return Err(plonk::Error::Synthesis)
                }
            }
        }

        trace!(target: "zk::vm", "Exiting synthesize() successfully");
        Ok(())
    }
}
