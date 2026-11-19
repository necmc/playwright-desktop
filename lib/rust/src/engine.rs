use crate::{
    backend::{Backend, ControlResult, SessionOptions, TIMEOUT_MESSAGE, WindowKey},
    error::{Failure, Result},
    windows,
};
use providers::{Providers, WindowProvider};
use pyo3::prelude::*;
use std::{collections::{HashMap, HashSet, VecDeque},
    sync::{Arc, Condvar, Mutex, OnceLock, atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering}, mpsc::{self, Sender, Receiver}},
    time::{Duration, Instant}};

struct SessionState { id: u64, closed: AtomicBool, options: SessionOptions }

struct RequestState {
    session: Arc<SessionState>,
    deadline: Instant,
    immediate: bool,
    cancelled: AtomicBool,
    action_pending: AtomicBool,
}

impl RequestState {
    fn check(&self) -> Result<()> {
        if self.cancelled.load(Ordering::Acquire) || self.session.closed.load(Ordering::Acquire) {
            return Err(Failure::new("CANCELLED", "The desktop request or its session was cancelled."));
        }
        if Instant::now() >= self.deadline {
            let message = if self.action_pending.load(Ordering::Acquire) {
                "The desktop action timed out after dispatch; its effect may be unknown."
            } else {
                "The desktop request exceeded its deadline."
            };
            return Err(Failure::new("TIMEOUT", message));
        }
        Ok(())
    }
}

struct Shared {
    sender: Sender<Command>,
    sessions: Mutex<HashMap<u64, Arc<SessionState>>>,
    captures_cancelled: Mutex<HashSet<(u64, u64)>>,
    next: AtomicU64,
    queued: AtomicUsize,
    poisoned: AtomicBool,
    native_activity: Mutex<NativeActivity>,
    native_finished: Condvar,
}

#[derive(Default)]
struct NativeActivity {
    generation: u64,
    deadline: Option<Instant>,
}

impl NativeActivity {
    fn overdue_generation(&self) -> Option<u64> {
        self.deadline.filter(|&deadline| Instant::now() >= deadline).map(|_| self.generation)
    }
}

static SHARED: OnceLock<Mutex<Option<Arc<Shared>>>> = OnceLock::new();

#[derive(Clone, Default)]
struct Arguments {
    window_id: u64,
    capture_id: u64,
    title: String,
    pid: Option<u32>,
    role: String,
    name: String,
    text: String,
    scope_role: String,
    scope_name: String,
}

#[pyclass]
#[derive(Default)]
pub struct Reply {
    #[pyo3(get)] pub handle: u64,
    #[pyo3(get)] pub text: Option<String>,
}

#[derive(Clone, Copy)]
enum Operation {
    PrepareCapture,
    CompleteCapture,
    Window(WindowOperation),
}

