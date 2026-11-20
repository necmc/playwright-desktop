use super::{QueuedCommand, RequestState, Shared, Worker};
use crate::error::{Failure, Result};
use std::{
    collections::HashSet,
    hash::Hash,
    sync::{Arc, mpsc::Receiver},
    time::Instant,
};

pub(crate) enum ControlAction {
    WaitVisible,
    ReadText,
    Fill(String),
    Click,
}

#[derive(Clone, Default)]
pub(super) struct ControlQuery {
    pub(super) role: String,
    pub(super) name: String,
    pub(super) scope_role: String,
    pub(super) scope_name: String,
}

pub(crate) struct ControlResult {
    pub(crate) text: Option<String>,
}

#[derive(Clone, Default)]
pub(super) struct WindowQuery {
    pub(super) title: String,
    pub(super) pid: Option<u32>,
    pub(super) title_prefix: String,
}

impl WindowQuery {
    pub(super) fn validate(&self) -> Result<()> {
        if (!self.title.is_empty() && !self.title_prefix.is_empty())
            || self.title.contains('\0') || self.title_prefix.contains('\0')
            || (self.title.is_empty() && self.title_prefix.is_empty() && self.pid.is_none())
        {
            return Err(Failure::new("CONFIGURATION", "Use an exact title, a title prefix or a PID. Do not combine the two title filters."));
        }
        Ok(())
    }

    pub(super) fn matches(&self, title: &str, pid: u32) -> bool {
        self.pid.is_none_or(|expected| expected == pid)
            && (self.title.is_empty() || self.title == title)
            && title.starts_with(self.title_prefix.as_str())
    }
}

pub(super) struct CapturePreparation {
    pub(super) result: Receiver<Result<()>>,
    pub(super) until: Option<Instant>,
}

pub(super) enum ProviderSelection<P> {
    Ready(P),
    Pending {
        result: Receiver<Result<P>>,
        until: Instant,
        fallback: P,
    },
}

pub(super) trait Platform {
    type Window: Clone;
    type WindowKey: Clone + Eq + Hash;
    type Provider: Copy;

    fn candidates(&self, query: &WindowQuery) -> Result<Vec<Self::Window>>;
    fn window_key(&self, window: &Self::Window) -> Self::WindowKey;
    fn is_window_alive(&self, window: &Self::Window) -> bool;
    fn prepare_capture(&mut self, request: Arc<RequestState>) -> Result<Option<CapturePreparation>>;
    fn resolve_provider(
        &mut self, request: Arc<RequestState>, window: Self::Window,
    ) -> ProviderSelection<Self::Provider>;
    fn act(
        &mut self, provider: Self::Provider, request: Arc<RequestState>, id: u64,
        window: Self::Window, action: ControlAction, query: ControlQuery,
    ) -> Result<Receiver<Result<Option<ControlResult>>>>;
    fn release(&self, id: u64, provider: Self::Provider);
    fn retain_sessions(&self, sessions: HashSet<u64>);
    fn check_health(&self);
}

#[cfg(windows)]
pub(super) fn start_host_worker(shared: Arc<Shared>, receiver: Receiver<QueuedCommand>) -> Result<()> {
    start_worker(super::windows_platform::WindowsPlatform::default, shared, receiver)
}

#[cfg(not(windows))]
pub(super) fn start_host_worker(_: Arc<Shared>, _: Receiver<QueuedCommand>) -> Result<()> {
    Err(Failure::new("UNSUPPORTED", "This system has no desktop accessibility adapter."))
}

#[cfg(windows)]
fn start_worker<P: Platform + 'static>(
    create_platform: fn() -> P, shared: Arc<Shared>, receiver: Receiver<QueuedCommand>,
) -> Result<()> {
    std::thread::Builder::new().name("playwright-desktop".into())
        .spawn(move || {
            let state = shared.clone();
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                Worker::new(shared, create_platform()).run(receiver);
            })).is_err() {
                state.poisoned.store(true, std::sync::atomic::Ordering::Release);
            }
        })
        .map_err(|error| Failure::new("BRIDGE", format!("Cannot start the native worker: {error}")))?;
    Ok(())
}
