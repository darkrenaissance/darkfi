/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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

use std::collections::HashSet;

use darkfi_sdk::crypto::{
    constants::{
        sinsemilla::{OrchardCommitDomains, OrchardHashDomains, K},
        util::gen_const_array,
        ConstBaseFieldElement, OrchardFixedBases, OrchardFixedBasesFull, ValueCommitV,
        MERKLE_DEPTH_ORCHARD,
    },
    smt::SMT_FP_DEPTH,
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
        smt,
        zero_cond::{ZeroCondChip, ZeroCondConfig},
    },
    tracer::ZkTracer,
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

    /// Sparse merkle tree (using Poseidon)
    SparseTree(smt::PathConfig),

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
    NativeRange64(NativeRangeCheckConfig<K, 64>),

    /// 253 bit native range check
    NativeRange253(NativeRangeCheckConfig<K, 253>),

    /// 253 bit `a < b` check
    LessThan(LessThanConfig<K, 253>),

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
    /// Advice column used to witness values
    witness: Column<Advice>,
}

impl VmConfig {
    fn ecc_chip(&self) -> Option<EccChip<OrchardFixedBases>> {
        let Some(VmChip::Ecc(ecc_config)) =
            self.chips.iter().find(|&c| matches!(c, VmChip::Ecc(_)))
        else {
            return None
        };

        Some(EccChip::construct(ecc_config.clone()))
    }

    fn merkle_chip_1(
        &self,
    ) -> Option<MerkleChip<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>> {
        let Some(VmChip::Merkle((merkle_cfg1, _))) =
            self.chips.iter().find(|&c| matches!(c, VmChip::Merkle(_)))
        else {
            return None
        };

        Some(MerkleChip::construct(merkle_cfg1.clone()))
    }

    fn merkle_chip_2(
        &self,
    ) -> Option<MerkleChip<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>> {
        let Some(VmChip::Merkle((_, merkle_cfg2))) =
            self.chips.iter().find(|&c| matches!(c, VmChip::Merkle(_)))
        else {
            return None
        };

        Some(MerkleChip::construct(merkle_cfg2.clone()))
    }

    fn smt_chip(&self) -> Option<smt::PathChip> {
        let Some(VmChip::SparseTree(config)) =
            self.chips.iter().find(|&c| matches!(c, VmChip::SparseTree(_)))
        else {
            return None
        };

        Some(smt::PathChip::construct(config.clone()))
    }

    fn poseidon_chip(&self) -> Option<PoseidonChip<pallas::Base, 3, 2>> {
        let Some(VmChip::Poseidon(poseidon_config)) =
            self.chips.iter().find(|&c| matches!(c, VmChip::Poseidon(_)))
        else {
            return None
        };

        Some(PoseidonChip::construct(poseidon_config.clone()))
    }

    fn arithmetic_chip(&self) -> Option<ArithChip<pallas::Base>> {
        let Some(VmChip::Arithmetic(arith_config)) =
            self.chips.iter().find(|&c| matches!(c, VmChip::Arithmetic(_)))
        else {
            return None
        };

        Some(ArithChip::construct(arith_config.clone()))
    }

    fn condselect_chip(&self) -> Option<ConditionalSelectChip<pallas::Base>> {
        let Some(VmChip::CondSelect(condselect_config)) =
            self.chips.iter().find(|&c| matches!(c, VmChip::CondSelect(_)))
        else {
            return None
        };

        Some(ConditionalSelectChip::construct(condselect_config.clone()))
    }

    fn zerocond_chip(&self) -> Option<ZeroCondChip<pallas::Base>> {
        let Some(VmChip::ZeroCond(zerocond_config)) =
            self.chips.iter().find(|&c| matches!(c, VmChip::ZeroCond(_)))
        else {
            return None
        };

        Some(ZeroCondChip::construct(zerocond_config.clone()))
    }

    fn rangecheck64_chip(&self) -> Option<NativeRangeCheckChip<K, 64>> {
        let Some(VmChip::NativeRange64(range_config)) =
            self.chips.iter().find(|&c| matches!(c, VmChip::NativeRange64(_)))
        else {
            return None
        };

        Some(NativeRangeCheckChip::construct(range_config.clone()))
    }

