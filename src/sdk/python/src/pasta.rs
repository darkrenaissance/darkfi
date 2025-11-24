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

use std::ops::Deref;

use darkfi_sdk::{
    crypto::{constants::NullifierK, pasta_prelude::*, util},
    pasta::{pallas, vesta},
};
use halo2_gadgets::ecc::chip::FixedPoint;
use pyo3::{
    basic::CompareOp,
    pyclass, pyfunction, pymethods,
    types::{PyAnyMethods, PyModule, PyModuleMethods, PyStringMethods, PyTypeMethods},
    wrap_pyfunction, Bound, PyResult,
};
use rand::rngs::OsRng;

macro_rules! impl_elem {
    ($x:ty, $inner:ty) => {
        #[pymethods]
        impl $x {
            #[new]
            fn new(v: &str) -> PyResult<Self> {
                assert!(v.starts_with("0x") && v.len() == 66);
                let v = v.trim_start_matches("0x");
                let (a, b) = v.split_at(32);
                let (le_1, le_0) = b.split_at(16);
                let (le_3, le_2) = a.split_at(16);

                let le_0 = u64::from_str_radix(le_0, 16)?;
                let le_1 = u64::from_str_radix(le_1, 16)?;
                let le_2 = u64::from_str_radix(le_2, 16)?;
                let le_3 = u64::from_str_radix(le_3, 16)?;

                Ok(Self(<$inner>::from_raw([le_0, le_1, le_2, le_3])))
            }

            #[staticmethod]
            fn from_u64(v: u64) -> Self {
                Self(<$inner>::from(v))
            }

            #[staticmethod]
            fn from_u128(v: u128) -> Self {
                Self(<$inner>::from_u128(v))
            }

            #[staticmethod]
            const fn from_raw(v: [u64; 4]) -> Self {
                Self(<$inner>::from_raw(v))
            }

            #[staticmethod]
            fn from_uniform_bytes(bytes: [u8; 64]) -> Self {
                Self(<$inner>::from_uniform_bytes(&bytes))
            }

            #[staticmethod]
            fn random() -> Self {
                Self(<$inner>::random(&mut OsRng))
            }

            #[staticmethod]
            fn modulus() -> &'static str {
                <$inner>::MODULUS
            }

            #[staticmethod]
            fn zero() -> Self {
                Self(<$inner>::ZERO)
            }

            #[staticmethod]
            fn one() -> Self {
                Self(<$inner>::ONE)
            }

            fn double(&self) -> Self {
                Self(self.0.double())
            }

            fn square(&self) -> Self {
                Self(self.0.square())
            }

            fn __str__(&self) -> PyResult<String> {
                Ok(format!("{:?}", self.0))
            }

            fn __repr__(slf: &Bound<Self>) -> PyResult<String> {
                Ok(format!("{}({:?})", slf.get_type().name()?.to_str()?, slf.borrow().0))
            }

            fn __add__(&self, other: &Self) -> Self {
                Self(self.0 + other.0)
            }

            fn __sub__(&self, other: &Self) -> Self {
                Self(self.0 - other.0)
            }

            fn __mul__(&self, other: &Self) -> Self {
                Self(self.0 * other.0)
            }

            fn __neg__(&self) -> Self {
                Self(self.0.neg())
            }

            fn __richcmp__(&self, other: &Self, op: CompareOp) -> PyResult<bool> {
                match op {
                    CompareOp::Lt => Ok(self.0 < other.0),
                    CompareOp::Le => Ok(self.0 <= other.0),
                    CompareOp::Eq => Ok(self.0 == other.0),
                    CompareOp::Ne => Ok(self.0 != other.0),
                    CompareOp::Gt => Ok(self.0 > other.0),
                    CompareOp::Ge => Ok(self.0 >= other.0),
                }
            }
        }
    };
}

macro_rules! impl_affine {
    ($x:ty, $inner:ty, $base:ident, $projective:ty) => {
        #[pymethods]
        impl $x {
            fn coordinates(&self) -> ($base, $base) {
                let coords = self.0.coordinates().unwrap();
                ($base(*coords.x()), $base(*coords.y()))
            }

            fn coordinates_str(&self) -> PyResult<Vec<String>> {
                let coords = self.0.coordinates().unwrap();
                let x = $base(*coords.x()).__str__()?;
                let y = $base(*coords.y()).__str__()?;
                Ok(vec![x, y])
            }

            #[staticmethod]
            fn from_xy(x: &Bound<$base>, y: &Bound<$base>) -> PyResult<Self> {
                let affine_point =
                    <$inner>::from_xy(x.borrow().deref().0, y.borrow().deref().0).unwrap();
                Ok(Self(affine_point))
            }

            #[staticmethod]
            fn from_projective(x: &Bound<$projective>) -> Self {
                Self(<$inner>::from(x.borrow().deref().0))
            }

            fn __str__(&self) -> String {
                format!("{:?}", self.0)
            }

            fn __repr__(slf: &Bound<Self>) -> PyResult<String> {
                Ok(format!("{}({:?})", slf.get_type().name()?.to_str()?, slf.borrow().0))
            }
        }
    };
}

