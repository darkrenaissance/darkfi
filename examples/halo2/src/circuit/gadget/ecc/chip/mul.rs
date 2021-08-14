use super::{add, CellValue, EccConfig, EccPoint, Var};
use crate::{circuit::gadget::utilities::copy, constants::T_Q};
use std::ops::{Deref, Range};

use bigint::U256;
use ff::PrimeField;
use halo2::{
    arithmetic::FieldExt,
    circuit::{Layouter, Region},
    plonk::{ConstraintSystem, Error, Expression, Selector},
    poly::Rotation,
};

use pasta_curves::pallas;

mod complete;
mod incomplete;
mod overflow;

/// Number of bits for which complete addition needs to be used in variable-base
/// scalar multiplication
const NUM_COMPLETE_BITS: usize = 3;

// Bits used in incomplete addition. k_{254} to k_{4} inclusive
const INCOMPLETE_LEN: usize = pallas::Scalar::NUM_BITS as usize - 1 - NUM_COMPLETE_BITS;
const INCOMPLETE_RANGE: Range<usize> = 0..INCOMPLETE_LEN;

// Bits k_{254} to k_{4} inclusive are used in incomplete addition.
// The `hi` half is k_{254} to k_{130} inclusive (length 125 bits).
// (It is a coincidence that k_{130} matches the boundary of the
// overflow check described in [the book](https://zcash.github.io/halo2/design/gadgets/ecc/var-base-scalar-mul.html#overflow-check).)
const INCOMPLETE_HI_RANGE: Range<usize> = 0..(INCOMPLETE_LEN / 2);

// Bits k_{254} to k_{4} inclusive are used in incomplete addition.
// The `lo` half is k_{129} to k_{4} inclusive (length 126 bits).
const INCOMPLETE_LO_RANGE: Range<usize> = (INCOMPLETE_LEN / 2)..INCOMPLETE_LEN;

// Bits k_{3} to k_{1} inclusive are used in complete addition.
// Bit k_{0} is handled separately.
const COMPLETE_RANGE: Range<usize> = INCOMPLETE_LEN..(INCOMPLETE_LEN + NUM_COMPLETE_BITS);

pub struct Config {
    // Selector used to check switching logic on LSB
    q_mul_lsb: Selector,
    // Configuration used in complete addition
    add_config: add::Config,
    // Configuration used for `hi` bits of the scalar
    hi_config: incomplete::HiConfig,
    // Configuration used for `lo` bits of the scalar
    lo_config: incomplete::LoConfig,
    // Configuration used for complete addition part of double-and-add algorithm
    complete_config: complete::Config,
    // Configuration used to check for overflow
    overflow_config: overflow::Config,
}

impl From<&EccConfig> for Config {
    fn from(ecc_config: &EccConfig) -> Self {
        let config = Self {
            q_mul_lsb: ecc_config.q_mul_lsb,
            add_config: ecc_config.into(),
            hi_config: ecc_config.into(),
            lo_config: ecc_config.into(),
            complete_config: ecc_config.into(),
            overflow_config: ecc_config.into(),
        };

        assert_eq!(
            config.hi_config.x_p, config.lo_config.x_p,
            "x_p is shared across hi and lo halves."
        );
        assert_eq!(
            config.hi_config.y_p, config.lo_config.y_p,
            "y_p is shared across hi and lo halves."
        );

        // For both hi_config and lo_config:
        // z and lambda1 are assigned on the same row as the add_config output.
        // Therefore, z and lambda1 must not overlap with add_config.x_qr, add_config.y_qr.
        let add_config_outputs = config.add_config.output_columns();
        for config in [&(*config.hi_config), &(*config.lo_config)].iter() {
            assert!(
                !add_config_outputs.contains(&config.z),
                "incomplete config z cannot overlap with complete addition columns."
            );
            assert!(
                !add_config_outputs.contains(&config.lambda1),
                "incomplete config lambda1 cannot overlap with complete addition columns."
            );
        }

        config
    }
}

