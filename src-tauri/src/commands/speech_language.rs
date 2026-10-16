//! Spoken-language policy is separate from explicit output-language validation:
//! "auto" is a recognition choice, never a language to translate text into.
use crate::parakeet::models::AVAILABLE_MODELS;
use crate::whisper::languages::validate_language;

pub fn model_requires_english_speech(engine: &str, model_name: &str) -> bool {
    match engine {
        "whisper" => model_name.ends_with(".en"),
        "parakeet" => AVAILABLE_MODELS
            .iter()
            .find(|model| model.id == model_name)
            .map(|model| model.languages == ["en"])
            .unwrap_or_else(|| model_name.contains("-v2")),
        _ => false,
    }
}

pub fn supports_auto_speech_language(engine: &str, model_name: &str) -> bool {
    matches!(engine, "whisper" | "parakeet" | "crispasr")
        && !model_requires_english_speech(engine, model_name)
}

pub fn normalize_speech_language_for_model(
    engine: &str,
    model_name: &str,
    speech_language: &str,
) -> String {
    if speech_language.trim() == "auto" && supports_auto_speech_language(engine, model_name) {
        return "auto".to_string();
    }
    let validated = validate_language(Some(speech_language));
    match engine {
        "crispasr" => crate::crispasr::models::get(model_name)
            .filter(|model| model.languages().contains(&validated))
            .map(|_| validated)
            .unwrap_or("en")
            .to_string(),
        "whisper" if model_requires_english_speech(engine, model_name) => "en".to_string(),
        "parakeet" => {
            if let Some(definition) = AVAILABLE_MODELS.iter().find(|model| model.id == model_name) {
                if definition.languages.contains(&validated) {
                    validated.to_string()
                } else {
                    definition
                        .languages
                        .first()
                        .copied()
                        .unwrap_or("en")
                        .to_string()
                }
            } else if model_requires_english_speech(engine, model_name) {
                "en".to_string()
            } else {
                validated.to_string()
            }
        }
        "soniox" => {
            const SONIOX_SUPPORTED_LANGUAGES: &[&str] = &[
                "en", "es", "fr", "de", "it", "pt", "nl", "ru", "zh", "ja", "ko", "ar", "hi", "tr",
                "pl", "sv", "no", "da", "fi", "el", "cs", "ro", "hu", "sk", "uk", "he", "id", "vi",
                "th", "ms", "tl", "fa", "ur", "bn", "ta", "te", "gu", "pa", "bg", "hr", "sr", "sl",
                "lv", "lt", "et", "is", "ca", "gl",
            ];
            if SONIOX_SUPPORTED_LANGUAGES.contains(&validated) {
                validated.to_string()
            } else {
                "en".to_string()
            }
        }
        "cohere" => {
            const COHERE_SUPPORTED_LANGUAGES: &[&str] = &[
                "en", "de", "fr", "it", "es", "pt", "el", "nl", "pl", "vi", "zh", "ar", "ja", "ko",
            ];
            if COHERE_SUPPORTED_LANGUAGES.contains(&validated) {
                validated.to_string()
            } else {
                "en".to_string()
            }
        }
        _ => validated.to_string(),
    }
}

pub(crate) fn normalize_stored_speech_language<R: tauri::Runtime>(
    store: &tauri_plugin_store::Store<R>,
    language: &str,
) -> String {
    let engine = store
        .get("current_model_engine")
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| "whisper".into());
    let model = store
        .get("current_model")
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default();
    normalize_speech_language_for_model(&engine, &model, language)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn startup_preserves_auto_for_the_selected_multilingual_model() {
        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_store::Builder::default().build())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let temp = tempfile::tempdir().unwrap();
        let store =
            tauri_plugin_store::StoreBuilder::new(app.handle(), temp.path().join("settings"))
                .build()
                .unwrap();
        store.set("current_model_engine", serde_json::json!("whisper"));
        store.set("current_model", serde_json::json!("base"));
        assert_eq!(normalize_stored_speech_language(&store, "auto"), "auto");
        store.set("current_model", serde_json::json!("base.en"));
        assert_eq!(normalize_stored_speech_language(&store, "auto"), "en");
    }
}
