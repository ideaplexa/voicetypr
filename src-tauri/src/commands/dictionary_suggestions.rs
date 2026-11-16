//! Dictionary suggestions learned from the user's own History edits.
//! Everything stays in a local store; words are never logged or sent to analytics.
use serde::{Deserialize, Serialize};
use tauri_plugin_store::StoreExt;

use crate::writing::{corrected_words, CorrectedWord, WritingSettings};

const SUGGESTIONS_STORE: &str = "dictionary_suggestions";
const SUGGESTIONS_KEY: &str = "suggestions";
const MAX_STORED: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionStatus {
    Pending,
    Accepted,
    Dismissed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DictionarySuggestion {
    pub id: String,
    pub heard: String,
    pub word: String,
    pub status: SuggestionStatus,
}

fn same(left: &str, right: &str) -> bool {
    left.to_lowercase() == right.to_lowercase()
}

fn already_in_dictionary(settings: &WritingSettings, found: &CorrectedWord) -> bool {
    settings.custom_words.iter().any(|entry| {
        entry.phrase == found.word
            && (same(&found.heard, &found.word)
                || entry
                    .spoken_form
                    .as_deref()
                    .is_some_and(|spoken| same(spoken, &found.heard)))
    }) || settings
        .replacements
        .iter()
        .any(|rule| same(&rule.from, &found.heard) && rule.to == found.word)
}

/// Adds the corrections from one edit to the stored suggestions and returns the
/// ones now waiting for the user. A dismissed suggestion is never offered again.
fn offer(
    stored: &mut Vec<DictionarySuggestion>,
    found: Vec<CorrectedWord>,
    settings: &WritingSettings,
) -> Vec<DictionarySuggestion> {
    let mut offered = Vec::new();
    for found in found {
        if already_in_dictionary(settings, &found) {
            continue;
        }
        let suggestion = match stored
            .iter_mut()
            .find(|s| same(&s.heard, &found.heard) && s.word == found.word)
        {
            Some(existing) if existing.status == SuggestionStatus::Dismissed => continue,
            Some(existing) => {
                // Accepted earlier, but the user has since removed it from the Dictionary.
                existing.status = SuggestionStatus::Pending;
                existing.clone()
            }
            None => {
                let suggestion = DictionarySuggestion {
                    id: uuid::Uuid::new_v4().to_string(),
                    heard: found.heard,
                    word: found.word,
                    status: SuggestionStatus::Pending,
                };
                stored.push(suggestion.clone());
                suggestion
            }
        };
        offered.push(suggestion);
    }
    while stored.len() > MAX_STORED {
        let oldest_settled = stored
            .iter()
            .position(|s| s.status != SuggestionStatus::Pending)
            .unwrap_or(0);
        stored.remove(oldest_settled);
    }
    offered
}

fn load<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Result<Vec<DictionarySuggestion>, String> {
    let store = app
        .store(SUGGESTIONS_STORE)
        .map_err(|_| "Could not read dictionary suggestions")?;
    Ok(store
        .get(SUGGESTIONS_KEY)
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default())
}

fn save<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    suggestions: &[DictionarySuggestion],
) -> Result<(), String> {
    let store = app
        .store(SUGGESTIONS_STORE)
        .map_err(|_| "Could not save dictionary suggestions")?;
    store.set(
        SUGGESTIONS_KEY,
        serde_json::to_value(suggestions).map_err(|_| "Could not save dictionary suggestions")?,
    );
    store
        .save()
        .map_err(|_| "Could not save dictionary suggestions".to_string())
}

/// Saves the user's edit of one History entry. Returns whether the text changed
/// and the corrected words it offers for the Dictionary.
fn edit_history_entry<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    id: &str,
    text: &str,
) -> Result<(bool, Vec<DictionarySuggestion>), String> {
    let edited = text.trim();
    if edited.is_empty() {
        return Err("Text can't be empty".into());
    }
    let store = app
        .store("transcriptions")
        .map_err(|_| "Could not read history")?;
    let mut entry = store
        .get(id)
        .ok_or("This dictation is no longer in History")?;
    if entry
        .get("status")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|status| status != "completed")
    {
        return Err("This dictation can't be edited".into());
    }
    let before = entry
        .get("text")
        .and_then(serde_json::Value::as_str)
        .ok_or("This dictation can't be edited")?
        .to_owned();
    if before == edited {
        return Ok((false, Vec::new()));
    }
    entry["text"] = edited.into();
    store.set(id, entry);
    store.save().map_err(|_| "Could not save history")?;

    let found = corrected_words(&before, edited);
    if found.is_empty() {
        return Ok((true, Vec::new()));
    }
    let settings = crate::writing::load_writing_settings(app)?;
    let mut stored = load(app)?;
    let offered = offer(&mut stored, found, &settings);
    save(app, &stored)?;
    Ok((true, offered))
}

#[tauri::command]
pub async fn update_transcription_text(
    app: tauri::AppHandle,
    id: String,
    text: String,
) -> Result<Vec<DictionarySuggestion>, String> {
    let (changed, offered) = edit_history_entry(&app, &id, &text)?;
    if changed {
        let _ = crate::emit_to_window(&app, "main", "history-updated", ());
        if let Err(error) = crate::commands::settings::update_tray_menu(app.clone()).await {
            log::warn!("Failed to update tray menu after a history edit: {error}");
        }
    }
    Ok(offered)
}

#[tauri::command]
pub async fn list_dictionary_suggestions(
    app: tauri::AppHandle,
) -> Result<Vec<DictionarySuggestion>, String> {
    Ok(load(&app)?
        .into_iter()
        .filter(|s| s.status == SuggestionStatus::Pending)
        .collect())
}