impl Config {
    pub(super) fn create_gate(&self, meta: &mut ConstraintSystem<pallas::Base>) {
        // If `lsb` is 0, (x, y) = (x_p, -y_p). If `lsb` is 1, (x, y) = (0,0).
        meta.create_gate("LSB check", |meta| {
            let q_mul_lsb = meta.query_selector(self.q_mul_lsb);

            let z_1 = meta.query_advice(self.complete_config.z_complete, Rotation::prev());
            let z_0 = meta.query_advice(self.complete_config.z_complete, Rotation::cur());
            let x_p = meta.query_advice(self.add_config.x_p, Rotation::prev());
            let y_p = meta.query_advice(self.add_config.y_p, Rotation::prev());
            let base_x = meta.query_advice(self.add_config.x_p, Rotation::cur());
            let base_y = meta.query_advice(self.add_config.y_p, Rotation::cur());

            //    z_0 = 2 * z_1 + k_0
            // => k_0 = z_0 - 2 * z_1
            let lsb = z_0 - z_1 * pallas::Base::from_u64(2);
            let one_minus_lsb = Expression::Constant(pallas::Base::one()) - lsb.clone();

            let bool_check = lsb.clone() * one_minus_lsb.clone();

            // `lsb` = 0 => (x_p, y_p) = (x, -y)
            // `lsb` = 1 => (x_p, y_p) = (0,0)
            let lsb_x = (lsb.clone() * x_p.clone()) + one_minus_lsb.clone() * (x_p - base_x);
            let lsb_y = (lsb * y_p.clone()) + one_minus_lsb * (y_p + base_y);

            std::array::IntoIter::new([bool_check, lsb_x, lsb_y])
                .map(move |poly| q_mul_lsb.clone() * poly)
        });

        self.hi_config.create_gate(meta);
        self.lo_config.create_gate(meta);
        self.complete_config.create_gate(meta);
        self.overflow_config.create_gate(meta);
    }

