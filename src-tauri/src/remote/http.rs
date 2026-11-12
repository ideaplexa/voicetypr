//! HTTP server implementation for remote transcription
//!
//! Uses warp to create REST API endpoints for status and transcription.

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use log::{info, warn};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{OwnedSemaphorePermit, RwLock, Semaphore};
use warp::{http::StatusCode, Filter, Rejection, Reply};

use super::server::{
    ErrorResponse, RemoteCapabilities, RemoteModelControlSnapshot, RemoteModelControlUpdate,
    StatusResponse, TranscribeResponse, REMOTE_PROTOCOL_VERSION,
};
use crate::transcription::TranscriptionResult;

/// Auth header name
const AUTH_HEADER: &str = "X-Voicetypr-Key";
const CONTEXT_HEADER: &str = "X-Voicetypr-Context";

/// Maximum wall-clock time for a single remote transcription. Bounds how long
/// the single serialization permit can be held so a slow upload or a runaway
/// engine run cannot starve other remote clients.
const REMOTE_TRANSCRIBE_TIMEOUT: Duration = Duration::from_secs(3600); // 1 hour

/// In-memory OOM-safety ceiling on the request body (warp buffers the whole
/// body before the handler runs). Replaces the former 50 MB cap that rejected
/// legitimate long recordings — 512 MB comfortably holds well over an hour of
/// audio. The real bound on server resources is [`REMOTE_TRANSCRIBE_TIMEOUT`];
/// this just stops a pathological upload from exhausting host memory. (Streaming
/// the body to disk would avoid the in-memory copy, but the engine trait takes
/// `&[u8]` and re-materializes to a tempfile internally, so it isn't worth it.)
const MAX_AUDIO_BODY_BYTES: u64 = 512 * 1024 * 1024; // 512 MB
const MAX_CONTEXT_HEADER_BYTES: usize = 4096;
const MAX_CONTEXT_HEADER_ENCODED_BYTES: usize = MAX_CONTEXT_HEADER_BYTES.div_ceil(3) * 4;
const MAX_CONTROL_BODY_BYTES: u64 = 4 * 1024;

/// Shared map from client IP to last-seen time for recent-client counting.
pub type ClientActivityMap = Arc<Mutex<HashMap<IpAddr, Instant>>>;

/// How long after a client's last transcription request they are counted as "recent".
pub const RECENT_CLIENT_WINDOW: std::time::Duration = std::time::Duration::from_secs(300);

/// Remove entries from `map` whose last-seen time is older than `window` relative to `now`.
///
/// Called on both write (to bound map size) and read (for the status count).
pub fn prune_stale_clients(
    map: &mut HashMap<IpAddr, Instant>,
    now: Instant,
    window: std::time::Duration,
) {
    map.retain(|_, last_seen| {
        now.checked_duration_since(*last_seen)
            .is_some_and(|elapsed| elapsed <= window)
    });
}

/// Count distinct client IPs whose last-seen time is within `window` of `now`,
/// pruning stale entries from `map` in place.
///
/// Pure function — takes explicit `now` so it can be unit-tested without timers.
pub fn count_recent_clients(
    map: &mut HashMap<IpAddr, Instant>,
    now: Instant,
    window: std::time::Duration,
) -> u32 {
    prune_stale_clients(map, now, window);
    map.len() as u32
}

/// Trait for server context (allows mocking in tests)
pub trait ServerContext: Send + Sync {
    fn get_model_name(&self) -> String;
    fn get_server_name(&self) -> String;
    fn get_password(&self) -> Option<String>;
    fn allow_model_control(&self) -> bool {
        crate::remote::model_control::is_model_control_enabled()
    }
    fn transcribe(
        &self,
        audio_data: &[u8],
        spoken_language: Option<&str>,
        transcription_task: Option<&str>,
    ) -> Result<TranscriptionResult, String>;
    fn get_engine(&self) -> String {
        "whisper".to_string()
    }
    /// Engine and model for status/capabilities in one consistent read.
    fn model_status_snapshot(&self) -> (String, String) {
        let engine = self.get_engine();
        let model = self.get_model_name();
        (engine, model)
    }
    fn transcribe_with_context(
        &self,
        audio_data: &[u8],
        spoken_language: Option<&str>,
        transcription_task: Option<&str>,
        context: Option<&str>,
    ) -> Result<crate::transcription::TranscriptionResult, String> {
        let _ = context;
        self.transcribe(audio_data, spoken_language, transcription_task)
    }
    fn get_model_control_snapshot(&self) -> Result<RemoteModelControlSnapshot, String> {
        Err("Remote model control is unavailable for this host".to_string())
    }
    fn update_shared_model(
        &self,
        _model_id: &str,
        _engine: &str,
    ) -> Result<RemoteModelControlSnapshot, String> {
        Err("Remote model control is unavailable for this host".to_string())
    }
}

/// Create all warp routes for the remote transcription API.
///
/// Uses production defaults for the transcription timeout (1 hour) and disk
/// guard (2 GB). Tests that need to control either should use
/// [`create_routes_with_limits`].
pub fn create_routes<T: ServerContext + 'static>(
    ctx: Arc<RwLock<T>>,
    transcription_guard: Arc<Semaphore>,
    client_activity: ClientActivityMap,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone {
    create_routes_with_limits(
        ctx,
        transcription_guard,
        client_activity,
        REMOTE_TRANSCRIBE_TIMEOUT,
        MAX_AUDIO_BODY_BYTES,
    )
}

/// Same as [`create_routes`] but with explicit transcription timeout and disk
/// guard, for tests. Production callers always use [`create_routes`] which
/// passes the module-level defaults.
pub fn create_routes_with_limits<T: ServerContext + 'static>(
    ctx: Arc<RwLock<T>>,
    transcription_guard: Arc<Semaphore>,
    client_activity: ClientActivityMap,
    transcribe_timeout: Duration,
    audio_disk_guard: u64,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone {
    let status_route = status_endpoint(ctx.clone());
    let transcribe_route = transcribe_endpoint(
        ctx.clone(),
        transcription_guard,
        client_activity,
        transcribe_timeout,
        audio_disk_guard,
    );
    let control_models_route = control_models_endpoint(ctx);

    status_route.or(transcribe_route).or(control_models_route)
}

/// GET /api/v1/status - Returns server status and model info
fn status_endpoint<T: ServerContext + 'static>(
    ctx: Arc<RwLock<T>>,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone {
    warp::path!("api" / "v1" / "status")
        .and(warp::get())
        .and(warp::header::optional::<String>(AUTH_HEADER))
        .and(with_context(ctx))
        .and_then(handle_status)
}

/// POST /api/v1/transcribe - Accepts audio and returns transcription
fn transcribe_endpoint<T: ServerContext + 'static>(
    ctx: Arc<RwLock<T>>,
    transcription_guard: Arc<Semaphore>,
    client_activity: ClientActivityMap,
    transcribe_timeout: Duration,
    audio_disk_guard: u64,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone {
    // Auth preflight runs first and carries NO permit/body filters, so an unauthenticated
    // client is rejected before it can take the single transcription permit or stream the
    // body. Authenticated (or no-password) requests fall through to `main_route`.
    let auth_error_route = warp::path!("api" / "v1" / "transcribe")
        .and(warp::post())
        .and(warp::header::optional::<String>(AUTH_HEADER))
        .and(with_context(ctx.clone()))
        .and_then(handle_transcribe_auth_preflight);

    // Content-type preflight runs after auth and before the permit/body filters so a
    // non-audio request can never hold the single transcription permit or stream its
    // body. A non-audio content type returns 415 here; an audio content type falls
    // through to `main_route`.
    let content_type_error_route = warp::path!("api" / "v1" / "transcribe")
        .and(warp::post())
        .and(warp::header::<String>("content-type"))
        .and_then(handle_transcribe_content_type_preflight);

    let main_route = warp::path!("api" / "v1" / "transcribe")
        .and(warp::post())
        .and(warp::header::optional::<String>(AUTH_HEADER))
        .and(warp::header::<String>("content-type"))
        .and(warp::header::optional::<String>(
            "X-Voicetypr-Speech-Language",
        ))
        .and(warp::header::optional::<String>(
            "X-Voicetypr-Transcription-Task",
        ))
        .and(warp::header::optional::<String>(CONTEXT_HEADER))
        // Buffer the body BEFORE acquiring the transcription permit. A client
        // that sends the audio headers then stalls or trickles the upload used
        // to hold the single serialization permit for the entire (unbounded)
        // upload, queuing every other remote transcription behind it with no
        // 408. Now the stalled upload just fills memory — bounded by
        // `content_length_limit` above — while the permit stays free, so other
        // clients keep transcribing. The permit + 1h timeout then bound only
        // the actual transcription. The permit is still owned inside the
        // spawn_blocking task in `handle_transcribe`, so a timeout or client
        // disconnect drops it without releasing it early.
        .and(warp::body::content_length_limit(audio_disk_guard))
        .and(warp::body::bytes())
        .and(with_transcription_permit(transcription_guard))
        .map(
            move |auth_key,
                  content_type,
                  spoken_language,
                  transcription_task,
                  request_context,
                  body,
                  permit| {
                TranscribeRequestParts {
                    auth_key,
                    content_type,
                    spoken_language,
                    transcription_task,
                    request_context,
                    body,
                    transcribe_timeout,
                    permit,
                }
            },
        )
        .and(warp::filters::addr::remote())
        .and(with_context(ctx))
        .and(with_client_activity(client_activity))
        .and_then(handle_transcribe);

    auth_error_route.or(content_type_error_route).or(main_route)
}

/// Auth preflight for POST /api/v1/transcribe. Runs BEFORE the permit/body filters so an
/// unauthenticated client can never hold the single transcription permit or stream the body.
/// On a password-protected server a missing/wrong key returns 401 here; otherwise it rejects
/// with not_found so the request falls through to the main route. A server without a password
/// leaves transcription open and always falls through. Mirrors the handler's own auth check,
/// which stays as defense-in-depth.
async fn handle_transcribe_auth_preflight<T: ServerContext + 'static>(
    auth_key: Option<String>,
    ctx: Arc<RwLock<T>>,
) -> Result<Box<dyn Reply>, Rejection> {
    let required_password = { ctx.read().await.get_password() };
    let Some(required_password) = required_password else {
        // Open server: no password configured. Fall through to the main route,
        // which performs transcription without authentication.
        return Err(warp::reject::not_found());
    };
    // A configured-but-empty password is not a valid credential. Fail closed:
    // reject every request rather than treating the server as open, or letting an
    // empty X-Voicetypr-Key match the empty configured value via `"" == ""`.
    // (`auth_matches` independently rejects empty keys; this is belt-and-braces.)
    if required_password.is_empty() {
        return Ok(Box::new(warp::reply::with_status(
            warp::reply::json(&ErrorResponse {
                error: "unauthorized".to_string(),
            }),
            StatusCode::UNAUTHORIZED,
        )));
    }
    match auth_key {
        Some(provided) if auth_matches(&provided, &required_password) => {
            Err(warp::reject::not_found())
        }
        _ => Ok(Box::new(warp::reply::with_status(
            warp::reply::json(&ErrorResponse {
                error: "unauthorized".to_string(),
            }),
            StatusCode::UNAUTHORIZED,
        ))),
    }
}

/// Content-type preflight for POST /api/v1/transcribe. Runs AFTER the auth preflight
/// and BEFORE the permit/body filters, so a non-audio request is rejected with 415
/// without ever acquiring the single transcription permit or buffering its body. An
/// audio content type rejects with `not_found` so the request falls through to the
/// main route, which re-extracts the header for the handler's defense-in-depth check.
async fn handle_transcribe_content_type_preflight(
    content_type: String,
) -> Result<Box<dyn Reply>, Rejection> {
    if content_type.starts_with("audio/") {
        Err(warp::reject::not_found())
    } else {
        Ok(Box::new(warp::reply::with_status(
            warp::reply::json(&ErrorResponse {
                error: "unsupported_media_type".to_string(),
            }),
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
        )))
    }
}