#[tauri::command]
pub async fn set_dictionary_suggestion_status(
    app: tauri::AppHandle,
    id: String,
    status: SuggestionStatus,
) -> Result<(), String> {
    let mut stored = load(&app)?;
    let suggestion = stored
        .iter_mut()
        .find(|s| s.id == id)
        .ok_or("This suggestion no longer exists")?;
    suggestion.status = status;
    save(&app, &stored)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::writing::CustomWord;

    fn found(heard: &str, word: &str) -> Vec<CorrectedWord> {
        vec![CorrectedWord {
            heard: heard.into(),
            word: word.into(),
        }]
    }

    fn app_with_history(
        dir: &tempfile::TempDir,
        entry: serde_json::Value,
    ) -> tauri::App<tauri::test::MockRuntime> {
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.config_mut().identifier = dir.path().to_str().unwrap().to_string();
        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_store::Builder::default().build())
            .build(context)
            .unwrap();
        app.store("transcriptions")
            .unwrap()
            .set("2026-11-16T09:00:00Z", entry);
        app
    }

    fn history_text(app: &tauri::App<tauri::test::MockRuntime>) -> String {
        app.store("transcriptions")
            .unwrap()
            .get("2026-11-16T09:00:00Z")
            .unwrap()["text"]
            .as_str()
            .unwrap()
            .to_string()
    }

    #[test]
    fn editing_a_dictation_saves_it_and_offers_the_fixed_word_once() {
        let dir = tempfile::tempdir().unwrap();
        let app = app_with_history(
            &dir,
            serde_json::json!({
                "text": "ask Jamini to review it",
                "model": "parakeet-tdt-0.6b-v2",
                "status": "completed",
            }),
        );
        let (changed, offered) = edit_history_entry(
            app.handle(),
            "2026-11-16T09:00:00Z",
            " ask Gemini to review it ",
        )
        .unwrap();
        assert!(changed);
        assert_eq!(history_text(&app), "ask Gemini to review it");
        assert_eq!(offered.len(), 1);
        assert_eq!(
            (offered[0].heard.as_str(), offered[0].word.as_str()),
            ("Jamini", "Gemini")
        );
        assert_eq!(load(app.handle()).unwrap(), offered);

        let (changed, offered) = edit_history_entry(
            app.handle(),
            "2026-11-16T09:00:00Z",
            "ask Gemini to review it",
        )
        .unwrap();
        assert!(!changed && offered.is_empty());
    }

    #[test]
    fn unfinished_or_emptied_dictations_are_not_changed() {
        let dir = tempfile::tempdir().unwrap();
        let app = app_with_history(
            &dir,
            serde_json::json!({
                "text": "In progress...",
                "model": "parakeet-tdt-0.6b-v2",
                "status": "in_progress",
            }),
        );
        assert!(edit_history_entry(app.handle(), "2026-11-16T09:00:00Z", "edited").is_err());
        assert!(edit_history_entry(app.handle(), "2026-11-16T09:00:00Z", "   ").is_err());
        assert!(edit_history_entry(app.handle(), "missing", "edited").is_err());
        assert_eq!(history_text(&app), "In progress...");
        assert!(load(app.handle()).unwrap().is_empty());
    }

    #[test]
    fn a_correction_is_offered_once_and_stays_dismissed() {
        let settings = WritingSettings::default();
        let mut stored = Vec::new();
        let first = offer(&mut stored, found("voice typer", "Voicetypr"), &settings);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].status, SuggestionStatus::Pending);

        let again = offer(&mut stored, found("Voice typer", "Voicetypr"), &settings);
        assert_eq!(again, first);
        assert_eq!(stored.len(), 1);

        stored[0].status = SuggestionStatus::Dismissed;
        assert!(offer(&mut stored, found("voice typer", "Voicetypr"), &settings).is_empty());
        assert_eq!(stored[0].status, SuggestionStatus::Dismissed);
    }

    #[test]
    fn a_word_already_in_the_dictionary_is_not_offered_until_it_is_removed() {
        let mut settings = WritingSettings {
            custom_words: vec![CustomWord {
                phrase: "Voicetypr".into(),
                spoken_form: Some("voice typer".into()),
                language: None,
                enabled: true,
            }],
            ..Default::default()
        };
        let mut stored = vec![DictionarySuggestion {
            id: "one".into(),
            heard: "voice typer".into(),
            word: "Voicetypr".into(),
            status: SuggestionStatus::Accepted,
        }];
        assert!(offer(&mut stored, found("voice typer", "Voicetypr"), &settings).is_empty());

        settings.custom_words.clear();
        let offered = offer(&mut stored, found("voice typer", "Voicetypr"), &settings);
        assert_eq!(offered.len(), 1);
        assert_eq!(offered[0].id, "one");
        assert_eq!(stored[0].status, SuggestionStatus::Pending);
    }

    #[test]
    fn old_settled_suggestions_make_room_before_waiting_ones() {
        let settings = WritingSettings::default();
        let mut stored = vec![DictionarySuggestion {
            id: "waiting".into(),
            heard: "first heard".into(),
            word: "FirstWord".into(),
            status: SuggestionStatus::Pending,
        }];
        stored.extend((0..MAX_STORED - 1).map(|n| DictionarySuggestion {
            id: n.to_string(),
            heard: format!("heard {n}"),
            word: format!("Word{n}"),
            status: SuggestionStatus::Dismissed,
        }));
        offer(&mut stored, found("voice typer", "Voicetypr"), &settings);
        assert_eq!(stored.len(), MAX_STORED);
        assert_eq!(stored[0].id, "waiting");
        assert_eq!(stored.last().unwrap().word, "Voicetypr");
    }
}
