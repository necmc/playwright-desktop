use super::*;
use crate::{jab::{ffi::{JavaObject, VmId}, loader::tests::test_library}, windows};
use ::windows::{
    Win32::{
        Foundation::HWND,
        UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, WINDOW_EX_STYLE, WS_POPUP},
    },
    core::w,
};
use std::{cell::{Cell, RefCell}, rc::Rc};

fn session(id: u64) -> Arc<SessionState> {
    Arc::new(SessionState {
        id,
        closed: AtomicBool::new(false),
        options: SessionOptions::java("C:/test-java").unwrap(),
    })
}

fn request(session: Arc<SessionState>, deadline: Instant) -> Arc<RequestState> {
    Arc::new(RequestState { session, deadline, immediate: false, cancelled: AtomicBool::new(false) })
}

fn test_worker() -> Worker {
    let (sender, _receiver) = mpsc::channel();
    Worker::new(Arc::new(Shared::new(sender)))
}

fn command(
    request: Arc<RequestState>, operation: Operation, arguments: Arguments,
) -> (Command, Receiver<Result<Reply>>) {
    let (reply, receiver) = mpsc::channel();
    (Command { request, operation, arguments, reply }, receiver)
}

struct HiddenWindow(HWND);

impl HiddenWindow {
    fn new() -> Self {
        // SAFETY: STATIC is a system class and the window has no parent or callback data.
        let handle = unsafe {
            CreateWindowExW(WINDOW_EX_STYLE(0), w!("STATIC"), w!("JAB worker test"),
                           WS_POPUP, 0, 0, 1, 1, None, None, None, None)
        }.expect("create hidden window");
        Self(handle)
    }

    fn window(&self) -> windows::Window {
        let window = windows::Window {
            hwnd: self.0.0 as isize,
            pid: std::process::id(),
            title: "JAB worker test".into(),
        };
        assert!(windows::valid(&window));
        window
    }
}

impl Drop for HiddenWindow {
    fn drop(&mut self) {
        // SAFETY: this test thread created and owns the window.
        unsafe { let _ = DestroyWindow(self.0); }
    }
}

#[test]
fn request_cancellation_and_closure_are_checked() {
    let session = session(1);
    let request = request(session.clone(), Instant::now() + Duration::from_secs(1));
    assert!(request.check().is_ok());
    request.cancelled.store(true, Ordering::Release);
    assert_eq!(request.check().unwrap_err().code, "CANCELLED");
    request.cancelled.store(false, Ordering::Release);
    session.closed.store(true, Ordering::Release);
    assert_eq!(request.check().unwrap_err().code, "CANCELLED");
}
#[test]
fn deadlines_are_monotonic() {
    let request = request(session(2), Instant::now() - Duration::from_millis(1));
    assert_eq!(request.check().unwrap_err().code, "TIMEOUT");
}

#[test]
fn a_short_queued_deadline_does_not_poison_an_active_call_with_time_left() {
    let mut worker = test_worker();
    let shared = worker.shared.clone();
    let now = Instant::now();
    let owner = session(1);
    let active = request(owner.clone(), now + Duration::from_secs(10));
    let queued = request(owner, now + Duration::from_millis(25));
    let (queued_command, queued_reply) = command(queued.clone(), Operation::PrepareCapture, Arguments::default());
    let mut pending = VecDeque::from([queued_command]);
    shared.queued.store(1, Ordering::Release);
    shared.run_native(active.deadline, || {
        let error = shared.wait_for_reply(&queued, queued_reply).err().expect("queued request expired");
        assert_eq!(error.code, "TIMEOUT");
        assert!(queued.cancelled.load(Ordering::Acquire));
        assert!(active.check().is_ok());
        assert!(!shared.poisoned.load(Ordering::Acquire));
    }).unwrap();

    let (sender, receiver) = mpsc::channel();
    sender.send(Ok(Reply { handle: 7, text: None })).unwrap();
    assert_eq!(shared.wait_for_reply(&active, receiver).unwrap().handle, 7);
    worker.process_pending(&mut pending);
    assert!(pending.is_empty());
    assert_eq!(shared.queued.load(Ordering::Acquire), 0);
}

