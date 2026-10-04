//! Sorted key index over the `transcriptions` store, so paging history does not
//! clone and re-sort every key on each call.
//!
//! Built lazily, updated on add/delete, and self-healing: if its length or key
//! set ever disagrees with the store (a writer that did not notify it), it rebuilds.
use std::{cmp::Ordering, sync::Mutex};
use tauri::{AppHandle, Manager};

fn parse_history_key(key: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(key)
        .ok()
        .map(|timestamp| timestamp.with_timezone(&chrono::Utc))
}

/// Newest first; unparsable keys sort after parsable ones, ties broken by key.
pub(crate) fn compare_history_keys(a: &str, b: &str) -> Ordering {
    match (parse_history_key(a), parse_history_key(b)) {
        (Some(a_timestamp), Some(b_timestamp)) => {
            b_timestamp.cmp(&a_timestamp).then_with(|| b.cmp(a))
        }
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => b.cmp(a),
    }
}

#[derive(Default)]
pub struct HistoryIndex {
    sorted: Mutex<Option<Vec<String>>>,
}

impl HistoryIndex {
    /// First `limit` keys, newest first. `store_len` is the store's current
    /// length; `has_key` checks membership in the store; `all_keys` is only called
    /// when the index must be (re)built. Equal length alone is not enough (an add
    /// and a delete can both land before their notifications), so every indexed key
    /// is verified: same length plus all indexed keys present means the same set.
    pub fn page(
        &self,
        store_len: usize,
        has_key: impl Fn(&str) -> bool,
        all_keys: impl FnOnce() -> Vec<String>,
        limit: usize,
    ) -> Vec<String> {
        let mut guard = self.sorted.lock().unwrap_or_else(|e| e.into_inner());
        let stale = guard
            .as_ref()
            .is_none_or(|keys| keys.len() != store_len || !keys.iter().all(|key| has_key(key)));
        if stale {
            let mut keys = all_keys();
            keys.sort_unstable_by(|a, b| compare_history_keys(a, b));
            *guard = Some(keys);
        }
        let keys = guard.as_ref().map(Vec::as_slice).unwrap_or_default();
        keys[..limit.min(keys.len())].to_vec()
    }

    pub fn insert(&self, key: &str) {
        let mut guard = self.sorted.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(keys) = guard.as_mut() {
            if let Err(at) = keys.binary_search_by(|probe| compare_history_keys(probe, key)) {
                keys.insert(at, key.to_string());
            }
        }
    }

    pub fn remove(&self, key: &str) {
        let mut guard = self.sorted.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(keys) = guard.as_mut() {
            if let Ok(at) = keys.binary_search_by(|probe| compare_history_keys(probe, key)) {
                keys.remove(at);
            }
        }
    }

