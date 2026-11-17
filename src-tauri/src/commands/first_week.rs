//! First-week discovery progress shown on Home: the day the person is on,
//! the last local date that day advanced, and the cards they put away.
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

const STORE_KEY: &str = "first_week";
/// Day 8 means the first week is over and no card shows again.
const LAST_DAY: u8 = 8;
const CARD_IDS: [&str; 7] = [
    "hold",
    "dictionary",
    "polish",
    "voice_commands",
    "realtime",
    "file",
    "share",
];

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct FirstWeekProgress {
    #[serde(default)]
    pub day: u8,
    #[serde(default)]
    pub last_seen: Option<String>,
    #[serde(default)]
    pub dismissed: Vec<String>,
}

fn is_local_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10
        && bytes.iter().enumerate().all(|(index, byte)| match index {
            4 | 7 => *byte == b'-',
            _ => byte.is_ascii_digit(),
        })
}

impl FirstWeekProgress {
    pub fn normalized(self) -> Self {
        let mut dismissed: Vec<String> = Vec::new();
        for id in self.dismissed {
            if CARD_IDS.contains(&id.as_str()) && !dismissed.contains(&id) {
                dismissed.push(id);
            }
        }
        Self {
            day: self.day.min(LAST_DAY),
            last_seen: self.last_seen.filter(|date| is_local_date(date)),
            dismissed,
        }
    }
}

pub(crate) fn load_first_week<R: tauri::Runtime>(
    store: &tauri_plugin_store::Store<R>,
) -> FirstWeekProgress {
    store
        .get(STORE_KEY)
        .and_then(|value| serde_json::from_value::<FirstWeekProgress>(value).ok())
        .unwrap_or_default()
        .normalized()
}

pub(crate) fn save_first_week<R: tauri::Runtime>(
    store: &tauri_plugin_store::Store<R>,
    progress: &FirstWeekProgress,
) {
    store.set(STORE_KEY, json!(progress.clone().normalized()));
}

static WRITE: std::sync::Mutex<()> = std::sync::Mutex::new(());

impl FirstWeekProgress {
    /// Progress only moves forward: the later day and date win and put-away cards stay put away,
    /// so a write captured before another one can't undo it.
    fn merged(self, newer: Self) -> Self {
        let mut dismissed = self.dismissed;
        dismissed.extend(newer.dismissed);
        Self {
            day: self.day.max(newer.day),
            last_seen: self.last_seen.max(newer.last_seen),
            dismissed,
        }
        .normalized()
    }
}

/// Merges into the stored progress and saves; on a failed save the stored value is put back.
pub(crate) fn commit_first_week<R: tauri::Runtime>(
    store: &tauri_plugin_store::Store<R>,
    progress: FirstWeekProgress,
) -> Result<FirstWeekProgress, String> {
    let _guard = WRITE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let previous = store.get(STORE_KEY);
    let merged = load_first_week(store).merged(progress);
    save_first_week(store, &merged);
    if let Err(error) = store.save() {
        match previous {
            Some(value) => store.set(STORE_KEY, value),
            None => {
                store.delete(STORE_KEY);
            }
        }
        return Err(error.to_string());
    }
    Ok(merged)
}

/// Writes only the first-week key. Generic settings saves read it but never write it,
/// so a stale settings snapshot can't undo progress and progress never triggers
/// the side effects of a full settings save.
#[tauri::command]
pub async fn set_first_week_progress(
    app: AppHandle,
    progress: FirstWeekProgress,
) -> Result<FirstWeekProgress, String> {
    let store = app.store("settings").map_err(|e| e.to_string())?;
    commit_first_week(&store, progress)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store_in(
        dir: &std::path::Path,
    ) -> std::sync::Arc<tauri_plugin_store::Store<tauri::test::MockRuntime>> {
        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_store::Builder::default().build())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        tauri_plugin_store::StoreBuilder::new(app.handle(), dir.join("settings"))
            .build()
            .unwrap()
    }

    #[test]
    fn missing_or_corrupt_progress_starts_before_day_one() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(dir.path());
        assert_eq!(load_first_week(&store), FirstWeekProgress::default());
        store.set(STORE_KEY, json!("not an object"));
        assert_eq!(load_first_week(&store), FirstWeekProgress::default());
    }

    #[test]
    fn progress_round_trips_and_drops_unknown_cards_and_bad_dates() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(dir.path());
        save_first_week(
            &store,
            &FirstWeekProgress {
                day: 40,
                last_seen: Some("yesterday".into()),
                dismissed: vec!["hold".into(), "hold".into(), "nope".into(), "share".into()],
            },
        );
        store.save().unwrap();
        store.reload().unwrap();
        assert_eq!(
            load_first_week(&store),
            FirstWeekProgress {
                day: LAST_DAY,
                last_seen: None,
                dismissed: vec!["hold".into(), "share".into()],
            }
        );
        let kept = FirstWeekProgress {
            day: 3,
            last_seen: Some("2026-11-17".into()),
            dismissed: vec!["dictionary".into()],
        };
        save_first_week(&store, &kept);
        assert_eq!(load_first_week(&store), kept);
    }
    #[test]
    fn a_stale_write_never_rewinds_the_day_or_brings_back_a_card() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(dir.path());
        let today = FirstWeekProgress {
            day: 3,
            last_seen: Some("2026-11-18".into()),
            dismissed: vec!["dictionary".into()],
        };
        commit_first_week(&store, today).unwrap();
        let stale = FirstWeekProgress {
            day: 2,
            last_seen: Some("2026-11-17".into()),
            dismissed: vec!["hold".into()],
        };
        assert_eq!(
            commit_first_week(&store, stale).unwrap(),
            FirstWeekProgress {
                day: 3,
                last_seen: Some("2026-11-18".into()),
                dismissed: vec!["dictionary".into(), "hold".into()],
            }
        );
        store.reload().unwrap();
        assert_eq!(load_first_week(&store).day, 3);
    }
}
