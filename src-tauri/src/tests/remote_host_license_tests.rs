//! A host without an active license or trial does not serve transcription,
//! and the client shows the host's fixed refusal instead of a generic failure.

use crate::commands::license::{CachedLicense, RuntimeLicenseCache};
use crate::license::{LicenseState, LicenseStatus};
use crate::remote::client::{
    self, RemoteClientError, RemoteServerConnection, TranscriptionRequest, TranscriptionSource,
};
use crate::remote::http::{create_routes, ServerContext};
use crate::remote::server::{ErrorResponse, TranscribeResponse};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::{RwLock, Semaphore};

struct HostContext {
    license: Arc<RwLock<RuntimeLicenseCache>>,
    transcribed: AtomicBool,
}

impl ServerContext for HostContext {
    fn get_model_name(&self) -> String {
        "base.en".to_string()
    }

    fn get_server_name(&self) -> String {
        "studio".to_string()
    }

    fn get_password(&self) -> Option<String> {
        None
    }

    fn license_cache(&self) -> Arc<RwLock<RuntimeLicenseCache>> {
        self.license.clone()
    }

    fn transcribe(
        &self,
        _audio_data: &[u8],
        _spoken_language: Option<&str>,
        _transcription_task: Option<&str>,
    ) -> Result<crate::transcription::TranscriptionResult, String> {
        self.transcribed.store(true, Ordering::SeqCst);
        let job = crate::transcription::TranscriptionJob::from_legacy_settings(
            crate::transcription::TranscriptionSource::RemoteServer,
            "remote",
            "base.en".to_string(),
            None,
            false,
        );
        Ok(crate::transcription::TranscriptionResult::new(
            &job,
            "hello".to_string(),
        ))
    }
}

fn cache(status: LicenseState) -> RuntimeLicenseCache {
    RuntimeLicenseCache::Ready(CachedLicense::new(LicenseStatus {
        status,
        trial_days_left: None,
        license_type: None,
        license_key: None,
        expires_at: None,
        verification_state: None,
        verification_expires_at: None,
    }))
}

fn host(license: RuntimeLicenseCache) -> Arc<RwLock<HostContext>> {
    Arc::new(RwLock::new(HostContext {
        license: Arc::new(RwLock::new(license)),
        transcribed: AtomicBool::new(false),
    }))
}

async fn post_audio(ctx: Arc<RwLock<HostContext>>) -> (u16, Vec<u8>) {
    let routes = create_routes(
        ctx,
        Arc::new(Semaphore::new(1)),
        Arc::new(Mutex::new(HashMap::new())),
    );
    let response = warp::test::request()
        .method("POST")
        .path("/api/v1/transcribe")
        .header("Content-Type", "audio/wav")
        .body(vec![0u8; 64])
        .reply(&routes)
        .await;
    (response.status().as_u16(), response.body().to_vec())
}

#[tokio::test]
async fn host_without_license_or_trial_refuses_and_never_transcribes() {
    for status in [LicenseState::Expired, LicenseState::None] {
        let ctx = host(cache(status));
        let (code, body) = post_audio(ctx.clone()).await;
        assert_eq!(code, 403);
        let body: ErrorResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(body.error, "host_license_required");
        assert!(!ctx.read().await.transcribed.load(Ordering::SeqCst));
    }
}

#[tokio::test]
async fn host_with_license_or_trial_serves() {
    for status in [LicenseState::Licensed, LicenseState::Trial] {
        let ctx = host(cache(status));
        let (code, body) = post_audio(ctx.clone()).await;
        assert_eq!(code, 200);
        let body: TranscribeResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(body.text, "hello");
    }
}

#[tokio::test]
async fn host_license_check_failure_refuses_like_the_hotkey_without_saying_no_trial() {
    let mut needs_verification = cache(LicenseState::Licensed);
    if let RuntimeLicenseCache::Ready(cached) = &mut needs_verification {
        cached.status.verification_expires_at =
            Some(chrono::Utc::now() - chrono::Duration::minutes(1));
    }
    for license in [RuntimeLicenseCache::Failed, needs_verification] {
        let ctx = host(license);
        let (code, body) = post_audio(ctx.clone()).await;
        assert_eq!(code, 403);
        let body: ErrorResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(body.error, "host_license_unverified");
        assert!(!ctx.read().await.transcribed.load(Ordering::SeqCst));
    }
}

#[tokio::test]
async fn host_waits_for_a_startup_check_like_the_hotkey() {
    let ctx = host(RuntimeLicenseCache::Loading);
    let license = ctx.read().await.license.clone();
    let (response, ()) = tokio::join!(post_audio(ctx), async {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        *license.write().await = cache(LicenseState::Trial);
    });
    assert_eq!(response.0, 200);
}

#[tokio::test]
async fn unlicensed_host_refuses_before_taking_the_transcription_permit() {
    let guard = Arc::new(Semaphore::new(1));
    let _busy = guard.clone().acquire_owned().await.unwrap();
    let routes = create_routes(
        host(cache(LicenseState::Expired)),
        guard,
        Arc::new(Mutex::new(HashMap::new())),
    );
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        warp::test::request()
            .method("POST")
            .path("/api/v1/transcribe")
            .header("Content-Type", "audio/wav")
            .body(vec![0u8; 64])
            .reply(&routes),
    )
    .await
    .expect("refusal must not queue behind a running transcription");
    assert_eq!(response.status(), 403);
}

#[test]
fn a_host_context_without_the_app_has_no_license() {
    use crate::remote::transcription::{RealTranscriptionContext, TranscriptionServerConfig};
    let ctx = RealTranscriptionContext::new(TranscriptionServerConfig {
        server_name: "studio".to_string(),
        password: None,
        model_path: std::path::PathBuf::from("/nonexistent"),
        model_name: "base.en".to_string(),
    });
    let license = ctx.license_cache();
    assert!(matches!(
        *license.try_read().unwrap(),
        RuntimeLicenseCache::Failed
    ));
}

#[tokio::test]
async fn client_shows_the_hosts_fixed_license_message() {
    for (status, expected) in [
        (
            Some(LicenseState::Expired),
            "The other computer's Voicetypr has no active license or trial.",
        ),
        (
            None,
            "The other computer's Voicetypr could not check its license. Open Voicetypr on that computer, check Settings → License, then try again.",
        ),
    ] {
        let license = status.map(cache).unwrap_or(RuntimeLicenseCache::Failed);
        let routes = create_routes(
            host(license),
            Arc::new(Semaphore::new(1)),
            Arc::new(Mutex::new(HashMap::new())),
        );
        let (addr, server) = warp::serve(routes).bind_ephemeral(([127, 0, 0, 1], 0));
        let server = tokio::spawn(server);
        let connection = RemoteServerConnection::new("127.0.0.1".to_string(), addr.port(), None);
        let request = TranscriptionRequest::new(vec![0u8; 64], TranscriptionSource::Upload);
        let error = client::transcribe_audio(&connection, request, 5_000)
            .await
            .unwrap_err();
        server.abort();
        assert!(matches!(error, RemoteClientError::HostLicense { .. }));
        assert_eq!(error.to_string(), expected);
    }
}
