use super::{
    RequestState,
    platform::{
        CandidateSearch, CapturePreparation, ControlAction, ControlQuery, ControlResult,
        Platform, ProviderSelection, WindowQuery,
    },
    providers::ProviderRegistry,
};
use crate::{config::ProviderId, error::Result, windows};
use std::{
    collections::HashSet,
    sync::{Arc, mpsc::Receiver},
};

#[derive(Default)]
pub(super) struct WindowsPlatform {
    providers: ProviderRegistry,
}

impl Platform for WindowsPlatform {
    type Window = windows::Window;
    type WindowKey = (isize, u32);
    type Provider = ProviderId;

    fn snapshot_windows(&self) -> Result<HashSet<Self::WindowKey>> {
        Ok(windows::list_for_query(true)?
            .into_iter()
            .map(|window| self.window_key(&window))
            .collect())
    }

    fn candidates(&self, query: &WindowQuery) -> Result<Vec<Self::Window>> {
        Ok(windows::list_for_query(query.allows_untitled())?
            .into_iter()
            .filter(|window| query.matches(&window.title, window.pid))
            .collect())
    }

    fn find_candidates(
        &mut self,
        request: Arc<RequestState>,
        query: &WindowQuery,
        exclude: &HashSet<Self::WindowKey>,
    ) -> Result<CandidateSearch<Self::Window>> {
        let candidates: Vec<Self::Window> = self
            .candidates(query)?
            .into_iter()
            .filter(|window| !exclude.contains(&self.window_key(window)))
            .collect();
        if query.has_text.is_empty() || candidates.is_empty() {
            return Ok(CandidateSearch::Ready(candidates));
        }
        self.providers.find_with_text(request, candidates, &query.has_text)
    }

    fn window_key(&self, window: &Self::Window) -> Self::WindowKey {
        (window.hwnd, window.pid)
    }

    fn window_metadata(&self, window: &Self::Window) -> (u32, String) {
        (window.pid, window.title.clone())
    }

    fn is_window_alive(&self, window: &Self::Window) -> bool {
        windows::valid(window)
    }

    fn bring_to_front(&self, window: &Self::Window) -> Result<()> {
        windows::bring_to_front(window)
    }

    fn post_close(&self, window: &Self::Window) -> Result<bool> {
        windows::post_close(window)
    }

    fn prepare_capture(&mut self, request: Arc<RequestState>) -> Result<Option<CapturePreparation>> {
        self.providers.prepare_capture(request)
    }

    fn resolve_provider(
        &mut self,
        request: Arc<RequestState>,
        window: Self::Window,
    ) -> ProviderSelection<Self::Provider> {
        self.providers.resolve_provider(request, window)
    }

    fn act(
        &mut self,
        provider: Self::Provider,
        request: Arc<RequestState>,
        id: u64,
        window: Self::Window,
        action: ControlAction,
        query: ControlQuery,
    ) -> Result<Receiver<Result<Option<ControlResult>>>> {
        self.providers.act(provider, request, id, window, action, query)
    }

    fn release(&self, id: u64, provider: Self::Provider) {
        self.providers.release(id, provider);
    }

    fn retain_sessions(&self, sessions: HashSet<u64>) {
        self.providers.retain_sessions(sessions);
    }

    fn check_health(&self) {
        self.providers.check_health();
    }
}
