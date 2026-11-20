//! UI Automation objects belong to the MTA worker which created them.

use crate::{
    engine::{ControlAction, ControlResult, RequestState},
    error::{Failure, Result},
    windows::{self, WindowState},
};
use ::windows::{
    Win32::{
        System::{
            Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
                  CoUninitialize},
            Variant::VARIANT,
        },
        UI::Accessibility::*,
    },
    core::{BSTR, Error},
};

struct Apartment;

impl Drop for Apartment {
    fn drop(&mut self) {
        // SAFETY: this guard is dropped on the thread that initialized COM.
        unsafe { CoUninitialize() };
    }
}

pub(crate) struct Client {
    automation: IUIAutomation,
    _apartment: Apartment,
}

pub(crate) struct Target {
    root: IUIAutomationElement,
}

pub(crate) struct Selector<'a> {
    pub role: &'a str,
    pub name: &'a str,
}

impl Client {
    pub(crate) fn new() -> Result<Self> {
        // SAFETY: the UIA worker owns its COM apartment and has no windows.
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok() }
            .map_err(|error| Failure::new("BRIDGE", format!("Cannot initialize UI Automation: {error}")))?;
        let apartment = Apartment;
        // SAFETY: COM is initialized and the interface matches the UI Automation class.
        let automation = unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }
            .map_err(|error| Failure::new("BRIDGE", format!("Cannot create UI Automation: {error}")))?;
        Ok(Self { automation, _apartment: apartment })
    }

    pub(crate) fn attach(&self, window: &windows::Window) -> Result<Option<Target>> {
        if !windows::valid(window) {
            return Ok(None);
        }
        // SAFETY: handle comes from Win32 enumeration and COM stays on its owner thread.
        match unsafe { self.automation.ElementFromHandle(windows::hwnd(window.hwnd)) } {
            Ok(root) => Ok(Some(Target { root })),
            Err(error) if error.code().0 as u32 == UIA_E_ELEMENTNOTAVAILABLE || !windows::valid(window) => Ok(None),
            Err(error) => Err(action_error("access the window", error)),
        }
    }

    pub(crate) fn state(&self, target: &Target, window: &windows::Window) -> WindowState {
        if !windows::valid(window) {
            return WindowState::Closed;
        }
        let Ok(Some(current)) = self.attach(window) else {
            return WindowState::Inaccessible(Failure::new("STALE", "The UI Automation window is no longer accessible."));
        };
        // SAFETY: both elements belong to this worker's live COM apartment.
        match unsafe { self.automation.CompareElements(&target.root, &current.root) } {
            Ok(same) if same.as_bool() => WindowState::Ready,
            _ => WindowState::Replaced(Failure::new("STALE", "Windows reused the handle for a different UI Automation window.")),
        }
    }

    pub(crate) fn perform(
        &self, target: &Target, action: ControlAction, control: Selector<'_>,
        scope: Option<Selector<'_>>, request: &RequestState,
    ) -> Result<Option<ControlResult>> {
        match (&action, control.role) {
            (ControlAction::WaitVisible, _) | (ControlAction::Click, "button" | "link" | "menuitem") => {}
            _ => return Err(Failure::new("ACTION", "UI Automation supports wait_for() and limits click() to buttons, links and menu items.")),
        }
        let check = || request.check();
        let container = if let Some(scope) = scope {
            let Some(container) = self.find(&target.root, scope, &check)? else { return Ok(None); };
            container
        } else {
            target.root.clone()
        };
        let Some(element) = self.find(&container, control, &check)? else { return Ok(None); };
        if matches!(action, ControlAction::WaitVisible) {
            return Ok(Some(ControlResult { text: None }));
        }
        // SAFETY: the element is live on this worker; stale elements report an HRESULT.
        if !unsafe { element.CurrentIsEnabled() }.map_err(|error| action_error("check whether the control is enabled", error))?.as_bool() {
            return Ok(None);
        }
        check()?;
        // SAFETY: pattern access and invocation are confined to the owning apartment.
        let invoke: IUIAutomationInvokePattern = unsafe { element.GetCurrentPatternAs(UIA_InvokePatternId) }
            .map_err(|error| action_error("get InvokePattern for the control", error))?;
        request.begin_mutation()?;
        unsafe { invoke.Invoke() }.map_err(|error| action_error("invoke the control", error))?;
        Ok(Some(ControlResult { text: None }))
    }

    fn find(
        &self, root: &IUIAutomationElement, selector: Selector<'_>, check: &dyn Fn() -> Result<()>,
    ) -> Result<Option<IUIAutomationElement>> {
        let control_type = match selector.role {
            "button" => UIA_ButtonControlTypeId,
            "text" => UIA_TextControlTypeId,
            "textbox" => UIA_EditControlTypeId,
            "link" => UIA_HyperlinkControlTypeId,
            "menuitem" => UIA_MenuItemControlTypeId,
            "dialog" => UIA_WindowControlTypeId,
            "group" => UIA_GroupControlTypeId,
            "pane" => UIA_PaneControlTypeId,
            "menu" => UIA_MenuControlTypeId,
            "listitem" => UIA_ListItemControlTypeId,
            _ => return Err(Failure::new("CONFIGURATION", "Unsupported UI Automation role.")),
        };
        check()?;
        // SAFETY: conditions and the descendants query belong to the current apartment.
        let elements = unsafe {
            let name = self.automation.CreatePropertyCondition(
                UIA_NamePropertyId, &VARIANT::from(BSTR::from(selector.name)),
            );
            let kind = self.automation.CreatePropertyCondition(
                UIA_ControlTypePropertyId, &VARIANT::from(control_type.0),
            );
            name.and_then(|name| kind.and_then(|kind| self.automation.CreateAndCondition(&name, &kind)))
                .and_then(|condition| root.FindAll(TreeScope_Descendants, &condition))
        }.map_err(|error| action_error("find controls in the window", error))?;
        let count = unsafe { elements.Length() }.map_err(|error| action_error("count controls", error))?;
        unique_visible((0..count).map(|index| {
            check()?;
            // SAFETY: index is within the returned array.
            let element = unsafe { elements.GetElement(index) }.map_err(|error| action_error("read a control", error))?;
            let offscreen = unsafe { element.CurrentIsOffscreen() }
                .map_err(|error| action_error("check control visibility", error))?;
            Ok((!offscreen.as_bool()).then_some(element))
        }), &selector)
    }
}