    pub(super) fn assign(
        &self,
        mut layouter: impl Layouter<pallas::Base>,
        alpha: CellValue<pallas::Base>,
        base: &EccPoint,
    ) -> Result<(EccPoint, CellValue<pallas::Base>), Error> {
        let (result, zs): (EccPoint, Vec<Z<pallas::Base>>) = layouter.assign_region(
            || "variable-base scalar mul",
            |mut region| {
                let offset = 0;
                // Decompose `k = alpha + t_q` bitwise (big-endian bit order).
                let bits = decompose_for_scalar_mul(alpha.value());

                // Define ranges for each part of the algorithm.
                let bits_incomplete_hi = &bits[INCOMPLETE_HI_RANGE];
                let bits_incomplete_lo = &bits[INCOMPLETE_LO_RANGE];
                let lsb = bits[pallas::Scalar::NUM_BITS as usize - 1];

                // Initialize the accumulator `acc = [2]base`
                let acc = self
                    .add_config
                    .assign_region(base, base, offset, &mut region)?;

                // Increase the offset by 1 after complete addition.
                let offset = offset + 1;

                // Initialize the running sum for scalar decomposition to zero
                let z_init = {
                    let z_init_cell = region.assign_advice_from_constant(
                        || "z_init = 0",
                        self.hi_config.z,
                        offset,
                        pallas::Base::zero(),
                    )?;

                    Z(CellValue::new(z_init_cell, Some(pallas::Base::zero())))
                };

                // Double-and-add (incomplete addition) for the `hi` half of the scalar decomposition
                let (x_a, y_a, zs_incomplete_hi) = self.hi_config.double_and_add(
                    &mut region,
                    offset,
                    base,
                    bits_incomplete_hi,
                    (X(acc.x), Y(acc.y), z_init),
                )?;

                // Double-and-add (incomplete addition) for the `lo` half of the scalar decomposition
                let z = zs_incomplete_hi.last().expect("should not be empty");
                let (x_a, y_a, zs_incomplete_lo) = self.lo_config.double_and_add(
                    &mut region,
                    offset,
                    base,
                    bits_incomplete_lo,
                    (x_a, y_a, *z),
                )?;

                // Move from incomplete addition to complete addition.
                // Inside incomplete::double_and_add, the offset was increased once after initialization
                // of the running sum.
                // Then, the final assignment of double-and-add was made on row + offset + 1.
                // Outside of incomplete addition, we must account for these offset increases by adding
                // 2 to the incomplete addition length.
                let offset = offset + INCOMPLETE_LO_RANGE.len() + 2;

                // Complete addition
                let (acc, zs_complete) = {
                    let z = zs_incomplete_lo.last().expect("should not be empty");
                    // Bits used in complete addition. k_{3} to k_{1} inclusive
                    // The LSB k_{0} is handled separately.
                    let bits_complete = &bits[COMPLETE_RANGE];
                    self.complete_config.assign_region(
                        &mut region,
                        offset,
                        bits_complete,
                        base,
                        x_a,
                        y_a,
                        *z,
                    )?
                };

                // Each iteration of the complete addition uses two rows.
                let offset = offset + COMPLETE_RANGE.len() * 2;

                // Process the least significant bit
                let z_1 = zs_complete.last().unwrap();
                let (result, z_0) = self.process_lsb(&mut region, offset, base, acc, *z_1, lsb)?;

                #[cfg(test)]
                // Check that the correct multiple is obtained.
                {
                    use group::Curve;

                    let base = base.point();
                    let alpha = alpha
                        .value()
                        .map(|alpha| pallas::Scalar::from_bytes(&alpha.to_bytes()).unwrap());
                    let real_mul = base.zip(alpha).map(|(base, alpha)| base * alpha);
                    let result = result.point();

                    if let (Some(real_mul), Some(result)) = (real_mul, result) {
                        assert_eq!(real_mul.to_affine(), result);
                    }
                }

                let zs = {
                    let mut zs = std::iter::empty()
                        .chain(Some(z_init))
                        .chain(zs_incomplete_hi.into_iter())
                        .chain(zs_incomplete_lo.into_iter())
                        .chain(zs_complete.into_iter())
                        .chain(Some(z_0))
                        .collect::<Vec<_>>();
                    assert_eq!(zs.len(), pallas::Scalar::NUM_BITS as usize + 1);

                    // This reverses zs to give us [z_0, z_1, ..., z_{254}, z_{255}].
                    zs.reverse();
                    zs
                };

                Ok((result, zs))
            },
        )?;

        self.overflow_config
            .overflow_check(layouter.namespace(|| "overflow check"), alpha, &zs)?;

        Ok((result, alpha))
    }

