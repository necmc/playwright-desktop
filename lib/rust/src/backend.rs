//! Windows desktop access for the Java Access Bridge backend.

use crate::{error::{Failure, Result}, jab::{Object, loader::{Library, dll_path}}, windows};
use std::{collections::HashSet, path::{Path, PathBuf}, rc::Rc};

pub(crate) struct SessionOptions {
    java_home: Option<String>,
    pub(crate) automatic: bool,
    pub(crate) explicit_java_home: bool,
}

impl SessionOptions {
    pub(crate) fn java(java_home: &str) -> Result<Self> {
        if java_home.is_empty() || java_home.contains('\0') || !Path::new(java_home).is_absolute() {
            return Err(Failure::new("CONFIGURATION", "java_home must be an absolute runtime directory."));
        }
        Ok(Self { java_home: Some(java_home.into()), automatic: false, explicit_java_home: true })
    }

    pub(crate) fn automatic(java_home: Option<&str>, explicit: bool) -> Result<Self> {
        let mut options = match java_home {
            Some(java_home) => Self::java(java_home)?,
            None => Self { java_home: None, automatic: true, explicit_java_home: false },
        };
        options.automatic = true;
        options.explicit_java_home = explicit && java_home.is_some();
        Ok(options)
    }
}

struct LoadedBridge {
    path: PathBuf,
    library: Rc<Library>,
}

#[derive(Default)]
pub(crate) struct Backend {
    bridge: Option<LoadedBridge>,
    validated_sessions: HashSet<u64>,
}

pub(crate) const TIMEOUT_MESSAGE: &str =
    "No unique desktop window or control became ready before the deadline. Check its title, PID and accessibility provider.";

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub(crate) struct WindowKey(isize, u32);

pub(crate) struct WindowCandidate(pub(crate) windows::Window);

impl WindowCandidate {
    pub(crate) fn key(&self) -> WindowKey {
        WindowKey(self.0.hwnd, self.0.pid)
    }
}

pub(crate) struct Target {
    window: windows::Window,
    root: Object,
}

pub(crate) enum WindowState {
    Ready,
    Closed,
    Inaccessible(Failure),
    Replaced(Failure),
}

pub(crate) enum ControlAction<'a> {
    ReadText,
    Fill(&'a str),
    Click,
}

pub(crate) struct ControlResult {
    pub(crate) text: Option<String>,
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

    pub(crate) fn candidates(title: &str, pid: Option<u32>) -> Result<Vec<WindowCandidate>> {
        if title.is_empty() || title.contains('\0') {
            return Err(Failure::new("CONFIGURATION", "An exact, nonempty window title is required."));
        }
        Ok(windows::list()?.into_iter().filter(|window| matches_window(window, title, pid))
            .map(WindowCandidate).collect())
    }

    pub(crate) fn attach(&self, candidate: WindowCandidate) -> Result<Option<Target>> {
        let bridge = self.bridge.as_ref()
            .ok_or_else(|| Failure::new("BRIDGE", "The Java bridge is not initialized."))?;
        let window = candidate.0;
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

    #[cfg(test)]
    pub(crate) fn attach_test_window(&self, window: windows::Window) -> Result<Option<Target>> {
        self.attach(WindowCandidate(window))
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
        &self, action: ControlAction<'_>, role: &str, name: &str, check: &dyn Fn() -> Result<()>,
    ) -> Result<Option<ControlResult>> {
        if !matches!(role, "textbox" | "button") || name.is_empty() {
            return Err(Failure::new("CONFIGURATION", "A supported role and exact accessible name are required."));
        }
        let Some(control) = self.root.find(role, name, check)? else { return Ok(None); };
        check()?;
        if !control.ready(matches!(action, ControlAction::Fill(_)))? {
            return Ok(None);
        }
        check()?;
        let text = match (action, role) {
            (ControlAction::ReadText, "textbox") => Some(control.text()?),
            (ControlAction::Fill(text), "textbox") => { control.fill(text, check)?; None }
            (ControlAction::Click, "button") => { control.click(check)?; None }
            _ => return Err(Failure::new("CONFIGURATION", "The action does not support this role.")),
        };
        Ok(Some(ControlResult { text }))
    }
}

fn matches_window(window: &windows::Window, title: &str, pid: Option<u32>) -> bool {
    window.title == title && pid.is_none_or(|pid| window.pid == pid)
}

fn different_bridge() -> Failure {
    Failure::new("CONFIGURATION", "This process already loaded a different Java bridge. Start another Python process to change it.")
}
