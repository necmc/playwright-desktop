pub mod ffi;
pub mod loader;

use crate::{
    config::roles,
    engine::RequestState,
    error::{Failure, Result, codes},
    windows,
};
use ffi::*;
use loader::Library;
use std::rc::Rc;

const JAB_ROLE_TEXT: &str = "text";
const JAB_ROLE_PUSH_BUTTON: &str = "push button";
const JAB_ACTION_CLICK: &str = "click";
const MAX_NODES: usize = 4096;
const MAX_DEPTH: usize = 64;
const MAX_ACTIONS: i32 = 256;

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
    let end = value.iter().position(|v| *v == 0)
        .ok_or_else(|| Failure::new(codes::ACTION, "Unterminated UTF-16 returned by Java Access Bridge."))?;
    decode_utf16(&value[..end])
}

fn decode_utf16(value: &[u16]) -> Result<String> {
    String::from_utf16(value)
        .map_err(|_| Failure::new(codes::ACTION, "Invalid UTF-16 returned by Java Access Bridge."))
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

    pub(crate) fn refresh(&self, window: &windows::Window) -> Option<Self> {
        Self::root(&self.0.library, window)
    }

    fn info(&self) -> Result<Box<ContextInfo>> {
        let mut info = ContextInfo::zeroed();
        // SAFETY: info has the checked C layout and writable capacity.
        if unsafe { (self.0.library.info)(self.0.vm, self.0.object, &mut *info) } == 0 {
            return Err(Failure::new(codes::STALE, "The Java accessible context is no longer available."));
        }
        Ok(info)
    }

    pub fn find(&self, role: &str, name: &str, check: &dyn Fn() -> Result<()>) -> Result<Option<Self>> {
        let expected = match role {
            r if r == roles::TEXTBOX => JAB_ROLE_TEXT,
            r if r == roles::BUTTON => JAB_ROLE_PUSH_BUTTON,
            _ => return Err(Failure::new(codes::ACTION, "Unsupported accessible role.")),
        };
        let mut stack = vec![(self.clone(), 0)];
        let mut found = None;
        let mut visited = 0;
        while let Some((node, depth)) = stack.pop() {
            check()?;
            visited += 1;
            let info = node.info()?;
            if decode(&info.role_en_us)? == expected && decode(&info.name)? == name {
                if found.is_some() {
                    return Err(Failure::new(codes::AMBIGUOUS, format!("More than one {role} is named {name:?}.")));
                }
                found = Some(node.clone());
            }
            if !(0..=MAX_NODES as i32).contains(&info.children_count) {
                return Err(Failure::new(codes::ACTION, "Invalid or excessive child count from Java Access Bridge."));
            }
            let children = info.children_count as usize;
            if visited + stack.len() + children > MAX_NODES || (depth == MAX_DEPTH && children != 0) {
                return Err(Failure::new(codes::ACTION, "The Java accessibility tree exceeds the traversal limit."));
            }
            for index in (0..info.children_count).rev() {
                check()?;
                // SAFETY: the node is live; returned references are owned and released by Object.
                let object = unsafe { (node.0.library.child)(node.0.vm, node.0.object, index) };
                if object == 0 {
                    return Err(Failure::new(codes::STALE, "The Java accessibility tree changed during traversal."));
                }
                stack.push((
                    Self(Rc::new(ObjectInner { library: node.0.library.clone(), vm: node.0.vm, object })),
                    depth + 1,
                ));
            }
        }
        Ok(found)
    }

    pub fn has_states(&self, required: &[&str]) -> Result<bool> {
        let info = self.info()?;
        let states = decode(&info.states_en_us)?;
        Ok(required.iter().all(|state| states.split(',').any(|part| part.trim() == *state)))
    }

    pub fn text(&self) -> Result<String> {
        let mut info = TextInfo::default();
        // SAFETY: writable checked TextInfo layout and live context.
        if unsafe { (self.0.library.text_info)(self.0.vm, self.0.object, &mut info, 0, 0) } == 0 {
            return Err(Failure::new(codes::ACTION, "The Java control does not expose text."));
        }
        if !(0..MAX_TEXT as i32).contains(&info.count) {
            return Err(Failure::new(
                codes::ACTION,
                format!("Text exceeds the supported {} UTF-16 code units.", MAX_TEXT - 1),
            ));
        }
        if info.count == 0 { return Ok(String::new()); }
        let count = info.count as usize;
        let mut buffer = [u16::MAX; MAX_TEXT];
        let capacity = (count + 1) as i16;
        // SAFETY: the requested range fits in the buffer with room for its terminator.
        if unsafe {
            (self.0.library.text_range)(self.0.vm, self.0.object, 0, info.count - 1, buffer.as_mut_ptr(), capacity)
        } == 0 {
            return Err(Failure::new(codes::ACTION, "Java Access Bridge could not read the text range."));
        }
        let text = &buffer[..count];
        if buffer[count] != 0 || text.contains(&0) {
            return Err(Failure::new(
                codes::ACTION,
                "Java Access Bridge returned an incomplete or unterminated text range.",
            ));
        }
        decode_utf16(text)
    }

    pub fn fill(&self, text: &str, request: &RequestState) -> Result<()> {
        let mut value: Vec<u16> = text.encode_utf16().take(MAX_TEXT).collect();
        if value.len() == MAX_TEXT || value.contains(&0) {
            return Err(Failure::new(
                codes::INVALID_ARGUMENT,
                format!("Java Access Bridge accepts at most {} UTF-16 code units without embedded NUL.", MAX_TEXT - 1),
            ));
        }
        value.push(0);
        request.begin_mutation()?;
        // SAFETY: value is NUL-terminated with checked size, context is live and editable.
        if unsafe { (self.0.library.set_text)(self.0.vm, self.0.object, value.as_ptr()) } == 0 {
            return Err(Failure::new(codes::ACTION, "Java Access Bridge rejected the text replacement."));
        }
        request.check()?;
        if self.text()? != text {
            return Err(Failure::new(codes::ACTION, "The Java text value does not match after replacement."));
        }
        Ok(())
    }

    pub fn click(&self, request: &RequestState) -> Result<()> {
        let mut actions = Actions::zeroed();
        // SAFETY: actions has the checked header layout and this context is still live.
        if unsafe { (self.0.library.actions)(self.0.vm, self.0.object, &mut *actions) } == 0
            || !(1..=MAX_ACTIONS).contains(&actions.count)
        {
            return Err(Failure::new(codes::ACTION, "The Java button exposes no supported accessible action."));
        }
        let mut selected = None;
        for index in 0..actions.count as usize {
            if decode(&actions.actions[index].name)?.eq_ignore_ascii_case(JAB_ACTION_CLICK) {
                selected = Some(index);
                break;
            }
        }
        let index = selected
            .or(if actions.count == 1 { Some(0) } else { None })
            .ok_or_else(|| Failure::new(codes::ACTION, "The button's accessible action is ambiguous."))?;
        let mut todo = ActionsToDo::zeroed();
        todo.count = 1;
        todo.actions[0] = actions.actions[index];
        let mut failed = -1;
        request.begin_mutation()?;
        // SAFETY: exactly one action obtained from this live control is requested once.
        if unsafe { (self.0.library.do_actions)(self.0.vm, self.0.object, &mut *todo, &mut failed) } == 0 {
            return Err(Failure::new(
                codes::ACTION,
                format!("Java rejected the button action (index {failed}); it was not retried."),
            ));
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
