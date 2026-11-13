use super::*;
use objc2::rc::Retained;

struct PrivateBoard(Retained<NSPasteboard>);

impl PrivateBoard {
    fn new() -> Self {
        Self(NSPasteboard::pasteboardWithUniqueName())
    }
}

impl Drop for PrivateBoard {
    fn drop(&mut self) {
        // SAFETY: Tests exclusively own these private named pasteboards.
        unsafe { let _: () = objc2::msg_send![&*self.0, releaseGlobally]; }
    }
}

fn write(board: &NSPasteboard, value: &Snapshot) {
    assert!(restore(board, value, board.changeCount()));
}

fn original() -> Snapshot {
    // Multiple items and representations, including RTF, image and file data.
    vec![
        vec![
            ("public.utf8-plain-text".into(), b"original".to_vec()),
            ("public.rtf".into(), b"{\\rtf1 original}".to_vec()),
            ("public.png".into(), vec![137, 80, 78, 71]),
        ],
        vec![("public.file-url".into(), b"file:///controlled-fixture".to_vec())],
    ]
}

#[test]
fn newer_user_copy_including_identical_text_is_never_restored_over() {
    for copied in ["controlled-fixture", "new user copy"] {
        let board = PrivateBoard::new();
        write(&board.0, &original());
        let mut transactions = Transactions::default();
        let generation = transactions.begin(&board.0, "controlled-fixture", false).unwrap();
        transactions.finish(generation, true);
        let copy = vec![vec![("public.utf8-plain-text".into(), copied.as_bytes().to_vec())]];
        write(&board.0, &copy);
        let expected = snapshot(&board.0).unwrap();
        assert_eq!(transactions.poll(&board.0, generation, Instant::now() + RESTORE_DELAY), "ownership_lost");
        assert_eq!(snapshot(&board.0), Some(expected));
    }
}

#[test]
fn superseded_identical_take_carries_every_original_format_forward() {
    let board = PrivateBoard::new();
    write(&board.0, &original());
    let expected = snapshot(&board.0).unwrap();
    let mut transactions = Transactions::default();
    let first = transactions.begin(&board.0, "controlled-fixture", false).unwrap();
    transactions.finish(first, true);
    let second = transactions.begin(&board.0, "controlled-fixture", false).unwrap();
    transactions.finish(second, true);
    assert_eq!(transactions.poll(&board.0, first, Instant::now() + RESTORE_DELAY), "superseded");
    assert!(read_fixture(&board.0));
    assert_eq!(transactions.poll(&board.0, second, Instant::now() + RESTORE_DELAY), "restored");
    assert_eq!(snapshot(&board.0), Some(expected));
}

#[test]
fn user_copy_between_rapid_takes_becomes_the_new_original() {
    let board = PrivateBoard::new();
    write(&board.0, &original());
    let mut transactions = Transactions::default();
    let first = transactions.begin(&board.0, "controlled-fixture", false).unwrap();
    transactions.finish(first, true);
    // Same text as the first take, but a distinct OS ownership token.
    let copied = snapshot(&board.0).unwrap();
    write(&board.0, &copied);
    let second = transactions.begin(&board.0, "new take", false).unwrap();
    transactions.finish(second, true);
    assert_eq!(transactions.poll(&board.0, second, Instant::now() + RESTORE_DELAY), "restored");
    assert_eq!(snapshot(&board.0), Some(copied));
}

#[test]
fn keep_and_unsuccessful_paste_leave_eager_data_for_manual_paste() {
    for (keep, pasted) in [(true, true), (false, false)] {
        let board = PrivateBoard::new();
        write(&board.0, &original());
        let mut transactions = Transactions::default();
        let generation = transactions.begin(&board.0, "controlled-fixture", keep).unwrap();
        assert!(!transactions.finish(generation, pasted));
        transactions.poll(&board.0, generation, Instant::now() + RESTORE_DELAY);
        drop(transactions);
        assert!(read_fixture(&board.0));
    }
}

#[test]
fn transcript_remains_available_through_the_restore_grace_period() {
    let board = PrivateBoard::new();
    write(&board.0, &original());
    let mut transactions = Transactions::default();
    let generation = transactions.begin(&board.0, "controlled-fixture", false).unwrap();
    transactions.finish(generation, true);
    let dispatched = transactions.pending.as_ref().unwrap().dispatched.unwrap();
    assert_eq!(transactions.poll(&board.0, generation, dispatched + Duration::from_millis(499)), "waiting");
    assert!(read_fixture(&board.0));
}