#[test]
fn pending_commands_with_expired_cancelled_or_closed_requests_are_discarded() {
    let mut worker = test_worker();
    let now = Instant::now();
    let live = session(1);
    let closed = session(2);
    worker.shared.sessions.lock().unwrap().insert(closed.id, closed.clone());
    let expired = request(live.clone(), now - Duration::from_millis(1));
    let cancelled = request(live.clone(), now + Duration::from_secs(10));
    let closing = request(closed.clone(), now + Duration::from_secs(10));
    let valid = request(live, now + Duration::from_secs(10));

    let mut pending = VecDeque::new();
    let mut replies = Vec::new();
    for state in [expired, cancelled.clone(), closing, valid] {
        let (command, reply) = command(
            state, Operation::Window(WindowOperation::Click),
            Arguments { window_id: 7, ..Arguments::default() },
        );
        pending.push_back(command);
        replies.push(reply);
    }
    worker.shared.queued.store(pending.len(), Ordering::Release);
    cancelled.cancelled.store(true, Ordering::Release);
    Engine { shared: worker.shared.clone() }.close_session(closed.id);
    worker.process_pending(&mut pending);

    for (reply, expected) in replies.into_iter().zip(["TIMEOUT", "CANCELLED", "CANCELLED", "STALE"]) {
        let error = reply.try_recv().unwrap().err().expect("request rejected");
        assert_eq!(error.code, expected);
    }
    assert!(pending.is_empty());
    assert_eq!(worker.shared.queued.load(Ordering::Acquire), 0);
}

#[test]
fn cancelling_a_queued_preparation_after_reservation_releases_it_before_the_next_command() {
    let mut worker = test_worker();
    let owner = session(1);
    let other = session(2);
    worker.backend = Backend::with_test_library(test_library(), [owner.id, other.id]);
    let deadline = Instant::now() + Duration::from_secs(10);
    let abandoned = request(owner, deadline);
    let next = request(other, deadline);
    let (prepare, first_reply) = command(abandoned.clone(), Operation::PrepareCapture, Arguments {
        title: "First queued capture".into(), ..Arguments::default()
    });
    let mut pending = VecDeque::from([prepare]);
    worker.shared.queued.store(1, Ordering::Release);
    worker.process_pending(&mut pending);
    let previous_capture = first_reply.try_recv().unwrap().unwrap().handle;
    assert_eq!(worker.lease.as_ref().unwrap().capture, previous_capture);

    // Cancellation arrives after the preparation ran, before another command in the queue.
    abandoned.cancelled.store(true, Ordering::Release);
    let (prepare, second_reply) = command(next, Operation::PrepareCapture, Arguments {
        title: "Second queued capture".into(), ..Arguments::default()
    });
    pending.push_back(prepare);
    worker.shared.queued.store(1, Ordering::Release);
    worker.process_pending(&mut pending);
    let capture = second_reply.try_recv().unwrap().expect("the next preparation can reserve the engine").handle;
    assert!(!worker.captures.contains_key(&previous_capture));
    assert_eq!(worker.lease.as_ref().unwrap().capture, capture);
}

#[test]
fn a_reused_hwnd_cannot_route_an_action_to_a_different_java_root() {
    assert_stale_root(4, "reused the handle", 2);
}

#[test]
fn an_unavailable_java_root_releases_the_window_on_the_worker_thread() {
    assert_stale_root(0, "no longer accessible", 1);
}

fn assert_stale_root(next_root: JavaObject, expected_error: &str, expected_releases: usize) {
    thread_local! {
        static ROOT: Cell<JavaObject> = const { Cell::new(1) };
        static JAB_THREADS: RefCell<Vec<(bool, std::thread::ThreadId)>> = const { RefCell::new(Vec::new()) };
    }

    unsafe extern "C" fn current_root(_: HWND, vm: *mut VmId, object: *mut JavaObject) -> i32 {
        JAB_THREADS.with_borrow_mut(|threads| threads.push((false, std::thread::current().id())));
        // SAFETY: Object::root passes writable out parameters.
        unsafe {
            *vm = 7;
            *object = ROOT.get();
        }
        1
    }

    extern "C" fn release(_: VmId, _: JavaObject) {
        JAB_THREADS.with_borrow_mut(|threads| threads.push((true, std::thread::current().id())));
    }

    let handle = HiddenWindow::new();
    let window = handle.window();

    let worker = std::thread::spawn(move || {
        let mut library = test_library();
        let bindings = Rc::get_mut(&mut library).unwrap();
        bindings.context = current_root;
        bindings.release = release;
        let mut worker = test_worker();
        worker.backend = Backend::with_test_library(library, []);
        let target = worker.backend.attach_test_window(window).unwrap().expect("original Java root");
        let owner = session(1);
        worker.windows.insert(2, BoundWindow { session: owner.id, target });
        worker.lease = Some(Lease { session: owner.id, capture: 0, window: 2 });
        let request = request(owner, Instant::now() + Duration::from_secs(10));
        let (action, result) = command(request, Operation::Window(WindowOperation::Click), Arguments {
            window_id: 2, role: "button".into(), name: "Confirmar".into(), ..Arguments::default()
        });

        ROOT.set(next_root);
        worker.shared.queued.store(1, Ordering::Release);
        worker.process_pending(&mut VecDeque::from([action]));
        let error = result.try_recv().unwrap().err().expect("reject the stale Java root");
        assert!(!worker.windows.contains_key(&2));
        assert!(worker.lease.is_none());
        let calls = JAB_THREADS.with_borrow_mut(std::mem::take);
        (error, calls)
    });
    let thread_id = worker.thread().id();
    let (error, calls) = worker.join().unwrap();
    assert_eq!(error.code, "STALE");
    assert!(error.message.contains(expected_error));
    assert_eq!(calls.iter().filter(|(released, _)| *released).count(), expected_releases);
    assert_eq!(calls.iter().filter(|(released, _)| !*released).count(), 2);
    assert!(calls.iter().all(|(_, id)| *id == thread_id));
}

