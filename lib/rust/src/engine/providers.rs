use super::{
    RequestState,
    platform::{
        CandidateSearch, CapturePreparation, ControlAction, ControlQuery, ControlResult,
        ProviderSelection,
    },
};
use crate::{
    backend::{Backend, JAVA_EXPLICIT_OPTION, JAVA_PROVIDER_ID, Target},
    config::{ProviderId, SelectionMode},
    error::{Failure, Result},
    uia,
    windows::{self, WindowState},
};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::{Duration, Instant},
};

const UIA_PROVIDER_ID: ProviderId = "uia";

type Job<State> = Box<dyn FnOnce(&mut State) + Send>;

/// Each provider owns its handles on an independent thread. The coordinator polls
/// replies without waiting on either provider's native calls.
struct Lane<State> {
    sender: Sender<Job<State>>,
    status: Arc<Mutex<LaneStatus>>,
}

#[derive(Default)]
struct LaneStatus {
    active_deadline: Option<Instant>,
    stopped: bool,
}

impl<State: 'static> Lane<State> {
    fn start(name: &str, init: fn() -> State, idle: fn(&mut State)) -> Result<Self> {
        let (sender, receiver) = mpsc::channel::<Job<State>>();
        thread::Builder::new()
            .name(name.into())
            .spawn(move || {
                let mut state = init();
                loop {
                    idle(&mut state);
                    match receiver.recv_timeout(Duration::from_millis(10)) {
                        Ok(job) => job(&mut state),
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
            })
            .map_err(|error| Failure::new("BRIDGE", format!("Cannot start {name}: {error}")))?;
        Ok(Self {
            sender,
            status: Arc::new(Mutex::new(LaneStatus::default())),
        })
    }

    fn submit<Output: Send + 'static>(
        &self,
        request: Arc<RequestState>,
        operation: impl FnOnce(&mut State, &RequestState) -> Result<Output> + Send + 'static,
    ) -> Result<Receiver<Result<Output>>> {
        self.check_health()?;
        let (reply, receiver) = mpsc::channel();
        let status = self.status.clone();
        self.sender
            .send(Box::new(move |state| {
                let result = (|| {
                    request.check()?;
                    {
                        let mut status = status.lock().unwrap();
                        if status.stopped {
                            return Err(unavailable());
                        }
                        status.active_deadline = Some(request.deadline);
                    }
                    let result = operation(state, &request);
                    status.lock().unwrap().active_deadline = None;
                    result
                })();
                let _ = reply.send(result);
            }))
            .map_err(|_| unavailable())?;
        Ok(receiver)
    }

    fn check_health(&self) -> Result<()> {
        let mut status = self.status.lock().unwrap();
        if status.active_deadline.is_some_and(|deadline| {
            Instant::now().saturating_duration_since(deadline) >= Duration::from_millis(100)
        }) {
            status.stopped = true;
        }
        if status.stopped { Err(unavailable()) } else { Ok(()) }
    }

    fn cleanup(&self, job: impl FnOnce(&mut State) + Send + 'static) {
        if self.check_health().is_ok() {
            let _ = self.sender.send(Box::new(job));
        }
    }
}

fn unavailable() -> Failure {
    Failure::new(
        "BRIDGE",
        "The desktop accessibility provider is unavailable. Restart the host process to recover it.",
    )
}

#[derive(Default)]
struct Java {
    backend: Backend,
    targets: HashMap<u64, Target>,
}

#[derive(Default)]
struct Uia {
    targets: HashMap<u64, uia::Target>,
    client: Option<uia::Client>,
}

pub(super) trait WindowsAccessibilityProvider {
    fn id(&self) -> ProviderId;
    fn prepare_capture(&mut self, request: Arc<RequestState>) -> Result<Option<CapturePreparation>>;
    fn probe_window(
        &mut self,
        request: Arc<RequestState>,
        window: windows::Window,
        deadline: Instant,
        fallback: ProviderId,
    ) -> Result<Option<Receiver<Result<ProviderId>>>>;
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

#[derive(Default)]
struct JavaProviderAdapter {
    lane: Option<Lane<Java>>,
}

impl JavaProviderAdapter {
    fn lane(&mut self) -> Result<&Lane<Java>> {
        if self.lane.is_none() {
            self.lane = Some(Lane::start("playwright-desktop-jab", Java::default, |state| state.backend.poll())?);
        }
        Ok(self.lane.as_ref().unwrap())
    }
}

impl WindowsAccessibilityProvider for JavaProviderAdapter {
    fn id(&self) -> ProviderId {
        JAVA_PROVIDER_ID
    }

    fn prepare_capture(&mut self, request: Arc<RequestState>) -> Result<Option<CapturePreparation>> {
        if request.session.config.mode == SelectionMode::ExplicitProvider(JAVA_PROVIDER_ID) {
            let result = self.lane()?.submit(request, |state, request| {
                state.backend.prepare_session(request.session.id, &request.session.config, &|| request.check())
            })?;
            return Ok(Some(CapturePreparation { result, until: None }));
        }
        if !request.session.config.flag(JAVA_EXPLICIT_OPTION) {
            return Ok(None);
        }
        let until = Some(provider_probe_deadline(&request));
        let preparation = self.lane.as_ref().and_then(|lane| {
            lane.submit(request, |state, request| state.backend.validate_loaded_bridge(&request.session.config))
                .ok()
        });
        Ok(preparation.map(|result| CapturePreparation { result, until }))
    }

