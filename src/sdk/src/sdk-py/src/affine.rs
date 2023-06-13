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

use crate::base::Base;
use pasta_curves::{arithmetic::CurveAffine, pallas};
use pyo3::prelude::*;

/// A Pallas point in the affine coordinate space (or the point at infinity).
#[pyclass]
pub struct Affine(pub(crate) pallas::Affine);

#[pymethods]
impl Affine {
    fn __str__(&self) -> String {
        format!("Affine({:?})", self.0)
    }

    fn coordinates(&self) -> (Base, Base) {
        let coords = self.0.coordinates().unwrap();
        (Base(*coords.x()), Base(*coords.y()))
    }
}

pub fn create_module(py: pyo3::Python<'_>) -> pyo3::PyResult<&PyModule> {
    let submod = PyModule::new(py, "affine")?;
    submod.add_class::<Affine>()?;
    Ok(submod)
}
