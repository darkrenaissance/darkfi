use super::super::{copy, CellValue, EccConfig, Var};
use super::Z;
use crate::{
    circuit::gadget::utilities::lookup_range_check::LookupRangeCheckConfig, constants::T_Q,
    primitives::sinsemilla,
};
use halo2::{
    circuit::Layouter,
    plonk::{Advice, Column, ConstraintSystem, Error, Expression, Selector},
    poly::Rotation,
};

use ff::Field;
use pasta_curves::{arithmetic::FieldExt, pallas};

use std::iter;

pub struct Config {
    // Selector to check z_0 = alpha + t_q (mod p)
    q_mul_overflow: Selector,
    // 10-bit lookup table
    lookup_config: LookupRangeCheckConfig<pallas::Base, { sinsemilla::K }>,
    // Advice columns
    advices: [Column<Advice>; 3],
}

impl From<&EccConfig> for Config {
    fn from(ecc_config: &EccConfig) -> Self {
        Self {
            q_mul_overflow: ecc_config.q_mul_overflow,
            lookup_config: ecc_config.lookup_config.clone(),
            // Use advice columns that don't conflict with the either the incomplete
            // additions in fixed-base scalar mul, or the lookup range checks.
            advices: [
                ecc_config.advices[6],
                ecc_config.advices[7],
                ecc_config.advices[8],
            ],
        }
    }
}

impl Config {
    pub(super) fn create_gate(&self, meta: &mut ConstraintSystem<pallas::Base>) {
        meta.create_gate("overflow checks", |meta| {
            let q_mul_overflow = meta.query_selector(self.q_mul_overflow);

            // Constant expressions
            let one = Expression::Constant(pallas::Base::one());
            let two_pow_124 = Expression::Constant(pallas::Base::from_u128(1 << 124));
            let two_pow_130 =
                two_pow_124.clone() * Expression::Constant(pallas::Base::from_u128(1 << 6));

            let z_0 = meta.query_advice(self.advices[0], Rotation::prev());
            let z_130 = meta.query_advice(self.advices[0], Rotation::cur());
            let eta = meta.query_advice(self.advices[0], Rotation::next());

            let k_254 = meta.query_advice(self.advices[1], Rotation::prev());
            let alpha = meta.query_advice(self.advices[1], Rotation::cur());

            // s_minus_lo_130 = s - sum_{i = 0}^{129} 2^i ⋅ s_i
            let s_minus_lo_130 = meta.query_advice(self.advices[1], Rotation::next());

            let s = meta.query_advice(self.advices[2], Rotation::cur());
            let s_check = s - (alpha.clone() + k_254.clone() * two_pow_130);

            // q = 2^254 + t_q is the Pallas scalar field modulus.
            // We cast t_q into the base field to check alpha + t_q (mod p).
            let t_q = Expression::Constant(pallas::Base::from_u128(T_Q));

            // z_0 - alpha - t_q = 0 (mod p)
            let recovery = z_0 - alpha - t_q;

            // k_254 * (z_130 - 2^124) = 0
            let lo_zero = k_254.clone() * (z_130.clone() - two_pow_124);

            // k_254 * s_minus_lo_130 = 0
            let s_minus_lo_130_check = k_254.clone() * s_minus_lo_130.clone();

            // (1 - k_254) * (1 - z_130 * eta) * s_minus_lo_130 = 0
            let canonicity = (one.clone() - k_254) * (one - z_130 * eta) * s_minus_lo_130;

            iter::empty()
                .chain(Some(s_check))
                .chain(Some(recovery))
                .chain(Some(lo_zero))
                .chain(Some(s_minus_lo_130_check))
                .chain(Some(canonicity))
                .map(|poly| q_mul_overflow.clone() * poly)
                .collect::<Vec<_>>()
        });
    }