    /// Processes the final scalar bit `k_0`.
    ///
    /// Assumptions for this sub-region:
    /// - `acc_x` and `acc_y` are assigned in row `offset` by the previous complete
    ///   addition. They will be copied into themselves.
    /// - `z_1 is assigned in row `offset` by the mul::complete region assignment. We only
    ///   use its value here.
    ///
    /// `x_p` and `y_p` are assigned here, and then copied into themselves by the complete
    /// addition subregion.
    ///
    /// ```text
    /// | x_p  | y_p  | acc_x | acc_y | complete addition  | z_1 |
    /// |base_x|base_y| res_x | res_y |   |   |    |   |   | z_0 | q_mul_lsb = 1
    /// ```
    fn process_lsb(
        &self,
        region: &mut Region<'_, pallas::Base>,
        offset: usize,
        base: &EccPoint,
        acc: EccPoint,
        z_1: Z<pallas::Base>,
        lsb: Option<bool>,
    ) -> Result<(EccPoint, Z<pallas::Base>), Error> {
        // Enforce switching logic on LSB using a custom gate
        self.q_mul_lsb.enable(region, offset + 1)?;

        // z_1 has been assigned at (z_complete, offset).
        // Assign z_0 = 2⋅z_1 + k_0
        let z_0 = {
            let z_0_val = z_1.value().zip(lsb).map(|(z_1, lsb)| {
                let lsb = pallas::Base::from_u64(lsb as u64);
                z_1 * pallas::Base::from_u64(2) + lsb
            });
            let z_0_cell = region.assign_advice(
                || "z_0",
                self.complete_config.z_complete,
                offset + 1,
                || z_0_val.ok_or(Error::SynthesisError),
            )?;

            Z(CellValue::new(z_0_cell, z_0_val))
        };

        // Copy in `base_x`, `base_y` to use in the LSB gate
        copy(
            region,
            || "copy base_x",
            self.add_config.x_p,
            offset + 1,
            &base.x(),
        )?;
        copy(
            region,
            || "copy base_y",
            self.add_config.y_p,
            offset + 1,
            &base.y(),
        )?;

        // If `lsb` is 0, return `Acc + (-P)`. If `lsb` is 1, simply return `Acc + 0`.
        let x = if let Some(lsb) = lsb {
            if !lsb {
                base.x.value()
            } else {
                Some(pallas::Base::zero())
            }
        } else {
            None
        };

        let y = if let Some(lsb) = lsb {
            if !lsb {
                base.y.value().map(|y_p| -y_p)
            } else {
                Some(pallas::Base::zero())
            }
        } else {
            None
        };

        let x_cell = region.assign_advice(
            || "x",
            self.add_config.x_p,
            offset,
            || x.ok_or(Error::SynthesisError),
        )?;

        let y_cell = region.assign_advice(
            || "y",
            self.add_config.y_p,
            offset,
            || y.ok_or(Error::SynthesisError),
        )?;

        let p = EccPoint {
            x: CellValue::<pallas::Base>::new(x_cell, x),
            y: CellValue::<pallas::Base>::new(y_cell, y),
        };

        // Return the result of the final complete addition as `[scalar]B`
        let result = self.add_config.assign_region(&p, &acc, offset, region)?;

        Ok((result, z_0))
    }
}

#[derive(Clone, Debug)]
// `x`-coordinate of the accumulator.
struct X<F: FieldExt>(CellValue<F>);
impl<F: FieldExt> Deref for X<F> {
    type Target = CellValue<F>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Copy, Clone, Debug)]
// `y`-coordinate of the accumulator.
struct Y<F: FieldExt>(CellValue<F>);
impl<F: FieldExt> Deref for Y<F> {
    type Target = CellValue<F>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Copy, Clone, Debug)]
// Cumulative sum `z` used to decompose the scalar.
struct Z<F: FieldExt>(CellValue<F>);
impl<F: FieldExt> Deref for Z<F> {
    type Target = CellValue<F>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

fn decompose_for_scalar_mul(scalar: Option<pallas::Base>) -> Vec<Option<bool>> {
    let bitstring = scalar.map(|scalar| {
        // We use `k = scalar + t_q` in the double-and-add algorithm, where
        // the scalar field `F_q = 2^254 + t_q`.
        // Note that the addition `scalar + t_q` is not reduced.
        //
        let scalar = U256::from_little_endian(&scalar.to_bytes());
        let t_q = U256::from_little_endian(&T_Q.to_le_bytes());
        let k = scalar + t_q;

        // Big-endian bit representation of `k`.
        let bitstring: Vec<bool> = {
            let mut le_bytes = [0u8; 32];
            k.to_little_endian(&mut le_bytes);
            le_bytes.iter().fold(Vec::new(), |mut bitstring, byte| {
                let bits = (0..8)
                    .map(|shift| (byte >> shift) % 2 == 1)
                    .collect::<Vec<_>>();
                bitstring.extend_from_slice(&bits);
                bitstring
            })
        };

        // Take the first 255 bits.
        let mut bitstring = bitstring[0..pallas::Scalar::NUM_BITS as usize].to_vec();
        bitstring.reverse();
        bitstring
    });

    if let Some(bitstring) = bitstring {
        bitstring.into_iter().map(Some).collect()
    } else {
        vec![None; pallas::Scalar::NUM_BITS as usize]
    }
}

#[cfg(test)]
pub mod tests {
    use group::Curve;
    use halo2::{
        circuit::{Chip, Layouter},
        plonk::Error,
    };
    use pasta_curves::{arithmetic::FieldExt, pallas};

