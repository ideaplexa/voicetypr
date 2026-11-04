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
const MAX_MANIFEST_BYTES: usize = 64 * 1024;

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
    read_highlights(response, version).await
}
async fn read_highlights(mut response: reqwest::Response, version: &str) -> Option<String> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_MANIFEST_BYTES as u64)
    {
        return None;
    }
    // Enforce the cap on actual bytes too: headers can be absent or misleading.
    // Never buffer an unbounded response, including with chunked transfer encoding.
    let mut bytes = Vec::with_capacity(MAX_MANIFEST_BYTES);
    while let Some(chunk) = response.chunk().await.ok()? {
        if chunk.len() > MAX_MANIFEST_BYTES - bytes.len() {
            return None;
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice::<ReleaseManifest>(&bytes)
        .ok()?
        .highlights_for(version)
}
#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    fn manifest_of_size(size: usize) -> String {
        let mut body =
            r#"{"version":"2.1.0-beta.6","notes":"- Better dictation","padding":""#.to_owned();
        body.push_str(&"x".repeat(size - body.len() - 2));
        body.push_str("\"}");
        assert_eq!(body.len(), size);
        body
    }

    // One-shot loopback fixture: the listener and socket are dropped before returning;
    // no spawned server task or process can outlive the test.
    async fn highlights_from_http(body: &str, headers: &str, chunked: bool) -> Option<String> {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let mut response = format!("HTTP/1.1 200 OK\r\nConnection: close\r\n{headers}\r\n");
        if chunked {
            for chunk in body.as_bytes().chunks(1024) {
                response.push_str(&format!("{:X}\r\n", chunk.len()));
                response.push_str(std::str::from_utf8(chunk).unwrap());
                response.push_str("\r\n");
            }
            response.push_str("0\r\n\r\n");
        } else {
            response.push_str(body);
        }
        let serve = async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0; 1024];
            assert!(socket.read(&mut request).await.unwrap() > 0);
            // Early cap rejection can close the connection while the server is writing.
            let _ = socket.write_all(response.as_bytes()).await;
        };
        let fetch = async {
            let response = reqwest::Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap()
                .get(format!("http://{address}/latest.json"))
                .send()
                .await
                .unwrap();
            read_highlights(response, "2.1.0-beta.6").await
        };
        let (_, highlights) = tokio::join!(serve, fetch);
        highlights
    }

    #[tokio::test]
    async fn accepts_beta_sized_and_exact_limit_manifests() {
        for size in [8 * 1024, MAX_MANIFEST_BYTES] {
            let body = manifest_of_size(size);
            for (headers, chunked) in [
                (format!("Content-Length: {size}\r\n"), false),
                (String::new(), false),
                ("Transfer-Encoding: chunked\r\n".into(), true),
            ] {
                assert_eq!(
                    highlights_from_http(&body, &headers, chunked)
                        .await
                        .as_deref(),
                    Some("- Better dictation")
                );
            }
        }
    }

    #[tokio::test]
    async fn rejects_oversized_declared_length_before_reading_body() {
        // Even a small, valid body must not bypass the declared-length guard.
        assert!(highlights_from_http(
            &manifest_of_size(8 * 1024),
            &format!("Content-Length: {}\r\n", MAX_MANIFEST_BYTES + 1),
            false,
        )
        .await
        .is_none());
    }

    #[tokio::test]
    async fn oversized_streams_keep_the_friendly_fallback_and_release_link() {
        let body = manifest_of_size(MAX_MANIFEST_BYTES + 1);
        for (headers, chunked) in [
            ("", false),
            ("Transfer-Encoding: chunked\r\n", true),
            // Transfer-Encoding overrides this lying Content-Length on the wire.
            ("Content-Length: 8\r\nTransfer-Encoding: chunked\r\n", true),
        ] {
            let body = highlights_from_http(&body, headers, chunked).await;
            let notes = release_notes("2.1.0-beta.6".into(), body);
            assert!(notes.body.is_none());
            assert_eq!(
                notes.url,
                "https://github.com/ideaplexa/voicetypr/releases/tag/v2.1.0-beta.6"
            );
        }
    }

    #[tokio::test]
    async fn malformed_or_truncated_responses_fall_back() {
        assert!(highlights_from_http("not JSON", "", false).await.is_none());
        let body = manifest_of_size(8 * 1024);
        // Too-small framing truncates the JSON; too-large framing ends prematurely.
        for length in [8, body.len() + 1] {
            assert!(
                highlights_from_http(&body, &format!("Content-Length: {length}\r\n"), false,)
                    .await
                    .is_none()
            );
        }
    }

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
