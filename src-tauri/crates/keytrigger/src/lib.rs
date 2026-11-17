//! `keytrigger` — a cross-platform, observation-only global key-trigger engine.
//!
//! Detects four kinds of triggers from the global key-event stream and emits
//! [`TriggerEvent`]s for caller-supplied [`TriggerId`]s:
//!
//! - [`Trigger::ModifierHold`] — hold a modifier (optionally side-specific), e.g. hold Right-Option.
//! - [`Trigger::DoubleTap`] — double-tap a key or modifier within a window.
//! - [`Trigger::Chord`] — modifier(s) + key, e.g. Cmd+Shift+Space.
//! - [`Trigger::SingleKey`] — a lone non-modifier key (no modifiers held).
//!
//! The engine never consumes keys (observation-only): triggers fire without
//! preventing the key from reaching the focused application. Multiple triggers
//! may share a key and all fire (non-exclusive).
//!
//! Platform backends: macOS `CGEventTap` (listen-only), Windows `WH_KEYBOARD_LL`.
//! The platform-independent [`matcher::Matcher`] holds all detection logic and is
//! exhaustively unit-tested; backends only normalize raw OS events.

pub mod backend;
pub mod engine;
pub mod matcher;
pub mod secure_input;
pub mod types;
pub use secure_input::SecureInputState;

pub use engine::{ConsumeSet, Control, KeyEventSource, Msg, ReadySignal, TriggerEngine};
pub use matcher::Matcher;
pub use types::{
    EngineError, KeyPhase, KeySpec, ModSet, Modifier, ModifierSpec, NamedKey, RawKeyEvent, Side,
    TapKey, Trigger, TriggerEvent, TriggerId,
};

/// Sentinel written to a synthetic keystroke's `dwExtraInfo` (Windows) to mark it
/// as Voicetypr's OWN injection — currently the post-transcription Ctrl+V paste in
/// `commands::text::paste_windows`. The low-level keyboard hook ignores keystrokes
/// carrying this value (so our paste can't re-trigger a hotkey), while external
/// tools' injected input (Stream Deck, AutoHotkey, PowerToys) carries a different
/// `dwExtraInfo` and passes through to trigger hotkeys normally. Value is arbitrary
/// but must be non-zero; both the injector and the hook reference this one constant.
pub const INJECTED_SIGNATURE: usize = 0x5654_5950; // "VTYP"

static TEXT_KEY_DOWNS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// How many non-modifier key-downs (repeats included) from anyone but
/// Voicetypr have reached the focused app since the backend started. Only a
/// count: no key is recorded. Keys consumed as a trigger are not counted, and
/// neither are modifier changes, so pressing a dictation hotkey does not move
/// it; typing, Backspace, arrows and shortcuts do. "Delete that" uses it to
/// know nothing was typed since the last dictation.
pub fn text_key_downs() -> u64 {
    TEXT_KEY_DOWNS.load(std::sync::atomic::Ordering::Acquire)
}

pub(crate) fn note_text_key_down() {
    TEXT_KEY_DOWNS.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
}
