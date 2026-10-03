//! Pointer/tray starts use toggle semantics regardless of the configured hotkey mode.
#[derive(Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartSource {
    #[default]
    Hotkey,
    Pointer,
    Tray,
}
impl StartSource {
    pub fn blocked_after_release(self, push_to_talk: bool, key_held: bool) -> bool {
        self == Self::Hotkey && push_to_talk && !key_held
    }
    pub fn is_toggle(self) -> bool {
        self != Self::Hotkey
    }
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
        }
    }
}