#[test]
fn a_blocked_jab_call_keeps_the_engine_unavailable_after_it_resumes() {
    struct Gate {
        entered: Sender<()>,
        resume: Receiver<()>,
    }

    thread_local! {
        static GATE: RefCell<Option<Gate>> = const { RefCell::new(None) };
    }

    unsafe extern "C" fn blocking_context(_: HWND, vm: *mut VmId, object: *mut JavaObject) -> i32 {
        if let Some(gate) = GATE.with_borrow_mut(Option::take) {
            if gate.entered.send(()).is_err() || gate.resume.recv_timeout(Duration::from_secs(10)).is_err() {
                return 0;
            }
        }
        // SAFETY: Object::root passes writable out parameters.
        unsafe {
            *vm = 7;
            *object = 1;
        }
        1
    }

    let handle = HiddenWindow::new();
    let window = handle.window();
    let (sender, _commands) = mpsc::channel();
    let shared = Arc::new(Shared::new(sender));
    let engine = Engine { shared: shared.clone() };
    let owner = session(1);
    let worker_session = owner.clone();
    let worker_state = shared.clone();
    let (ready, worker_ready) = mpsc::channel();
    let (submit, batch) = mpsc::channel();
    let (entered, call_entered) = mpsc::channel();
    let (release, resume) = mpsc::channel();

    let worker = std::thread::spawn(move || {
        let mut library = test_library();
        Rc::get_mut(&mut library).unwrap().context = blocking_context;
        let mut worker = Worker::new(worker_state);
        worker.backend = Backend::with_test_library(library, []);
        let target = worker.backend.attach_test_window(window).unwrap().expect("initial Java root");
        worker.windows.insert(2, BoundWindow { session: worker_session.id, target });
        worker.lease = Some(Lease { session: worker_session.id, capture: 0, window: 2 });
        GATE.set(Some(Gate { entered, resume }));
        ready.send(()).unwrap();
        let mut pending = batch.recv_timeout(Duration::from_secs(5)).expect("test sends the request batch");
        worker.process_pending(&mut pending);
        assert!(pending.is_empty());
    });

    worker_ready.recv_timeout(Duration::from_secs(5)).expect("worker ready");
    let blocked = request(owner.clone(), Instant::now() + Duration::from_secs(1));
    let queued = request(owner, Instant::now() + Duration::from_secs(5));
    let (action, result) = command(blocked.clone(), Operation::Window(WindowOperation::ReadText), Arguments {
        window_id: 2, role: "textbox".into(), name: "Alias".into(), ..Arguments::default()
    });
    let (next, next_result) = command(queued, Operation::PrepareCapture, Arguments::default());
    shared.queued.store(2, Ordering::Release);
    assert!(submit.send(VecDeque::from([action, next])).is_ok());
    call_entered.recv_timeout(Duration::from_secs(5)).expect("worker entered the JAB callback");

    let timeout = shared.wait_for_reply(&blocked, result).err().expect("the JAB call exceeded its deadline");
    let before_release = engine.healthy();
    release.send(()).expect("release the blocked callback");
    worker.join().expect("worker completed after releasing the callback");

    assert_eq!(timeout.code, "TIMEOUT");
    assert!(timeout.message.contains("Restart Python"));
    assert_eq!(before_release.unwrap_err().code, "BRIDGE");
    let rejected = next_result.try_recv().unwrap().err().expect("already queued command is rejected");
    assert_eq!(rejected.code, "BRIDGE");
    let unavailable = engine.healthy().unwrap_err();
    assert_eq!(unavailable.code, "BRIDGE");
    assert!(unavailable.message.contains("Restart Python"));
    assert_eq!(shared.queued.load(Ordering::Acquire), 0);
}
