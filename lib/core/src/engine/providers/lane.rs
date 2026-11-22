use super::super::RequestState;
use crate::error::{Failure, Result, codes};
use std::{
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::{Duration, Instant},
};

const LANE_IDLE_POLL: Duration = Duration::from_millis(10);
const LANE_STALL_GRACE: Duration = Duration::from_millis(100);

type Job<State> = Box<dyn FnOnce(&mut State) + Send>;

/// Each provider owns its handles on an independent thread. The coordinator polls
/// replies without waiting on either provider's native calls.
pub(super) struct Lane<State> {
    sender: Sender<Job<State>>,
    status: Arc<Mutex<LaneStatus>>,
}

#[derive(Default)]
struct LaneStatus {
    active_deadline: Option<Instant>,
    stopped: bool,
}

impl<State: 'static> Lane<State> {
    pub(super) fn start(name: &str, init: fn() -> State, idle: fn(&mut State)) -> Result<Self> {
        let (sender, receiver) = mpsc::channel::<Job<State>>();
        thread::Builder::new()
            .name(name.into())
            .spawn(move || {
                let mut state = init();
                loop {
                    idle(&mut state);
                    match receiver.recv_timeout(LANE_IDLE_POLL) {
                        Ok(job) => job(&mut state),
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
            })
            .map_err(|error| Failure::new(codes::BRIDGE, format!("Cannot start {name}: {error}")))?;
        Ok(Self {
            sender,
            status: Arc::new(Mutex::new(LaneStatus::default())),
        })
    }

    pub(super) fn submit<Output: Send + 'static>(
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

    pub(super) fn check_health(&self) -> Result<()> {
        let mut status = self.status.lock().unwrap();
        if status.active_deadline.is_some_and(|deadline| {
            Instant::now().saturating_duration_since(deadline) >= LANE_STALL_GRACE
        }) {
            status.stopped = true;
        }
        if status.stopped { Err(unavailable()) } else { Ok(()) }
    }

    pub(super) fn cleanup(&self, job: impl FnOnce(&mut State) + Send + 'static) {
        if self.check_health().is_ok() {
            let _ = self.sender.send(Box::new(job));
        }
    }
}

fn unavailable() -> Failure {
    Failure::new(
        codes::BRIDGE,
        "The desktop accessibility provider is unavailable. Restart the host process to recover it.",
    )
}