impl Operation {
    fn parse(name: &str) -> Result<Self> {
        match name {
            "prepare" => Ok(Self::PrepareCapture),
            "capture" => Ok(Self::CompleteCapture),
            "read_text" => Ok(Self::Window(WindowOperation::ReadText)),
            "fill" => Ok(Self::Window(WindowOperation::Fill)),
            "click" => Ok(Self::Window(WindowOperation::Click)),
            "wait_closed" => Ok(Self::Window(WindowOperation::WaitClosed)),
            _ => Err(Failure::new("CONFIGURATION", "Unsupported native operation.")),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum WindowOperation {
    ReadText,
    Fill,
    Click,
    WaitClosed,
}

struct Command {
    request: Arc<RequestState>,
    operation: Operation,
    arguments: Arguments,
    reply: Sender<Result<Reply>>,
    pending: Option<PendingCall>,
}

enum PendingCall {
    ValidateJava { result: Receiver<Result<()>>, until: Option<Instant> },
    ResolveWindowProvider { id: u64, java_recognition: Receiver<Result<bool>>, until: Instant },
    Action { id: u64, result: Receiver<Result<Option<ControlResult>>> },
}

#[pyclass]
#[derive(Clone)]
pub struct Request { state: Arc<RequestState> }

#[pymethods]
impl Request {
    fn cancel(&self) { self.state.cancelled.store(true, Ordering::Release); }
}

#[pyclass]
pub struct Engine { shared: Arc<Shared> }

fn as_python(error: Failure) -> PyErr { crate::NativeError::new_err((error.code, error.message)) }

#[pymethods]
impl Engine {
    #[new]
    fn new() -> PyResult<Self> {
        let mut global = SHARED.get_or_init(|| Mutex::new(None)).lock()
            .map_err(|_| as_python(Failure::new("BRIDGE", "The engine initialization lock failed.")))?;
        if global.is_none() {
            let (sender, receiver) = mpsc::channel();
            let shared = Arc::new(Shared::new(sender));
            let owner = shared.clone();
            std::thread::Builder::new().name("playwright-desktop-windows".into())
                .spawn(move || {
                    let state = owner.clone();
                    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| Worker::new(owner).run(receiver))).is_err() {
                        state.poisoned.store(true, Ordering::Release);
                    }
                }).map_err(|e| as_python(Failure::new("BRIDGE", format!("Cannot start the native worker: {e}"))))?;
            *global = Some(shared);
        }
        Ok(Self { shared: global.as_ref().unwrap().clone() })
    }

    fn open_session(&self, java_home: &str) -> PyResult<u64> {
        let options = SessionOptions::java(java_home).map_err(as_python)?;
        self.register_session(options)
    }

    #[pyo3(signature = (java_home=None, *, explicit=false))]
    fn open_auto_session(&self, java_home: Option<&str>, explicit: bool) -> PyResult<u64> {
        self.register_session(SessionOptions::automatic(java_home, explicit).map_err(as_python)?)
    }

    fn new_request(&self, session_id: u64, timeout_ms: u64) -> PyResult<Request> {
        self.healthy().map_err(as_python)?;
        if timeout_ms > u32::MAX as u64 { return Err(as_python(Failure::new("CONFIGURATION", "Timeout exceeds the supported range."))); }
        let session = self.shared.sessions.lock().unwrap().get(&session_id).cloned()
            .ok_or_else(|| as_python(Failure::new("CLOSED", "The desktop session is closed.")))?;
        let timeout = if timeout_ms == 0 { 10_000 } else { timeout_ms };
        Ok(Request { state: Arc::new(RequestState {
            session,
            deadline: Instant::now() + Duration::from_millis(timeout),
            immediate: timeout_ms == 0, cancelled: AtomicBool::new(false), action_pending: AtomicBool::new(false),
        }) })
    }

    #[pyo3(signature = (request, operation, *, window_id=0, capture_id=0, title="", pid=None, role="", name="", text="", scope_role="", scope_name=""))]
    fn invoke(&self, py: Python<'_>, request: &Request, operation: &str,
              window_id: u64, capture_id: u64, title: &str, pid: Option<u32>,
              role: &str, name: &str, text: &str, scope_role: &str, scope_name: &str) -> PyResult<Reply> {
        self.healthy().map_err(as_python)?;
        request.state.check().map_err(as_python)?;
        let operation = Operation::parse(operation).map_err(as_python)?;
        if self.shared.queued.fetch_add(1, Ordering::AcqRel) >= 64 {
            self.shared.queued.fetch_sub(1, Ordering::AcqRel);
            return Err(as_python(Failure::new("BUSY", "The native request queue is full.")));
        }
        let (sender, receiver) = mpsc::channel();
        let state = request.state.clone();
        let command = Command { request: state.clone(), operation,
            arguments: Arguments { window_id, capture_id, title: title.into(), pid, role: role.into(),
                name: name.into(), text: text.into(), scope_role: scope_role.into(), scope_name: scope_name.into() },
            reply: sender, pending: None };
        if self.shared.sender.send(command).is_err() {
            self.shared.queued.fetch_sub(1, Ordering::AcqRel);
            self.shared.poisoned.store(true, Ordering::Release);
            return Err(as_python(Failure::new("BRIDGE", "The native worker stopped. Restart Python.")));
        }
        let shared = self.shared.clone();
        py.detach(move || shared.wait_for_reply(&state, receiver)).map_err(as_python)
    }

    fn close_session(&self, session_id: u64) {
        if let Some(session) = self.shared.sessions.lock().unwrap().remove(&session_id) {
            session.closed.store(true, Ordering::Release);
        }
    }

    fn cancel_capture(&self, session_id: u64, capture_id: u64) {
        self.shared.captures_cancelled.lock().unwrap().insert((session_id, capture_id));
    }
}

impl Engine {
    fn register_session(&self, options: SessionOptions) -> PyResult<u64> {
        self.healthy().map_err(as_python)?;
        let id = self.shared.next.fetch_add(1, Ordering::Relaxed);
        self.shared.sessions.lock().unwrap().insert(id, Arc::new(SessionState {
            id, closed: AtomicBool::new(false), options,
        }));
        Ok(id)
    }

