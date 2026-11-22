use crate::error::{Failure, Result};
use std::{collections::HashMap, path::Path};

pub type ProviderId = &'static str;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SelectionMode {
    #[default]
    Automatic,
    ExplicitProvider(ProviderId),
}

#[derive(Clone, Debug, Default)]
pub struct SessionConfig {
    pub mode: SelectionMode,
    values: HashMap<String, String>,
}

impl SessionConfig {
    pub fn automatic() -> Self {
        Self::default()
    }

    pub fn explicit_provider(provider: ProviderId) -> Self {
        Self {
            mode: SelectionMode::ExplicitProvider(provider),
            values: HashMap::new(),
        }
    }

    pub fn set_option(&mut self, key: &str, value: &str) -> Result<()> {
        if key.is_empty() || key.contains('\0') || value.contains('\0') {
            return Err(Failure::new(
                "CONFIGURATION",
                "Configuration keys and values must be non-empty and valid strings.",
            ));
        }
        self.values.insert(key.into(), value.into());
        Ok(())
    }

    pub fn option(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }

    pub fn flag(&self, key: &str) -> bool {
        self.option(key) == Some("true")
    }
}

pub fn validate_absolute_path(value: &str, message: &str) -> Result<()> {
    if value.is_empty() || value.contains('\0') || !Path::new(value).is_absolute() {
        return Err(Failure::new("CONFIGURATION", message));
    }
    Ok(())
}