    pub(super) fn overflow_check(
        &self,
        mut layouter: impl Layouter<pallas::Base>,
        alpha: CellValue<pallas::Base>,
        zs: &[Z<pallas::Base>], // [z_0, z_1, ..., z_{254}, z_{255}]
    ) -> Result<(), Error> {
        // s = alpha + k_254 ⋅ 2^130 is witnessed here, and then copied into
        // the decomposition as well as the overflow check gate.
        // In the overflow check gate, we check that s is properly derived
        // from alpha and k_254.
        let s = {
            let k_254 = *zs[254];
            let s_val = alpha
                .value()
                .zip(k_254.value())
                .map(|(alpha, k_254)| alpha + k_254 * pallas::Base::from_u128(1 << 65).square());

            layouter.assign_region(
                || "s = alpha + k_254 ⋅ 2^130",
                |mut region| {
                    let s_cell = region.assign_advice(
                        || "s = alpha + k_254 ⋅ 2^130",
                        self.advices[0],
                        0,
                        || s_val.ok_or(Error::SynthesisError),
                    )?;
                    Ok(CellValue::new(s_cell, s_val))
                },
            )?
        };

        // Subtract the first 130 low bits of s = alpha + k_254 ⋅ 2^130
        // using thirteen 10-bit lookups, s_{0..=129}
        let s_minus_lo_130 =
            self.s_minus_lo_130(layouter.namespace(|| "decompose s_{0..=129}"), s)?;

        layouter.assign_region(
            || "overflow check",
            |mut region| {
                let offset = 0;

                // Enable overflow check gate
                self.q_mul_overflow.enable(&mut region, offset + 1)?;

                // Copy `z_0`
                copy(&mut region, || "copy z_0", self.advices[0], offset, &*zs[0])?;

                // Copy `z_130`
                copy(
                    &mut region,
                    || "copy z_130",
                    self.advices[0],
                    offset + 1,
                    &*zs[130],
                )?;

                // Witness η = inv0(z_130), where inv0(x) = 0 if x = 0, 1/x otherwise
                {
                    let eta = zs[130].value().map(|z_130| {
                        if z_130 == pallas::Base::zero() {
                            pallas::Base::zero()
                        } else {
                            z_130.invert().unwrap()
                        }
                    });
                    region.assign_advice(
                        || "η = inv0(z_130)",
                        self.advices[0],
                        offset + 2,
                        || eta.ok_or(Error::SynthesisError),
                    )?;
                }

                // Copy `k_254` = z_254
                copy(
                    &mut region,
                    || "copy k_254",
                    self.advices[1],
                    offset,
                    &*zs[254],
                )?;

                // Copy original alpha
                copy(
                    &mut region,
                    || "copy original alpha",
                    self.advices[1],
                    offset + 1,
                    &alpha,
                )?;

                // Copy weighted sum of the decomposition of s = alpha + k_254 ⋅ 2^130.
                copy(
                    &mut region,
                    || "copy s_minus_lo_130",
                    self.advices[1],
                    offset + 2,
                    &s_minus_lo_130,
                )?;

                // Copy witnessed s to check that it was properly derived from alpha and k_254.
                copy(&mut region, || "copy s", self.advices[2], offset + 1, &s)?;

                Ok(())
            },
        )?;

        Ok(())
    }

    fn s_minus_lo_130(
        &self,
        mut layouter: impl Layouter<pallas::Base>,
        s: CellValue<pallas::Base>,
    ) -> Result<CellValue<pallas::Base>, Error> {
        // Number of k-bit words we can use in the lookup decomposition.
        let num_words = 130 / sinsemilla::K;
        assert!(num_words * sinsemilla::K == 130);

        // Decompose the low 130 bits of `s` using thirteen 10-bit lookups.
        let zs = self.lookup_config.copy_check(
            layouter.namespace(|| "Decompose low 130 bits of s"),
            s,
            num_words,
            false,
        )?;
        // (s - (2^0 s_0 + 2^1 s_1 + ... + 2^129 s_129)) / 2^130
        Ok(zs[zs.len() - 1])
    }
}
