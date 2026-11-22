use crate::error::{Failure, Result};
use coordinator::{mutation_timeout_message, timeout_message};
use platform::{CapturePreparation, Platform};
pub(crate) use platform::{ControlAction, ControlResult};
pub use platform::{ControlQuery, WindowQuery};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::{
        Arc, Condvar, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU8, AtomicU64, AtomicUsize, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    time::{Duration, Instant},
};

pub(crate) struct SessionState {
    pub(crate) id: u64,
    closed: AtomicBool,
    pub(crate) options: SessionOptions,
}

/// Session settings accepted by the core engine. Only the Windows adapter
/// interprets the Java settings when choosing or preparing a provider.
#[derive(Clone)]
pub struct SessionOptions {
    pub(crate) java_home: Option<String>,
    pub(crate) automatic: bool,
    pub(crate) explicit_java_home: bool,
}

impl SessionOptions {
    pub fn java(java_home: &str) -> Result<Self> {
        if java_home.is_empty() || java_home.contains('\0') || !Path::new(java_home).is_absolute() {
            return Err(Failure::new("CONFIGURATION", "java_home must be an absolute runtime directory."));
        }
        Ok(Self {
            java_home: Some(java_home.into()),
            automatic: false,
            explicit_java_home: true,
        })
    }

    pub fn automatic(java_home: Option<&str>, explicit: bool) -> Result<Self> {
        let mut options = match java_home {
            Some(java_home) => Self::java(java_home)?,
            None => Self {
                java_home: None,
                automatic: true,
                explicit_java_home: false,
            },
        };
        options.automatic = true;
        options.explicit_java_home = explicit && java_home.is_some();
        Ok(options)
    }
}

const REQUEST_CANCELLED: u8 = 1;
const MUTATION_STARTED: u8 = 2;
const REQUEST_EXPIRED: u8 = 4;

pub(crate) fn worker_stopped_failure() -> Failure {
    Failure::new("BRIDGE", "The native worker stopped. Restart the host process.")
}

pub(crate) fn engine_unavailable_failure() -> Failure {
    Failure::new("BRIDGE", "The native engine is unavailable. Restart the host process.")
}

pub(crate) fn shared_engine_unavailable_failure() -> Failure {
    Failure::new(
        "BRIDGE",
        "The shared desktop engine is unavailable after a native failure. Restart the host process.",
    )
}

pub(crate) fn stalled_timeout_failure() -> Failure {
    Failure::new(
        "TIMEOUT",
        "A native bridge call exceeded its deadline; its effect may be unknown. Restart the host process before using desktop actions again.",
    )
}

pub(crate) struct RequestState {
    pub(crate) session: Arc<SessionState>,
    pub(crate) deadline: Instant,
    immediate: bool,
    status: AtomicU8,
}

impl RequestState {
    fn new(session: Arc<SessionState>, deadline: Instant, immediate: bool) -> Self {
        Self {
            session,
            deadline,
            immediate,
            status: AtomicU8::new(0),
        }
    }

    pub(crate) fn check(&self) -> Result<()> {
        let status = self.status.load(Ordering::Acquire);
        if status & REQUEST_CANCELLED != 0 || self.session.closed.load(Ordering::Acquire) {
            return Err(Self::cancelled_error());
        }
        if status & REQUEST_EXPIRED != 0 || Instant::now() >= self.deadline {
            let message = if self.expire() {
                mutation_timeout_message()
            } else {
                "The desktop request exceeded its deadline."
            };
            return Err(Failure::new("TIMEOUT", message));
        }
        Ok(())
    }

    pub(crate) fn begin_mutation(&self) -> Result<()> {
        self.check()?;
        // The deadline, cancellation and mutation admission share one state.
        // A timeout can report whether the native call was already admitted.
        match self.status.compare_exchange(0, MUTATION_STARTED, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) | Err(MUTATION_STARTED) => Ok(()),
            Err(_) => self.check(),
        }
    }

    fn cancel(&self) {
        self.status.fetch_or(REQUEST_CANCELLED, Ordering::AcqRel);
    }

    fn expire(&self) -> bool {
        self.status.fetch_or(REQUEST_EXPIRED, Ordering::AcqRel) & MUTATION_STARTED != 0
    }

    fn cancelled_error() -> Failure {
        Failure::new("CANCELLED", "The desktop request or its session was cancelled.")
    }
}

