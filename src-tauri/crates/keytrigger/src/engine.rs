//! Engine orchestration: a backend [`KeyEventSource`] thread feeds normalized
//! events over a channel to a dispatcher thread that owns the [`Matcher`] and
//! invokes the caller's `on_event`. No lock is ever taken on the OS-callback
//! path — backends only send on the channel.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;
use parking_lot::Mutex;

use crate::backend::platform_source;
use crate::matcher::Matcher;
use crate::types::{
    EngineError, KeySpec, ModSet, NamedKey, RawKeyEvent, Trigger, TriggerEvent, TriggerId,
};

/// Messages flowing to the dispatcher thread. Raw events and control messages
/// share one channel so they are processed in order.
pub enum Msg {
    Raw(RawKeyEvent),
    Control(Control),
}

/// Out-of-band control for the dispatcher.
pub enum Control {
    /// Event stream had a gap (tap re-enabled / session transition): reset the
    /// matcher, synthesizing `Released` for any active trigger.
    ReEnable,
    /// Swap the binding set.
    SetBindings(Vec<(TriggerId, Trigger)>),
    /// Reset the matcher and stop the dispatcher loop.
    Stop,
}

/// One-shot readiness signal a backend fires once its loop is live (so `start`
/// does not race the OS hook/tap installation).
pub struct ReadySignal {
    tx: Sender<Result<(), String>>,
}

impl ReadySignal {
    /// Signal that the backend loop is live and receiving events.
    pub fn ok(self) {
        let _ = self.tx.send(Ok(()));
    }
    /// Signal that the backend failed to start; surfaced as `EngineError`.
    pub fn err(self, msg: impl Into<String>) {
        let _ = self.tx.send(Err(msg.into()));
    }
}

/// The set of key/modifier combinations the engine currently CONSUMES
/// (swallows) at the OS level, shared lock-free with the event source via an
/// [`ArcSwap`]. It is derived from the active [`Trigger::ComboExact`] and
/// [`Trigger::SingleKey`] bindings, but binding swaps intentionally bias it to
/// a subset of the matcher's bindings so a race passes through instead of
/// consuming without a matching trigger. The pure [`ConsumeSet::consumes`]
/// predicate runs on the hot OS-callback path with no lock.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConsumeSet {
    /// `(mods, key)` combos consumed on an exact-modifier key-down.
    pub combos: Vec<(ModSet, KeySpec)>,
    /// Bare keys consumed only when NO modifiers are held.
    pub singles: Vec<KeySpec>,
}
/// Whether `key` is a bare (side-specific) modifier key. Such keys must NEVER be
/// swallowed at the OS level: consuming Control/Shift/Alt/Win would globally
/// break shortcuts like Ctrl+C/V. The OS backends additionally guard modifier
/// virtual-key codes at the hook, but excluding bare-modifier `SingleKey`
/// bindings here ensures the matcher's own consume set cannot resurrect the bug
/// via a stray binding.
fn is_modifier_key(key: KeySpec) -> bool {
    matches!(
        key,
        KeySpec::Named(NamedKey::AltLeft)
            | KeySpec::Named(NamedKey::AltRight)
            | KeySpec::Named(NamedKey::ControlLeft)
            | KeySpec::Named(NamedKey::ControlRight)
            | KeySpec::Named(NamedKey::MetaLeft)
            | KeySpec::Named(NamedKey::MetaRight)
            | KeySpec::Named(NamedKey::ShiftLeft)
            | KeySpec::Named(NamedKey::ShiftRight)
    )
}

impl ConsumeSet {
    /// Build the consume set from a binding list (ignores non-consuming triggers).
    pub fn from_bindings(bindings: &[(TriggerId, Trigger)]) -> Self {
        let mut combos = Vec::new();
        let mut singles = Vec::new();
        for (_, trig) in bindings {
            match trig {
                Trigger::ComboExact { mods, key } => combos.push((*mods, *key)),
                // Belt-and-suspenders: a bare-modifier SingleKey never enters
                // the consume set. Consuming Control/Shift/Alt/Win would
                // globally break Ctrl+C/V etc.; the OS backends also guard
                // modifier VKs at the hook, but filtering here means a stray
                // bare-modifier binding can't be swallowed even transiently.
                Trigger::SingleKey { key } if !is_modifier_key(*key) => {
                    singles.push(*key);
                }
                _ => {}
            }
        }
        Self { combos, singles }
    }

