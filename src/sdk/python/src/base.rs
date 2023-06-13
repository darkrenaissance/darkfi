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

use std::ops::Deref;

use darkfi_sdk::{
    bridgetree::Hashable,
    crypto::{poseidon_hash, MerkleNode},
    pasta::{
        group::ff::{Field, FromUniformBytes, PrimeField},
        pallas,
    },
};
use pyo3::prelude::*;
use rand::rngs::OsRng;

/// The base field of the Pallas and iso-Pallas curves.
/// Randomness is provided by the OS and on the Rust side.
#[pyclass]
pub struct Base(pub(crate) pallas::Base);

#[pymethods]
impl Base {
    // Why is this not callable?
    #[new]
    fn from_u64(v: u64) -> Self {
        Self(pallas::Base::from(v))
    }

    #[staticmethod]
    fn from_raw(v: [u64; 4]) -> Self {
        Self(pallas::Base::from_raw(v))
    }

    #[staticmethod]
    fn from_uniform_bytes(bytes: [u8; 64]) -> Self {
        Self(pallas::Base::from_uniform_bytes(&bytes))
    }

    #[staticmethod]
    fn random() -> Self {
        Self(pallas::Base::random(&mut OsRng))
    }

    #[staticmethod]
    fn modulus() -> String {
        pallas::Base::MODULUS.to_string()
    }

    #[staticmethod]
    fn zero() -> Self {
        Self(pallas::Base::zero())
    }

    #[staticmethod]
    fn one() -> Self {
        Self(pallas::Base::one())
    }

    #[staticmethod]
    fn poseidon_hash(messages: Vec<&PyCell<Self>>) -> Self {
        let l = messages.len();
        let messages: Vec<pallas::Base> = messages.iter().map(|m| m.borrow().deref().0).collect();
        if l == 1 {
            let m: [pallas::Base; 1] = messages.try_into().unwrap();
            Self(poseidon_hash(m))
        } else if l == 2 {
            let m: [pallas::Base; 2] = messages.try_into().unwrap();
            Self(poseidon_hash(m))
        } else if l == 3 {
            let m: [pallas::Base; 3] = messages.try_into().unwrap();
            Self(poseidon_hash(m))
        } else if l == 4 {
            let m: [pallas::Base; 4] = messages.try_into().unwrap();
            Self(poseidon_hash(m))
        } else if l == 5 {
            let m: [pallas::Base; 5] = messages.try_into().unwrap();
            Self(poseidon_hash(m))
        } else if l == 6 {
            let m: [pallas::Base; 6] = messages.try_into().unwrap();
            Self(poseidon_hash(m))
        } else if l == 7 {
            let m: [pallas::Base; 7] = messages.try_into().unwrap();
            Self(poseidon_hash(m))
        } else if l == 8 {
            let m: [pallas::Base; 8] = messages.try_into().unwrap();
            Self(poseidon_hash(m))
        } else if l == 9 {
            let m: [pallas::Base; 9] = messages.try_into().unwrap();
            Self(poseidon_hash(m))
        } else if l == 10 {
            let m: [pallas::Base; 10] = messages.try_into().unwrap();
            Self(poseidon_hash(m))
        } else if l == 11 {
            let m: [pallas::Base; 11] = messages.try_into().unwrap();
            Self(poseidon_hash(m))
        } else if l == 12 {
            let m: [pallas::Base; 12] = messages.try_into().unwrap();
            Self(poseidon_hash(m))
        } else if l == 13 {
            let m: [pallas::Base; 13] = messages.try_into().unwrap();
            Self(poseidon_hash(m))
        } else if l == 14 {
            let m: [pallas::Base; 14] = messages.try_into().unwrap();
            Self(poseidon_hash(m))
        } else if l == 15 {
            let m: [pallas::Base; 15] = messages.try_into().unwrap();
            Self(poseidon_hash(m))
        } else if l == 16 {
            let m: [pallas::Base; 16] = messages.try_into().unwrap();
            Self(poseidon_hash(m))
        } else {
            panic!("Messages length violation, must be: 1 <= len <= 16");
        }
    }

    /// pos(ition) encodes the left/right position on each level
    /// path is the the silbling on each level
    #[staticmethod]
    fn merkle_root(i: u64, p: Vec<&PyCell<Base>>, a: &Base) -> Self {
        // TOOD: consider adding length check, for i and path, for extra defensiness
        let mut current = MerkleNode::new(a.0);
        for (level, sibling) in p.iter().enumerate() {
            let level = level as u8;
            let sibling = MerkleNode::new(sibling.borrow().deref().0);
            current = if i & (1 << level) == 0 {
                MerkleNode::combine(level.into(), &current, &sibling)
            } else {
                MerkleNode::combine(level.into(), &sibling, &current)
            };
        }
        let root = current.inner();
        Self(root)
    }

    // For some reason, the name needs to be explictely stated
    // for Python to correctly implement
    #[pyo3(name = "__str__")]
    fn __str_(&self) -> String {
        format!("Base({:?})", self.0)
    }

    #[pyo3(name = "__repr__")]
    fn __repr_(&self) -> String {
        format!("Base({:?})", self.0)
    }

    fn eq(&self, rhs: &Self) -> bool {
        self.0.eq(&rhs.0)
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
    let submod = PyModule::new(py, "base")?;
    submod.add_class::<Base>()?;
    Ok(submod)
}
