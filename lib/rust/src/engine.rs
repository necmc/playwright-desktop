use crate::{
    backend::{Backend, ControlAction, SessionOptions, TIMEOUT_MESSAGE, Target, WindowKey, WindowState},
    error::{Failure, Result},
};
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
}

impl RequestState {
    fn check(&self) -> Result<()> {
        if self.cancelled.load(Ordering::Acquire) || self.session.closed.load(Ordering::Acquire) {
            return Err(Failure::new("CANCELLED", "The desktop request or its session was cancelled."));
        }
        if Instant::now() >= self.deadline { return Err(Failure::new("TIMEOUT", "The desktop request exceeded its deadline.")); }
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

#[derive(Default)]
struct Arguments {
    window_id: u64,
    capture_id: u64,
    title: String,
    pid: Option<u32>,
    role: String,
    name: String,
    text: String,
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
            std::thread::Builder::new().name("playwright-desktop-jab".into())
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
        self.healthy().map_err(as_python)?;
        let options = SessionOptions::java(java_home).map_err(as_python)?;
        let id = self.shared.next.fetch_add(1, Ordering::Relaxed);
        self.shared.sessions.lock().unwrap().insert(id, Arc::new(SessionState { id, closed: AtomicBool::new(false), options }));
        Ok(id)
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
            immediate: timeout_ms == 0, cancelled: AtomicBool::new(false),
        }) })
    }

    #[pyo3(signature = (request, operation, *, window_id=0, capture_id=0, title="", pid=None, role="", name="", text=""))]
    fn invoke(&self, py: Python<'_>, request: &Request, operation: &str,
              window_id: u64, capture_id: u64, title: &str, pid: Option<u32>,
              role: &str, name: &str, text: &str) -> PyResult<Reply> {
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
            arguments: Arguments { window_id, capture_id, title: title.into(), pid, role: role.into(), name: name.into(), text: text.into() }, reply: sender };
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

struct Capture { session: u64, title: String, pid: Option<u32>, before: HashSet<WindowKey>, request: Arc<RequestState> }
struct BoundWindow { session: u64, target: Target }
struct Lease { session: u64, capture: u64, window: u64 }

struct Worker {
    shared: Arc<Shared>, backend: Backend, captures: HashMap<u64, Capture>,
    windows: HashMap<u64, BoundWindow>, closed: HashMap<u64, u64>, lease: Option<Lease>,
}

impl Worker {
    fn new(shared: Arc<Shared>) -> Self {
        Self { shared, backend: Backend::default(), captures: HashMap::new(), windows: HashMap::new(), closed: HashMap::new(), lease: None }
    }

    fn run(mut self, receiver: Receiver<Command>) {
        let shared = self.shared.clone();
        let mut pending = VecDeque::new();
        loop {
            if let Ok(command) = receiver.recv_timeout(Duration::from_millis(if pending.is_empty() { 15 } else { 5 })) { pending.push_back(command); }
            for command in receiver.try_iter().take(64) { pending.push_back(command); }
            let _ = shared.run_native(Instant::now() + Duration::from_secs(10), || {
                self.backend.poll();
                self.cleanup();
            });
            self.process_pending(&mut pending);
        }
    }

