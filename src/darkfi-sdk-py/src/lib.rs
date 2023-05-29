use darkfi_sdk::crypto::{
    pallas,
    pasta_prelude::{Field, PrimeField},
    poseidon_hash as poseidon_hash_,
};
use pyo3::prelude::*;
use rand::rngs::OsRng;

/// QUESTION: How do export a class that is defined in our dependencies?
// #[pyclass]
// struct BaseWrappper {
//     b: Base
// }

/// This version deals in lists to avoid the problem converting
/// Rust structs to Python classes.
// #[pyfunction]
// fn poseidon_hash_list(messages: Vec<Vec<u8>>) {
//     println!("messages: {:?}", messages);
//     let mut bases = vec![];
//     for m in messages.iter() {
//         let m: [u8; 32] = m[0..32].try_into().expect("[DARKFI_SDK_PY] slice with incorrect length");
//         let base = pallas::Base::from_repr(m).unwrap();
//         bases.push(base);
//     }
//     println!("{:?}", bases);

//     // QUESTION: How to convert bases into [pallas::Base; N]? Is this even possible?
//     println!("{:?}", poseidon_hash_(bases));
// }

#[derive(Clone)]
#[pyclass]
struct Foo {
    x: u64,
}

/// The Foo struct
#[pymethods]
impl Foo {
    #[new]
    fn new(x: u64) -> Self {
        Foo { x }
    }
    // don't know why self: &Self doesn't work
    // why can Self be a return type but not &Self?
    fn bar(&self) -> Self {
        self.clone()
    }
}

/// Binding that comes with the bolierplate.
#[pyfunction]
fn sum_as_string(a: usize, b: usize) -> PyResult<String> {
    Ok((a + b).to_string())
}
/// On how to do submodules: https://pyo3.rs/v0.18.3/module#python-submodules
/// Binding that comes with the bolierplate.
/// The #[pymodule] procedural macro takes care of exporting the initialization function of your module to Python.
#[pymodule]
fn darkfi_sdk_py(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(sum_as_string, m)?)?;
    m.add_class::<Foo>()?;
    m.add_class::<PallasBaseWrapper>()?;
    Ok(())
}

// ======================================= Actual implementation =========================================

// new type pattern
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
// // associated types: https://doc.rust-lang.org/book/ch19-03-advanced-traits.html#using-the-newtype-pattern-to-implement-external-traits-on-external-types
// // to get every method inner type has
// impl Deref for PallasBaseWrapper {
//     type Target = pallas::Base;
//     fn deref(&self) -> &pallas::Base { &self.0 }
// }


