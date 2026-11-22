use playwright_desktop_core::{
    BOOL_TRUE, ControlQuery, CoreEngine, CoreReply, CoreRequest, Failure, InvokeParams,
    JAVA_EXPLICIT_OPTION, JAVA_HOME_ERROR, JAVA_HOME_OPTION, JAVA_PROVIDER_ID,
    Result as CoreResult, SessionConfig, WindowQuery, host_os_language, validate_absolute_path,
};
use pyo3::prelude::*;

pyo3::create_exception!(_native, NativeError, pyo3::exceptions::PyException);

fn as_python(error: Failure) -> PyErr {
    NativeError::new_err((error.code, error.message))
}

fn set_java_home(config: &mut SessionConfig, java_home: &str) -> CoreResult<()> {
    validate_absolute_path(java_home, JAVA_HOME_ERROR)?;
    config.set_option(JAVA_HOME_OPTION, java_home)
}

#[pyfunction]
fn binding_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[pyfunction]
fn os_language() -> PyResult<String> {
    host_os_language().map_err(as_python)
}

#[derive(FromPyObject)]
struct PyInvokeParams {
    operation: String,
    window_id: u64,
    capture_id: u64,
    title: String,
    pid: Option<u32>,
    role: String,
    name: String,
    text: String,
    scope_role: String,
    scope_name: String,
    title_prefix: String,
    has_text: String,
    only_new: bool,
}

impl From<PyInvokeParams> for InvokeParams {
    fn from(params: PyInvokeParams) -> Self {
        Self {
            operation: params.operation,
            window_id: params.window_id,
            capture_id: params.capture_id,
            window: WindowQuery {
                title: params.title,
                pid: params.pid,
                title_prefix: params.title_prefix,
                has_text: params.has_text,
                only_new: params.only_new,
            },
            text: params.text,
            control: ControlQuery {
                role: params.role,
                name: params.name,
                scope_role: params.scope_role,
                scope_name: params.scope_name,
            },
        }
    }
}

#[pyclass(name = "Reply")]
#[derive(Debug, Default)]
pub struct PyReply {
    #[pyo3(get)]
    pub handle: u64,
    #[pyo3(get)]
    pub text: Option<String>,
    #[pyo3(get)]
    pub pid: Option<u32>,
    #[pyo3(get)]
    pub title: Option<String>,
}

impl From<CoreReply> for PyReply {
    fn from(reply: CoreReply) -> Self {
        Self {
            handle: reply.handle,
            text: reply.text,
            pid: reply.pid,
            title: reply.title,
        }
    }
}

#[pyclass(name = "Request")]
#[derive(Clone)]
pub struct PyRequest(CoreRequest);

#[pymethods]
impl PyRequest {
    fn cancel(&self) {
        self.0.cancel();
    }
}

#[pyclass(name = "Engine")]
pub struct PyEngine(CoreEngine);

#[pymethods]
impl PyEngine {
    #[new]
    fn new() -> PyResult<Self> {
        CoreEngine::new().map(Self).map_err(as_python)
    }

    fn open_session(&self, java_home: &str) -> PyResult<u64> {
        let mut config = SessionConfig::explicit_provider(JAVA_PROVIDER_ID);
        set_java_home(&mut config, java_home).map_err(as_python)?;
        config.set_option(JAVA_EXPLICIT_OPTION, BOOL_TRUE).map_err(as_python)?;
        self.0.open_session(config).map_err(as_python)
    }

    #[pyo3(signature = (java_home=None, *, explicit=false))]
    fn open_auto_session(&self, java_home: Option<&str>, explicit: bool) -> PyResult<u64> {
        let mut config = SessionConfig::automatic();
        if let Some(home) = java_home {
            set_java_home(&mut config, home).map_err(as_python)?;
            if explicit {
                config.set_option(JAVA_EXPLICIT_OPTION, BOOL_TRUE).map_err(as_python)?;
            }
        }
        self.0.open_session(config).map_err(as_python)
    }

    fn new_request(&self, session_id: u64, timeout_ms: u64) -> PyResult<PyRequest> {
        self.0.new_request(session_id, timeout_ms).map(PyRequest).map_err(as_python)
    }

    fn invoke(&self, py: Python<'_>, request: &PyRequest, params: PyInvokeParams) -> PyResult<PyReply> {
        let core = self.0.clone();
        let req = request.0.clone();
        let core_params = InvokeParams::from(params);
        py.detach(move || core.invoke(&req, core_params)).map(PyReply::from).map_err(as_python)
    }

    fn close_session(&self, session_id: u64) {
        self.0.close_session(session_id);
    }

    fn cancel_capture(&self, session_id: u64, capture_id: u64) {
        self.0.cancel_capture(session_id, capture_id);
    }
}

#[pymodule]
fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(binding_version, module)?)?;
    module.add_function(wrap_pyfunction!(os_language, module)?)?;
    module.add("NativeError", module.py().get_type::<NativeError>())?;
    module.add_class::<PyEngine>()?;
    module.add_class::<PyRequest>()?;
    module.add_class::<PyReply>()?;
    Ok(())
}
