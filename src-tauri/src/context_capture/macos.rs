//! Read-only AX capture. No document-value, clipboard, title, or path reads.
use super::Status;
use crate::ai::context_reference::ContextReference;
use objc2::{msg_send, rc::Retained, runtime::AnyObject};
use objc2_app_kit::NSWorkspace;
use objc2_foundation::NSString;
use std::ffi::{c_char, c_void, CStr};
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

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
    fn AXUIElementCreateApplication(pid: i32) -> Ref;
    fn AXUIElementSetMessagingTimeout(element: Ref, timeout: f32) -> i32;
    fn AXUIElementGetPid(element: Ref, pid: *mut i32) -> i32;
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

// SAFETY: these are retained, immutable CF/AX references. AX explicitly supports
// background-thread clients; no AppKit objects or borrowed references cross threads.
unsafe impl Send for Owned {}
unsafe impl Sync for Owned {}

impl Owned {
    fn new(value: Ref) -> Result<Self, Status> {
        if value.is_null() {
            Err(Status::Unsupported)
        } else {
            Ok(Self(value))
        }
    }

    fn string(value: &CStr) -> Result<Self, Status> {
        // SAFETY: literal is NUL terminated UTF-8; Create transfers ownership.
        Self::new(unsafe { CFStringCreateWithCString(ptr::null(), value.as_ptr(), 0x08000100) })
    }

    fn equals(&self, other: &Self) -> bool {
        // SAFETY: both objects remain retained for the comparison.
        unsafe { CFEqual(self.0, other.0) != 0 }
    }

    fn is_string(&self, value: &CStr) -> Result<bool, Status> {
        Ok(self.equals(&Self::string(value)?))
    }
}

impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: exactly one owned Create/Copy reference.
        unsafe { CFRelease(self.0) };
    }
}

pub(super) struct Target {
    pid: i32,
    bundle: String,
    window: Owned,
    element: Owned,
    range: Range,
}

pub(super) struct Captured {
    pub target: Target,
    pub reference: ContextReference,
    pub partial: bool,
}

struct Budget<'a> {
    deadline: Instant,
    cancelled: &'a AtomicBool,
}