    fn healthy(&self) -> Result<()> {
        if self.shared.poisoned.load(Ordering::Acquire) {
            Err(Failure::new("BRIDGE", "The shared desktop engine is unavailable after a native failure. Restart Python."))
        } else { Ok(()) }
    }
}

impl Shared {
    fn wait_for_reply(&self, state: &RequestState, receiver: Receiver<Result<Reply>>) -> Result<Reply> {
        match receiver.recv_timeout(state.deadline.saturating_duration_since(Instant::now())) {
            Ok(result) => result,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                state.cancelled.store(true, Ordering::Release);
                if self.mark_stalled_call() {
                    Err(Failure::new("TIMEOUT", "A native bridge call exceeded its deadline; its effect may be unknown. Restart Python before using desktop actions again."))
                } else if state.action_pending.load(Ordering::Acquire) {
                    Err(Failure::new("TIMEOUT", "The desktop action timed out after dispatch; its effect may be unknown."))
                } else {
                    Err(Failure::new("TIMEOUT", TIMEOUT_MESSAGE))
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                self.poisoned.store(true, Ordering::Release);
                Err(Failure::new("BRIDGE", "The native worker stopped. Restart Python."))
            }
        }
    }

    fn new(sender: Sender<Command>) -> Self {
        Self {
            sender,
            sessions: Mutex::new(HashMap::new()),
            captures_cancelled: Mutex::new(HashSet::new()),
            next: AtomicU64::new(1),
            queued: AtomicUsize::new(0),
            poisoned: AtomicBool::new(false),
            native_activity: Mutex::new(NativeActivity::default()),
            native_finished: Condvar::new(),
        }
    }

    fn run_native<T>(&self, deadline: Instant, work: impl FnOnce() -> T) -> Option<T> {
        {
            let mut activity = self.native_activity.lock().unwrap();
            if self.poisoned.load(Ordering::Acquire) {
                return None;
            }
            activity.generation = activity.generation.wrapping_add(1);
            activity.deadline = Some(deadline);
        }
        let result = work();
        self.native_activity.lock().unwrap().deadline = None;
        self.native_finished.notify_all();
        Some(result)
    }

    fn mark_stalled_call(&self) -> bool {
        let activity = self.native_activity.lock().unwrap();
        let Some(generation) = activity.overdue_generation() else { return false; };
        // Allow a bounded interval for the worker to acknowledge cancellation. The
        // same lock covers the final check and the admission of the next native call.
        let (activity, _) = self.native_finished.wait_timeout_while(
            activity,
            Duration::from_millis(100),
            |activity| activity.overdue_generation() == Some(generation),
        ).unwrap();
        if activity.overdue_generation() != Some(generation) {
            return false;
        }
        self.poisoned.store(true, Ordering::Release);
        true
    }
}

struct Capture {
    session: u64,
    title: String,
    pid: Option<u32>,
    before: HashSet<WindowKey>,
    request: Arc<RequestState>,
}

struct BoundWindow {
    session: u64,
    window: windows::Window,
    capture: Option<u64>,
    provider: Option<WindowProvider>,
}

struct Worker {
    shared: Arc<Shared>,
    providers: Providers,
    captures: HashMap<u64, Capture>,
    windows: HashMap<u64, BoundWindow>,
    closed: HashMap<u64, u64>,
    live: HashSet<u64>,
}

impl Worker {
    fn new(shared: Arc<Shared>) -> Self {
        Self {
            shared, providers: Providers::default(), captures: HashMap::new(),
            windows: HashMap::new(), closed: HashMap::new(), live: HashSet::new(),
        }
    }

