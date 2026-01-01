/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use darkfi::zk::assign_free_advice;
use darkfi_sdk::{
    crypto::{
        constants::{
            sinsemilla::{OrchardCommitDomains, OrchardHashDomains},
            OrchardFixedBases,
        },
        pasta_prelude::Curve,
    },
    pasta::pallas,
};
use halo2_gadgets::{
    ecc::{
        chip::{EccChip, EccConfig},
        NonIdentityPoint, ScalarVar,
    },
    sinsemilla::chip::{SinsemillaChip, SinsemillaConfig},
    utilities::lookup_range_check::LookupRangeCheckConfig,
};
use halo2_proofs::{
    circuit::{floor_planner, Layouter, Value},
    plonk::{Advice, Circuit, Column, ConstraintSystem, Error, Instance as InstanceColumn},
};

#[derive(Clone, Debug)]
pub struct EcipConfig {
    primary: Column<InstanceColumn>,
    advices: [Column<Advice>; 10],
    ecc_config: EccConfig<OrchardFixedBases>,
    sinsemilla_config:
        SinsemillaConfig<OrchardHashDomains, OrchardCommitDomains, OrchardFixedBases>,
}

impl EcipConfig {
    fn ecc_chip(&self) -> EccChip<OrchardFixedBases> {
        EccChip::construct(self.ecc_config.clone())
    }
}

#[derive(Default, Debug)]
pub struct EcipCircuit {
    pub g1: Value<pallas::Point>,
    pub s1: Value<pallas::Base>,
}

impl Circuit<pallas::Base> for EcipCircuit {
    type Config = EcipConfig;
    type FloorPlanner = floor_planner::V1;

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
        meta.enable_equality(primary);

        // Permutation over all advice columns
        for advice in advices.iter() {
            meta.enable_equality(*advice);
        }

        // Fixed columns for the ECC chip
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

        // Use the first Lagrange coefficient column for loading global constants.
        meta.enable_constant(lagrange_coeffs[0]);

        // Use one of the right-most advice columns for all of our range checks.
        let range_check = LookupRangeCheckConfig::configure(meta, advices[9], table_idx);

        // Sinsemilla configuration, used for the lookup table
        let sinsemilla_config = SinsemillaChip::configure(
            meta,
            advices[..5].try_into().unwrap(),
            advices[6],
            lagrange_coeffs[0],
            lookup,
            range_check,
        );

        // Configuration for curve point operations.
        // This uses 10 advice columns and spans the whole circuit.
        let ecc_config =
            EccChip::<OrchardFixedBases>::configure(meta, advices, lagrange_coeffs, range_check);

        EcipConfig { primary, advices, ecc_config, sinsemilla_config }
    }

    fn synthesize(
        &self,
        config: Self::Config,
        mut layouter: impl Layouter<pallas::Base>,
    ) -> Result<(), Error> {
        // Load the Sinsemilla generator lookup table used by the whole circuit
        SinsemillaChip::load(config.sinsemilla_config.clone(), &mut layouter)?;

        let g1 = NonIdentityPoint::new(
            config.ecc_chip(),
            layouter.namespace(|| "Witness g1"),
            self.g1.as_ref().map(|cm| cm.to_affine()),
        )?;

        let s1 =
            assign_free_advice(layouter.namespace(|| "Witness s1"), config.advices[0], self.s1)?;

        let s1 =
            ScalarVar::from_base(config.ecc_chip(), layouter.namespace(|| "fp_mod_fv(s1)"), &s1)?;

        let (r, _) = g1.mul(layouter.namespace(|| "g1 * s1"), s1)?;

        let r_x = r.inner().x();
        let r_y = r.inner().y();

        layouter.constrain_instance(r_x.cell(), config.primary, 0)?;
        layouter.constrain_instance(r_y.cell(), config.primary, 1)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use darkfi_sdk::crypto::{pasta_prelude::Group, util::fp_mod_fv};
    use halo2_proofs::{
        arithmetic::{CurveAffine, Field},
        dev::MockProver,
    };
    use rand::rngs::OsRng;

    #[test]
    fn test_circuit() {
        let k = 11;

        let g1 = pallas::Point::random(&mut OsRng);
        let s1 = pallas::Base::random(&mut OsRng);

        let circuit = EcipCircuit { g1: Value::known(g1), s1: Value::known(s1) };

        let g1s1 = g1 * fp_mod_fv(s1);
        let g1s1_coords = g1s1.to_affine().coordinates().unwrap();

        let public_inputs = vec![*g1s1_coords.x(), *g1s1_coords.y()];
        let prover = MockProver::run(k, &circuit, vec![public_inputs]).unwrap();
        prover.assert_satisfied();
    }
}
