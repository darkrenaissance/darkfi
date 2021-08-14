use super::super::{add, copy, CellValue, EccConfig, EccPoint, Var};
use super::{COMPLETE_RANGE, X, Y, Z};

use halo2::{
    circuit::Region,
    plonk::{Advice, Column, ConstraintSystem, Error, Expression, Selector},
    poly::Rotation,
};

use pasta_curves::{arithmetic::FieldExt, pallas};

pub struct Config {
    // Selector used to constrain the cells used in complete addition.
    q_mul_decompose_var: Selector,
    // Advice column used to decompose scalar in complete addition.
    pub z_complete: Column<Advice>,
    // Configuration used in complete addition
    add_config: add::Config,
}

impl From<&EccConfig> for Config {
    fn from(ecc_config: &EccConfig) -> Self {
        let config = Self {
            q_mul_decompose_var: ecc_config.q_mul_decompose_var,
            z_complete: ecc_config.advices[9],
            add_config: ecc_config.into(),
        };

        let add_config_advices = config.add_config.advice_columns();
        assert!(
            !add_config_advices.contains(&config.z_complete),
            "z_complete cannot overlap with complete addition columns."
        );

        config
    }
}

impl Config {
    /// Gate used to check scalar decomposition is correct.
    /// This is used to check the bits used in complete addition, since the incomplete
    /// addition gate (controlled by `q_mul`) already checks scalar decomposition for
    /// the other bits.
    pub(super) fn create_gate(&self, meta: &mut ConstraintSystem<pallas::Base>) {
        meta.create_gate(
            "Decompose scalar for complete bits of variable-base mul",
            |meta| {
                let q_mul_decompose_var = meta.query_selector(self.q_mul_decompose_var);
                // z_{i + 1}
                let z_prev = meta.query_advice(self.z_complete, Rotation::prev());
                // z_i
                let z_next = meta.query_advice(self.z_complete, Rotation::next());

                // k_{i} = z_{i} - 2⋅z_{i+1}
                let k = z_next - Expression::Constant(pallas::Base::from_u64(2)) * z_prev;
                let k_minus_one = k.clone() - Expression::Constant(pallas::Base::one());
                // (k_i) ⋅ (k_i - 1) = 0
                let bool_check = k.clone() * k_minus_one.clone();

                // base_y
                let base_y = meta.query_advice(self.z_complete, Rotation::cur());
                // y_p
                let y_p = meta.query_advice(self.add_config.y_p, Rotation::prev());

                // k_i = 0 => y_p = -base_y
                // k_i = 1 => y_p = base_y
                let y_switch = k_minus_one * (base_y.clone() + y_p.clone()) + k * (base_y - y_p);

                std::array::IntoIter::new([("bool_check", bool_check), ("y_switch", y_switch)])
                    .map(move |(name, poly)| (name, q_mul_decompose_var.clone() * poly))
            },
        );
    }

    #[allow(clippy::type_complexity)]
    #[allow(non_snake_case)]
    #[allow(clippy::too_many_arguments)]
    pub(super) fn assign_region(
        &self,
        region: &mut Region<'_, pallas::Base>,
        offset: usize,
        bits: &[Option<bool>],
        base: &EccPoint,
        x_a: X<pallas::Base>,
        y_a: Y<pallas::Base>,
        z: Z<pallas::Base>,
    ) -> Result<(EccPoint, Vec<Z<pallas::Base>>), Error> {
        // Make sure we have the correct number of bits for the complete addition
        // part of variable-base scalar mul.
        assert_eq!(bits.len(), COMPLETE_RANGE.len());

        // Enable selectors for complete range
        for row in 0..COMPLETE_RANGE.len() {
            // Each iteration uses 2 rows (two complete additions)
            let row = 2 * row;
            // Check scalar decomposition for each iteration. Since the gate enabled by
            // `q_mul_decompose_var` queries the previous row, we enable the selector on
            // `row + offset + 1` (instead of `row + offset`).
            self.q_mul_decompose_var.enable(region, row + offset + 1)?;
        }

        // Use x_a, y_a output from incomplete addition
        let mut acc = EccPoint { x: *x_a, y: *y_a };

        // Copy running sum `z` from incomplete addition
        let mut z = {
            let z = copy(
                region,
                || "Copy `z` running sum from incomplete addition",
                self.z_complete,
                offset,
                &z,
            )?;
            Z(z)
        };

        // Store interstitial running sum `z`s in vector
        let mut zs: Vec<Z<pallas::Base>> = Vec::with_capacity(bits.len());

        // Complete addition
        for (iter, k) in bits.iter().enumerate() {
            // Each iteration uses 2 rows (two complete additions)
            let row = 2 * iter;

            // Update `z`.
            z = {
                // z_next = z_cur * 2 + k_next
                let z_val = z.value().zip(k.as_ref()).map(|(z_val, k)| {
                    pallas::Base::from_u64(2) * z_val + pallas::Base::from_u64(*k as u64)
                });
                let z_cell = region.assign_advice(
                    || "z",
                    self.z_complete,
                    row + offset + 2,
                    || z_val.ok_or(Error::SynthesisError),
                )?;
                Z(CellValue::new(z_cell, z_val))
            };
            zs.push(z);

            // Assign `y_p` for complete addition.
            let y_p = {
                let base_y = copy(
                    region,
                    || "Copy `base.y`",
                    self.z_complete,
                    row + offset + 1,
                    &base.y,
                )?;

                // If the bit is set, use `y`; if the bit is not set, use `-y`
                let y_p = base_y
                    .value()
                    .zip(k.as_ref())
                    .map(|(base_y, k)| if !k { -base_y } else { base_y });

                let y_p_cell = region.assign_advice(
                    || "y_p",
                    self.add_config.y_p,
                    row + offset,
                    || y_p.ok_or(Error::SynthesisError),
                )?;
                CellValue::<pallas::Base>::new(y_p_cell, y_p)
            };

            // U = P if the bit is set; U = -P is the bit is not set.
            let U = EccPoint { x: base.x, y: y_p };

            // Acc + U
            let tmp_acc = self
                .add_config
                .assign_region(&U, &acc, row + offset, region)?;

            // Acc + U + Acc
            acc = self
                .add_config
                .assign_region(&acc, &tmp_acc, row + offset + 1, region)?;
        }
        Ok((acc, zs))
    }
}
