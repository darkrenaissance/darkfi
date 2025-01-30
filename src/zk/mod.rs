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

/// Halo2 zkas virtual machine
pub mod vm;
pub use vm::ZkCircuit;

/// VM heap variable definitions and utility functions
pub mod vm_heap;
pub use vm_heap::{empty_witnesses, Witness};

/// ZK gadget implementations
pub mod gadget;

/// Proof creation API
pub mod proof;
pub use proof::{Proof, ProvingKey, VerifyingKey};

/// Trace computation of intermediate values in circuit
mod tracer;
pub use tracer::DebugOpValue;

mod debug;
pub use debug::zkas_type_checks;
#[cfg(feature = "tinyjson")]
pub use debug::{export_witness_json, import_witness_json};

pub mod halo2 {
    pub use halo2_proofs::{
        arithmetic::Field,
        circuit::{AssignedCell, Layouter, Value},
        dev, plonk,
        plonk::{Advice, Assigned, Column},
    };
}

//pub(in crate::zk) fn assign_free_advice<F: Field, V: Copy>(
pub fn assign_free_advice<F: halo2::Field, V: Copy>(
    mut layouter: impl halo2::Layouter<F>,
    column: halo2::Column<halo2::Advice>,
    value: halo2::Value<V>,
) -> Result<halo2::AssignedCell<V, F>, halo2::plonk::Error>
where
    for<'v> halo2::Assigned<F>: From<&'v V>,
{
    layouter.assign_region(
        || "load private",
        |mut region| region.assign_advice(|| "load private", column, 0, || value),
    )
}