    /// Keep only specs that are consumed by both sets.
    ///
    /// Used during binding swaps to shrink the live OS-level consume set before
    /// the dispatcher installs the new matcher: removed specs stop being
    /// swallowed immediately, while newly-added specs remain pass-through until
    /// the matcher is ready to fire them.
    pub fn intersect(&self, other: &Self) -> Self {
        let combos = self
            .combos
            .iter()
            .copied()
            .filter(|combo| other.combos.contains(combo))
            .collect();
        let singles = self
            .singles
            .iter()
            .copied()
            .filter(|key| other.singles.contains(key))
            .collect();
        Self { combos, singles }
    }

    /// Pure consume predicate (no I/O): consume iff an exact `(mods, key)` combo
    /// matches, or a single key matches with zero modifiers held. Superset
    /// modifiers, modifier-qualified single keys, and everything else pass
    /// through unconsumed.
    pub fn consumes(&self, key: KeySpec, mods: ModSet) -> bool {
        if self.combos.iter().any(|(m, k)| *k == key && *m == mods) {
            return true;
        }
        mods.is_empty() && self.singles.contains(&key)
    }
}

/// A source of raw key events (an OS backend, or a test mock).
pub trait KeyEventSource: Send + Sync {
    /// Run the event loop until [`KeyEventSource::request_stop`]; send events on
    /// `tx` and call `ready.ok()` (or `ready.err(..)`) when the loop is live or
    /// has failed to start. `consume` is the lock-free consume set derived from
    /// the matcher's binding set: the source consumes (swallows) any OS event
    /// whose `(key, mods)` the predicate accepts, so combo/single-key triggers
    /// do not reach the focused application; everything else passes through
    /// untouched. During swaps this may be a conservative subset so removed
    /// bindings pass through instead of being swallowed without a match.
    fn run(&self, tx: Sender<Msg>, ready: ReadySignal, consume: Arc<ArcSwap<ConsumeSet>>);
    /// Ask the running loop to terminate (from another thread).
    fn request_stop(&self);
}

/// A scripted source for tests: emits a fixed sequence of messages, then parks
/// until stopped.
pub struct MockSource {
    script: Mutex<Option<Vec<(Msg, Duration)>>>,
    stop: AtomicBool,
}

impl MockSource {
    pub fn new(script: Vec<(Msg, Duration)>) -> Self {
        Self {
            script: Mutex::new(Some(script)),
            stop: AtomicBool::new(false),
        }
    }
}