    fn probe_window(
        &mut self,
        request: Arc<RequestState>,
        window: windows::Window,
        deadline: Instant,
        fallback: ProviderId,
    ) -> Result<Option<Receiver<Result<ProviderId>>>> {
        let retry_until = (!request.immediate && windows::has_java_launcher_name(&window)).then_some(deadline);
        let receiver = self.lane()?.submit(request, move |state, request| {
            let prepared = state
                .backend
                .prepare_session(request.session.id, &request.session.config, &|| request.check());
            loop {
                request.check()?;
                if state.backend.recognizes(&window) {
                    prepared?;
                    return Ok(JAVA_PROVIDER_ID);
                }

                let remaining = retry_until.map_or(Duration::ZERO, |until| {
                    until.saturating_duration_since(Instant::now())
                });
                if prepared.is_err() || remaining.is_zero() {
                    return Ok(fallback);
                }

                // Swing may create its window before JAB receives the registration.
                state.backend.poll();
                thread::sleep(remaining.min(Duration::from_millis(10)));
            }
        })?;
        Ok(Some(receiver))
    }

    fn act(
        &mut self,
        request: Arc<RequestState>,
        id: u64,
        window: windows::Window,
        action: ControlAction,
        query: ControlQuery,
    ) -> Result<Receiver<Result<Option<ControlResult>>>> {
        self.lane()?.submit(request, move |state, request| {
            let check = || request.check();
            state.backend.prepare_session(request.session.id, &request.session.config, &check)?;
            if !query.scope_role.is_empty() {
                return Err(Failure::new("ACTION", "Scoped control lookup requires UI Automation."));
            }
            let target = if let Some(target) = state.targets.get(&id) {
                target
            } else {
                check()?;
                let Some(target) = state.backend.attach(window)? else {
                    return Ok(None);
                };
                state.targets.entry(id).or_insert(target)
            };
            ensure_ready(target.state())?;
            target.perform(action, &query.role, &query.name, request)
        })
    }

    fn release(&self, id: u64) {
        if let Some(lane) = &self.lane {
            lane.cleanup(move |state| {
                state.targets.remove(&id);
            });
        }
    }

    fn retain_sessions(&self, sessions: &HashSet<u64>) {
        if let Some(lane) = &self.lane {
            let live = sessions.clone();
            lane.cleanup(move |state| state.backend.retain_sessions(&live));
        }
    }

    fn check_health(&self) {
        if let Some(lane) = &self.lane {
            let _ = lane.check_health();
        }
    }
}

#[derive(Default)]
struct UiaProviderAdapter {
    lane: Option<Lane<Uia>>,
}

impl UiaProviderAdapter {
    fn lane(&mut self) -> Result<&Lane<Uia>> {
        if self.lane.is_none() {
            self.lane = Some(Lane::start("playwright-desktop-uia", Uia::default, |_| {})?);
        }
        Ok(self.lane.as_ref().unwrap())
    }
}

impl WindowsAccessibilityProvider for UiaProviderAdapter {
    fn id(&self) -> ProviderId {
        UIA_PROVIDER_ID
    }

    fn prepare_capture(&mut self, _: Arc<RequestState>) -> Result<Option<CapturePreparation>> {
        Ok(None)
    }

    fn probe_window(
        &mut self,
        _: Arc<RequestState>,
        _: windows::Window,
        _: Instant,
        _: ProviderId,
    ) -> Result<Option<Receiver<Result<ProviderId>>>> {
        Ok(None)
    }

    fn contains_visible_text(
        &mut self,
        request: Arc<RequestState>,
        candidates: Vec<windows::Window>,
        has_text: &str,
    ) -> Result<Option<Receiver<Result<Vec<windows::Window>>>>> {
        let has_text = has_text.to_string();
        let receiver = self.lane()?.submit(request, move |state, request| {
            let check = || request.check();
            if state.client.is_none() {
                state.client = Some(uia::Client::new()?);
            }
            let client = state.client.as_ref().unwrap();
            let mut matched = Vec::new();
            for window in candidates {
                if client.contains_visible_text(&window, &has_text, &check)? {
                    matched.push(window);
                }
            }
            Ok(matched)
        })?;
        Ok(Some(receiver))
    }

