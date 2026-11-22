mod jab;
mod lane;
mod uia;

use super::{
    RequestState,
    platform::{
        CandidateSearch, CapturePreparation, ControlAction, ControlQuery, ControlResult,
        ProviderSelection,
    },
};
use crate::{
    config::{ProviderId, SelectionMode, UIA_PROVIDER_ID},
    error::{Failure, Result, codes},
    windows::{self, WINDOW_NOT_FOUND_MESSAGE, WindowState},
};
use jab::JavaProviderAdapter;
use std::{
    collections::{HashSet, VecDeque},
    sync::{Arc, mpsc::Receiver},
    time::{Duration, Instant},
};
use uia::UiaProviderAdapter;

const MAX_PROVIDER_PROBE_WINDOW: Duration = Duration::from_millis(500);

pub(super) trait WindowsAccessibilityProvider {
    fn id(&self) -> ProviderId;

    fn prepare_capture(&mut self, _request: Arc<RequestState>) -> Result<Option<CapturePreparation>> {
        Ok(None)
    }

    fn probe_window(
        &mut self,
        _request: Arc<RequestState>,
        _window: windows::Window,
        _deadline: Instant,
    ) -> Result<Option<Receiver<Result<bool>>>> {
        Ok(None)
    }

    fn contains_visible_text(
        &mut self,
        _request: Arc<RequestState>,
        _candidates: Vec<windows::Window>,
        _has_text: &str,
    ) -> Result<Option<Receiver<Result<Vec<windows::Window>>>>> {
        Ok(None)
    }

    fn act(
        &mut self,
        request: Arc<RequestState>,
        id: u64,
        window: windows::Window,
        action: ControlAction,
        query: ControlQuery,
    ) -> Result<Receiver<Result<Option<ControlResult>>>>;

    fn release(&self, id: u64);
    fn retain_sessions(&self, _sessions: &HashSet<u64>) {}
    fn check_health(&self);
}

pub(super) struct ProviderRegistry {
    providers: Vec<Box<dyn WindowsAccessibilityProvider>>,
    fallback: ProviderId,
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::windows_default()
    }
}

impl ProviderRegistry {
    pub(super) fn windows_default() -> Self {
        Self {
            providers: vec![
                Box::new(JavaProviderAdapter::default()),
                Box::new(UiaProviderAdapter::default()),
            ],
            fallback: UIA_PROVIDER_ID,
        }
    }

    #[cfg(test)]
    pub(super) fn with_providers(
        providers: Vec<Box<dyn WindowsAccessibilityProvider>>,
        fallback: ProviderId,
    ) -> Self {
        Self { providers, fallback }
    }

    fn find_provider_mut(&mut self, id: ProviderId) -> Option<&mut Box<dyn WindowsAccessibilityProvider>> {
        self.providers.iter_mut().find(|provider| provider.id() == id)
    }

    pub(super) fn prepare_capture(&mut self, request: Arc<RequestState>) -> Result<Option<CapturePreparation>> {
        match request.session.config.mode {
            SelectionMode::ExplicitProvider(expected) => {
                let Some(provider) = self.find_provider_mut(expected) else {
                    return Err(Failure::new(
                        codes::CONFIGURATION,
                        format!("Unsupported accessibility provider: {expected}."),
                    ));
                };
                provider.prepare_capture(request)
            }
            SelectionMode::Automatic => {
                for provider in &mut self.providers {
                    if let Some(preparation) = provider.prepare_capture(request.clone())? {
                        return Ok(Some(preparation));
                    }
                }
                Ok(None)
            }
        }
    }

    pub(super) fn resolve_provider(
        &mut self,
        request: Arc<RequestState>,
        window: windows::Window,
    ) -> ProviderSelection<ProviderId> {
        if let SelectionMode::ExplicitProvider(expected) = request.session.config.mode {
            return ProviderSelection::Ready(expected);
        }
        let until = provider_probe_deadline(&request);
        let fallback = self.fallback;
        let mut probes = VecDeque::new();
        for provider in &mut self.providers {
            let id = provider.id();
            if id == fallback {
                continue;
            }
            if let Ok(Some(receiver)) = provider.probe_window(request.clone(), window.clone(), until) {
                probes.push_back((id, receiver));
            }
        }
        if probes.is_empty() {
            ProviderSelection::Ready(fallback)
        } else {
            ProviderSelection::Pending { probes, until, fallback }
        }
    }

