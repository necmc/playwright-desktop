use super::{
    Arguments, ActiveCommand, CoreReply, Operation, PendingCall, QueuedCommand,
    RequestState, Shared, WindowOperation, engine_unavailable_failure,
};
use super::platform::{
    CandidateSearch, ControlAction, Platform, ProviderSelection, WindowQuery,
};
use crate::error::{Failure, Result};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{Arc, atomic::Ordering, mpsc::{self, Receiver}},
    time::{Duration, Instant},
};

const TIMEOUT_MESSAGE: &str =
    "No unique desktop window or control became ready before the deadline. Check its title, PID and accessibility provider.";
const MUTATION_TIMEOUT_MESSAGE: &str =
    "The desktop action timed out after entering the native operation; its effect may be unknown.";

pub(super) struct Capture<K> {
    session: u64,
    query: WindowQuery,
    before: HashSet<K>,
    request: Arc<RequestState>,
}

pub(super) struct BoundWindow<W, P> {
    session: u64,
    pub(super) window: W,
    capture: Option<u64>,
    provider: Option<P>,
}

pub(super) struct Worker<P: Platform> {
    pub(super) shared: Arc<Shared>,
    pub(super) platform: P,
    pub(super) captures: HashMap<u64, Capture<P::WindowKey>>,
    pub(super) baselines: HashMap<u64, HashSet<P::WindowKey>>,
    pub(super) windows: HashMap<u64, BoundWindow<P::Window, P::Provider>>,
    closed: HashMap<u64, u64>,
    live: HashSet<u64>,
}

impl<P: Platform> Worker<P> {
    pub(super) fn new(shared: Arc<Shared>, platform: P) -> Self {
        Self {
            shared,
            platform,
            captures: HashMap::new(),
            baselines: HashMap::new(),
            windows: HashMap::new(),
            closed: HashMap::new(),
            live: HashSet::new(),
        }
    }

    pub(super) fn run(mut self, receiver: Receiver<QueuedCommand>) {
        let shared = self.shared.clone();
        let mut pending = VecDeque::new();
        loop {
            if let Ok(command) = receiver.recv_timeout(Duration::from_millis(if pending.is_empty() { 15 } else { 5 })) {
                pending.push_back(command.activate());
            }
            pending.extend(receiver.try_iter().take(64).map(QueuedCommand::activate));
            let _ = shared.run_native(Instant::now() + Duration::from_secs(10), || self.cleanup());
            self.process_pending(&mut pending);
        }
    }

    pub(super) fn process_pending(&mut self, pending: &mut VecDeque<ActiveCommand<P>>) {
        let shared = self.shared.clone();
        for _ in 0..pending.len() {
            let mut command = pending.pop_front().unwrap();
            let result = if self.shared.poisoned.load(Ordering::Acquire) {
                Err(engine_unavailable_failure())
            } else if let Err(error) = command.request.check() {
                Err(error)
            } else {
                shared
                    .run_native(command.request.deadline, || self.step(&mut command))
                    .unwrap_or_else(|| Err(engine_unavailable_failure()))
            };
            match result {
                Ok(None) if command.pending.is_some() || !command.request.immediate => pending.push_back(command),
                Ok(None) => self.complete(
                    command,
                    Err(Failure::new("TIMEOUT", "The requested desktop window or control is not ready (timeout=0).")),
                ),
                Ok(Some(reply)) => self.complete(command, Ok(reply)),
                Err(error) => self.complete(command, Err(error)),
            }
        }
    }

    fn complete(&mut self, command: ActiveCommand<P>, result: Result<CoreReply>) {
        if result.is_err() && matches!(command.operation, Operation::CompleteCapture) {
            self.cancel(command.request.session.id, command.arguments.capture_id);
        }
        self.shared.queued.fetch_sub(1, Ordering::AcqRel);
        let _ = command.reply.send(result);
    }

