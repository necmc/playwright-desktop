use super::*;
use crate::jab::{ffi::{JavaObject, VmId}, loader::tests::test_library};
use ::windows::{
    Win32::{
        Foundation::HWND,
        UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, WINDOW_EX_STYLE, WS_POPUP},
    },
    core::w,
};
use std::{cell::{Cell, RefCell}, rc::Rc};

struct HiddenWindow(HWND);

impl HiddenWindow {
    fn new() -> Self {
        // SAFETY: STATIC is a system class and the window has no parent or callback data.
        let handle = unsafe {
            CreateWindowExW(WINDOW_EX_STYLE(0), w!("STATIC"), w!("JAB worker test"),
                           WS_POPUP, 0, 0, 1, 1, None, None, None, None)
        }.expect("create hidden window");
        Self(handle)
    }

    fn window(&self) -> windows::Window {
        let window = windows::Window {
            hwnd: self.0.0 as isize,
            pid: std::process::id(),
            title: "JAB worker test".into(),
        };
        assert!(windows::valid(&window));
        window
    }
}

impl Drop for HiddenWindow {
    fn drop(&mut self) {
        // SAFETY: this test thread created and owns the window.
        unsafe { let _ = DestroyWindow(self.0); }
    }
}

#[test]
fn a_reused_hwnd_cannot_route_an_action_to_a_different_java_root() {
    assert_stale_root(4, "reused the handle", 2);
}

#[test]
fn an_unavailable_java_root_releases_the_window_on_the_worker_thread() {
    assert_stale_root(0, "no longer accessible", 1);
}

fn assert_stale_root(next_root: JavaObject, expected_error: &str, expected_releases: usize) {
    thread_local! {
        static ROOT: Cell<JavaObject> = const { Cell::new(1) };
        static JAB_THREADS: RefCell<Vec<(bool, std::thread::ThreadId)>> = const { RefCell::new(Vec::new()) };
    }

    unsafe extern "C" fn current_root(_: HWND, vm: *mut VmId, object: *mut JavaObject) -> i32 {
        JAB_THREADS.with_borrow_mut(|threads| threads.push((false, std::thread::current().id())));
        // SAFETY: Object::root passes writable out parameters.
        unsafe {
            *vm = 7;
            *object = ROOT.get();
        }
        1
    }

    extern "C" fn release(_: VmId, _: JavaObject) {
        JAB_THREADS.with_borrow_mut(|threads| threads.push((true, std::thread::current().id())));
    }

    let handle = HiddenWindow::new();
    let window = handle.window();

    let worker = std::thread::spawn(move || {
        let mut library = test_library();
        let bindings = Rc::get_mut(&mut library).unwrap();
        bindings.context = current_root;
        bindings.release = release;
        let backend = Backend::with_test_library(library, []);
        let target = backend.attach(window).unwrap().expect("original Java root");
        ROOT.set(next_root);
        let error = match target.state() {
            WindowState::Replaced(error) | WindowState::Inaccessible(error) => error,
            _ => panic!("reject the stale Java root"),
        };
        drop(target);
        let calls = JAB_THREADS.with_borrow_mut(std::mem::take);
        (error, calls)
    });
    let thread_id = worker.thread().id();
    let (error, calls) = worker.join().unwrap();
    assert_eq!(error.code, "STALE");
    assert!(error.message.contains(expected_error));
    assert_eq!(calls.iter().filter(|(released, _)| *released).count(), expected_releases);
    assert_eq!(calls.iter().filter(|(released, _)| !*released).count(), 2);
    assert!(calls.iter().all(|(_, id)| *id == thread_id));
}
