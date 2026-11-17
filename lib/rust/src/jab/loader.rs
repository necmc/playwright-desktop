use super::ffi::*;
use crate::error::{Failure, Result};
use ::windows::Win32::Foundation::{FreeLibrary, HMODULE};
use ::windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_DEFAULT_DIRS, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR};
use ::windows::core::{PCSTR, PCWSTR};
use std::{fs, path::{Path, PathBuf}, rc::Rc};

struct Module(HMODULE);
impl Drop for Module {
    fn drop(&mut self) {
        // SAFETY: this guard owns one successful LoadLibraryExW reference.
        unsafe { let _ = FreeLibrary(self.0); }
    }
}

pub struct Library {
    _module: Module,
    pub is_java: IsJavaWindow,
    pub context: ContextFromWindow,
    pub release: Release,
    pub same: SameObject,
    pub info: GetInfo,
    pub child: GetChild,
    pub text_info: GetTextInfo,
    pub text_range: GetTextRange,
    pub set_text: SetText,
    pub actions: GetActions,
    pub do_actions: DoActions,
}

pub fn dll_path(java_home: &str) -> Result<PathBuf> {
    let path = Path::new(java_home).join("bin/windowsaccessbridge-64.dll");
    let path = fs::canonicalize(&path).map_err(|e| Failure::new("BRIDGE", format!("Cannot find Java Access Bridge at {}: {e}", path.display())))?;
    let bytes = fs::read(&path).map_err(|e| Failure::new("BRIDGE", e.to_string()))?;
    let offset = bytes.get(0x3c..0x40).map(|v| u32::from_le_bytes(v.try_into().unwrap()) as usize);
    if bytes.get(..2) != Some(b"MZ") || offset.and_then(|n| bytes.get(n..n+6)).is_none_or(|header| header[..4] != *b"PE\0\0" || header[4..6] != [0x64, 0x86]) {
        return Err(Failure::new("BRIDGE", "The Java bridge must be a Windows x64 DLL."));
    }
    Ok(path)
}

unsafe fn symbol<T: Copy>(module: HMODULE, name: &'static [u8]) -> Result<T> {
    // SAFETY: name is a static NUL-terminated symbol; callers select its exact header ABI.
    let function = unsafe { GetProcAddress(module, PCSTR(name.as_ptr())) }
        .ok_or_else(|| Failure::new("BRIDGE", format!("Missing Java bridge symbol {}.", String::from_utf8_lossy(&name[..name.len()-1]))))?;
    assert_eq!(size_of::<T>(), size_of_val(&function));
    // SAFETY: every instantiation below is a C function pointer with a checked symbol ABI.
    Ok(unsafe { std::mem::transmute_copy(&function) })
}

impl Library {
    pub fn load(path: &Path) -> Result<Rc<Self>> {
        use std::os::windows::ffi::OsStrExt;
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        // SAFETY: absolute, validated DLL path, terminated and alive for this call.
        let module = Module(unsafe { LoadLibraryExW(PCWSTR(wide.as_ptr()), None, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS) }
            .map_err(|e| Failure::new("BRIDGE", format!("Cannot load Java Access Bridge: {e}")))?);
        // SAFETY: these names and signatures follow AccessBridgeCalls.h/c for x64.
        unsafe {
            let run: Run = symbol(module.0, b"Windows_run\0")?;
            let library = Rc::new(Self {
                is_java: symbol(module.0, b"isJavaWindow\0")?,
                context: symbol(module.0, b"getAccessibleContextFromHWND\0")?,
                release: symbol(module.0, b"releaseJavaObject\0")?,
                same: symbol(module.0, b"isSameObject\0")?,
                info: symbol(module.0, b"getAccessibleContextInfo\0")?,
                child: symbol(module.0, b"getAccessibleChildFromContext\0")?,
                text_info: symbol(module.0, b"getAccessibleTextInfo\0")?,
                text_range: symbol(module.0, b"getAccessibleTextRange\0")?,
                set_text: symbol(module.0, b"setTextContents\0")?,
                actions: symbol(module.0, b"getAccessibleActions\0")?,
                do_actions: symbol(module.0, b"doAccessibleActions\0")?,
                _module: module,
            });
            run();
            Ok(library)
        }
    }
}
