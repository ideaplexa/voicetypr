import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef } from "react";
import { useSetting } from "@/contexts/SettingsContext";
import { useRecording } from "@/hooks/useRecording";
import { createLogger } from "@/lib/logger";
import { isMacOS } from "@/lib/platform";
import { findActivePrimaryBinding } from "@/lib/shortcut-display";
import { eventMatchesShortcut } from "@/lib/shortcut-event-match";
import type { ModifierSpec, ShortcutBinding, ShortcutSettings } from "@/types/shortcuts";

const log = createLogger("in-app-hotkey");

/**
 * Minimum gap between two in-app toggles. Guards against duplicate keydowns
 * (e.g. WebView2/IME quirks) and the rare case where the OS-level hook also
 * fires for the same press.
 */
const TOGGLE_DEBOUNCE_MS = 300;

/** DOM `KeyboardEvent.code` prefix for each bare-modifier kind. */
const MODIFIER_CODE_PREFIX: Record<ModifierSpec["modifier"], string> = {
  control: "Control",
  alt: "Alt",
  meta: "Meta",
  shift: "Shift",
};

/**
 * True when focus is inside an editable element (text input / textarea /
 * contenteditable) — the ONLY context where the OS-level keytrigger hook can be
 * bypassed by the browser/IME (e.g. Ctrl+Space in our own bug-report box).
 * Restricting the fallback to this case avoids double-triggering everywhere the
 * native engine already handles the hotkey.
 */
function isEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable) return true;
  const tag = target.tagName;
  return tag === "INPUT" || tag === "TEXTAREA";
}

/** True when the event's physical key is the configured bare modifier + side. */
function eventMatchesBareModifier(event: KeyboardEvent, spec: ModifierSpec): boolean {
  const code = event.code || "";
  if (!code.startsWith(MODIFIER_CODE_PREFIX[spec.modifier])) return false;
  if (spec.side === "left") return code.endsWith("Left");
  if (spec.side === "right") return code.endsWith("Right");
  return true; // "either"
}

/** True when a modifier OTHER than the configured one is currently held. */
function hasOtherModifierHeld(event: KeyboardEvent, spec: ModifierSpec): boolean {
  return (
    (spec.modifier !== "control" && event.ctrlKey) ||
    (spec.modifier !== "alt" && event.altKey) ||
    (spec.modifier !== "shift" && event.shiftKey) ||
    (spec.modifier !== "meta" && event.metaKey)
  );
}

function hasAltGraph(event: KeyboardEvent): boolean {
  return event.getModifierState?.("AltGraph") === true;
}

function isAltGraphSecondKey(event: KeyboardEvent): boolean {
  return hasAltGraph(event) || event.code === "AltRight" || event.key === "AltGraph";
}

type BareModifierFallback = {
  modifier: ModifierSpec;
  kind: "isolated_tap" | "modifier_hold";
};

/**
 * Resolve a bare-modifier primary we can emulate with DOM key events:
 * isolated-tap toggles on clean keydown->keyup, while modifier-hold mirrors the
 * native hold_to_record path by starting on keydown and stopping on keyup.
 */
function bareModifierFallback(binding: ShortcutBinding | null): BareModifierFallback | null {
  if (!binding?.modifier) return null;
  if (!binding.enabled) return null;
  if (binding.action === "toggle_recording" && binding.trigger_kind === "isolated_tap") {
    return { modifier: binding.modifier, kind: "isolated_tap" };
  }
  if (binding.action === "hold_to_record" && binding.trigger_kind === "modifier_hold") {
    return { modifier: binding.modifier, kind: "modifier_hold" };
  }
  return null;
}

/**
 * In-app fallback for the recording hotkey.
 *
 * The OS-level `keytrigger` engine starts/stops recording globally, but when
 * focus is inside one of our own WebView2 text fields the browser/IME can
 * swallow the trigger before the low-level hook sees it, so the global hotkey
 * silently does nothing. Because the keys still reach the DOM, this hook
 * re-handles them when an editable element is focused:
 *   - combos (e.g. Ctrl+Space) — matched on keydown against `settings.hotkey`;
 *   - bare-modifier isolated-tap (e.g. Control alone) — a clean modifier
 *     keydown→keyup with no other key, matched against the native primary
 *     binding (which leaves `settings.hotkey` empty).
 *
 * Editable-focus gating is mandatory: a tap fires on key-UP while the native
 * hook starts on key-DOWN, so emulating it where the native hook works would
 * start-then-stop. Mount once in the main window (it has the text inputs).
 */