    fn run(mut self, receiver: Receiver<Command>) {
        let shared = self.shared.clone();
        let mut pending = VecDeque::new();
        loop {
            if let Ok(command) = receiver.recv_timeout(Duration::from_millis(if pending.is_empty() { 15 } else { 5 })) {
                pending.push_back(command);
            }
            pending.extend(receiver.try_iter().take(64));
            let _ = shared.run_native(Instant::now() + Duration::from_secs(10), || self.cleanup());
            self.process_pending(&mut pending);
        }
    }

    fn process_pending(&mut self, pending: &mut VecDeque<Command>) {
        let shared = self.shared.clone();
        for _ in 0..pending.len() {
            let mut command = pending.pop_front().unwrap();
            let result = if self.shared.poisoned.load(Ordering::Acquire) {
                Err(Failure::new("BRIDGE", "The native engine is unavailable; restart Python."))
            } else if let Err(error) = command.request.check() {
                Err(error)
            } else {
                shared.run_native(command.request.deadline, || self.step(&mut command))
                    .unwrap_or_else(|| Err(Failure::new("BRIDGE", "The native engine is unavailable; restart Python.")))
            };
            match result {
                Ok(None) if command.pending.is_some() || !command.request.immediate => pending.push_back(command),
                Ok(None) => self.complete(command, Err(Failure::new("TIMEOUT", "The requested desktop window or control is not ready (timeout=0)."))),
                Ok(Some(reply)) => self.complete(command, Ok(reply)),
                Err(error) => self.complete(command, Err(error)),
            }
        }
    }

    fn complete(&mut self, command: Command, result: Result<Reply>) {
        if result.is_err() && matches!(command.operation, Operation::CompleteCapture) {
            self.cancel(command.request.session.id, command.arguments.capture_id);
        }
        self.shared.queued.fetch_sub(1, Ordering::AcqRel);
        let _ = command.reply.send(result);
    }

    fn cleanup(&mut self) {
        self.providers.check_health();
        let live: HashSet<u64> = self.shared.sessions.lock().unwrap().keys().copied().collect();
        let cancellations = std::mem::take(&mut *self.shared.captures_cancelled.lock().unwrap());
        for (session, capture) in cancellations { self.cancel(session, capture); }
        self.discard_abandoned_captures();
        self.captures.retain(|_, capture| live.contains(&capture.session));
        self.closed.retain(|_, session| live.contains(session));

        let removed: Vec<u64> = self.windows.iter()
            .filter(|(_, item)| !live.contains(&item.session) || !windows::valid(&item.window))
            .map(|(id, _)| *id).collect();
        for id in removed {
            self.release_window(id, &live);
        }
        if live != self.live {
            self.providers.retain_sessions(live.clone());
            self.live = live;
        }
    }

    fn discard_abandoned_captures(&mut self) {
        self.captures.retain(|_, capture| capture.request.check().is_ok());
    }

    fn cancel(&mut self, session: u64, capture_id: u64) {
        if self.captures.get(&capture_id).is_some_and(|capture| capture.session == session) {
            self.captures.remove(&capture_id);
        }
        let owned = self.windows.iter().find(|(_, window)| {
            window.session == session && window.capture == Some(capture_id)
        }).map(|(id, _)| *id);
        if let Some(id) = owned {
            self.release_window(id, &HashSet::new());
        }
    }

    fn require_owner(&self, session: u64) -> Result<()> {
        if self.windows.values().any(|window| window.session != session)
            || self.captures.values().any(|capture| capture.session != session) {
            return Err(Failure::new("BUSY", "Another desktop flow is active. Close its window or desktop session first."));
        }
        Ok(())
    }

    fn discover(
        &mut self, request: &RequestState, title: &str, pid: Option<u32>, capture: Option<(u64, &HashSet<WindowKey>)>,
    ) -> Result<Option<u64>> {
        let session = request.session.id;
        self.require_owner(session)?;
        let mut candidates = Backend::candidates(title, pid)?.into_iter()
            .filter(|window| !capture.is_some_and(|(_, before)| before.contains(&window.key())));
        let Some(candidate) = candidates.next() else { return Ok(None); };
        if candidates.next().is_some() {
            let filter = if title.is_empty() { "an exact title" } else { "a PID" };
            return Err(Failure::new("AMBIGUOUS", format!("More than one window matches. Add {filter} to narrow the search.")));
        }
        request.check()?;
        if let Some((&id, _)) = self.windows.iter().find(|(_, item)| {
            item.window.hwnd == candidate.0.hwnd && item.window.pid == candidate.0.pid
        }) {
            return Ok(Some(id));
        }
        let id = self.shared.next.fetch_add(1, Ordering::Relaxed);
        self.windows.insert(id, BoundWindow {
            session, window: candidate.0, capture: capture.map(|(id, _)| id), provider: None,
        });
        Ok(Some(id))
    }

