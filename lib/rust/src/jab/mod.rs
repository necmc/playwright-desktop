pub mod ffi;
pub mod loader;

use crate::{error::{Failure, Result}, windows};
use ffi::*;
use loader::Library;
use std::rc::Rc;

struct ObjectInner { library: Rc<Library>, vm: VmId, object: JavaObject }
impl Drop for ObjectInner {
    fn drop(&mut self) {
        // SAFETY: the worker owns this JAB reference and its library is still alive.
        unsafe { (self.library.release)(self.vm, self.object); }
    }
}

#[derive(Clone)]
pub struct Object(Rc<ObjectInner>);

pub fn decode(value: &[u16]) -> Result<String> {
    let end = value.iter().position(|v| *v == 0).ok_or_else(|| Failure::new("ACTION", "Unterminated UTF-16 returned by Java Access Bridge."))?;
    String::from_utf16(&value[..end]).map_err(|_| Failure::new("ACTION", "Invalid UTF-16 returned by Java Access Bridge."))
}

impl Object {
    pub fn root(library: &Rc<Library>, window: &windows::Window) -> Option<Self> {
        let mut vm = 0;
        let mut object = 0;
        // SAFETY: this only runs on the bridge's owner thread, with valid out pointers.
        unsafe {
            if (library.is_java)(windows::hwnd(window.hwnd)) == 0 { return None; }
            if (library.context)(windows::hwnd(window.hwnd), &mut vm, &mut object) == 0 || object == 0 { return None; }
        }
        Some(Self(Rc::new(ObjectInner { library: library.clone(), vm, object })))
    }

    pub fn same(&self, other: &Self) -> bool {
        self.0.vm == other.0.vm
            // SAFETY: both references remain live and belong to this worker.
            && unsafe { (self.0.library.same)(self.0.vm, self.0.object, other.0.object) != 0 }
    }

    fn info(&self) -> Result<Box<ContextInfo>> {
        let mut info = zeroed_box::<ContextInfo>();
        // SAFETY: info has the checked C layout and writable capacity.
        if unsafe { (self.0.library.info)(self.0.vm, self.0.object, &mut *info) } == 0 {
            return Err(Failure::new("STALE", "The Java accessible context is no longer available."));
        }
        Ok(info)
    }

    pub fn find(&self, role: &str, name: &str, check: &dyn Fn() -> Result<()>) -> Result<Option<Self>> {
        let expected = match role { "textbox" => "text", "button" => "push button", _ => return Err(Failure::new("ACTION", "Unsupported accessible role.")) };
        let mut stack = vec![(self.clone(), 0)];
        let mut found = None;
        let mut visited = 0;
        while let Some((node, depth)) = stack.pop() {
            check()?;
            visited += 1;
            if visited > 4096 || depth > 64 {
                return Err(Failure::new("ACTION", "The Java accessibility tree exceeds the traversal limit."));
            }
            let info = node.info()?;
            if decode(&info.role_en_us)? == expected && decode(&info.name)? == name {
                if found.is_some() { return Err(Failure::new("AMBIGUOUS", format!("More than one {role} is named {name:?}."))); }
                found = Some(node.clone());
            }
            if !(0..=4096).contains(&info.children_count) {
                return Err(Failure::new("ACTION", "Invalid or excessive child count from Java Access Bridge."));
            }
            for index in (0..info.children_count).rev() {
                check()?;
                // SAFETY: the node is live; returned references are owned and released by Object.
                let object = unsafe { (node.0.library.child)(node.0.vm, node.0.object, index) };
                if object == 0 { return Err(Failure::new("STALE", "The Java accessibility tree changed during traversal.")); }
                stack.push((Self(Rc::new(ObjectInner { library: node.0.library.clone(), vm: node.0.vm, object })), depth + 1));
            }
        }
        Ok(found)
    }

    pub fn ready(&self, editable: bool) -> Result<bool> {
        let info = self.info()?;
        let states = decode(&info.states_en_us)?;
        let has = |state: &str| states.split(',').any(|part| part.trim() == state);
        Ok(has("enabled") && has("showing") && (!editable || has("editable")))
    }

    pub fn text(&self) -> Result<String> {
        let mut info = TextInfo::default();
        // SAFETY: writable checked TextInfo layout and live context.
        if unsafe { (self.0.library.text_info)(self.0.vm, self.0.object, &mut info, 0, 0) } == 0 {
            return Err(Failure::new("ACTION", "The Java control does not expose text."));
        }
        if !(0..MAX_TEXT as i32).contains(&info.count) {
            return Err(Failure::new("ACTION", "Text exceeds the supported 1023 UTF-16 code units."));
        }
        if info.count == 0 { return Ok(String::new()); }
        let mut buffer = [0u16; MAX_TEXT];
        // SAFETY: indices match the returned text length and buffer capacity is explicit.
        if unsafe { (self.0.library.text_range)(self.0.vm, self.0.object, 0, info.count - 1, buffer.as_mut_ptr(), MAX_TEXT as i16) } == 0 {
            return Err(Failure::new("ACTION", "Java Access Bridge could not read the text range."));
        }
        decode(&buffer)
    }

    pub fn fill(&self, text: &str, check: &dyn Fn() -> Result<()>) -> Result<()> {
        let value: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
        if value.len() > MAX_TEXT || value[..value.len()-1].contains(&0) {
            return Err(Failure::new("ACTION", "Text must fit in 1023 UTF-16 code units without embedded NUL."));
        }
        check()?;
        // SAFETY: value is NUL-terminated with checked size, context is live and editable.
        if unsafe { (self.0.library.set_text)(self.0.vm, self.0.object, value.as_ptr()) } == 0 {
            return Err(Failure::new("ACTION", "Java Access Bridge rejected the text replacement."));
        }
        check()?;
        if self.text()? != text { return Err(Failure::new("ACTION", "The Java text value does not match after replacement.")); }
        Ok(())
    }

    pub fn click(&self, check: &dyn Fn() -> Result<()>) -> Result<()> {
        let mut actions = zeroed_box::<Actions>();
        // SAFETY: actions has the checked header layout and this context is still live.
        if unsafe { (self.0.library.actions)(self.0.vm, self.0.object, &mut *actions) } == 0 || !(1..=256).contains(&actions.count) {
            return Err(Failure::new("ACTION", "The Java button exposes no supported accessible action."));
        }
        let mut selected = None;
        for index in 0..actions.count as usize {
            if decode(&actions.actions[index].name)?.eq_ignore_ascii_case("click") { selected = Some(index); break; }
        }
        let index = selected.or(if actions.count == 1 { Some(0) } else { None })
            .ok_or_else(|| Failure::new("ACTION", "The button's accessible action is ambiguous."))?;
        let mut todo = zeroed_box::<ActionsToDo>();
        todo.count = 1;
        todo.actions[0] = actions.actions[index];
        let mut failed = -1;
        check()?;
        // SAFETY: exactly one action obtained from this live control is requested once.
        if unsafe { (self.0.library.do_actions)(self.0.vm, self.0.object, &mut *todo, &mut failed) } == 0 {
            return Err(Failure::new("ACTION", format!("Java rejected the button action (index {failed}); it was not retried.")));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_is_decoded_without_silent_truncation() {
        let value: Vec<u16> = "Álias \u{10400}".encode_utf16().chain(Some(0)).collect();
        assert_eq!(decode(&value).unwrap(), "Álias \u{10400}");
        assert!(decode(&[0xd800, 0]).is_err());
        assert!(decode(&[65, 66]).is_err());
    }
}
