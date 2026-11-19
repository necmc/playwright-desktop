use crate::{error::{Failure, Result}, jab::{Object, loader::{Library, dll_path}}, windows};
use pyo3::prelude::*;
use std::{collections::{HashMap, HashSet, VecDeque}, path::PathBuf, rc::Rc,
    sync::{Arc, Mutex, OnceLock, atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering}, mpsc::{self, Sender, Receiver}},
    time::{Duration, Instant}};

struct SessionState { id: u64, closed: AtomicBool, java_home: String }

struct RequestState {
    id: u64,
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
    in_native: AtomicBool,
    progress: AtomicU64,
    active_request: AtomicU64,
    active_deadline: Mutex<Option<Instant>>,
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
        if java_home.is_empty() || java_home.contains('\0') || !std::path::Path::new(java_home).is_absolute() {
            return Err(as_python(Failure::new("CONFIGURATION", "java_home must be an absolute runtime directory.")));
        }
        let id = self.shared.next.fetch_add(1, Ordering::Relaxed);
        self.shared.sessions.lock().unwrap().insert(id, Arc::new(SessionState { id, closed: AtomicBool::new(false), java_home: java_home.into() }));
        Ok(id)
    }

    fn new_request(&self, session_id: u64, timeout_ms: u64) -> PyResult<Request> {
        self.healthy().map_err(as_python)?;
        if timeout_ms > u32::MAX as u64 { return Err(as_python(Failure::new("CONFIGURATION", "Timeout exceeds the supported range."))); }
        let session = self.shared.sessions.lock().unwrap().get(&session_id).cloned()
            .ok_or_else(|| as_python(Failure::new("CLOSED", "The desktop session is closed.")))?;
        let timeout = if timeout_ms == 0 { 10_000 } else { timeout_ms };
        Ok(Request { state: Arc::new(RequestState {
            id: self.shared.next.fetch_add(1, Ordering::Relaxed), session,
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
        py.detach(move || {
            match receiver.recv_timeout(state.deadline.saturating_duration_since(Instant::now())) {
                Ok(result) => result,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    state.cancelled.store(true, Ordering::Release);
                    let progress = shared.progress.load(Ordering::Acquire);
                    let active_request = shared.active_request.load(Ordering::Acquire);
                    // A deadline can coincide with a normal enumeration tick. Give the
                    // worker a bounded opportunity to acknowledge cancellation before
                    // classifying it as stuck inside a foreign native call.
                    if shared.native_overdue() {
                        for _ in 0..20 {
                            if !shared.in_native.load(Ordering::Acquire) || shared.progress.load(Ordering::Acquire) != progress { break; }
                            std::thread::sleep(Duration::from_millis(5));
                        }
                    }
                    if shared.native_overdue() && shared.progress.load(Ordering::Acquire) == progress
                        && shared.active_request.load(Ordering::Acquire) == active_request {
                        shared.poisoned.store(true, Ordering::Release);
                        Err(Failure::new("TIMEOUT", "A native bridge call exceeded its deadline; its effect may be unknown. Restart Python before using desktop actions again."))
                    } else {
                        Err(Failure::new("TIMEOUT", "No unique accessible Java window or control became ready before the deadline. Check the title, PID and Java Access Bridge configuration."))
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    shared.poisoned.store(true, Ordering::Release);
                    Err(Failure::new("BRIDGE", "The native worker stopped. Restart Python."))
                }
            }
        }).map_err(as_python)
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
    fn new(sender: Sender<Command>) -> Self {
        Self {
            sender,
            sessions: Mutex::new(HashMap::new()),
            captures_cancelled: Mutex::new(HashSet::new()),
            next: AtomicU64::new(1),
            queued: AtomicUsize::new(0),
            poisoned: AtomicBool::new(false),
            in_native: AtomicBool::new(false),
            progress: AtomicU64::new(0),
            active_request: AtomicU64::new(0),
            active_deadline: Mutex::new(None),
        }
    }

    fn native_overdue(&self) -> bool {
        self.in_native.load(Ordering::Acquire)
            && self.active_deadline.lock().unwrap().is_some_and(|deadline| Instant::now() >= deadline)
    }
}

struct Capture { session: u64, title: String, pid: Option<u32>, before: HashSet<(isize, u32)>, request: Arc<RequestState> }
struct BoundWindow { session: u64, window: windows::Window, root: Object }
struct Lease { session: u64, capture: u64, window: u64 }

struct Worker {
    shared: Arc<Shared>, library: Option<Rc<Library>>, captures: HashMap<u64, Capture>,
    windows: HashMap<u64, BoundWindow>, closed: HashMap<u64, u64>, lease: Option<Lease>,
    library_path: Option<PathBuf>, validated_sessions: HashSet<u64>,
}

impl Worker {
    fn new(shared: Arc<Shared>) -> Self {
        Self { shared, library: None, captures: HashMap::new(), windows: HashMap::new(), closed: HashMap::new(), lease: None, library_path: None, validated_sessions: HashSet::new() }
    }

    fn run(mut self, receiver: Receiver<Command>) {
        let mut pending = VecDeque::new();
        loop {
            if let Ok(command) = receiver.recv_timeout(Duration::from_millis(if pending.is_empty() { 15 } else { 5 })) { pending.push_back(command); }
            for command in receiver.try_iter().take(64) { pending.push_back(command); }
            if !self.shared.poisoned.load(Ordering::Acquire) {
                self.shared.active_request.store(0, Ordering::Release);
                *self.shared.active_deadline.lock().unwrap() = Some(Instant::now() + Duration::from_secs(10));
                self.shared.in_native.store(true, Ordering::Release);
                windows::pump();
                self.cleanup();
                self.shared.in_native.store(false, Ordering::Release);
                *self.shared.active_deadline.lock().unwrap() = None;
                self.shared.progress.fetch_add(1, Ordering::Release);
            }
            for _ in 0..pending.len() {
                let command = pending.pop_front().unwrap();
                let result = if self.shared.poisoned.load(Ordering::Acquire) {
                    Err(Failure::new("BRIDGE", "The native engine is unavailable; restart Python."))
                } else if let Err(error) = command.request.check() { Err(error) }
                else {
                    self.shared.active_request.store(command.request.id, Ordering::Release);
                    *self.shared.active_deadline.lock().unwrap() = Some(command.request.deadline);
                    self.shared.in_native.store(true, Ordering::Release);
                    let result = self.step(&command);
                    self.shared.in_native.store(false, Ordering::Release);
                    *self.shared.active_deadline.lock().unwrap() = None;
                    self.shared.progress.fetch_add(1, Ordering::Release);
                    result
                };
                match result {
                    Ok(None) if !command.request.immediate => pending.push_back(command),
                    Ok(None) => self.complete(command, Err(Failure::new("TIMEOUT", "The requested accessible window or control is not ready (timeout=0)."))),
                    Ok(Some(reply)) => self.complete(command, Ok(reply)),
                    Err(error) => self.complete(command, Err(error)),
                }
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
        let abandoned: Vec<(u64, u64)> = self.captures.iter()
            .filter(|(_, capture)| capture.request.cancelled.load(Ordering::Acquire) || Instant::now() >= capture.request.deadline)
            .map(|(id, capture)| (capture.session, *id)).collect();
        for (session, capture) in abandoned { self.cancel(session, capture); }
        self.captures.retain(|_, capture| live.contains(&capture.session));
        self.closed.retain(|_, session| live.contains(session));
        self.validated_sessions.retain(|session| live.contains(session));
        let removed: Vec<u64> = self.windows.iter().filter(|(_, item)| !live.contains(&item.session) || !windows::valid(&item.window)).map(|(id, _)| *id).collect();
        for id in removed {
            if let Some(item) = self.windows.remove(&id) {
                if live.contains(&item.session) { self.closed.insert(id, item.session); }
            }
            if self.lease.as_ref().is_some_and(|lease| lease.window == id) { self.lease = None; }
        }
        if self.lease.as_ref().is_some_and(|lease| !live.contains(&lease.session)) { self.lease = None; }
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

    fn library(&mut self, request: &RequestState) -> Result<Rc<Library>> {
        if !self.validated_sessions.contains(&request.session.id) {
            request.check()?;
            let path = dll_path(&request.session.java_home)?;
            request.check()?;
            if let Some(selected) = self.library_path.as_ref() {
                if selected != &path { return Err(Failure::new("CONFIGURATION", "This process already loaded a different Java bridge. Start another Python process to change it.")); }
            } else {
                self.library = Some(Library::load(&path)?);
                self.library_path = Some(path);
                windows::pump();
            }
            self.validated_sessions.insert(request.session.id);
        }
        Ok(self.library.as_ref().unwrap().clone())
    }

    fn matches(title: &str, pid: Option<u32>) -> Result<Vec<windows::Window>> {
        if title.is_empty() || title.contains('\0') { return Err(Failure::new("CONFIGURATION", "An exact, nonempty window title is required.")); }
        Ok(windows::list()?.into_iter().filter(|window| window.title == title && pid.is_none_or(|pid| window.pid == pid)).collect())
    }

    fn discover(&mut self, request: &RequestState, title: &str, pid: Option<u32>, before: Option<&HashSet<(isize, u32)>>) -> Result<Option<u64>> {
        let session = request.session.id;
        let library = self.library(request)?;
        let mut found = None;
        for window in Self::matches(title, pid)? {
            if before.is_some_and(|before| before.contains(&(window.hwnd, window.pid))) { continue; }
            if let Some(root) = Object::root(&library, &window) {
                if found.is_some() { return Err(Failure::new("AMBIGUOUS", format!("More than one accessible Java window matches {title:?}. Add a PID filter."))); }
                found = Some((window, root));
            }
        }
        if let Some((window, root)) = found {
            let id = self.shared.next.fetch_add(1, Ordering::Relaxed);
            self.windows.insert(id, BoundWindow { session, window, root });
            if let Some(lease) = self.lease.as_mut() { lease.window = id; }
            else { self.lease = Some(Lease { session, capture: 0, window: id }); }
            Ok(Some(id))
        } else { Ok(None) }
    }

    fn step(&mut self, command: &Command) -> Result<Option<Reply>> {
        let request = &command.request;
        let args = &command.arguments;
        request.check()?;
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
        self.library(request)?;
        let before = Self::matches(&args.title, args.pid)?
            .into_iter()
            .map(|item| (item.hwnd, item.pid))
            .collect();
        request.check()?;
        let session = request.session.id;
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
        if item.window.title != args.title || args.pid.is_some_and(|pid| pid != item.window.pid) {
            return Err(Failure::new("BUSY", "This session is already bound to a different Java window."));
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
        if !windows::valid(&item.window) {
            self.invalidate_window(id);
            if waiting_for_close { return Ok(Some(Reply { handle: id, text: None })); }
            return Err(Failure::new("STALE", "The native window no longer exists."));
        }
        let library = self.library.as_ref().unwrap();
        let current = Object::root(library, &item.window);
        if let Some(current) = current {
            if !item.root.same(&current) {
                self.invalidate_window(id);
                return Err(Failure::new("STALE", "Windows reused the handle for a different Java window."));
            }
        } else if !waiting_for_close {
            self.invalidate_window(id);
            return Err(Failure::new("STALE", "The Java window is no longer accessible."));
        }
        if waiting_for_close { return Ok(None); }
        if !matches!(args.role.as_str(), "textbox" | "button") || args.name.is_empty() {
            return Err(Failure::new("CONFIGURATION", "A supported role and exact accessible name are required."));
        }
        let check = || request.check();
        let control = match item.root.find(&args.role, &args.name, &check)? { Some(control) => control, None => return Ok(None) };
        check()?;
        if !control.ready(operation == WindowOperation::Fill)? { return Ok(None); }
        let mut reply = Reply { handle: id, text: None };
        match (operation, args.role.as_str()) {
            (WindowOperation::ReadText, "textbox") => reply.text = Some(control.text()?),
            (WindowOperation::Fill, "textbox") => control.fill(&args.text, &check)?,
            (WindowOperation::Click, "button") => control.click(&check)?,
            _ => return Err(Failure::new("CONFIGURATION", "The action does not support this role.")),
        }
        Ok(Some(reply))
    }

    fn invalidate_window(&mut self, id: u64) {
        if let Some(item) = self.windows.remove(&id) { self.closed.insert(id, item.session); }
        if self.lease.as_ref().is_some_and(|lease| lease.window == id) { self.lease = None; }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jab::{ffi::{JavaObject, VmId}, loader::tests::test_library};
    use ::windows::{
        Win32::{
            Foundation::HWND,
            UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, WINDOW_EX_STYLE, WS_POPUP},
        },
        core::w,
    };
    use std::cell::Cell;

    #[test]
    fn request_cancellation_and_closure_are_checked() {
        let session = Arc::new(SessionState { id: 1, closed: AtomicBool::new(false), java_home: String::new() });
        let request = RequestState { id: 2, session: session.clone(), deadline: Instant::now() + Duration::from_secs(1), immediate: false, cancelled: AtomicBool::new(false) };
        assert!(request.check().is_ok());
        request.cancelled.store(true, Ordering::Release);
        assert_eq!(request.check().unwrap_err().code, "CANCELLED");
        request.cancelled.store(false, Ordering::Release);
        session.closed.store(true, Ordering::Release);
        assert_eq!(request.check().unwrap_err().code, "CANCELLED");
    }
    #[test]
    fn deadlines_are_monotonic() {
        let request = RequestState { id: 1, session: Arc::new(SessionState { id: 2, closed: AtomicBool::new(false), java_home: String::new() }), deadline: Instant::now() - Duration::from_millis(1), immediate: false, cancelled: AtomicBool::new(false) };
        assert_eq!(request.check().unwrap_err().code, "TIMEOUT");
    }

    #[test]
    fn a_reused_hwnd_cannot_route_an_action_to_a_different_java_root() {
        struct HiddenWindow(HWND);
        impl Drop for HiddenWindow {
            fn drop(&mut self) {
                // SAFETY: this test thread created and owns the window.
                unsafe { let _ = DestroyWindow(self.0); }
            }
        }

        thread_local! {
            static ROOT: Cell<JavaObject> = const { Cell::new(1) };
        }

        unsafe extern "C" fn current_root(_: HWND, vm: *mut VmId, object: *mut JavaObject) -> i32 {
            // SAFETY: Object::root passes writable out parameters.
            unsafe {
                *vm = 7;
                *object = ROOT.get();
            }
            1
        }

        // SAFETY: STATIC is a system class and the window has no parent or callback data.
        let handle = HiddenWindow(unsafe {
            CreateWindowExW(WINDOW_EX_STYLE(0), w!("STATIC"), w!("JAB identity test"),
                           WS_POPUP, 0, 0, 1, 1, None, None, None, None)
        }.expect("create hidden window"));
        let window = windows::Window {
            hwnd: handle.0.0 as isize,
            pid: std::process::id(),
            title: "JAB identity test".into(),
        };
        assert!(windows::valid(&window));

        let mut library = test_library();
        Rc::get_mut(&mut library).unwrap().context = current_root;
        let root = Object::root(&library, &window).expect("original Java root");
        let (sender, _receiver) = mpsc::channel();
        let mut worker = Worker::new(Arc::new(Shared::new(sender)));
        worker.library = Some(library);
        let session = Arc::new(SessionState { id: 1, closed: AtomicBool::new(false), java_home: String::new() });
        worker.windows.insert(2, BoundWindow { session: session.id, window, root });
        worker.lease = Some(Lease { session: session.id, capture: 0, window: 2 });
        let request = Arc::new(RequestState {
            id: 3,
            session,
            deadline: Instant::now() + Duration::from_secs(1),
            immediate: false,
            cancelled: AtomicBool::new(false),
        });
        let (reply, _result) = mpsc::channel();
        let action = Command {
            request,
            operation: Operation::Window(WindowOperation::Click),
            arguments: Arguments { window_id: 2, role: "button".into(), name: "Confirmar".into(), ..Arguments::default() },
            reply,
        };

        ROOT.set(4);
        let error = worker.step(&action).err().expect("reject the changed Java root");
        assert_eq!(error.code, "STALE");
        assert!(error.message.contains("reused the handle"));
        assert!(!worker.windows.contains_key(&2));
        assert!(worker.lease.is_none());
    }
}