    pub(super) fn cleanup(&mut self) {
        self.platform.check_health();
        let live: HashSet<u64> = self.shared.sessions.lock().unwrap().keys().copied().collect();
        let cancellations = std::mem::take(&mut *self.shared.captures_cancelled.lock().unwrap());
        for (session, capture) in cancellations {
            self.cancel(session, capture);
        }
        self.discard_abandoned_captures();
        self.captures.retain(|_, capture| live.contains(&capture.session));
        self.baselines.retain(|session, _| live.contains(session));
        self.closed.retain(|_, session| live.contains(session));

        let removed: Vec<u64> = self
            .windows
            .iter()
            .filter(|(_, item)| !live.contains(&item.session) || !self.platform.is_window_alive(&item.window))
            .map(|(id, _)| *id)
            .collect();
        for id in removed {
            self.release_window(id, &live);
        }
        if live != self.live {
            self.platform.retain_sessions(live.clone());
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
        let owned = self
            .windows
            .iter()
            .find(|(_, window)| window.session == session && window.capture == Some(capture_id))
            .map(|(id, _)| *id);
        if let Some(id) = owned {
            self.release_window(id, &HashSet::new());
        }
    }

    fn require_owner(&self, session: u64) -> Result<()> {
        if self.windows.values().any(|window| window.session != session)
            || self.captures.values().any(|capture| capture.session != session)
        {
            return Err(Failure::new("BUSY", "Another desktop flow is active. Close its window or desktop session first."));
        }
        Ok(())
    }

    pub(super) fn select_candidate(
        &mut self,
        request: &RequestState,
        query: &WindowQuery,
        candidates: Vec<P::Window>,
        capture: Option<(u64, &HashSet<P::WindowKey>)>,
    ) -> Result<Option<u64>> {
        let session = request.session.id;
        self.require_owner(session)?;
        if capture.is_some() {
            query.validate_capture()?;
        } else {
            query.validate()?;
        }
        let mut candidates = candidates
            .into_iter()
            .filter(|window| !capture.is_some_and(|(_, before)| before.contains(&self.platform.window_key(window))));
        let Some(candidate) = candidates.next() else {
            return Ok(None);
        };
        if candidates.next().is_some() {
            let filter = if query.title.is_empty() { "an exact title" } else { "a PID" };
            return Err(Failure::new("AMBIGUOUS", format!("More than one window matches. Add {filter} to narrow the search.")));
        }
        request.check()?;
        let key = self.platform.window_key(&candidate);
        if let Some((&id, _)) = self.windows.iter().find(|(_, item)| self.platform.window_key(&item.window) == key) {
            return Ok(Some(id));
        }
        let id = self.shared.next.fetch_add(1, Ordering::Relaxed);
        self.windows.insert(
            id,
            BoundWindow {
                session,
                window: candidate,
                capture: capture.map(|(id, _)| id),
                provider: None,
            },
        );
        Ok(Some(id))
    }

    pub(super) fn step(&mut self, command: &mut ActiveCommand<P>) -> Result<Option<CoreReply>> {
        command.request.check()?;
        self.discard_abandoned_captures();
        if let Some(pending) = command.pending.take() {
            return self.resume(command, pending);
        }
        match command.operation {
            Operation::PrepareCapture => {
                self.require_owner(command.request.session.id)?;
                if let Some(preparation) = self.platform.prepare_capture(command.request.clone())? {
                    command.pending = Some(PendingCall::PrepareCapture(preparation));
                    return Ok(None);
                }
                self.prepare_capture(&command.request, &command.arguments)
            }
            Operation::CompleteCapture => self.complete_capture(command, command.arguments.capture_id),
            Operation::Window(operation) => self.operate_window(command, operation),
        }
    }

    fn resume(&mut self, command: &mut ActiveCommand<P>, pending: PendingCall<P>) -> Result<Option<CoreReply>> {
        match pending {
            PendingCall::PrepareCapture(preparation) => {
                match receive(&preparation.result) {
                    Ok(Some(response)) => response?,
                    Ok(None) if preparation.until.is_none_or(|deadline| Instant::now() < deadline) => {
                        command.pending = Some(PendingCall::PrepareCapture(preparation));
                        return Ok(None);
                    }
                    Err(error) if preparation.until.is_none() => return Err(error),
                    _ => {}
                }
                self.prepare_capture(&command.request, &command.arguments)
            }
            PendingCall::CompleteCapture { capture_id, result } => {
                let Some(response) = receive(&result)? else {
                    command.pending = Some(PendingCall::CompleteCapture { capture_id, result });
                    return Ok(None);
                };
                self.finish_capture(&command.request, capture_id, response?)
            }
            PendingCall::Discover { operation, result } => {
                let Some(response) = receive(&result)? else {
                    command.pending = Some(PendingCall::Discover { operation, result });
                    return Ok(None);
                };
                let candidates = response?;
                let Some(id) = self.select_candidate(&command.request, &command.arguments.window, candidates, None)? else {
                    return Ok(None);
                };
                self.act_on_window(command, id, operation)
            }
            PendingCall::ResolveProvider { id, result, until, fallback } => {
                let provider = match receive(&result) {
                    Ok(Some(response)) => response?,
                    Err(_) => fallback,
                    Ok(None) if Instant::now() >= until => fallback,
                    Ok(None) => {
                        command.pending = Some(PendingCall::ResolveProvider { id, result, until, fallback });
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
                match response {
                    Ok(Some(result)) => Ok(Some(self.reply_for_window(id, result.text))),
                    Ok(None) => Ok(None),
                    Err(error) => {
                        if error.code == "STALE" {
                            self.invalidate_window(id);
                        }
                        Err(error)
                    }
                }
            }
            PendingCall::Closing { id } => self.poll_closing(command, id),
        }
    }

    fn reply_for_window(&self, id: u64, text: Option<String>) -> CoreReply {
        if let Some(entry) = self.windows.get(&id) {
            let (pid, title) = self.platform.window_metadata(&entry.window);
            return CoreReply {
                handle: id,
                text,
                pid: Some(pid),
                title: Some(title),
            };
        }
        CoreReply {
            handle: id,
            text,
            pid: None,
            title: None,
        }
    }

    fn ensure_baseline(&mut self, session: u64) -> Result<()> {
        if !self.baselines.contains_key(&session) {
            self.baselines.insert(session, self.platform.snapshot_windows()?);
        }
        Ok(())
    }

    fn discovery_exclude(&self, session: u64, query: &WindowQuery) -> HashSet<P::WindowKey> {
        if query.only_new {
            self.baselines.get(&session).cloned().unwrap_or_default()
        } else {
            HashSet::new()
        }
    }

    fn prepare_capture(&mut self, request: &Arc<RequestState>, args: &Arguments) -> Result<Option<CoreReply>> {
        let session = request.session.id;
        self.require_owner(session)?;
        args.window.validate_capture()?;
        self.ensure_baseline(session)?;
        let before = self
            .platform
            .candidates(&args.window)?
            .iter()
            .map(|window| self.platform.window_key(window))
            .collect();
        request.check()?;
        let id = self.shared.next.fetch_add(1, Ordering::Relaxed);
        self.captures.insert(
            id,
            Capture {
                session,
                query: args.window.clone(),
                before,
                request: request.clone(),
            },
        );
        Ok(Some(CoreReply::handle(id)))
    }

    fn complete_capture(&mut self, command: &mut ActiveCommand<P>, capture_id: u64) -> Result<Option<CoreReply>> {
        let capture = self
            .captures
            .get(&capture_id)
            .filter(|capture| capture.session == command.request.session.id)
            .ok_or_else(|| Failure::new("CLOSED", "The window capture is no longer active."))?;
        let query = capture.query.clone();
        let before = capture.before.clone();
        match self.platform.find_candidates(command.request.clone(), &query, &before)? {
            CandidateSearch::Ready(candidates) => self.finish_capture(&command.request, capture_id, candidates),
            CandidateSearch::Pending(result) => {
                command.pending = Some(PendingCall::CompleteCapture { capture_id, result });
                Ok(None)
            }
        }
    }

    fn finish_capture(&mut self, request: &RequestState, capture_id: u64, candidates: Vec<P::Window>) -> Result<Option<CoreReply>> {
        let capture = self
            .captures
            .get(&capture_id)
            .filter(|capture| capture.session == request.session.id)
            .ok_or_else(|| Failure::new("CLOSED", "The window capture is no longer active."))?;
        let query = capture.query.clone();
        let before = capture.before.clone();
        let Some(id) = self.select_candidate(request, &query, candidates, Some((capture_id, &before)))? else {
            return Ok(None);
        };
        self.captures.remove(&capture_id);
        Ok(Some(self.reply_for_window(id, None)))
    }

    fn operate_window(&mut self, command: &mut ActiveCommand<P>, operation: WindowOperation) -> Result<Option<CoreReply>> {
        if command.arguments.window_id != 0 {
            return self.act_on_window(command, command.arguments.window_id, operation);
        }
        let session = command.request.session.id;
        self.require_owner(session)?;
        command.arguments.window.validate()?;
        self.ensure_baseline(session)?;
        let exclude = self.discovery_exclude(session, &command.arguments.window);
        match self.platform.find_candidates(command.request.clone(), &command.arguments.window, &exclude)? {
            CandidateSearch::Ready(candidates) => {
                let Some(id) = self.select_candidate(&command.request, &command.arguments.window, candidates, None)? else {
                    return Ok(None);
                };
                self.act_on_window(command, id, operation)
            }
            CandidateSearch::Pending(result) => {
                command.pending = Some(PendingCall::Discover { operation, result });
                Ok(None)
            }
        }
    }

    fn act_on_window(&mut self, command: &mut ActiveCommand<P>, id: u64, operation: WindowOperation) -> Result<Option<CoreReply>> {
        let request = &command.request;
        let session = request.session.id;
        let tolerates_closed = matches!(operation, WindowOperation::WaitClosed | WindowOperation::CloseWindow);
        if tolerates_closed && self.closed.get(&id) == Some(&session) {
            return Ok(Some(CoreReply::handle(id)));
        }
        let item = self.window(id, session)?;
        if !self.platform.is_window_alive(&item.window) {
            self.invalidate_window(id);
            if tolerates_closed {
                return Ok(Some(CoreReply::handle(id)));
            }
            return Err(Failure::new("STALE", "The native window no longer exists."));
        }
        match operation {
            WindowOperation::WaitClosed => return Ok(None),
            WindowOperation::BringToFront => {
                self.platform.bring_to_front(&item.window)?;
                return Ok(Some(self.reply_for_window(id, None)));
            }
            WindowOperation::CloseWindow => return self.begin_close(command, id),
            _ => {}
        }
        if let Some(provider) = item.provider {
            return self.dispatch(command, id, provider);
        }
        let window = item.window.clone();
        match self.platform.resolve_provider(request.clone(), window) {
            ProviderSelection::Ready(provider) => self.dispatch(command, id, provider),
            ProviderSelection::Pending { result, until, fallback } => {
                command.pending = Some(PendingCall::ResolveProvider { id, result, until, fallback });
                Ok(None)
            }
        }
    }

    fn begin_close(&mut self, command: &mut ActiveCommand<P>, id: u64) -> Result<Option<CoreReply>> {
        command.request.begin_mutation()?;
        let session = command.request.session.id;
        let window = self.window(id, session)?.window.clone();
        if !self.platform.post_close(&window)? {
            self.invalidate_window(id);
            return Ok(Some(CoreReply::handle(id)));
        }
        command.pending = Some(PendingCall::Closing { id });
        Ok(None)
    }

    fn poll_closing(&mut self, command: &mut ActiveCommand<P>, id: u64) -> Result<Option<CoreReply>> {
        let session = command.request.session.id;
        if self.closed.get(&id) == Some(&session) {
            return Ok(Some(CoreReply::handle(id)));
        }
        let item = self.window(id, session)?;
        if !self.platform.is_window_alive(&item.window) {
            self.invalidate_window(id);
            return Ok(Some(CoreReply::handle(id)));
        }
        command.pending = Some(PendingCall::Closing { id });
        Ok(None)
    }

    fn dispatch(&mut self, command: &mut ActiveCommand<P>, id: u64, provider: P::Provider) -> Result<Option<CoreReply>> {
        let request = &command.request;
        request.check()?;
        self.window(id, request.session.id)?;
        let item = self.windows.get_mut(&id).unwrap();
        let provider = *item.provider.get_or_insert(provider);
        let action = match command.operation {
            Operation::Window(WindowOperation::WaitVisible) => ControlAction::WaitVisible,
            Operation::Window(WindowOperation::ReadText) => ControlAction::ReadText,
            Operation::Window(WindowOperation::Fill) => ControlAction::Fill(command.arguments.text.clone()),
            Operation::Window(WindowOperation::Click) => ControlAction::Click,
            _ => unreachable!("only control actions are dispatched to the accessibility provider"),
        };
        let result = self.platform.act(
            provider,
            request.clone(),
            id,
            item.window.clone(),
            action,
            command.arguments.control.clone(),
        )?;
        command.pending = Some(PendingCall::Action { id, result });
        Ok(None)
    }

    fn window(&self, id: u64, session: u64) -> Result<&BoundWindow<P::Window, P::Provider>> {
        self.windows
            .get(&id)
            .filter(|window| window.session == session)
            .ok_or_else(|| Failure::new("STALE", "This native window is closed or belongs to another session."))
    }

    fn invalidate_window(&mut self, id: u64) {
        if let Some(item) = self.windows.remove(&id) {
            self.closed.insert(id, item.session);
            self.release_provider(id, item.provider);
        }
    }

    fn release_window(&mut self, id: u64, live: &HashSet<u64>) {
        if let Some(item) = self.windows.remove(&id) {
            if live.contains(&item.session) {
                self.closed.insert(id, item.session);
            }
            self.release_provider(id, item.provider);
        }
    }

    fn release_provider(&self, id: u64, provider: Option<P::Provider>) {
        if let Some(provider) = provider {
            self.platform.release(id, provider);
        }
    }
}

pub(super) fn timeout_message() -> &'static str { TIMEOUT_MESSAGE }
pub(super) fn mutation_timeout_message() -> &'static str { MUTATION_TIMEOUT_MESSAGE }

fn receive<T>(reply: &Receiver<T>) -> Result<Option<T>> {
    match reply.try_recv() {
        Ok(value) => Ok(Some(value)),
        Err(mpsc::TryRecvError::Empty) => Ok(None),
        Err(mpsc::TryRecvError::Disconnected) => Err(Failure::new("BRIDGE", "The desktop accessibility provider stopped.")),
    }
}
