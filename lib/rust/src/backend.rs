//! Windows desktop access for the Java Access Bridge backend.

use crate::{
    engine::{ControlAction, ControlResult, RequestState, SessionOptions},
    error::{Failure, Result},
    jab::{Object, loader::{Library, dll_path}},
    windows::{self, WindowState},
};
use std::{collections::HashSet, path::PathBuf, rc::Rc};

struct LoadedBridge {
    path: PathBuf,
    library: Rc<Library>,
}

#[derive(Default)]
pub(crate) struct Backend {
    bridge: Option<LoadedBridge>,
    validated_sessions: HashSet<u64>,
}

pub(crate) struct Target {
    window: windows::Window,
    root: Object,
}

impl Backend {
    pub(crate) fn validate_loaded_bridge(&self, options: &SessionOptions) -> Result<()> {
        if let (Some(bridge), Some(home)) = (&self.bridge, options.java_home.as_deref()) {
            if dll_path(home).is_ok_and(|path| path != bridge.path) {
                return Err(different_bridge());
            }
        }
        Ok(())
    }

    pub(crate) fn prepare_session(
        &mut self, session_id: u64, options: &SessionOptions, check: &dyn Fn() -> Result<()>,
    ) -> Result<()> {
        if self.validated_sessions.contains(&session_id) {
            return Ok(());
        }
        check()?;
        let Some(java_home) = options.java_home.as_deref() else {
            return if self.bridge.is_some() {
                Ok(())
            } else {
                Err(Failure::new("BRIDGE", "Set JAVA_HOME or configure the Java runtime for Java Access Bridge."))
            };
        };
        let path = dll_path(java_home)?;
        check()?;
        if let Some(bridge) = &self.bridge {
            if bridge.path != path {
                return Err(different_bridge());
            }
        } else {
            self.bridge = Some(LoadedBridge { library: Library::load(&path)?, path });
            self.poll();
        }
        self.validated_sessions.insert(session_id);
        Ok(())
    }

    pub(crate) fn poll(&self) {
        windows::pump();
    }

    pub(crate) fn retain_sessions(&mut self, live: &HashSet<u64>) {
        self.validated_sessions.retain(|session| live.contains(session));
    }

    pub(crate) fn attach(&self, window: windows::Window) -> Result<Option<Target>> {
        let bridge = self.bridge.as_ref()
            .ok_or_else(|| Failure::new("BRIDGE", "The Java bridge is not initialized."))?;
        Ok(Object::root(&bridge.library, &window).map(|root| Target { window, root }))
    }

    pub(crate) fn recognizes(&self, window: &windows::Window) -> bool {
        self.bridge.as_ref().is_some_and(|bridge| {
            // SAFETY: the bridge is alive on its owner thread.
            unsafe { (bridge.library.is_java)(windows::hwnd(window.hwnd)) != 0 }
        })
    }

    #[cfg(test)]
    pub(crate) fn with_test_library(library: Rc<Library>, sessions: impl IntoIterator<Item = u64>) -> Self {
        Self {
            bridge: Some(LoadedBridge { path: PathBuf::new(), library }),
            validated_sessions: sessions.into_iter().collect(),
        }
    }
}

impl Target {
    pub(crate) fn state(&self) -> WindowState {
        if !windows::valid(&self.window) {
            return WindowState::Closed;
        }
        match self.root.refresh(&self.window) {
            Some(current) if self.root.same(&current) => WindowState::Ready,
            Some(_) => WindowState::Replaced(Failure::new("STALE", "Windows reused the handle for a different Java window.")),
            None => WindowState::Inaccessible(Failure::new("STALE", "The Java window is no longer accessible.")),
        }
    }

    pub(crate) fn perform(
        &self, action: ControlAction, role: &str, name: &str, request: &RequestState,
    ) -> Result<Option<ControlResult>> {
        if !matches!(role, "textbox" | "button") || name.is_empty() {
            return Err(Failure::new("CONFIGURATION", "A supported role and exact accessible name are required."));
        }
        let Some(control) = self.root.find(role, name, &|| request.check())? else { return Ok(None); };
        request.check()?;
        let required_states: &[&str] = match &action {
            ControlAction::WaitVisible => &["showing"],
            ControlAction::Fill(_) => &["showing", "enabled", "editable"],
            _ => &["showing", "enabled"],
        };
        if !control.has_states(required_states)? {
            return Ok(None);
        }
        request.check()?;
        let text = match (action, role) {
            (ControlAction::WaitVisible, _) => None,
            (ControlAction::ReadText, "textbox") => Some(control.text()?),
            (ControlAction::Fill(text), "textbox") => { control.fill(&text, request)?; None }
            (ControlAction::Click, "button") => { control.click(request)?; None }
            _ => return Err(Failure::new("CONFIGURATION", "The action does not support this role.")),
        };
        Ok(Some(ControlResult { text }))
    }
}

fn different_bridge() -> Failure {
    Failure::new("CONFIGURATION", "This process already loaded a different Java bridge. Start another Python process to change it.")
}

#[cfg(test)]
mod tests;
