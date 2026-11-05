//! macOS public registered-hotkey fallback. Only runs on the native main loop.
//! No raw keys, app identities, owner PIDs or window contents are inspected.
use global_hotkey::{
    hotkey::{HotKey, Modifiers},
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
};
use keytrigger::{KeyPhase, KeySpec, Modifier, NamedKey, Trigger, TriggerEvent};
use std::{
    cell::RefCell,
    sync::{
        atomic::{AtomicU32, Ordering},
        Mutex,
    },
};
use tauri::{AppHandle, Manager};

static DESIRED: Mutex<Vec<(String, Trigger)>> = Mutex::new(Vec::new());
static NEXT_ID: AtomicU32 = AtomicU32::new(1);
struct Registered {
    manager: GlobalHotKeyManager,
    epoch: u64,
    desired: Vec<(String, Trigger)>,
    keys: Vec<(HotKey, String)>,
}
impl Drop for Registered {
    fn drop(&mut self) {
        for (key, _) in &self.keys {
            if self.manager.unregister(*key).is_err() {
                log::warn!("Secure Input: fallback unregister failed");
            }
        }
    }
}
thread_local! { static REGISTERED: RefCell<Option<Registered>> = const { RefCell::new(None) }; }

pub(crate) fn bindings(app: &AppHandle, triggers: &[(String, Trigger)]) {
    let desired: Vec<_> = triggers
        .iter()
        .filter(|(_, t)| hotkey(*t).is_some())
        .cloned()
        .collect();
    let mut previous = DESIRED.lock().unwrap_or_else(|e| e.into_inner());
    if *previous == desired {
        return;
    }
    *previous = desired;
    drop(previous);
    // Ordered BEFORE any replacement registration can emit: retire its epoch,
    // clear held ownership and request a stop, even for a same-id chord edit.
    app.state::<crate::AppState>()
        .trigger_engine
        .reconfigure_fallback();
    refresh(app);
}

/// Conservative first subset: Command/Control/Option plus Space, letter, digit
/// or F1-F20, with optional Shift. Shift-only typing chords stay unsupported.
/// Never register bare keys/modifiers, isolated taps, or clipboard actions.
fn hotkey(trigger: Trigger) -> Option<HotKey> {
    let Trigger::ComboExact {
        mods,
        key: KeySpec::Named(key),
    } = trigger
    else {
        return None;
    };
    if ![Modifier::Alt, Modifier::Control, Modifier::Meta]
        .iter()
        .any(|m| mods.contains(*m))
    {
        return None;
    }
    let name = format!("{key:?}");
    let code = match key {
        NamedKey::Space => "Space".to_string(),
        _ if name.len() == 1 && name.as_bytes()[0].is_ascii_uppercase() => format!("Key{name}"),
        _ if name.starts_with("Digit") => name,
        _ if name
            .strip_prefix('F')
            .and_then(|n| n.parse::<u8>().ok())
            .is_some_and(|n| (1..=20).contains(&n)) =>
        {
            name
        }
        _ => return None,
    }
    .parse()
    .ok()?;
    let mut mapped = Modifiers::empty();
    for (source, target) in [
        (Modifier::Alt, Modifiers::ALT),
        (Modifier::Control, Modifiers::CONTROL),
        (Modifier::Meta, Modifiers::META),
        (Modifier::Shift, Modifiers::SHIFT),
    ] {
        if mods.contains(source) {
            mapped.insert(target);
        }
    }
    Some(HotKey::new(Some(mapped), code))
}

pub(crate) fn refresh(app: &AppHandle) {
    let app = app.clone();
    let dispatch = app.clone();
    let _ = dispatch.run_on_main_thread(move || reconcile(&app));
}

fn reconcile(app: &AppHandle) {
    let state = app
        .state::<crate::AppState>()
        .trigger_engine
        .secure_input_state();
    let desired = DESIRED.lock().unwrap_or_else(|e| e.into_inner()).clone();
    REGISTERED.with(|slot| {
        let mut slot = slot.borrow_mut();
        if !state.enabled {
            slot.take();
            crate::commands::island_notice::clear(
                app,
                crate::commands::island_notice::NoticeKind::SecureInput,
            );
            return;
        }
        if slot
            .as_ref()
            .is_some_and(|old| old.epoch == state.epoch && old.desired == desired)
        {
            return;
        }
        slot.take();
        let app_events = app.clone();
        // The library's handler is installed once. Carbon invokes it on main.
        GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
            REGISTERED.with(|slot| {
                let slot = slot.borrow();
                let Some(registered) = slot.as_ref() else {
                    return;
                };
                let Some((_, id)) = registered.keys.iter().find(|(key, _)| key.id() == event.id)
                else {
                    return;
                };
                let phase = if event.state == HotKeyState::Pressed {
                    KeyPhase::Pressed
                } else {
                    KeyPhase::Released
                };
                app_events
                    .state::<crate::AppState>()
                    .trigger_engine
                    .fallback_event(
                        TriggerEvent {
                            id: id.clone(),
                            phase,
                        },
                        registered.epoch,
                    );
            });
        }));
        let Ok(manager) = GlobalHotKeyManager::new() else {
            log::warn!("Secure Input: fallback registration unavailable");
            crate::commands::island_notice::notice(
                app,
                crate::commands::island_notice::NoticeKind::SecureInput,
            );
            return;
        };
        let mut keys = vec![];
        for (id, trigger) in &desired {
            let Some(mut key) = hotkey(*trigger) else {
                continue;
            };
            // Never reuse a Carbon event id: delayed events from a retired
            // registration cannot acquire the next route's epoch.
            key.id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
            if manager.register(key).is_ok() {
                keys.push((key, id.clone()));
            } else {
                log::warn!("Secure Input: fallback shortcut unavailable");
            }
        }
        *slot = Some(Registered {
            manager,
            epoch: state.epoch,
            desired,
            keys,
        });
        crate::commands::island_notice::notice(
            app,
            crate::commands::island_notice::NoticeKind::SecureInput,
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fallback_only_registers_exact_modified_supported_keys() {
        let mods = keytrigger::ModSet::empty().with(Modifier::Alt);
        assert!(hotkey(Trigger::ComboExact {
            mods,
            key: KeySpec::Named(NamedKey::Space)
        })
        .is_some());
        for key in [NamedKey::A, NamedKey::Digit1, NamedKey::F20] {
            assert!(hotkey(Trigger::ComboExact {
                mods,
                key: KeySpec::Named(key)
            })
            .is_some());
        }
        for key in [NamedKey::Escape, NamedKey::AltRight, NamedKey::F21] {
            assert!(hotkey(Trigger::ComboExact {
                mods,
                key: KeySpec::Named(key)
            })
            .is_none());
        }
        assert!(hotkey(Trigger::SingleKey {
            key: KeySpec::Named(NamedKey::F8)
        })
        .is_none());
        assert!(hotkey(Trigger::ComboExact {
            mods: keytrigger::ModSet::empty().with(Modifier::Shift),
            key: KeySpec::Named(NamedKey::A),
        })
        .is_none());
        assert!(hotkey(Trigger::ModifierHold {
            modifier: Modifier::Alt,
            side: keytrigger::Side::Right
        })
        .is_none());
    }
}
