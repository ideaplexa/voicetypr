use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use objc2::runtime::ProtocolObject;
use objc2_app_kit::{NSPasteboard, NSPasteboardItem, NSPasteboardWriting};
use objc2_foundation::{ns_string, NSArray, NSData, NSString};

// Explicit fallback policy: a provider read cannot identify the destination,
// and a lazy promise loses its data if Voicetypr crashes before the first read.
// Keep eager data and today's timer; do not describe this as a read receipt.
const RESTORE_DELAY: Duration = Duration::from_millis(500);
type Snapshot = Vec<Vec<(String, Vec<u8>)>>;

struct Pending {
    generation: u64,
    change_count: isize,
    original: Option<Arc<Snapshot>>,
    dispatched: Option<Instant>,
}

#[derive(Default)]
struct Transactions {
    generation: u64,
    pending: Option<Pending>,
}

static TRANSACTIONS: Mutex<Transactions> = Mutex::new(Transactions {
    generation: 0,
    pending: None,
});

// Materialize every representation of every item before overwriting the board.
// A failed/lazy/unreadable representation disqualifies the whole snapshot; RTF,
// images and file URLs must never be silently downgraded to plain text.
fn snapshot(board: &NSPasteboard) -> Option<Snapshot> {
    let count = board.changeCount();
    let mut result = Vec::new();
    let items = board.pasteboardItems()?;
    for item in items.iter() {
        let mut formats = Vec::new();
        for kind in item.types().iter() {
            let data = item.dataForType(&kind)?;
            formats.push((kind.to_string(), data.to_vec()));
        }
        if formats.is_empty() {
            return None;
        }
        result.push(formats);
    }
    (board.changeCount() == count).then_some(result)
}

fn restore(board: &NSPasteboard, original: &Snapshot, owned: isize) -> bool {
    // Prepare objects before the final OS ownership check. NSPasteboard has no
    // compare-and-swap operation; keep the check/write gap as small as possible.
    let mut items = Vec::with_capacity(original.len());
    for formats in original {
        let item = NSPasteboardItem::new();
        for (kind, bytes) in formats {
            if !item.setData_forType(&NSData::with_bytes(bytes), &NSString::from_str(kind)) {
                return false;
            }
        }
        items.push(item);
    }
    let objects: Vec<_> = items
        .iter()
        .map(|item| ProtocolObject::<dyn NSPasteboardWriting>::from_ref(&**item))
        .collect();
    let objects = NSArray::from_slice(&objects);
    if board.changeCount() != owned {
        return false;
    }
    let cleared = board.clearContents();
    // Also fail closed if another writer won immediately after clearContents.
    if board.changeCount() != cleared {
        return false;
    }
    original.is_empty() || board.writeObjects(&objects)
}

impl Transactions {
    fn begin(&mut self, board: &NSPasteboard, text: &str, keep: bool) -> Result<u64, String> {
        let before = board.changeCount();
        let original = if keep {
            None
        } else if let Some(pending) = self
            .pending
            .as_ref()
            .filter(|p| p.change_count == before && p.original.is_some())
        {
            pending.original.clone()
        } else {
            snapshot(board).map(Arc::new)
        };
        let item = NSPasteboardItem::new();
        if !item.setString_forType(
            &NSString::from_str(text),
            ns_string!("public.utf8-plain-text"),
        ) {
            return Err("Clipboard text unavailable".into());
        }
        // Snapshotting may invoke another app's slow provider. Never overwrite
        // a new copy that arrived during that operation.
        if board.changeCount() != before {
            return Err("Clipboard changed before delivery; try again".into());
        }
        self.generation = self.generation.wrapping_add(1);
        self.pending = None;
        let owned = board.clearContents();
        if board.changeCount() != owned
            || !board.writeObjects(&NSArray::from_slice(&[ProtocolObject::<
                dyn NSPasteboardWriting,
            >::from_ref(&*item)]))
            || board.changeCount() != owned
        {
            return Err("Clipboard copy failed or ownership changed".into());
        }
        self.pending = Some(Pending {
            generation: self.generation,
            change_count: owned,
            original,
            dispatched: None,
        });
        Ok(self.generation)
    }

    fn finish(&mut self, generation: u64, pasted: bool) -> bool {
        let Some(pending) = self.pending.as_mut().filter(|p| p.generation == generation) else {
            return false;
        };
        if !pasted {
            // Failed, protected-field and clipboard-only delivery must remain
            // available for manual paste, including after the process exits.
            pending.original = None;
            return false;
        }
        pending.dispatched = Some(Instant::now());
        pending.original.is_some()
    }

