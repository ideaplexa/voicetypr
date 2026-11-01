//! UIA objects stay on their owning, short-lived COM MTA thread.
use windows::core::Interface;
use windows::Win32::Foundation::E_NOINTERFACE;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation8, IUIAutomation2, IUIAutomationElement, IUIAutomationTextPattern,
    IUIAutomationTextPattern2, IUIAutomationTextRange, TextPatternRangeEndpoint_End,
    TextPatternRangeEndpoint_Start, TextUnit_Character, UIA_TextPattern2Id, UIA_TextPatternId,
    UIA_E_NOTSUPPORTED,
};

struct ComApartment;
impl Drop for ComApartment {
    fn drop(&mut self) {
        // SAFETY: paired with successful CoInitializeEx on this same thread.
        unsafe { CoUninitialize() };
    }
}

unsafe fn caret(element: &IUIAutomationElement) -> Option<IUIAutomationTextRange> {
    // TextPattern2 inherits GetSelection; TextPattern remains widely supported.
    let pattern: IUIAutomationTextPattern =
        match element.GetCurrentPatternAs::<IUIAutomationTextPattern2>(UIA_TextPattern2Id) {
            Ok(pattern2) => pattern2.cast().ok()?,
            Err(error)
                if error.code() == E_NOINTERFACE || error.code().0 as u32 == UIA_E_NOTSUPPORTED =>
            {
                element.GetCurrentPatternAs(UIA_TextPatternId).ok()?
            }
            Err(_) => return None,
        };
    let ranges = pattern.GetSelection().ok()?;
    if ranges.Length().ok()? != 1 {
        return None;
    }
    let range = ranges.GetElement(0).ok()?;
    (range
        .CompareEndpoints(
            TextPatternRangeEndpoint_Start,
            &range,
            TextPatternRangeEndpoint_End,
        )
        .ok()?
        == 0)
        .then_some(range)
}

pub(super) fn previous_character() -> Option<char> {
    // SAFETY: the dedicated worker has no pre-existing COM apartment; all UIA
    // interfaces and ranges are released before the apartment guard.
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok().ok()?;
        let _apartment = ComApartment;
        let automation: IUIAutomation2 =
            CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER).ok()?;
        // UIA's own IPC limits plus the caller's total 150ms wall-clock budget.
        automation.SetConnectionTimeout(100).ok()?;
        automation.SetTransactionTimeout(100).ok()?;
        let element = automation.GetFocusedElement().ok()?;
        // Failed/unavailable protection status is NOT equivalent to false.
        if element.CurrentIsPassword().ok()?.as_bool()
            || !element.CurrentHasKeyboardFocus().ok()?.as_bool()
        {
            return None;
        }
        let selected = caret(&element)?;
        let preceding = selected.Clone().ok()?;
        if preceding
            .MoveEndpointByUnit(TextPatternRangeEndpoint_Start, TextUnit_Character, -1)
            .ok()?
            != -1
        {
            return None; // No character before the caret.
        }
        // A UIA "character" may be a multi-unit grapheme; reject ambiguous
        // multi-scalar results rather than guessing or reading more text.
        let text = preceding.GetText(8).ok()?;
        let text = String::from_utf16(&text).ok()?;
        let mut characters = text.chars();
        let previous = characters.next()?;
        if characters.next().is_some() {
            return None;
        }
        let focused = automation.GetFocusedElement().ok()?;
        if !automation
            .CompareElements(&element, &focused)
            .ok()?
            .as_bool()
            || element.CurrentIsPassword().ok()?.as_bool()
            || !selected.Compare(&caret(&element)?).ok()?.as_bool()
        {
            return None;
        }
        Some(previous)
    }
}
