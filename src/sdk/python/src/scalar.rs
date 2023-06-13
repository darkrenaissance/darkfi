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
    crypto::pasta_prelude::{Field, PrimeField},
    pasta::{group::ff::FromUniformBytes, pallas},
};
use pyo3::{basic::CompareOp, prelude::*};
use rand::rngs::OsRng;

/// The scalar field of the Pallas and iso-Pallas curves.
#[pyclass]
pub struct Scalar(pub(crate) pallas::Scalar);

#[pymethods]
impl Scalar {
    #[staticmethod]
    fn from_u64(v: u64) -> Self {
        Self(pallas::Scalar::from(v))
    }

    #[staticmethod]
    fn from_u128(v: u128) -> Self {
        Self(pallas::Scalar::from_u128(v))
    }

    #[staticmethod]
    fn from_raw(v: [u64; 4]) -> Self {
        Self(pallas::Scalar::from_raw(v))
    }

    #[staticmethod]
    fn from_uniform_bytes(bytes: [u8; 64]) -> Self {
        Self(pallas::Scalar::from_uniform_bytes(&bytes))
    }

    #[staticmethod]
    fn random() -> Self {
        Self(pallas::Scalar::random(&mut OsRng))
    }

    #[staticmethod]
    fn modulus() -> String {
        pallas::Scalar::MODULUS.to_string()
    }

    #[staticmethod]
    fn zero() -> Self {
        Self(pallas::Scalar::zero())
    }

    #[staticmethod]
    fn one() -> Self {
        Self(pallas::Scalar::one())
    }

    fn __str__(&self) -> String {
        format!("{:?}", self.0)
    }

    fn __repr__(slf: &PyCell<Self>) -> PyResult<String> {
        let class_name: &str = slf.get_type().name()?;
        Ok(format!("{}({:?})", class_name, slf.borrow().0))
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

    fn double(&self) -> Self {
        Self(self.0.double())
    }

    fn square(&self) -> Self {
        Self(self.0.square())
    }
}

pub fn create_module(py: pyo3::Python<'_>) -> pyo3::PyResult<&PyModule> {
    let submod = PyModule::new(py, "scalar")?;
    submod.add_class::<Scalar>()?;
    Ok(submod)
}
