use darkfi_sdk::crypto::{pallas, pasta_prelude::PrimeField, poseidon_hash as poseidon_hash_};
use pyo3::prelude::*;

/// QUESTION: How do export a class that is defined in our dependencies? 
// #[pyclass]
// struct BaseWrappper {
//     b: Base
// }

/// This version deals in lists to avoid the problem converting
/// Rust structs to Python classes.
#[pyfunction]
fn poseidon_hash_list(messages: Vec<Vec<u8>>) {
    println!("messages: {:?}", messages);
    let mut bases = vec![];
    for m in messages.iter() {
        let m: [u8; 32] = m[0..32].try_into().expect("[DARKFI_SDK_PY] slice with incorrect length");
        let base = pallas::Base::from_repr(m).unwrap();
        bases.push(base);
    }
    println!("{:?}", bases);

    // QUESTION: How to convert bases into [pallas::Base; N]? Is this even possible?
    println!("{:?}", poseidon_hash_(bases));
}

/// Binding that comes with the bolierplate.
#[pyfunction]
fn sum_as_string(a: usize, b: usize) -> PyResult<String> {
    Ok((a + b).to_string())
}

/// Binding that comes with the bolierplate.
#[pymodule]
fn darkfi_sdk_py(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(sum_as_string, m)?)?;
    m.add_function(wrap_pyfunction!(poseidon_hash_list, m)?)?;
    Ok(())
}
