//! Existing Windows/Linux restoration policy (macOS uses receipt transactions).
use super::*;
use std::sync::atomic::AtomicU64;
use std::sync::Mutex;

/// In-flight clipboard restoration state, guarded by [`CLIPBOARD_GUARD`].
///
/// `transcript` is what we last placed on the clipboard; `original` is the
/// user's clipboard content to restore once the transcript is no longer needed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PendingClipboardRestore {
    /// Monotonic id of the insertion that scheduled this restore; the restore
    /// only acts on its own generation, so a newer insertion cancels it.
    pub(super) generation: u64,
    pub(super) transcript: String,
    pub(super) original: String,
}

/// Serializes the insertion (capture -> set -> paste -> record) sequence against
/// the deferred restore, and carries the pending restore between them. A single
/// global lock is correct here because `IS_INSERTING` already prevents
/// concurrent insertions; only the background restore contends.
pub(super) static CLIPBOARD_GUARD: Mutex<Option<PendingClipboardRestore>> = Mutex::new(None);

/// Monotonic insertion id. Each insertion takes the next value so its deferred
/// restore can detect when a newer insertion has superseded it.
pub(super) static CLIPBOARD_GENERATION: AtomicU64 = AtomicU64::new(0);

/// Decide which ORIGINAL clipboard text an insertion must eventually restore.
///
/// If the clipboard currently holds a transcript from a prior, not-yet-restored
/// dictation, carry that dictation's original forward (so back-to-back
/// dictations restore the user's real clipboard, never an intermediate
/// transcript). Returns `None` when there is nothing safe to restore (a
/// non-text or unreadable clipboard).
pub(super) fn resolve_original_to_restore(
    pending: &Option<PendingClipboardRestore>,
    current_clipboard: Option<&str>,
) -> Option<String> {
    let current = current_clipboard?;
    match pending {
        Some(p) if p.transcript == current => Some(p.original.clone()),
        _ => Some(current.to_string()),
    }
}

/// What a deferred restore timer should do with the pending state.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum DeferredRestore {
    /// Clipboard still holds our transcript: restore this original, then clear.
    Restore(String),
    /// Our entry, but the clipboard changed (or is non-text): clear it, no restore.
    ClearPending,
    /// Not our entry (a newer insertion owns it) or nothing pending: leave it.
    Skip,
}

/// Decide what the deferred restore timer for `generation` should do. A timer
/// always resolves its OWN generation's entry (restore-then-clear, or
/// clear-only), so a skipped restore never leaves a stale entry that a later
/// identical clipboard value could misclassify.
pub(super) fn deferred_restore_action(
    pending: &Option<PendingClipboardRestore>,
    current_clipboard: Option<&str>,
    generation: u64,
) -> DeferredRestore {
    let Some(pending) = pending.as_ref() else {
        return DeferredRestore::Skip;
    };
    // A newer insertion owns the pending entry; this stale timer must not touch it.
    if pending.generation != generation {
        return DeferredRestore::Skip;
    }
    match current_clipboard {
        Some(current) if current == pending.transcript => {
            DeferredRestore::Restore(pending.original.clone())
        }
        _ => DeferredRestore::ClearPending,
    }
}

/// Spawn the background, lock-serialized clipboard restore. After the settle
/// delay it restores the user's original clipboard iff our transcript is still
/// present, then clears the pending state. Fails closed on any clipboard error,
/// so it never clobbers a user copy or a newer dictation's transcript.
pub(super) fn spawn_deferred_clipboard_restore(generation: u64) {
    if let Err(e) = thread::Builder::new()
        .name("clipboard-restore".into())
        .spawn(move || {
            thread::sleep(CLIPBOARD_RESTORE_DELAY);

            let mut pending = CLIPBOARD_GUARD
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());

            // If the clipboard cannot be opened or read, treat current as unknown
            // (None). For our own entry that maps to `ClearPending` below, so we
            // still clear it and never leave a stale entry behind.
            let mut clipboard = match Clipboard::new() {
                Ok(c) => Some(c),
                Err(e) => {
                    log::debug!("clipboard-restore: failed to open clipboard: {}", e);
                    None
                }
            };
            let current = clipboard.as_mut().and_then(|c| c.get_text().ok());

            match deferred_restore_action(&pending, current.as_deref(), generation) {
                DeferredRestore::Restore(original) => {
                    if let Some(clipboard) = clipboard.as_mut() {
                        match clipboard.set_text(&original) {
                            Ok(()) => {
                                log::debug!("clipboard-restore: restored original clipboard text")
                            }
                            Err(e) => log::debug!("clipboard-restore: set_text error: {}", e),
                        }
                    }
                    // Resolve our own entry either way so it never lingers.
                    *pending = None;
                }
                DeferredRestore::ClearPending => {
                    *pending = None;
                    log::debug!(
                        "clipboard-restore: clipboard unavailable or changed, cleared stale pending"
                    )
                }
                DeferredRestore::Skip => {
                    log::debug!("clipboard-restore: superseded by a newer insertion, skipping")
                }
            }
        })
    {
        log::debug!("Failed to spawn clipboard-restore thread: {}", e);
    }
}

