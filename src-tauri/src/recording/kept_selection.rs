//! Retry again is bound to the last failed attempt, not today's Settings.
use super::{RecoveryKind, RetrySelection};
use crate::commands::settings::Settings;

impl RetrySelection {
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
            if matches!(previous.engine.as_deref(), Some("whisper" | "parakeet"))
                && !candidates
                    .iter()
                    .any(|(m, e)| *m == previous.model && Some(e) == previous.engine.as_ref())
            {
                return Err("Requested local engine is not ready".into());
            }
            return Ok(previous.clone());
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
            engine: (kind != RecoveryKind::RemoteOffline)
                .then(|| settings.current_model_engine.clone()),
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
}
