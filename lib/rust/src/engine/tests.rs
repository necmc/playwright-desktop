use super::{*, coordinator::Worker, platform::{CandidateSearch, ProviderSelection}};
use std::{cell::{Cell, RefCell}, collections::VecDeque};

#[derive(Default)]
struct FakePlatform {
    windows: Vec<(u32, &'static str)>,
    pending_windows: Option<Vec<(u32, &'static str)>>,
    actions: usize,
    released: Cell<Option<u64>>,
    brought_to_front: RefCell<Vec<u32>>,
    close_requests: RefCell<Vec<u32>>,
}

impl Platform for FakePlatform {
    type Window = (u32, &'static str);
    type WindowKey = u32;
    type Provider = &'static str;

    fn snapshot_windows(&self) -> Result<HashSet<Self::WindowKey>> {
        Ok(self.windows.iter().map(|window| self.window_key(window)).collect())
    }

    fn candidates(&self, query: &WindowQuery) -> Result<Vec<Self::Window>> {
        Ok(self.windows.iter().copied()
            .filter(|(id, name)| query.matches(name, *id))
            .collect())
    }

    fn find_candidates(
        &mut self, _request: Arc<RequestState>, query: &WindowQuery, exclude: &HashSet<Self::WindowKey>,
    ) -> Result<CandidateSearch<Self::Window>> {
        let filtered: Vec<_> = self.candidates(query)?
            .into_iter()
            .filter(|window| !exclude.contains(&self.window_key(window)))
            .collect();
        if query.has_text.is_empty() || filtered.is_empty() {
            return Ok(CandidateSearch::Ready(filtered));
        }
        if let Some(windows) = self.pending_windows.clone() {
            let pending_filtered: Vec<_> = windows
                .into_iter()
                .filter(|window| !exclude.contains(&self.window_key(window)))
                .collect();
            let (sender, receiver) = mpsc::channel();
            sender.send(Ok(pending_filtered)).unwrap();
            return Ok(CandidateSearch::Pending(receiver));
        }
        Ok(CandidateSearch::Ready(filtered))
    }

    fn window_key(&self, window: &Self::Window) -> Self::WindowKey {
        window.0
    }

    fn window_metadata(&self, window: &Self::Window) -> (u32, String) {
        (window.0, window.1.to_string())
    }

    fn is_window_alive(&self, window: &Self::Window) -> bool {
        self.windows.contains(window)
    }

    fn bring_to_front(&self, window: &Self::Window) -> Result<()> {
        if !self.is_window_alive(window) {
            return Err(Failure::new("STALE", "The native window no longer exists."));
        }
        self.brought_to_front.borrow_mut().push(window.0);
        Ok(())
    }

    fn post_close(&self, window: &Self::Window) -> Result<bool> {
        if !self.is_window_alive(window) {
            return Ok(false);
        }
        self.close_requests.borrow_mut().push(window.0);
        Ok(true)
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
        config: SessionConfig::automatic(),
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
) -> (ActiveCommand<FakePlatform>, Receiver<Result<CoreReply>>) {
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

    let ambiguous = worker.select_candidate(&state, &query, worker.platform.candidates(&query).unwrap(), None).unwrap_err();
    assert_eq!(ambiguous.code, "AMBIGUOUS");
    query.pid = Some(2);
    let id = worker.select_candidate(&state, &query, worker.platform.candidates(&query).unwrap(), None).unwrap().unwrap();
    assert_eq!(worker.windows[&id].window, (2, "Informe - Perfil B"));

    query.title = "Informe".into();
    query.title_prefix.clear();
    assert!(worker.select_candidate(&state, &query, worker.platform.candidates(&query).unwrap(), None).unwrap().is_none());
}

#[test]
fn pending_has_text_discovery_handles_untitled_windows_and_ambiguity() {
    let mut worker = test_worker();
    let owner = session(1);
    worker.shared.sessions.lock().unwrap().insert(owner.id, owner.clone());
    let deadline = Instant::now() + Duration::from_secs(10);

    let untitled_query = WindowQuery { has_text: "Windows protegió su PC".into(), ..WindowQuery::default() };
    assert!(untitled_query.allows_untitled());
    assert!(untitled_query.matches("", 11520));

    worker.platform.windows.push((2, ""));
    worker.platform.pending_windows = Some(vec![(2, "")]);
    let (mut click, _) = command(request(owner.clone(), deadline), Operation::Window(WindowOperation::Click), Arguments {
        window: untitled_query.clone(),
        control: ControlQuery { name: "Aceptar".into(), ..ControlQuery::default() },
        ..Arguments::default()
    });
    assert!(worker.step(&mut click).unwrap().is_none());
    assert!(worker.step(&mut click).unwrap().is_none());
    assert!(worker.step(&mut click).unwrap().is_none());
    assert!(worker.step(&mut click).unwrap().unwrap().handle > 0);
    assert_eq!(worker.platform.actions, 1);

    worker.platform.pending_windows = Some(vec![(2, ""), (3, "")]);
    let (mut ambiguous, _) = command(request(owner, deadline), Operation::Window(WindowOperation::Click), Arguments {
        window: untitled_query,
        control: ControlQuery { name: "Aceptar".into(), ..ControlQuery::default() },
        ..Arguments::default()
    });
    assert!(worker.step(&mut ambiguous).unwrap().is_none());
    assert_eq!(worker.step(&mut ambiguous).err().unwrap().code, "AMBIGUOUS");
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
    sender.send(Ok(CoreReply::handle(7))).unwrap();
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
    CoreEngine { shared: worker.shared.clone() }.close_session(closed.id);
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

#[test]
fn same_session_can_hold_multiple_handles_with_metadata_while_blocking_other_sessions() {
    let mut worker = test_worker();
    let owner = session(1);
    let other = session(2);
    worker.shared.sessions.lock().unwrap().insert(owner.id, owner.clone());
    worker.shared.sessions.lock().unwrap().insert(other.id, other.clone());
    let deadline = Instant::now() + Duration::from_secs(10);

    // Captura 1: ventana principal de una App.
    let (mut prep_a, _) = command(request(owner.clone(), deadline), Operation::PrepareCapture, Arguments {
        window: WindowQuery::default(), ..Arguments::default()
    });
    let cap_a = worker.step(&mut prep_a).unwrap().unwrap().handle;
    worker.platform.windows.push((101, "Launcher"));
    let (mut comp_a, _) = command(request(owner.clone(), deadline), Operation::CompleteCapture, Arguments {
        capture_id: cap_a, ..Arguments::default()
    });
    let reply_a = worker.step(&mut comp_a).unwrap().unwrap();
    assert_eq!(reply_a.pid, Some(101));
    assert_eq!(reply_a.title.as_deref(), Some("Launcher"));

    // Captura 2 dentro de la misma sesión: otra ventana/app coexiste con la primera.
    let (mut prep_b, _) = command(request(owner.clone(), deadline), Operation::PrepareCapture, Arguments {
        window: WindowQuery { title: "Alerta".into(), ..WindowQuery::default() }, ..Arguments::default()
    });
    let cap_b = worker.step(&mut prep_b).unwrap().unwrap().handle;
    worker.platform.windows.push((2, "Alerta"));
    let (mut comp_b, _) = command(request(owner.clone(), deadline), Operation::CompleteCapture, Arguments {
        capture_id: cap_b, ..Arguments::default()
    });
    let reply_b = worker.step(&mut comp_b).unwrap().unwrap();
    assert_ne!(reply_a.handle, reply_b.handle);
    assert_eq!(reply_b.pid, Some(2));
    assert_eq!(reply_b.title.as_deref(), Some("Alerta"));

    // Una acción UIA sobre un handle capturado también conserva pid y title en Reply.
    let (mut action_b, _) = command(request(owner.clone(), deadline), Operation::Window(WindowOperation::Click), Arguments {
        window_id: reply_b.handle,
        control: ControlQuery { name: "Aceptar".into(), ..ControlQuery::default() },
        ..Arguments::default()
    });
    assert!(worker.step(&mut action_b).unwrap().is_none());
    assert!(worker.step(&mut action_b).unwrap().is_none());
    let action_reply = worker.step(&mut action_b).unwrap().unwrap();
    assert_eq!(action_reply.handle, reply_b.handle);
    assert_eq!(action_reply.pid, Some(2));
    assert_eq!(action_reply.title.as_deref(), Some("Alerta"));

    // La búsqueda directa sin filtros (no captura) sigue exigiendo criterio en validate().
    assert_eq!(WindowQuery::default().validate().unwrap_err().code, "CONFIGURATION");
    let conflict = WindowQuery { title: "A".into(), title_prefix: "B".into(), ..WindowQuery::default() };
    assert_eq!(conflict.validate_capture().unwrap_err().code, "CONFIGURATION");

    // Otra sesión distinta queda bloqueada con BUSY mientras owner tenga ventanas activas.
    let (mut blocked, _) = command(request(other.clone(), deadline), Operation::PrepareCapture, Arguments {
        window: WindowQuery { title: "Otra".into(), ..WindowQuery::default() }, ..Arguments::default()
    });
    assert_eq!(worker.step(&mut blocked).unwrap_err().code, "BUSY");

    // Se cierra solo la segunda ventana; la primera permanece operativa en la misma sesión.
    worker.platform.windows.retain(|(id, _)| *id != 2);
    worker.cleanup();
    let (mut click_b, _) = command(request(owner.clone(), deadline), Operation::Window(WindowOperation::Click), Arguments {
        window_id: reply_b.handle, control: ControlQuery { name: "Aceptar".into(), ..ControlQuery::default() }, ..Arguments::default()
    });
    assert_eq!(worker.step(&mut click_b).unwrap_err().code, "STALE");
    assert!(worker.windows.contains_key(&reply_a.handle));

    // Al cerrar la sesión dueña, se liberan todos sus handles y otra sesión puede operar.
    CoreEngine { shared: worker.shared.clone() }.close_session(owner.id);
    worker.cleanup();
    assert!(worker.windows.is_empty());
    let (mut unblocked, _) = command(request(other, deadline), Operation::PrepareCapture, Arguments {
        window: WindowQuery { title: "Otra".into(), ..WindowQuery::default() }, ..Arguments::default()
    });
    assert!(worker.step(&mut unblocked).unwrap().is_some());
}

#[test]
fn bring_to_front_and_close_window_coordinate_lifecycle_without_duplicate_close_signals() {
    assert!(matches!(Operation::parse("bring_to_front"), Ok(Operation::Window(WindowOperation::BringToFront))));
    assert!(matches!(Operation::parse("close_window"), Ok(Operation::Window(WindowOperation::CloseWindow))));

    let mut worker = test_worker();
    let owner = session(1);
    worker.shared.sessions.lock().unwrap().insert(owner.id, owner.clone());
    let deadline = Instant::now() + Duration::from_secs(10);

    let (mut prep, _) = command(request(owner.clone(), deadline), Operation::PrepareCapture, Arguments {
        window: WindowQuery::default(), ..Arguments::default()
    });
    let cap = worker.step(&mut prep).unwrap().unwrap().handle;
    worker.platform.windows.push((101, "App Principal"));
    let (mut comp, _) = command(request(owner.clone(), deadline), Operation::CompleteCapture, Arguments {
        capture_id: cap, ..Arguments::default()
    });
    let app = worker.step(&mut comp).unwrap().unwrap();

    // bring_to_front activa el handle vivo y conserva metadatos en Reply.
    let (mut front, _) = command(request(owner.clone(), deadline), Operation::Window(WindowOperation::BringToFront), Arguments {
        window_id: app.handle, ..Arguments::default()
    });
    let front_reply = worker.step(&mut front).unwrap().unwrap();
    assert_eq!(front_reply.handle, app.handle);
    assert_eq!(front_reply.pid, Some(101));
    assert_eq!(*worker.platform.brought_to_front.borrow(), vec![101]);

    // close_window envía WM_CLOSE una sola vez y espera cooperativamente hasta que la ventana desaparezca.
    let (mut close, _) = command(request(owner.clone(), deadline), Operation::Window(WindowOperation::CloseWindow), Arguments {
        window_id: app.handle, ..Arguments::default()
    });
    assert!(worker.step(&mut close).unwrap().is_none());
    assert!(worker.step(&mut close).unwrap().is_none());
    assert_eq!(*worker.platform.close_requests.borrow(), vec![101]);

    worker.platform.windows.retain(|(id, _)| *id != 101);
    let closed_reply = worker.step(&mut close).unwrap().unwrap();
    assert_eq!(closed_reply.handle, app.handle);
    assert!(!worker.windows.contains_key(&app.handle));

    // Llamar close_window otra vez sobre el mismo handle ya cerrado es idempotente.
    let (mut close_again, _) = command(request(owner.clone(), deadline), Operation::Window(WindowOperation::CloseWindow), Arguments {
        window_id: app.handle, ..Arguments::default()
    });
    assert_eq!(worker.step(&mut close_again).unwrap().unwrap().handle, app.handle);
    assert_eq!(*worker.platform.close_requests.borrow(), vec![101]);

    // En cambio bring_to_front sobre el handle cerrado o close_window de un handle desconocido falla con STALE.
    let (mut stale_front, _) = command(request(owner.clone(), deadline), Operation::Window(WindowOperation::BringToFront), Arguments {
        window_id: app.handle, ..Arguments::default()
    });
    assert_eq!(worker.step(&mut stale_front).unwrap_err().code, "STALE");

    let (mut unknown_close, _) = command(request(owner, deadline), Operation::Window(WindowOperation::CloseWindow), Arguments {
        window_id: 99999, ..Arguments::default()
    });
    assert_eq!(worker.step(&mut unknown_close).unwrap_err().code, "STALE");
}

#[test]
fn only_new_ignores_windows_present_when_session_baseline_was_recorded() {
    let mut worker = test_worker();
    let owner = session(1);
    worker.shared.sessions.lock().unwrap().insert(owner.id, owner.clone());
    let deadline = Instant::now() + Duration::from_secs(10);

    // Ventana vieja previa a iniciar la sesión desktop.
    worker.platform.windows.push((1, "Aviso"));

    let (mut prep, _) = command(request(owner.clone(), deadline), Operation::PrepareCapture, Arguments {
        window: WindowQuery { title: "Destino".into(), ..WindowQuery::default() }, ..Arguments::default()
    });
    assert!(worker.step(&mut prep).unwrap().is_some());

    // Aparece una nueva ventana con el mismo título después del baseline.
    worker.platform.windows.push((2, "Aviso"));

    // Con only_new=true descarta la ventana (1, "Aviso") del baseline y opera sobre (2, "Aviso").
    let (mut click_new, _) = command(request(owner.clone(), deadline), Operation::Window(WindowOperation::Click), Arguments {
        window: WindowQuery { title: "Aviso".into(), only_new: true, ..WindowQuery::default() },
        control: ControlQuery { name: "Aceptar".into(), ..ControlQuery::default() },
        ..Arguments::default()
    });
    assert!(worker.step(&mut click_new).unwrap().is_none());
    assert!(worker.step(&mut click_new).unwrap().is_none());
    let reply = worker.step(&mut click_new).unwrap().unwrap();
    assert_eq!(reply.pid, Some(2));
    assert_eq!(worker.platform.actions, 1);

    // Con only_new=false ambas ventanas coinciden y produce AMBIGUOUS.
    let (mut click_all, _) = command(request(owner.clone(), deadline), Operation::Window(WindowOperation::Click), Arguments {
        window: WindowQuery { title: "Aviso".into(), only_new: false, ..WindowQuery::default() },
        control: ControlQuery { name: "Aceptar".into(), ..ControlQuery::default() },
        ..Arguments::default()
    });
    assert_eq!(worker.step(&mut click_all).unwrap_err().code, "AMBIGUOUS");

    CoreEngine { shared: worker.shared.clone() }.close_session(owner.id);
    worker.cleanup();
    assert!(!worker.baselines.contains_key(&owner.id));
}

#[test]
fn complete_capture_supports_has_text_and_excludes_before_windows() {
    let mut worker = test_worker();
    let owner = session(1);
    worker.shared.sessions.lock().unwrap().insert(owner.id, owner.clone());
    let deadline = Instant::now() + Duration::from_secs(10);

    // Ventana sin título ya abierta antes de PrepareCapture.
    worker.platform.windows.push((1, ""));
    let query = WindowQuery { has_text: "Windows protegió su PC".into(), ..WindowQuery::default() };
    let (mut prep, _) = command(request(owner.clone(), deadline), Operation::PrepareCapture, Arguments {
        window: query, ..Arguments::default()
    });
    let capture_id = worker.step(&mut prep).unwrap().unwrap().handle;

    // Aparece una segunda ventana sin título (2, "") y find_candidates (Pending) reporta ambas.
    worker.platform.windows.push((2, ""));
    worker.platform.pending_windows = Some(vec![(1, ""), (2, "")]);

    let (mut capture, _) = command(request(owner, deadline), Operation::CompleteCapture, Arguments {
        capture_id, ..Arguments::default()
    });
    assert!(worker.step(&mut capture).unwrap().is_none());
    let reply = worker.step(&mut capture).unwrap().unwrap();
    assert_eq!(reply.pid, Some(2));
}

#[test]
fn core_engine_error_messages_do_not_reference_client_runtime() {
    let failures = [
        crate::engine::worker_stopped_failure(),
        crate::engine::engine_unavailable_failure(),
        crate::engine::shared_engine_unavailable_failure(),
        crate::engine::stalled_timeout_failure(),
    ];
    for failure in failures {
        assert!(!failure.message.contains("Python"), "unexpected client name in: {}", failure.message);
        assert!(
            failure.message.to_ascii_lowercase().contains("host process"),
            "missing host process guidance in: {}",
            failure.message
        );
    }
}

#[test]
fn invoke_params_rejects_unknown_operation_without_python_context() {
    assert_eq!(Operation::parse("unsupported").err().unwrap().code, "CONFIGURATION");
    let params = crate::engine::InvokeParams::default();
    assert!(params.operation.is_empty());
    assert_eq!(params.window_id, 0);
}

#[test]
fn session_config_validates_non_empty_and_nul_free_options() {
    use crate::config::{SelectionMode, SessionConfig};

    let auto = SessionConfig::automatic();
    assert_eq!(auto.mode, SelectionMode::Automatic);
    assert!(auto.option("java.home").is_none());

    let mut explicit = SessionConfig::explicit_provider("jab");
    explicit.set_option("java.home", r"C:\jdk").unwrap();
    explicit.set_option("java.explicit", "true").unwrap();
    assert_eq!(explicit.mode, SelectionMode::ExplicitProvider("jab"));
    assert_eq!(explicit.option("java.home"), Some(r"C:\jdk"));
    assert!(explicit.flag("java.explicit"));

    assert_eq!(
        SessionConfig::automatic().set_option("", "val").unwrap_err().code,
        "CONFIGURATION"
    );
    assert_eq!(
        SessionConfig::automatic().set_option("java.home", "a\0b").unwrap_err().code,
        "CONFIGURATION"
    );
    assert_eq!(
        SessionConfig::automatic().set_option("a\0k", "val").unwrap_err().code,
        "CONFIGURATION"
    );
}

#[cfg(windows)]
#[test]
fn host_os_language_returns_non_empty_bcp47_tag() {
    let direct = crate::windows::host_os_language();
    let via_engine = crate::engine::host_os_language().expect("windows host_os_language should succeed");
    assert!(!direct.is_empty());
    assert!(!direct.contains('_'));
    assert!(!direct.contains('\0'));
    assert_eq!(direct, via_engine);
}