    fn poll(&mut self, board: &NSPasteboard, generation: u64, now: Instant) -> &'static str {
        let Some(pending) = self.pending.as_ref().filter(|p| p.generation == generation) else {
            return "superseded";
        };
        if board.changeCount() != pending.change_count {
            self.pending = None;
            return "ownership_lost";
        }
        let Some(dispatched) = pending.dispatched else {
            return "left_in_clipboard";
        };
        if now.saturating_duration_since(dispatched) < RESTORE_DELAY {
            return "waiting";
        }
        let restored = pending
            .original
            .as_ref()
            .is_some_and(|original| restore(board, original, pending.change_count));
        self.pending = None;
        if restored {
            "restored"
        } else {
            "left_in_clipboard"
        }
    }
}

pub(crate) fn begin(text: &str, keep: bool) -> Result<u64, String> {
    objc2::rc::autoreleasepool(|_| {
        TRANSACTIONS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .begin(&NSPasteboard::generalPasteboard(), text, keep)
    })
}

pub(crate) fn finish(generation: u64, pasted: bool) {
    if !TRANSACTIONS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .finish(generation, pasted)
    {
        return;
    }
    // Failure to spawn leaves eager transcript data, not an immediate restore.
    let _ = thread::Builder::new()
        .name("clipboard-restore".into())
        .spawn(move || {
            thread::sleep(RESTORE_DELAY);
            objc2::rc::autoreleasepool(|_| {
                TRANSACTIONS.lock().unwrap_or_else(|e| e.into_inner()).poll(
                    &NSPasteboard::generalPasteboard(),
                    generation,
                    Instant::now(),
                );
            });
        });
}

/// Uses real native pasteboard data on a private board, not the user's clipboard.
/// A fixture read is not evidence that an arbitrary application inserted text.
pub(crate) fn verify_slow_reader() -> Result<serde_json::Value, &'static str> {
    objc2::rc::autoreleasepool(|_| {
        let board = NSPasteboard::pasteboardWithUniqueName();
        let result = verify_board(&board);
        // SAFETY: This private named pasteboard is owned exclusively by us.
        unsafe {
            let _: () = objc2::msg_send![&*board, releaseGlobally];
        }
        result
    })
}

fn verify_board(board: &NSPasteboard) -> Result<serde_json::Value, &'static str> {
    let original = vec![vec![
        ("public.rtf".into(), b"{\\rtf1 fixture}".to_vec()),
        ("public.png".into(), vec![137, 80, 78, 71]),
    ]];
    if !restore(board, &original, board.changeCount()) {
        return Err("fixture_setup_failed");
    }
    let before = snapshot(board).ok_or("fixture_snapshot_failed")?;
    let mut transactions = Transactions::default();
    let generation = transactions
        .begin(board, "controlled-fixture", false)
        .map_err(|_| "fixture_publish_failed")?;
    transactions.finish(generation, true);
    thread::sleep(Duration::from_millis(250));
    let received = read_fixture(board);
    thread::sleep(RESTORE_DELAY);
    let outcome = transactions.poll(board, generation, Instant::now());
    let restored = snapshot(board).is_some_and(|s| s == before);

    let stale = transactions
        .begin(board, "controlled-fixture", false)
        .map_err(|_| "fixture_publish_failed")?;
    transactions.finish(stale, true);
    let latest = transactions
        .begin(board, "controlled-fixture", false)
        .map_err(|_| "fixture_publish_failed")?;
    transactions.finish(latest, true);
    let superseded =
        transactions.poll(board, stale, Instant::now() + RESTORE_DELAY) == "superseded";
    let carried = transactions.poll(board, latest, Instant::now() + RESTORE_DELAY) == "restored"
        && snapshot(board).is_some_and(|s| s == before);

    let copied = transactions
        .begin(board, "controlled-fixture", false)
        .map_err(|_| "fixture_publish_failed")?;
    transactions.finish(copied, true);
    // A real same-text user copy increments the OS token; text equality must
    // never authorize restoration over that newer copy.
    let copy = vec![vec![(
        "public.utf8-plain-text".into(),
        b"controlled-fixture".to_vec(),
    )]];
    restore(board, &copy, board.changeCount());
    let protected_copy = transactions.poll(board, copied, Instant::now() + RESTORE_DELAY)
        == "ownership_lost"
        && read_fixture(board);
    Ok(serde_json::json!({
        "platform": "macos", "fixture": "slow-reader", "policy": "eager_timer_fallback",
        "reader_delay_ms": 250, "restore_delay_ms": 500, "receipt_supported": false,
        "reader_received_transcript": received, "outcome": outcome,
        "all_original_formats_restored": restored, "superseded_take_skipped": superseded,
        "rapid_takes_preserved_original": carried, "same_text_copy_protected": protected_copy,
        "passed": received && outcome == "restored" && restored && superseded && carried && protected_copy,
        "readers_slower_than_restore_delay_supported": false, "target_insertion_verified": false
    }))
}

fn read_fixture(board: &NSPasteboard) -> bool {
    board
        .stringForType(ns_string!("public.utf8-plain-text"))
        .is_some_and(|s| s.to_string() == "controlled-fixture")
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
