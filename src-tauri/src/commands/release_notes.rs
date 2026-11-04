//! Installed-version manifest highlights. Full notes live only on the release page.
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::LazyLock,
    time::{Duration, Instant},
};

#[derive(Clone, Serialize)]
pub struct ReleaseNotes {
    version: String,
    body: Option<String>,
    url: String,
}
#[derive(Deserialize)]
struct ReleaseManifest {
    version: String,
    notes: Option<String>,
}
impl ReleaseManifest {
    fn highlights_for(self, version: &str) -> Option<String> {
        // Never show a different release's highlights (including a rolling channel manifest).
        (self.version.trim_start_matches('v') == version.trim_start_matches('v'))
            .then_some(self.notes)
            .flatten()
    }
}
static CACHE: LazyLock<Mutex<HashMap<String, (Instant, ReleaseNotes)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[tauri::command]
pub async fn get_release_notes(app: tauri::AppHandle) -> ReleaseNotes {
    let version = app.package_info().version.to_string();
    if let Some((at, notes)) = CACHE.lock().get(&version) {
        let ttl = if notes.body.is_some() {
            Duration::from_secs(3600)
        } else {
            Duration::from_secs(30)
        };
        if at.elapsed() < ttl {
            return notes.clone();
        }
    }
    let body = fetch_body(&version).await;
    let notes = release_notes(version.clone(), body);
    CACHE
        .lock()
        .insert(version, (Instant::now(), notes.clone()));
    notes
}
fn release_notes(version: String, body: Option<String>) -> ReleaseNotes {
    let url = format!("https://github.com/ideaplexa/voicetypr/releases/tag/v{version}");
    ReleaseNotes {
        version,
        body: body.filter(|body| !body.trim().is_empty()),
        url,
    }
}
async fn fetch_body(version: &str) -> Option<String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .user_agent("Voicetypr-release-notes")
        .build()
        .ok()?;
    let response = client
        .get(format!(
            "https://github.com/ideaplexa/voicetypr/releases/download/v{version}/latest.json"
        ))
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?;
    response
        .json::<ReleaseManifest>()
        .await
        .ok()?
        .highlights_for(version)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn manifest_highlights_are_for_the_installed_version_only() {
        for version in ["2.1.0-beta.6", "v2.1.0-beta.6"] {
            let manifest: ReleaseManifest = serde_json::from_str(
                r#"{"version":"v2.1.0-beta.6","notes":"- Better dictation\n- Faster startup"}"#,
            )
            .unwrap();
            assert_eq!(
                manifest.highlights_for(version).as_deref(),
                Some("- Better dictation\n- Faster startup")
            );
        }
        let manifest = ReleaseManifest {
            version: "v2.1.0-beta.7".into(),
            notes: Some("- Newer release".into()),
        };
        assert!(manifest.highlights_for("2.1.0-beta.6").is_none());
    }
    #[test]
    fn missing_manifest_notes_does_not_use_the_full_release_body() {
        let manifest: ReleaseManifest =
            serde_json::from_str(r#"{"version":"v2.1.0-beta.6","body":"Full release body"}"#)
                .unwrap();
        assert!(manifest.highlights_for("2.1.0-beta.6").is_none());
    }
    #[test]
    fn missing_or_empty_body_keeps_version_release_link() {
        for body in [None, Some("  ".into())] {
            let notes = release_notes("2.1.0-beta.4".into(), body);
            assert!(notes.body.is_none());
            assert_eq!(
                notes.url,
                "https://github.com/ideaplexa/voicetypr/releases/tag/v2.1.0-beta.4"
            );
        }
    }
}
