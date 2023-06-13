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

use darkfi_sdk::crypto::pasta_prelude::{Field, PrimeField};
use pasta_curves::pallas;
use pyo3::prelude::*;
use rand::rngs::OsRng;

/// The scalar field of the Pallas and iso-Pallas curves.
#[pyclass]
pub struct Scalar(pub(crate) pallas::Scalar);

#[pymethods]
impl Scalar {
    #[new]
    fn from_u128(v: u128) -> Self {
        Self(pallas::Scalar::from_u128(v))
    }

    #[staticmethod]
    fn from_raw(v: [u64; 4]) -> Self {
        Self(pallas::Scalar::from_raw(v))
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

    #[pyo3(name = "__str__")]
    fn __str__(&self) -> String {
        format!("Scalar({:?})", self.0)
    }

    #[pyo3(name = "__repr__")]
    fn __repr__(&self) -> String {
        format!("Scalar({:?})", self.0)
    }

    fn add(&self, rhs: &Self) -> Self {
        Self(self.0.add(&rhs.0))
    }

    fn sub(&self, rhs: &Self) -> Self {
        Self(self.0.sub(&rhs.0))
    }

    fn double(&self) -> Self {
        Self(self.0.double())
    }

    fn mul(&self, rhs: &Self) -> Self {
        Self(self.0.mul(&rhs.0))
    }

    fn neg(&self) -> Self {
        Self(self.0.neg())
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