/// GET/PATCH /api/v1/control/models - Password-gated remote model control
fn control_models_endpoint<T: ServerContext + 'static>(
    ctx: Arc<RwLock<T>>,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone {
    let get_route = warp::path!("api" / "v1" / "control" / "models")
        .and(warp::get())
        .and(warp::header::optional::<String>(AUTH_HEADER))
        .and(with_context(ctx.clone()))
        .and_then(handle_get_model_control);

    let patch_auth_error_route = warp::path!("api" / "v1" / "control" / "models")
        .and(warp::patch())
        .and(warp::header::optional::<String>(AUTH_HEADER))
        .and(with_context(ctx.clone()))
        .and_then(handle_patch_control_preflight);

    let patch_route = warp::path!("api" / "v1" / "control" / "models")
        .and(warp::patch())
        .and(warp::header::optional::<String>(AUTH_HEADER))
        .and(with_context(ctx))
        .and_then(require_control_auth)
        .and(warp::body::content_length_limit(MAX_CONTROL_BODY_BYTES))
        .and(warp::body::json())
        .and_then(handle_patch_model_control);

    get_route.or(patch_auth_error_route).or(patch_route)
}

/// Helper to inject context into handlers
fn with_context<T: ServerContext + 'static>(
    ctx: Arc<RwLock<T>>,
) -> impl Filter<Extract = (Arc<RwLock<T>>,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || ctx.clone())
}

/// Acquire an owned transcription permit, waiting if all permits are in use.
/// This filter runs AFTER the body has been fully buffered, so a slow or
/// stalled upload consumes only memory (bounded by `content_length_limit`) and
/// never the serialization permit — other remote clients keep transcribing
/// while a trickled upload drains. The permit is then owned by the blocking
/// transcription task, bounding only the actual engine work.
fn with_transcription_permit(
    guard: Arc<Semaphore>,
) -> impl Filter<Extract = (OwnedSemaphorePermit,), Error = Rejection> + Clone {
    warp::any().and_then(move || {
        let guard = guard.clone();
        async move {
            guard
                .acquire_owned()
                .await
                .map_err(|_| warp::reject::reject())
        }
    })
}

/// Helper to inject the client-activity map into the transcribe handler
fn with_client_activity(
    map: ClientActivityMap,
) -> impl Filter<Extract = (ClientActivityMap,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || map.clone())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    let max_len = a.len().max(b.len());
    let mut diff = a.len() ^ b.len();

    for i in 0..max_len {
        let a_byte = a.get(i).copied().unwrap_or(0);
        let b_byte = b.get(i).copied().unwrap_or(0);
        diff |= usize::from(a_byte ^ b_byte);
    }

    diff == 0
}

fn auth_matches(provided: &str, required: &str) -> bool {
    // An empty string is never a valid credential. Reject it up front so an empty
    // configured password (`Some("")`) paired with an empty X-Voicetypr-Key can
    // never authorize via `"" == ""`, and an empty provided key can never match a
    // non-empty configured password. This is the central primitive shared by the
    // transcribe, status and control auth checks.
    if provided.is_empty() || required.is_empty() {
        return false;
    }
    constant_time_eq(provided.as_bytes(), required.as_bytes())
}

async fn require_control_auth<T: ServerContext + 'static>(
    auth_key: Option<String>,
    ctx: Arc<RwLock<T>>,
) -> Result<Arc<RwLock<T>>, Rejection> {
    let (password, allow_model_control) = {
        let ctx = ctx.read().await;
        (ctx.get_password(), ctx.allow_model_control())
    };

    check_control_auth(&password, allow_model_control, auth_key)
        .map_err(|_| warp::reject::not_found())?;

    Ok(ctx)
}

async fn handle_patch_control_preflight<T: ServerContext + 'static>(
    auth_key: Option<String>,
    ctx: Arc<RwLock<T>>,
) -> Result<Box<dyn Reply>, Rejection> {
    let (password, allow_model_control) = {
        let ctx = ctx.read().await;
        (ctx.get_password(), ctx.allow_model_control())
    };

    match check_control_auth(&password, allow_model_control, auth_key) {
        Ok(()) => Err(warp::reject::not_found()),
        Err(failure) => Ok(control_auth_error(failure)),
    }
}

fn control_requires_password(password: &Option<String>) -> bool {
    password.as_ref().is_some_and(|value| !value.is_empty())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ControlAuthFailure {
    RequiresPassword,
    ModelControlDisabled,
    Unauthorized,
}

fn check_control_auth(
    password: &Option<String>,
    allow_model_control: bool,
    auth_key: Option<String>,
) -> Result<(), ControlAuthFailure> {
    if !control_requires_password(password) {
        return Err(ControlAuthFailure::RequiresPassword);
    }

    if !allow_model_control {
        return Err(ControlAuthFailure::ModelControlDisabled);
    }

    let required = password.as_ref().expect("password checked above");
    match auth_key {
        Some(provided) if auth_matches(&provided, required) => Ok(()),
        _ => Err(ControlAuthFailure::Unauthorized),
    }
}

fn control_auth_error(failure: ControlAuthFailure) -> Box<dyn Reply> {
    let (status, error) = match failure {
        ControlAuthFailure::RequiresPassword => {
            (StatusCode::FORBIDDEN, "control_requires_password")
        }
        ControlAuthFailure::ModelControlDisabled => {
            (StatusCode::FORBIDDEN, "model_control_disabled")
        }
        ControlAuthFailure::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
    };
    Box::new(warp::reply::with_status(
        warp::reply::json(&ErrorResponse {
            error: error.to_string(),
        }),
        status,
    ))
}

/// Handle GET /api/v1/control/models
async fn handle_get_model_control<T: ServerContext + 'static>(
    auth_key: Option<String>,
    ctx: Arc<RwLock<T>>,
) -> Result<Box<dyn Reply>, Rejection> {
    let ctx = ctx.read().await;
    if let Err(failure) =
        check_control_auth(&ctx.get_password(), ctx.allow_model_control(), auth_key)
    {
        return Ok(control_auth_error(failure));
    }

    match ctx.get_model_control_snapshot() {
        Ok(snapshot) => Ok(Box::new(warp::reply::with_status(
            warp::reply::json(&snapshot),
            StatusCode::OK,
        ))),
        Err(error) => Ok(Box::new(warp::reply::with_status(
            warp::reply::json(&ErrorResponse { error }),
            StatusCode::INTERNAL_SERVER_ERROR,
        ))),
    }
}

/// Handle PATCH /api/v1/control/models
async fn handle_patch_model_control<T: ServerContext + 'static>(
    ctx: Arc<RwLock<T>>,
    update: RemoteModelControlUpdate,
) -> Result<Box<dyn Reply>, Rejection> {
    let ctx = ctx.write().await;

    match ctx.update_shared_model(&update.model, &update.engine) {
        Ok(snapshot) => Ok(Box::new(warp::reply::with_status(
            warp::reply::json(&snapshot),
            StatusCode::OK,
        ))),
        Err(error) => Ok(Box::new(warp::reply::with_status(
            warp::reply::json(&ErrorResponse { error }),
            StatusCode::BAD_REQUEST,
        ))),
    }
}

/// Handle GET /api/v1/status
async fn handle_status<T: ServerContext + 'static>(
    auth_key: Option<String>,
    ctx: Arc<RwLock<T>>,
) -> Result<impl Reply, Rejection> {
    let ctx = ctx.read().await;
    let server_name = ctx.get_server_name();

    info!(
        "[Remote Server] Status request received on '{}'",
        server_name
    );

    if let Some(required_password) = ctx.get_password() {
        match auth_key {
            Some(provided) if auth_matches(&provided, &required_password) => {
                info!("[Remote Server] Status request authenticated successfully");
            }
            _ => {
                warn!(
                    "[Remote Server] Status request REJECTED - authentication failed on '{}'",
                    server_name
                );
                return Ok(warp::reply::with_status(
                    warp::reply::json(&ErrorResponse {
                        error: "unauthorized".to_string(),
                    }),
                    StatusCode::UNAUTHORIZED,
                ));
            }
        }
    }

    // Use the same sharing-only identifier as discovery, never the licence hash.
    let machine_id =
        crate::remote::discovery::sharing_machine_id().unwrap_or_else(|_| "unknown".to_string());
    let (engine, model) = ctx.model_status_snapshot();
    let response = StatusResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        model,
        name: ctx.get_server_name(),
        machine_id,
        protocol_version: REMOTE_PROTOCOL_VERSION,
        engine: Some(engine.clone()),
        capabilities: Some(remote_capabilities_for_engine(&engine)),
    };

    info!(
        "[Remote Server] Status response sent: model='{}', server='{}'",
        response.model, response.name
    );

    Ok(warp::reply::with_status(
        warp::reply::json(&response),
        StatusCode::OK,
    ))
}

fn remote_capabilities_for_engine(engine: &str) -> RemoteCapabilities {
    match crate::provider_capabilities::capabilities_for_engine(engine) {
        Some(capabilities) => RemoteCapabilities {
            supports_initial_prompt: capabilities.supports_initial_prompt,
            supports_structured_terms: capabilities.supports_structured_terms,
            supports_vocabulary_terms: capabilities.supports_vocabulary_terms,
            accepts_request_context: capabilities.supports_initial_prompt,
            max_context_bytes: if capabilities.supports_initial_prompt {
                900
            } else {
                0
            },
            acceleration: vec!["cpu".to_string()],
        },
        None => RemoteCapabilities {
            acceleration: vec!["cpu".to_string()],
            ..RemoteCapabilities::default()
        },
    }
}

fn decode_context_header(encoded_context: Option<String>) -> Option<String> {
    let encoded_context = encoded_context?;
    let encoded_len = encoded_context.len();

    if encoded_len > MAX_CONTEXT_HEADER_ENCODED_BYTES {
        warn!(
            "[Remote Server] Dropping oversized encoded transcription context header: {} bytes",
            encoded_len
        );
        return None;
    }

    let decoded = match BASE64.decode(encoded_context.as_bytes()) {
        Ok(decoded) => decoded,
        Err(_) => {
            warn!("[Remote Server] Dropping invalid base64 transcription context header");
            return None;
        }
    };

    let decoded_len = decoded.len();
    if decoded_len > MAX_CONTEXT_HEADER_BYTES {
        warn!(
            "[Remote Server] Dropping oversized transcription context header: {} decoded bytes",
            decoded_len
        );
        return None;
    }

    match String::from_utf8(decoded) {
        Ok(context) => Some(context),
        Err(_) => {
            warn!("[Remote Server] Dropping non-UTF-8 transcription context header");
            None
        }
    }
}

struct TranscribeRequestParts {
    auth_key: Option<String>,
    content_type: String,
    spoken_language: Option<String>,
    transcription_task: Option<String>,
    request_context: Option<String>,
    body: bytes::Bytes,
    /// Deadline for the transcription; elapses → HTTP 408.
    transcribe_timeout: Duration,
    /// Owned semaphore permit acquired AFTER the body was buffered, so a slow
    /// or stalled upload can't hold it. Owned by the blocking transcription
    /// task and held for the full engine run (not released early on timeout or
    /// client disconnect).
    permit: OwnedSemaphorePermit,
}

