/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

use pyo3::{prelude::PyModule, pyclass, pymethods, PyResult, Python};
use sled_overlay::sled;

#[pyclass]
pub struct SledDb(pub sled::Db);

#[pymethods]
impl SledDb {
    #[new]
    fn new(pathpy: String) -> Self {
        let path: &std::path::Path = std::path::Path::new(&pathpy);
        let db_res = sled::open(path);
        Self(db_res.unwrap())
    }
}

pub(crate) fn create_module(py: Python<'_>) -> PyResult<&PyModule> {
    let submod = PyModule::new(py, "sled")?;
    submod.add_class::<SledDb>()?;
    Ok(submod)
}