macro_rules! impl_point {
    ($x:ty, $inner:ty, $base:ty, $scalar:ty, $affine:ty) => {
        #[pymethods]
        impl $x {
            #[new]
            fn new(x: &Bound<$base>, y: &Bound<$base>) -> PyResult<Self> {
                let affine_point = <$affine>::from_xy(x, y).unwrap();
                Ok(Self::from_affine(affine_point))
            }

            #[staticmethod]
            fn identity() -> Self {
                Self(<$inner>::identity())
            }

            #[staticmethod]
            fn generator() -> Self {
                Self(<$inner>::generator())
            }

            #[staticmethod]
            fn random() -> Self {
                Self(<$inner>::random(&mut OsRng))
            }

            #[staticmethod]
            fn from_affine(p: $affine) -> Self {
                Self(<$inner>::from(p.0))
            }

            fn __str__(slf: &Bound<Self>) -> PyResult<String> {
                let affine = <$affine>::from_projective(slf);
                let (x, y) = affine.coordinates();
                Ok(format!("[{}, {}]", x.__str__()?, y.__str__()?))
            }

            fn __repr__(slf: &Bound<Self>) -> PyResult<String> {
                Ok(format!("{}({:?})", slf.get_type().name()?.to_str()?, slf.borrow().0))
            }

            fn __add__(&self, rhs: &Self) -> Self {
                Self(self.0 + rhs.0)
            }

            fn __sub__(&self, rhs: &Self) -> Self {
                Self(self.0 - rhs.0)
            }

            fn __mul__(&self, scalar: &$scalar) -> Self {
                Self(self.0 * scalar.0)
            }

            fn __neg__(&self) -> Self {
                Self(-self.0)
            }

            fn __richcmp__(&self, other: &Self, op: CompareOp) -> PyResult<bool> {
                match op {
                    CompareOp::Eq => Ok(self.0 == other.0),
                    CompareOp::Ne => Ok(self.0 != other.0),
                    CompareOp::Lt => unimplemented!(),
                    CompareOp::Le => unimplemented!(),
                    CompareOp::Gt => unimplemented!(),
                    CompareOp::Ge => unimplemented!(),
                }
            }
        }
    };
}

/// The base field of the Pallas curve and the scalar field of the Vesta curve.
#[pyclass(dict)]
#[derive(Copy, Clone, PartialEq, std::cmp::Eq, Ord, PartialOrd, Debug)]
pub struct Fp(pub(crate) pallas::Base);
impl_elem!(Fp, pallas::Base);

/// The scalar field of the Pallas curve and the base field of the Vesta curve.
#[pyclass]
#[derive(Copy, Clone, PartialEq, std::cmp::Eq, Ord, PartialOrd, Debug)]
pub struct Fq(pub(crate) pallas::Scalar);
impl_elem!(Fq, pallas::Scalar);

/// A Pallas curve point in the projective space
#[pyclass]
#[derive(Copy, Clone, PartialEq, std::cmp::Eq, Debug)]
pub struct Ep(pub(crate) pallas::Point);
impl_point!(Ep, pallas::Point, Fp, Fq, EpAffine);

/// A Pallas curve point in the affine space
#[pyclass]
#[derive(Copy, Clone, PartialEq, std::cmp::Eq, Debug)]
pub struct EpAffine(pub(crate) pallas::Affine);
impl_affine!(EpAffine, pallas::Affine, Fp, Ep);

/// A Vesta curve point in the projective space
#[pyclass]
#[derive(Copy, Clone, PartialEq, std::cmp::Eq, Debug)]
pub struct Eq(pub(crate) vesta::Point);
impl_point!(Eq, vesta::Point, Fq, Fp, EqAffine);

/// A Vesta curve point in the affine space
#[pyclass]
#[derive(Copy, Clone, PartialEq, std::cmp::Eq, Debug)]
pub struct EqAffine(pub(crate) vesta::Affine);
impl_affine!(EqAffine, vesta::Affine, Fq, Eq);

#[pyfunction]
/// Return the NullifierK generator point as EpAffine.
pub fn nullifier_k() -> EpAffine {
    EpAffine(NullifierK.generator())
}

#[pyfunction]
/// Convert Fp to Fq safely.
pub fn fp_mod_fv(x: &Bound<Fp>) -> Fq {
    Fq(util::fp_mod_fv(x.borrow().deref().0))
}

pub fn create_module(py: pyo3::Python<'_>) -> PyResult<Bound<'_, PyModule>> {
    let submod = PyModule::new(py, "pasta")?;

    submod.add_class::<Fp>()?;
    submod.add_class::<Fq>()?;
    submod.add_class::<Ep>()?;
    submod.add_class::<EpAffine>()?;
    submod.add_class::<Eq>()?;
    submod.add_class::<EqAffine>()?;

    submod.add_function(wrap_pyfunction!(nullifier_k, &submod)?)?;
    submod.add_function(wrap_pyfunction!(fp_mod_fv, &submod)?)?;

    Ok(submod)
}