    pub(super) fn find_with_text(
        &mut self,
        request: Arc<RequestState>,
        candidates: Vec<windows::Window>,
        has_text: &str,
    ) -> Result<CandidateSearch<windows::Window>> {
        for provider in &mut self.providers {
            if let Some(receiver) = provider.contains_visible_text(request.clone(), candidates.clone(), has_text)? {
                return Ok(CandidateSearch::Pending(receiver));
            }
        }
        Ok(CandidateSearch::Ready(candidates))
    }

    pub(super) fn act(
        &mut self,
        provider_id: ProviderId,
        request: Arc<RequestState>,
        id: u64,
        window: windows::Window,
        action: ControlAction,
        query: ControlQuery,
    ) -> Result<Receiver<Result<Option<ControlResult>>>> {
        let Some(provider) = self.find_provider_mut(provider_id) else {
            return Err(Failure::new(
                codes::CONFIGURATION,
                format!("Unsupported accessibility provider: {provider_id}."),
            ));
        };
        provider.act(request, id, window, action, query)
    }

    pub(super) fn release(&self, id: u64, provider_id: ProviderId) {
        if let Some(provider) = self.providers.iter().find(|provider| provider.id() == provider_id) {
            provider.release(id);
        }
    }

    pub(super) fn retain_sessions(&self, sessions: HashSet<u64>) {
        for provider in &self.providers {
            provider.retain_sessions(&sessions);
        }
    }

    pub(super) fn check_health(&self) {
        for provider in &self.providers {
            provider.check_health();
        }
    }
}

pub(super) fn provider_probe_deadline(request: &RequestState) -> Instant {
    let now = Instant::now();
    now + (request.deadline.saturating_duration_since(now) / 2).min(MAX_PROVIDER_PROBE_WINDOW)
}