    fn rangecheck253_chip(&self) -> Option<NativeRangeCheckChip<K, 253>> {
        let Some(VmChip::NativeRange253(range_config)) =
            self.chips.iter().find(|&c| matches!(c, VmChip::NativeRange253(_)))
        else {
            return None
        };

        Some(NativeRangeCheckChip::construct(range_config.clone()))
    }

    fn lessthan_chip(&self) -> Option<LessThanChip<K, 253>> {
        let Some(VmChip::LessThan(lessthan_config)) =
            self.chips.iter().find(|&c| matches!(c, VmChip::LessThan(_)))
        else {
            return None
        };

        Some(LessThanChip::construct(lessthan_config.clone()))
    }

    fn boolcheck_chip(&self) -> Option<SmallRangeCheckChip<pallas::Base>> {
        let Some(VmChip::BoolCheck(boolcheck_config)) =
            self.chips.iter().find(|&c| matches!(c, VmChip::BoolCheck(_)))
        else {
            return None
        };

        Some(SmallRangeCheckChip::construct(boolcheck_config.clone()))
    }
}

/// Configuration parameters for the circuit.
/// Defines which chips we need to initialize and configure.
#[derive(Default)]
#[allow(dead_code)]
pub struct ZkParams {
    init_ecc: bool,
    init_poseidon: bool,
    init_sinsemilla: bool,
    init_arithmetic: bool,
    init_nativerange: bool,
    init_lessthan: bool,
    init_boolcheck: bool,
    init_condselect: bool,
    init_zerocond: bool,
}

#[derive(Clone)]
pub struct ZkCircuit {
    constants: Vec<String>,
    pub(super) witnesses: Vec<Witness>,
    literals: Vec<(LitType, String)>,
    pub(super) opcodes: Vec<(Opcode, Vec<(HeapType, usize)>)>,
    pub tracer: ZkTracer,
}

impl ZkCircuit {
    pub fn new(witnesses: Vec<Witness>, circuit_code: &ZkBinary) -> Self {
        let constants = circuit_code.constants.iter().map(|x| x.1.clone()).collect();
        let literals = circuit_code.literals.clone();
        Self {
            constants,
            witnesses,
            literals,
            opcodes: circuit_code.opcodes.clone(),
            tracer: ZkTracer::new(true),
        }
    }

    pub fn enable_trace(&mut self) {
        self.tracer.init();
    }
}

impl Circuit<pallas::Base> for ZkCircuit {
    type Config = VmConfig;
    type FloorPlanner = floor_planner::V1;
    type Params = ZkParams;

    fn without_witnesses(&self) -> Self {
        Self {
            constants: self.constants.clone(),
            witnesses: self.witnesses.clone(),
            literals: self.literals.clone(),
            opcodes: self.opcodes.clone(),
            tracer: ZkTracer::new(false),
        }
    }

    fn configure(_meta: &mut ConstraintSystem<pallas::Base>) -> Self::Config {
        unreachable!();
    }

