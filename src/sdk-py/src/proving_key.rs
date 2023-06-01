use crate::zk_circuit::ZkCircuit;
use darkfi::zk::{proof, vm};
use pyo3::prelude::*;
use std::ops::Deref;

#[pyclass]
pub struct ProvingKey(pub(crate) proof::ProvingKey);

#[pymethods]
impl ProvingKey {
    #[staticmethod]
    fn build(k: u32, circuit: &PyCell<ZkCircuit>) -> Self {
        let circuit_ref = circuit.borrow();
        let circuit: &vm::ZkCircuit = &circuit_ref.deref().0;
        let proving_key = proof::ProvingKey::build(k, circuit);
        Self(proving_key)
    }
}

pub fn create_module(py: pyo3::Python<'_>) -> pyo3::PyResult<&PyModule> {
    let submod = PyModule::new(py, "proving_key")?;
    submod.add_class::<ProvingKey>()?;
    Ok(submod)
}