    fn process_pending(&mut self, pending: &mut VecDeque<Command>) {
        let shared = self.shared.clone();
        for _ in 0..pending.len() {
            let command = pending.pop_front().unwrap();
            let result = if self.shared.poisoned.load(Ordering::Acquire) {
                Err(Failure::new("BRIDGE", "The native engine is unavailable; restart Python."))
            } else if let Err(error) = command.request.check() {
                Err(error)
            } else {
                shared.run_native(command.request.deadline, || self.step(&command))
                    .unwrap_or_else(|| Err(Failure::new("BRIDGE", "The native engine is unavailable; restart Python.")))
            };
            match result {
                Ok(None) if !command.request.immediate => pending.push_back(command),
                Ok(None) => self.complete(command, Err(Failure::new("TIMEOUT", "The requested accessible window or control is not ready (timeout=0)."))),
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
        let live: HashSet<u64> = self.shared.sessions.lock().unwrap().keys().copied().collect();
        let cancellations = std::mem::take(&mut *self.shared.captures_cancelled.lock().unwrap());
        for (session, capture) in cancellations { self.cancel(session, capture); }
        self.discard_abandoned_capture();
        self.captures.retain(|_, capture| live.contains(&capture.session));
        self.closed.retain(|_, session| live.contains(session));
        self.backend.retain_sessions(&live);
        let removed: Vec<u64> = self.windows.iter().filter(|(_, item)| !live.contains(&item.session) || !item.target.is_present()).map(|(id, _)| *id).collect();
        for id in removed {
            if let Some(item) = self.windows.remove(&id) {
                if live.contains(&item.session) { self.closed.insert(id, item.session); }
            }
            if self.lease.as_ref().is_some_and(|lease| lease.window == id) { self.lease = None; }
        }
        if self.lease.as_ref().is_some_and(|lease| !live.contains(&lease.session)) { self.lease = None; }
    }

    fn discard_abandoned_capture(&mut self) {
        let Some(lease) = &self.lease else { return; };
        if self.captures.get(&lease.capture).is_some_and(|capture| capture.request.check().is_err()) {
            self.cancel(lease.session, lease.capture);
        }
    }

    fn cancel(&mut self, session: u64, capture_id: u64) {
        if self.captures.get(&capture_id).is_some_and(|capture| capture.session == session) {
            self.captures.remove(&capture_id);
        }
        if self.lease.as_ref().is_some_and(|lease| lease.session == session && lease.capture == capture_id) {
            let lease = self.lease.take().unwrap();
            self.windows.remove(&lease.window);
        }
    }

    fn discover(&mut self, request: &RequestState, title: &str, pid: Option<u32>, before: Option<&HashSet<WindowKey>>) -> Result<Option<u64>> {
        let session = request.session.id;
        self.backend.prepare_session(session, &request.session.options, &|| request.check())?;
        let mut found = None;
        for window in self.backend.candidates(title, pid)? {
            request.check()?;
            if before.is_some_and(|before| before.contains(&window.key())) { continue; }
            if let Some(target) = self.backend.attach(window)? {
                if found.is_some() { return Err(Failure::new("AMBIGUOUS", format!("More than one accessible window matches {title:?}. Add a PID filter."))); }
                found = Some(target);
            }
        }
        if let Some(target) = found {
            request.check()?;
            let id = self.shared.next.fetch_add(1, Ordering::Relaxed);
            self.windows.insert(id, BoundWindow { session, target });
            if let Some(lease) = self.lease.as_mut() { lease.window = id; }
            else { self.lease = Some(Lease { session, capture: 0, window: id }); }
            Ok(Some(id))
        } else { Ok(None) }
    }

    fn step(&mut self, command: &Command) -> Result<Option<Reply>> {
        let request = &command.request;
        let args = &command.arguments;
        request.check()?;
        // A preparation can be cancelled after cleanup and before the next queued command.
        self.discard_abandoned_capture();
        match command.operation {
            Operation::PrepareCapture => self.prepare_capture(request, args),
            Operation::CompleteCapture => self.complete_capture(request, args.capture_id),
            Operation::Window(operation) => self.operate_window(request, operation, args),
        }
    }

    fn prepare_capture(&mut self, request: &Arc<RequestState>, args: &Arguments) -> Result<Option<Reply>> {
        if self.lease.is_some() {
            return Err(Failure::new("BUSY", "Another desktop flow is active. Close its window or desktop session first."));
        }
        let session = request.session.id;
        self.backend.prepare_session(session, &request.session.options, &|| request.check())?;
        let before = self.backend.candidates(&args.title, args.pid)?
            .into_iter()
            .map(|window| window.key())
            .collect();
        request.check()?;
        let id = self.shared.next.fetch_add(1, Ordering::Relaxed);
        self.captures.insert(id, Capture {
            session,
            title: args.title.clone(),
            pid: args.pid,
            before,
            request: request.clone(),
        });
        self.lease = Some(Lease { session, capture: id, window: 0 });
        Ok(Some(Reply { handle: id, text: None }))
    }

    fn complete_capture(&mut self, request: &RequestState, capture_id: u64) -> Result<Option<Reply>> {
        let capture = self.captures.get(&capture_id)
            .filter(|capture| capture.session == request.session.id)
            .ok_or_else(|| Failure::new("CLOSED", "The window capture is no longer active."))?;
        let title = capture.title.clone();
        let pid = capture.pid;
        let before = capture.before.clone();
        let Some(id) = self.discover(request, &title, pid, Some(&before))? else {
            return Ok(None);
        };
        self.captures.remove(&capture_id);
        Ok(Some(Reply { handle: id, text: None }))
    }

    fn resolve_window(&mut self, request: &RequestState, args: &Arguments) -> Result<Option<u64>> {
        if args.window_id != 0 {
            return Ok(Some(args.window_id));
        }
        let Some(lease) = &self.lease else {
            return self.discover(request, &args.title, args.pid, None);
        };
        if lease.session != request.session.id || lease.window == 0 {
            return Err(Failure::new("BUSY", "Another desktop flow is active."));
        }
        let item = self.windows.get(&lease.window)
            .ok_or_else(|| Failure::new("STALE", "The desktop session's window is no longer available."))?;
        if !item.target.matches(&args.title, args.pid) {
            return Err(Failure::new("BUSY", "This session is already bound to a different desktop window."));
        }
        Ok(Some(lease.window))
    }

    fn operate_window(
        &mut self, request: &RequestState, operation: WindowOperation, args: &Arguments,
    ) -> Result<Option<Reply>> {
        let Some(id) = self.resolve_window(request, args)? else {
            return Ok(None);
        };
        let session = request.session.id;
        let waiting_for_close = operation == WindowOperation::WaitClosed;
        if waiting_for_close && self.closed.get(&id) == Some(&session) {
            return Ok(Some(Reply { handle: id, text: None }));
        }
        let item = self.windows.get(&id).filter(|window| window.session == session)
            .ok_or_else(|| Failure::new("STALE", "This native window is closed or belongs to another session."))?;
        match item.target.state() {
            WindowState::Closed => {
                self.invalidate_window(id);
                if waiting_for_close { return Ok(Some(Reply { handle: id, text: None })); }
                return Err(Failure::new("STALE", "The native window no longer exists."));
            }
            WindowState::Inaccessible(_) if waiting_for_close => return Ok(None),
            WindowState::Inaccessible(error) | WindowState::Replaced(error) => {
                self.invalidate_window(id);
                return Err(error);
            }
            WindowState::Ready => {}
        }
        let action = match operation {
            WindowOperation::ReadText => ControlAction::ReadText,
            WindowOperation::Fill => ControlAction::Fill(&args.text),
            WindowOperation::Click => ControlAction::Click,
            WindowOperation::WaitClosed => return Ok(None),
        };
        Ok(item.target.perform(action, &args.role, &args.name, &|| request.check())?
            .map(|result| Reply { handle: id, text: result.text }))
    }

    fn invalidate_window(&mut self, id: u64) {
        if let Some(item) = self.windows.remove(&id) { self.closed.insert(id, item.session); }
        if self.lease.as_ref().is_some_and(|lease| lease.window == id) { self.lease = None; }
    }
}

#[cfg(test)]
mod tests;
