use crate::error::{Failure, Result};
use ::windows::Win32::Foundation::{HWND, LPARAM};
use ::windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, EnumWindows, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, IsWindow, IsWindowVisible, MSG, PM_REMOVE,
    PeekMessageW, TranslateMessage,
};
use ::windows::core::BOOL;

#[derive(Clone, Debug)]
pub struct Window {
    pub hwnd: isize,
    pub pid: u32,
    pub title: String,
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

unsafe extern "system" fn collect(handle: HWND, parameter: LPARAM) -> BOOL {
    // SAFETY: EnumWindows invokes synchronously; parameter points to our live Vec.
    unsafe {
        if IsWindowVisible(handle).as_bool() {
            let length = GetWindowTextLengthW(handle);
            if (1..32768).contains(&length) {
                let mut text = vec![0; length as usize + 1];
                let read = GetWindowTextW(handle, &mut text);
                let mut pid = 0;
                GetWindowThreadProcessId(handle, Some(&mut pid));
                if read > 0 && GetWindowTextLengthW(handle) <= read {
                    if let Ok(title) = String::from_utf16(&text[..read as usize]) {
                        let values = &mut *(parameter.0 as *mut Vec<Window>);
                        values.push(Window { hwnd: handle.0 as isize, pid, title });
                    }
                }
            }
        }
    }
    BOOL(1)
}

pub fn list() -> Result<Vec<Window>> {
    let mut values = Vec::<Window>::new();
    // SAFETY: collect only uses the Vec while this synchronous enumeration is active.
    unsafe { EnumWindows(Some(collect), LPARAM(&mut values as *mut _ as isize)) }
        .map_err(|e| Failure::new("ACTION", format!("Cannot enumerate Windows windows: {e}")))?;
    Ok(values)
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
