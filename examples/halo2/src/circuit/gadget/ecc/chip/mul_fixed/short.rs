use std::{array, convert::TryInto};

use super::super::{EccConfig, EccPoint, EccScalarFixedShort};
use crate::{
    circuit::gadget::utilities::{copy, decompose_running_sum::RunningSumConfig, CellValue, Var},
    constants::{ValueCommitV, FIXED_BASE_WINDOW_SIZE, L_VALUE, NUM_WINDOWS_SHORT},
};

use halo2::{
    circuit::{Layouter, Region},
    plonk::{ConstraintSystem, Error, Expression, Selector},
    poly::Rotation,
};
use pasta_curves::pallas;

#[derive(Clone)]
pub struct Config {
    // Selector used for fixed-base scalar mul with short signed exponent.
    q_mul_fixed_short: Selector,
    q_mul_fixed_running_sum: Selector,
    running_sum_config: RunningSumConfig<pallas::Base, { FIXED_BASE_WINDOW_SIZE }>,
    super_config: super::Config<NUM_WINDOWS_SHORT>,
}

impl From<&EccConfig> for Config {
    fn from(config: &EccConfig) -> Self {
        Self {
            q_mul_fixed_short: config.q_mul_fixed_short,
            q_mul_fixed_running_sum: config.q_mul_fixed_running_sum,
            running_sum_config: config.running_sum_config.clone(),
            super_config: config.into(),
        }
    }
}

impl Config {
    pub(crate) fn create_gate(&self, meta: &mut ConstraintSystem<pallas::Base>) {
        meta.create_gate("Short fixed-base mul gate", |meta| {
            let q_mul_fixed_short = meta.query_selector(self.q_mul_fixed_short);
            let y_p = meta.query_advice(self.super_config.y_p, Rotation::cur());
            let y_a = meta.query_advice(self.super_config.add_config.y_qr, Rotation::cur());
            // z_21
            let last_window = meta.query_advice(self.super_config.u, Rotation::cur());
            let sign = meta.query_advice(self.super_config.window, Rotation::cur());

            let one = Expression::Constant(pallas::Base::one());

            // Check that last window is either 0 or 1.
            let last_window_check = last_window.clone() * (one.clone() - last_window);
            // Check that sign is either 1 or -1.
            let sign_check = sign.clone() * sign.clone() - one;

            // `(x_a, y_a)` is the result of `[m]B`, where `m` is the magnitude.
            // We conditionally negate this result using `y_p = y_a * s`, where `s` is the sign.

            // Check that the final `y_p = y_a` or `y_p = -y_a`
            let y_check = (y_p.clone() - y_a.clone()) * (y_p.clone() + y_a.clone());

            // Check that the correct sign is witnessed s.t. sign * y_p = y_a
            let negation_check = sign * y_p - y_a;

            array::IntoIter::new([
                ("last_window_check", last_window_check),
                ("sign_check", sign_check),
                ("y_check", y_check),
                ("negation_check", negation_check),
            ])
            .map(move |(name, poly)| (name, q_mul_fixed_short.clone() * poly))
        });
    }

    fn decompose(
        &self,
        region: &mut Region<'_, pallas::Base>,
        offset: usize,
        magnitude_sign: (CellValue<pallas::Base>, CellValue<pallas::Base>),
    ) -> Result<EccScalarFixedShort, Error> {
        let (magnitude, sign) = magnitude_sign;

        // Decompose magnitude
        let running_sum = self.running_sum_config.copy_decompose(
            region,
            offset,
            magnitude,
            true,
            L_VALUE,
            NUM_WINDOWS_SHORT,
        )?;

        Ok(EccScalarFixedShort {
            magnitude,
            sign,
            running_sum: (*running_sum).as_slice().try_into().unwrap(),
        })
    }