impl KeyEventSource for MockSource {
    fn run(&self, tx: Sender<Msg>, ready: ReadySignal, _consume: Arc<ArcSwap<ConsumeSet>>) {
        ready.ok();
        let script = self.script.lock().take().unwrap_or_default();
        for (msg, delay) in script {
            if self.stop.load(Ordering::Relaxed) {
                return;
            }
            if !delay.is_zero() {
                std::thread::sleep(delay);
            }
            if tx.send(msg).is_err() {
                return;
            }
        }
        while !self.stop.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn request_stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

struct Inner {
    tx: Sender<Msg>,
    source: Arc<dyn KeyEventSource>,
    dispatcher: JoinHandle<()>,
    source_thread: JoinHandle<()>,
}

/// The trigger engine. One instance per process.
pub struct TriggerEngine {
    inner: Mutex<Option<Inner>>,
    pending: Mutex<Vec<(TriggerId, Trigger)>>,
    running: AtomicBool,
    consume: Arc<ArcSwap<ConsumeSet>>,
}

impl Default for TriggerEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TriggerEngine {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(None),
            pending: Mutex::new(Vec::new()),
            running: AtomicBool::new(false),
            consume: Arc::new(ArcSwap::from_pointee(ConsumeSet::default())),
        }
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    /// Set (or replace) the active bindings. Safe to call before `start` (stored
    /// and applied on start) or while running (applied in-order with events).
    ///
    /// The OS callback decides pass-through/consume synchronously from
    /// `self.consume`, while the matcher sees the queued raw event later on the
    /// dispatcher. Before queuing a live swap, shrink consume to
    /// `current ∩ next`: removed bindings become pass-through immediately, and
    /// additions are not consumed until the dispatcher has installed the matcher.
    pub fn set_bindings(&self, bindings: Vec<(TriggerId, Trigger)>) {
        *self.pending.lock() = bindings.clone();
        if let Some(inner) = self.inner.lock().as_ref() {
            let next_consume = ConsumeSet::from_bindings(&bindings);
            let current_consume = self.consume.load();
            self.consume
                .store(Arc::new(current_consume.intersect(&next_consume)));
            let _ = inner.tx.send(Msg::Control(Control::SetBindings(bindings)));
        }
    }

    /// Start with the platform backend.
    pub fn start(
        &self,
        on_event: impl Fn(TriggerEvent) + Send + 'static,
    ) -> Result<(), EngineError> {
        self.start_with_source(Arc::from(platform_source()), on_event)
    }

    /// Start with an explicit source (used by the platform `start` and by tests).
    pub fn start_with_source(
        &self,
        source: Arc<dyn KeyEventSource>,
        on_event: impl Fn(TriggerEvent) + Send + 'static,
    ) -> Result<(), EngineError> {
        let mut guard = self.inner.lock();
        if guard.is_some() {
            return Err(EngineError::AlreadyRunning);
        }

        let (tx, rx) = mpsc::channel::<Msg>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();
        let initial = self.pending.lock().clone();
        let consume_for_dispatch = Arc::clone(&self.consume);
        let consume_for_source = Arc::clone(&self.consume);

        let dispatcher = std::thread::Builder::new()
            .name("keytrigger-dispatch".into())
            .spawn(move || {
                let mut matcher = Matcher::new();
                let mut emit = |te: TriggerEvent| {
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| on_event(te)));
                };
                let initial_consume = ConsumeSet::from_bindings(&initial);
                matcher.set_bindings(initial, &mut emit);
                consume_for_dispatch.store(Arc::new(initial_consume));
                while let Ok(msg) = rx.recv() {
                    match msg {
                        Msg::Raw(ev) => matcher.handle(ev, Instant::now(), &mut emit),
                        Msg::Control(Control::SetBindings(b)) => {
                            let next_consume = ConsumeSet::from_bindings(&b);
                            matcher.set_bindings(b, &mut emit);
                            // Matcher first, consume second: additions become
                            // swallowable only after they can fire.
                            consume_for_dispatch.store(Arc::new(next_consume));
                        }
                        Msg::Control(Control::ReEnable) => matcher.reset(&mut emit),
                        Msg::Control(Control::Stop) => {
                            matcher.reset(&mut emit);
                            break;
                        }
                    }
                }
            })
            .map_err(|e| EngineError::Backend(format!("spawn dispatcher: {e}")))?;

        let src_for_thread = Arc::clone(&source);
        let tx_for_source = tx.clone();
        let ready = ReadySignal { tx: ready_tx };
        let source_thread = match std::thread::Builder::new()
            .name("keytrigger-source".into())
            .spawn(move || {
                src_for_thread.run(tx_for_source, ready, consume_for_source);
            }) {
            Ok(h) => h,
            Err(e) => {
                // Unwind the dispatcher we already spawned.
                let _ = tx.send(Msg::Control(Control::Stop));
                let _ = dispatcher.join();
                return Err(EngineError::Backend(format!("spawn source: {e}")));
            }
        };

        // Bounded wait for the backend to come up (Windows queue handshake / tap
        // creation). A timeout, disconnect, or explicit failure aborts start and
        // tears down the threads we already spawned.
        match ready_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                let _ = tx.send(Msg::Control(Control::Stop));
                let _ = dispatcher.join();
                source.request_stop();
                let _ = source_thread.join();
                return Err(EngineError::Backend(e));
            }
            Err(_) => {
                let _ = tx.send(Msg::Control(Control::Stop));
                let _ = dispatcher.join();
                source.request_stop();
                let _ = source_thread.join();
                return Err(EngineError::Backend(
                    "backend did not become ready".to_string(),
                ));
            }
        }

        *guard = Some(Inner {
            tx,
            source,
            dispatcher,
            source_thread,
        });
        self.running.store(true, Ordering::Relaxed);
        Ok(())
    }

    /// Stop the engine: reset the matcher (synth-release), stop the backend, and
    /// join both threads. Idempotent.
    pub fn stop(&self) {
        let inner = self.inner.lock().take();
        if let Some(inner) = inner {
            let _ = inner.tx.send(Msg::Control(Control::Stop));
            inner.source.request_stop();
            let _ = inner.dispatcher.join();
            let _ = inner.source_thread.join();
        }
        self.running.store(false, Ordering::Relaxed);
    }
}