impl Budget<'_> {
    fn check(&self) -> Result<(), Status> {
        if self.cancelled.load(Ordering::Acquire) || Instant::now() >= self.deadline {
            Err(Status::Timeout)
        } else {
            Ok(())
        }
    }

    fn call<T>(&self, operation: impl FnOnce() -> T) -> Result<T, Status> {
        self.check()?;
        let result = operation();
        self.check()?;
        Ok(result)
    }

    fn ipc(&self, element: &Owned, operation: impl FnOnce() -> i32) -> Result<(), Status> {
        self.check()?;
        // Refresh before EVERY IPC: no inherited/default AX timeout is trusted.
        // A small margin also prevents f32 rounding from exceeding 50ms.
        let seconds = self.deadline.saturating_duration_since(Instant::now()).as_secs_f32();
        let timeout = seconds.min(0.049);
        if timeout <= 0.0 {
            return Err(Status::Timeout);
        }
        // SAFETY: element is a live AX object, timeout is positive and <=49ms.
        let result = self.call(|| unsafe { AXUIElementSetMessagingTimeout(element.0, timeout) })?;
        ax_result(result)?;
        ax_result(self.call(operation)?)
    }

    fn attribute(&self, element: &Owned, name: &CStr) -> Result<Owned, Status> {
        let name = Owned::string(name)?;
        let mut result = ptr::null();
        // SAFETY: live AX element, CFString attribute and initialized output.
        let status = self.ipc(element, || unsafe {
            AXUIElementCopyAttributeValue(element.0, name.0, &mut result)
        });
        let owned = Owned::new(result);
        status?;
        owned
    }

    fn pid(&self, element: &Owned) -> Result<i32, Status> {
        let mut pid = 0;
        // SAFETY: live AX object and valid pid output.
        self.ipc(element, || unsafe { AXUIElementGetPid(element.0, &mut pid) })?;
        if pid > 0 { Ok(pid) } else { Err(Status::Denied) }
    }

    fn security(&self) -> Result<(), Status> {
        // SAFETY: public system queries, no pointers or prompting.
        if self.call(|| unsafe { IsSecureEventInputEnabled() })? != 0 {
            return Err(Status::Secure);
        }
        if self.call(|| unsafe { AXIsProcessTrusted() })? == 0 {
            return Err(Status::Denied);
        }
        Ok(())
    }

    fn range(&self, element: &Owned) -> Result<Range, Status> {
        let value = self.attribute(element, c"AXSelectedTextRange")?;
        let mut range = Range::default();
        // SAFETY: validate CF and AX types before interpreting the range.
        let type_id = self.call(|| unsafe { AXValueGetTypeID() })?;
        if unsafe { CFGetTypeID(value.0) } != type_id
            || self.call(|| unsafe { AXValueGetType(value.0) })? != 4
        {
            return Err(Status::Unsupported);
        }
        let valid = self.call(|| unsafe {
            AXValueGetValue(value.0, 4, (&mut range as *mut Range).cast()) != 0
        })?;
        if valid && valid_range(range) { Ok(range) } else { Err(Status::Unsupported) }
    }

    fn field_security(&self, element: &Owned) -> Result<(), Status> {
        let role = self.attribute(element, c"AXRole")?;
        if role.is_string(c"AXSecureTextField")? {
            return Err(Status::Secure);
        }
        if !role.is_string(c"AXTextArea")?
            && !role.is_string(c"AXTextField")?
            && !role.is_string(c"AXComboBox")?
        {
            return Err(Status::Unsupported);
        }
        match self.attribute(element, c"AXSubrole") {
            Ok(subrole) if subrole.is_string(c"AXSecureTextField")? => Err(Status::Secure),
            // Standard editable controls can omit subrole. Only explicit AX
            // unsupported/no-value is accepted, never failed/denied/timed-out IPC.
            Err(Status::Unsupported) | Ok(_) => Ok(()),
            Err(error) => Err(error),
        }
    }

    fn text(&self, element: &Owned, range: Range) -> Result<String, Status> {
        self.check()?;
        if range.length == 0 {
            return Ok(String::new());
        }
        self.security()?;
        self.field_security(element)?;
        // SAFETY: range is a valid bounded CFRange; AXValueCreate copies it.
        let parameter = Owned::new(self.call(|| unsafe {
            AXValueCreate(4, (&range as *const Range).cast())
        })?)?;
        let name = Owned::string(c"AXStringForRange")?;
        let mut value = ptr::null();
        // SAFETY: live AX element and retained parameter/attribute.
        let status = self.ipc(element, || unsafe {
            AXUIElementCopyParameterizedAttributeValue(element.0, name.0, parameter.0, &mut value)
        });
        let value = Owned::new(value);
        status?;
        let value = value?;
        // Never allocate according to an untrusted provider's unbounded length.
        // SAFETY: CF type is checked before any string operations.
        let length = unsafe {
            if CFGetTypeID(value.0) != CFStringGetTypeID() {
                return Err(Status::Unsupported);
            }
            CFStringGetLength(value.0)
        };
        if length < 0 || length > range.length {
            return Err(Status::Unsupported);
        }
        let mut units = vec![0; length as usize];
        // SAFETY: buffer holds the exact validated number of UTF-16 units.
        unsafe { CFStringGetCharacters(value.0, Range { location: 0, length }, units.as_mut_ptr()) };
        self.check()?;
        decode_bounded(&units)
    }
}

fn ax_result(code: i32) -> Result<(), Status> {
    match code {
        0 => Ok(()),
        -25204 => Err(Status::Timeout), // kAXErrorCannotComplete
        -25211 => Err(Status::Denied), // kAXErrorAPIDisabled
        -25205 | -25212 | -25213 => Err(Status::Unsupported),
        _ => Err(Status::Denied),
    }
}

fn valid_range(range: Range) -> bool {
    range.location >= 0 && range.length >= 0 && range.location.checked_add(range.length).is_some()
}

fn decode_bounded(units: &[u16]) -> Result<String, Status> {
    // AX offsets are UTF-16: discard only a boundary-split surrogate, not
    // arbitrary invalid interior data, and never manufacture replacement text.
    let start = usize::from(units.first().is_some_and(|v| (0xdc00..=0xdfff).contains(v)));
    let end = units.len() - usize::from(units.last().is_some_and(|v| (0xd800..=0xdbff).contains(v)));
    String::from_utf16(&units[start.min(end)..end]).map_err(|_| Status::Unsupported)
}