    pub fn assign(
        &self,
        mut layouter: impl Layouter<pallas::Base>,
        magnitude_sign: (CellValue<pallas::Base>, CellValue<pallas::Base>),
        base: &ValueCommitV,
    ) -> Result<(EccPoint, EccScalarFixedShort), Error> {
        let (scalar, acc, mul_b) = layouter.assign_region(
            || "Short fixed-base mul (incomplete addition)",
            |mut region| {
                let offset = 0;

                // Decompose the scalar
                let scalar = self.decompose(&mut region, offset, magnitude_sign)?;

                let (acc, mul_b) = self.super_config.assign_region_inner(
                    &mut region,
                    offset,
                    &(&scalar).into(),
                    base.clone().into(),
                    self.q_mul_fixed_running_sum,
                )?;

                Ok((scalar, acc, mul_b))
            },
        )?;

        // Last window
        let result = layouter.assign_region(
            || "Short fixed-base mul (most significant word)",
            |mut region| {
                let offset = 0;
                // Add to the cumulative sum to get `[magnitude]B`.
                let magnitude_mul = self.super_config.add_config.assign_region(
                    &mul_b,
                    &acc,
                    offset,
                    &mut region,
                )?;

                // Increase offset by 1 after complete addition
                let offset = offset + 1;

                // Copy sign to `window` column
                let sign = copy(
                    &mut region,
                    || "sign",
                    self.super_config.window,
                    offset,
                    &scalar.sign,
                )?;

                // Copy last window to `u` column.
                // (Although the last window is not a `u` value; we are copying it into the `u`
                // column because there is an available cell there.)
                let z_21 = scalar.running_sum[21];
                copy(
                    &mut region,
                    || "last_window",
                    self.super_config.u,
                    offset,
                    &z_21,
                )?;

                // Conditionally negate `y`-coordinate
                let y_val = if let Some(sign) = sign.value() {
                    if sign == -pallas::Base::one() {
                        magnitude_mul.y.value().map(|y: pallas::Base| -y)
                    } else {
                        magnitude_mul.y.value()
                    }
                } else {
                    None
                };

                // Enable mul_fixed_short selector on final row
                self.q_mul_fixed_short.enable(&mut region, offset)?;

                // Assign final `y` to `y_p` column and return final point
                let y_var = region.assign_advice(
                    || "y_var",
                    self.super_config.y_p,
                    offset,
                    || y_val.ok_or(Error::SynthesisError),
                )?;

                Ok(EccPoint {
                    x: magnitude_mul.x,
                    y: CellValue::new(y_var, y_val),
                })
            },
        )?;

        #[cfg(test)]
        // Check that the correct multiple is obtained.
        // This inlined test is only done for valid 64-bit magnitudes
        // and valid +/- 1 signs.
        // Invalid values result in constraint failures which are
        // tested at the circuit-level.
        {
            use group::Curve;
            use pasta_curves::arithmetic::FieldExt;

            if let (Some(magnitude), Some(sign)) = (scalar.magnitude.value(), scalar.sign.value()) {
                let magnitude_is_valid =
                    magnitude <= pallas::Base::from_u64(0xFFFF_FFFF_FFFF_FFFFu64);
                let sign_is_valid = sign * sign == pallas::Base::one();
                if magnitude_is_valid && sign_is_valid {
                    let base: super::OrchardFixedBases = base.clone().into();

                    let scalar = scalar.magnitude.value().zip(scalar.sign.value()).map(
                        |(magnitude, sign)| {
                            // Move magnitude from base field into scalar field (which always fits
                            // for Pallas).
                            let magnitude =
                                pallas::Scalar::from_bytes(&magnitude.to_bytes()).unwrap();

                            let sign = if sign == pallas::Base::one() {
                                pallas::Scalar::one()
                            } else {
                                -pallas::Scalar::one()
                            };

                            magnitude * sign
                        },
                    );
                    let real_mul = scalar.map(|scalar| base.generator() * scalar);

                    let result = result.point();

                    if let (Some(real_mul), Some(result)) = (real_mul, result) {
                        assert_eq!(real_mul.to_affine(), result);
                    }
                }
            }
        }

        Ok((result, scalar))
    }
}

#[cfg(test)]
pub mod tests {
    use group::Curve;
    use halo2::{
        circuit::{Chip, Layouter},
        plonk::{Any, Error},
    };
    use pasta_curves::{arithmetic::FieldExt, pallas};

    use crate::circuit::gadget::{
        ecc::{chip::EccChip, FixedPointShort, Point},
        utilities::{lookup_range_check::LookupRangeCheckConfig, CellValue, UtilitiesInstructions},
    };
    use crate::constants::load::ValueCommitV;

