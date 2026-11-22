#[cfg(windows)]
mod backend;
mod config;
mod engine;
mod error;
#[cfg(windows)]
mod jab;
#[cfg(windows)]
mod uia;
#[cfg(windows)]
mod windows;

pub use config::{
    BOOL_TRUE, JAVA_EXPLICIT_OPTION, JAVA_HOME_ERROR, JAVA_HOME_OPTION, JAVA_PROVIDER_ID,
    ProviderId, SelectionMode, SessionConfig, UIA_PROVIDER_ID, validate_absolute_path,
};
pub use engine::{
    ControlQuery, CoreEngine, CoreReply, CoreRequest, InvokeParams, WindowQuery, host_os_language,
};
pub use error::{Failure, Result, codes};