pub(super) fn ensure_ready(state: WindowState) -> Result<()> {
    match state {
        WindowState::Ready => Ok(()),
        WindowState::Closed => Err(Failure::new(codes::STALE, WINDOW_NOT_FOUND_MESSAGE)),
        WindowState::Inaccessible(error) | WindowState::Replaced(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::SessionConfig, engine::SessionState};
    use lane::Lane;
    use std::sync::{atomic::AtomicBool, mpsc};

    fn request_with_config(config: SessionConfig, timeout: Duration) -> Arc<RequestState> {
        let session = Arc::new(SessionState {
            id: 1,
            closed: AtomicBool::new(false),
            config,
        });
        Arc::new(RequestState::new(session, Instant::now() + timeout, false))
    }

    fn request(timeout: Duration) -> Arc<RequestState> {
        request_with_config(SessionConfig::automatic(), timeout)
    }

    struct FakeWindowProvider {
        id: ProviderId,
        fail_probe: bool,
        match_probe: bool,
    }

    impl FakeWindowProvider {
        fn new(id: ProviderId) -> Self {
            Self {
                id,
                fail_probe: false,
                match_probe: false,
            }
        }
    }

    impl WindowsAccessibilityProvider for FakeWindowProvider {
        fn id(&self) -> ProviderId {
            self.id
        }

        fn probe_window(
            &mut self,
            _: Arc<RequestState>,
            _: windows::Window,
            _: Instant,
        ) -> Result<Option<Receiver<Result<bool>>>> {
            if self.fail_probe {
                return Err(Failure::new(codes::BRIDGE, "probe failed"));
            }
            let (sender, receiver) = mpsc::channel();
            sender.send(Ok(self.match_probe)).unwrap();
            Ok(Some(receiver))
        }

        fn act(
            &mut self,
            _: Arc<RequestState>,
            _: u64,
            _: windows::Window,
            _: ControlAction,
            _: ControlQuery,
        ) -> Result<Receiver<Result<Option<ControlResult>>>> {
            let (sender, receiver) = mpsc::channel();
            sender.send(Ok(Some(ControlResult { text: Some(self.id.into()) }))).unwrap();
            Ok(receiver)
        }

        fn release(&self, _: u64) {}

        fn check_health(&self) {}
    }

    fn dummy_window() -> windows::Window {
        windows::Window {
            hwnd: 1,
            pid: 1,
            title: "Test".into(),
        }
    }

    #[test]
    fn a_blocked_provider_does_not_keep_the_other_provider_from_responding() {
        let blocked = Lane::start("blocked-test-provider", || (), |_| {}).unwrap();
        let ready = Lane::start("available-test-provider", || (), |_| {}).unwrap();
        let (entered, started) = mpsc::channel();
        let (release, finish) = mpsc::channel();
        let blocked_reply = blocked
            .submit(request(Duration::from_millis(250)), move |_, _| {
                entered.send(()).unwrap();
                finish.recv_timeout(Duration::from_secs(5)).unwrap();
                Ok(())
            })
            .unwrap();
        started.recv_timeout(Duration::from_secs(2)).unwrap();
        let reply = ready.submit(request(Duration::from_secs(1)), |_, _| Ok(7)).unwrap();
        assert_eq!(reply.recv_timeout(Duration::from_secs(1)).unwrap().unwrap(), 7);
        assert!(matches!(
            blocked_reply.recv_timeout(Duration::from_millis(365)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        assert_eq!(blocked.check_health().unwrap_err().code, codes::BRIDGE);
        release.send(()).unwrap();
        blocked_reply.recv_timeout(Duration::from_secs(1)).unwrap().unwrap();
        assert_eq!(blocked.check_health().unwrap_err().code, codes::BRIDGE);
    }

    #[test]
    fn provider_registry_selects_first_matching_provider_and_falls_back_on_error() {
        let mut failing_primary = FakeWindowProvider::new("custom");
        failing_primary.fail_probe = true;
        let mut registry = ProviderRegistry::with_providers(
            vec![Box::new(failing_primary), Box::new(FakeWindowProvider::new(UIA_PROVIDER_ID))],
            UIA_PROVIDER_ID,
        );
        match registry.resolve_provider(request(Duration::from_secs(1)), dummy_window()) {
            ProviderSelection::Ready(selected) => assert_eq!(selected, UIA_PROVIDER_ID),
            ProviderSelection::Pending { .. } => panic!("expected immediate fallback"),
        }

        let mut matching_primary = FakeWindowProvider::new("custom");
        matching_primary.match_probe = true;
        let mut registry = ProviderRegistry::with_providers(
            vec![Box::new(matching_primary), Box::new(FakeWindowProvider::new(UIA_PROVIDER_ID))],
            UIA_PROVIDER_ID,
        );
        let selected = match registry.resolve_provider(request(Duration::from_secs(1)), dummy_window()) {
            ProviderSelection::Pending { mut probes, .. } => {
                let (id, rx) = probes.pop_front().unwrap();
                assert!(rx.recv().unwrap().unwrap());
                id
            }
            ProviderSelection::Ready(selected) => selected,
        };
        assert_eq!(selected, "custom");
        let reply = registry
            .act(
                selected,
                request(Duration::from_secs(1)),
                1,
                dummy_window(),
                ControlAction::Click,
                ControlQuery::default(),
            )
            .unwrap()
            .recv()
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(reply.text.as_deref(), Some("custom"));

        let explicit_req = request_with_config(SessionConfig::explicit_provider("custom"), Duration::from_secs(1));
        match registry.resolve_provider(explicit_req.clone(), dummy_window()) {
            ProviderSelection::Ready(id) => assert_eq!(id, "custom"),
            ProviderSelection::Pending { .. } => panic!("expected Ready for explicit provider"),
        }
        let err = registry
            .act(
                "unknown",
                explicit_req,
                1,
                dummy_window(),
                ControlAction::Click,
                ControlQuery::default(),
            )
            .err()
            .unwrap();
        assert_eq!(err.code, codes::CONFIGURATION);
    }

    #[test]
    fn provider_registry_chains_multiple_providers_before_fallback() {
        let declining = FakeWindowProvider::new("first");
        let mut matching = FakeWindowProvider::new("second");
        matching.match_probe = true;
        let mut registry = ProviderRegistry::with_providers(
            vec![
                Box::new(declining),
                Box::new(matching),
                Box::new(FakeWindowProvider::new(UIA_PROVIDER_ID)),
            ],
            UIA_PROVIDER_ID,
        );
        let ProviderSelection::Pending { mut probes, fallback, .. } =
            registry.resolve_provider(request(Duration::from_secs(1)), dummy_window())
        else {
            panic!("expected chained pending probes");
        };
        assert_eq!(fallback, UIA_PROVIDER_ID);
        assert_eq!(probes.len(), 2);
        let (first_id, first_rx) = probes.pop_front().unwrap();
        assert_eq!(first_id, "first");
        assert!(!first_rx.recv().unwrap().unwrap());
        let (second_id, second_rx) = probes.pop_front().unwrap();
        assert_eq!(second_id, "second");
        assert!(second_rx.recv().unwrap().unwrap());
    }
}