    fn params(&self) -> Self::Params {
        // Gather all opcodes used in the circuit.
        let mut opcodes = HashSet::new();
        for (opcode, _) in &self.opcodes {
            opcodes.insert(opcode);
        }

        // Conditions on which we enable the ECC chip
        let init_ecc = !self.constants.is_empty() ||
            opcodes.contains(&Opcode::EcAdd) ||
            opcodes.contains(&Opcode::EcMul) ||
            opcodes.contains(&Opcode::EcMulBase) ||
            opcodes.contains(&Opcode::EcMulShort) ||
            opcodes.contains(&Opcode::EcMulVarBase) ||
            opcodes.contains(&Opcode::EcGetX) ||
            opcodes.contains(&Opcode::EcGetY) ||
            opcodes.contains(&Opcode::ConstrainEqualPoint) ||
            self.witnesses.iter().any(|x| {
                matches!(x, Witness::EcPoint(_)) ||
                    matches!(x, Witness::EcNiPoint(_)) ||
                    matches!(x, Witness::EcFixedPoint(_)) ||
                    matches!(x, Witness::Scalar(_))
            });

        // Conditions on which we enable the Poseidon hash chip
        let init_poseidon = opcodes.contains(&Opcode::PoseidonHash);

        // Conditions on which we enable the Sinsemilla and Merkle chips
        let init_sinsemilla = opcodes.contains(&Opcode::MerkleRoot);

        // Conditions on which we enable the base field Arithmetic chip
        let init_arithmetic = opcodes.contains(&Opcode::BaseAdd) ||
            opcodes.contains(&Opcode::BaseSub) ||
            opcodes.contains(&Opcode::BaseMul);

        // Conditions on which we enable the native range check chips
        // TODO: Separate 253 and 64.
        let init_nativerange = opcodes.contains(&Opcode::RangeCheck) ||
            opcodes.contains(&Opcode::LessThanLoose) ||
            opcodes.contains(&Opcode::LessThanStrict);

        // Conditions on which we enable the less than comparison chip
        let init_lessthan =
            opcodes.contains(&Opcode::LessThanLoose) || opcodes.contains(&Opcode::LessThanStrict);

        // Conditions on which we enable the boolean check chip
        let init_boolcheck = opcodes.contains(&Opcode::BoolCheck);

        // Conditions on which we enable the conditional selection chip
        let init_condselect = opcodes.contains(&Opcode::CondSelect);

        // Conditions on which we enable the zero cond selection chip
        let init_zerocond = opcodes.contains(&Opcode::ZeroCondSelect);

        ZkParams {
            init_ecc,
            init_poseidon,
            init_sinsemilla,
            init_arithmetic,
            init_nativerange,
            init_lessthan,
            init_boolcheck,
            init_condselect,
            init_zerocond,
        }
    }

    fn configure_with_params(
        meta: &mut ConstraintSystem<pallas::Base>,
        _params: Self::Params,
    ) -> Self::Config {
        // Advice columns used in the circuit
        let mut advices = vec![];
        for _ in 0..10 {
            advices.push(meta.advice_column());
        }

        // Instance column used for public inputs
        let primary = meta.instance_column();
        meta.enable_equality(primary);

        // Permutation over all advice columns
        for advice in advices.iter() {
            meta.enable_equality(*advice);
        }

        // Fixed columns for the Sinsemilla generator lookup table
        let table_idx = meta.lookup_table_column();
        let lookup = (table_idx, meta.lookup_table_column(), meta.lookup_table_column());

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
            advices[0..10].try_into().unwrap(),
            lagrange_coeffs,
            range_check,
        );

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

        let smt_config = smt::PathChip::configure(
            meta,
            advices[0..2].try_into().unwrap(),
            advices[2..6].try_into().unwrap(),
            poseidon_config.clone(),
        );

        // K-table for 64 bit range check lookups
        let native_64_range_check_config =
            NativeRangeCheckChip::<K, 64>::configure(meta, advices[8], table_idx);

        // K-table for 253 bit range check lookups
        let native_253_range_check_config =
            NativeRangeCheckChip::<K, 253>::configure(meta, advices[8], table_idx);

        // TODO: FIXME: Configure these better, this is just a stop-gap
        let z1 = meta.advice_column();
        let z2 = meta.advice_column();

