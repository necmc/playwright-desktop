use super::{Arguments, RequestState, WindowOperation};
use crate::{
    backend::{Backend, ControlAction, ControlResult, Target, WindowCandidate, WindowState},
    error::{Failure, Result},
    uia, windows,
};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex, mpsc::{self, Receiver, Sender}},
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum WindowProvider {
    JavaAccessBridge,
    UiAutomation,
}

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
        thread::Builder::new().name(name.into()).spawn(move || {
            let mut state = init();
            loop {
                idle(&mut state);
                match receiver.recv_timeout(Duration::from_millis(10)) {
                    Ok(job) => job(&mut state),
                    Err(mpsc::RecvTimeoutError::Timeout) => {},
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        }).map_err(|error| Failure::new("BRIDGE", format!("Cannot start {name}: {error}")))?;
        Ok(Self { sender, status: Arc::new(Mutex::new(LaneStatus::default())) })
    }

    fn submit<Output: Send + 'static>(
        &self, request: Arc<RequestState>, operation: impl FnOnce(&mut State, &RequestState) -> Result<Output> + Send + 'static,
    ) -> Result<Receiver<Result<Output>>> {
        self.check_health()?;
        let (reply, receiver) = mpsc::channel();
        let status = self.status.clone();
        self.sender.send(Box::new(move |state| {
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
        })).map_err(|_| unavailable())?;
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
    Failure::new("BRIDGE", "The desktop accessibility provider is unavailable. Restart Python to recover it.")
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

#[derive(Default)]
pub(super) struct Providers {
    java: Option<Lane<Java>>,
    uia: Option<Lane<Uia>>,
}

impl Providers {
    pub(super) fn check_health(&self) {
        if let Some(lane) = &self.java { let _ = lane.check_health(); }
        if let Some(lane) = &self.uia { let _ = lane.check_health(); }
    }

    fn java(&mut self) -> Result<&Lane<Java>> {
        if self.java.is_none() {
            self.java = Some(Lane::start("playwright-desktop-jab", Java::default, |state| state.backend.poll())?);
        }
        Ok(self.java.as_ref().unwrap())
    }

    fn uia(&mut self) -> Result<&Lane<Uia>> {
        if self.uia.is_none() {
            self.uia = Some(Lane::start("playwright-desktop-uia", Uia::default, |_| {})?);
        }
        Ok(self.uia.as_ref().unwrap())
    }

    pub(super) fn validate_java(&mut self, request: Arc<RequestState>) -> Result<Receiver<Result<()>>> {
        self.java()?.submit(request, |state, request| {
            state.backend.prepare_session(request.session.id, &request.session.options, &|| request.check())
        })
    }

    pub(super) fn validate_loaded_java(&self, request: Arc<RequestState>) -> Option<Receiver<Result<()>>> {
        self.java.as_ref()?.submit(request, |state, request| {
            state.backend.validate_loaded_bridge(&request.session.options)
        }).ok()
    }

    pub(super) fn check_java_window(
        &mut self, request: Arc<RequestState>, window: windows::Window,
    ) -> Result<Receiver<Result<bool>>> {
        self.java()?.submit(request, move |state, request| {
            let prepared = state.backend.prepare_session(request.session.id, &request.session.options, &|| request.check());
            request.check()?;
            let recognized = state.backend.recognizes(&window);
            if recognized { prepared?; }
            Ok(recognized)
        })
    }

    pub(super) fn act(
        &mut self, provider: WindowProvider, request: Arc<RequestState>, id: u64, window: windows::Window,
        operation: WindowOperation, args: Arguments,
    ) -> Result<Receiver<Result<Option<ControlResult>>>> {
        match provider {
            WindowProvider::JavaAccessBridge => self.java()?.submit(request, move |state, request| {
                let check = || request.check();
                state.backend.prepare_session(request.session.id, &request.session.options, &check)?;
                if !args.scope_role.is_empty() {
                    return Err(Failure::new("ACTION", "Scoped control lookup requires UI Automation."));
                }
                let target = if let Some(target) = state.targets.get(&id) {
                    target
                } else {
                    check()?;
                    let Some(target) = state.backend.attach(WindowCandidate(window))? else {
                        return Ok(None);
                    };
                    state.targets.entry(id).or_insert(target)
                };
                ensure_ready(target.state())?;
                target.perform(action(operation, &args), &args.role, &args.name, &check)
            }),
            WindowProvider::UiAutomation => self.uia()?.submit(request, move |state, request| {
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
                    let Some(target) = client.attach(&window)? else { return Ok(None); };
                    state.targets.entry(id).or_insert(target)
                };
                client.perform(
                    target, action(operation, &args), uia::Selector { role: &args.role, name: &args.name },
                    (!args.scope_role.is_empty()).then_some(uia::Selector { role: &args.scope_role, name: &args.scope_name }),
                    &check,
                )
            }),
        }
    }

    pub(super) fn release(&self, id: u64, provider: Option<WindowProvider>) {
        match provider {
            Some(WindowProvider::JavaAccessBridge) => {
                if let Some(lane) = &self.java { lane.cleanup(move |state| { state.targets.remove(&id); }); }
            }
            Some(WindowProvider::UiAutomation) => {
                if let Some(lane) = &self.uia { lane.cleanup(move |state| { state.targets.remove(&id); }); }
            }
            None => {},
        }
    }

    pub(super) fn retain_sessions(&self, sessions: HashSet<u64>) {
        if let Some(lane) = &self.java {
            lane.cleanup(move |state| state.backend.retain_sessions(&sessions));
        }
    }
}

