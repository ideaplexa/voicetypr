//! Normalize the user's language against the active source, not an inactive
//! local model while a remote Voicetypr is selected.
use crate::remote::settings::RemoteSettings;
use tauri::{async_runtime::Mutex, AppHandle, Manager, Runtime};

pub(crate) async fn normalize_for_active_source<R: Runtime>(
    app: &AppHandle<R>,
    engine: &str,
    model: &str,
    language: &str,
) -> String {
    let remote = if let Some(state) = app.try_state::<Mutex<RemoteSettings>>() {
        let settings = state.lock().await;
        settings.get_active_connection().is_some()
    } else {
        false
    };
    super::settings::normalize_speech_language_for_model(
        if remote { "remote" } else { engine },
        model,
        language,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn remote_language_is_not_restricted_by_the_inactive_local_model() {
        let app = tauri::test::mock_app();
        app.manage(Mutex::new(RemoteSettings::default()));
        assert_eq!(
            normalize_for_active_source(app.handle(), "whisper", "small.en", "ja").await,
            "en"
        );
        {
            let state = app.state::<Mutex<RemoteSettings>>();
            let mut settings = state.lock().await;
            let connection = settings.add_connection(
                "127.0.0.1".into(),
                47842,
                None,
                None,
                Some("large-v3".into()),
            );
            settings.set_active_connection(Some(connection.id)).unwrap();
        }
        for (engine, model) in [
            ("whisper", "small.en"),
            ("parakeet", "parakeet-tdt-0.6b-v2"),
            ("parakeet", "parakeet-tdt-0.6b-v3"),
        ] {
            assert_eq!(
                normalize_for_active_source(app.handle(), engine, model, "ja").await,
                "ja"
            );
        }
        assert_eq!(
            normalize_for_active_source(app.handle(), "whisper", "small.en", "invalid").await,
            "en",
            "remote selection must not bypass language validation"
        );
    }
}
