use super::{WindowsAccessibilityProvider, ensure_ready, lane::Lane, provider_probe_deadline};
use crate::{
    backend::{Backend, Target},
    config::{JAVA_EXPLICIT_OPTION, JAVA_PROVIDER_ID, ProviderId, SelectionMode},
    engine::{
        RequestState,
        platform::{CapturePreparation, ControlAction, ControlQuery, ControlResult},
    },
    error::{Failure, Result, codes},
    windows,
};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, mpsc::Receiver},
    thread,
    time::{Duration, Instant},
};

const JAB_LANE_NAME: &str = "playwright-desktop-jab";
const JAB_RETRY_POLL: Duration = Duration::from_millis(10);

#[derive(Default)]
struct Java {
    backend: Backend,
    targets: HashMap<u64, Target>,
}

#[derive(Default)]
pub(super) struct JavaProviderAdapter {
    lane: Option<Lane<Java>>,
}

impl JavaProviderAdapter {
    fn lane(&mut self) -> Result<&Lane<Java>> {
        if self.lane.is_none() {
            self.lane = Some(Lane::start(JAB_LANE_NAME, Java::default, |state| state.backend.poll())?);
        }
        Ok(self.lane.as_ref().unwrap())
    }
}

impl WindowsAccessibilityProvider for JavaProviderAdapter {
    fn id(&self) -> ProviderId {
        JAVA_PROVIDER_ID
    }

    fn prepare_capture(&mut self, request: Arc<RequestState>) -> Result<Option<CapturePreparation>> {
        if request.session.config.mode == SelectionMode::ExplicitProvider(JAVA_PROVIDER_ID) {
            let result = self.lane()?.submit(request, |state, request| {
                state.backend.prepare_session(request.session.id, &request.session.config, &|| request.check())
            })?;
            return Ok(Some(CapturePreparation { result, until: None }));
        }
        if !request.session.config.flag(JAVA_EXPLICIT_OPTION) {
            return Ok(None);
        }
        let until = Some(provider_probe_deadline(&request));
        let preparation = self.lane.as_ref().and_then(|lane| {
            lane.submit(request, |state, request| state.backend.validate_loaded_bridge(&request.session.config))
                .ok()
        });
        Ok(preparation.map(|result| CapturePreparation { result, until }))
    }

    fn probe_window(
        &mut self,
        request: Arc<RequestState>,
        window: windows::Window,
        deadline: Instant,
    ) -> Result<Option<Receiver<Result<bool>>>> {
        let retry_until = (!request.immediate && windows::has_java_launcher_name(&window)).then_some(deadline);
        let receiver = self.lane()?.submit(request, move |state, request| {
            let prepared = state
                .backend
                .prepare_session(request.session.id, &request.session.config, &|| request.check());
            loop {
                request.check()?;
                if state.backend.recognizes(&window) {
                    prepared?;
                    return Ok(true);
                }

                let remaining = retry_until.map_or(Duration::ZERO, |until| {
                    until.saturating_duration_since(Instant::now())
                });
                if prepared.is_err() || remaining.is_zero() {
                    return Ok(false);
                }

                // Swing may create its window before JAB receives the registration.
                state.backend.poll();
                thread::sleep(remaining.min(JAB_RETRY_POLL));
            }
        })?;
        Ok(Some(receiver))
    }

    fn act(
        &mut self,
        request: Arc<RequestState>,
        id: u64,
        window: windows::Window,
        action: ControlAction,
        query: ControlQuery,
    ) -> Result<Receiver<Result<Option<ControlResult>>>> {
        self.lane()?.submit(request, move |state, request| {
            let check = || request.check();
            state.backend.prepare_session(request.session.id, &request.session.config, &check)?;
            if !query.scope_role.is_empty() {
                return Err(Failure::new(codes::ACTION, "Scoped control lookup requires UI Automation."));
            }
            let target = if let Some(target) = state.targets.get(&id) {
                target
            } else {
                check()?;
                let Some(target) = state.backend.attach(window)? else {
                    return Ok(None);
                };
                state.targets.entry(id).or_insert(target)
            };
            ensure_ready(target.state())?;
            target.perform(action, &query.role, &query.name, request)
        })
    }

    fn release(&self, id: u64) {
        if let Some(lane) = &self.lane {
            lane.cleanup(move |state| {
                state.targets.remove(&id);
            });
        }
    }

    fn retain_sessions(&self, sessions: &HashSet<u64>) {
        if let Some(lane) = &self.lane {
            let live = sessions.clone();
            lane.cleanup(move |state| state.backend.retain_sessions(&live));
        }
    }

    fn check_health(&self) {
        if let Some(lane) = &self.lane {
            let _ = lane.check_health();
        }
    }
}