fn ensure_ready(state: WindowState) -> Result<()> {
    match state {
        WindowState::Ready => Ok(()),
        WindowState::Closed => Err(Failure::new("STALE", "The native window no longer exists.")),
        WindowState::Inaccessible(error) | WindowState::Replaced(error) => Err(error),
    }
}

fn action(operation: WindowOperation, args: &Arguments) -> ControlAction<'_> {
    match operation {
        WindowOperation::ReadText => ControlAction::ReadText,
        WindowOperation::Fill => ControlAction::Fill(&args.text),
        WindowOperation::Click => ControlAction::Click,
        WindowOperation::WaitClosed => unreachable!("window closure is resolved by the coordinator"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{backend::SessionOptions, engine::SessionState};
    use std::sync::atomic::AtomicBool;

    fn request(timeout: Duration) -> Arc<RequestState> {
        Arc::new(RequestState {
            session: Arc::new(SessionState {
                id: 1, closed: AtomicBool::new(false), options: SessionOptions::automatic(None, false).unwrap(),
            }),
            deadline: Instant::now() + timeout, immediate: false, cancelled: AtomicBool::new(false),
            action_pending: AtomicBool::new(false),
        })
    }

    #[test]
    fn a_blocked_provider_does_not_keep_the_other_provider_from_responding() {
        let blocked = Lane::start("blocked-test-provider", || (), |_| {}).unwrap();
        let ready = Lane::start("available-test-provider", || (), |_| {}).unwrap();
        let (entered, started) = mpsc::channel();
        let (release, finish) = mpsc::channel();
        let blocked_reply = blocked.submit(request(Duration::from_millis(250)), move |_, _| {
            entered.send(()).unwrap();
            finish.recv_timeout(Duration::from_secs(5)).unwrap();
            Ok(())
        }).unwrap();
        started.recv_timeout(Duration::from_secs(2)).unwrap();
        let reply = ready.submit(request(Duration::from_secs(1)), |_, _| Ok(7)).unwrap();
        assert_eq!(reply.recv_timeout(Duration::from_secs(1)).unwrap().unwrap(), 7);
        assert!(matches!(blocked_reply.recv_timeout(Duration::from_millis(365)), Err(mpsc::RecvTimeoutError::Timeout)));
        assert_eq!(blocked.check_health().unwrap_err().code, "BRIDGE");
        release.send(()).unwrap();
        blocked_reply.recv_timeout(Duration::from_secs(1)).unwrap().unwrap();
        assert_eq!(blocked.check_health().unwrap_err().code, "BRIDGE");
    }
}
