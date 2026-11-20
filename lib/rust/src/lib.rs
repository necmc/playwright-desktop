use pyo3::prelude::*;

#[cfg(windows)]
mod backend;
mod engine;
mod error;
#[cfg(windows)]
mod jab;
#[cfg(windows)]
mod uia;
#[cfg(windows)]
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