    #[allow(clippy::op_ref)]
    pub fn test_mul_fixed_short(
        chip: EccChip,
        mut layouter: impl Layouter<pallas::Base>,
    ) -> Result<(), Error> {
        // value_commit_v
        let value_commit_v = ValueCommitV::get();
        let base_val = value_commit_v.generator;
        let value_commit_v = FixedPointShort::from_inner(chip.clone(), value_commit_v);

        fn load_magnitude_sign(
            chip: EccChip,
            mut layouter: impl Layouter<pallas::Base>,
            magnitude: pallas::Base,
            sign: pallas::Base,
        ) -> Result<(CellValue<pallas::Base>, CellValue<pallas::Base>), Error> {
            let column = chip.config().advices[0];
            let magnitude =
                chip.load_private(layouter.namespace(|| "magnitude"), column, Some(magnitude))?;
            let sign = chip.load_private(layouter.namespace(|| "sign"), column, Some(sign))?;

            Ok((magnitude, sign))
        }

        fn constrain_equal(
            chip: EccChip,
            mut layouter: impl Layouter<pallas::Base>,
            base_val: pallas::Affine,
            scalar_val: pallas::Scalar,
            result: Point<pallas::Affine, EccChip>,
        ) -> Result<(), Error> {
            let expected = Point::new(
                chip,
                layouter.namespace(|| "expected point"),
                Some((base_val * scalar_val).to_affine()),
            )?;
            result.constrain_equal(layouter.namespace(|| "constrain result"), &expected)
        }

        let magnitude_signs = [
            ("mul by +zero", pallas::Base::zero(), pallas::Base::one()),
            ("mul by -zero", pallas::Base::zero(), -pallas::Base::one()),
            (
                "random [a]B",
                pallas::Base::from_u64(rand::random::<u64>()),
                {
                    let mut random_sign = pallas::Base::one();
                    if rand::random::<bool>() {
                        random_sign = -random_sign;
                    }
                    random_sign
                },
            ),
            (
                "[2^64 - 1]B",
                pallas::Base::from_u64(0xFFFF_FFFF_FFFF_FFFFu64),
                pallas::Base::one(),
            ),
            (
                "-[2^64 - 1]B",
                pallas::Base::from_u64(0xFFFF_FFFF_FFFF_FFFFu64),
                -pallas::Base::one(),
            ),
            // There is a single canonical sequence of window values for which a doubling occurs on the last step:
            // 1333333333333333333334 in octal.
            // [0xB6DB_6DB6_DB6D_B6DC] B
            (
                "mul_with_double",
                pallas::Base::from_u64(0xB6DB_6DB6_DB6D_B6DCu64),
                pallas::Base::one(),
            ),
            (
                "mul_with_double negative",
                pallas::Base::from_u64(0xB6DB_6DB6_DB6D_B6DCu64),
                -pallas::Base::one(),
            ),
        ];

        for (name, magnitude, sign) in magnitude_signs.iter() {
            let (result, _) = {
                let magnitude_sign = load_magnitude_sign(
                    chip.clone(),
                    layouter.namespace(|| *name),
                    *magnitude,
                    *sign,
                )?;
                value_commit_v.mul(layouter.namespace(|| *name), magnitude_sign)?
            };
            // Move from base field into scalar field
            let scalar = {
                let magnitude = pallas::Scalar::from_bytes(&magnitude.to_bytes()).unwrap();
                let sign = if *sign == pallas::Base::one() {
                    pallas::Scalar::one()
                } else {
                    -pallas::Scalar::one()
                };
                magnitude * sign
            };
            constrain_equal(
                chip.clone(),
                layouter.namespace(|| *name),
                base_val,
                scalar,
                result,
            )?;
        }

        Ok(())
    }

