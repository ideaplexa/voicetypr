//! AX calls are safe off-main-thread; no AppKit or synthetic selection/copy.
use std::ffi::{c_char, c_void, CStr};
use std::ptr;

type Ref = *const c_void;

#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Range {
    location: isize,
    length: isize,
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> u8;
    fn AXUIElementCreateSystemWide() -> Ref;
    fn AXUIElementSetMessagingTimeout(element: Ref, timeout: f32) -> i32;
    fn AXUIElementCopyAttributeValue(element: Ref, attribute: Ref, value: *mut Ref) -> i32;
    fn AXUIElementCopyParameterizedAttributeValue(
        element: Ref,
        attribute: Ref,
        parameter: Ref,
        value: *mut Ref,
    ) -> i32;
    fn AXValueCreate(kind: u32, value: *const c_void) -> Ref;
    fn AXValueGetTypeID() -> usize;
    fn AXValueGetType(value: Ref) -> u32;
    fn AXValueGetValue(value: Ref, kind: u32, output: *mut c_void) -> u8;
}

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn IsSecureEventInputEnabled() -> u8;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(value: Ref);
    fn CFEqual(a: Ref, b: Ref) -> u8;
    fn CFGetTypeID(value: Ref) -> usize;
    fn CFStringGetTypeID() -> usize;
    fn CFStringCreateWithCString(allocator: Ref, value: *const c_char, encoding: u32) -> Ref;
    fn CFStringGetLength(value: Ref) -> isize;
    fn CFStringGetCharacters(value: Ref, range: Range, buffer: *mut u16);
}

struct Owned(Ref);
impl Owned {
    fn new(value: Ref) -> Option<Self> {
        if value.is_null() {
            None
        } else {
            Some(Self(value))
        }
    }

    fn string(value: &CStr) -> Option<Self> {
        // SAFETY: a valid NUL-terminated UTF-8 literal; Create returns ownership.
        Self::new(unsafe { CFStringCreateWithCString(ptr::null(), value.as_ptr(), 0x08000100) })
    }

    fn attribute(&self, name: &CStr) -> Option<Self> {
        self.attribute_result(name).ok()
    }

    fn attribute_result(&self, name: &CStr) -> Result<Self, i32> {
        let name = Self::string(name).ok_or(-25200)?;
        let mut value = ptr::null();
        // SAFETY: self is a live AX element and output is initialized.
        let status = unsafe { AXUIElementCopyAttributeValue(self.0, name.0, &mut value) };
        let owned = Self::new(value);
        if status == 0 {
            owned.ok_or(-25212)
        } else {
            Err(status)
        }
    }

    fn equals_string(&self, expected: &CStr) -> bool {
        let Some(expected) = Self::string(expected) else {
            return false;
        };
        // SAFETY: both are live CF objects; CFEqual accepts any CF type.
        unsafe { CFEqual(self.0, expected.0) != 0 }
    }

    fn character_at(&self, index: isize) -> Option<char> {
        // AX ranges index UTF-16 units, not UTF-8 bytes or Rust scalar indices.
        // A partial surrogate/grapheme is uncertain: preserve today's insertion.
        // SAFETY: type and bounds checked before reading exactly one UTF-16 unit.
        unsafe {
            if CFGetTypeID(self.0) != CFStringGetTypeID()
                || index < 0
                || index >= CFStringGetLength(self.0)
            {
                return None;
            }
            let mut unit = 0u16;
            CFStringGetCharacters(
                self.0,
                Range {
                    location: index,
                    length: 1,
                },
                &mut unit,
            );
            char::from_u32(u32::from(unit))
        }
    }
}

impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: every Owned holds one non-null Create/Copy reference.
        unsafe { CFRelease(self.0) };
    }
}

fn selection(element: &Owned) -> Option<Range> {
    let value = element.attribute(c"AXSelectedTextRange")?;
    let mut range = Range::default();
    // SAFETY: type-check before interpreting the value as CFRange (AX type 4).
    unsafe {
        if CFGetTypeID(value.0) != AXValueGetTypeID() || AXValueGetType(value.0) != 4 {
            return None;
        }
        (AXValueGetValue(value.0, 4, (&mut range as *mut Range).cast()) != 0).then_some(range)
    }
}

pub(super) fn previous_character() -> Option<char> {
    // Do not prompt, inspect secure fields, or read an entire document into Rust.
    // SAFETY: these system queries take no pointers.
    if unsafe { AXIsProcessTrusted() == 0 || IsSecureEventInputEnabled() != 0 } {
        return None;
    }
    let system = Owned::new(unsafe { AXUIElementCreateSystemWide() })?;
    // Each AX IPC is capped at 120ms, and the caller has a total 150ms deadline.
    if unsafe { AXUIElementSetMessagingTimeout(system.0, 0.120) } != 0 {
        return None;
    }
    let element = system.attribute(c"AXFocusedUIElement")?;
    if unsafe { AXUIElementSetMessagingTimeout(element.0, 0.120) } != 0 {
        return None;
    }
    let role = element.attribute(c"AXRole")?;
    let text_area = role.equals_string(c"AXTextArea");
    if !text_area && !role.equals_string(c"AXTextField") && !role.equals_string(c"AXComboBox") {
        return None;
    }
    match element.attribute_result(c"AXSubrole") {
        Ok(subrole) if subrole.equals_string(c"AXSecureTextField") => return None,
        // Text areas commonly omit AXSubrole. A field with unknown protection
        // status is not safe to inspect.
        Err(-25205 | -25212) if text_area => {}
        Err(_) => return None,
        Ok(_) => {}
    }
    let selected = selection(&element)?;
    if selected.location <= 0 || selected.length != 0 {
        return None; // Start of document or selection replacement, not a caret.
    }
    let preceding = Range {
        location: selected.location - 1,
        length: 1,
    };
    let parameter = Owned::new(unsafe { AXValueCreate(4, (&preceding as *const Range).cast()) })?;
    let name = Owned::string(c"AXStringForRange")?;
    let mut value = ptr::null();
    let status = unsafe {
        AXUIElementCopyParameterizedAttributeValue(element.0, name.0, parameter.0, &mut value)
    };
    let ranged = Owned::new(value);
    let character = if status == 0 {
        ranged?.character_at(0)?
    } else if matches!(status, -25205 | -25212 | -25213) {
        // Unsupported/no-value StringForRange: AXValue uses the SAME UTF-16
        // coordinate system. Never treat location as a Rust .chars() index.
        element
            .attribute(c"AXValue")?
            .character_at(preceding.location)?
    } else {
        return None; // Timeout, permission failure, or any other AX error.
    };
    let focused = system.attribute(c"AXFocusedUIElement")?;
    // Reject focus/caret drift during the query (without changing either).
    if unsafe { CFEqual(element.0, focused.0) == 0 || IsSecureEventInputEnabled() != 0 }
        || selection(&element)? != selected
    {
        return None;
    }
    Some(character)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_ax_values_never_release_null() {
        assert!(Owned::new(ptr::null()).is_none());
    }

    #[test]
    fn fallback_indexes_utf16_not_bytes_or_scalars() {
        let value = Owned::string(c"😀éHello").unwrap();
        assert_eq!(value.character_at(2), Some('é'));
        assert_eq!(value.character_at(7), Some('o'));
        assert_eq!(value.character_at(8), None);
        assert_eq!(value.character_at(-1), None);
    }

    #[test]
    fn partial_surrogates_are_unknown_not_replacement_characters() {
        let value = Owned::string(c"😀").unwrap();
        assert_eq!(value.character_at(0), None);
        assert_eq!(value.character_at(1), None);
    }
}
