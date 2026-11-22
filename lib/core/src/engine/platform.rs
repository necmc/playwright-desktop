use super::{QueuedCommand, RequestState, Shared, coordinator::Worker};
use crate::error::{Failure, Result, codes};
use std::{
    collections::{HashSet, VecDeque},
    hash::Hash,
    sync::{Arc, mpsc::Receiver},
    time::Instant,
};

#[cfg(windows)]
const WORKER_THREAD_NAME: &str = "playwright-desktop";
#[cfg(not(windows))]
const UNSUPPORTED_PLATFORM_MESSAGE: &str = "This system has no desktop accessibility adapter.";
const INVALID_QUERY_MESSAGE: &str =
    "Use an exact title, a title prefix, a PID or visible text. Do not combine the two title filters.";
pub(super) const WINDOW_NOT_FOUND_MESSAGE: &str = "The native window no longer exists.";

pub(crate) enum ControlAction {
    WaitVisible,
    ReadText,
    Fill(String),
    Click,
}

#[derive(Clone, Default)]
pub struct ControlQuery {
    pub role: String,
    pub name: String,
    pub scope_role: String,
    pub scope_name: String,
}

pub(crate) struct ControlResult {
    pub(crate) text: Option<String>,
}

#[derive(Clone, Default)]
pub struct WindowQuery {
    pub title: String,
    pub pid: Option<u32>,
    pub title_prefix: String,
    pub has_text: String,
    pub only_new: bool,
}

impl WindowQuery {
    pub(super) fn validate_capture(&self) -> Result<()> {
        if (!self.title.is_empty() && !self.title_prefix.is_empty())
            || self.title.contains('\0') || self.title_prefix.contains('\0') || self.has_text.contains('\0')
        {
            return Err(Failure::new(codes::CONFIGURATION, INVALID_QUERY_MESSAGE));
        }
        Ok(())
    }

    pub(super) fn validate(&self) -> Result<()> {
        self.validate_capture()?;
        if self.title.is_empty() && self.title_prefix.is_empty() && self.pid.is_none() && self.has_text.is_empty() {
            return Err(Failure::new(codes::CONFIGURATION, INVALID_QUERY_MESSAGE));
        }
        Ok(())
    }

    pub(super) fn allows_untitled(&self) -> bool {
        !self.has_text.is_empty() && self.title.is_empty() && self.title_prefix.is_empty()
    }

    pub(super) fn matches(&self, title: &str, pid: u32) -> bool {
        self.pid.is_none_or(|expected| expected == pid)
            && (self.title.is_empty() || self.title == title)
            && (self.title_prefix.is_empty() || title.starts_with(self.title_prefix.as_str()))
    }
}

pub(super) enum CandidateSearch<W> {
    Ready(Vec<W>),
    Pending(Receiver<Result<Vec<W>>>),
}

pub(super) struct CapturePreparation {
    pub(super) result: Receiver<Result<()>>,
    pub(super) until: Option<Instant>,
}

pub(super) enum ProviderSelection<P> {
    Ready(P),
    Pending {
        probes: VecDeque<(P, Receiver<Result<bool>>)>,
        until: Instant,
        fallback: P,
    },
}

pub(super) trait Platform {
    type Window: Clone;
    type WindowKey: Clone + Eq + Hash;
    type Provider: Copy;

    fn snapshot_windows(&self) -> Result<HashSet<Self::WindowKey>>;
    fn candidates(&self, query: &WindowQuery) -> Result<Vec<Self::Window>>;
    fn find_candidates(
        &mut self,
        _request: Arc<RequestState>,
        query: &WindowQuery,
        exclude: &HashSet<Self::WindowKey>,
    ) -> Result<CandidateSearch<Self::Window>> {
        Ok(CandidateSearch::Ready(
            self.candidates(query)?
                .into_iter()
                .filter(|window| !exclude.contains(&self.window_key(window)))
                .collect(),
        ))
    }
    fn window_key(&self, window: &Self::Window) -> Self::WindowKey;
    fn window_metadata(&self, window: &Self::Window) -> (u32, String);
    fn is_window_alive(&self, window: &Self::Window) -> bool;
    fn bring_to_front(&self, window: &Self::Window) -> Result<()>;
    fn post_close(&self, window: &Self::Window) -> Result<bool>;
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
pub fn host_os_language() -> Result<String> {
    Ok(crate::windows::host_os_language())
}

#[cfg(not(windows))]
pub fn host_os_language() -> Result<String> {
    Err(Failure::new(codes::UNSUPPORTED, UNSUPPORTED_PLATFORM_MESSAGE))
}

#[cfg(windows)]
pub(super) fn start_host_worker(shared: Arc<Shared>, receiver: Receiver<QueuedCommand>) -> Result<()> {
    start_worker(super::windows_platform::WindowsPlatform::default, shared, receiver)
}

#[cfg(not(windows))]
pub(super) fn start_host_worker(_: Arc<Shared>, _: Receiver<QueuedCommand>) -> Result<()> {
    Err(Failure::new(codes::UNSUPPORTED, UNSUPPORTED_PLATFORM_MESSAGE))
}

#[cfg(windows)]
fn start_worker<P: Platform + 'static>(
    create_platform: fn() -> P, shared: Arc<Shared>, receiver: Receiver<QueuedCommand>,
) -> Result<()> {
    std::thread::Builder::new().name(WORKER_THREAD_NAME.into())
        .spawn(move || {
            let state = shared.clone();
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                Worker::new(shared, create_platform()).run(receiver);
            })).is_err() {
                state.poisoned.store(true, std::sync::atomic::Ordering::Release);
            }
        })
        .map_err(|error| Failure::new(codes::BRIDGE, format!("Cannot start the native worker: {error}")))?;
    Ok(())
}
