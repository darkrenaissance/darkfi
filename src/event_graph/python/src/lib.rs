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

mod event_graph;
mod p2p;
mod sled;

#[pyo3::prelude::pymodule]
fn darkfi_eventgraph_py(
    py: pyo3::Python<'_>,
    m: &pyo3::Bound<'_, pyo3::prelude::PyModule>,
) -> pyo3::PyResult<()> {
    // sled
    let submodule = sled::create_module(py)?;
    pyo3::py_run!(
        py,
        submodule,
        "import sys; sys.modules['darkfi_eventgraph_py.sled'] = submodule"
    );
    pyo3::types::PyModuleMethods::add_submodule(m, &submodule)?;
    // p2p
    let submodule = p2p::create_module(py)?;
    pyo3::py_run!(py, submodule, "import sys; sys.modules['darkfi_eventgraph_py.p2p'] = submodule");
    pyo3::types::PyModuleMethods::add_submodule(m, &submodule)?;
    // event_graph
    let submodule = event_graph::create_module(py)?;
    pyo3::py_run!(
        py,
        submodule,
        "import sys; sys.modules['darkfi_eventgraph_py.event_graph'] = submodule"
    );
    pyo3::types::PyModuleMethods::add_submodule(m, &submodule)?;
    //
    Ok(())
}