export function useInAppRecordingHotkey(): void {
  const hotkey = useSetting("hotkey");
  const recording = useRecording();
  const lastToggleRef = useRef(0);

  // Active bare-modifier primary fallback. Loaded only on Windows and only
  // when no combo hotkey is set: macOS's native engine handles bare-modifier
  // bindings in our own windows, where this detector could stutter
  // against it; and a combo primary leaves the bare-modifier binding inactive.
  const bareModifierRef = useRef<BareModifierFallback | null>(null);
  // Monotonic load token: only the most recent reload wins, so an in-flight
  // load whose result resolves late can't clobber a fresher binding (e.g. a
  // combo hotkey just set, or a second shortcut-settings-changed event).
  const bareModifierLoadTokenRef = useRef(0);

  /**
   * Re-resolve the active bare-modifier primary from the backend.
   * No-op on macOS. Also no-op when a combo hotkey is set unless the backend
   * shortcut-settings signal explicitly asks for a reload; that signal can arrive
   * before the cached settings hotkey has refreshed during onboarding's bare-
   * modifier save. Called on mount and whenever `hotkey` changes, and again when
   * the backend reports shortcut settings changed, so an in-session binding save
   * refreshes the cached spec without an app restart.
   */
  const reloadBareModifier = useCallback(
    (currentHotkey: string | undefined, force = false): void => {
      if ((!force && currentHotkey) || isMacOS) {
        bareModifierRef.current = null;
        return;
      }
      const token = ++bareModifierLoadTokenRef.current;
      invoke<ShortcutSettings>("get_shortcut_settings")
        .then((settings) => {
          if (token === bareModifierLoadTokenRef.current) {
            bareModifierRef.current = bareModifierFallback(
              findActivePrimaryBinding(settings.bindings),
            );
          }
        })
        .catch(() => {
          if (token === bareModifierLoadTokenRef.current) {
            bareModifierRef.current = null;
          }
        });
    },
    [],
  );

  useEffect(() => {
    reloadBareModifier(hotkey);
  }, [hotkey, reloadBareModifier]);

  // Keep the latest hotkey + recording controls in a ref so the listeners are
  // bound once and never re-attach on state changes. Updated in an effect (not
  // during render) so reads always see the committed values.
  const latest = useRef({ hotkey, recording });
  useEffect(() => {
    latest.current = { hotkey, recording };
  });

  // Backend signals that shortcut settings were persisted (e.g. an in-session
  // bare-modifier binding change). Re-resolve so the fallback picks up the new
  // spec without a restart. Registered once; the handler reads the current
  // hotkey from `latest` so it never closes over a stale combo value.
  useEffect(() => {
    if (isMacOS) return; // native engine handles bare-modifier taps on macOS
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void listen("shortcut-settings-changed", () => {
      reloadBareModifier(latest.current.hotkey, true);
    }).then((unlistenFn) => {
      // listen() resolved after unmount → unlisten right away to avoid a leak;
      // otherwise hold the unlisten fn for cleanup.
      if (cancelled) {
        unlistenFn();
      } else {
        unlisten = unlistenFn;
      }
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [reloadBareModifier]);

  useEffect(() => {
    // A pending lone-modifier tap: the code of the modifier pressed with no
    // other key + the recording state at keydown. Cleared on completion, on any
    // intervening key, or on focus/visibility loss.
    let pendingTap: { code: string; stateAtDown: string } | null = null;
    let activeHoldCode: string | null = null;
    let pendingHoldStart: number | null = null;
    let holdStartDispatched = false;
    let holdStartPromise: Promise<boolean> | null = null;

    // Toggle recording, mirroring the native state machine (handle_toggle_mode):
    // queue a stop during Starting, ignore other transitional states, and debounce.
    const performToggle = (reason: string): void => {
      const { recording: currentRecording } = latest.current;
      const now = Date.now();
      if (now - lastToggleRef.current < TOGGLE_DEBOUNCE_MS) return;
      lastToggleRef.current = now;
      const state = currentRecording.state;
      if (state === "idle" || state === "error") {
        log.debug(`In-app ${reason} in editable field — starting recording`);
        void currentRecording.startRecording();
      } else if (state === "starting" || state === "recording") {
        log.debug(`In-app ${reason} in editable field — stopping recording`);
        void currentRecording.stopRecording();
      }
    };

    const startHold = (): boolean => {
      const { recording: currentRecording } = latest.current;
      const state = currentRecording.state;
      if (state === "idle" || state === "error") {
        log.debug("In-app bare-modifier hold in editable field — starting recording");
        // Keep the start promise so a stop that races this in-flight start can
        // chain after it — a stop_recording that reaches the backend before the
        // start publishes `Starting` is silently dropped, and the start also
        // clears pending_stop_after_start before Starting (stale-flag hygiene),
        // so backend-side queueing cannot cover this window.
        holdStartPromise = currentRecording.startRecording();
        return true;
      }
      return false;
    };

    const clearPendingHoldStart = (): void => {
      if (pendingHoldStart !== null) {
        window.clearTimeout(pendingHoldStart);
        pendingHoldStart = null;
      }
    };

    const armHoldStart = (code: string): void => {
      clearPendingHoldStart();
      pendingHoldStart = window.setTimeout(() => {
        pendingHoldStart = null;
        if (activeHoldCode === code) {
          holdStartDispatched = startHold();
        }
      }, 0);
    };

    const cancelHoldBeforeStart = (): void => {
      clearPendingHoldStart();
      // The hold-start timer can fire between AltGr's synthesized Control
      // keydown and the AltGraph keydown that reveals it was never a bare
      // hold — if the start already dispatched, stop it or the recording
      // sticks with no keyup owner.
      if (holdStartDispatched) {
        stopHold();
      }
      activeHoldCode = null;
      holdStartDispatched = false;
    };

    const stopHold = (): void => {
      const { recording: currentRecording } = latest.current;
      const state = currentRecording.state;
      const startInFlight = holdStartPromise;
      holdStartPromise = null;
      if (startInFlight) {
        log.debug("In-app bare-modifier hold — stop chained after in-flight start");
        void startInFlight.then(
          (started) => {
            // startRecording reports its own outcome — React state can be
            // stale at settle time (events/re-renders lag the invoke), so it
            // must not gate this. Stop only a start that actually took; a
            // failed start keeps its error state instead of being reset.
            if (started) {
              void latest.current.recording.stopRecording();
            }
          },
          () => {},
        );
        return;
      }
      if (state === "recording" || state === "starting" || activeHoldCode) {
        log.debug("In-app bare-modifier hold in editable field — stopping recording");
        void currentRecording.stopRecording();
      }
    };

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.isComposing) return;
      if (!isEditableTarget(event.target)) return;

      const currentHotkey = latest.current.hotkey;
      const bareModifier = bareModifierRef.current;

      if (bareModifier) {
        if (eventMatchesBareModifier(event, bareModifier.modifier)) {
          if (event.repeat) return; // held modifier auto-repeats; keep the pending tap/hold
          if (hasOtherModifierHeld(event, bareModifier.modifier) || hasAltGraph(event)) {
            pendingTap = null;
            cancelHoldBeforeStart();
            return;
          }
          if (bareModifier.kind === "modifier_hold") {
            if (activeHoldCode !== null || holdStartPromise !== null) {
              // A hold is already live (duplicate keydown without repeat —
              // lost keyup, focus churn). Rebind to the latest physical key so
              // its keyup owns the stop, but preserve the dispatched state and
              // in-flight start: re-arming here would clobber
              // holdStartDispatched and orphan the recording.
              activeHoldCode = event.code;
              return;
            }
            activeHoldCode = event.code;
            holdStartDispatched = false;
            armHoldStart(event.code);
            return;
          }
          pendingTap = { code: event.code, stateAtDown: latest.current.recording.state };
          return;
        }
        if (!currentHotkey) {
          // Any non-matching key while the modifier is held → it's a chord/combo,
          // not a clean tap (covers Ctrl+C/V and AltGr's synthesized second key).
          pendingTap = null;
          if (activeHoldCode && isAltGraphSecondKey(event)) {
            cancelHoldBeforeStart();
          }
          return;
        }
      }

      // ── Combo path (e.g. Ctrl+Space) ─────────────────────────────────────
      if (currentHotkey) {
        if (event.repeat) return;
        // Only Ctrl/Cmd-class combos (the IME-swallowed class); leave bare keys
        // and Shift/Alt-only combos to the field's normal typing.
        if (!event.ctrlKey && !event.metaKey) return;
        // AltGr is reported as Ctrl+Alt on Windows and produces typed chars.
        if (hasAltGraph(event)) return;
        if (!eventMatchesShortcut(event, currentHotkey)) return;
        event.preventDefault();
        performToggle("hotkey");
        return;
      }
    };

    const onKeyUp = (event: KeyboardEvent) => {
      if (activeHoldCode && event.code === activeHoldCode) {
        clearPendingHoldStart();
        if (event.isComposing) {
          activeHoldCode = null;
          holdStartDispatched = false;
          return;
        }
        if (!isEditableTarget(event.target)) {
          activeHoldCode = null;
          holdStartDispatched = false;
          return;
        }
        if (holdStartDispatched || holdStartPromise !== null) {
          stopHold();
        }
        activeHoldCode = null;
        holdStartDispatched = false;
        return;
      }
      const pending = pendingTap;
      if (!pending || event.code !== pending.code) return;
      pendingTap = null;
      if (event.isComposing || hasAltGraph(event)) return;
      if (!isEditableTarget(event.target)) return;
      // Bail if the recording state moved since keydown (e.g. the native hook
      // did fire) — avoids a start→stop stutter.
      if (latest.current.recording.state !== pending.stateAtDown) return;
      performToggle("bare-modifier tap");
    };

    const clearPending = () => {
      clearPendingHoldStart();
      if (activeHoldCode) {
        if (holdStartDispatched) {
          stopHold();
        }
        activeHoldCode = null;
        holdStartDispatched = false;
      }
      pendingTap = null;
    };

    // Capture phase: run before the focused field's own handlers.
    window.addEventListener("keydown", onKeyDown, true);
    window.addEventListener("keyup", onKeyUp, true);
    window.addEventListener("blur", clearPending, true);
    document.addEventListener("visibilitychange", clearPending);

    return () => {
      clearPending();
      window.removeEventListener("keydown", onKeyDown, true);
      window.removeEventListener("keyup", onKeyUp, true);
      window.removeEventListener("blur", clearPending, true);
      document.removeEventListener("visibilitychange", clearPending);
    };
  }, []);
}
