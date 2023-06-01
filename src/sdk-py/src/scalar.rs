use darkfi_sdk::crypto::{
    pallas,
    pasta_prelude::{Field, PrimeField},
};
use pyo3::prelude::*;
use rand::rngs::OsRng;

/// Why does Vesta use Fq?
/// The scalar field of the Pallas and iso-Pallas curves.
#[pyclass]
pub struct Scalar(pub(crate) pallas::Scalar);

#[pymethods]
impl Scalar {
    #[staticmethod]
    fn from_raw(v: [u64; 4]) -> Self {
        Self(pallas::Scalar::from_raw(v))
    }

    #[staticmethod]
    fn from_u128(v: u128) -> Self {
        Self(pallas::Scalar::from_u128(v))
    }

    #[staticmethod]
    fn random() -> Self {
        Self(pallas::Scalar::random(&mut OsRng))
    }

    #[staticmethod]
    fn modulus() -> String {
        pallas::Scalar::MODULUS.to_string()
    }

    #[staticmethod]
    fn zero() -> Self {
        Self(pallas::Scalar::zero())
    }

    #[staticmethod]
    fn one() -> Self {
        Self(pallas::Scalar::one())
    }

    fn __str__(&self) -> String {
        format!("Scalar({:?})", self.0)
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
    let submod = PyModule::new(py, "scalar")?;
    submod.add_class::<Scalar>()?;
    Ok(submod)
}
