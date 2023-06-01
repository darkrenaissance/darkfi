use crate::zk_circuit::ZkCircuit;
use darkfi::zk::proof;
use pyo3::prelude::*;
use std::ops::Deref;

#[pyclass]
pub struct VerifyingKey(pub(crate) proof::VerifyingKey);

#[pymethods]
impl VerifyingKey {
    #[staticmethod]
    fn build(k: u32, circuit: &PyCell<ZkCircuit>) -> Self {
        let circuit_ref = circuit.borrow();
        let circuit = &circuit_ref.deref().0;
        let proving_key = proof::VerifyingKey::build(k, circuit);
        Self(proving_key)
    }
}

pub fn create_module(py: pyo3::Python<'_>) -> pyo3::PyResult<&PyModule> {
    let submod = PyModule::new(py, "verifying_key")?;
    submod.add_class::<VerifyingKey>()?;
    Ok(submod)
}
