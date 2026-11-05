//! Exclusive tap/registered-hotkey routing. No protected key data enters here.
use std::collections::HashSet;

use crate::{KeyPhase, TriggerEvent};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SecureInputState {
    pub enabled: bool,
    pub epoch: u64,
}

impl SecureInputState {
    pub(crate) fn packed(self) -> u64 {
        (self.epoch << 1) | u64::from(self.enabled)
    }
    pub(crate) fn unpack(value: u64) -> Self {
        Self {
            enabled: value & 1 != 0,
            epoch: value >> 1,
        }
    }
}

#[derive(Default)]
pub struct SecureInputGate {
    pub state: SecureInputState,
    active: HashSet<String>,
}

impl SecureInputGate {
    pub(crate) fn new(state: SecureInputState) -> Self {
        Self {
            state,
            ..Self::default()
        }
    }

    pub fn transition(&mut self, enabled: bool) -> bool {
        if self.state.enabled == enabled {
            return false;
        }
        self.state = SecureInputState {
            enabled,
            epoch: self.state.epoch + 1,
        };
        true
    }

    pub fn accepts_raw(&self) -> bool {
        !self.state.enabled
    }

    /// Invalidate every outstanding registration before the host replaces it.
    pub fn reconfigure(&mut self) -> bool {
        if !self.state.enabled {
            return false;
        }
        self.state.epoch += 1;
        true
    }

    pub fn accepts_fallback(&mut self, event: &TriggerEvent, epoch: u64) -> bool {
        if !self.state.enabled || self.state.epoch != epoch {
            return false;
        }
        match event.phase {
            KeyPhase::Pressed => self.active.insert(event.id.clone()),
            KeyPhase::Released => self.active.remove(&event.id),
        }
    }

    pub fn release_all(&mut self, emit: &mut dyn FnMut(TriggerEvent)) {
        for id in self.active.drain() {
            emit(TriggerEvent {
                id,
                phase: KeyPhase::Released,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transitions_exclude_tap_and_stale_or_duplicate_registered_events() {
        let mut gate = SecureInputGate::default();
        let press = TriggerEvent {
            id: "primary".into(),
            phase: KeyPhase::Pressed,
        };
        let release = TriggerEvent {
            phase: KeyPhase::Released,
            ..press.clone()
        };
        assert!(gate.accepts_raw());
        assert!(!gate.accepts_fallback(&press, 0));
        assert!(gate.transition(true));
        assert!(!gate.transition(true));
        assert!(!gate.accepts_raw());
        assert!(gate.accepts_fallback(&press, 1));
        assert!(!gate.accepts_fallback(&press, 1));
        assert!(gate.accepts_fallback(&release, 1));
        assert!(!gate.accepts_fallback(&release, 1));
        assert!(gate.accepts_fallback(&press, 1));
        assert!(gate.transition(false));
        let mut released = vec![];
        gate.release_all(&mut |event| released.push(event));
        assert_eq!(released, vec![release]);
        assert!(gate.accepts_raw());
        assert!(!gate.accepts_fallback(&press, 1));
        assert!(gate.transition(true));
        assert!(!gate.accepts_fallback(&press, 1));
        assert!(gate.accepts_fallback(&press, 3));
    }

    #[test]
    fn replacing_a_held_registration_releases_and_rejects_its_late_events() {
        let mut gate = SecureInputGate::default();
        assert!(!gate.reconfigure());
        gate.transition(true);
        let press = TriggerEvent {
            id: "primary".into(),
            phase: KeyPhase::Pressed,
        };
        assert!(gate.accepts_fallback(&press, 1));
        assert!(gate.reconfigure());
        let mut released = vec![];
        gate.release_all(&mut |event| released.push(event));
        assert_eq!(
            released,
            vec![TriggerEvent {
                id: "primary".into(),
                phase: KeyPhase::Released
            }]
        );
        assert!(!gate.accepts_fallback(&press, 1));
        assert!(gate.accepts_fallback(&press, 2));
    }
}
