use crate::error::{Failure, Result};
use ::windows::Win32::Foundation::{CloseHandle, HWND, LPARAM};
use ::windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use ::windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, EnumWindows, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, IsWindow, IsWindowVisible, MSG, PM_REMOVE,
    PeekMessageW, TranslateMessage,
};
use ::windows::core::{BOOL, PWSTR};
use std::path::PathBuf;

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

fn executable_path(window: &Window) -> Option<PathBuf> {
    // SAFETY: the PID comes from the window, and the query uses a writable UTF-16 buffer.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, window.pid) }.ok()?;
    let mut buffer = vec![0u16; 32768];
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
            name.eq_ignore_ascii_case("java.exe") || name.eq_ignore_ascii_case("javaw.exe")
        })
    })
}

struct CollectState {
    values: Vec<Window>,
    include_untitled: bool,
}

unsafe extern "system" fn collect(handle: HWND, parameter: LPARAM) -> BOOL {
    // SAFETY: EnumWindows invokes synchronously; parameter points to our live CollectState.
    unsafe {
        if IsWindowVisible(handle).as_bool() {
            let state = &mut *(parameter.0 as *mut CollectState);
            let length = GetWindowTextLengthW(handle);
            let mut pid = 0;
            GetWindowThreadProcessId(handle, Some(&mut pid));
            if length == 0 && state.include_untitled {
                state.values.push(Window { hwnd: handle.0 as isize, pid, title: String::new() });
            } else if (1..32768).contains(&length) {
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
    // SAFETY: collect only uses CollectState while this synchronous enumeration is active.
    unsafe { EnumWindows(Some(collect), LPARAM(&mut state as *mut _ as isize)) }
        .map_err(|e| Failure::new("ACTION", format!("Cannot enumerate Windows windows: {e}")))?;
    Ok(state.values)
}

pub fn pump() {
    let mut message = MSG::default();
    // SAFETY: the worker owns this thread's Windows message queue.
    unsafe {
        for _ in 0..128 {
            if !PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() { break; }
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}