    #[test]
    fn invalid_magnitude_sign() {
        use crate::circuit::gadget::{
            ecc::chip::EccConfig,
            utilities::{CellValue, UtilitiesInstructions},
        };
        use halo2::{
            circuit::{Layouter, SimpleFloorPlanner},
            dev::{MockProver, VerifyFailure},
            plonk::{Circuit, ConstraintSystem, Error},
        };

        #[derive(Default)]
        struct MyCircuit {
            magnitude: Option<pallas::Base>,
            sign: Option<pallas::Base>,
        }

        impl UtilitiesInstructions<pallas::Base> for MyCircuit {
            type Var = CellValue<pallas::Base>;
        }

        impl Circuit<pallas::Base> for MyCircuit {
            type Config = EccConfig;
            type FloorPlanner = SimpleFloorPlanner;

            fn without_witnesses(&self) -> Self {
                Self::default()
            }

            fn configure(meta: &mut ConstraintSystem<pallas::Base>) -> Self::Config {
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
                let lookup_table = meta.lookup_table_column();
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

                // Shared fixed column for loading constants
                let constants = meta.fixed_column();
                meta.enable_constant(constants);

                let range_check = LookupRangeCheckConfig::configure(meta, advices[9], lookup_table);
                EccChip::configure(meta, advices, lagrange_coeffs, range_check)
            }

            fn synthesize(
                &self,
                config: Self::Config,
                mut layouter: impl Layouter<pallas::Base>,
            ) -> Result<(), Error> {
                let column = config.advices[0];

                let short_config: super::Config = (&config).into();
                let magnitude_sign = {
                    let magnitude = self.load_private(
                        layouter.namespace(|| "load magnitude"),
                        column,
                        self.magnitude,
                    )?;
                    let sign =
                        self.load_private(layouter.namespace(|| "load sign"), column, self.sign)?;
                    (magnitude, sign)
                };

                short_config.assign(layouter, magnitude_sign, &ValueCommitV::get())?;

                Ok(())
            }
        }

        // Magnitude larger than 64 bits should fail
        {
            let circuits = [
                // 2^64
                MyCircuit {
                    magnitude: Some(pallas::Base::from_u128(1 << 64)),
                    sign: Some(pallas::Base::one()),
                },
                // -2^64
                MyCircuit {
                    magnitude: Some(pallas::Base::from_u128(1 << 64)),
                    sign: Some(-pallas::Base::one()),
                },
                // 2^66
                MyCircuit {
                    magnitude: Some(pallas::Base::from_u128(1 << 66)),
                    sign: Some(pallas::Base::one()),
                },
                // -2^66
                MyCircuit {
                    magnitude: Some(pallas::Base::from_u128(1 << 66)),
                    sign: Some(-pallas::Base::one()),
                },
                // 2^254
                MyCircuit {
                    magnitude: Some(pallas::Base::from_u128(1 << 127).square()),
                    sign: Some(pallas::Base::one()),
                },
                // -2^254
                MyCircuit {
                    magnitude: Some(pallas::Base::from_u128(1 << 127).square()),
                    sign: Some(-pallas::Base::one()),
                },
            ];

            for circuit in circuits.iter() {
                let prover = MockProver::<pallas::Base>::run(11, circuit, vec![]).unwrap();
                assert_eq!(
                    prover.verify(),
                    Err(vec![
                        VerifyFailure::ConstraintNotSatisfied {
                            constraint: (
                                (16, "Short fixed-base mul gate").into(),
                                0,
                                "last_window_check"
                            )
                                .into(),
                            row: 26
                        },
                        VerifyFailure::Permutation {
                            column: (Any::Fixed, 9).into(),
                            row: 0
                        },
                        VerifyFailure::Permutation {
                            column: (Any::Advice, 4).into(),
                            row: 24
                        }
                    ])
                );
            }
        }

        // Sign that is not +/- 1 should fail
        {
            let circuit = MyCircuit {
                magnitude: Some(pallas::Base::from_u64(rand::random::<u64>())),
                sign: Some(pallas::Base::zero()),
            };

            let prover = MockProver::<pallas::Base>::run(11, &circuit, vec![]).unwrap();
            assert_eq!(
                prover.verify(),
                Err(vec![
                    VerifyFailure::ConstraintNotSatisfied {
                        constraint: ((16, "Short fixed-base mul gate").into(), 1, "sign_check")
                            .into(),
                        row: 26
                    },
                    VerifyFailure::ConstraintNotSatisfied {
                        constraint: (
                            (16, "Short fixed-base mul gate").into(),
                            3,
                            "negation_check"
                        )
                            .into(),
                        row: 26
                    }
                ])
            );
        }
    }
}
