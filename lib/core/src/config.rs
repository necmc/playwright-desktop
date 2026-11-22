use crate::error::{Failure, Result, codes};
use std::{collections::HashMap, path::Path};

pub type ProviderId = &'static str;

pub const JAVA_PROVIDER_ID: ProviderId = "jab";
pub const UIA_PROVIDER_ID: ProviderId = "uia";
pub const JAVA_HOME_OPTION: &str = "java.home";
pub const JAVA_EXPLICIT_OPTION: &str = "java.explicit";
pub const JAVA_HOME_ERROR: &str = "java_home must be an absolute runtime directory.";
pub const BOOL_TRUE: &str = "true";

pub mod roles {
    pub const TEXT: &str = "text";
    pub const TEXTBOX: &str = "textbox";
    pub const BUTTON: &str = "button";
    pub const LINK: &str = "link";
    pub const MENUITEM: &str = "menuitem";
    pub const DIALOG: &str = "dialog";
    pub const GROUP: &str = "group";
    pub const PANE: &str = "pane";
    pub const MENU: &str = "menu";
    pub const LISTITEM: &str = "listitem";
}

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
        if key.is_empty() || value.is_empty() || key.contains('\0') || value.contains('\0') {
            return Err(Failure::new(
                codes::CONFIGURATION,
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
        self.option(key) == Some(BOOL_TRUE)
    }
}

pub fn validate_absolute_path(value: &str, message: &str) -> Result<()> {
    if value.is_empty() || value.contains('\0') || !Path::new(value).is_absolute() {
        return Err(Failure::new(codes::CONFIGURATION, message));
    }
    Ok(())
}
