use super::*;
use std::cell::Cell;

#[derive(Default)]
struct FakePlatform {
    windows: Vec<(u32, &'static str)>,
    actions: usize,
    released: Cell<Option<u64>>,
}

impl Platform for FakePlatform {
    type Window = (u32, &'static str);
    type WindowKey = u32;
    type Provider = &'static str;

    fn candidates(&self, query: &WindowQuery) -> Result<Vec<Self::Window>> {
        Ok(self.windows.iter().copied()
            .filter(|(id, name)| query.matches(name, *id))
            .collect())
    }

    fn window_key(&self, window: &Self::Window) -> Self::WindowKey {
        window.0
    }

    fn is_window_alive(&self, window: &Self::Window) -> bool {
        self.windows.contains(window)
    }

    fn prepare_capture(&mut self, _: Arc<RequestState>) -> Result<Option<CapturePreparation>> {
        Ok(None)
    }

    fn resolve_provider(&mut self, request: Arc<RequestState>, _: Self::Window) -> ProviderSelection<Self::Provider> {
        let (sender, result) = mpsc::channel();
        sender.send(Ok("fake")).unwrap();
        ProviderSelection::Pending { result, until: request.deadline, fallback: "fallback" }
    }

    fn act(
        &mut self, provider: Self::Provider, request: Arc<RequestState>, _: u64, window: Self::Window,
        action: ControlAction, query: ControlQuery,
    ) -> Result<Receiver<Result<Option<ControlResult>>>> {
        request.begin_mutation()?;
        assert_eq!(provider, "fake");
        assert_eq!(window.0, 2);
        assert_eq!(query.name, "Aceptar");
        assert!(matches!(action, ControlAction::Click));
        self.actions += 1;
        let (sender, result) = mpsc::channel();
        sender.send(Ok(Some(ControlResult { text: None }))).unwrap();
        Ok(result)
    }

    fn release(&self, id: u64, provider: Self::Provider) {
        assert_eq!(provider, "fake");
        self.released.set(Some(id));
    }

    fn retain_sessions(&self, _: HashSet<u64>) {}

    fn check_health(&self) {}
}
fn session(id: u64) -> Arc<SessionState> {
    Arc::new(SessionState {
        id,
        closed: AtomicBool::new(false),
        options: SessionOptions::automatic(None, false).unwrap(),
    })
}

fn request(session: Arc<SessionState>, deadline: Instant) -> Arc<RequestState> {
    Arc::new(RequestState::new(session, deadline, false))
}

fn test_worker() -> Worker<FakePlatform> {
    let (sender, _receiver) = mpsc::channel();
    Worker::new(Arc::new(Shared::new(sender)), FakePlatform::default())
}

fn command(
    request: Arc<RequestState>, operation: Operation, arguments: Arguments,
) -> (ActiveCommand<&'static str>, Receiver<Result<Reply>>) {
    let (reply, receiver) = mpsc::channel();
    (Command { request, operation, arguments, reply, pending: None }, receiver)
}

#[test]
fn a_platform_adapter_can_capture_operate_and_release_a_window() {
    let mut worker = test_worker();
    let owner = session(1);
    worker.shared.sessions.lock().unwrap().insert(owner.id, owner.clone());
    let deadline = Instant::now() + Duration::from_secs(10);
    worker.platform.windows.push((1, "Opciones"));

    let (mut prepare, _) = command(request(owner.clone(), deadline), Operation::PrepareCapture, Arguments {
        window: WindowQuery { title: "Opciones".into(), ..WindowQuery::default() }, ..Arguments::default()
    });
    let capture_id = worker.step(&mut prepare).unwrap().unwrap().handle;

    worker.platform.windows.push((2, "Opciones"));
    let (mut capture, _) = command(request(owner.clone(), deadline), Operation::CompleteCapture, Arguments {
        capture_id, ..Arguments::default()
    });
    let window_id = worker.step(&mut capture).unwrap().unwrap().handle;

    let (mut click, _) = command(request(owner.clone(), deadline), Operation::Window(WindowOperation::Click), Arguments {
        window_id, control: ControlQuery { name: "Aceptar".into(), ..ControlQuery::default() }, ..Arguments::default()
    });
    assert!(worker.step(&mut click).unwrap().is_none());
    assert!(worker.step(&mut click).unwrap().is_none());
    assert_eq!(worker.step(&mut click).unwrap().unwrap().handle, window_id);
    assert_eq!(worker.platform.actions, 1);

    worker.platform.windows.pop();
    worker.cleanup();
    assert_eq!(worker.platform.released.get(), Some(window_id));
    let (mut closed, _) = command(request(owner, deadline), Operation::Window(WindowOperation::WaitClosed), Arguments {
        window_id, ..Arguments::default()
    });
    assert_eq!(worker.step(&mut closed).unwrap().unwrap().handle, window_id);
}

#[test]
fn title_prefix_respects_pid_and_rejects_ambiguous_matches() {
    let mut worker = test_worker();
    let state = request(session(1), Instant::now() + Duration::from_secs(1));
    worker.platform.windows = vec![(1, "Informe - Perfil A"), (2, "Informe - Perfil B")];
    let mut query = WindowQuery { title_prefix: "Informe".into(), ..WindowQuery::default() };

    let ambiguous = worker.discover(&state, &query, None).unwrap_err();
    assert_eq!(ambiguous.code, "AMBIGUOUS");
    query.pid = Some(2);
    let id = worker.discover(&state, &query, None).unwrap().unwrap();
    assert_eq!(worker.windows[&id].window, (2, "Informe - Perfil B"));

    query.title = "Informe".into();
    query.title_prefix.clear();
    assert!(worker.discover(&state, &query, None).unwrap().is_none());
}

#[test]
fn request_cancellation_and_closure_are_checked() {
    let session = session(1);
    let deadline = Instant::now() + Duration::from_secs(1);
    let cancelled = request(session.clone(), deadline);
    assert!(cancelled.check().is_ok());
    cancelled.cancel();
    assert_eq!(cancelled.check().unwrap_err().code, "CANCELLED");
    let closing = request(session.clone(), deadline);
    session.closed.store(true, Ordering::Release);
    assert_eq!(closing.check().unwrap_err().code, "CANCELLED");
}
#[test]
fn deadlines_are_monotonic() {
    let request = request(session(2), Instant::now() - Duration::from_millis(1));
    assert_eq!(request.check().unwrap_err().code, "TIMEOUT");
}

#[test]
fn timeouts_distinguish_started_mutations_from_requests_that_cannot_start() {
    let worker = test_worker();
    let owner = session(1);
    for mutation_started in [false, true] {
        let state = request(owner.clone(), Instant::now() + Duration::from_millis(100));
        if mutation_started {
            state.begin_mutation().unwrap();
        }
        let (_sender, receiver) = mpsc::channel();
        let error = worker.shared.wait_for_reply(&state, receiver).err().expect("request should time out");
        assert_eq!(error.code, "TIMEOUT");
        assert_eq!(error.message.contains("effect may be unknown"), mutation_started);
        assert_eq!(state.begin_mutation().unwrap_err().code, "TIMEOUT");
    }
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
        assert_eq!(queued.check().unwrap_err().code, "TIMEOUT");
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
    cancelled.cancel();
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
    let deadline = Instant::now() + Duration::from_secs(10);
    let abandoned = request(owner, deadline);
    let next = request(other, deadline);
    let (prepare, first_reply) = command(abandoned.clone(), Operation::PrepareCapture, Arguments {
        window: WindowQuery { title: "First queued capture".into(), ..WindowQuery::default() }, ..Arguments::default()
    });
    let mut pending = VecDeque::from([prepare]);
    worker.shared.queued.store(1, Ordering::Release);
    worker.process_pending(&mut pending);
    let previous_capture = first_reply.try_recv().unwrap().unwrap().handle;
    assert!(worker.captures.contains_key(&previous_capture));

    // Cancellation arrives after the preparation ran, before another command in the queue.
    abandoned.cancel();
    let (prepare, second_reply) = command(next, Operation::PrepareCapture, Arguments {
        window: WindowQuery { title: "Second queued capture".into(), ..WindowQuery::default() }, ..Arguments::default()
    });
    pending.push_back(prepare);
    worker.shared.queued.store(1, Ordering::Release);
    worker.process_pending(&mut pending);
    let capture = second_reply.try_recv().unwrap().expect("the next preparation can reserve the engine").handle;
    assert!(!worker.captures.contains_key(&previous_capture));
    assert!(worker.captures.contains_key(&capture));
}