impl Drop for TriggerEngine {
    fn drop(&mut self) {
        if self.running.load(Ordering::Relaxed) {
            self.stop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{KeyPhase, KeySpec, Modifier, NamedKey, Side};

    fn raw(key: NamedKey, side: Option<Side>, down: bool) -> Msg {
        Msg::Raw(RawKeyEvent {
            key: KeySpec::Named(key),
            side,
            down,
            is_repeat: false,
        })
    }

    #[test]
    fn engine_mock_hold_end_to_end() {
        let engine = TriggerEngine::new();
        engine.set_bindings(vec![(
            "hold".into(),
            Trigger::ModifierHold {
                modifier: Modifier::Alt,
                side: Side::Right,
            },
        )]);

        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&events);

        let script = vec![
            (
                raw(NamedKey::AltRight, Some(Side::Right), true),
                Duration::from_millis(0),
            ),
            (
                raw(NamedKey::AltRight, Some(Side::Right), false),
                Duration::from_millis(10),
            ),
        ];
        let source: Arc<dyn KeyEventSource> = Arc::new(MockSource::new(script));

        engine
            .start_with_source(source, move |te| sink.lock().push(te))
            .expect("start");

        // Allow the scripted events to flow through the dispatcher.
        std::thread::sleep(Duration::from_millis(150));
        engine.stop();

        let got = events.lock().clone();
        assert_eq!(
            got,
            vec![
                TriggerEvent {
                    id: "hold".into(),
                    phase: KeyPhase::Pressed,
                },
                TriggerEvent {
                    id: "hold".into(),
                    phase: KeyPhase::Released,
                },
            ]
        );
        assert!(!engine.is_running());
    }

    #[test]
    fn concurrent_start_installs_only_one_event_source() {
        let engine = Arc::new(TriggerEngine::new());
        let barrier = Arc::new(std::sync::Barrier::new(3));

        let start = |engine: Arc<TriggerEngine>, barrier: Arc<std::sync::Barrier>| {
            std::thread::spawn(move || {
                barrier.wait();
                engine.start_with_source(Arc::new(MockSource::new(Vec::new())), |_| {})
            })
        };

        let first = start(Arc::clone(&engine), Arc::clone(&barrier));
        let second = start(Arc::clone(&engine), Arc::clone(&barrier));
        barrier.wait();

        let outcomes = [
            first.join().expect("first starter"),
            second.join().expect("second starter"),
        ];
        assert_eq!(outcomes.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            outcomes
                .iter()
                .filter(|result| matches!(result, Err(EngineError::AlreadyRunning)))
                .count(),
            1
        );

        engine.stop();
        assert!(!engine.is_running());
    }

    fn j() -> KeySpec {
        KeySpec::Named(NamedKey::J)
    }

    #[test]
    fn consume_set_intersect_keeps_only_shared_specs() {
        let meta_j = ModSet::empty().with(Modifier::Meta);
        let current = ConsumeSet::from_bindings(&[
            (
                "survives".into(),
                Trigger::ComboExact {
                    mods: meta_j,
                    key: j(),
                },
            ),
            (
                "removed".into(),
                Trigger::SingleKey {
                    key: KeySpec::Named(NamedKey::Escape),
                },
            ),
        ]);
        let next = ConsumeSet::from_bindings(&[
            (
                "survives-renamed".into(),
                Trigger::ComboExact {
                    mods: meta_j,
                    key: j(),
                },
            ),
            (
                "added".into(),
                Trigger::SingleKey {
                    key: KeySpec::Named(NamedKey::K),
                },
            ),
        ]);

        let intersect = current.intersect(&next);

        assert_eq!(
            intersect,
            ConsumeSet {
                combos: vec![(meta_j, j())],
                singles: vec![],
            }
        );
        assert!(intersect.consumes(j(), meta_j));
        assert!(!intersect.consumes(KeySpec::Named(NamedKey::Escape), ModSet::empty()));
        assert!(!intersect.consumes(KeySpec::Named(NamedKey::K), ModSet::empty()));
    }
    #[test]
    fn consume_set_exact_combo_consumes() {
        let set = ConsumeSet::from_bindings(&[(
            "ce".into(),
            Trigger::ComboExact {
                mods: ModSet::empty().with(Modifier::Meta).with(Modifier::Shift),
                key: j(),
            },
        )]);
        // exact mods + matching key → consume
        assert!(set.consumes(
            j(),
            ModSet::empty().with(Modifier::Meta).with(Modifier::Shift),
        ));
    }

    #[test]
    fn consume_set_superset_and_subset_combo_do_not_consume() {
        let set = ConsumeSet::from_bindings(&[(
            "ce".into(),
            Trigger::ComboExact {
                mods: ModSet::empty().with(Modifier::Meta),
                key: j(),
            },
        )]);
        // SUPERSET (Cmd+Shift) → must NOT consume (exact match only)
        assert!(!set.consumes(
            j(),
            ModSet::empty().with(Modifier::Meta).with(Modifier::Shift),
        ));
        // subset (no mods) → must NOT consume
        assert!(!set.consumes(j(), ModSet::empty()));
        // wrong key → must NOT consume
        assert!(!set.consumes(
            KeySpec::Named(NamedKey::K),
            ModSet::empty().with(Modifier::Meta),
        ));
    }

    #[test]
    fn consume_set_single_key_consumes_only_with_zero_mods() {
        let set = ConsumeSet::from_bindings(&[(
            "s".into(),
            Trigger::SingleKey {
                key: KeySpec::Named(NamedKey::Escape),
            },
        )]);
        // zero mods → consume
        assert!(set.consumes(KeySpec::Named(NamedKey::Escape), ModSet::empty()));
        // any modifier held → must NOT consume
        assert!(!set.consumes(
            KeySpec::Named(NamedKey::Escape),
            ModSet::empty().with(Modifier::Shift),
        ));
    }

    #[test]
    fn consume_set_empty_never_consumes() {
        let set = ConsumeSet::default();
        assert!(!set.consumes(j(), ModSet::empty()));
        assert!(!set.consumes(j(), ModSet::empty().with(Modifier::Meta)));
    }
    #[test]
    fn consume_set_filters_bare_modifier_single_keys() {
        // Bare-modifier SingleKeys must NOT enter the consume set: consuming a
        // modifier would globally swallow Control/Shift/Alt/Win (Ctrl+C/V etc.).
        let set = ConsumeSet::from_bindings(&[
            (
                "blocked-ctrl".into(),
                Trigger::SingleKey {
                    key: KeySpec::Named(NamedKey::ControlLeft),
                },
            ),
            (
                "blocked-alt".into(),
                Trigger::SingleKey {
                    key: KeySpec::Named(NamedKey::AltLeft),
                },
            ),
            (
                "blocked-shift".into(),
                Trigger::SingleKey {
                    key: KeySpec::Named(NamedKey::ShiftRight),
                },
            ),
            (
                "blocked-meta".into(),
                Trigger::SingleKey {
                    key: KeySpec::Named(NamedKey::MetaLeft),
                },
            ),
            (
                "kept".into(),
                Trigger::SingleKey {
                    key: KeySpec::Named(NamedKey::F8),
                },
            ),
        ]);
        // Only the non-modifier single survives into the set.
        assert_eq!(set.singles, vec![KeySpec::Named(NamedKey::F8)]);
        // The filtered modifier is not consumed even with zero mods held.
        assert!(!set.consumes(KeySpec::Named(NamedKey::ControlLeft), ModSet::empty()));
        // The non-modifier single is consumed as before.
        assert!(set.consumes(KeySpec::Named(NamedKey::F8), ModSet::empty()));
    }
}
