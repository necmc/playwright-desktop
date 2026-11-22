use pyo3::prelude::*;

#[cfg(windows)]
mod backend;
mod bindings_py;
mod engine;
mod error;
#[cfg(windows)]
mod jab;
#[cfg(windows)]
mod uia;
#[cfg(windows)]
mod windows;

#[pymodule]
fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    bindings_py::init_module(module)
}
