use crate::config::{SessionConfig, validate_absolute_path};
use crate::engine::{ControlQuery, CoreEngine, CoreReply, CoreRequest, InvokeParams, WindowQuery, host_os_language};
use crate::error::Failure;
use pyo3::prelude::*;

const JAVA_PROVIDER_ID: &str = "jab";
const JAVA_HOME_OPTION: &str = "java.home";
const JAVA_EXPLICIT_OPTION: &str = "java.explicit";
const JAVA_HOME_ERROR: &str = "java_home must be an absolute runtime directory.";

pyo3::create_exception!(_native, NativeError, pyo3::exceptions::PyException);

fn as_python(error: Failure) -> PyErr {
    NativeError::new_err((error.code, error.message))
}

#[pyfunction]
fn binding_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[pyfunction]
fn os_language() -> PyResult<String> {
    host_os_language().map_err(as_python)
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
        let config = build_java_config(Some(java_home), true, true).map_err(as_python)?;
        self.0.open_session(config).map_err(as_python)
    }

    #[pyo3(signature = (java_home=None, *, explicit=false))]
    fn open_auto_session(&self, java_home: Option<&str>, explicit: bool) -> PyResult<u64> {
        let config = build_java_config(java_home, explicit, false).map_err(as_python)?;
        self.0.open_session(config).map_err(as_python)
    }

    fn new_request(&self, session_id: u64, timeout_ms: u64) -> PyResult<PyRequest> {
        self.0.new_request(session_id, timeout_ms).map(PyRequest).map_err(as_python)
    }

    #[pyo3(signature = (request, operation, *, window_id=0, capture_id=0, title="", pid=None, role="", name="", text="", scope_role="", scope_name="", title_prefix="", has_text="", only_new=false))]
    #[allow(clippy::too_many_arguments)]
    fn invoke(
        &self,
        py: Python<'_>,
        request: &PyRequest,
        operation: &str,
        window_id: u64,
        capture_id: u64,
        title: &str,
        pid: Option<u32>,
        role: &str,
        name: &str,
        text: &str,
        scope_role: &str,
        scope_name: &str,
        title_prefix: &str,
        has_text: &str,
        only_new: bool,
    ) -> PyResult<PyReply> {
        let params = InvokeParams {
            operation: operation.into(),
            window_id,
            capture_id,
            window: WindowQuery {
                title: title.into(),
                pid,
                title_prefix: title_prefix.into(),
                has_text: has_text.into(),
                only_new,
            },
            text: text.into(),
            control: ControlQuery {
                role: role.into(),
                name: name.into(),
                scope_role: scope_role.into(),
                scope_name: scope_name.into(),
            },
        };
        let core = self.0.clone();
        let req = request.0.clone();
        py.detach(move || core.invoke(&req, params)).map(PyReply::from).map_err(as_python)
    }

    fn close_session(&self, session_id: u64) {
        self.0.close_session(session_id);
    }

    fn cancel_capture(&self, session_id: u64, capture_id: u64) {
        self.0.cancel_capture(session_id, capture_id);
    }
}

fn build_java_config(
    java_home: Option<&str>,
    explicit: bool,
    fixed_provider: bool,
) -> crate::error::Result<SessionConfig> {
    let mut config = if fixed_provider {
        SessionConfig::explicit_provider(JAVA_PROVIDER_ID)
    } else {
        SessionConfig::automatic()
    };
    if let Some(home) = java_home {
        validate_absolute_path(home, JAVA_HOME_ERROR)?;
        config.set_option(JAVA_HOME_OPTION, home)?;
        if explicit {
            config.set_option(JAVA_EXPLICIT_OPTION, "true")?;
        }
    }
    Ok(config)
}

pub fn init_module(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(binding_version, module)?)?;
    module.add_function(wrap_pyfunction!(os_language, module)?)?;
    module.add("NativeError", module.py().get_type::<NativeError>())?;
    module.add_class::<PyEngine>()?;
    module.add_class::<PyRequest>()?;
    module.add_class::<PyReply>()?;
    Ok(())
}
