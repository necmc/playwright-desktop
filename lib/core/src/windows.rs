use crate::error::{Failure, Result, codes};
use ::windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, NO_ERROR, SetLastError, WPARAM};
use ::windows::Win32::Globalization::{GetUserDefaultUILanguage, LCIDToLocaleName};
use ::windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
use ::windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use ::windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, DispatchMessageW, EnumWindows, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible, MSG, PM_REMOVE,
    PeekMessageW, PostMessageW, SW_RESTORE, SetForegroundWindow, ShowWindow, TranslateMessage,
    WM_CLOSE,
};
use ::windows::core::{BOOL, PWSTR};
use std::path::PathBuf;

const MAX_PROCESS_PATH_UTF16: usize = 32768;
const MAX_WINDOW_TITLE_UTF16: i32 = 32768;
const MAX_PUMP_MESSAGES: usize = 128;
const LOCALE_NAME_MAX_LENGTH: usize = 85;
const DEFAULT_OS_LANGUAGE: &str = "en";
const JAVA_LAUNCHER_NAMES: &[&str] = &["java.exe", "javaw.exe"];

pub(crate) const WINDOW_NOT_FOUND_MESSAGE: &str = "The native window no longer exists.";

#[derive(Clone, Debug)]
pub struct Window {
    pub hwnd: isize,
    pub pid: u32,
    pub title: String,
}

pub(crate) enum WindowState {
    Ready,
    Closed,
    Inaccessible(Failure),
    Replaced(Failure),
}

pub fn hwnd(value: isize) -> HWND { HWND(value as *mut _) }

pub fn valid(window: &Window) -> bool {
    let mut pid = 0;
    // SAFETY: handles originate from EnumWindows; these APIs tolerate stale handles.
    unsafe {
        IsWindow(Some(hwnd(window.hwnd))).as_bool()
            && GetWindowThreadProcessId(hwnd(window.hwnd), Some(&mut pid)) != 0
            && pid == window.pid
    }
}

pub(crate) fn bring_to_front(window: &Window) -> Result<()> {
    if !valid(window) {
        return Err(Failure::new(codes::STALE, WINDOW_NOT_FOUND_MESSAGE));
    }
    let handle = hwnd(window.hwnd);
    // SAFETY: handle is verified live for this PID immediately above.
    unsafe {
        if IsIconic(handle).as_bool() {
            let _ = ShowWindow(handle, SW_RESTORE);
        }
        let _ = BringWindowToTop(handle);
        let _ = SetForegroundWindow(handle);
    }
    if !valid(window) {
        return Err(Failure::new(codes::STALE, WINDOW_NOT_FOUND_MESSAGE));
    }
    Ok(())
}

pub(crate) fn post_close(window: &Window) -> Result<bool> {
    if !valid(window) {
        return Ok(false);
    }
    // SAFETY: handle is verified live; WM_CLOSE requests a graceful close without forcing termination.
    unsafe { PostMessageW(Some(hwnd(window.hwnd)), WM_CLOSE, WPARAM(0), LPARAM(0)) }
        .map_err(|e| Failure::new(codes::ACTION, format!("Cannot close native window: {e}")))?;
    Ok(true)
}

fn executable_path(window: &Window) -> Option<PathBuf> {
    // SAFETY: the PID comes from the window, and the query uses a writable UTF-16 buffer.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, window.pid) }.ok()?;
    let mut buffer = vec![0u16; MAX_PROCESS_PATH_UTF16];
    let mut length = buffer.len() as u32;
    let result = unsafe {
        let result = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buffer.as_mut_ptr()), &mut length);
        let _ = CloseHandle(process);
        result
    };
    result.ok()?;
    Some(PathBuf::from(String::from_utf16(&buffer[..length as usize]).ok()?))
}

pub(crate) fn has_java_launcher_name(window: &Window) -> bool {
    executable_path(window).is_some_and(|path| {
        path.file_name().and_then(|name| name.to_str()).is_some_and(|name| {
            JAVA_LAUNCHER_NAMES.iter().any(|candidate| name.eq_ignore_ascii_case(candidate))
        })
    })
}