fn app_policy(bundle: &str) -> Result<(), Status> {
    match bundle {
        "com.apple.Terminal" | "com.googlecode.iterm2" | "dev.warp.Warp-Stable"
        | "com.mitchellh.ghostty" | "net.kovidgoyal.kitty" | "org.alacritty" => Err(Status::Unsupported),
        "com.agilebits.onepassword7" | "com.1password.1password" | "com.bitwarden.desktop"
        | "org.keepassxc.keepassxc" | "com.dashlane.Dashlane" | "com.lastpass.LastPass" => Err(Status::Secure),
        "com.apple.TextEdit" | "com.apple.iWork.Pages" | "com.apple.mail"
        | "com.apple.Safari" | "com.google.Chrome" | "com.google.Chrome.beta"
        | "org.mozilla.firefox" | "com.microsoft.edgemac" | "com.brave.Browser"
        | "com.microsoft.VSCode" | "com.microsoft.VSCodeInsiders" | "com.todesktop.230313mzl4w4u92"
        | "com.tinyspeck.slackmacgap" | "com.hnc.Discord" | "com.microsoft.teams2"
        | "com.microsoft.Word" | "com.microsoft.Outlook" | "com.apple.Notes"
        | "notion.id" | "md.obsidian" | "com.sublimetext.4" | "com.panic.Nova"
        | "dev.zed.Zed" | "com.apple.MobileSMS" | "net.whatsapp.WhatsApp"
        // AI assistants and agent apps: where prompts are dictated.
        | "com.openai.chat" | "com.openai.codex" | "com.anthropic.claudefordesktop"
        | "com.conductor.app" | "so.stencil.tern" | "at.studio.AsideBrowser" => Ok(()),
        _ => Err(Status::Denied),
    }
}

fn frontmost(budget: &Budget<'_>) -> Result<(i32, String), Status> {
    budget.call(|| {
        objc2::rc::autoreleasepool(|_| {
            let workspace = NSWorkspace::sharedWorkspace();
            // SAFETY: these public NSWorkspace/NSRunningApplication selectors
            // have the declared Objective-C return types. Dynamic messaging
            // avoids requiring additional objc2-app-kit feature dependencies.
            let app: Option<Retained<AnyObject>> = unsafe { msg_send![&*workspace, frontmostApplication] };
            let app = app.ok_or(Status::Denied)?;
            let pid: i32 = unsafe { msg_send![&*app, processIdentifier] };
            let bundle: Option<Retained<NSString>> = unsafe { msg_send![&*app, bundleIdentifier] };
            let bundle = bundle.ok_or(Status::Denied)?.to_string();
            app_policy(&bundle)?;
            Ok((pid, bundle))
        })
    })?
}

fn snapshot(budget: &Budget<'_>) -> Result<Target, Status> {
    budget.security()?;
    let (pid, bundle) = frontmost(budget)?;
    // Ask the frontmost application's own AX element. The system-wide element
    // answers kAXErrorCannotComplete for focus queries under a short messaging
    // timeout (observed with TextEdit), which would make every capture time out.
    // SAFETY: Create returns an owned AX application element for a live pid.
    let app = Owned::new(budget.call(|| unsafe { AXUIElementCreateApplication(pid) })?)?;
    let window = budget.attribute(&app, c"AXFocusedWindow")?;
    let element = budget.attribute(&app, c"AXFocusedUIElement")?;
    if budget.pid(&element)? != pid {
        return Err(Status::TargetChanged);
    }
    if !budget.attribute(&element, c"AXWindow")?.equals(&window) {
        return Err(Status::TargetChanged);
    }
    budget.field_security(&element)?;
    let range = budget.range(&element)?;
    budget.security()?;
    Ok(Target { pid, bundle, window, element, range })
}

fn recheck(target: &Target, budget: &Budget<'_>) -> Result<(), Status> {
    let current = snapshot(budget)?;
    if current.pid != target.pid || current.bundle != target.bundle
        || !current.window.equals(&target.window) || !current.element.equals(&target.element)
        || current.range != target.range
    {
        return Err(Status::TargetChanged);
    }
    budget.security()
}

