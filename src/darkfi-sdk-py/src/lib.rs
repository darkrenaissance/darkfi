use darkfi_sdk::crypto::{
    pallas,
    pasta_prelude::{Field, PrimeField},
};
use pyo3::prelude::*;
use rand::rngs::OsRng;

#[pyclass]
struct PallasBaseWrapper(pallas::Base);

/// This represents an element of $\mathbb{F}_p$ where
///
/// `p = 0x40000000000000000000000000000000224698fc094cf91b992d30ed00000001`
///
/// is the base field of the Pallas curve.
// The internal representation of this type is four 64-bit unsigned
// integers in little-endian order. `Fp` values are always in
// Montgomery form; i.e., Fp(a) = aR mod p, with R = 2^256.
#[pymethods]
impl PallasBaseWrapper {
    /// For now, we work with 128 bits in Python.
    /// Becasue pallas::Base has nice debug formatting for it.
    /// TODO: change it from_raw
    #[new]
    fn from_u128(v: u128) -> Self {
        Self(pallas::Base::from_u128(v))
    }
    
    #[staticmethod]
    fn random() -> PallasBaseWrapper {
        PallasBaseWrapper(pallas::Base::random(&mut OsRng))
    }

    #[staticmethod]
    fn modulus() -> String {
        pallas::Base::MODULUS.to_string()
    }

    fn __repr__(&self) -> String {
        format!("PallasBaseWrapper({:?})", self.0)
    }

    fn add(&self, rhs: &Self) -> Self {
        Self(self.0.add(&rhs.0))
    }

    fn double(&self) -> Self {
        Self(self.0.double())
    }

    fn mul(&self, rhs: &Self) -> Self {
        Self(self.0.mul(&rhs.0))
    }
}

/// On how to do submodules: https://pyo3.rs/v0.18.3/module#python-submodules
/// Binding that comes with the bolierplate.
/// The #[pymodule] procedural macro takes care of exporting the initialization function of your module to Python.
#[pymodule]
fn darkfi_sdk_py(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<PallasBaseWrapper>()?;
    Ok(())
}
