use pyo3::prelude::*;

mod backend;
mod engine;
mod error;
mod jab;
mod uia;
mod windows;

pyo3::create_exception!(_native, NativeError, pyo3::exceptions::PyException);

#[pyfunction]
fn binding_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[pymodule]
fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(binding_version, module)?)?;
    module.add("NativeError", module.py().get_type::<NativeError>())?;
    module.add_class::<engine::Engine>()?;
    module.add_class::<engine::Request>()?;
    module.add_class::<engine::Reply>()?;
    Ok(())
}
