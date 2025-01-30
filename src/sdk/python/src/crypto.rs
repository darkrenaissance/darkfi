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

use darkfi_sdk::{crypto, pasta::pallas};
use pyo3::{
    prelude::{PyModule, PyModuleMethods},
    pyfunction, wrap_pyfunction, Bound, PyResult, Python,
};

use super::pasta::{Ep, Fp, Fq};

/// Calculate the Poseidon hash of given `Fp` elements.
#[pyfunction]
pub fn poseidon_hash(messages: Vec<Bound<Fp>>) -> Fp {
    let messages: Vec<pallas::Base> = messages.iter().map(|x| x.borrow().deref().0).collect();
    match messages.len() {
        1 => Fp(crypto::util::poseidon_hash::<1>(messages.try_into().unwrap())),
        2 => Fp(crypto::util::poseidon_hash::<2>(messages.try_into().unwrap())),
        3 => Fp(crypto::util::poseidon_hash::<3>(messages.try_into().unwrap())),
        4 => Fp(crypto::util::poseidon_hash::<4>(messages.try_into().unwrap())),
        5 => Fp(crypto::util::poseidon_hash::<5>(messages.try_into().unwrap())),
        6 => Fp(crypto::util::poseidon_hash::<6>(messages.try_into().unwrap())),
        7 => Fp(crypto::util::poseidon_hash::<7>(messages.try_into().unwrap())),
        8 => Fp(crypto::util::poseidon_hash::<8>(messages.try_into().unwrap())),
        9 => Fp(crypto::util::poseidon_hash::<9>(messages.try_into().unwrap())),
        10 => Fp(crypto::util::poseidon_hash::<10>(messages.try_into().unwrap())),
        11 => Fp(crypto::util::poseidon_hash::<11>(messages.try_into().unwrap())),
        12 => Fp(crypto::util::poseidon_hash::<12>(messages.try_into().unwrap())),
        13 => Fp(crypto::util::poseidon_hash::<13>(messages.try_into().unwrap())),
        14 => Fp(crypto::util::poseidon_hash::<14>(messages.try_into().unwrap())),
        15 => Fp(crypto::util::poseidon_hash::<15>(messages.try_into().unwrap())),
        16 => Fp(crypto::util::poseidon_hash::<16>(messages.try_into().unwrap())),
        _ => unimplemented!(),
    }
}

/// Calculate a Pedersen commitment with an u64 value.
#[pyfunction]
pub fn pedersen_commitment_u64(value: u64, blind: &Bound<Fq>) -> Ep {
    Ep(crypto::pedersen::pedersen_commitment_u64(value, crypto::Blind(blind.borrow().deref().0)))
}

/// Calculate a Pedersen commitment with an Fp value.
#[pyfunction]
pub fn pedersen_commitment_base(value: &Bound<Fp>, blind: &Bound<Fq>) -> Ep {
    Ep(crypto::pedersen::pedersen_commitment_base(
        value.borrow().deref().0,
        crypto::Blind(blind.borrow().deref().0),
    ))
}

/// Wrapper function for creating this Python module.
pub(crate) fn create_module(py: Python<'_>) -> PyResult<Bound<PyModule>> {
    let submod = PyModule::new_bound(py, "crypto")?;
    submod.add_function(wrap_pyfunction!(poseidon_hash, &submod)?)?;
    submod.add_function(wrap_pyfunction!(pedersen_commitment_u64, &submod)?)?;
    submod.add_function(wrap_pyfunction!(pedersen_commitment_base, &submod)?)?;
    Ok(submod)
}