fn unique_visible<T>(matches: impl Iterator<Item = Result<Option<T>>>, selector: &Selector<'_>) -> Result<Option<T>> {
    let mut found = None;
    for candidate in matches {
        if let Some(candidate) = candidate? {
            if found.is_some() {
                return Err(Failure::new("AMBIGUOUS", format!(
                    "More than one visible {} is named {:?}.", selector.role, selector.name
                )));
            }
            found = Some(candidate);
        }
    }
    Ok(found)
}

fn action_error(action: &str, error: Error) -> Failure {
    Failure::new("ACTION", format!("UI Automation could not {action}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_one_visible_control_under_the_selected_root_can_match() {
        let selector = Selector { role: "button", name: "Aplicar" };
        assert_eq!(unique_visible([Ok(None), Ok(Some(2))].into_iter(), &selector).unwrap(), Some(2));
        assert_eq!(unique_visible([Ok(None::<i32>)].into_iter(), &selector).unwrap(), None);
        let duplicate = unique_visible([Ok(Some(1)), Ok(Some(2))].into_iter(), &selector).unwrap_err();
        assert_eq!(duplicate.code, "AMBIGUOUS");
    }

    #[test]
    fn an_unavailable_invoke_pattern_has_an_action_diagnostic() {
        let error = Error::from_hresult(::windows::core::HRESULT(UIA_E_NOTSUPPORTED as i32));
        let failure = action_error("get InvokePattern for the control", error);
        assert_eq!(failure.code, "ACTION");
        assert!(failure.message.contains("InvokePattern"));
    }
}