    fn step(&mut self, command: &mut Command) -> Result<Option<Reply>> {
        command.request.check()?;
        self.discard_abandoned_captures();
        if let Some(pending) = command.pending.take() {
            return self.resume(command, pending);
        }
        match command.operation {
            Operation::PrepareCapture if !command.request.session.options.automatic => {
                self.require_owner(command.request.session.id)?;
                command.pending = Some(PendingCall::ValidateJava {
                    result: self.providers.validate_java(command.request.clone())?, until: None,
                });
                Ok(None)
            }
            Operation::PrepareCapture => {
                if command.request.session.options.explicit_java_home {
                    self.require_owner(command.request.session.id)?;
                    if let Some(result) = self.providers.validate_loaded_java(command.request.clone()) {
                        command.pending = Some(PendingCall::ValidateJava {
                            result, until: Some(java_check_deadline(&command.request)),
                        });
                        return Ok(None);
                    }
                }
                self.prepare_capture(&command.request, &command.arguments)
            }
            Operation::CompleteCapture => self.complete_capture(&command.request, command.arguments.capture_id),
            Operation::Window(operation) => self.operate_window(command, operation),
        }
    }

    fn resume(&mut self, command: &mut Command, pending: PendingCall) -> Result<Option<Reply>> {
        match pending {
            PendingCall::ValidateJava { result, until } => {
                match receive(&result) {
                    Ok(Some(response)) => response?,
                    Ok(None) if until.is_none_or(|deadline| Instant::now() < deadline) => {
                        command.pending = Some(PendingCall::ValidateJava { result, until });
                        return Ok(None);
                    }
                    Err(error) if until.is_none() => return Err(error),
                    _ => {},
                }
                self.prepare_capture(&command.request, &command.arguments)
            }
            PendingCall::ResolveWindowProvider { id, java_recognition, until } => {
                let provider = match receive(&java_recognition) {
                    Ok(Some(response)) => {
                        if response? { WindowProvider::JavaAccessBridge } else { WindowProvider::UiAutomation }
                    }
                    Err(_) => WindowProvider::UiAutomation,
                    Ok(None) if Instant::now() >= until => WindowProvider::UiAutomation,
                    Ok(None) => {
                        command.pending = Some(PendingCall::ResolveWindowProvider { id, java_recognition, until });
                        return Ok(None);
                    }
                };
                self.dispatch(command, id, provider)
            }
            PendingCall::Action { id, result } => {
                let Some(response) = receive(&result)? else {
                    command.pending = Some(PendingCall::Action { id, result });
                    return Ok(None);
                };
                command.request.action_pending.store(false, Ordering::Release);
                match response {
                    Ok(Some(result)) => Ok(Some(Reply { handle: id, text: result.text })),
                    Ok(None) => Ok(None),
                    Err(error) => {
                        if error.code == "STALE" { self.invalidate_window(id); }
                        Err(error)
                    }
                }
            }
        }
    }

    fn prepare_capture(&mut self, request: &Arc<RequestState>, args: &Arguments) -> Result<Option<Reply>> {
        let session = request.session.id;
        self.require_owner(session)?;
        let before = Backend::candidates(&args.title, args.pid)?.into_iter().map(|window| window.key()).collect();
        request.check()?;
        let id = self.shared.next.fetch_add(1, Ordering::Relaxed);
        self.captures.insert(id, Capture {
            session, title: args.title.clone(), pid: args.pid, before, request: request.clone(),
        });
        Ok(Some(Reply { handle: id, text: None }))
    }

