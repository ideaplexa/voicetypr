//! Installed-version release notes. Network failures return a friendly UI fallback.
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
struct GitHubRelease {
    body: Option<String>,
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
            "https://api.github.com/repos/ideaplexa/voicetypr/releases/tags/v{version}"
        ))
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?;
    response.json::<GitHubRelease>().await.ok()?.body
}
#[cfg(test)]
mod tests {
    use super::*;
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
