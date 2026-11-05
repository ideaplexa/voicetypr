//! Retry again is bound to the last failed attempt, not today's Settings.
use super::{RecoveryKind, RetrySelection};
use crate::commands::settings::Settings;

impl RetrySelection {
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
    ) -> Result<Self, String> {
        if let Some(requested) = requested {
            return candidates
                .iter()
                .find(|(m, _)| m == requested)
                .map(|(m, e)| Self {
                    model: m.clone(),
                    engine: Some(e.clone()),
                })
                .ok_or_else(|| "Requested local engine is not ready".into());
        }
        if let Some(previous) = previous {
            if previous.engine.is_none() {
                return Ok(Self {
                    model: "Network".into(),
                    engine: None,
                });
            }
            if matches!(previous.engine.as_deref(), Some("whisper" | "parakeet"))
                && !candidates
                    .iter()
                    .any(|(m, e)| *m == previous.model && Some(e) == previous.engine.as_ref())
            {
                return Err("Requested local engine is not ready".into());
            }
            return Ok(previous.clone());
        }
        if kind == RecoveryKind::RemoteOffline {
            return Ok(Self {
                model: "Network".into(),
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
        )
        .unwrap();
        assert_eq!(alternative.model, "parakeet-v3");
        assert!(
            RetrySelection::resolve(
                &settings,
                RecoveryKind::CloudFailed,
                Some(&previous),
                None,
                &[]
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
        let remote =
            RetrySelection::resolve(&settings, RecoveryKind::RemoteOffline, None, None, &[])
                .unwrap();
        assert_eq!(remote.model, "Network");
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
        )
        .unwrap();
        assert_eq!(again.model, "Network");
        assert_eq!(again.short_name(), "Your other computer");
        let local = RetrySelection::resolve(
            &settings,
            RecoveryKind::RemoteOffline,
            Some(&remote),
            Some("small.en"),
            &[("small.en".into(), "whisper".into())],
        )
        .unwrap();
        assert_eq!(local.short_name(), "Whisper Small");
        assert_eq!(local.engine.as_deref(), Some("whisper"));
    }
}