    fn act(
        &mut self,
        request: Arc<RequestState>,
        id: u64,
        window: windows::Window,
        action: ControlAction,
        query: ControlQuery,
    ) -> Result<Receiver<Result<Option<ControlResult>>>> {
        self.lane()?.submit(request, move |state, request| {
            let check = || request.check();
            if state.client.is_none() {
                state.client = Some(uia::Client::new()?);
            }
            let client = state.client.as_ref().unwrap();
            let target = if let Some(target) = state.targets.get(&id) {
                ensure_ready(client.state(target, &window))?;
                target
            } else {
                check()?;
                let Some(target) = client.attach(&window)? else {
                    return Ok(None);
                };
                state.targets.entry(id).or_insert(target)
            };
            client.perform(
                target,
                action,
                uia::Selector {
                    role: &query.role,
                    name: &query.name,
                },
                (!query.scope_role.is_empty()).then_some(uia::Selector {
                    role: &query.scope_role,
                    name: &query.scope_name,
                }),
                request,
            )
        })
    }

    fn release(&self, id: u64) {
        if let Some(lane) = &self.lane {
            lane.cleanup(move |state| {
                state.targets.remove(&id);
            });
        }
    }

    fn check_health(&self) {
        if let Some(lane) = &self.lane {
            let _ = lane.check_health();
        }
    }
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
                        "CONFIGURATION",
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

    pub(super) fn resolve_provider(&mut self, request: Arc<RequestState>, window: windows::Window) -> ProviderSelection<ProviderId> {
        if let SelectionMode::ExplicitProvider(expected) = request.session.config.mode {
            return ProviderSelection::Ready(expected);
        }
        let until = provider_probe_deadline(&request);
        let fallback = self.fallback;
        for provider in &mut self.providers {
            if provider.id() == fallback {
                continue;
            }
            if let Ok(Some(result)) = provider.probe_window(request.clone(), window.clone(), until, fallback) {
                return ProviderSelection::Pending { result, until, fallback };
            }
        }
        ProviderSelection::Ready(fallback)
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
                "CONFIGURATION",
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

fn provider_probe_deadline(request: &RequestState) -> Instant {
    let now = Instant::now();
    now + (request.deadline.saturating_duration_since(now) / 2).min(Duration::from_millis(500))
}

fn ensure_ready(state: WindowState) -> Result<()> {
    match state {
        WindowState::Ready => Ok(()),
        WindowState::Closed => Err(Failure::new("STALE", "The native window no longer exists.")),
        WindowState::Inaccessible(error) | WindowState::Replaced(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SessionConfig;
    use crate::engine::SessionState;
    use std::sync::atomic::AtomicBool;

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
        acted: bool,
    }

    impl FakeWindowProvider {
        fn new(id: ProviderId) -> Self {
            Self {
                id,
                fail_probe: false,
                match_probe: false,
                acted: false,
            }
        }
    }

    impl WindowsAccessibilityProvider for FakeWindowProvider {
        fn id(&self) -> ProviderId {
            self.id
        }

        fn prepare_capture(&mut self, _: Arc<RequestState>) -> Result<Option<CapturePreparation>> {
            Ok(None)
        }

        fn probe_window(
            &mut self,
            _: Arc<RequestState>,
            _: windows::Window,
            _: Instant,
            fallback: ProviderId,
        ) -> Result<Option<Receiver<Result<ProviderId>>>> {
            if self.fail_probe {
                return Err(Failure::new("BRIDGE", "probe failed"));
            }
            let (sender, receiver) = mpsc::channel();
            sender.send(Ok(if self.match_probe { self.id } else { fallback })).unwrap();
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
            self.acted = true;
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
        assert_eq!(blocked.check_health().unwrap_err().code, "BRIDGE");
        release.send(()).unwrap();
        blocked_reply.recv_timeout(Duration::from_secs(1)).unwrap().unwrap();
        assert_eq!(blocked.check_health().unwrap_err().code, "BRIDGE");
    }

    #[test]
    fn provider_registry_selects_first_matching_provider_and_falls_back_on_error() {
        // Proveedor primario con fallo cae al fallback "uia" sin abortar en modo automático.
        let mut failing_primary = FakeWindowProvider::new("custom");
        failing_primary.fail_probe = true;
        let mut registry = ProviderRegistry::with_providers(
            vec![Box::new(failing_primary), Box::new(FakeWindowProvider::new("uia"))],
            "uia",
        );
        match registry.resolve_provider(request(Duration::from_secs(1)), dummy_window()) {
            ProviderSelection::Ready(selected) => assert_eq!(selected, "uia"),
            ProviderSelection::Pending { .. } => panic!("expected immediate fallback"),
        }

        // Cuando un proveedor reconoce la ventana, el registro enruta act() hacia él.
        let mut matching_primary = FakeWindowProvider::new("custom");
        matching_primary.match_probe = true;
        let mut registry = ProviderRegistry::with_providers(
            vec![Box::new(matching_primary), Box::new(FakeWindowProvider::new("uia"))],
            "uia",
        );
        let selected = match registry.resolve_provider(request(Duration::from_secs(1)), dummy_window()) {
            ProviderSelection::Pending { result, .. } => result.recv().unwrap().unwrap(),
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

        // Modo explícito retorna directamente el ID configurado y rechaza desconocidos con CONFIGURATION.
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
        assert_eq!(err.code, "CONFIGURATION");
    }
}