struct Shared {
    sender: Sender<QueuedCommand>,
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
    window: WindowQuery,
    text: String,
    control: ControlQuery,
}

#[derive(Clone, Debug, Default)]
pub struct CoreReply {
    pub handle: u64,
    pub text: Option<String>,
    pub pid: Option<u32>,
    pub title: Option<String>,
}

impl CoreReply {
    fn handle(handle: u64) -> Self {
        Self {
            handle,
            ..Self::default()
        }
    }
}

#[derive(Clone, Default)]
pub struct InvokeParams {
    pub operation: String,
    pub window_id: u64,
    pub capture_id: u64,
    pub window: WindowQuery,
    pub text: String,
    pub control: ControlQuery,
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
            "wait_visible" => Ok(Self::Window(WindowOperation::WaitVisible)),
            "read_text" => Ok(Self::Window(WindowOperation::ReadText)),
            "fill" => Ok(Self::Window(WindowOperation::Fill)),
            "click" => Ok(Self::Window(WindowOperation::Click)),
            "wait_closed" => Ok(Self::Window(WindowOperation::WaitClosed)),
            "bring_to_front" => Ok(Self::Window(WindowOperation::BringToFront)),
            "close_window" => Ok(Self::Window(WindowOperation::CloseWindow)),
            _ => Err(Failure::new("CONFIGURATION", "Unsupported native operation.")),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum WindowOperation {
    WaitVisible,
    ReadText,
    Fill,
    Click,
    WaitClosed,
    BringToFront,
    CloseWindow,
}

struct Command<P> {
    request: Arc<RequestState>,
    operation: Operation,
    arguments: Arguments,
    reply: Sender<Result<CoreReply>>,
    pending: P,
}

type QueuedCommand = Command<()>;
type ActiveCommand<P> = Command<Option<PendingCall<P>>>;

impl QueuedCommand {
    fn activate<P: Platform>(self) -> ActiveCommand<P> {
        let Self {
            request,
            operation,
            arguments,
            reply,
            pending: (),
        } = self;
        Command {
            request,
            operation,
            arguments,
            reply,
            pending: None,
        }
    }
}

enum PendingCall<P: Platform> {
    PrepareCapture(CapturePreparation),
    CompleteCapture {
        capture_id: u64,
        result: Receiver<Result<Vec<P::Window>>>,
    },
    Discover {
        operation: WindowOperation,
        result: Receiver<Result<Vec<P::Window>>>,
    },
    ResolveProvider {
        id: u64,
        result: Receiver<Result<P::Provider>>,
        until: Instant,
        fallback: P::Provider,
    },
    Action {
        id: u64,
        result: Receiver<Result<Option<ControlResult>>>,
    },
    Closing {
        id: u64,
    },
}

#[derive(Clone)]
pub struct CoreRequest {
    state: Arc<RequestState>,
}

impl CoreRequest {
    pub fn cancel(&self) {
        self.state.cancel();
    }
}

#[derive(Clone)]
pub struct CoreEngine {
    shared: Arc<Shared>,
}

impl CoreEngine {
    pub fn new() -> Result<Self> {
        let mut global = SHARED
            .get_or_init(|| Mutex::new(None))
            .lock()
            .map_err(|_| Failure::new("BRIDGE", "The engine initialization lock failed."))?;
        if global.is_none() {
            let (sender, receiver) = mpsc::channel();
            let shared = Arc::new(Shared::new(sender));
            platform::start_host_worker(shared.clone(), receiver)?;
            *global = Some(shared);
        }
        Ok(Self {
            shared: global.as_ref().unwrap().clone(),
        })
    }

    pub fn open_session(&self, options: SessionOptions) -> Result<u64> {
        self.healthy()?;
        let id = self.shared.next.fetch_add(1, Ordering::Relaxed);
        self.shared.sessions.lock().unwrap().insert(
            id,
            Arc::new(SessionState {
                id,
                closed: AtomicBool::new(false),
                options,
            }),
        );
        Ok(id)
    }

