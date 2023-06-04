use crate::affine::Affine;
use crate::base::Base;
use crate::scalar::Scalar;
use darkfi_sdk::{
    crypto::{
        constants::{
            fixed_bases::{
                VALUE_COMMITMENT_PERSONALIZATION, VALUE_COMMITMENT_R_BYTES,
                VALUE_COMMITMENT_V_BYTES,
            },
            NullifierK,
        },
        pallas,
        util::mod_r_p,
        ValueCommit,
    },
    pasta::{
        arithmetic::CurveExt,
        group::{Curve, Group},
    },
};
use halo2_gadgets::ecc::chip::FixedPoint;
use pyo3::prelude::*;
use std::ops::{Add, Mul};

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
        // QUESTION: Why does v need to be a random element from EP?
        // Why not NullifierK.generator() or some other pre-determined generator?
        let hasher = ValueCommit::hash_to_curve(VALUE_COMMITMENT_PERSONALIZATION);
        let v = hasher(&VALUE_COMMITMENT_V_BYTES);
        Self(v * mod_r_p(value.0))
    }

    // why value doesn't need to be a Pycell
    #[staticmethod]
    fn mul_base(value: &Base) -> Self {
        let v = NullifierK.generator();
        Self(v * mod_r_p(value.0))
    }

    // why not a pycell
    #[staticmethod]
    fn mul_r_generator(blind: &Scalar) -> Self {
        let hasher = ValueCommit::hash_to_curve(VALUE_COMMITMENT_PERSONALIZATION);
        let r = hasher(&VALUE_COMMITMENT_R_BYTES);
        let r = Self(r);
        r.mul(blind)
    }

    #[pyo3(name = "__str__")]
    fn __str__(&self) -> String {
        format!("Point({:?})", self.0)
    }

    fn to_affine(&self) -> Affine {
        Affine(self.0.to_affine())
    }

    fn add(&self, rhs: &Self) -> Self {
        Self(self.0.add(rhs.0))
    }

    fn mul(&self, scalar: &Scalar) -> Self {
        Self(self.0.mul(scalar.0))
    }
}

pub fn create_module(py: pyo3::Python<'_>) -> pyo3::PyResult<&PyModule> {
    let submod = PyModule::new(py, "point")?;
    submod.add_class::<Point>()?;
    Ok(submod)
}