struct CollectState {
    values: Vec<Window>,
    include_untitled: bool,
}

fn is_cloaked(handle: HWND) -> bool {
    let mut cloaked: u32 = 0;
    // SAFETY: handle comes from EnumWindows; cloaked is a writable u32 out-parameter for DWMWA_CLOAKED.
    let result = unsafe {
        DwmGetWindowAttribute(
            handle,
            DWMWA_CLOAKED,
            std::ptr::from_mut(&mut cloaked).cast(),
            std::mem::size_of::<u32>() as u32,
        )
    };
    result.is_ok() && cloaked != 0
}

unsafe extern "system" fn collect(handle: HWND, parameter: LPARAM) -> BOOL {
    // SAFETY: EnumWindows invokes synchronously; parameter points to our live CollectState.
    unsafe {
        if IsWindowVisible(handle).as_bool() && !is_cloaked(handle) {
            let state = &mut *(parameter.0 as *mut CollectState);
            let length = GetWindowTextLengthW(handle);
            let mut pid = 0;
            GetWindowThreadProcessId(handle, Some(&mut pid));
            if length == 0 && state.include_untitled {
                state.values.push(Window { hwnd: handle.0 as isize, pid, title: String::new() });
            } else if (1..MAX_WINDOW_TITLE_UTF16).contains(&length) {
                let mut text = vec![0; length as usize + 1];
                let read = GetWindowTextW(handle, &mut text);
                if read > 0 && GetWindowTextLengthW(handle) <= read {
                    if let Ok(title) = String::from_utf16(&text[..read as usize]) {
                        state.values.push(Window { hwnd: handle.0 as isize, pid, title });
                    }
                }
            }
        }
    }
    BOOL(1)
}

pub fn list_for_query(include_untitled: bool) -> Result<Vec<Window>> {
    let mut state = CollectState { values: Vec::new(), include_untitled };
    // EnumWindows returns FALSE with NO_ERROR when the desktop has zero windows.
    // Clear any stale Win32 error in TLS first so empty desktops are not misreported.
    unsafe { SetLastError(NO_ERROR) };
    // SAFETY: collect only uses CollectState while this synchronous enumeration is active.
    if let Err(error) = unsafe { EnumWindows(Some(collect), LPARAM(&mut state as *mut _ as isize)) } {
        if error.code().0 != 0 {
            return Err(Failure::new(codes::ACTION, format!("Cannot enumerate Windows windows: {error}")));
        }
    }
    Ok(state.values)
}

pub fn pump() {
    let mut message = MSG::default();
    // SAFETY: the worker owns this thread's Windows message queue.
    unsafe {
        for _ in 0..MAX_PUMP_MESSAGES {
            if !PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() { break; }
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

pub fn host_os_language() -> String {
    // SAFETY: GetUserDefaultUILanguage has no preconditions and LCIDToLocaleName writes into our fixed UTF-16 buffer.
    unsafe {
        let lang_id = u32::from(GetUserDefaultUILanguage());
        let mut buffer = [0u16; LOCALE_NAME_MAX_LENGTH];
        let written = LCIDToLocaleName(lang_id, Some(&mut buffer), 0);
        if written > 0 {
            let slice = &buffer[..(written as usize).min(buffer.len())];
            let end = slice.iter().position(|&unit| unit == 0).unwrap_or(slice.len());
            if let Ok(tag) = String::from_utf16(&slice[..end]) {
                let normalized = tag.trim().replace('_', "-");
                if !normalized.is_empty() {
                    return normalized;
                }
            }
        }
    }
    DEFAULT_OS_LANGUAGE.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloaked_check_handles_invalid_handle_without_panicking() {
        assert!(!is_cloaked(HWND(std::ptr::null_mut())));
    }

    #[test]
    fn list_for_query_succeeds_with_cloaked_filter() {
        assert!(list_for_query(true).is_ok());
    }
}