    pub fn invalidate(&self) {
        *self.sorted.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

/// Notify the managed index (if any) that `key` was added.
pub fn note_added(app: &AppHandle, key: &str) {
    if let Some(index) = app.try_state::<HistoryIndex>() {
        index.insert(key);
    }
}

/// Notify the managed index (if any) that `key` was removed.
pub fn note_removed(app: &AppHandle, key: &str) {
    if let Some(index) = app.try_state::<HistoryIndex>() {
        index.remove(key);
    }
}

/// Drop the cached order after a bulk change.
pub fn note_reset(app: &AppHandle) {
    if let Some(index) = app.try_state::<HistoryIndex>() {
        index.invalidate();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verbatim copy of the pre-index implementation, kept as the reference.
    fn page_history_keys(mut keys: Vec<String>, limit: usize) -> Vec<String> {
        keys.sort_unstable_by(|a, b| match (parse_history_key(a), parse_history_key(b)) {
            (Some(a_timestamp), Some(b_timestamp)) => {
                b_timestamp.cmp(&a_timestamp).then_with(|| b.cmp(a))
            }
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => b.cmp(a),
        });
        keys.truncate(limit);
        keys
    }

    fn sample_keys() -> Vec<String> {
        let mut keys = vec![
            "2026-10-01T10:00:00.000000+00:00".to_string(),
            "2026-10-01T10:00:00Z".to_string(),
            "2026-10-01T12:00:00+02:00".to_string(),
            "2026-10-03T08:15:30.123456789+00:00".to_string(),
            "2025-01-01T00:00:00+00:00".to_string(),
            "not-a-timestamp".to_string(),
            "another-bad-key".to_string(),
            "2026-10-02T23:59:59-05:00".to_string(),
        ];
        for i in 0..40 {
            keys.push(format!(
                "2026-09-{:02}T{:02}:00:00+00:00",
                1 + i % 28,
                i % 24
            ));
        }
        keys
    }

    #[test]
    fn index_pages_match_page_history_keys() {
        let keys = sample_keys();
        let index = HistoryIndex::default();
        for limit in [0, 1, 5, 50, 500] {
            assert_eq!(
                index.page(
                    keys.len(),
                    |k| keys.iter().any(|x| x == k),
                    || keys.clone(),
                    limit
                ),
                page_history_keys(keys.clone(), limit),
                "limit {limit}"
            );
        }
    }

    #[test]
    fn incremental_add_and_remove_match_full_resort() {
        let mut keys = sample_keys();
        let index = HistoryIndex::default();
        index.page(
            keys.len(),
            |k| keys.iter().any(|x| x == k),
            || keys.clone(),
            10,
        );

        let added = "2026-10-04T09:30:00+00:00".to_string();
        keys.push(added.clone());
        index.insert(&added);
        index.insert(&added); // duplicate insert is a no-op
        assert_eq!(
            index.page(
                keys.len(),
                |k| keys.iter().any(|x| x == k),
                || panic!("must not rebuild"),
                500
            ),
            page_history_keys(keys.clone(), 500)
        );

        let removed = "2026-10-01T12:00:00+02:00".to_string();
        keys.retain(|k| k != &removed);
        index.remove(&removed);
        assert_eq!(
            index.page(
                keys.len(),
                |k| keys.iter().any(|x| x == k),
                || panic!("must not rebuild"),
                500
            ),
            page_history_keys(keys.clone(), 500)
        );
    }

    #[test]
    fn rebuilds_when_store_length_disagrees() {
        let mut keys = sample_keys();
        let index = HistoryIndex::default();
        index.page(
            keys.len(),
            |k| keys.iter().any(|x| x == k),
            || keys.clone(),
            10,
        );
        keys.push("2026-10-05T00:00:00+00:00".to_string());
        assert_eq!(
            index.page(
                keys.len(),
                |k| keys.iter().any(|x| x == k),
                || keys.clone(),
                500
            ),
            page_history_keys(keys.clone(), 500)
        );
        index.invalidate();
        assert_eq!(
            index.page(
                keys.len(),
                |k| keys.iter().any(|x| x == k),
                || keys.clone(),
                3
            ),
            page_history_keys(keys.clone(), 3)
        );
    }

    #[test]
    fn add_and_delete_before_notifications_still_rebuilds() {
        let mut keys = sample_keys();
        let index = HistoryIndex::default();
        index.page(
            keys.len(),
            |k| keys.iter().any(|x| x == k),
            || keys.clone(),
            10,
        );

        // Store: add B, delete A. Length is unchanged; no notification yet.
        let a = "2026-10-03T08:15:30.123456789+00:00".to_string();
        let b = "2026-10-04T09:30:00+00:00".to_string();
        keys.retain(|k| k != &a);
        keys.push(b.clone());
        let page = index.page(
            keys.len(),
            |k| keys.iter().any(|x| x == k),
            || keys.clone(),
            500,
        );
        assert_eq!(page, page_history_keys(keys.clone(), 500));
        assert_eq!(page[0], b);
        assert!(!page.contains(&a));

        // Late notifications are harmless.
        index.insert(&b);
        index.remove(&a);
        assert_eq!(
            index.page(
                keys.len(),
                |k| keys.iter().any(|x| x == k),
                || panic!("must not rebuild"),
                500
            ),
            page_history_keys(keys.clone(), 500)
        );
    }
}
