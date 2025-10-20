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

use darkfi_sdk::crypto::{merkle_node, MerkleNode};
use pyo3::{
    prelude::{PyModule, PyModuleMethods},
    pyclass, pymethods, Bound, PyResult,
};

use super::pasta::Fp;

#[pyclass]
/// Class representing a bridgetree
pub struct MerkleTree(merkle_node::MerkleTree);

#[pymethods]
impl MerkleTree {
    #[new]
    fn new() -> Self {
        Self(merkle_node::MerkleTree::new(1))
    }

    fn append(&mut self, node: &Bound<Fp>) -> bool {
        self.0.append(MerkleNode::from(node.borrow().deref().0))
    }

    fn mark(&mut self) -> u32 {
        u64::from(self.0.mark().unwrap()) as u32
    }

    fn root(&self, checkpoint_depth: usize) -> Fp {
        let root = self.0.root(checkpoint_depth).unwrap();
        Fp(root.inner())
    }

    fn witness(&self, position: u32, checkpoint_depth: usize) -> Vec<Fp> {
        let path = self.0.witness((position as u64).into(), checkpoint_depth).unwrap();
        path.iter().map(|x| Fp(x.inner())).collect()
    }
}

/// Wrapper function for creating this Python module.
pub(crate) fn create_module(py: pyo3::Python<'_>) -> PyResult<Bound<PyModule>> {
    let submod = PyModule::new(py, "merkle")?;
    submod.add_class::<MerkleTree>()?;
    Ok(submod)
}
