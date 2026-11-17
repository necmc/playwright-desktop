//! Small, explicit ABI surface matching the Windows x64 OpenJDK bridge headers.
use ::windows::Win32::Foundation::HWND;

pub type VmId = i32;
pub type JavaObject = i64;
pub const MAX_TEXT: usize = 1024;
pub const SHORT_TEXT: usize = 256;

#[repr(C)]
pub struct ContextInfo {
    pub name: [u16; MAX_TEXT],
    pub description: [u16; MAX_TEXT],
    pub role: [u16; SHORT_TEXT],
    pub role_en_us: [u16; SHORT_TEXT],
    pub states: [u16; SHORT_TEXT],
    pub states_en_us: [u16; SHORT_TEXT],
    pub index_in_parent: i32,
    pub children_count: i32,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub accessible_component: i32,
    pub accessible_action: i32,
    pub accessible_selection: i32,
    pub accessible_text: i32,
    pub accessible_interfaces: i32,
}

#[repr(C)]
#[derive(Default)]
pub struct TextInfo { pub count: i32, pub caret: i32, pub at_point: i32 }

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ActionInfo { pub name: [u16; SHORT_TEXT] }

#[repr(C)]
pub struct Actions { pub count: i32, pub actions: [ActionInfo; 256] }

#[repr(C)]
pub struct ActionsToDo { pub count: i32, pub actions: [ActionInfo; 32] }

pub type Run = unsafe extern "C" fn();
pub type IsJavaWindow = unsafe extern "C" fn(HWND) -> i32;
pub type ContextFromWindow = unsafe extern "C" fn(HWND, *mut VmId, *mut JavaObject) -> i32;
pub type Release = unsafe extern "C" fn(VmId, JavaObject);
pub type SameObject = unsafe extern "C" fn(VmId, JavaObject, JavaObject) -> i32;
pub type GetInfo = unsafe extern "C" fn(VmId, JavaObject, *mut ContextInfo) -> i32;
pub type GetChild = unsafe extern "C" fn(VmId, JavaObject, i32) -> JavaObject;
pub type GetTextInfo = unsafe extern "C" fn(VmId, JavaObject, *mut TextInfo, i32, i32) -> i32;
pub type GetTextRange = unsafe extern "C" fn(VmId, JavaObject, i32, i32, *mut u16, i16) -> i32;
pub type SetText = unsafe extern "C" fn(VmId, JavaObject, *const u16) -> i32;
pub type GetActions = unsafe extern "C" fn(VmId, JavaObject, *mut Actions) -> i32;
pub type DoActions = unsafe extern "C" fn(VmId, JavaObject, *mut ActionsToDo, *mut i32) -> i32;

pub fn zeroed_box<T>() -> Box<T> {
    let mut value = Box::<T>::new_uninit();
    // SAFETY: only called for the repr(C) POD structures above, whose fields permit zero.
    unsafe { value.as_mut_ptr().write_bytes(0, 1); value.assume_init() }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expected_windows_layouts() {
        assert_eq!(size_of::<ContextInfo>(), 6188);
        assert_eq!(align_of::<ContextInfo>(), 4);
        assert_eq!(std::mem::offset_of!(ContextInfo, role_en_us), 4608);
        assert_eq!(std::mem::offset_of!(ContextInfo, children_count), 6148);
        assert_eq!(size_of::<TextInfo>(), 12);
        assert_eq!(size_of::<Actions>(), 131076);
        assert_eq!(size_of::<ActionsToDo>(), 16388);
    }
}
