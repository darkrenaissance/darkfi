mod affine;
mod base;
mod point;
mod proof;
mod proving_key;
mod scalar;
mod verifying_key;
mod zk_binary;
mod zk_circuit;

#[pyo3::prelude::pymodule]
fn darkfi_sdk_py(py: pyo3::Python<'_>, m: &pyo3::types::PyModule) -> pyo3::PyResult<()> {
    let submodule = affine::create_module(py)?;
    pyo3::py_run!(py, submodule, "import sys; sys.modules['darkfi_sdk_py.affine'] = submodule");
    m.add_submodule(submodule)?;

    let submodule = base::create_module(py)?;
    pyo3::py_run!(py, submodule, "import sys; sys.modules['darkfi_sdk_py.base'] = submodule");
    m.add_submodule(submodule)?;

    let submodule = scalar::create_module(py)?;
    pyo3::py_run!(py, submodule, "import sys; sys.modules['darkfi_sdk_py.scalar'] = submodule");
    m.add_submodule(scalar::create_module(py)?)?;

    let submodule = point::create_module(py)?;
    pyo3::py_run!(py, submodule, "import sys; sys.modules['darkfi_sdk_py.point'] = submodule");
    m.add_submodule(point::create_module(py)?)?;

    let submodule = proof::create_module(py)?;
    pyo3::py_run!(py, submodule, "import sys; sys.modules['darkfi_sdk_py.proof'] = submodule");
    m.add_submodule(proof::create_module(py)?)?;

    let submodule = proving_key::create_module(py)?;
    pyo3::py_run!(
        py,
        submodule,
        "import sys; sys.modules['darkfi_sdk_py.proving_key'] = submodule"
    );
    m.add_submodule(proving_key::create_module(py)?)?;

    let submodule = verifying_key::create_module(py)?;
    pyo3::py_run!(
        py,
        submodule,
        "import sys; sys.modules['darkfi_sdk_py.verifying_key'] = submodule"
    );
    m.add_submodule(verifying_key::create_module(py)?)?;

    let submodule = zk_binary::create_module(py)?;
    pyo3::py_run!(py, submodule, "import sys; sys.modules['darkfi_sdk_py.zk_binary'] = submodule");
    m.add_submodule(zk_binary::create_module(py)?)?;

    let submodule = zk_circuit::create_module(py)?;
    pyo3::py_run!(py, submodule, "import sys; sys.modules['darkfi_sdk_py.zk_circuit'] = submodule");
    m.add_submodule(zk_circuit::create_module(py)?)?;

    Ok(())
}
