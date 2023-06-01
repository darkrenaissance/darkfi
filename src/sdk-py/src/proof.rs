use crate::base::Base;
use crate::proving_key::ProvingKey;
use crate::verifying_key::VerifyingKey;
use crate::zk_circuit::ZkCircuit;
use darkfi::zk::{proof, vm};
use darkfi_sdk::crypto::pallas;
use pyo3::prelude::*;
use rand::rngs::OsRng;
use std::ops::Deref;

#[pyclass]
pub struct Proof(pub(crate) proof::Proof);

#[pymethods]
impl Proof {
    #[staticmethod]
    fn create(
        pk: &PyCell<ProvingKey>,
        circuits: Vec<&PyCell<ZkCircuit>>,
        instances: Vec<&PyCell<Base>>,
    ) -> Self {
        let pk = pk.borrow().deref().0.clone();
        let circuits: Vec<vm::ZkCircuit> =
            circuits.iter().map(|c| c.borrow().deref().0.clone()).collect();
        let instances: Vec<pallas::Base> = instances.iter().map(|i| i.borrow().deref().0).collect();
        let proof =
            proof::Proof::create(&pk, circuits.as_slice(), instances.as_slice(), &mut OsRng);
        let proof = proof.unwrap();
        Self(proof)
    }

    fn verify(&self, vk: &PyCell<VerifyingKey>, instances: Vec<&PyCell<Base>>) {
        let vk = vk.borrow().deref().0.clone();
        let proof = self.0.clone();
        let instances: Vec<pallas::Base> = instances.iter().map(|i| i.borrow().deref().0).collect();
        proof.verify(&vk, instances.as_slice()).unwrap();
    }
}

pub fn create_module(py: pyo3::Python<'_>) -> pyo3::PyResult<&PyModule> {
    let submod = PyModule::new(py, "proof")?;
    submod.add_class::<Proof>()?;
    Ok(submod)
}