        let lessthan_config = LessThanChip::<K, 253>::configure(
            meta, advices[6], advices[7], advices[8], z1, z2, table_idx,
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
            VmChip::SparseTree(smt_config),
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

        VmConfig { primary, witness: advices[0], chips }
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
        if let Some(VmChip::Sinsemilla((sinsemilla_cfg1, _))) =
            config.chips.iter().find(|&c| matches!(c, VmChip::Sinsemilla(_)))
        {
            trace!(target: "zk::vm", "Initializing Sinsemilla generator lookup table");
            SinsemillaChip::load(sinsemilla_cfg1.clone(), &mut layouter)?;
        }

        let no_sinsemilla_chip = !config.chips.iter().any(|c| matches!(c, VmChip::Sinsemilla(_)));

        // Construct the 64-bit NativeRangeCheck chip
        let rangecheck64_chip = config.rangecheck64_chip();
        if let Some(VmChip::NativeRange64(rangecheck64_config)) =
            config.chips.iter().find(|&c| matches!(c, VmChip::NativeRange64(_)))
        {
            if no_sinsemilla_chip {
                trace!(target: "zk::vm", "Initializing k table for 64bit NativeRangeCheck");
                NativeRangeCheckChip::<K, 64>::load_k_table(
                    &mut layouter,
                    rangecheck64_config.k_values_table,
                )?;
            }
        }

        let no_rangecheck64_chip =
            !config.chips.iter().any(|c| matches!(c, VmChip::NativeRange64(_)));

        // Construct the 253-bit NativeRangeCheck and LessThan chips.
        let rangecheck253_chip = config.rangecheck253_chip();
        let lessthan_chip = config.lessthan_chip();

        if let Some(VmChip::NativeRange253(rangecheck253_config)) =
            config.chips.iter().find(|&c| matches!(c, VmChip::NativeRange253(_)))
        {
            if no_sinsemilla_chip && no_rangecheck64_chip {
                trace!(target: "zk::vm", "Initializing k table for 253bit NativeRangeCheck");
                NativeRangeCheckChip::<K, 253>::load_k_table(
                    &mut layouter,
                    rangecheck253_config.k_values_table,
                )?;
            }
        }

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

        // Construct sparse Merkle tree chip
        let smt_chip = config.smt_chip().unwrap();

        // ==========================
        // Constants setup
        // ==========================

        // This constant one is used for short multiplication
        let one = assign_free_advice(
            layouter.namespace(|| "Load constant one"),
            config.witness,
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
                    let vcv = FixedPointShort::from_inner(ecc_chip.as_ref().unwrap().clone(), vcv);
                    heap.push(HeapVar::EcFixedPointShort(vcv));
                }
                "VALUE_COMMIT_RANDOM" => {
                    let vcr = OrchardFixedBasesFull::ValueCommitR;
                    let vcr = FixedPoint::from_inner(ecc_chip.as_ref().unwrap().clone(), vcr);
                    heap.push(HeapVar::EcFixedPoint(vcr));
                }
                "VALUE_COMMIT_RANDOM_BASE" => {
                    let vcr = ConstBaseFieldElement::value_commit_r();
                    let vcr =
                        FixedPointBaseField::from_inner(ecc_chip.as_ref().unwrap().clone(), vcr);
                    heap.push(HeapVar::EcFixedPointBase(vcr));
                }
                "NULLIFIER_K" => {
                    let nfk = ConstBaseFieldElement::nullifier_k();
                    let nfk =
                        FixedPointBaseField::from_inner(ecc_chip.as_ref().unwrap().clone(), nfk);
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
                        ecc_chip.as_ref().unwrap().clone(),
                        layouter.namespace(|| "Witness EcPoint"),
                        w.as_ref().map(|cm| cm.to_affine()),
                    )?;

                    trace!(target: "zk::vm", "Pushing EcPoint to heap address {}", heap.len());
                    heap.push(HeapVar::EcPoint(point));
                }

                Witness::EcNiPoint(w) => {
                    trace!(target: "zk::vm", "Witnessing EcNiPoint into circuit");
                    let point = NonIdentityPoint::new(
                        ecc_chip.as_ref().unwrap().clone(),
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
                        config.witness,
                        *w,
                    )?;

                    trace!(target: "zk::vm", "Pushing Base to heap address {}", heap.len());
                    heap.push(HeapVar::Base(base));
                }

                Witness::Scalar(w) => {
                    trace!(target: "zk::vm", "Witnessing Scalar into circuit");
                    let scalar = ScalarFixed::new(
                        ecc_chip.as_ref().unwrap().clone(),
                        layouter.namespace(|| "Witness ScalarFixed"),
                        *w,
                    )?;

                    trace!(target: "zk::vm", "Pushing Scalar to heap address {}", heap.len());
                    heap.push(HeapVar::Scalar(scalar));
                }