    fn complete_capture(&mut self, request: &RequestState, capture_id: u64) -> Result<Option<Reply>> {
        let capture = self.captures.get(&capture_id).filter(|capture| capture.session == request.session.id)
            .ok_or_else(|| Failure::new("CLOSED", "The window capture is no longer active."))?;
        let title = capture.title.clone();
        let pid = capture.pid;
        let before = capture.before.clone();
        let Some(id) = self.discover(request, &title, pid, Some((capture_id, &before)))? else {
            return Ok(None);
        };
        self.captures.remove(&capture_id);
        Ok(Some(Reply { handle: id, text: None }))
    }

    fn resolve_window(&mut self, request: &RequestState, args: &Arguments) -> Result<Option<u64>> {
        if args.window_id != 0 {
            return Ok(Some(args.window_id));
        }
        self.discover(request, &args.title, args.pid, None)
    }

    fn operate_window(&mut self, command: &mut Command, operation: WindowOperation) -> Result<Option<Reply>> {
        let request = &command.request;
        let Some(id) = self.resolve_window(request, &command.arguments)? else { return Ok(None); };
        let session = request.session.id;
        let waiting_for_close = operation == WindowOperation::WaitClosed;
        if waiting_for_close && self.closed.get(&id) == Some(&session) {
            return Ok(Some(Reply { handle: id, text: None }));
        }
        let item = self.window(id, session)?;
        if !windows::valid(&item.window) {
            self.invalidate_window(id);
            if waiting_for_close { return Ok(Some(Reply { handle: id, text: None })); }
            return Err(Failure::new("STALE", "The native window no longer exists."));
        }
        if waiting_for_close {
            return Ok(None);
        }
        if let Some(provider) = item.provider {
            return self.dispatch(command, id, provider);
        }
        self.resolve_window_provider(command, id, item.window.clone())
    }

    fn resolve_window_provider(&mut self, command: &mut Command, id: u64, window: windows::Window) -> Result<Option<Reply>> {
        let request = &command.request;
        if !request.session.options.automatic || windows::is_java_process(&window) {
            return self.dispatch(command, id, WindowProvider::JavaAccessBridge);
        }
        let until = java_check_deadline(request);
        match self.providers.check_java_window(request.clone(), window) {
            Ok(java_recognition) => {
                command.pending = Some(PendingCall::ResolveWindowProvider { id, java_recognition, until });
                Ok(None)
            }
            Err(_) => self.dispatch(command, id, WindowProvider::UiAutomation),
        }
    }

    fn dispatch(&mut self, command: &mut Command, id: u64, provider: WindowProvider) -> Result<Option<Reply>> {
        let request = &command.request;
        request.check()?;
        self.window(id, request.session.id)?;
        let item = self.windows.get_mut(&id).unwrap();
        let provider = *item.provider.get_or_insert(provider);
        let Operation::Window(operation) = command.operation else { unreachable!() };
        let result = self.providers.act(provider, request.clone(), id, item.window.clone(), operation, command.arguments.clone())?;
        if operation == WindowOperation::Click {
            request.action_pending.store(true, Ordering::Release);
        }
        command.pending = Some(PendingCall::Action { id, result });
        Ok(None)
    }

    fn window(&self, id: u64, session: u64) -> Result<&BoundWindow> {
        self.windows.get(&id).filter(|window| window.session == session)
            .ok_or_else(|| Failure::new("STALE", "This native window is closed or belongs to another session."))
    }

    fn invalidate_window(&mut self, id: u64) {
        if let Some(item) = self.windows.remove(&id) {
            self.closed.insert(id, item.session);
            self.providers.release(id, item.provider);
        }
    }

    fn release_window(&mut self, id: u64, live: &HashSet<u64>) {
        if let Some(item) = self.windows.remove(&id) {
            if live.contains(&item.session) { self.closed.insert(id, item.session); }
            self.providers.release(id, item.provider);
        }
    }
}

fn receive<T>(reply: &Receiver<T>) -> Result<Option<T>> {
    match reply.try_recv() {
        Ok(value) => Ok(Some(value)),
        Err(mpsc::TryRecvError::Empty) => Ok(None),
        Err(mpsc::TryRecvError::Disconnected) => Err(Failure::new("BRIDGE", "The desktop accessibility provider stopped.")),
    }
}

fn java_check_deadline(request: &RequestState) -> Instant {
    let now = Instant::now();
    now + (request.deadline.saturating_duration_since(now) / 2).min(Duration::from_millis(500))
}

mod providers;

#[cfg(test)]
mod tests;
