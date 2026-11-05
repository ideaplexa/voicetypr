//! Retry again is bound to the last failed attempt, not today's Settings.
use super::{RecoveryKind, RetrySelection};
use crate::commands::settings::Settings;
use tauri::{AppHandle, Manager};

/// Some("") means a selected remote whose model has not been reported yet.
/// Never copy connection names, addresses or credentials into recovery metadata.
pub(super) async fn active_remote_model(app: &AppHandle) -> Option<String> {
    let settings = app.state::<tokio::sync::Mutex<crate::remote::settings::RemoteSettings>>();
    let settings = settings.lock().await;
    settings
        .get_active_connection()
        .map(|c| c.model.clone().unwrap_or_default())
}

impl RetrySelection {
    pub(super) fn pin_cloud_model(&mut self, app: &AppHandle) {
        if let Some(provider) = self
            .engine
            .as_deref()
            .and_then(crate::cloud_stt::CloudProvider::from_id)
        {
            self.model = provider
                .execution_model_id(&self.model, || provider.selected_model(app).id)
                .into();
        }
    }

    pub(super) fn short_name(&self) -> String {
        match self.engine.as_deref() {
            Some(engine) => crate::pill::context::engine_short_name(&self.model, engine),
            None => "Your other computer".into(),
        }
    }

