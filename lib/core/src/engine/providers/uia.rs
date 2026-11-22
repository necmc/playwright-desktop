use super::{WindowsAccessibilityProvider, ensure_ready, lane::Lane};
use crate::{
    config::{ProviderId, UIA_PROVIDER_ID},
    engine::{
        RequestState,
        platform::{ControlAction, ControlQuery, ControlResult},
    },
    error::Result,
    uia,
    windows,
};
use std::{
    collections::HashMap,
    sync::{Arc, mpsc::Receiver},
};

const UIA_LANE_NAME: &str = "playwright-desktop-uia";

#[derive(Default)]
struct Uia {
    targets: HashMap<u64, uia::Target>,
    client: Option<uia::Client>,
}

impl Uia {
    fn client(&mut self) -> Result<&uia::Client> {
        if self.client.is_none() {
            self.client = Some(uia::Client::new()?);
        }
        Ok(self.client.as_ref().unwrap())
    }
}

#[derive(Default)]
pub(super) struct UiaProviderAdapter {
    lane: Option<Lane<Uia>>,
}

impl UiaProviderAdapter {
    fn lane(&mut self) -> Result<&Lane<Uia>> {
        if self.lane.is_none() {
            self.lane = Some(Lane::start(UIA_LANE_NAME, Uia::default, |_| {})?);
        }
        Ok(self.lane.as_ref().unwrap())
    }
}

impl WindowsAccessibilityProvider for UiaProviderAdapter {
    fn id(&self) -> ProviderId {
        UIA_PROVIDER_ID
    }

    fn contains_visible_text(
        &mut self,
        request: Arc<RequestState>,
        candidates: Vec<windows::Window>,
        has_text: &str,
    ) -> Result<Option<Receiver<Result<Vec<windows::Window>>>>> {
        let has_text = has_text.to_string();
        let receiver = self.lane()?.submit(request, move |state, request| {
            let check = || request.check();
            let client = state.client()?;
            let mut matched = Vec::new();
            for window in candidates {
                if client.contains_visible_text(&window, &has_text, &check)? {
                    matched.push(window);
                }
            }
            Ok(matched)
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
            state.client()?;
            let client = state.client.as_ref().unwrap();
            let target = if let Some(target) = state.targets.get(&id) {
                ensure_ready(client.state(target, &window))?;
                target
            } else {
                check()?;
                let Some(target) = client.attach(&window)? else {
                    return Ok(None);
                };
                state.targets.entry(id).or_insert(target)
            };
            client.perform(
                target,
                action,
                uia::Selector {
                    role: &query.role,
                    name: &query.name,
                },
                (!query.scope_role.is_empty()).then_some(uia::Selector {
                    role: &query.scope_role,
                    name: &query.scope_name,
                }),
                request,
            )
        })
    }

    fn release(&self, id: u64) {
        if let Some(lane) = &self.lane {
            lane.cleanup(move |state| {
                state.targets.remove(&id);
            });
        }
    }

    fn check_health(&self) {
        if let Some(lane) = &self.lane {
            let _ = lane.check_health();
        }
    }
}
