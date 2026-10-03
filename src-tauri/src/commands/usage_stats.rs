//! Content-free Insights aggregates. History values never leave the blocking worker.
use std::{
    collections::{BTreeMap, HashMap},
    time::{Duration, Instant},
};

use chrono::{Local, NaiveDate, TimeZone};
use parking_lot::Mutex;
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Manager};
use tauri_plugin_store::StoreExt;

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct UsageStats {
    first_use: Option<String>,
    total_words: u64,
    total_dictations: u64,
    total_audio_ms: u64,
    polished_dictations: u64,
    days: Vec<UsageDay>,
    apps: Vec<UsageApp>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
struct UsageDay {
    date: String,
    words: u64,
    dictations: u64,
    audio_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
struct UsageApp {
    name: String,
    words: u64,
}

const CACHE_TTL: Duration = Duration::from_secs(5);

struct CachedStats {
    since: Option<NaiveDate>,
    created: Instant,
    stats: UsageStats,
}

#[derive(Default)]
pub(crate) struct UsageStatsCache(Mutex<CacheState>);

#[derive(Default)]
struct CacheState {
    generation: u64,
    entry: Option<CachedStats>,
}

impl CacheState {
    fn get(&self, since: Option<NaiveDate>, now: Instant) -> Option<UsageStats> {
        self.entry
            .as_ref()
            .filter(|entry| entry.since == since && now.duration_since(entry.created) < CACHE_TTL)
            .map(|entry| entry.stats.clone())
    }

    fn insert(&mut self, generation: u64, since: Option<NaiveDate>, stats: UsageStats) {
        // An update during the scan must not repopulate the cache with stale data.
        if generation == self.generation {
            self.entry = Some(CachedStats {
                since,
                created: Instant::now(),
                stats,
            });
        }
    }

    fn invalidate(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.entry = None;
    }
}

pub(crate) fn invalidate_on_event(app: &AppHandle, event: &str) {
    if matches!(
        event,
        "history-updated" | "transcription-added" | "transcription-updated"
    ) {
        if let Some(cache) = app.try_state::<UsageStatsCache>() {
            cache.0.lock().invalidate();
        }
    }
}

#[tauri::command]
pub async fn get_usage_stats(app: AppHandle, since: Option<String>) -> Result<UsageStats, String> {
    let since = since
        .as_deref()
        .map(|day| {
            NaiveDate::parse_from_str(day, "%Y-%m-%d")
                .map_err(|_| "Invalid usage start date".to_string())
        })
        .transpose()?;
    tauri::async_runtime::spawn_blocking(move || {
        let cache = app.state::<UsageStatsCache>();
        let generation = {
            let state = cache.0.lock();
            if let Some(stats) = state.get(since, Instant::now()) {
                return Ok(stats);
            }
            state.generation
        };
        let store = app
            .store("transcriptions")
            .map_err(|_| "Unable to read usage history".to_string())?;
        let mut aggregate = UsageAggregate::default();
        // Same keys/get API as get_transcription_history, without sorting, paging,
        // reconciliation writes, or retaining/cloning transcripts beyond Store::get.
        for key in store.keys() {
            if let Some(value) = store.get(&key) {
                aggregate.add(&value, history_day(&key, &Local), since);
            }
        }
        let stats = aggregate.finish();
        cache.0.lock().insert(generation, since, stats.clone());
        Ok(stats)
    })
    .await
    .map_err(|_| "Unable to compute usage statistics".to_string())?
}

fn history_day<T: TimeZone>(key: &str, timezone: &T) -> Option<NaiveDate> {
    chrono::DateTime::parse_from_rfc3339(key)
        .ok()
        .map(|timestamp| timestamp.with_timezone(timezone).date_naive())
}

// Match JavaScript /\s+/ (including BOM, excluding Unicode NEXT LINE), as used
// by useOverviewStats. Splitting borrows the stored text and allocates no words.
fn word_count(text: &str) -> u64 {
    text.split(|c: char| {
        matches!(c,
            '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}' |
            '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' |
            '\u{205f}' | '\u{3000}' | '\u{feff}'
        )
    })
    .filter(|word| !word.is_empty())
    .count() as u64
}

#[derive(Default)]
struct UsageAggregate {
    stats: UsageStats,
    days: BTreeMap<NaiveDate, UsageDay>,
    apps: HashMap<String, u64>,
}

impl UsageAggregate {
    fn add(&mut self, value: &Value, date: Option<NaiveDate>, since: Option<NaiveDate>) {
        if !value.is_object()
            || value
                .get("status")
                .is_some_and(|status| status.as_str() != Some("completed"))
        {
            return;
        }
        let words = word_count(
            value
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        );
        let writing = value.get("writing");
        let audio_ms = writing
            .and_then(|w| w.get("audio_duration_ms"))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        self.stats.total_words += words;
        self.stats.total_dictations += 1;
        self.stats.total_audio_ms += audio_ms;
        self.stats.polished_dictations += u64::from(
            writing
                .and_then(|w| w.get("ai_applied"))
                .and_then(Value::as_bool)
                == Some(true),
        );
        if let Some(date) = date {
            let day = self.days.entry(date).or_insert_with(|| UsageDay {
                date: date.to_string(),
                ..UsageDay::default()
            });
            day.words += words;
            day.dictations += 1;
            day.audio_ms += audio_ms;
        }
        if since.is_some_and(|start| date.is_none_or(|date| date < start)) {
            return;
        }
        if let Some(name) = writing
            .and_then(|w| w.get("context_hint"))
            .and_then(|hint| hint.get("app_name"))
            .and_then(Value::as_str)
            .filter(|name| !name.trim().is_empty())
        {
            if let Some(total) = self.apps.get_mut(name) {
                *total += words;
            } else {
                self.apps.insert(name.to_string(), words);
            }
        }
    }

    fn finish(mut self) -> UsageStats {
        self.stats.first_use = self
            .days
            .first_key_value()
            .map(|(date, _)| date.to_string());
        self.stats.days = self.days.into_values().collect();
        let mut apps: Vec<_> = self
            .apps
            .into_iter()
            .map(|(name, words)| UsageApp { name, words })
            .collect();
        apps.sort_unstable_by(|a, b| b.words.cmp(&a.words).then_with(|| a.name.cmp(&b.name)));
        if apps.len() > 8 {
            let words = apps.drain(8..).map(|app| app.words).sum();
            if let Some(other) = apps.iter_mut().find(|app| app.name == "Other") {
                other.words += words;
            } else {
                apps.push(UsageApp {
                    name: "Other".to_string(),
                    words,
                });
            }
        }
        self.stats.apps = apps;
        self.stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;
    use serde_json::json;

    fn day(date: &str) -> Option<NaiveDate> {
        Some(NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap())
    }

    #[test]
    fn totals_and_local_midnight_grouping() {
        let offset = FixedOffset::east_opt(7200).unwrap();
        let mut aggregate = UsageAggregate::default();
        for (timestamp, text, duration, polished) in [
            ("2026-10-02T21:59:59Z", " one\ttwo\nthree ", 1000, true),
            ("2026-10-02T22:00:00Z", "four", 2000, false),
            ("2026-10-03T02:00:00+02:00", "", 0, false),
        ] {
            aggregate.add(&json!({"text": text, "writing": {"audio_duration_ms": duration, "ai_applied": polished}}), history_day(timestamp, &offset), None);
        }
        let stats = aggregate.finish();
        assert_eq!(stats.first_use.as_deref(), Some("2026-10-02"));
        assert_eq!(
            (
                stats.total_words,
                stats.total_dictations,
                stats.total_audio_ms,
                stats.polished_dictations
            ),
            (4, 3, 3000, 1)
        );
        assert_eq!(
            stats.days,
            vec![
                UsageDay {
                    date: "2026-10-02".into(),
                    words: 3,
                    dictations: 1,
                    audio_ms: 1000
                },
                UsageDay {
                    date: "2026-10-03".into(),
                    words: 1,
                    dictations: 2,
                    audio_ms: 2000
                },
            ]
        );
    }

    #[test]
    fn apps_top_eight_and_other_with_inclusive_apps_only_filter() {
        let mut aggregate = UsageAggregate::default();
        aggregate.add(
            &json!({"text": "old words", "writing": {"context_hint": {"app_name": "Old"}}}),
            day("2026-10-01"),
            day("2026-10-02"),
        );
        for count in 1..=10 {
            aggregate.add(&json!({"text": "word ".repeat(count), "writing": {"context_hint": {"app_name": format!("App {count}"), "window_title": "private", "path": "private"}}}), day("2026-10-02"), day("2026-10-02"));
        }
        let stats = aggregate.finish();
        assert_eq!(stats.total_words, 57);
        assert_eq!(stats.total_dictations, 11);
        assert_eq!(stats.days.len(), 2);
        assert_eq!(stats.first_use.as_deref(), Some("2026-10-01"));
        assert_eq!(stats.apps.len(), 9);
        for (index, app) in stats.apps[..8].iter().enumerate() {
            assert_eq!(app.name, format!("App {}", 10 - index));
            assert_eq!(app.words, (10 - index) as u64);
        }
        assert_eq!(
            stats.apps[8],
            UsageApp {
                name: "Other".into(),
                words: 3
            }
        );
        assert!(!serde_json::to_string(&stats).unwrap().contains("private"));
    }

    #[test]
    fn status_filtering_and_missing_metadata() {
        let mut aggregate = UsageAggregate::default();
        for status in ["failed", "in_progress", "unknown"] {
            aggregate.add(
                &json!({"text": "ignored", "status": status}),
                day("2020-01-01"),
                None,
            );
        }
        for value in [
            json!({"text": "legacy"}),
            json!({"text": "completed", "status": "completed"}),
        ] {
            aggregate.add(&value, day("2026-10-02"), None);
        }
        let stats = aggregate.finish();
        assert_eq!(stats.total_dictations, 2);
        assert_eq!(stats.total_words, 2);
        assert_eq!(stats.total_audio_ms, 0);
        assert_eq!(stats.polished_dictations, 0);
        assert!(stats.apps.is_empty());
        assert_eq!(stats.first_use.as_deref(), Some("2026-10-02"));
    }

    #[test]
    fn empty_history() {
        assert_eq!(UsageAggregate::default().finish(), UsageStats::default());
    }

    #[test]
    fn aggregates_fifty_thousand_entries_without_a_page_limit() {
        let value = json!({"text": "three whole words", "writing": {
            "audio_duration_ms": 1000, "ai_applied": true,
            "context_hint": {"app_name": "Editor"}
        }});
        let mut aggregate = UsageAggregate::default();
        for _ in 0..50_000 {
            aggregate.add(&value, day("2026-10-02"), None);
        }
        let stats = aggregate.finish();
        assert_eq!(stats.total_dictations, 50_000);
        assert_eq!(stats.total_words, 150_000);
        assert_eq!(stats.total_audio_ms, 50_000_000);
        assert_eq!(stats.polished_dictations, 50_000);
        assert_eq!(stats.days[0].dictations, 50_000);
        assert_eq!(
            stats.apps,
            vec![UsageApp {
                name: "Editor".into(),
                words: 150_000
            }]
        );
    }

    #[test]
    fn other_app_name_does_not_create_duplicate_rows() {
        let mut aggregate = UsageAggregate::default();
        aggregate.add(&json!({"text": "word ".repeat(20), "writing": {"context_hint": {"app_name": "Other"}}}), day("2026-10-02"), None);
        for count in 1..=9 {
            aggregate.add(&json!({"text": "word ".repeat(count), "writing": {"context_hint": {"app_name": format!("App {count}")}}}), day("2026-10-02"), None);
        }
        let stats = aggregate.finish();
        assert_eq!(stats.apps.len(), 8);
        assert_eq!(
            stats.apps[0],
            UsageApp {
                name: "Other".into(),
                words: 23
            }
        );
        assert_eq!(stats.apps.iter().map(|app| app.words).sum::<u64>(), 65);
    }

    #[test]
    fn javascript_whitespace_word_count() {
        assert_eq!(word_count(" \t\n\u{feff}\u{a0}"), 0);
        assert_eq!(word_count("one\u{feff}two\u{a0}three\u{2028}four"), 4);
        assert_eq!(word_count("one\u{85}two"), 1);
    }

    #[test]
    fn cache_key_expiry_and_update_during_scan() {
        let mut cache = CacheState::default();
        cache.insert(0, None, UsageStats::default());
        let created = cache.entry.as_ref().unwrap().created;
        assert!(cache.get(None, created).is_some());
        assert!(cache.get(day("2026-10-02"), created).is_none());
        assert!(cache.get(None, created + CACHE_TTL).is_none());
        cache.invalidate();
        cache.insert(0, None, UsageStats::default());
        assert!(cache.get(None, Instant::now()).is_none());
        cache.insert(1, day("2026-10-02"), UsageStats::default());
        assert!(cache.get(day("2026-10-02"), Instant::now()).is_some());
    }
}
