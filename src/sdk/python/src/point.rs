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

use darkfi_sdk::{
    crypto::{
        constants::fixed_bases::{
            NullifierK, VALUE_COMMITMENT_PERSONALIZATION, VALUE_COMMITMENT_R_BYTES,
            VALUE_COMMITMENT_V_BYTES,
        },
        util::mod_r_p,
        ValueCommit,
    },
    pasta::{
        arithmetic::CurveExt,
        group::{Curve, Group},
        pallas,
    },
};
use halo2_gadgets::ecc::chip::FixedPoint;
use pyo3::{basic::CompareOp, prelude::*};

use super::{affine::Affine, base::Base, scalar::Scalar};

/// A Pallas point in the projective coordinate space.
#[pyclass]
pub struct Point(pub(crate) pallas::Point);

#[pymethods]
impl Point {
    #[staticmethod]
    fn identity() -> Self {
        Self(pallas::Point::identity())
    }

    #[staticmethod]
    fn generator() -> Self {
        Self(pallas::Point::generator())
    }

    #[staticmethod]
    fn mul_short(value: &Base) -> Self {
        let hasher = ValueCommit::hash_to_curve(VALUE_COMMITMENT_PERSONALIZATION);
        let v = hasher(&VALUE_COMMITMENT_V_BYTES);
        Self(v * mod_r_p(value.0))
    }

    // Why value doesn't need to be a Pycell?
    #[staticmethod]
    fn mul_base(value: &Base) -> Self {
        let v = NullifierK.generator();
        Self(v * mod_r_p(value.0))
    }

    // Why not a pycell?
    #[staticmethod]
    fn mul_r_generator(blind: &Scalar) -> Self {
        let hasher = ValueCommit::hash_to_curve(VALUE_COMMITMENT_PERSONALIZATION);
        let r = hasher(&VALUE_COMMITMENT_R_BYTES);
        let r = Self(r);
        Self(r.0 * blind.0)
    }

    fn __str__(&self) -> String {
        format!("{:?}", self.0)
    }

    fn __repr__(slf: &PyCell<Self>) -> PyResult<String> {
        let class_name: &str = slf.get_type().name()?;
        Ok(format!("{}({:?})", class_name, slf.borrow().0))
    }

    fn to_affine(&self) -> Affine {
        Affine(self.0.to_affine())
    }

    fn __add__(&self, rhs: &Self) -> Self {
        Self(self.0 + rhs.0)
    }

    fn __sub__(&self, rhs: &Self) -> Self {
        Self(self.0 - rhs.0)
    }

    fn __mul__(&self, scalar: &Scalar) -> Self {
        Self(self.0 * scalar.0)
    }

    fn __neg__(&self) -> Self {
        Self(-self.0)
    }

    fn __richcmp__(&self, other: &Self, op: CompareOp) -> PyResult<bool> {
        match op {
            CompareOp::Eq => Ok(self.0 == other.0),
            CompareOp::Ne => Ok(self.0 != other.0),
            CompareOp::Lt => todo!(),
            CompareOp::Le => todo!(),
            CompareOp::Gt => todo!(),
            CompareOp::Ge => todo!(),
        }
    }
}

pub fn create_module(py: pyo3::Python<'_>) -> pyo3::PyResult<&PyModule> {
    let submod = PyModule::new(py, "point")?;
    submod.add_class::<Point>()?;
    Ok(submod)
}