                Witness::MerklePath(w) => {
                    trace!(target: "zk::vm", "Witnessing MerklePath into circuit");
                    let path: Value<[pallas::Base; MERKLE_DEPTH_ORCHARD]> =
                        w.map(|typed_path| gen_const_array(|i| typed_path[i].inner()));

                    trace!(target: "zk::vm", "Pushing MerklePath to heap address {}", heap.len());
                    heap.push(HeapVar::MerklePath(path));
                }

                Witness::SparseMerklePath(w) => {
                    let path: Value<[pallas::Base; SMT_FP_DEPTH]> =
                        w.map(|typed_path| gen_const_array(|i| typed_path[i]));

                    trace!(target: "zk::vm", "Pushing SparseMerklePath to heap address {}", heap.len());
                    heap.push(HeapVar::SparseMerklePath(path));
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
        self.tracer.clear();
        // TODO: Copy constraints
        // ANCHOR: opcode_begin
        for opcode in &self.opcodes {
            match opcode.0 {
                Opcode::EcAdd => {
                    trace!(target: "zk::vm", "Executing `EcAdd{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let lhs: Point<pallas::Affine, EccChip<OrchardFixedBases>> =
                        heap[args[0].1].clone().try_into()?;

                    let rhs: Point<pallas::Affine, EccChip<OrchardFixedBases>> =
                        heap[args[1].1].clone().try_into()?;

                    let ret = lhs.add(layouter.namespace(|| "EcAdd()"), &rhs)?;

                    trace!(target: "zk::vm", "Pushing result to heap address {}", heap.len());
                    self.tracer.push_ecpoint(&ret);
                    heap.push(HeapVar::EcPoint(ret));
                }
                // ANCHOR_END: opcode_begin
                Opcode::EcMul => {
                    trace!(target: "zk::vm", "Executing `EcMul{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let lhs: FixedPoint<pallas::Affine, EccChip<OrchardFixedBases>> =
                        heap[args[1].1].clone().try_into()?;

                    let rhs: ScalarFixed<pallas::Affine, EccChip<OrchardFixedBases>> =
                        heap[args[0].1].clone().try_into()?;

                    let (ret, _) = lhs.mul(layouter.namespace(|| "EcMul()"), rhs)?;

                    trace!(target: "zk::vm", "Pushing result to heap address {}", heap.len());
                    self.tracer.push_ecpoint(&ret);
                    heap.push(HeapVar::EcPoint(ret));
                }

                Opcode::EcMulVarBase => {
                    trace!(target: "zk::vm", "Executing `EcMulVarBase{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let lhs: NonIdentityPoint<pallas::Affine, EccChip<OrchardFixedBases>> =
                        heap[args[1].1].clone().try_into()?;

                    let rhs: AssignedCell<Fp, Fp> = heap[args[0].1].clone().try_into()?;
                    let rhs = ScalarVar::from_base(
                        ecc_chip.as_ref().unwrap().clone(),
                        layouter.namespace(|| "EcMulVarBase::from_base()"),
                        &rhs,
                    )?;

                    let (ret, _) = lhs.mul(layouter.namespace(|| "EcMulVarBase()"), rhs)?;

                    trace!(target: "zk::vm", "Pushing result to heap address {}", heap.len());
                    self.tracer.push_ecpoint(&ret);
                    heap.push(HeapVar::EcPoint(ret));
                }

                Opcode::EcMulBase => {
                    trace!(target: "zk::vm", "Executing `EcMulBase{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let lhs: FixedPointBaseField<pallas::Affine, EccChip<OrchardFixedBases>> =
                        heap[args[1].1].clone().try_into()?;

                    let rhs: AssignedCell<Fp, Fp> = heap[args[0].1].clone().try_into()?;

                    let ret = lhs.mul(layouter.namespace(|| "EcMulBase()"), rhs)?;

                    trace!(target: "zk::vm", "Pushing result to heap address {}", heap.len());
                    self.tracer.push_ecpoint(&ret);
                    heap.push(HeapVar::EcPoint(ret));
                }

                Opcode::EcMulShort => {
                    trace!(target: "zk::vm", "Executing `EcMulShort{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let lhs: FixedPointShort<pallas::Affine, EccChip<OrchardFixedBases>> =
                        heap[args[1].1].clone().try_into()?;

                    let rhs = ScalarFixedShort::new(
                        ecc_chip.as_ref().unwrap().clone(),
                        layouter.namespace(|| "EcMulShort: ScalarFixedShort::new()"),
                        (heap[args[0].1].clone().try_into()?, one.clone()),
                    )?;

                    let (ret, _) = lhs.mul(layouter.namespace(|| "EcMulShort()"), rhs)?;

                    trace!(target: "zk::vm", "Pushing result to heap address {}", heap.len());
                    self.tracer.push_ecpoint(&ret);
                    heap.push(HeapVar::EcPoint(ret));
                }

                Opcode::EcGetX => {
                    trace!(target: "zk::vm", "Executing `EcGetX{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let point: Point<pallas::Affine, EccChip<OrchardFixedBases>> =
                        heap[args[0].1].clone().try_into()?;

                    let ret = point.inner().x();

                    trace!(target: "zk::vm", "Pushing result to heap address {}", heap.len());
                    self.tracer.push_base(&ret);
                    heap.push(HeapVar::Base(ret));
                }

                Opcode::EcGetY => {
                    trace!(target: "zk::vm", "Executing `EcGetY{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let point: Point<pallas::Affine, EccChip<OrchardFixedBases>> =
                        heap[args[0].1].clone().try_into()?;

                    let ret = point.inner().y();

                    trace!(target: "zk::vm", "Pushing result to heap address {}", heap.len());
                    self.tracer.push_base(&ret);
                    heap.push(HeapVar::Base(ret));
                }

                Opcode::PoseidonHash => {
                    trace!(target: "zk::vm", "Executing `PoseidonHash{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let mut poseidon_message: Vec<AssignedCell<Fp, Fp>> =
                        Vec::with_capacity(args.len());

                    for idx in args {
                        poseidon_message.push(heap[idx.1].clone().try_into()?);
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
                                config.poseidon_chip().unwrap(),
                                layouter.namespace(|| "PoseidonHash init"),
                            )?;

                            let $output = $hasher.hash(
                                layouter.namespace(|| "PoseidonHash hash"),
                                poseidon_message.try_into().unwrap(),
                            )?;

                            let $cell: AssignedCell<Fp, Fp> = $output.into();

                            trace!(target: "zk::vm", "Pushing hash to heap address {}", heap.len());
                            self.tracer.push_base(&$cell);
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

                    vla!(args, a, b, c, 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24);
                }

                Opcode::MerkleRoot => {
                    // TODO: all these trace statements could have trace!(..., args) instead
                    trace!(target: "zk::vm", "Executing `MerkleRoot{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let leaf_pos = heap[args[0].1].clone().try_into()?;
                    let merkle_path: Value<[Fp; MERKLE_DEPTH_ORCHARD]> =
                        heap[args[1].1].clone().try_into()?;
                    let leaf = heap[args[2].1].clone().try_into()?;

                    let merkle_inputs = MerklePath::construct(
                        [config.merkle_chip_1().unwrap(), config.merkle_chip_2().unwrap()],
                        OrchardHashDomains::MerkleCrh,
                        leaf_pos,
                        merkle_path,
                    );

                    let root = merkle_inputs
                        .calculate_root(layouter.namespace(|| "MerkleRoot()"), leaf)?;

                    trace!(target: "zk::vm", "Pushing merkle root to heap address {}", heap.len());
                    self.tracer.push_base(&root);
                    heap.push(HeapVar::Base(root));
                }

                Opcode::SparseMerkleRoot => {
                    trace!(target: "zk::vm", "Executing `SparseTreeIsMember{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let pos = heap[args[0].1].clone().try_into()?;
                    let path: Value<[Fp; SMT_FP_DEPTH]> = heap[args[1].1].clone().try_into()?;
                    let leaf = heap[args[2].1].clone().try_into()?;

                    let root = smt_chip.check_membership(&mut layouter, pos, path, leaf)?;

                    self.tracer.push_base(&root);
                    heap.push(HeapVar::Base(root));
                }

                Opcode::BaseAdd => {
                    trace!(target: "zk::vm", "Executing `BaseAdd{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let lhs = &heap[args[0].1].clone().try_into()?;
                    let rhs = &heap[args[1].1].clone().try_into()?;

                    let sum = arith_chip.as_ref().unwrap().add(
                        layouter.namespace(|| "BaseAdd()"),
                        lhs,
                        rhs,
                    )?;

                    trace!(target: "zk::vm", "Pushing sum to heap address {}", heap.len());
                    self.tracer.push_base(&sum);
                    heap.push(HeapVar::Base(sum));
                }

                Opcode::BaseMul => {
                    trace!(target: "zk::vm", "Executing `BaseSub{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let lhs = &heap[args[0].1].clone().try_into()?;
                    let rhs = &heap[args[1].1].clone().try_into()?;

                    let product = arith_chip.as_ref().unwrap().mul(
                        layouter.namespace(|| "BaseMul()"),
                        lhs,
                        rhs,
                    )?;

                    trace!(target: "zk::vm", "Pushing product to heap address {}", heap.len());
                    self.tracer.push_base(&product);
                    heap.push(HeapVar::Base(product));
                }

                Opcode::BaseSub => {
                    trace!(target: "zk::vm", "Executing `BaseSub{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let lhs = &heap[args[0].1].clone().try_into()?;
                    let rhs = &heap[args[1].1].clone().try_into()?;

                    let difference = arith_chip.as_ref().unwrap().sub(
                        layouter.namespace(|| "BaseSub()"),
                        lhs,
                        rhs,
                    )?;

                    trace!(target: "zk::vm", "Pushing difference to heap address {}", heap.len());
                    self.tracer.push_base(&difference);
                    heap.push(HeapVar::Base(difference));
                }

                Opcode::WitnessBase => {
                    trace!(target: "zk::vm", "Executing `WitnessBase{:?}` opcode", opcode.1);
                    //let args = &opcode.1;

                    let lit = litheap[literals_offset];
                    literals_offset += 1;

                    let witness = assign_free_advice(
                        layouter.namespace(|| "Witness literal"),
                        config.witness,
                        Value::known(pallas::Base::from(lit)),
                    )?;

                    layouter.assign_region(
                        || "constrain constant",
                        |mut region| {
                            region.constrain_constant(witness.cell(), pallas::Base::from(lit))
                        },
                    )?;

                    trace!(target: "zk::vm", "Pushing assignment to heap address {}", heap.len());
                    self.tracer.push_base(&witness);
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
                            rangecheck64_chip.as_ref().unwrap().copy_range_check(
                                layouter.namespace(|| "copy range check 64"),
                                arg.try_into()?,
                            )?;
                        }
                        253 => {
                            rangecheck253_chip.as_ref().unwrap().copy_range_check(
                                layouter.namespace(|| "copy range check 253"),
                                arg.try_into()?,
                            )?;
                        }
                        x => {
                            error!(target: "zk::vm", "Unsupported bit-range {} for range_check", x);
                            return Err(plonk::Error::Synthesis)
                        }
                    }
                    self.tracer.push_void();
                }

                Opcode::LessThanStrict => {
                    trace!(target: "zk::vm", "Executing `LessThanStrict{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let a = heap[args[0].1].clone().try_into()?;
                    let b = heap[args[1].1].clone().try_into()?;

                    lessthan_chip.as_ref().unwrap().copy_less_than(
                        layouter.namespace(|| "copy a<b check"),
                        a,
                        b,
                        0,
                        true,
                    )?;
                    self.tracer.push_void();
                }

                Opcode::LessThanLoose => {
                    trace!(target: "zk::vm", "Executing `LessThanLoose{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let a = heap[args[0].1].clone().try_into()?;
                    let b = heap[args[1].1].clone().try_into()?;

                    lessthan_chip.as_ref().unwrap().copy_less_than(
                        layouter.namespace(|| "copy a<b check"),
                        a,
                        b,
                        0,
                        false,
                    )?;
                    self.tracer.push_void();
                }

                Opcode::BoolCheck => {
                    trace!(target: "zk::vm", "Executing `BoolCheck{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let w = heap[args[0].1].clone().try_into()?;

                    boolcheck_chip
                        .as_ref()
                        .unwrap()
                        .small_range_check(layouter.namespace(|| "copy boolean check"), w)?;
                    self.tracer.push_void();
                }

                Opcode::CondSelect => {
                    trace!(target: "zk::vm", "Executing `CondSelect{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let cond: AssignedCell<Fp, Fp> = heap[args[0].1].clone().try_into()?;
                    let lhs: AssignedCell<Fp, Fp> = heap[args[1].1].clone().try_into()?;
                    let rhs: AssignedCell<Fp, Fp> = heap[args[2].1].clone().try_into()?;

                    let out: AssignedCell<Fp, Fp> =
                        condselect_chip.as_ref().unwrap().conditional_select(
                            &mut layouter.namespace(|| "cond_select"),
                            lhs,
                            rhs,
                            cond,
                        )?;

                    trace!(target: "zk::vm", "Pushing assignment to heap address {}", heap.len());
                    self.tracer.push_base(&out);
                    heap.push(HeapVar::Base(out));
                }

                Opcode::ZeroCondSelect => {
                    trace!(target: "zk::vm", "Executing `ZeroCondSelect{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let lhs: AssignedCell<Fp, Fp> = heap[args[0].1].clone().try_into()?;
                    let rhs: AssignedCell<Fp, Fp> = heap[args[1].1].clone().try_into()?;

                    let out: AssignedCell<Fp, Fp> = zerocond_chip.as_ref().unwrap().assign(
                        layouter.namespace(|| "zero_cond"),
                        lhs,
                        rhs,
                    )?;

                    trace!(target: "zk::vm", "Pushing assignment to heap address {}", heap.len());
                    self.tracer.push_base(&out);
                    heap.push(HeapVar::Base(out));
                }

                Opcode::ConstrainEqualBase => {
                    trace!(target: "zk::vm", "Executing `ConstrainEqualBase{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let lhs: AssignedCell<Fp, Fp> = heap[args[0].1].clone().try_into()?;
                    let rhs: AssignedCell<Fp, Fp> = heap[args[1].1].clone().try_into()?;

                    layouter.assign_region(
                        || "constrain witnessed base equality",
                        |mut region| region.constrain_equal(lhs.cell(), rhs.cell()),
                    )?;
                    self.tracer.push_void();
                }

                Opcode::ConstrainEqualPoint => {
                    trace!(target: "zk::vm", "Executing `ConstrainEqualPoint{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let lhs: Point<pallas::Affine, EccChip<OrchardFixedBases>> =
                        heap[args[0].1].clone().try_into()?;

                    let rhs: Point<pallas::Affine, EccChip<OrchardFixedBases>> =
                        heap[args[1].1].clone().try_into()?;

                    lhs.constrain_equal(
                        layouter.namespace(|| "constrain ec point equality"),
                        &rhs,
                    )?;
                    self.tracer.push_void();
                }

                Opcode::ConstrainInstance => {
                    trace!(target: "zk::vm", "Executing `ConstrainInstance{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    let var: AssignedCell<Fp, Fp> = heap[args[0].1].clone().try_into()?;

                    layouter.constrain_instance(
                        var.cell(),
                        config.primary,
                        public_inputs_offset,
                    )?;

                    public_inputs_offset += 1;
                    self.tracer.push_void();
                }

                Opcode::DebugPrint => {
                    trace!(target: "zk::vm", "Executing `DebugPrint{:?}` opcode", opcode.1);
                    let args = &opcode.1;

                    println!("[ZKVM DEBUG] HEAP INDEX: {}", args[0].1);
                    println!("[ZKVM DEBUG] {:#?}", heap[args[0].1]);
                    self.tracer.push_void();
                }

                Opcode::Noop => {
                    error!(target: "zk::vm", "Unsupported opcode");
                    return Err(plonk::Error::Synthesis)
                }
            }
        }
        self.tracer.assert_correct(self.opcodes.len());

        trace!(target: "zk::vm", "Exiting synthesize() successfully");
        Ok(())
    }
}
