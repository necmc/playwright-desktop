#[cfg(windows)]
mod backend;
pub mod config;
pub mod engine;
pub mod error;
#[cfg(windows)]
mod jab;
#[cfg(windows)]
mod uia;
#[cfg(windows)]
mod windows;

pub use config::{ProviderId, SelectionMode, SessionConfig, validate_absolute_path};
pub use engine::{
    ControlQuery, CoreEngine, CoreReply, CoreRequest, InvokeParams, WindowQuery, host_os_language,
};
pub use error::{Failure, Result};