    pub(super) fn resolve(
        settings: &Settings,
        kind: RecoveryKind,
        previous: Option<&Self>,
        requested: Option<&str>,
        candidates: &[(String, String)],
        remote_active: bool,
    ) -> Result<Self, String> {
        if let Some(requested) = requested {
            return candidates
                .iter()
                .find(|(m, _)| m == requested)
                .map(|(m, e)| Self {
                    model: m.clone(),
                    engine: Some(e.clone()),
                })
                .ok_or_else(|| "Requested engine is not ready".into());
        }
        if let Some(previous) = previous {
            if previous.engine.is_none() {
                if !remote_active {
                    return Err("Requested engine is not ready".into());
                }
                return Ok(Self {
                    model: String::new(),
                    engine: None,
                });
            }
            if matches!(previous.engine.as_deref(), Some("whisper" | "parakeet"))
                && !candidates
                    .iter()
                    .any(|(m, e)| *m == previous.model && Some(e) == previous.engine.as_ref())
            {
                return Err("Requested engine is not ready".into());
            }
            return Ok(previous.clone());
        }
        if kind == RecoveryKind::RemoteOffline || remote_active {
            if !remote_active {
                return Err("Requested engine is not ready".into());
            }
            return Ok(Self {
                model: String::new(),
                engine: None,
            });
        }
        let model = if settings.current_model.is_empty() {
            candidates
                .iter()
                .find(|(_, e)| *e == settings.current_model_engine)
                .map(|(m, _)| m.clone())
                .unwrap_or_default()
        } else {
            settings.current_model.clone()
        };
        Ok(Self {
            model,
            engine: Some(settings.current_model_engine.clone()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn saved_cloud_retry_reaches_executor_with_the_original_api_model() {
        use crate::cloud_stt::CloudProvider;
        use crate::transcription::executor::execute_cloud_attempt;
        let provider = CloudProvider::Openai;
        let original = provider.available_models()[0].id;
        let changed = provider.available_models()[1].id;
        let previous = RetrySelection {
            model: provider.execution_model_id("openai", || original).into(),
            engine: Some("openai".into()),
        };
        // Cross the same durable metadata boundary used by a restart.
        let previous: RetrySelection =
            serde_json::from_slice(&serde_json::to_vec(&previous).unwrap()).unwrap();
        let selected = RetrySelection::resolve(
            &Settings {
                current_model: "openai".into(),
                current_model_engine: "openai".into(),
                ..Default::default()
            },
            RecoveryKind::CloudFailed,
            Some(&previous),
            None,
            &[],
            false,
        )
        .unwrap();
        // Exercise the executor's actual provider-dispatch boundary, returning
        // the attempted API model as a transport failure without network/key I/O.
        let failed = execute_cloud_attempt(
            provider,
            &selected.model,
            || changed,
            |sent_model| async move { Err::<(), _>(sent_model) },
        )
        .await;
        assert_eq!(failed.unwrap_err(), original);
        let ordinary = execute_cloud_attempt(
            provider,
            "openai",
            || changed,
            |sent_model| async move { Err::<(), _>(sent_model) },
        )
        .await;
        assert_eq!(
            ordinary.unwrap_err(),
            changed,
            "Ordinary provider-alias requests must still follow Settings"
        );
    }

    #[test]
    fn retry_again_reuses_failed_whisper_instead_of_current_soniox() {
        let settings = Settings {
            current_model: "soniox".into(),
            current_model_engine: "soniox".into(),
            ..Default::default()
        };
        let previous = RetrySelection {
            model: "small.en".into(),
            engine: Some("whisper".into()),
        };
        let candidates = vec![
            ("small.en".into(), "whisper".into()),
            ("parakeet-v3".into(), "parakeet".into()),
        ];
        let selected = RetrySelection::resolve(
            &settings,
            RecoveryKind::CloudFailed,
            Some(&previous),
            None,
            &candidates,
            false,
        )
        .unwrap();
        assert_eq!(selected.model, "small.en");
        assert_eq!(selected.engine.as_deref(), Some("whisper"));
        let alternative = RetrySelection::resolve(
            &settings,
            RecoveryKind::CloudFailed,
            Some(&previous),
            Some("parakeet-v3"),
            &candidates,
            false,
        )
        .unwrap();
        assert_eq!(alternative.model, "parakeet-v3");
        assert!(
            RetrySelection::resolve(
                &settings,
                RecoveryKind::CloudFailed,
                Some(&previous),
                None,
                &[],
                false,
            )
            .is_err(),
            "An unavailable previous model must not silently switch to Settings"
        );
    }

    #[test]
    fn remote_retry_never_names_the_local_settings_model() {
        let settings = Settings {
            current_model: "soniox".into(),
            current_model_engine: "soniox".into(),
            ..Default::default()
        };
        let remote = RetrySelection::resolve(
            &settings,
            RecoveryKind::RemoteOffline,
            None,
            None,
            &[],
            true,
        )
        .unwrap();
        assert!(remote.model.is_empty());
        assert!(remote.engine.is_none());
        assert_eq!(remote.short_name(), "Your other computer");
        let legacy = RetrySelection {
            model: "soniox".into(),
            engine: None,
        };
        let again = RetrySelection::resolve(
            &settings,
            RecoveryKind::RemoteOffline,
            Some(&legacy),
            None,
            &[],
            true,
        )
        .unwrap();
        assert!(again.model.is_empty());
        assert_eq!(again.short_name(), "Your other computer");
        let local = RetrySelection::resolve(
            &settings,
            RecoveryKind::RemoteOffline,
            Some(&remote),
            Some("small.en"),
            &[("small.en".into(), "whisper".into())],
            true,
        )
        .unwrap();
        assert_eq!(local.short_name(), "Whisper Small");
        assert_eq!(local.engine.as_deref(), Some("whisper"));
    }

    #[test]
    fn active_remote_is_used_for_unattempted_audio_but_never_overrides_a_failed_model() {
        let settings = Settings {
            current_model: "soniox".into(),
            current_model_engine: "soniox".into(),
            ..Default::default()
        };
        let candidates = vec![("small.en".into(), "whisper".into())];
        for kind in [
            RecoveryKind::NoSpeech,
            RecoveryKind::Unknown,
            RecoveryKind::Integrity,
        ] {
            let selected =
                RetrySelection::resolve(&settings, kind, None, None, &candidates, true).unwrap();
            assert!(selected.engine.is_none());
            assert!(
                selected.model.is_empty(),
                "Unknown remote model must not become a fake model ID"
            );
        }
        assert!(RetrySelection::resolve(
            &settings,
            RecoveryKind::RemoteOffline,
            None,
            None,
            &candidates,
            false
        )
        .is_err());
        let previous = RetrySelection {
            model: "small.en".into(),
            engine: Some("whisper".into()),
        };
        let selected = RetrySelection::resolve(
            &settings,
            RecoveryKind::CloudFailed,
            Some(&previous),
            None,
            &candidates,
            true,
        )
        .unwrap();
        assert_eq!(selected.model, "small.en");
        assert_eq!(selected.engine.as_deref(), Some("whisper"));
    }
}
