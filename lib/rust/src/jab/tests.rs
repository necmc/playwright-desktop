use super::*;
use crate::{jab::Object, windows};
use ::windows::Win32::Foundation::HWND;
use std::cell::RefCell;

thread_local! {
    static RELEASED: RefCell<Vec<(VmId, JavaObject)>> = const { RefCell::new(Vec::new()) };
}

fn system_dll() -> PathBuf {
    let windows = std::env::var_os("SystemRoot").expect("Windows system directory");
    PathBuf::from(windows).join("System32/version.dll")
}

#[test]
fn reports_the_missing_mandatory_export() {
    let error = Library::load(&system_dll())
        .err()
        .expect("the system DLL does not export Windows_run");
    assert_eq!(error.code, "BRIDGE");
    assert_eq!(error.message, "Missing Java bridge symbol Windows_run.");
}

extern "C" fn recognized(_: HWND) -> i32 { 1 }
extern "C" fn unrecognized(_: HWND) -> i32 { 0 }

unsafe extern "C" fn root_context(_: HWND, vm: *mut VmId, object: *mut JavaObject) -> i32 {
    // SAFETY: Object::root supplies valid, writable out parameters.
    unsafe {
        *vm = 7;
        *object = 1;
    }
    1
}

extern "C" fn release(vm: VmId, object: JavaObject) {
    RELEASED.with_borrow_mut(|objects| objects.push((vm, object)));
}

unsafe extern "C" fn node_info(_: VmId, object: JavaObject, info: *mut ContextInfo) -> i32 {
    // SAFETY: Object::info passes a live, zeroed ContextInfo allocation.
    let info = unsafe { &mut *info };
    match object {
        1 => info.children_count = 3,
        2 => {
            for (target, unit) in info.role_en_us.iter_mut().zip("text".encode_utf16()) {
                *target = unit;
            }
            info.name[0] = u16::from(b'A');
        }
        3 => info.role_en_us[0] = 0xD800,
        _ => (),
    }
    1
}

extern "C" fn child(_: VmId, _: JavaObject, index: i32) -> JavaObject {
    JavaObject::from(index) + 2
}

unsafe extern "C" fn text_info(_: VmId, _: JavaObject, info: *mut TextInfo, _: i32, _: i32) -> i32 {
    // SAFETY: Object::text passes a writable TextInfo value.
    unsafe { (*info).count = 3; }
    1
}

unsafe extern "C" fn incomplete_text(
    _: VmId, _: JavaObject, _: i32, _: i32, text: *mut u16, length: i16,
) -> i32 {
    // SAFETY: Object::text passes a writable buffer of the reported size.
    let buffer = unsafe { std::slice::from_raw_parts_mut(text, length as usize) };
    buffer[..2].copy_from_slice(&[u16::from(b'A'), 0]);
    1
}

fn test_library(is_java: IsJavaWindow) -> Rc<Library> {
    extern "C" fn same(_: VmId, first: JavaObject, second: JavaObject) -> i32 {
        i32::from(first == second)
    }
    extern "C" fn set_text(_: VmId, _: JavaObject, _: *const u16) -> i32 { 0 }
    extern "C" fn get_actions(_: VmId, _: JavaObject, _: *mut Actions) -> i32 { 0 }
    extern "C" fn do_actions(_: VmId, _: JavaObject, _: *mut ActionsToDo, _: *mut i32) -> i32 { 0 }

    Rc::new(Library {
        _module: Module::load(&system_dll()).expect("system DLL loads"),
        is_java,
        context: root_context,
        release,
        same,
        info: node_info,
        child,
        text_info,
        text_range: incomplete_text,
        set_text,
        actions: get_actions,
        do_actions,
    })
}

fn test_window() -> windows::Window {
    windows::Window { hwnd: 0, pid: 1, title: String::from("JAB test") }
}

#[test]
fn ignores_a_window_the_bridge_does_not_recognize() {
    let library = test_library(unrecognized);
    assert!(Object::root(&library, &test_window()).is_none());
    RELEASED.with_borrow(|objects| assert!(objects.is_empty()));
}

#[test]
fn releases_the_candidate_and_pending_children_when_decoding_fails() {
    let library = test_library(recognized);
    let root = Object::root(&library, &test_window()).expect("reachable root");
    let error = root.find("textbox", "A", &|| Ok(())).err().expect("invalid role encoding");
    assert_eq!(error.code, "ACTION");
    assert!(error.message.contains("Invalid UTF-16"));
    RELEASED.with_borrow_mut(|objects| {
        objects.sort_unstable();
        assert_eq!(*objects, [(7, 2), (7, 3), (7, 4)]);
        objects.clear();
    });
    drop(root);
    RELEASED.with_borrow_mut(|objects| assert_eq!(std::mem::take(objects), [(7, 1)]));
}

#[test]
fn rejects_a_text_range_that_ends_before_the_reported_length() {
    let library = test_library(recognized);
    let root = Object::root(&library, &test_window()).expect("reachable root");
    let error = root.text().expect_err("the bridge returned an incomplete range");
    assert_eq!(error.code, "ACTION");
    assert!(error.message.contains("text range"));
    drop(root);
    RELEASED.with_borrow_mut(|objects| assert_eq!(std::mem::take(objects), [(7, 1)]));
}