    pub fn new_request(&self, session_id: u64, timeout_ms: u64) -> Result<CoreRequest> {
        self.healthy()?;
        if timeout_ms > u32::MAX as u64 {
            return Err(Failure::new("CONFIGURATION", "Timeout exceeds the supported range."));
        }
        let session = self
            .shared
            .sessions
            .lock()
            .unwrap()
            .get(&session_id)
            .cloned()
            .ok_or_else(|| Failure::new("CLOSED", "The desktop session is closed."))?;
        let timeout = if timeout_ms == 0 { 10_000 } else { timeout_ms };
        let deadline = Instant::now() + Duration::from_millis(timeout);
        Ok(CoreRequest {
            state: Arc::new(RequestState::new(session, deadline, timeout_ms == 0)),
        })
    }

    pub fn invoke(&self, request: &CoreRequest, params: InvokeParams) -> Result<CoreReply> {
        self.healthy()?;
        request.state.check()?;
        let operation = Operation::parse(&params.operation)?;
        if self.shared.queued.fetch_add(1, Ordering::AcqRel) >= 64 {
            self.shared.queued.fetch_sub(1, Ordering::AcqRel);
            return Err(Failure::new("BUSY", "The native request queue is full."));
        }
        let (sender, receiver) = mpsc::channel();
        let command = Command {
            request: request.state.clone(),
            operation,
            arguments: Arguments {
                window_id: params.window_id,
                capture_id: params.capture_id,
                window: params.window,
                text: params.text,
                control: params.control,
            },
            reply: sender,
            pending: (),
        };
        if self.shared.sender.send(command).is_err() {
            self.shared.queued.fetch_sub(1, Ordering::AcqRel);
            self.shared.poisoned.store(true, Ordering::Release);
            return Err(worker_stopped_failure());
        }
        self.shared.wait_for_reply(&request.state, receiver)
    }

    pub fn close_session(&self, session_id: u64) {
        if let Some(session) = self.shared.sessions.lock().unwrap().remove(&session_id) {
            session.closed.store(true, Ordering::Release);
        }
    }

    pub fn cancel_capture(&self, session_id: u64, capture_id: u64) {
        self.shared.captures_cancelled.lock().unwrap().insert((session_id, capture_id));
    }

    fn healthy(&self) -> Result<()> {
        if self.shared.poisoned.load(Ordering::Acquire) {
            Err(shared_engine_unavailable_failure())
        } else {
            Ok(())
        }
    }
}

impl Shared {
    fn wait_for_reply(&self, state: &RequestState, receiver: Receiver<Result<CoreReply>>) -> Result<CoreReply> {
        match receiver.recv_timeout(state.deadline.saturating_duration_since(Instant::now())) {
            Ok(result) => result,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let mutation_started = state.expire();
                if self.mark_stalled_call() {
                    Err(stalled_timeout_failure())
                } else if mutation_started {
                    Err(Failure::new("TIMEOUT", mutation_timeout_message()))
                } else {
                    Err(Failure::new("TIMEOUT", timeout_message()))
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                self.poisoned.store(true, Ordering::Release);
                Err(worker_stopped_failure())
            }
        }
    }

    fn new(sender: Sender<QueuedCommand>) -> Self {
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
        let Some(generation) = activity.overdue_generation() else {
            return false;
        };
        // Allow a bounded interval for the worker to acknowledge cancellation. The
        // same lock covers the final check and the admission of the next native call.
        let (activity, _) = self
            .native_finished
            .wait_timeout_while(activity, Duration::from_millis(100), |activity| {
                activity.overdue_generation() == Some(generation)
            })
            .unwrap();
        if activity.overdue_generation() != Some(generation) {
            return false;
        }
        self.poisoned.store(true, Ordering::Release);
        true
    }
}

mod coordinator;
mod platform;
#[cfg(windows)]
mod windows_platform;

#[cfg(test)]
mod tests;