/// Handle POST /api/v1/transcribe
///
/// The transcription (the blocking CPU work) is wrapped in a
/// `tokio::time::timeout` so a runaway engine run releases the single
/// serialization permit within the deadline.
async fn handle_transcribe<T: ServerContext + 'static>(
    parts: TranscribeRequestParts,
    client_addr: Option<SocketAddr>,
    ctx: Arc<RwLock<T>>,
    client_activity: ClientActivityMap,
) -> Result<impl Reply, Rejection> {
    let TranscribeRequestParts {
        auth_key,
        content_type,
        spoken_language,
        transcription_task,
        request_context,
        body,
        transcribe_timeout,
        permit,
    } = parts;

    let audio_size_kb = body.len() as f64 / 1024.0;
    let (server_name, model_name, required_password) = {
        let ctx = ctx.read().await;
        (
            ctx.get_server_name(),
            ctx.get_model_name(),
            ctx.get_password(),
        )
    };

    info!(
        "🎙️ [Remote Server] Transcription request received on '{}': {:.1} KB audio, content-type='{}'",
        server_name, audio_size_kb, content_type
    );

    // Check authentication (defense-in-depth; the auth preflight already enforced this).
    if let Some(required_password) = required_password {
        // A configured-but-empty password is not a valid credential: fail closed,
        // mirroring the auth preflight so defense-in-depth agrees even if a request
        // reaches here with an empty configured password.
        if required_password.is_empty() {
            warn!(
                "[Remote Server] Transcription request REJECTED - empty configured password on '{}'",
                server_name
            );
            return Ok(warp::reply::with_status(
                warp::reply::json(&ErrorResponse {
                    error: "unauthorized".to_string(),
                }),
                StatusCode::UNAUTHORIZED,
            ));
        }
        match auth_key {
            Some(provided) if auth_matches(&provided, &required_password) => {
                info!("[Remote Server] Transcription request authenticated successfully");
            }
            _ => {
                warn!(
                    "[Remote Server] Transcription request REJECTED - authentication failed on '{}'",
                    server_name
                );
                return Ok(warp::reply::with_status(
                    warp::reply::json(&ErrorResponse {
                        error: "unauthorized".to_string(),
                    }),
                    StatusCode::UNAUTHORIZED,
                ));
            }
        }
    }

    // Validate content type
    if !content_type.starts_with("audio/") {
        warn!(
            "[Remote Server] Transcription request REJECTED - unsupported content type: '{}'",
            content_type
        );
        return Ok(warp::reply::with_status(
            warp::reply::json(&ErrorResponse {
                error: "unsupported_media_type".to_string(),
            }),
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
        ));
    }

    // Record client IP for active-connections counting (rolling-window distinct clients).
    // We record after auth + content-type pass so only genuine requests are counted.
    if let Some(addr) = client_addr {
        if let Ok(mut map) = client_activity.lock() {
            let now = Instant::now();
            // Prune stale entries on every write so the map is bounded to
            // distinct-recent IPs regardless of how often get_status() is called.
            prune_stale_clients(&mut map, now, RECENT_CLIENT_WINDOW);
            map.insert(addr.ip(), now);
        }
    }

    info!(
        "[Remote Server] Starting transcription with model '{}' for {:.1} KB audio",
        model_name, audio_size_kb
    );
    let request_context = decode_context_header(request_context);

    // The single serialization permit is OWNED BY THE BLOCKING TASK, not the
    // async scope: it is released only when the engine work actually finishes,
    // even if we stop waiting for it. So neither a timeout nor a client
    // disconnect frees the permit early — a subsequent remote request correctly
    // waits until the host engine is genuinely free (remote-vs-remote stays
    // serialized). The timeout only bounds how long the CLIENT waits: on elapse
    // we return 408 and the detached engine task drains in the background
    // (spawn_blocking cannot be truly cancelled).
    let ctx_for_blocking = ctx.clone();
    let outcome = tokio::time::timeout(transcribe_timeout, async move {
        let body_for_blocking = body.clone();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let guard = ctx_for_blocking.blocking_read();
            guard.transcribe_with_context(
                &body_for_blocking,
                spoken_language.as_deref(),
                transcription_task.as_deref(),
                request_context.as_deref(),
            )
        })
        .await
        .unwrap_or_else(|join_err| Err(format!("transcription task failed: {join_err}")))
    })
    .await;

    match outcome {
        Err(_elapsed) => {
            warn!(
                "⏱️ [Remote Server] Transcription TIMED OUT on '{}' after {:?}; \
                 returned 408 to client. Engine work continues in the background \
                 and keeps the serialization permit until it completes.",
                server_name, transcribe_timeout
            );
            Ok(warp::reply::with_status(
                warp::reply::json(&ErrorResponse {
                    error: "transcription_timeout".to_string(),
                }),
                StatusCode::REQUEST_TIMEOUT, // 408
            ))
        }
        Ok(Ok(result)) => {
            let response = TranscribeResponse {
                text: result.raw_text,
                duration_ms: result.timings.processing_duration_ms.unwrap_or_default(),
                model: result.model,
                transcript_language: result.transcript_language,
            };
            info!(
                "🎯 [Remote Server] Transcription COMPLETED on '{}': {} chars in {}ms using '{}'",
                server_name,
                response.text.len(),
                response.duration_ms,
                response.model
            );
            Ok(warp::reply::with_status(
                warp::reply::json(&response),
                StatusCode::OK,
            ))
        }
        Ok(Err(error)) => {
            warn!(
                "[Remote Server] Transcription FAILED on '{}': {}",
                server_name, error
            );
            Ok(warp::reply::with_status(
                warp::reply::json(&ErrorResponse {
                    error: "transcription_failed".to_string(),
                }),
                StatusCode::INTERNAL_SERVER_ERROR,
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::server::ShareableRemoteModelInfo;

    fn mock_model_control_snapshot(model_name: &str) -> RemoteModelControlSnapshot {
        RemoteModelControlSnapshot {
            current: ShareableRemoteModelInfo {
                id: model_name.to_string(),
                display_name: format!("{model_name} display"),
                engine: "whisper".to_string(),
                recommended: Some(true),
                speed_score: Some(8),
                accuracy_score: Some(7),
            },
            available: vec![ShareableRemoteModelInfo {
                id: model_name.to_string(),
                display_name: format!("{model_name} display"),
                engine: "whisper".to_string(),
                recommended: Some(true),
                speed_score: Some(8),
                accuracy_score: Some(7),
            }],
        }
    }
    fn test_transcription_guard() -> Arc<Semaphore> {
        Arc::new(Semaphore::new(1))
    }

    fn test_client_activity() -> ClientActivityMap {
        Arc::new(Mutex::new(std::collections::HashMap::new()))
    }

    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::sync::Mutex;
    use std::time::Duration;
    use tokio::time::sleep;

    struct MockContext;

    impl ServerContext for MockContext {
        fn get_model_name(&self) -> String {
            "mock-model".to_string()
        }
        fn get_server_name(&self) -> String {
            "mock-server".to_string()
        }
        fn get_password(&self) -> Option<String> {
            None
        }
        fn transcribe(
            &self,
            _audio_data: &[u8],
            _spoken_language: Option<&str>,
            _transcription_task: Option<&str>,
        ) -> Result<TranscriptionResult, String> {
            let job = crate::transcription::TranscriptionJob::from_legacy_settings(
                crate::transcription::TranscriptionSource::RemoteServer,
                "remote",
                "mock-model",
                None,
                false,
            );
            Ok(
                crate::transcription::TranscriptionResult::new(&job, "mock transcription")
                    .with_processing_duration_ms(Some(100)),
            )
        }

        fn get_model_control_snapshot(&self) -> Result<RemoteModelControlSnapshot, String> {
            Ok(mock_model_control_snapshot("mock-model"))
        }
        fn update_shared_model(
            &self,
            model_id: &str,
            engine: &str,
        ) -> Result<RemoteModelControlSnapshot, String> {
            if engine != "whisper" && engine != "parakeet" {
                return Err(format!("Unsupported sharing engine '{engine}'"));
            }
            Ok(mock_model_control_snapshot(model_id))
        }
    }

    /// Mock context with configurable delay to simulate transcription time
    struct DelayedMockContext {
        delay_ms: u64,
        request_counter: AtomicU32,
        active_transcriptions: AtomicU32,
        max_concurrent: AtomicU32,
    }

    impl DelayedMockContext {
        fn new(delay_ms: u64) -> Self {
            Self {
                delay_ms,
                request_counter: AtomicU32::new(0),
                active_transcriptions: AtomicU32::new(0),
                max_concurrent: AtomicU32::new(0),
            }
        }

        fn max_concurrent_transcriptions(&self) -> u32 {
            self.max_concurrent.load(Ordering::SeqCst)
        }
    }

    impl ServerContext for DelayedMockContext {
        fn get_model_name(&self) -> String {
            "delayed-mock-model".to_string()
        }
        fn get_server_name(&self) -> String {
            "delayed-mock-server".to_string()
        }
        fn get_password(&self) -> Option<String> {
            None
        }
        fn transcribe(
            &self,
            audio_data: &[u8],
            _spoken_language: Option<&str>,
            _transcription_task: Option<&str>,
        ) -> Result<TranscriptionResult, String> {
            // Increment request counter
            let request_num = self.request_counter.fetch_add(1, Ordering::SeqCst) + 1;

            let current = self.active_transcriptions.fetch_add(1, Ordering::SeqCst) + 1;
            let mut max = self.max_concurrent.load(Ordering::SeqCst);
            while current > max {
                match self.max_concurrent.compare_exchange(
                    max,
                    current,
                    Ordering::SeqCst,
                    Ordering::SeqCst,
                ) {
                    Ok(_) => break,
                    Err(actual) => max = actual,
                }
            }

            // Simulate transcription delay (blocking, as real transcription would be)
            std::thread::sleep(std::time::Duration::from_millis(self.delay_ms));

            self.active_transcriptions.fetch_sub(1, Ordering::SeqCst);

            let job = crate::transcription::TranscriptionJob::from_legacy_settings(
                crate::transcription::TranscriptionSource::RemoteServer,
                "remote",
                "delayed-mock-model",
                None,
                false,
            );

            Ok(crate::transcription::TranscriptionResult::new(
                &job,
                format!("transcription-{}-len-{}", request_num, audio_data.len()),
            )
            .with_processing_duration_ms(Some(self.delay_ms)))
        }

        fn get_model_control_snapshot(&self) -> Result<RemoteModelControlSnapshot, String> {
            Ok(mock_model_control_snapshot("delayed-mock-model"))
        }
        fn update_shared_model(
            &self,
            model_id: &str,
            engine: &str,
        ) -> Result<RemoteModelControlSnapshot, String> {
            if engine != "whisper" && engine != "parakeet" {
                return Err(format!("Unsupported sharing engine '{engine}'"));
            }
            Ok(mock_model_control_snapshot(model_id))
        }
    }

    /// Mock context that PARKS `transcribe` on a blocking gate until the test
    /// releases it, and signals entry via `started`. Used to prove
    /// DETERMINISTICALLY (no wall-clock threshold) that GET /status is not
    /// serialized behind an in-flight transcription: the transcribe task blocks
    /// while the route holds only a shared READ guard on the context, and the
    /// test asserts /status still succeeds with the gate still closed. A
    /// serialized /status would instead hang and hit the watchdog.
    struct GatedMockContext {
        /// Set true as soon as `transcribe` is entered, before it parks. Held
        /// as `Arc` so the test can poll it without locking the context.
        started: Arc<AtomicBool>,
        /// Blocking release gate: `transcribe` parks on `recv()`. The test
        /// holds the matching `Sender` and drops it to open the gate. Wrapped
        /// in a `Mutex` because `mpsc::Receiver` is `Send` but not `Sync`;
        /// only the single transcribe consumer ever locks it (no contention).
        release: Arc<std::sync::Mutex<std::sync::mpsc::Receiver<()>>>,
    }

    impl GatedMockContext {
        /// Returns the context plus the `started` flag (to await entry) and the
        /// release `Sender` (drop to unblock transcription).
        fn new() -> (Self, Arc<AtomicBool>, std::sync::mpsc::Sender<()>) {
            let started = Arc::new(AtomicBool::new(false));
            let (release_tx, release_rx) = std::sync::mpsc::channel();
            let ctx = Self {
                started: started.clone(),
                release: Arc::new(std::sync::Mutex::new(release_rx)),
            };
            (ctx, started, release_tx)
        }
    }

    impl ServerContext for GatedMockContext {
        fn get_model_name(&self) -> String {
            "gated-mock-model".to_string()
        }
        fn get_server_name(&self) -> String {
            "gated-mock-server".to_string()
        }
        fn get_password(&self) -> Option<String> {
            None
        }
        fn transcribe(
            &self,
            audio_data: &[u8],
            _spoken_language: Option<&str>,
            _transcription_task: Option<&str>,
        ) -> Result<TranscriptionResult, String> {
            // Signal entry, then park on the release gate. This runs inside the
            // blocking transcription task WHILE the route holds a shared READ
            // guard on the context RwLock. /status also takes only a READ guard
            // and tokio's RwLock admits concurrent readers, so this parked
            // transcribe never blocks /status — exactly the property under
            // test. Holding the read guard here is also what keeps the test
            // meaningful: a write-lock regression would make /status hang.
            self.started.store(true, Ordering::SeqCst);
            let _ = self
                .release
                .lock()
                .expect("release gate mutex poisoned")
                .recv();
            let job = crate::transcription::TranscriptionJob::from_legacy_settings(
                crate::transcription::TranscriptionSource::RemoteServer,
                "remote",
                "gated-mock-model",
                None,
                false,
            );
            Ok(crate::transcription::TranscriptionResult::new(
                &job,
                format!("gated-transcription-len-{}", audio_data.len()),
            ))
        }
        fn get_model_control_snapshot(&self) -> Result<RemoteModelControlSnapshot, String> {
            Ok(mock_model_control_snapshot("gated-mock-model"))
        }
        fn update_shared_model(
            &self,
            model_id: &str,
            engine: &str,
        ) -> Result<RemoteModelControlSnapshot, String> {
            if engine != "whisper" && engine != "parakeet" {
                return Err(format!("Unsupported sharing engine '{engine}'"));
            }
            Ok(mock_model_control_snapshot(model_id))
        }
    }

    /// Mock context that fails on specific request numbers
    struct FailingMockContext {
        fail_on_requests: Vec<u32>,
        request_counter: AtomicU32,
    }

    impl FailingMockContext {
        fn new(fail_on_requests: Vec<u32>) -> Self {
            Self {
                fail_on_requests,
                request_counter: AtomicU32::new(0),
            }
        }
    }

    impl ServerContext for FailingMockContext {
        fn get_model_name(&self) -> String {
            "failing-mock-model".to_string()
        }
        fn get_server_name(&self) -> String {
            "failing-mock-server".to_string()
        }
        fn get_password(&self) -> Option<String> {
            None
        }
        fn transcribe(
            &self,
            _audio_data: &[u8],
            _spoken_language: Option<&str>,
            _transcription_task: Option<&str>,
        ) -> Result<TranscriptionResult, String> {
            let request_num = self.request_counter.fetch_add(1, Ordering::SeqCst) + 1;

            if self.fail_on_requests.contains(&request_num) {
                return Err(format!("Simulated failure on request {}", request_num));
            }

            let job = crate::transcription::TranscriptionJob::from_legacy_settings(
                crate::transcription::TranscriptionSource::RemoteServer,
                "remote",
                "failing-mock-model",
                None,
                false,
            );

            Ok(crate::transcription::TranscriptionResult::new(
                &job,
                format!("success-{}", request_num),
            )
            .with_processing_duration_ms(Some(10)))
        }

        fn get_model_control_snapshot(&self) -> Result<RemoteModelControlSnapshot, String> {
            Ok(mock_model_control_snapshot("failing-mock-model"))
        }
        fn update_shared_model(
            &self,
            model_id: &str,
            engine: &str,
        ) -> Result<RemoteModelControlSnapshot, String> {
            if engine != "whisper" && engine != "parakeet" {
                return Err(format!("Unsupported sharing engine '{engine}'"));
            }
            Ok(mock_model_control_snapshot(model_id))
        }
    }

    #[test]
    fn test_mock_context() {
        let ctx = MockContext;
        assert_eq!(ctx.get_model_name(), "mock-model");
        assert_eq!(ctx.get_server_name(), "mock-server");
        assert!(ctx.get_password().is_none());
    }
    struct ContextCaptureMock {
        captured_context: Arc<Mutex<Option<String>>>,
    }

    impl ServerContext for ContextCaptureMock {
        fn get_model_name(&self) -> String {
            "mock-model".to_string()
        }

        fn get_server_name(&self) -> String {
            "mock-server".to_string()
        }

        fn get_password(&self) -> Option<String> {
            None
        }

        fn transcribe(
            &self,
            _audio_data: &[u8],
            _spoken_language: Option<&str>,
            _transcription_task: Option<&str>,
        ) -> Result<TranscriptionResult, String> {
            Err("transcribe_with_context should be used".to_string())
        }

        fn transcribe_with_context(
            &self,
            _audio_data: &[u8],
            _spoken_language: Option<&str>,
            _transcription_task: Option<&str>,
            context: Option<&str>,
        ) -> Result<TranscriptionResult, String> {
            *self.captured_context.lock().unwrap() = context.map(str::to_string);

            let job = crate::transcription::TranscriptionJob::from_legacy_settings(
                crate::transcription::TranscriptionSource::RemoteServer,
                "remote",
                "mock-model",
                None,
                false,
            );
            Ok(crate::transcription::TranscriptionResult::new(
                &job,
                "mock transcription",
            ))
        }
    }

    struct ConsistentStatusMock;

    impl ServerContext for ConsistentStatusMock {
        fn get_model_name(&self) -> String {
            "stale-model".to_string()
        }

        fn get_server_name(&self) -> String {
            "consistent-server".to_string()
        }

        fn get_password(&self) -> Option<String> {
            None
        }

        fn get_engine(&self) -> String {
            "whisper".to_string()
        }

        fn model_status_snapshot(&self) -> (String, String) {
            ("parakeet".to_string(), "nano".to_string())
        }

        fn transcribe(
            &self,
            _audio_data: &[u8],
            _spoken_language: Option<&str>,
            _transcription_task: Option<&str>,
        ) -> Result<TranscriptionResult, String> {
            Err("unused".to_string())
        }
    }

    #[tokio::test]
    async fn test_status_uses_single_model_status_snapshot() {
        let ctx = Arc::new(RwLock::new(ConsistentStatusMock));
        let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

        let response = warp::test::request()
            .method("GET")
            .path("/api/v1/status")
            .reply(&routes)
            .await;

        assert_eq!(response.status(), 200);

        let body: StatusResponse = serde_json::from_slice(response.body()).unwrap();
        assert_eq!(body.model, "nano");
        assert_eq!(body.engine.as_deref(), Some("parakeet"));
        let caps = body.capabilities.expect("capabilities");
        assert!(
            !caps.accepts_request_context,
            "parakeet capabilities must not use whisper initial-prompt caps"
        );
    }

    struct SlowTranscribeMock {
        started: Arc<AtomicBool>,
        finished: Arc<AtomicBool>,
    }

    impl ServerContext for SlowTranscribeMock {
        fn get_model_name(&self) -> String {
            "slow-model".to_string()
        }

        fn get_server_name(&self) -> String {
            "slow-server".to_string()
        }

        fn get_password(&self) -> Option<String> {
            None
        }

        fn transcribe(
            &self,
            _audio_data: &[u8],
            _spoken_language: Option<&str>,
            _transcription_task: Option<&str>,
        ) -> Result<TranscriptionResult, String> {
            Err("use transcribe_with_context".to_string())
        }

        fn transcribe_with_context(
            &self,
            _audio_data: &[u8],
            _spoken_language: Option<&str>,
            _transcription_task: Option<&str>,
            _context: Option<&str>,
        ) -> Result<TranscriptionResult, String> {
            self.started.store(true, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(800));
            self.finished.store(true, Ordering::SeqCst);
            let job = crate::transcription::TranscriptionJob::from_legacy_settings(
                crate::transcription::TranscriptionSource::RemoteServer,
                "remote",
                "slow-model",
                None,
                false,
            );
            Ok(crate::transcription::TranscriptionResult::new(&job, "done"))
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn transcribe_owned_permit_held_when_handler_future_dropped() {
        let started = Arc::new(AtomicBool::new(false));
        let finished = Arc::new(AtomicBool::new(false));
        let guard = test_transcription_guard();
        let ctx = Arc::new(RwLock::new(SlowTranscribeMock {
            started: started.clone(),
            finished: finished.clone(),
        }));

        let ctx_for_handler = ctx.clone();
        // Acquire the permit explicitly so the test can hand it to the handler
        // (mirroring what the filter chain does before the body is read).
        let permit = guard
            .clone()
            .try_acquire_owned()
            .expect("semaphore has capacity");
        let handler_task = tokio::spawn(async move {
            handle_transcribe(
                TranscribeRequestParts {
                    auth_key: None,
                    content_type: "audio/wav".to_string(),
                    spoken_language: None,
                    transcription_task: None,
                    request_context: None,
                    body: bytes::Bytes::from_static(b"audio"),
                    transcribe_timeout: REMOTE_TRANSCRIBE_TIMEOUT,
                    permit,
                },
                None, // client_addr — not relevant to this permit-hold test
                ctx_for_handler,
                test_client_activity(),
            )
            .await
        });

        tokio::time::timeout(Duration::from_secs(2), async {
            while !started.load(Ordering::SeqCst) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("transcription should start");

        handler_task.abort();
        let _ = handler_task.await;

        let acquire_started = std::time::Instant::now();
        let permit = guard
            .acquire_owned()
            .await
            .expect("semaphore should remain held until blocking work completes");
        drop(permit);

        assert!(
            acquire_started.elapsed() >= Duration::from_millis(300),
            "dropping the handler future must not release the transcription permit early"
        );
        assert!(
            finished.load(Ordering::SeqCst),
            "blocking transcription should run to completion"
        );
    }

    #[tokio::test]
    async fn test_status_advertises_protocol_engine_and_capabilities() {
        let ctx = Arc::new(RwLock::new(MockContext));
        let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

        let response = warp::test::request()
            .method("GET")
            .path("/api/v1/status")
            .reply(&routes)
            .await;

        assert_eq!(response.status(), 200);

        let body: StatusResponse = serde_json::from_slice(response.body()).unwrap();
        assert_eq!(body.protocol_version, REMOTE_PROTOCOL_VERSION);
        assert_eq!(body.engine.as_deref(), Some("whisper"));
        assert!(
            body.capabilities
                .as_ref()
                .expect("capabilities should be advertised")
                .accepts_request_context
        );
    }

    #[tokio::test]
    async fn test_transcribe_forwards_context_header() {
        let captured_context = Arc::new(Mutex::new(None));
        let ctx = Arc::new(RwLock::new(ContextCaptureMock {
            captured_context: captured_context.clone(),
        }));
        let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

        let context = "project glossary: José 中";
        let encoded_context = BASE64.encode(context.as_bytes());

        let response = warp::test::request()
            .method("POST")
            .path("/api/v1/transcribe")
            .header("Content-Type", "audio/wav")
            .header(CONTEXT_HEADER, encoded_context)
            .body(b"audio")
            .reply(&routes)
            .await;

        assert_eq!(response.status(), 200);
        assert_eq!(captured_context.lock().unwrap().as_deref(), Some(context));
    }

    #[tokio::test]
    async fn test_transcribe_without_context_header_passes_none() {
        let captured_context = Arc::new(Mutex::new(Some("stale".to_string())));
        let ctx = Arc::new(RwLock::new(ContextCaptureMock {
            captured_context: captured_context.clone(),
        }));
        let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

        let response = warp::test::request()
            .method("POST")
            .path("/api/v1/transcribe")
            .header("Content-Type", "audio/wav")
            .body(b"audio")
            .reply(&routes)
            .await;

        assert_eq!(response.status(), 200);
        assert_eq!(captured_context.lock().unwrap().as_deref(), None);
    }

    #[tokio::test]
    async fn test_transcribe_drops_oversized_context_header() {
        let captured_context = Arc::new(Mutex::new(Some("stale".to_string())));
        let ctx = Arc::new(RwLock::new(ContextCaptureMock {
            captured_context: captured_context.clone(),
        }));
        let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());
        let oversized_context = BASE64.encode(vec![b'a'; MAX_CONTEXT_HEADER_BYTES + 1]);

        let response = warp::test::request()
            .method("POST")
            .path("/api/v1/transcribe")
            .header("Content-Type", "audio/wav")
            .header(CONTEXT_HEADER, oversized_context)
            .body(b"audio")
            .reply(&routes)
            .await;

        assert_eq!(response.status(), 200);
        assert_eq!(captured_context.lock().unwrap().as_deref(), None);
    }

    #[tokio::test]
    async fn test_transcribe_drops_invalid_base64_context() {
        let captured_context = Arc::new(Mutex::new(Some("stale".to_string())));
        let ctx = Arc::new(RwLock::new(ContextCaptureMock {
            captured_context: captured_context.clone(),
        }));
        let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

        let response = warp::test::request()
            .method("POST")
            .path("/api/v1/transcribe")
            .header("Content-Type", "audio/wav")
            .header(CONTEXT_HEADER, "not-valid-base64!")
            .body(b"audio")
            .reply(&routes)
            .await;

        assert_eq!(response.status(), 200);
        assert_eq!(captured_context.lock().unwrap().as_deref(), None);
    }

    // ============================================================================
    // Regression Tests for P1/P2 fixes
    // ============================================================================

    #[tokio::test]
    async fn transcribe_rejects_body_exceeding_max_body() {
        // Inject a tiny guard (1 KB) so we can test the rejection without
        // allocating gigabytes. Production uses MAX_AUDIO_BODY_BYTES (512 MB).
        let ctx = Arc::new(RwLock::new(MockContext));
        let routes = create_routes_with_limits(
            ctx,
            test_transcription_guard(),
            test_client_activity(),
            REMOTE_TRANSCRIBE_TIMEOUT,
            1024, // 1 KB guard
        );

        // 2 KB body exceeds the 1 KB injected guard → 413.
        let response = warp::test::request()
            .method("POST")
            .path("/api/v1/transcribe")
            .header("Content-Type", "audio/wav")
            .body(vec![0u8; 2048])
            .reply(&routes)
            .await;

        assert_eq!(response.status(), 413);
    }

    #[tokio::test]
    async fn transcribe_accepts_body_above_old_50mb_cap() {
        // 60 MB — comfortably above the former 50 MB cap. The body is buffered
        // in memory under the 512 MB ceiling, so this is accepted instead of 413.
        let ctx = Arc::new(RwLock::new(MockContext));
        let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

        let large_audio = vec![0u8; 60 * 1024 * 1024];

        let response = warp::test::request()
            .method("POST")
            .path("/api/v1/transcribe")
            .header("Content-Type", "audio/wav")
            .body(large_audio)
            .reply(&routes)
            .await;

        assert_eq!(response.status(), 200);
        let body: TranscribeResponse = serde_json::from_slice(response.body()).unwrap();
        assert_eq!(body.text, "mock transcription");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn transcribe_timeout_returns_408_then_serializes_next() {
        // The slow context sleeps 500 ms inside transcription; the fast context
        // returns instantly. Both share the same single-permit guard. The 100 ms
        // timeout makes the first request return 408 while its DETACHED engine
        // task keeps running and KEEPS THE PERMIT until it drains (~400 ms more).
        let slow_ctx = Arc::new(RwLock::new(DelayedMockContext::new(500)));
        let fast_ctx = Arc::new(RwLock::new(MockContext));
        let guard = Arc::new(Semaphore::new(1));

        let slow_routes = create_routes_with_limits(
            slow_ctx,
            guard.clone(),
            test_client_activity(),
            Duration::from_millis(100), // 100 ms timeout
            MAX_AUDIO_BODY_BYTES,
        );
        let fast_routes = create_routes_with_limits(
            fast_ctx,
            guard.clone(),
            test_client_activity(),
            Duration::from_secs(5),
            MAX_AUDIO_BODY_BYTES,
        );

        // First request: transcription sleeps 500 ms, timeout fires at 100 ms → 408.
        let slow_routes_clone = slow_routes.clone();
        let response = tokio::time::timeout(
            Duration::from_secs(5),
            warp::test::request()
                .method("POST")
                .path("/api/v1/transcribe")
                .header("Content-Type", "audio/wav")
                .body(vec![0u8; 100])
                .reply(&slow_routes_clone),
        )
        .await
        .expect("slow request should resolve (with timeout) within 5 s");

        assert_eq!(response.status(), 408);
        let body: ErrorResponse = serde_json::from_slice(response.body()).unwrap();
        assert_eq!(body.error, "transcription_timeout");

        // Second request on the fast context with the SAME guard: it must
        // SERIALIZE behind the first request's still-draining engine task and
        // cannot complete until that task releases the permit. We TIME it to
        // PROVE the permit is not released the instant the 408 is returned: the
        // slow engine task still has ~400 ms to run, so B must wait that long.
        // If the timeout released the permit early, B (instant context) would
        // finish in well under 100 ms and this lower bound would catch it.
        let b_start = std::time::Instant::now();
        let response2 = warp::test::request()
            .method("POST")
            .path("/api/v1/transcribe")
            .header("Content-Type", "audio/wav")
            .body(vec![0u8; 100])
            .reply(&fast_routes)
            .await;
        let b_elapsed = b_start.elapsed();

        assert_eq!(response2.status(), 200);
        assert!(
            b_elapsed >= Duration::from_millis(250),
            "second request completed in {b_elapsed:?}; the 408 released the \
             transcription permit early — it must wait ~400 ms for the first \
             request's detached engine task to finish and free the permit"
        );
    }

    struct Utf8PreviewContext;

    impl ServerContext for Utf8PreviewContext {
        fn get_model_name(&self) -> String {
            "utf8-preview-model".to_string()
        }

        fn get_server_name(&self) -> String {
            "utf8-preview-server".to_string()
        }

        fn get_password(&self) -> Option<String> {
            None
        }

        fn transcribe(
            &self,
            _audio_data: &[u8],
            _spoken_language: Option<&str>,
            _transcription_task: Option<&str>,
        ) -> Result<TranscriptionResult, String> {
            let job = crate::transcription::TranscriptionJob::from_legacy_settings(
                crate::transcription::TranscriptionSource::RemoteServer,
                "remote",
                "utf8-preview-model",
                None,
                false,
            );
            Ok(
                crate::transcription::TranscriptionResult::new(
                    &job,
                    format!("{}é", "a".repeat(99)),
                )
                .with_processing_duration_ms(Some(1)),
            )
        }

        fn get_model_control_snapshot(&self) -> Result<RemoteModelControlSnapshot, String> {
            Ok(mock_model_control_snapshot("utf8-preview-model"))
        }
        fn update_shared_model(
            &self,
            model_id: &str,
            engine: &str,
        ) -> Result<RemoteModelControlSnapshot, String> {
            if engine != "whisper" && engine != "parakeet" {
                return Err(format!("Unsupported sharing engine '{engine}'"));
            }
            Ok(mock_model_control_snapshot(model_id))
        }
    }

    #[tokio::test]
    async fn test_transcribe_endpoint_handles_multibyte_preview_safely() {
        let ctx = Arc::new(RwLock::new(Utf8PreviewContext));
        let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

        let response = warp::test::request()
            .method("POST")
            .path("/api/v1/transcribe")
            .header("Content-Type", "audio/wav")
            .body(b"audio")
            .reply(&routes)
            .await;

        assert_eq!(response.status(), 200);

        let body: TranscribeResponse = serde_json::from_slice(response.body()).unwrap();
        assert_eq!(body.text, format!("{}é", "a".repeat(99)));
    }

    #[tokio::test]
    async fn test_status_requests_are_not_blocked_by_transcription() {
        // Deterministic latch/gate proof (no wall-clock threshold): the mock
        // transcription parks on a blocking gate, and we assert /status
        // succeeds WHILE that gate is still closed. If /status were serialized
        // behind transcription it would hang and hit the 5s watchdog.
        let (mock_ctx, started, release_tx) = GatedMockContext::new();
        let context = Arc::new(RwLock::new(mock_ctx));

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();

        let server_context = context.clone();
        let server_handle = tokio::spawn(async move {
            let addr = ([127, 0, 0, 1], 0u16);
            let routes = create_routes(
                server_context,
                test_transcription_guard(),
                test_client_activity(),
            );

            let (addr, server) = warp::serve(routes).bind_with_graceful_shutdown(addr, async {
                shutdown_rx.await.ok();
            });

            let _ = addr_tx.send(addr);
            server.await;
        });

        let addr = addr_rx.await.expect("Server failed to start");
        sleep(Duration::from_millis(50)).await;

        let client = reqwest::Client::new();
        let transcribe_url = format!("http://{}/api/v1/transcribe", addr);
        let status_url = format!("http://{}/api/v1/status", addr);

        // Fire transcription: it enters `transcribe`, signals `started`, then
        // parks on the gate (held closed by `release_tx`).
        let transcribe_handle = tokio::spawn({
            let client = client.clone();
            let url = transcribe_url.clone();
            async move {
                client
                    .post(&url)
                    .header("Content-Type", "audio/wav")
                    .body(b"audio".as_slice())
                    .timeout(Duration::from_secs(5))
                    .send()
                    .await
            }
        });

        // Deterministic latch: wait until transcription has provably entered its
        // blocking path and parked on the gate — no fixed sleep.
        tokio::time::timeout(Duration::from_secs(5), async {
            while !started.load(Ordering::SeqCst) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("transcription never entered its blocking path within 5s");

        // THE PROOF: with the gate still closed (transcription provably parked),
        // /status MUST succeed. A serialized /status would block here and trip
        // the 5s watchdog. No latency is asserted — only completion ordering.
        let status_response = tokio::time::timeout(
            Duration::from_secs(5),
            client
                .get(&status_url)
                .timeout(Duration::from_secs(5))
                .send(),
        )
        .await
        .expect("status request hung — it was serialized behind transcription")
        .expect("status request failed");
        assert!(
            status_response.status().is_success(),
            "status failed: {}",
            status_response.status()
        );

        // Release the gate so transcription can finish.
        drop(release_tx);

        let transcribe_response = tokio::time::timeout(Duration::from_secs(5), transcribe_handle)
            .await
            .expect("transcription did not finish within 5s of gate release")
            .expect("transcribe task panicked")
            .expect("transcribe request failed");
        assert!(transcribe_response.status().is_success());

        let _ = shutdown_tx.send(());
        let _ = tokio::time::timeout(Duration::from_secs(5), server_handle).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn transcribe_runs_off_runtime_worker() {
        let context = Arc::new(RwLock::new(DelayedMockContext::new(300)));
        let routes = create_routes(
            context.clone(),
            test_transcription_guard(),
            test_client_activity(),
        );

        let transcribe_routes = routes.clone();
        let started_at = std::time::Instant::now();
        let transcribe_handle = tokio::spawn(async move {
            warp::test::request()
                .method("POST")
                .path("/api/v1/transcribe")
                .header("Content-Type", "audio/wav")
                .body(b"audio")
                .reply(&transcribe_routes)
                .await
        });

        tokio::time::timeout(Duration::from_millis(150), async {
            loop {
                if context.read().await.request_counter.load(Ordering::SeqCst) > 0 {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("transcription should start without pinning the runtime worker");

        let status_response = tokio::time::timeout(
            Duration::from_millis(150),
            warp::test::request()
                .method("GET")
                .path("/api/v1/status")
                .reply(&routes),
        )
        .await
        .expect("status route should respond while transcription is in flight");

        assert_eq!(status_response.status(), 200);
        assert!(
            started_at.elapsed() < Duration::from_millis(150),
            "status route waited for blocking transcription to finish"
        );

        let transcribe_response = transcribe_handle.await.expect("transcribe task panicked");
        assert_eq!(transcribe_response.status(), 200);
    }

    #[tokio::test]
    async fn test_concurrent_transcribe_requests_serialize() {
        let context = Arc::new(RwLock::new(DelayedMockContext::new(100)));

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();

        let server_context = context.clone();
        let server_handle = tokio::spawn(async move {
            let addr = ([127, 0, 0, 1], 0u16);
            let routes = create_routes(
                server_context,
                test_transcription_guard(),
                test_client_activity(),
            );

            let (addr, server) = warp::serve(routes).bind_with_graceful_shutdown(addr, async {
                shutdown_rx.await.ok();
            });

            let _ = addr_tx.send(addr);
            server.await;
        });

        let addr = addr_rx.await.expect("Server failed to start");
        sleep(Duration::from_millis(50)).await;

        let client = reqwest::Client::new();
        let transcribe_url = format!("http://{}/api/v1/transcribe", addr);
        let audio_data = b"audio".to_vec();

        let req1 = {
            let client = client.clone();
            let url = transcribe_url.clone();
            let audio_data = audio_data.clone();
            tokio::spawn(async move {
                client
                    .post(&url)
                    .header("Content-Type", "audio/wav")
                    .body(audio_data)
                    .timeout(Duration::from_secs(10))
                    .send()
                    .await
            })
        };
        let req2 = {
            let client = client.clone();
            let url = transcribe_url.clone();
            let audio_data = audio_data.clone();
            tokio::spawn(async move {
                client
                    .post(&url)
                    .header("Content-Type", "audio/wav")
                    .body(audio_data)
                    .timeout(Duration::from_secs(10))
                    .send()
                    .await
            })
        };

        let start = std::time::Instant::now();
        let (result1, result2) = tokio::join!(req1, req2);
        let elapsed = start.elapsed();

        assert!(result1
            .expect("task 1 panicked")
            .expect("request 1 failed")
            .status()
            .is_success());
        assert!(result2
            .expect("task 2 panicked")
            .expect("request 2 failed")
            .status()
            .is_success());
        assert!(
            elapsed >= Duration::from_millis(180),
            "Concurrent transcribe requests should serialize; elapsed {:?}",
            elapsed
        );

        let ctx = context.read().await;
        assert_eq!(
            ctx.max_concurrent_transcriptions(),
            1,
            "Only one transcription should run at a time"
        );

        let _ = shutdown_tx.send(());
        let _ = tokio::time::timeout(Duration::from_secs(5), server_handle).await;
    }

    #[tokio::test]
    async fn test_transcription_guard_is_shared_across_route_instances() {
        let context = Arc::new(RwLock::new(DelayedMockContext::new(100)));
        let guard = Arc::new(Semaphore::new(1));
        let routes_a = create_routes(context.clone(), guard.clone(), test_client_activity());
        let routes_b = create_routes(context.clone(), guard, test_client_activity());

        let req_a = tokio::spawn(async move {
            warp::test::request()
                .method("POST")
                .path("/api/v1/transcribe")
                .header("Content-Type", "audio/wav")
                .body(b"audio-a")
                .reply(&routes_a)
                .await
        });
        let req_b = tokio::spawn(async move {
            warp::test::request()
                .method("POST")
                .path("/api/v1/transcribe")
                .header("Content-Type", "audio/wav")
                .body(b"audio-b")
                .reply(&routes_b)
                .await
        });

        let (response_a, response_b) = tokio::join!(req_a, req_b);

        assert_eq!(response_a.expect("request A panicked").status(), 200);
        assert_eq!(response_b.expect("request B panicked").status(), 200);
        assert_eq!(context.read().await.max_concurrent_transcriptions(), 1);
    }

    #[tokio::test]
    async fn test_graceful_shutdown_drains_in_flight_transcription() {
        let context = Arc::new(RwLock::new(DelayedMockContext::new(250)));
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();

        let server_context = context.clone();
        let mut server_handle = tokio::spawn(async move {
            let addr = ([127, 0, 0, 1], 0u16);
            let routes = create_routes(
                server_context,
                test_transcription_guard(),
                test_client_activity(),
            );
            let (addr, server) = warp::serve(routes).bind_with_graceful_shutdown(addr, async {
                shutdown_rx.await.ok();
            });

            let _ = addr_tx.send(addr);
            server.await;
        });

        let addr = addr_rx.await.expect("server failed to start");
        let client = reqwest::Client::new();
        let transcribe_url = format!("http://{}/api/v1/transcribe", addr);
        let request = tokio::spawn(async move {
            client
                .post(&transcribe_url)
                .header("Content-Type", "audio/wav")
                .body(b"audio".to_vec())
                .timeout(Duration::from_secs(5))
                .send()
                .await
        });

        tokio::time::timeout(Duration::from_millis(150), async {
            loop {
                if context.read().await.request_counter.load(Ordering::SeqCst) > 0 {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("transcription should start before shutdown");

        let _ = shutdown_tx.send(());
        assert!(
            tokio::time::timeout(Duration::from_millis(50), &mut server_handle)
                .await
                .is_err(),
            "server future resolved before in-flight transcription drained"
        );

        let response = request
            .await
            .expect("request task panicked")
            .expect("request failed");
        assert_eq!(response.status(), 200);
        drop(response);
        tokio::time::timeout(Duration::from_secs(5), server_handle)
            .await
            .expect("server should finish after in-flight request drains")
            .expect("server task panicked");
    }

    // ============================================================================
    // Rapid Sequential Requests Tests (Issue #2)
    // ============================================================================

    /// Test that multiple rapid requests all complete successfully
    /// Verifies request queuing behavior under load
    #[tokio::test]
    async fn test_rapid_sequential_requests_all_complete() {
        // Create context with small delay to simulate work
        let context = Arc::new(RwLock::new(DelayedMockContext::new(10)));

        // Start server
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();

        let server_context = context.clone();
        let server_handle = tokio::spawn(async move {
            let addr = ([127, 0, 0, 1], 0u16);
            let routes = create_routes(
                server_context,
                test_transcription_guard(),
                test_client_activity(),
            );

            let (addr, server) = warp::serve(routes).bind_with_graceful_shutdown(addr, async {
                shutdown_rx.await.ok();
            });

            let _ = addr_tx.send(addr);
            server.await;
        });

        let addr = addr_rx.await.expect("Server failed to start");
        sleep(Duration::from_millis(50)).await;

        let client = reqwest::Client::new();
        let transcribe_url = format!("http://{}/api/v1/transcribe", addr);
        let num_requests = 5;

        // Send rapid sequential requests
        let mut handles = Vec::new();
        for i in 0..num_requests {
            let client = client.clone();
            let url = transcribe_url.clone();
            let audio_data = format!("audio-data-{}", i).into_bytes();

            handles.push(tokio::spawn(async move {
                client
                    .post(&url)
                    .header("Content-Type", "audio/wav")
                    .body(audio_data)
                    .timeout(Duration::from_secs(10))
                    .send()
                    .await
            }));
        }

        // Wait for all requests to complete
        let mut success_count = 0;
        for handle in handles {
            let result = handle.await.expect("Task panicked");
            match result {
                Ok(response) if response.status().is_success() => {
                    success_count += 1;
                }
                Ok(response) => {
                    panic!("Request failed with status: {}", response.status());
                }
                Err(e) => {
                    panic!("Request error: {}", e);
                }
            }
        }

        assert_eq!(
            success_count, num_requests,
            "All {} requests should complete successfully",
            num_requests
        );

        // Verify request counter
        let ctx = context.read().await;
        assert_eq!(
            ctx.request_counter.load(Ordering::SeqCst),
            num_requests as u32,
            "Request counter should match number of requests"
        );
        drop(ctx);

        // Shutdown
        let _ = shutdown_tx.send(());
        let _ = tokio::time::timeout(Duration::from_secs(5), server_handle).await;
    }

    /// Test that responses contain correct data for each request
    /// Verifies no data corruption or mixing between requests
    #[tokio::test]
    async fn test_rapid_requests_return_correct_responses() {
        let context = Arc::new(RwLock::new(DelayedMockContext::new(5)));

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();

        let server_context = context.clone();
        let server_handle = tokio::spawn(async move {
            let addr = ([127, 0, 0, 1], 0u16);
            let routes = create_routes(
                server_context,
                test_transcription_guard(),
                test_client_activity(),
            );

            let (addr, server) = warp::serve(routes).bind_with_graceful_shutdown(addr, async {
                shutdown_rx.await.ok();
            });

            let _ = addr_tx.send(addr);
            server.await;
        });

        let addr = addr_rx.await.expect("Server failed to start");
        sleep(Duration::from_millis(50)).await;

        let client = reqwest::Client::new();
        let transcribe_url = format!("http://{}/api/v1/transcribe", addr);

        // Send requests with different audio data sizes
        let audio_sizes = [100, 200, 300, 400, 500];
        let mut handles = Vec::new();

        for size in audio_sizes {
            let client = client.clone();
            let url = transcribe_url.clone();
            let audio_data = vec![0u8; size];

            handles.push(tokio::spawn(async move {
                let response = client
                    .post(&url)
                    .header("Content-Type", "audio/wav")
                    .body(audio_data.clone())
                    .timeout(Duration::from_secs(10))
                    .send()
                    .await
                    .expect("Request failed");

                let json: serde_json::Value = response.json().await.expect("Failed to parse JSON");
                (size, json)
            }));
        }

        // Collect and verify responses
        let mut responses: Vec<(usize, serde_json::Value)> = Vec::new();
        for handle in handles {
            let (size, json) = handle.await.expect("Task panicked");
            responses.push((size, json));
        }

        // Verify each response contains expected data
        for (size, json) in &responses {
            let text = json["text"].as_str().expect("Missing text field");
            assert!(
                text.contains(&format!("len-{}", size)),
                "Response should contain audio size: expected len-{}, got {}",
                size,
                text
            );
        }

        // Verify we got unique request numbers (no duplicates)
        let request_nums: Vec<&str> = responses
            .iter()
            .filter_map(|(_, json)| json["text"].as_str())
            .filter_map(|text| text.split('-').nth(1))
            .collect();
        let unique_nums: std::collections::HashSet<_> = request_nums.iter().collect();
        assert_eq!(
            request_nums.len(),
            unique_nums.len(),
            "All request numbers should be unique (no duplicates)"
        );

        let _ = shutdown_tx.send(());
        let _ = tokio::time::timeout(Duration::from_secs(5), server_handle).await;
    }

    /// Test that an error in one request doesn't affect subsequent requests
    /// Verifies error isolation and server resilience
    #[tokio::test]
    async fn test_error_in_one_request_doesnt_affect_others() {
        // Fail on request 2 and 4
        let context = Arc::new(RwLock::new(FailingMockContext::new(vec![2, 4])));

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();

        let server_context = context.clone();
        let server_handle = tokio::spawn(async move {
            let addr = ([127, 0, 0, 1], 0u16);
            let routes = create_routes(
                server_context,
                test_transcription_guard(),
                test_client_activity(),
            );

            let (addr, server) = warp::serve(routes).bind_with_graceful_shutdown(addr, async {
                shutdown_rx.await.ok();
            });

            let _ = addr_tx.send(addr);
            server.await;
        });

        let addr = addr_rx.await.expect("Server failed to start");
        sleep(Duration::from_millis(50)).await;

        let client = reqwest::Client::new();
        let transcribe_url = format!("http://{}/api/v1/transcribe", addr);

        // Send 5 sequential requests (2 and 4 will fail)
        let mut results = Vec::new();
        for i in 1..=5 {
            let response = client
                .post(&transcribe_url)
                .header("Content-Type", "audio/wav")
                .body(format!("audio-{}", i))
                .timeout(Duration::from_secs(5))
                .send()
                .await
                .expect("Request failed to send");

            results.push((i, response.status().is_success()));
        }

        // Verify expected results
        assert!(results[0].1, "Request 1 should succeed");
        assert!(!results[1].1, "Request 2 should fail");
        assert!(results[2].1, "Request 3 should succeed (after failure)");
        assert!(!results[3].1, "Request 4 should fail");
        assert!(results[4].1, "Request 5 should succeed (after failure)");

        let _ = shutdown_tx.send(());
        let _ = tokio::time::timeout(Duration::from_secs(5), server_handle).await;
    }

    /// Test high load with many concurrent requests
    /// Verifies server stability and no data corruption under stress
    #[tokio::test]
    async fn test_high_load_no_data_corruption() {
        let context = Arc::new(RwLock::new(DelayedMockContext::new(2)));

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();

        let server_context = context.clone();
        let server_handle = tokio::spawn(async move {
            let addr = ([127, 0, 0, 1], 0u16);
            let routes = create_routes(
                server_context,
                test_transcription_guard(),
                test_client_activity(),
            );

            let (addr, server) = warp::serve(routes).bind_with_graceful_shutdown(addr, async {
                shutdown_rx.await.ok();
            });

            let _ = addr_tx.send(addr);
            server.await;
        });

        let addr = addr_rx.await.expect("Server failed to start");
        sleep(Duration::from_millis(50)).await;

        let client = reqwest::Client::new();
        let transcribe_url = format!("http://{}/api/v1/transcribe", addr);
        let num_requests = 20;

        // Send many concurrent requests
        let mut handles = Vec::new();
        for i in 0..num_requests {
            let client = client.clone();
            let url = transcribe_url.clone();
            // Each request has unique size for identification
            let audio_data = vec![i as u8; (i + 1) * 10];

            handles.push(tokio::spawn(async move {
                client
                    .post(&url)
                    .header("Content-Type", "audio/wav")
                    .body(audio_data)
                    .timeout(Duration::from_secs(30))
                    .send()
                    .await
            }));
        }

        // Wait for all to complete
        let mut success_count = 0;
        let mut response_texts = Vec::new();
        for handle in handles {
            match handle.await {
                Ok(Ok(response)) if response.status().is_success() => {
                    success_count += 1;
                    if let Ok(json) = response.json::<serde_json::Value>().await {
                        if let Some(text) = json["text"].as_str() {
                            response_texts.push(text.to_string());
                        }
                    }
                }
                Ok(Ok(response)) => {
                    panic!("Request failed with status: {}", response.status());
                }
                Ok(Err(e)) => {
                    panic!("Request error: {}", e);
                }
                Err(e) => {
                    panic!("Task panicked: {}", e);
                }
            }
        }

        assert_eq!(
            success_count, num_requests,
            "All {} requests should succeed under high load",
            num_requests
        );

        // Verify no duplicate request numbers (would indicate corruption)
        let request_nums: Vec<&str> = response_texts
            .iter()
            .filter_map(|text| text.split('-').nth(1))
            .collect();
        let unique_nums: std::collections::HashSet<_> = request_nums.iter().collect();
        assert_eq!(
            request_nums.len(),
            unique_nums.len(),
            "No duplicate request numbers should exist"
        );

        // Verify request counter matches
        let ctx = context.read().await;
        assert_eq!(
            ctx.request_counter.load(Ordering::SeqCst),
            num_requests as u32,
            "Total processed requests should match sent requests"
        );

        let _ = shutdown_tx.send(());
        let _ = tokio::time::timeout(Duration::from_secs(5), server_handle).await;
    }

    /// Test that requests are queued, not rejected, when server is busy
    /// Verifies that no requests are dropped due to concurrent load
    #[tokio::test]
    async fn test_requests_queued_not_rejected() {
        // Use longer delay to ensure requests overlap
        let context = Arc::new(RwLock::new(DelayedMockContext::new(50)));

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();

        let server_context = context.clone();
        let server_handle = tokio::spawn(async move {
            let addr = ([127, 0, 0, 1], 0u16);
            let routes = create_routes(
                server_context,
                test_transcription_guard(),
                test_client_activity(),
            );

            let (addr, server) = warp::serve(routes).bind_with_graceful_shutdown(addr, async {
                shutdown_rx.await.ok();
            });

            let _ = addr_tx.send(addr);
            server.await;
        });

        let addr = addr_rx.await.expect("Server failed to start");
        sleep(Duration::from_millis(50)).await;

        let client = reqwest::Client::new();
        let transcribe_url = format!("http://{}/api/v1/transcribe", addr);
        let num_requests = 3;

        // Send all requests nearly simultaneously
        let start_time = std::time::Instant::now();
        let mut handles = Vec::new();
        for i in 0..num_requests {
            let client = client.clone();
            let url = transcribe_url.clone();
            let audio_data = format!("audio-{}", i).into_bytes();

            handles.push(tokio::spawn(async move {
                let req_start = std::time::Instant::now();
                let result = client
                    .post(&url)
                    .header("Content-Type", "audio/wav")
                    .body(audio_data)
                    .timeout(Duration::from_secs(30))
                    .send()
                    .await;
                let req_duration = req_start.elapsed();
                (result, req_duration)
            }));
        }

        // Collect results
        let mut durations = Vec::new();
        let mut all_success = true;
        for handle in handles {
            let (result, duration) = handle.await.expect("Task panicked");
            durations.push(duration);
            match result {
                Ok(response) if response.status().is_success() => {}
                Ok(response) => {
                    eprintln!("Request failed with status: {}", response.status());
                    all_success = false;
                }
                Err(e) => {
                    eprintln!("Request error: {}", e);
                    all_success = false;
                }
            }
        }

        let total_time = start_time.elapsed();

        assert!(all_success, "All requests should succeed (none rejected)");

        // With 50ms delay per request and 3 concurrent requests,
        // total time should be >= 150ms if properly queued
        assert!(
            total_time >= Duration::from_millis(100),
            "Requests should be queued (total time: {:?})",
            total_time
        );

        // Verify all requests were processed
        let ctx = context.read().await;
        assert_eq!(
            ctx.request_counter.load(Ordering::SeqCst),
            num_requests as u32,
            "All requests should be processed (none dropped)"
        );

        let _ = shutdown_tx.send(());
        let _ = tokio::time::timeout(Duration::from_secs(5), server_handle).await;
    }

    struct PasswordedControlContext {
        password: String,
        model_name: String,
        allow_model_control: bool,
    }

    impl ServerContext for PasswordedControlContext {
        fn get_model_name(&self) -> String {
            self.model_name.clone()
        }
        fn get_server_name(&self) -> String {
            "passworded-server".to_string()
        }
        fn get_password(&self) -> Option<String> {
            Some(self.password.clone())
        }
        fn allow_model_control(&self) -> bool {
            self.allow_model_control
        }
        fn transcribe(
            &self,
            _audio_data: &[u8],
            _spoken_language: Option<&str>,
            _transcription_task: Option<&str>,
        ) -> Result<TranscriptionResult, String> {
            let job = crate::transcription::TranscriptionJob::from_legacy_settings(
                crate::transcription::TranscriptionSource::RemoteServer,
                "remote",
                self.model_name.clone(),
                None,
                false,
            );
            Ok(crate::transcription::TranscriptionResult::new(&job, "ok"))
        }
        fn get_model_control_snapshot(&self) -> Result<RemoteModelControlSnapshot, String> {
            Ok(mock_model_control_snapshot(&self.model_name))
        }
        fn update_shared_model(
            &self,
            model_id: &str,
            engine: &str,
        ) -> Result<RemoteModelControlSnapshot, String> {
            if engine != "whisper" && engine != "parakeet" {
                return Err(format!("Unsupported sharing engine '{engine}'"));
            }
            if model_id == "missing" {
                return Err(format!("Model '{model_id}' not found or not downloaded"));
            }
            Ok(mock_model_control_snapshot(model_id))
        }
    }

    #[tokio::test]
    async fn test_control_models_requires_password_when_unconfigured() {
        let ctx = Arc::new(RwLock::new(MockContext));
        let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

        let response = warp::test::request()
            .method("GET")
            .path("/api/v1/control/models")
            .reply(&routes)
            .await;

        assert_eq!(response.status(), 403);
        let body: ErrorResponse = serde_json::from_slice(response.body()).unwrap();
        assert_eq!(body.error, "control_requires_password");
    }

    #[tokio::test]
    async fn test_control_models_rejects_missing_password() {
        let ctx = Arc::new(RwLock::new(PasswordedControlContext {
            password: "secret".to_string(),
            model_name: "base.en".to_string(),
            allow_model_control: true,
        }));
        let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

        let response = warp::test::request()
            .method("GET")
            .path("/api/v1/control/models")
            .reply(&routes)
            .await;

        assert_eq!(response.status(), 401);
    }

    #[tokio::test]
    async fn test_control_models_patch_rejects_missing_password_before_body() {
        let ctx = Arc::new(RwLock::new(PasswordedControlContext {
            password: "secret".to_string(),
            model_name: "base.en".to_string(),
            allow_model_control: true,
        }));
        let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

        let response = warp::test::request()
            .method("PATCH")
            .path("/api/v1/control/models")
            .header("content-type", "application/json")
            .body(vec![b'a'; MAX_CONTROL_BODY_BYTES as usize + 1])
            .reply(&routes)
            .await;

        assert_eq!(response.status(), 401);
        let body: ErrorResponse = serde_json::from_slice(response.body()).unwrap();
        assert_eq!(body.error, "unauthorized");
    }

    #[tokio::test]
    async fn test_control_models_get_returns_snapshot_with_auth() {
        let ctx = Arc::new(RwLock::new(PasswordedControlContext {
            password: "secret".to_string(),
            model_name: "base.en".to_string(),
            allow_model_control: true,
        }));
        let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

        let response = warp::test::request()
            .method("GET")
            .path("/api/v1/control/models")
            .header(AUTH_HEADER, "secret")
            .reply(&routes)
            .await;

        assert_eq!(response.status(), 200);
        let body: RemoteModelControlSnapshot = serde_json::from_slice(response.body()).unwrap();
        assert_eq!(body.current.id, "base.en");
        assert!(!body.available.is_empty());
    }

    #[tokio::test]
    async fn test_control_models_patch_validates_unknown_model() {
        let ctx = Arc::new(RwLock::new(PasswordedControlContext {
            password: "secret".to_string(),
            model_name: "base.en".to_string(),
            allow_model_control: true,
        }));
        let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

        let response = warp::test::request()
            .method("PATCH")
            .path("/api/v1/control/models")
            .header(AUTH_HEADER, "secret")
            .header("content-type", "application/json")
            .body(r#"{"model":"missing","engine":"whisper"}"#)
            .reply(&routes)
            .await;

        assert_eq!(response.status(), 400);
    }

    #[tokio::test]
    async fn test_control_models_patch_updates_model_with_auth() {
        let ctx = Arc::new(RwLock::new(PasswordedControlContext {
            password: "secret".to_string(),
            model_name: "base.en".to_string(),
            allow_model_control: true,
        }));
        let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

        let response = warp::test::request()
            .method("PATCH")
            .path("/api/v1/control/models")
            .header(AUTH_HEADER, "secret")
            .header("content-type", "application/json")
            .body(r#"{"model":"large-v3-turbo","engine":"whisper"}"#)
            .reply(&routes)
            .await;

        assert_eq!(response.status(), 200);
        let body: RemoteModelControlSnapshot = serde_json::from_slice(response.body()).unwrap();
        assert_eq!(body.current.id, "large-v3-turbo");
    }
    #[tokio::test]
    async fn test_control_models_rejects_when_host_opt_in_disabled() {
        let ctx = Arc::new(RwLock::new(PasswordedControlContext {
            password: "secret".to_string(),
            model_name: "base.en".to_string(),
            allow_model_control: false,
        }));
        let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

        let response = warp::test::request()
            .method("GET")
            .path("/api/v1/control/models")
            .header(AUTH_HEADER, "secret")
            .reply(&routes)
            .await;

        assert_eq!(response.status(), 403);
        let body: ErrorResponse = serde_json::from_slice(response.body()).unwrap();
        assert_eq!(body.error, "model_control_disabled");
    }

    #[tokio::test]
    async fn test_control_models_patch_rejects_when_host_opt_in_disabled() {
        let ctx = Arc::new(RwLock::new(PasswordedControlContext {
            password: "secret".to_string(),
            model_name: "base.en".to_string(),
            allow_model_control: false,
        }));
        let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

        let response = warp::test::request()
            .method("PATCH")
            .path("/api/v1/control/models")
            .header(AUTH_HEADER, "secret")
            .header("content-type", "application/json")
            .body(r#"{"model":"large-v3-turbo","engine":"whisper"}"#)
            .reply(&routes)
            .await;

        assert_eq!(response.status(), 403);
        let body: ErrorResponse = serde_json::from_slice(response.body()).unwrap();
        assert_eq!(body.error, "model_control_disabled");
    }

    // ------------------------------------------------------------------------
    // Auth regression: empty configured password / empty provided key (#1)
    // ------------------------------------------------------------------------

    #[test]
    fn auth_matches_rejects_empty_strings() {
        // `"" == ""` must never authorize; an empty key/value is never a credential.
        assert!(!auth_matches("", ""));
        assert!(!auth_matches("", "secret"));
        assert!(!auth_matches("secret", ""));
        assert!(auth_matches("secret", "secret"));
    }

    #[tokio::test]
    async fn transcribe_rejects_empty_configured_password_with_empty_key() {
        // A configured-but-empty password (`Some("")`) plus an empty X-Voicetypr-Key
        // must NOT authorize via `"" == ""`. Fail closed with 401.
        let ctx = Arc::new(RwLock::new(PasswordedControlContext {
            password: String::new(),
            model_name: "base.en".to_string(),
            allow_model_control: true,
        }));
        let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

        let response = warp::test::request()
            .method("POST")
            .path("/api/v1/transcribe")
            .header("content-type", "audio/wav")
            .header(AUTH_HEADER, "")
            .body(vec![0u8; 32])
            .reply(&routes)
            .await;

        assert_eq!(response.status(), 401);
        let body: ErrorResponse = serde_json::from_slice(response.body()).unwrap();
        assert_eq!(body.error, "unauthorized");
    }

    #[tokio::test]
    async fn transcribe_rejects_empty_configured_password_with_no_key() {
        // No key against an empty configured password: still 401 (never treated as open).
        let ctx = Arc::new(RwLock::new(PasswordedControlContext {
            password: String::new(),
            model_name: "base.en".to_string(),
            allow_model_control: true,
        }));
        let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

        let response = warp::test::request()
            .method("POST")
            .path("/api/v1/transcribe")
            .header("content-type", "audio/wav")
            .body(vec![0u8; 32])
            .reply(&routes)
            .await;

        assert_eq!(response.status(), 401);
    }

    #[tokio::test]
    async fn transcribe_rejects_empty_provided_key_for_nonempty_password() {
        // An empty X-Voicetypr-Key must never authorize a non-empty password.
        let ctx = Arc::new(RwLock::new(PasswordedControlContext {
            password: "secret".to_string(),
            model_name: "base.en".to_string(),
            allow_model_control: true,
        }));
        let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

        let response = warp::test::request()
            .method("POST")
            .path("/api/v1/transcribe")
            .header("content-type", "audio/wav")
            .header(AUTH_HEADER, "")
            .body(vec![0u8; 32])
            .reply(&routes)
            .await;

        assert_eq!(response.status(), 401);
    }

    // ------------------------------------------------------------------------
    // Content-type preflight: reject non-audio BEFORE permit/body (#2)
    // ------------------------------------------------------------------------

    #[tokio::test]
    async fn audio_with_content_type_params_is_accepted() {
        // "audio/wav; charset=binary" must pass the starts_with("audio/") preflight.
        let ctx = Arc::new(RwLock::new(MockContext));
        let routes = create_routes(ctx, test_transcription_guard(), test_client_activity());

        let response = warp::test::request()
            .method("POST")
            .path("/api/v1/transcribe")
            .header("content-type", "audio/wav; charset=binary")
            .body(b"audio")
            .reply(&routes)
            .await;

        assert_eq!(response.status(), 200);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn non_audio_request_rejected_before_permit() {
        // While request A (audio) holds the single transcription permit during a slow
        // transcription, a non-audio request must return 415 immediately from the
        // content-type preflight — without waiting for the permit or buffering its
        // body. Before the fix this hung on the permit behind A's ~400ms run.
        let ctx = Arc::new(RwLock::new(DelayedMockContext::new(400)));
        let guard = Arc::new(Semaphore::new(1));
        let routes = create_routes(ctx, guard, test_client_activity());

        // Request A acquires the permit and blocks in transcription.
        let routes_a = routes.clone();
        let req_a = tokio::spawn(async move {
            warp::test::request()
                .method("POST")
                .path("/api/v1/transcribe")
                .header("content-type", "audio/wav")
                .body(vec![0u8; 100])
                .reply(&routes_a)
                .await
        });

        // Give A time to acquire the permit and enter its blocking transcription.
        sleep(Duration::from_millis(80)).await;

        // Request B is non-audio: it must NOT wait for A's permit.
        let result = tokio::time::timeout(
            Duration::from_millis(150),
            warp::test::request()
                .method("POST")
                .path("/api/v1/transcribe")
                .header("content-type", "application/json")
                .body("{}")
                .reply(&routes),
        )
        .await;

        let response =
            result.expect("non-audio request must not wait for the transcription permit");
        assert_eq!(response.status(), 415);
        let body: ErrorResponse = serde_json::from_slice(response.body()).unwrap();
        assert_eq!(body.error, "unsupported_media_type");

        // Let A finish so the runtime isn't left with a blocked task.
        let _ = tokio::time::timeout(Duration::from_secs(5), req_a).await;
    }
    // ------------------------------------------------------------------------
    // Permit ordering: a slow upload must NOT hold the transcription permit (#1)
    // ------------------------------------------------------------------------

    /// Regression for the CRITICAL permit-before-body bug. The transcription
    /// permit used to be acquired in the filter chain BEFORE `warp::body::bytes()`
    /// read the body, so a client that sent the audio headers then stalled or
    /// trickled the body held the single permit for the whole (unbounded) upload
    /// — the 1h timeout only starts inside the handler, AFTER the body is
    /// buffered — queuing every other remote transcription behind it with no 408.
    ///
    /// Reproduction over a real server: request A opens a raw TCP socket, sends
    /// the headers + an explicit `Content-Length` + only PART of the body, then
    /// stalls. Request B (a complete body sent ~150 ms later) must succeed
    /// promptly because A holds NO permit while it is stuck in `bytes()`. With
    /// the OLD ordering A grabs the permit before reading its body, so B blocks
    /// on the permit until A's upload finishes (~1.5 s) and cannot complete
    /// within the 700 ms window; with the fixed ordering A is stuck in `bytes()`
    /// without the permit, B takes the free permit and succeeds right away.
    #[tokio::test(flavor = "multi_thread")]
    async fn slow_upload_does_not_block_second_transcription() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpStream;

        let ctx = Arc::new(RwLock::new(MockContext));
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();

        let server_context = ctx.clone();
        let server_handle = tokio::spawn(async move {
            let addr = ([127, 0, 0, 1], 0u16);
            let routes = create_routes(
                server_context,
                test_transcription_guard(),
                test_client_activity(),
            );
            let (addr, server) = warp::serve(routes).bind_with_graceful_shutdown(addr, async {
                shutdown_rx.await.ok();
            });
            let _ = addr_tx.send(addr);
            server.await;
        });

        let addr = addr_rx.await.expect("Server failed to start");
        sleep(Duration::from_millis(50)).await;

        let total_bytes: usize = 100;
        // Request A: send headers + a quarter of the body, then stall. The
        // explicit Content-Length lets warp's content_length_limit pass and makes
        // `bytes()` block reading until all `total_bytes` arrive.
        let mut stream_a = TcpStream::connect(addr).await.expect("connect A");
        let head = format!(
            "POST /api/v1/transcribe HTTP/1.1\r\nHost: localhost\r\n\
             Content-Type: audio/wav\r\nContent-Length: {total_bytes}\r\n\
             Connection: close\r\n\r\n"
        );
        stream_a.write_all(head.as_bytes()).await.unwrap();
        stream_a.flush().await.unwrap();
        stream_a
            .write_all(&vec![0u8; total_bytes / 4])
            .await
            .unwrap();
        stream_a.flush().await.unwrap();

        // Let A's headers be parsed so its filter chain is running (and, under
        // the old ordering, has already acquired the permit).
        sleep(Duration::from_millis(150)).await;

        // Request B: a complete, instantly-sent body on a fresh connection.
        // Under the fixed ordering A holds NO permit while stalled in `bytes()`,
        // so B takes the free permit and succeeds promptly. Under the old
        // ordering A already holds the permit, so B blocks on it for the rest of
        // A's ~1.5 s upload and cannot finish within the 700 ms window.
        let client = reqwest::Client::new();
        let transcribe_url = format!("http://{}/api/v1/transcribe", addr);
        let b_start = std::time::Instant::now();
        let response_b = tokio::time::timeout(
            Duration::from_millis(700),
            client
                .post(&transcribe_url)
                .header("Content-Type", "audio/wav")
                .body(vec![0u8; 32])
                .timeout(Duration::from_secs(5))
                .send(),
        )
        .await
        .expect(
            "second transcription stalled behind a slow upload — the permit \
             was acquired before the body was buffered",
        )
        .expect("request B send failed");
        let b_elapsed = b_start.elapsed();
        assert!(
            response_b.status().is_success(),
            "second transcription failed: {}",
            response_b.status()
        );
        assert!(
            b_elapsed < Duration::from_millis(700),
            "second transcription took {b_elapsed:?}; it waited behind the \
             slow upload's permit"
        );

        // Finish A's upload so the server can transcribe and respond.
        stream_a
            .write_all(&vec![0u8; total_bytes - total_bytes / 4])
            .await
            .unwrap();
        stream_a.flush().await.unwrap();

        // Read A's response. The status line arrives first, so read until we
        // have it (Connection: close also yields EOF after the body).
        let mut resp_a = Vec::with_capacity(1024);
        let mut buf = [0u8; 1024];
        let a_ok = loop {
            let n = tokio::time::timeout(Duration::from_secs(5), stream_a.read(&mut buf))
                .await
                .expect("A response read timed out")
                .expect("reading A response failed");
            if n == 0 {
                break false;
            }
            resp_a.extend_from_slice(&buf[..n]);
            if resp_a.starts_with(b"HTTP/1.1 200") {
                break true;
            }
            if resp_a.len() >= 64 {
                break false;
            }
        };
        assert!(
            a_ok,
            "A did not succeed: {}",
            String::from_utf8_lossy(&resp_a)
        );

        let _ = shutdown_tx.send(());
        let _ = tokio::time::timeout(Duration::from_secs(5), server_handle).await;
    }
}