pub(super) fn capture(
    local: bool,
    deadline: Instant,
    cancelled: &AtomicBool,
) -> Result<Captured, Status> {
    let budget = Budget { deadline, cancelled };
    let target = snapshot(&budget)?;
    // UTF-16 units are at least as numerous as Unicode scalar values. Reading
    // at most the character cap also bounds the transient AX response itself.
    let max_units = if local { 256 } else { 1000 };
    let before_range = Range {
        location: target.range.location - target.range.location.min(max_units),
        length: target.range.location.min(max_units),
    };
    let selected_range = Range {
        location: target.range.location,
        length: target.range.length.min(max_units),
    };
    // Security is complete before either bounded text IPC; a final complete
    // snapshot is required even if only one of the reads succeeds.
    let before = match budget.text(&target.element, before_range) {
        Ok(text) => Ok(text),
        Err(Status::Unsupported) => Err(Status::Unsupported),
        Err(error) => return Err(error),
    };
    let selected = match budget.text(&target.element, selected_range) {
        Ok(text) => Ok(text),
        Err(Status::Unsupported) => Err(Status::Unsupported),
        Err(error) => return Err(error),
    };
    recheck(&target, &budget)?;
    let mut partial = target.range.length > selected_range.length;
    let mut usable = false;
    let mut accept = |result: Result<String, Status>| match result {
        Ok(text) => { usable = true; Ok(text) }
        Err(Status::Unsupported) => { partial = true; Ok(String::new()) }
        Err(error) => Err(error),
    };
    let before = accept(before)?;
    let selected = accept(selected)?;
    if !usable {
        return Err(Status::Unsupported);
    }
    let reference = ContextReference::bounded(&before, &selected, local);
    budget.check()?;
    Ok(Captured { target, reference, partial })
}

pub(super) fn validate(target: &Target, deadline: Instant) -> Result<(), Status> {
    let cancelled = AtomicBool::new(false);
    recheck(target, &Budget { deadline, cancelled: &cancelled })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_apps_and_lookalike_bundle_ids_fail_closed() {
        assert!(matches!(app_policy("com.example.editor"), Err(Status::Denied)));
        assert!(matches!(app_policy("com.apple.TextEdit.evil"), Err(Status::Denied)));
        assert!(matches!(app_policy("com.1password.1password"), Err(Status::Secure)));
        assert!(matches!(app_policy("com.apple.Terminal"), Err(Status::Unsupported)));
        assert!(app_policy("com.apple.TextEdit").is_ok());
        assert!(app_policy("com.tinyspeck.slackmacgap").is_ok());
    }

    #[test]
    fn malformed_ax_ranges_are_rejected() {
        assert!(!valid_range(Range { location: -1, length: 1 }));
        assert!(!valid_range(Range { location: 1, length: -1 }));
        assert!(!valid_range(Range { location: isize::MAX, length: 1 }));
        assert!(valid_range(Range { location: 0, length: 0 }));
    }

    #[test]
    fn utf16_boundaries_do_not_invent_replacement_characters() {
        assert_eq!(decode_bounded(&[0xdc00, 0x61, 0xd800]).ok().as_deref(), Some("a"));
        assert_eq!(decode_bounded(&[0xd83d, 0xde00]).ok().as_deref(), Some("😀"));
        assert!(decode_bounded(&[0x61, 0xdc00, 0x62]).is_err());
        assert_eq!(decode_bounded(&[0xd800]).ok().as_deref(), Some(""));
        assert_eq!(decode_bounded(&[]).ok().as_deref(), Some(""));
    }

    #[test]
    fn cancelled_and_expired_budgets_never_dispatch() {
        let cancelled = AtomicBool::new(true);
        let budget = Budget { deadline: Instant::now() + std::time::Duration::from_secs(1), cancelled: &cancelled };
        assert!(matches!(budget.call(|| panic!("cancelled native operation dispatched")), Err(Status::Timeout)));
        cancelled.store(false, Ordering::Release);
        let budget = Budget { deadline: Instant::now(), cancelled: &cancelled };
        assert!(matches!(budget.call(|| panic!("expired native operation dispatched")), Err(Status::Timeout)));
    }

    #[test]
    fn retained_target_can_cross_worker_boundary() {
        fn assert_send<T: Send>() {}
        assert_send::<Target>();
    }
}