pub(super) fn insert(
    mut clipboard: Clipboard,
    mut paste: impl FnMut() -> PasteOutcome,
    text: String,
    keep_transcription_in_clipboard: bool,
    current: impl Fn() -> Result<(), String>,
) -> Result<PasteOutcome, String> {
    let mut sleep = thread::sleep;

    // Serialize capture -> set -> paste -> record against the deferred restore so
    // a stale restore can never clobber this (or a newer) insertion's clipboard.
    let mut guard = CLIPBOARD_GUARD
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());

    // This insertion's generation; the deferred restore only acts on its own
    // generation, so a newer insertion (which bumps this) cancels older timers.
    let generation = CLIPBOARD_GENERATION
        .fetch_add(1, Ordering::SeqCst)
        .wrapping_add(1);

    // Decide which ORIGINAL we must eventually restore, carrying it forward when
    // the clipboard still holds a transcript from a prior, not-yet-restored
    // dictation (so rapid dictations restore the user's real clipboard).
    let original_to_restore = if keep_transcription_in_clipboard {
        None
    } else {
        let current = clipboard.get_text().ok();
        resolve_original_to_restore(&guard, current.as_deref())
    };

    let outcome = run_clipboard_insertion(&mut clipboard, &mut paste, &mut sleep, &text, &current)?;

    current()?;
    log::debug!("clipboard insertion outcome: {:?}", outcome);

    // Every outcome supersedes any prior pending restore: we either record our
    // own (Pasted) or clear it (no-restore outcomes), so a stale restore from a
    // previous dictation can never fire over what we just placed or left.
    // The caller emits its existing paste-dispatch feedback below.
    let schedule_restore = match outcome {
        PasteOutcome::Pasted => match original_to_restore {
            Some(original) => {
                *guard = Some(PendingClipboardRestore {
                    generation,
                    transcript: text.clone(),
                    original,
                });
                true
            }
            None => {
                *guard = None;
                false
            }
        },
        // Paste failed but the transcript stays on the clipboard for manual
        // paste; schedule no restore AND invalidate any stale prior restore so
        // it cannot remove the transcript we just left.
        PasteOutcome::LeftInClipboard => {
            *guard = None;
            false
        }
        PasteOutcome::NoPermission => {
            *guard = None;
            false
        }
    };
    drop(guard);
    if schedule_restore {
        spawn_deferred_clipboard_restore(generation);
    }
    Ok(outcome)
}

#[cfg(test)]
// --- resolve_original_to_restore (which ORIGINAL to eventually restore) ---
#[cfg(not(target_os = "macos"))]
mod legacy_restore_tests {
    use super::*;

    #[test]
    fn resolve_uses_current_clipboard_when_nothing_pending() {
        assert_eq!(
            resolve_original_to_restore(&None, Some("user content")),
            Some("user content".to_string())
        );
    }

    #[test]
    fn resolve_carries_original_when_clipboard_holds_prior_transcript() {
        let pending = Some(PendingClipboardRestore {
            generation: 1,
            transcript: "prior transcript".into(),
            original: "real original".into(),
        });
        // Clipboard still holds our prior transcript → carry the real original.
        assert_eq!(
            resolve_original_to_restore(&pending, Some("prior transcript")),
            Some("real original".to_string())
        );
    }

    #[test]
    fn resolve_uses_user_copy_when_clipboard_changed() {
        let pending = Some(PendingClipboardRestore {
            generation: 1,
            transcript: "prior transcript".into(),
            original: "real original".into(),
        });
        // User copied something since our paste → that becomes the original.
        assert_eq!(
            resolve_original_to_restore(&pending, Some("user copy")),
            Some("user copy".to_string())
        );
    }

