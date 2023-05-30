use darkfi_sdk::crypto::{
    pallas,
    pasta_prelude::{Field, PrimeField},
};
use pyo3::prelude::*;
use rand::rngs::OsRng;

/// This represents an element of $\mathbb{F}_p$ where
///
/// `p = 0x40000000000000000000000000000000224698fc094cf91b992d30ed00000001`
///
/// is the base field of the Pallas curve.
// The internal representation of this type is four 64-bit unsigned
// integers in little-endian order. `Fp` values are always in
// Montgomery form; i.e., Fp(a) = aR mod p, with R = 2^256.
#[pyclass]
struct PallasBaseWrapper(pallas::Base);

#[pymethods]
impl PallasBaseWrapper {
    /// For now, we work with 128 bits in Python.
    /// Becasue pallas::Base has nice debug formatting for it.
    /// TODO: change it to from_raw
    #[new]
    fn from_u128(v: u128) -> Self {
        Self(pallas::Base::from_u128(v))
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

    fn __repr__(&self) -> String {
        format!("PallasBaseWrapper({:?})", self.0)
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

// Why Scalar field is from the field vesta curve is defined over?
/// This represents an element of $\mathbb{F}_q$ where
///
/// `q = 0x40000000000000000000000000000000224698fc0994a8dd8c46eb2100000001`
///
/// is the base field of the Vesta curve.
// The internal representation of this type is four 64-bit unsigned
// integers in little-endian order. `Fq` values are always in
// Montgomery form; i.e., Fq(a) = aR mod q, with R = 2^256.
#[pyclass]
struct PallasScalarWrapper(pallas::Scalar);

#[pymethods]
impl PallasScalarWrapper {
    /// TODO: change it to from_raw
    #[new]
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

    fn __repr__(&self) -> String {
        format!("PallasScalarWrapper({:?})", self.0)
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


/// On how to do submodules: https://pyo3.rs/v0.18.3/module#python-submodules
/// Binding that comes with the bolierplate.
/// The #[pymodule] procedural macro takes care of exporting the initialization function of your module to Python.
#[pymodule]
fn darkfi_sdk_py(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<PallasBaseWrapper>()?;
    m.add_class::<PallasScalarWrapper>()?;
    Ok(())
}
