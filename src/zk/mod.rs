/// Halo2 zkas virtual machine
pub mod vm;
pub mod vm_stack;

/// ZK circuits
pub mod circuit;

/// ZK gadget implementations
pub mod gadget;


use halo2_proofs::{
    arithmetic::Field,
    circuit::{AssignedCell, Layouter, Value},
    plonk,
    plonk::{Advice, Assigned, Column},
};

//pub(in crate::zk) fn assign_free_advice<F: Field, V: Copy>(
pub fn assign_free_advice<F: Field, V: Copy>(
    mut layouter: impl Layouter<F>,
    column: Column<Advice>,
    value: Value<V>,
) -> Result<AssignedCell<V, F>, plonk::Error>
where
    for<'v> Assigned<F>: From<&'v V>,
{
    layouter.assign_region(
        || "load private",
        |mut region| region.assign_advice(|| "load private", column, 0, || value),
    )
}
