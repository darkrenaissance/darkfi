use std::{array, collections::HashSet};

use super::{copy, CellValue, EccConfig, EccPoint, Var};
use group::Curve;
use halo2::{
    circuit::Region,
    plonk::{Advice, Column, ConstraintSystem, Error, Selector},
    poly::Rotation,
};
use pasta_curves::{arithmetic::CurveAffine, pallas};

#[derive(Clone, Debug)]
pub struct Config {
    q_add_incomplete: Selector,
    // x-coordinate of P in P + Q = R
    pub x_p: Column<Advice>,
    // y-coordinate of P in P + Q = R
    pub y_p: Column<Advice>,
    // x-coordinate of Q or R in P + Q = R
    pub x_qr: Column<Advice>,
    // y-coordinate of Q or R in P + Q = R
    pub y_qr: Column<Advice>,
}

impl From<&EccConfig> for Config {
    fn from(ecc_config: &EccConfig) -> Self {
        Self {
            q_add_incomplete: ecc_config.q_add_incomplete,
            x_p: ecc_config.advices[0],
            y_p: ecc_config.advices[1],
            x_qr: ecc_config.advices[2],
            y_qr: ecc_config.advices[3],
        }
    }
}

impl Config {
    pub(crate) fn advice_columns(&self) -> HashSet<Column<Advice>> {
        core::array::IntoIter::new([self.x_p, self.y_p, self.x_qr, self.y_qr]).collect()
    }

    pub(super) fn create_gate(&self, meta: &mut ConstraintSystem<pallas::Base>) {
        meta.create_gate("incomplete addition gates", |meta| {
            let q_add_incomplete = meta.query_selector(self.q_add_incomplete);
            let x_p = meta.query_advice(self.x_p, Rotation::cur());
            let y_p = meta.query_advice(self.y_p, Rotation::cur());
            let x_q = meta.query_advice(self.x_qr, Rotation::cur());
            let y_q = meta.query_advice(self.y_qr, Rotation::cur());
            let x_r = meta.query_advice(self.x_qr, Rotation::next());
            let y_r = meta.query_advice(self.y_qr, Rotation::next());

            // (x_r + x_q + x_p)⋅(x_p − x_q)^2 − (y_p − y_q)^2 = 0
            let poly1 = {
                (x_r.clone() + x_q.clone() + x_p.clone())
                    * (x_p.clone() - x_q.clone())
                    * (x_p.clone() - x_q.clone())
                    - (y_p.clone() - y_q.clone()).square()
            };

            // (y_r + y_q)(x_p − x_q) − (y_p − y_q)(x_q − x_r) = 0
            let poly2 = (y_r + y_q.clone()) * (x_p - x_q.clone()) - (y_p - y_q) * (x_q - x_r);

            array::IntoIter::new([poly1, poly2]).map(move |poly| q_add_incomplete.clone() * poly)
        });
    }

    pub(super) fn assign_region(
        &self,
        p: &EccPoint,
        q: &EccPoint,
        offset: usize,
        region: &mut Region<'_, pallas::Base>,
    ) -> Result<EccPoint, Error> {
        // Enable `q_add_incomplete` selector
        self.q_add_incomplete.enable(region, offset)?;

        // Handle exceptional cases
        let (x_p, y_p) = (p.x.value(), p.y.value());
        let (x_q, y_q) = (q.x.value(), q.y.value());
        x_p.zip(y_p)
            .zip(x_q)
            .zip(y_q)
            .map(|(((x_p, y_p), x_q), y_q)| {
                // P is point at infinity
                if (x_p == pallas::Base::zero() && y_p == pallas::Base::zero())
                // Q is point at infinity
                || (x_q == pallas::Base::zero() && y_q == pallas::Base::zero())
                // x_p = x_q
                || (x_p == x_q)
                {
                    Err(Error::SynthesisError)
                } else {
                    Ok(())
                }
            })
            .transpose()?;

        // Copy point `p` into `x_p`, `y_p` columns
        copy(region, || "x_p", self.x_p, offset, &p.x)?;
        copy(region, || "y_p", self.y_p, offset, &p.y)?;

        // Copy point `q` into `x_qr`, `y_qr` columns
        copy(region, || "x_q", self.x_qr, offset, &q.x)?;
        copy(region, || "y_q", self.y_qr, offset, &q.y)?;

        // Compute the sum `P + Q = R`
        let r = {
            let p = p.point();
            let q = q.point();
            let r = p
                .zip(q)
                .map(|(p, q)| (p + q).to_affine().coordinates().unwrap());
            let r_x = r.map(|r| *r.x());
            let r_y = r.map(|r| *r.y());

            (r_x, r_y)
        };

        // Assign the sum to `x_qr`, `y_qr` columns in the next row
        let x_r = r.0;
        let x_r_var = region.assign_advice(
            || "x_r",
            self.x_qr,
            offset + 1,
            || x_r.ok_or(Error::SynthesisError),
        )?;

        let y_r = r.1;
        let y_r_var = region.assign_advice(
            || "y_r",
            self.y_qr,
            offset + 1,
            || y_r.ok_or(Error::SynthesisError),
        )?;

        let result = EccPoint {
            x: CellValue::<pallas::Base>::new(x_r_var, x_r),
            y: CellValue::<pallas::Base>::new(y_r_var, y_r),
        };

        Ok(result)
    }
}

#[cfg(test)]
pub mod tests {
    use group::Curve;
    use halo2::{circuit::Layouter, plonk::Error};
    use pasta_curves::pallas;

    use crate::circuit::gadget::ecc::{EccInstructions, Point};

    #[allow(clippy::too_many_arguments)]
    pub fn test_add_incomplete<
        EccChip: EccInstructions<pallas::Affine> + Clone + Eq + std::fmt::Debug,
    >(
        chip: EccChip,
        mut layouter: impl Layouter<pallas::Base>,
        zero: &Point<pallas::Affine, EccChip>,
        p_val: pallas::Affine,
        p: &Point<pallas::Affine, EccChip>,
        q_val: pallas::Affine,
        q: &Point<pallas::Affine, EccChip>,
        p_neg: &Point<pallas::Affine, EccChip>,
    ) -> Result<(), Error> {
        // P + Q
        {
            let result = p.add_incomplete(layouter.namespace(|| "P + Q"), q)?;
            let witnessed_result = Point::new(
                chip,
                layouter.namespace(|| "witnessed P + Q"),
                Some((p_val + q_val).to_affine()),
            )?;
            result.constrain_equal(layouter.namespace(|| "constrain P + Q"), &witnessed_result)?;
        }

        // P + P should return an error
        p.add_incomplete(layouter.namespace(|| "P + P"), p)
            .expect_err("P + P should return an error");

        // P + (-P) should return an error
        p.add_incomplete(layouter.namespace(|| "P + (-P)"), p_neg)
            .expect_err("P + (-P) should return an error");

        // P + 𝒪 should return an error
        p.add_incomplete(layouter.namespace(|| "P + 𝒪"), zero)
            .expect_err("P + 0 should return an error");

        // 𝒪 + P should return an error
        zero.add_incomplete(layouter.namespace(|| "𝒪 + P"), p)
            .expect_err("0 + P should return an error");

        // 𝒪 + 𝒪 should return an error
        zero.add_incomplete(layouter.namespace(|| "𝒪 + 𝒪"), zero)
            .expect_err("𝒪 + 𝒪 should return an error");

        Ok(())
    }
}
