//! Pointer/tray starts use toggle semantics regardless of the configured hotkey mode.
#[derive(Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartSource {
    #[default]
    Hotkey,
    // Native hold bindings are hold-to-talk even when the primary is a toggle.
    #[serde(skip)]
    HoldHotkey,
    Pointer,
    Tray,
}
impl StartSource {
    pub fn blocked_after_release(self, push_to_talk: bool, key_held: bool) -> bool {
        !self.is_toggle() && (push_to_talk || self == Self::HoldHotkey) && !key_held
    }
    pub fn is_toggle(self) -> bool {
        matches!(self, Self::Pointer | Self::Tray)
    }
    pub fn telemetry_label(self) -> &'static str {
        match self {
            Self::Hotkey | Self::HoldHotkey => "hotkey",
            Self::Pointer => "pointer",
            Self::Tray => "tray",
        }
    }
}

pub(crate) fn ptt_key_released(app_state: &crate::AppState, source: StartSource) -> bool {
    let mode = match app_state.recording_mode.lock() {
        Ok(guard) => *guard,
        Err(poisoned) => {
            log::warn!("recording_mode mutex poisoned; recovering value for PTT guard");
            *poisoned.into_inner()
        }
    };
    source.blocked_after_release(
        mode == crate::RecordingMode::PushToTalk,
        app_state
            .ptt_key_held
            .load(std::sync::atomic::Ordering::SeqCst),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pointer_and_tray_start_in_both_modes_with_no_key_held() {
        for ptt in [true, false] {
            for source in [StartSource::Pointer, StartSource::Tray] {
                assert!(!source.blocked_after_release(ptt, false));
                assert!(source.is_toggle());
            }
            assert_eq!(StartSource::Hotkey.blocked_after_release(ptt, false), ptt);
            assert!(!StartSource::Hotkey.blocked_after_release(ptt, true));
            assert!(StartSource::HoldHotkey.blocked_after_release(ptt, false));
            assert!(!StartSource::HoldHotkey.blocked_after_release(ptt, true));
            assert!(!StartSource::HoldHotkey.is_toggle());
        }
    }
}
