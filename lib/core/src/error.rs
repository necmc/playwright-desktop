pub mod codes {
    pub const ACTION: &str = "ACTION";
    pub const AMBIGUOUS: &str = "AMBIGUOUS";
    pub const BRIDGE: &str = "BRIDGE";
    pub const BUSY: &str = "BUSY";
    pub const CANCELLED: &str = "CANCELLED";
    pub const CLOSED: &str = "CLOSED";
    pub const CONFIGURATION: &str = "CONFIGURATION";
    pub const INVALID_ARGUMENT: &str = "INVALID_ARGUMENT";
    pub const STALE: &str = "STALE";
    pub const TIMEOUT: &str = "TIMEOUT";
    pub const UNSUPPORTED: &str = "UNSUPPORTED";
}

#[derive(Debug, Clone)]
pub struct Failure {
    pub code: &'static str,
    pub message: String,
}

pub type Result<T> = std::result::Result<T, Failure>;

impl Failure {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }
}