    #[test]
    fn resolve_skips_for_non_text_clipboard() {
        let pending = Some(PendingClipboardRestore {
            generation: 1,
            transcript: "prior".into(),
            original: "orig".into(),
        });
        assert_eq!(resolve_original_to_restore(&pending, None), None);
    }

    // --- deferred_restore_action (what the background restore does) ---

    #[test]
    fn deferred_restores_when_transcript_still_present() {
        let pending = Some(PendingClipboardRestore {
            generation: 7,
            transcript: "transcript".into(),
            original: "original".into(),
        });
        assert_eq!(
            deferred_restore_action(&pending, Some("transcript"), 7),
            DeferredRestore::Restore("original".to_string())
        );
    }

    #[test]
    fn deferred_clears_own_entry_when_user_copied_something_else() {
        let pending = Some(PendingClipboardRestore {
            generation: 7,
            transcript: "transcript".into(),
            original: "original".into(),
        });
        // Our entry, but the clipboard changed -> clear it (no restore, no stale entry).
        assert_eq!(
            deferred_restore_action(&pending, Some("user copy"), 7),
            DeferredRestore::ClearPending
        );
    }

    #[test]
    fn deferred_skips_when_nothing_pending() {
        assert_eq!(
            deferred_restore_action(&None, Some("transcript"), 7),
            DeferredRestore::Skip
        );
    }

    #[test]
    fn deferred_clears_own_entry_for_non_text_clipboard() {
        let pending = Some(PendingClipboardRestore {
            generation: 7,
            transcript: "transcript".into(),
            original: "original".into(),
        });
        assert_eq!(
            deferred_restore_action(&pending, None, 7),
            DeferredRestore::ClearPending
        );
    }

    /// Blocker 1 guard: a stale timer must not act once a newer insertion took
    /// over, even if the clipboard still holds (an identical) transcript.
    #[test]
    fn deferred_skips_when_generation_superseded() {
        let pending = Some(PendingClipboardRestore {
            generation: 8, // newer insertion now owns the pending state
            transcript: "transcript".into(),
            original: "original".into(),
        });
        // This timer was scheduled for generation 7 — it must skip and not touch it.
        assert_eq!(
            deferred_restore_action(&pending, Some("transcript"), 7),
            DeferredRestore::Skip
        );
    }

    // --- rapid-dictation carry-forward regression guards (review blocker 2) ---

    #[test]
    fn rapid_dictation_preserves_user_original_not_intermediate_transcript() {
        // Dictation A: clipboard holds the user's ORIGINAL.
        let original_a = resolve_original_to_restore(&None, Some("ORIGINAL")).unwrap();
        let pending_a = PendingClipboardRestore {
            generation: 1,
            transcript: "A_text".into(),
            original: original_a,
        };
        // Dictation B starts before A's restore fires; clipboard now holds A_text.
        let original_b = resolve_original_to_restore(&Some(pending_a), Some("A_text")).unwrap();
        let pending_b = PendingClipboardRestore {
            generation: 2,
            transcript: "B_text".into(),
            original: original_b,
        };
        // B's deferred restore must restore ORIGINAL, never the intermediate A_text.
        assert_eq!(
            deferred_restore_action(&Some(pending_b), Some("B_text"), 2),
            DeferredRestore::Restore("ORIGINAL".to_string())
        );
    }

    #[test]
    fn rapid_identical_text_dictation_preserves_user_original() {
        // Same transcript twice — the case the epoch-only guard got wrong.
        let original_a = resolve_original_to_restore(&None, Some("ORIGINAL")).unwrap();
        let pending_a = PendingClipboardRestore {
            generation: 1,
            transcript: "SAME".into(),
            original: original_a,
        };
        // Clipboard holds "SAME" (from A) when B captures.
        let original_b = resolve_original_to_restore(&Some(pending_a), Some("SAME")).unwrap();
        assert_eq!(original_b, "ORIGINAL");
        let pending_b = PendingClipboardRestore {
            generation: 2,
            transcript: "SAME".into(),
            original: original_b,
        };
        assert_eq!(
            deferred_restore_action(&Some(pending_b), Some("SAME"), 2),
            DeferredRestore::Restore("ORIGINAL".to_string())
        );
    }
}
