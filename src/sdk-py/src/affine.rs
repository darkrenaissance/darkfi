use crate::base::Base;
use darkfi_sdk::{crypto::pallas, pasta::arithmetic::CurveAffine};
use pyo3::prelude::*;

/// A Pallas point in the affine coordinate space (or the point at infinity).
#[pyclass]
pub struct Affine(pub(crate) pallas::Affine);

#[pymethods]
impl Affine {
    fn __str__(&self) -> String {
        format!("Affine({:?})", self.0)
    }

    fn coordinates(&self) -> (Base, Base) {
        let coords = self.0.coordinates().unwrap();
        (Base(*coords.x()), Base(*coords.y()))
    }
}

pub fn create_module(py: pyo3::Python<'_>) -> pyo3::PyResult<&PyModule> {
    let submod = PyModule::new(py, "affine")?;
    submod.add_class::<Affine>()?;
    Ok(submod)
}