    use crate::circuit::gadget::{
        ecc::{chip::EccChip, EccInstructions, Point},
        utilities::UtilitiesInstructions,
    };

    pub fn test_mul(
        chip: EccChip,
        mut layouter: impl Layouter<pallas::Base>,
        zero: &Point<pallas::Affine, EccChip>,
        p: &Point<pallas::Affine, EccChip>,
        p_val: pallas::Affine,
    ) -> Result<(), Error> {
        let column = chip.config().advices[0];

        fn constrain_equal<
            EccChip: EccInstructions<pallas::Affine> + Clone + Eq + std::fmt::Debug,
        >(
            chip: EccChip,
            mut layouter: impl Layouter<pallas::Base>,
            base_val: pallas::Affine,
            scalar_val: pallas::Base,
            result: Point<pallas::Affine, EccChip>,
        ) -> Result<(), Error> {
            // Move scalar from base field into scalar field (which always fits
            // for Pallas).
            let scalar = pallas::Scalar::from_bytes(&scalar_val.to_bytes()).unwrap();
            let expected = Point::new(
                chip,
                layouter.namespace(|| "expected point"),
                Some((base_val * scalar).to_affine()),
            )?;
            result.constrain_equal(layouter.namespace(|| "constrain result"), &expected)
        }

        // [a]B
        {
            let scalar_val = pallas::Base::rand();
            let (result, _) = {
                let scalar = chip.load_private(
                    layouter.namespace(|| "random scalar"),
                    column,
                    Some(scalar_val),
                )?;
                p.mul(layouter.namespace(|| "random [a]B"), &scalar)?
            };
            constrain_equal(
                chip.clone(),
                layouter.namespace(|| "random [a]B"),
                p_val,
                scalar_val,
                result,
            )?;
        }

        // [a]𝒪 should return an error since variable-base scalar multiplication
        // uses incomplete addition at the beginning of its double-and-add.
        {
            let scalar_val = pallas::Base::rand();
            let scalar = chip.load_private(
                layouter.namespace(|| "random scalar"),
                column,
                Some(scalar_val),
            )?;
            zero.mul(layouter.namespace(|| "[a]𝒪"), &scalar)
                .expect_err("[a]𝒪 should return an error");
        }

        // [0]B should return (0,0) since variable-base scalar multiplication
        // uses complete addition for the final bits of the scalar.
        {
            let scalar_val = pallas::Base::zero();
            let (result, _) = {
                let scalar =
                    chip.load_private(layouter.namespace(|| "zero"), column, Some(scalar_val))?;
                p.mul(layouter.namespace(|| "[0]B"), &scalar)?
            };
            constrain_equal(
                chip.clone(),
                layouter.namespace(|| "[0]B"),
                p_val,
                scalar_val,
                result,
            )?;
        }

        // [-1]B (the largest possible base field element)
        {
            let scalar_val = -pallas::Base::one();
            let (result, _) = {
                let scalar =
                    chip.load_private(layouter.namespace(|| "-1"), column, Some(scalar_val))?;
                p.mul(layouter.namespace(|| "[-1]B"), &scalar)?
            };
            constrain_equal(
                chip,
                layouter.namespace(|| "[-1]B"),
                p_val,
                scalar_val,
                result,
            )?;
        }

        Ok(())
    }
}
