use super::{CellValue, EccConfig, EccPoint, Var};

use group::prime::PrimeCurveAffine;

use halo2::{
    circuit::Region,
    plonk::{Advice, Column, ConstraintSystem, Error, Expression, Selector},
    poly::Rotation,
};
use pasta_curves::{arithmetic::CurveAffine, pallas};

#[derive(Clone, Debug)]
pub struct Config {
    q_point: Selector,
    // x-coordinate
    pub x: Column<Advice>,
    // y-coordinate
    pub y: Column<Advice>,
}

impl From<&EccConfig> for Config {
    fn from(ecc_config: &EccConfig) -> Self {
        Self {
            q_point: ecc_config.q_point,
            x: ecc_config.advices[0],
            y: ecc_config.advices[1],
        }
    }
}

impl Config {
    pub(super) fn create_gate(&self, meta: &mut ConstraintSystem<pallas::Base>) {
        meta.create_gate("witness point", |meta| {
            // Check that either the point being witness is either:
            // - the identity, which is mapped to (0, 0) in affine coordinates; or
            // - a valid curve point y^2 = x^3 + b, where b = 5 in the Pallas equation

            let q_point = meta.query_selector(self.q_point);
            let x = meta.query_advice(self.x, Rotation::cur());
            let y = meta.query_advice(self.y, Rotation::cur());

            // y^2 = x^3 + b
            let curve_eqn = y.clone().square()
                - (x.clone().square() * x.clone())
                - Expression::Constant(pallas::Affine::b());

            vec![
                q_point.clone() * x * curve_eqn.clone(),
                q_point * y * curve_eqn,
            ]
        });
    }

    pub(super) fn assign_region(
        &self,
        value: Option<pallas::Affine>,
        offset: usize,
        region: &mut Region<'_, pallas::Base>,
    ) -> Result<EccPoint, Error> {
        // Enable `q_point` selector
        self.q_point.enable(region, offset)?;

        let value = value.map(|value| {
            // Map the identity to (0, 0).
            if value == pallas::Affine::identity() {
                (pallas::Base::zero(), pallas::Base::zero())
            } else {
                let value = value.coordinates().unwrap();
                (*value.x(), *value.y())
            }
        });

        // Assign `x` value
        let x_val = value.map(|value| value.0);
        let x_var = region.assign_advice(
            || "x",
            self.x,
            offset,
            || x_val.ok_or(Error::SynthesisError),
        )?;

        // Assign `y` value
        let y_val = value.map(|value| value.1);
        let y_var = region.assign_advice(
            || "y",
            self.y,
            offset,
            || y_val.ok_or(Error::SynthesisError),
        )?;

        Ok(EccPoint {
            x: CellValue::<pallas::Base>::new(x_var, x_val),
            y: CellValue::<pallas::Base>::new(y_var, y_val),
        })
    }
}
