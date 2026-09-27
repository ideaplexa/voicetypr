//! Opt-out, anonymous error reporting (Sentry SDK -> self-hosted GlitchTip
//! 6.2).
//!
//! Privacy posture (non-negotiable):
//! - On by default (opt-out). Active unless the user has explicitly opted out,
//!   AND only in release builds: the DSN is compiled in for release only, so
//!   dev/debug builds have no DSN and the client is never created (fully inert).
//! - No native minidumps: we use the `sentry` crate directly — no
//!   `tauri-plugin-sentry`, no browser-SDK injection, and no envelope/breadcrumb
//!   IPC — so the only capture paths are the Rust SDK and three independent
//!   egress gates, each checking consent:
//!   1. **Events**: `before_send` → [`scrub_event`] — rebuilds from an
//!      allowlist, scrubs secrets, preserves sanitized debug metadata.
//!   2. **Capture-time consent**: every `capture_*` helper checks
//!      [`is_enabled`] before building an event.
//!   3. **Transport**: [`ConsentTransport`] rechecks consent when each envelope
//!      is handed to HTTP.
//! - No breadcrumbs, no PII, no `release-health`/session tracking (the feature
//!   is not compiled in, so sessions are impossible), no `contexts`
//!   integration, no transactions/traces.
//! - [`scrub_event`] REBUILDS every event from a tiny allowlist (allowlist by
//!   construction) and scrubs structured secret runs (file paths, URLs, IPs,
//!   emails, keys, target app/window names), so those never leave the device.
//!   Raw audio is never captured. A frontend-reported error keeps its type and
//!   (length-capped) message for debuggability: the regex scrub strips
//!   structured secrets from the message but not arbitrary prose, so an opted-in
//!   error report can contain free-form frontend error text.
//! - Failure events (plan 044): terminal user-flow failures are captured as
//!   events with a fixed message per failure class (GlitchTip groups issues by
//!   message, and issue alerts are how failures page us) plus closed-
//!   vocabulary tags (`FAILURE_EVENT_TAG_KEYS`) for slicing. The curated-log
//!   funnel and 1% sampled transactions were removed: logs never created
//!   issues, so they never alerted.
//! - Native symbolication: `debug-images` is enabled so `DebugImagesIntegration`
//!   attaches debug metadata. [`scrub_debug_meta`] reduces every image/debug
//!   filename to a basename (dropping directory components) while preserving the
//!   debug IDs, code IDs, image sizes, and addresses needed for server-side
//!   symbolication. [`scrub_frame`] retains `instruction_addr`, `image_addr`,
//!   and `symbol_addr` alongside function/line/column/in-app.

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use regex::Regex;
use sentry::protocol::{DebugImage, DebugMeta, Event, Exception, Frame, Level, Stacktrace, Values};
use sentry::ClientInitGuard;

/// GlitchTip DSN. Compiled into RELEASE builds only; dev/debug builds have no
/// DSN and are fully inert (no client is ever created). A DSN is a client
/// ingestion key — it can only send events, never read — so embedding it is
/// expected/safe. The server is the self-hosted GlitchTip 6.2 instance at
/// `glitchtip.ideaplexa.com`, org `ideaplexa`, project `voicetypr-desktop`.
#[cfg(debug_assertions)]
const SENTRY_DSN: Option<&str> = None;
#[cfg(not(debug_assertions))]
const SENTRY_DSN: Option<&str> =
    Some("https://dc30154073564c529440b97bf18f1fdc@glitchtip.ideaplexa.com/1");

/// Environment tag attached to every event/transaction. Always `production` for
/// release builds (debug builds never send — no DSN).
const ENVIRONMENT: &str = "production";

use crate::release_channel::RELEASE_CHANNEL;
/// Store file (tauri-plugin-store) + keys that hold consent state. The store is
/// a flat top-level JSON object, so a raw reader can parse these keys before the
/// Tauri app (and its plugins) are built.
const SETTINGS_STORE_FILE: &str = "settings";
pub const KEY_TELEMETRY_ENABLED: &str = "telemetry_enabled";
pub const KEY_TELEMETRY_INSTALL_ID: &str = "telemetry_install_id";
/// Default consent when no explicit choice is stored (fresh installs, upgraders
/// who completed onboarding before diagnostics existed). Opt-out: reporting is
/// on unless the user explicitly disabled it. An explicit `telemetry_enabled:
/// false` always wins.
pub const TELEMETRY_DEFAULT_ENABLED: bool = true;

/// In-process consent gate, read on every `before_send` and before every manual
/// capture. Revoking consent stops egress immediately within the session; a full
/// re-enable still needs a restart because the client is only wired at startup.
/// The `false` initializer only covers the tiny pre-init window; `init()`
/// overwrites it from stored consent at startup.
static TELEMETRY_ENABLED: AtomicBool = AtomicBool::new(false);

/// True when this build is capable of reporting at all (a DSN was compiled in).
pub fn is_available() -> bool {
    SENTRY_DSN.is_some()
}

/// Whether reporting is currently allowed this session.
pub fn is_enabled() -> bool {
    TELEMETRY_ENABLED.load(Ordering::SeqCst)
}

/// Flip the in-process gate. Disabling takes effect immediately.
pub fn set_enabled(enabled: bool) {
    TELEMETRY_ENABLED.store(enabled, Ordering::SeqCst);
}

/// Closes the in-process gate and discards envelopes still queued by the
/// current Sentry transport. Re-enabling diagnostics requires a restart.
pub fn disable_and_drop_queued() {
    set_enabled(false);
    if let Some(client) = sentry::Hub::current().client() {
        let _ = client.close(Some(Duration::ZERO));
    }
}

// --- Scrubbing ---------------------------------------------------------------

static RE_PATH: LazyLock<Regex> = LazyLock::new(|| {
    // Windows drive paths, UNC shares, drive-less user/system dirs, Unix home/
    // system roots, and JS `file://`/`app://`/`asset://` URIs.
    Regex::new(
        r#"(?i)([a-z]:\\[^\s"'`]*|\\\\[^\s"'`]+|\\(?:users|windows|programdata)\\[^\s"'`]*|/(?:users|home|var|private|tmp|library|applications)/[^\s"'`]*|(?:file|app|asset)://[^\s"'`]*)"#,
    )
    .expect("valid path regex")
});
static RE_URL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)https?://[^\s"'`]+"#).expect("valid url regex"));
static RE_IP: LazyLock<Regex> = LazyLock::new(|| {
    // Bare IPv4, with optional :port (covers LAN endpoints like 192.168.1.20:8080).
    Regex::new(r#"\b(?:\d{1,3}\.){3}\d{1,3}(?::\d+)?\b"#).expect("valid ip regex")
});
static RE_EMAIL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}"#).expect("valid email regex")
});
static RE_LONG_TOKEN: LazyLock<Regex> = LazyLock::new(|| {
    // Long opaque runs: API keys, bearer tokens, hashes, foreign UUIDs.
    Regex::new(r#"\b[A-Za-z0-9_\-]{24,}\b"#).expect("valid token regex")
});

/// Redacts free-form text that may carry user content or environment detail.
pub fn scrub_text(input: &str) -> String {
    // Order matters: paths and URLs first so their inner hosts/IPs are consumed.
    let mut s = RE_PATH.replace_all(input, "[path]").into_owned();
    s = RE_URL.replace_all(&s, "[url]").into_owned();
    s = RE_IP.replace_all(&s, "[ip]").into_owned();
    s = RE_EMAIL.replace_all(&s, "[email]").into_owned();
    s = RE_LONG_TOKEN.replace_all(&s, "[redacted]").into_owned();
    s
}

/// Rebuilds an event from scratch — allowlist by construction. Only known-safe,
/// non-identifying fields are carried over; everything else (contexts, extra,
/// user, request, server_name, modules, fingerprint, culprit, transaction,
/// logger, sdk, breadcrumbs, threads, ...) is dropped because it is never copied
/// into the fresh event. The event `release`, `environment`, and scrubbed
/// `debug_meta` are preserved (needed for symbolication); `debug_meta` is
/// sanitized by [`scrub_debug_meta`] to reduce every image filename to a
/// basename while keeping IDs/sizes/addresses.
pub fn scrub_event(event: Event<'static>, install_id: Option<&str>) -> Event<'static> {
    let scrubbed_meta = scrub_debug_meta(event.debug_meta.into_owned());
    let mut clean = Event {
        event_id: event.event_id,
        level: event.level,
        timestamp: event.timestamp,
        platform: event.platform,
        // Our own release string ("voicetypr@<version>") — not identifying.
        release: event.release,
        // "production" for release builds; never identifying.
        environment: event.environment,
        message: event.message.map(|m| scrub_text(&m)),
        exception: Values {
            values: event
                .exception
                .values
                .into_iter()
                .map(scrub_exception)
                .collect(),
        },
        // Sanitized native debug metadata — basenames only, IDs/sizes/addresses
        // preserved for server-side symbolication.
        debug_meta: Cow::Owned(scrubbed_meta),
        ..Default::default()
    };

    // Re-attach a tiny, non-identifying allowlist as tags.
    clean.tags.insert("os".into(), std::env::consts::OS.into());
    clean
        .tags
        .insert("arch".into(), std::env::consts::ARCH.into());
    clean
        .tags
        .insert("app_version".into(), env!("CARGO_PKG_VERSION").into());
    clean
        .tags
        .insert("release_channel".into(), RELEASE_CHANNEL.into());
    if let Some(id) = install_id {
        clean.tags.insert("install_id".into(), id.into());
    }

    // Failure-event tags (fixed vocabulary emitted by the capture_* API):
    // copied through only when the key is allowlisted, with the value
    // re-scrubbed as defense in depth.
    for key in FAILURE_EVENT_TAG_KEYS {
        if let Some(value) = event.tags.get(*key) {
            clean.tags.insert((*key).to_string(), scrub_text(value));
        }
    }

    clean
}

/// Keeps the exception type + scrubbed message + sanitized stack; drops module,
/// mechanism, raw stacktrace, thread id (all potentially path/host-bearing).
fn scrub_exception(exception: Exception) -> Exception {
    Exception {
        ty: exception.ty,
        value: exception.value.map(|v| scrub_text(&v)),
        stacktrace: exception.stacktrace.map(scrub_stacktrace),
        ..Default::default()
    }
}

/// Keeps only the frame call shape; drops registers and frame-omitted markers.
fn scrub_stacktrace(stacktrace: Stacktrace) -> Stacktrace {
    Stacktrace {
        frames: stacktrace.frames.into_iter().map(scrub_frame).collect(),
        ..Default::default()
    }
}

/// Keeps function / line / column / in-app, AND the native addresses needed for
/// server-side symbolication (`instruction_addr`, `image_addr`, `symbol_addr`,
/// `addr_mode`). Drops filename, abs_path, module, package, symbol, registers,
/// context lines, and local variables — all potentially path/host-bearing.
fn scrub_frame(frame: Frame) -> Frame {
    Frame {
        function: frame.function,
        lineno: frame.lineno,
        colno: frame.colno,
        in_app: frame.in_app,
        // Native addresses required for server-side symbolication. These are
        // memory addresses, not user data.
        image_addr: frame.image_addr,
        instruction_addr: frame.instruction_addr,
        symbol_addr: frame.symbol_addr,
        addr_mode: frame.addr_mode,
        ..Default::default()
    }
}

// --- Native debug-metadata scrubbing -----------------------------------------

/// Reduces any path-bearing string to its file-name component (the last path
/// segment after either `/` or `\`). Splits on both separators independently of
/// the host OS so Windows paths are correctly reduced on any platform.
fn basename(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_string()
}

/// Sanitizes [`DebugMeta`]: keeps `sdk_info` and the list of images, but every
/// image filename/debug-file/code-file is reduced to a basename while the debug
/// IDs, code IDs, image sizes, and addresses (needed for server-side
/// symbolication) are preserved verbatim.
fn scrub_debug_meta(meta: DebugMeta) -> DebugMeta {
    DebugMeta {
        sdk_info: meta.sdk_info,
        images: meta.images.into_iter().map(scrub_debug_image).collect(),
    }
}

/// Reduces every path-bearing string on a [`DebugImage`] to its basename,
/// preserving all identifiers, sizes, and addresses for symbolication.
fn scrub_debug_image(image: DebugImage) -> DebugImage {
    match image {
        DebugImage::Apple(img) => DebugImage::Apple(sentry::protocol::AppleDebugImage {
            name: basename(&img.name),
            ..img
        }),
        DebugImage::Symbolic(img) => DebugImage::Symbolic(sentry::protocol::SymbolicDebugImage {
            name: basename(&img.name),
            debug_file: img.debug_file.map(|f| basename(&f)),
            ..img
        }),
        DebugImage::Wasm(img) => DebugImage::Wasm(sentry::protocol::WasmDebugImage {
            name: basename(&img.name),
            debug_file: img.debug_file.map(|f| basename(&f)),
            code_file: basename(&img.code_file),
            ..img
        }),
        // Proguard images carry only a UUID — no path-bearing fields.
        other => other,
    }
}

// --- Consent (early, opt-out default) ----------------------------------------

/// Reads telemetry consent + install id for the given app identifier. Opt-out
/// default: any missing / malformed / unreadable value yields
/// `(TELEMETRY_DEFAULT_ENABLED, None)`, so telemetry is on unless the user has
/// explicitly opted out. An explicit `telemetry_enabled: false` is always honored.
pub fn read_consent(identifier: &str) -> (bool, Option<String>) {
    match settings_store_path(identifier) {
        Some(path) => read_consent_from_path(&path),
        None => (TELEMETRY_DEFAULT_ENABLED, None),
    }
}

/// Mirrors tauri-plugin-store's default AppData base: `data_dir/<identifier>/<file>`.
fn settings_store_path(identifier: &str) -> Option<PathBuf> {
    dirs::data_dir().map(|dir| dir.join(identifier).join(SETTINGS_STORE_FILE))
}

/// Parses the flat top-level JSON store at `path` for the consent keys.
pub fn read_consent_from_path(path: &Path) -> (bool, Option<String>) {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => return (TELEMETRY_DEFAULT_ENABLED, None),
    };
    let value: serde_json::Value = match serde_json::from_slice(&bytes) {
        Ok(value) => value,
        Err(_) => return (TELEMETRY_DEFAULT_ENABLED, None),
    };
    let enabled = value
        .get(KEY_TELEMETRY_ENABLED)
        .and_then(|v| v.as_bool())
        .unwrap_or(TELEMETRY_DEFAULT_ENABLED);
    let install_id = value
        .get(KEY_TELEMETRY_INSTALL_ID)
        .and_then(|value| value.as_str())
        .and_then(|value| uuid::Uuid::parse_str(value).ok())
        .map(|value| value.to_string());
    (enabled, install_id)
}

// --- Init + capture ----------------------------------------------------------

/// Transport-level consent gate: the final egress chokepoint for every
/// envelope. Anything that reaches the SDK (panics, scrubbed frontend error
/// events, failure events) is dropped here once diagnostics are opted out.
struct ConsentTransport {
    inner: Arc<dyn sentry::Transport>,
}

impl sentry::Transport for ConsentTransport {
    fn send_envelope(&self, envelope: sentry::Envelope) {
        if is_enabled() {
            self.inner.send_envelope(envelope);
        }
    }

    fn flush(&self, timeout: Duration) -> bool {
        self.inner.flush(timeout)
    }

    fn shutdown(&self, timeout: Duration) -> bool {
        self.inner.shutdown(timeout)
    }
}

#[derive(Clone)]
struct ConsentTransportFactory;

impl sentry::TransportFactory for ConsentTransportFactory {
    fn create_transport(&self, options: &sentry::ClientOptions) -> Arc<dyn sentry::Transport> {
        let inner = sentry::TransportFactory::create_transport(
            &sentry::transports::DefaultTransportFactory,
            options,
        );
        Arc::new(ConsentTransport { inner })
    }
}

/// Initializes Sentry (→ GlitchTip) when reporting is enabled (on by default
/// unless the user opted out) and a DSN was compiled in. Returns the guard,
/// which the caller MUST keep alive for the program's lifetime; returns `None`
/// (no client created) otherwise. We do NOT register `tauri-plugin-sentry` (no
/// JS injection / no envelope IPC) — JS errors are captured explicitly via
/// [`capture_frontend_error`].
///
/// **Three egress gates**: (1) `before_send` → [`scrub_event`] for events,
/// (2) capture-time consent checks in every `capture_*` helper, and
/// (3) [`ConsentTransport`] at actual envelope send time — revoking consent
/// drops everything queued before the opt-out reaches the transport.
///
/// Configuration: `environment = "production"`; `release-health`/session
/// tracking is NOT compiled in (sessions are impossible).
pub fn init(enabled: bool, install_id: Option<String>) -> Option<ClientInitGuard> {
    set_enabled(enabled);

    let dsn = SENTRY_DSN?;
    if !enabled {
        return None;
    }

    let event_install_id = install_id;
    let guard = sentry::init((
        dsn,
        sentry::ClientOptions {
            release: sentry::release_name!(),
            environment: Some(Cow::Borrowed(ENVIRONMENT)),
            send_default_pii: false,
            max_breadcrumbs: 0,
            before_breadcrumb: Some(Arc::new(|_breadcrumb| None)),
            // Gate 1 — events: rebuild from allowlist + scrub secrets.
            before_send: Some(Arc::new(move |event| {
                if !is_enabled() {
                    return None;
                }
                Some(scrub_event(event, event_install_id.as_deref()))
            })),
            // Final egress gate, evaluated when an envelope is handed to the
            // HTTP transport.
            transport: Some(Arc::new(ConsentTransportFactory)),
            ..Default::default()
        },
    ));
    Some(guard)
}

/// Captures a frontend-reported error as a Sentry event. Gated on consent and
/// routed through `capture_event` so `before_send` scrubs it. No-op when
/// telemetry is disabled or no client was initialized.
///
/// Privacy: the frontend `message` is untrusted free-form text, so `before_send`
/// (`scrub_exception` -> `scrub_text`) redacts *structured* secret runs
/// (paths/URLs/IPs/emails/tokens) from the value before it leaves the process,
/// and the message is length-capped here to bound payload size. Telemetry is
/// on by default (opt-out).
pub fn capture_frontend_error(name: Option<&str>, message: &str) {
    if !is_enabled() {
        return;
    }
    let event = build_frontend_error_event(name, message);
    sentry::capture_event(event);
}

fn frontend_error_type(name: Option<&str>) -> &'static str {
    match name {
        Some("Error") => "Error",
        Some("TypeError") => "TypeError",
        Some("RangeError") => "RangeError",
        Some("ReferenceError") => "ReferenceError",
        Some("SyntaxError") => "SyntaxError",
        Some("URIError") => "URIError",
        Some("EvalError") => "EvalError",
        _ => "FrontendError",
    }
}

const FRONTEND_ERROR_MAX_LEN: usize = 2000;

/// Constructs the event for a frontend-reported error: the stable error
/// type/category plus the length-capped message. Pure (no Sentry client needed)
/// so it is unit-testable; structured-secret redaction runs in `before_send`.
fn build_frontend_error_event(name: Option<&str>, message: &str) -> Event<'static> {
    let value: String = message.chars().take(FRONTEND_ERROR_MAX_LEN).collect();
    Event {
        level: Level::Error,
        exception: Values {
            values: vec![Exception {
                ty: frontend_error_type(name).to_string(),
                value: Some(value),
                ..Default::default()
            }],
        },
        ..Default::default()
    }
}

// --- Failure events (closed API) ---------------------------------------------
//
// Replaces the structured-log funnel + sampled transactions (plan 044):
// GlitchTip alert rules fire on issues, and curated logs never created
// issues — user-flow failures were invisible until a customer emailed.
// Terminal failures are now EVENTS: one fixed message per failure class
// (drives grouping + alerting) plus closed-vocabulary tags for slicing.
// Every tag value comes from a fixed vocabulary produced by callers —
// never free-form user text — and `scrub_event` re-checks the tag
// allowlist as the egress gate.

/// Tag keys a failure event may carry through [`scrub_event`]. Fixed
/// vocabulary; anything else attached to an event is dropped at the gate.
const FAILURE_EVENT_TAG_KEYS: &[&str] = &[
    "engine",
    "model",
    "backend",
    "failure_class",
    "duration_ms",
    "stage",
];

/// Emits one failure event. Consent-gated; message is a fixed template with
/// the closed failure-class suffix so GlitchTip groups per class and issue
/// alerts fire on the first occurrence.
fn capture_failure_event(message: String, tags: Vec<(&'static str, String)>) {
    if !is_enabled() {
        return;
    }
    let mut event = Event {
        level: Level::Error,
        ..Default::default()
    };
    event.message = Some(message);
    for (key, value) in tags {
        event.tags.insert((*key).to_string(), value);
    }
    sentry::capture_event(event);
}

/// Terminal transcription failure for any engine (local/cloud/remote).
/// `engine`: whisper | parakeet | cloud provider id | remote.
/// `model`: model name, or the empty string when the engine has none.
/// `backend`: whisper acceleration actually in use (cpu | metal | sidecar),
/// or `None` for engines without backends.
/// `failure_class`: closed vocabulary from the audio sink's classifier.
pub fn capture_transcription_failure(
    engine: &str,
    model: &str,
    backend: Option<&str>,
    failure_class: &str,
    duration_ms: Option<u64>,
) {
    capture_failure_event(
        format!("flow.transcription.failed.{failure_class}"),
        vec![
            ("engine", engine.to_string()),
            ("model", model.to_string()),
            ("backend", backend.unwrap_or("none").to_string()),
            ("failure_class", failure_class.to_string()),
            (
                "duration_ms",
                duration_ms.map(|ms| ms.to_string()).unwrap_or_default(),
            ),
        ],
    );
}

/// Paste/clipboard delivery failure after a successful transcription.
/// `stage`: "insert" | "clipboard".
pub fn capture_paste_failure(stage: &str) {
    capture_failure_event(
        "flow.paste.failed".to_string(),
        vec![("stage", stage.to_string())],
    );
}

/// Local model load failure. `model_name` is the model file's basename only.
pub fn capture_model_load_failure(model_name: &str) {
    capture_failure_event(
        "flow.model_load.failed".to_string(),
        vec![("model", model_name.to_string())],
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    static CONSENT_TEST_LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

    #[test]
    fn scrub_text_redacts_sensitive_runs() {
        // Build the Stripe-style token at runtime so the literal never appears
        // verbatim in source (avoids tripping secret scanners on this fixture).
        let token = format!("sk_{}_{}", "live", "ABCDEFGHIJKLMNOPQRSTUVWX");
        let input = format!(
            r"open C:\Users\alice\secret.txt and \\fileserver\share\x from https://api.example.com/p at 10.0.0.5:9000 key {token} mail a@b.com"
        );
        let out = scrub_text(&input);
        assert!(!out.contains("alice"), "win path leaked: {out}");
        assert!(!out.contains("fileserver"), "unc path leaked: {out}");
        assert!(!out.contains("api.example.com"), "url leaked: {out}");
        assert!(!out.contains("10.0.0.5"), "ip leaked: {out}");
        assert!(!out.contains(&token), "token leaked: {out}");
        assert!(!out.contains("a@b.com"), "email leaked: {out}");
    }

    #[test]
    fn scrub_event_rebuilds_from_allowlist() {
        let frame = Frame {
            function: Some("do_work".into()),
            filename: Some("/Users/alice/app/src/main.rs".into()),
            abs_path: Some("/Users/alice/app/src/main.rs".into()),
            module: Some("app::secret".into()),
            lineno: Some(42),
            ..Default::default()
        };
        let exception = Exception {
            ty: "PanicException".into(),
            value: Some("failed reading /Users/alice/secret.txt at 192.168.1.20:8080".into()),
            module: Some("app::io".into()),
            stacktrace: Some(Stacktrace {
                frames: vec![frame],
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut event = Event {
            server_name: Some("alices-macbook".into()),
            transaction: Some("/Users/alice/route".into()),
            exception: Values {
                values: vec![exception],
            },
            ..Default::default()
        };
        event
            .extra
            .insert("transcript".into(), "my secret words".into());
        event.tags.insert("device".into(), "alices-macbook".into());

        let scrubbed = scrub_event(event, Some("install-123"));

        // Whole PII-bearing sections gone (dropped by reconstruction).
        assert!(scrubbed.server_name.is_none());
        assert!(scrubbed.transaction.is_none());
        assert!(scrubbed.extra.is_empty());
        let ex = &scrubbed.exception.values[0];
        assert_eq!(ex.ty, "PanicException", "error type must be kept");
        assert!(ex.module.is_none(), "exception.module must be dropped");
        let value = ex.value.as_deref().unwrap();
        assert!(!value.contains("/Users/alice"), "path leaked: {value}");
        assert!(!value.contains("192.168.1.20"), "ip leaked: {value}");
        let frame = &ex.stacktrace.as_ref().unwrap().frames[0];
        assert_eq!(frame.function.as_deref(), Some("do_work"), "shape kept");
        assert_eq!(frame.lineno, Some(42));
        assert!(frame.filename.is_none(), "filename dropped");
        assert!(frame.abs_path.is_none(), "abs_path dropped");
        assert!(frame.module.is_none(), "frame.module dropped");
        // Only the allowlisted tags survive (the injected "device" is gone).
        assert!(!scrubbed.tags.contains_key("device"));
        assert_eq!(
            scrubbed.tags.get("os").map(|s| s.as_str()),
            Some(std::env::consts::OS)
        );
        assert_eq!(
            scrubbed.tags.get("install_id").map(|s| s.as_str()),
            Some("install-123")
        );
    }

    #[test]
    fn read_consent_defaults_on_unless_opted_out() {
        let dir = tempfile::tempdir().unwrap();

        let missing = dir.path().join("settings");
        assert_eq!(read_consent_from_path(&missing), (true, None));

        let bad = dir.path().join("bad");
        std::fs::write(&bad, b"not json").unwrap();
        assert_eq!(read_consent_from_path(&bad), (true, None));

        let good = dir.path().join("good");
        let install_id = "018f3f5e-70b6-7ef0-a9d0-2d8c594cf0b3";
        std::fs::write(
            &good,
            format!(
                r#"{{"telemetry_enabled": true, "telemetry_install_id": "{install_id}", "hotkey": "Cmd+Space"}}"#
            ),
        )
        .unwrap();
        assert_eq!(
            read_consent_from_path(&good),
            (true, Some(install_id.to_string()))
        );

        // Non-UUID install ids are rejected (upstream hardening): they could
        // otherwise smuggle arbitrary strings into event tags.
        let invalid_id = dir.path().join("invalid-id");
        std::fs::write(
            &invalid_id,
            br#"{"telemetry_enabled": true, "telemetry_install_id": "/Users/alice/private"}"#,
        )
        .unwrap();
        assert_eq!(read_consent_from_path(&invalid_id), (true, None));

        // Explicit opt-out must always be honored even under the opt-out default.
        let off = dir.path().join("off");
        std::fs::write(&off, br#"{"telemetry_enabled": false}"#).unwrap();
        assert_eq!(read_consent_from_path(&off), (false, None));
    }
    #[test]
    fn frontend_error_event_keeps_type_and_message() {
        // An error message is diagnostic and is kept; `before_send` scrubs
        // structured secrets from it, and it is length-capped here.
        let event = build_frontend_error_event(Some("TypeError"), "kaboom");
        let ex = &event.exception.values[0];
        assert_eq!(ex.ty, "TypeError", "stable error type must be kept");
        assert_eq!(ex.value.as_deref(), Some("kaboom"), "message must be kept");

        // Over-long messages are length-capped.
        let long = "x".repeat(FRONTEND_ERROR_MAX_LEN + 500);
        let capped = build_frontend_error_event(None, &long);
        assert_eq!(
            capped.exception.values[0].value.as_deref().map(str::len),
            Some(FRONTEND_ERROR_MAX_LEN),
            "message must be length-capped"
        );
    }

    #[test]
    fn frontend_error_event_defaults_type_when_name_absent() {
        let event = build_frontend_error_event(None, "boom");
        assert_eq!(event.exception.values[0].ty, "FrontendError");
        assert_eq!(event.level, Level::Error);
    }

    // --- Native debug-metadata scrubbing tests --------------------------------

    #[test]
    fn basename_strips_directories() {
        // Unix paths.
        assert_eq!(basename("/usr/local/lib/libfoo.dylib"), "libfoo.dylib");
        assert_eq!(basename("/Users/alice/app/src/main.rs"), "main.rs");
        // Windows paths — must work on any host OS.
        assert_eq!(basename("C:\\Users\\alice\\app.exe"), "app.exe");
        assert_eq!(basename("\\\\server\\share\\lib\\foo.dll"), "foo.dll");
        // No separators — returned unchanged.
        assert_eq!(basename("libfoo.dylib"), "libfoo.dylib");
        assert_eq!(basename(""), "");
    }

    #[test]
    fn scrub_debug_meta_reduces_paths_keeps_ids() {
        use sentry::protocol::debugid::DebugId;
        use sentry::protocol::{Addr, AppleDebugImage, SymbolicDebugImage};
        use sentry::types::Uuid;

        let debug_id: DebugId = "5d2c9413-2edb-4a9e-9e9a-5d2c94132edb".parse().unwrap();
        let images = vec![
            DebugImage::Symbolic(SymbolicDebugImage {
                name: "/usr/local/lib/libvoicetypr.dylib".into(),
                arch: Some("arm64".into()),
                image_addr: Addr(0x100000),
                image_size: 65536,
                image_vmaddr: Addr(0x0),
                id: debug_id,
                code_id: None,
                debug_file: Some(
                    "/build/voicetypr.dylib.dSYM/Contents/Resources/DWARF/voicetypr.dylib".into(),
                ),
            }),
            DebugImage::Apple(AppleDebugImage {
                name: "/Users/builder/app/Frameworks/MyFw.framework/MyFw".into(),
                arch: Some("arm64".into()),
                cpu_type: None,
                cpu_subtype: None,
                image_addr: Addr(0x200000),
                image_size: 32768,
                image_vmaddr: Addr(0x0),
                uuid: Uuid::nil(),
            }),
        ];

        let meta = DebugMeta {
            sdk_info: None,
            images,
        };
        let scrubbed = scrub_debug_meta(meta);
        assert_eq!(scrubbed.images.len(), 2);

        // --- Symbolic image ---
        match &scrubbed.images[0] {
            DebugImage::Symbolic(img) => {
                assert_eq!(img.name, "libvoicetypr.dylib", "name must be basename");
                assert!(!img.name.contains('/'), "no path in name");
                assert_eq!(
                    img.debug_file.as_deref(),
                    Some("voicetypr.dylib"),
                    "debug_file must be basename"
                );
                assert_eq!(img.image_addr, Addr(0x100000), "image_addr preserved");
                assert_eq!(img.image_size, 65536, "image_size preserved");
                assert_eq!(img.id, debug_id, "debug id preserved");
            }
            _ => panic!("expected Symbolic image"),
        }

        // --- Apple image ---
        match &scrubbed.images[1] {
            DebugImage::Apple(img) => {
                assert_eq!(img.name, "MyFw", "name must be basename");
                assert!(!img.name.contains('/'), "no path in name");
                assert_eq!(img.uuid, Uuid::nil(), "uuid preserved");
                assert_eq!(img.image_addr, Addr(0x200000), "image_addr preserved");
                assert_eq!(img.image_size, 32768, "image_size preserved");
            }
            _ => panic!("expected Apple image"),
        }
    }

    #[test]
    fn scrub_frame_preserves_native_addresses() {
        use sentry::protocol::Addr;

        let frame = Frame {
            function: Some("transcribe".into()),
            filename: Some("/Users/alice/src/lib.rs".into()),
            abs_path: Some("/Users/alice/src/lib.rs".into()),
            module: Some("voicetypr::transcribe".into()),
            package: Some("voicetypr".into()),
            symbol: Some("_ZN12voicetypr10transcribe17h1234".into()),
            lineno: Some(42),
            colno: Some(8),
            in_app: Some(true),
            image_addr: Some(Addr(0x100000)),
            instruction_addr: Some(Addr(0x1000a0)),
            symbol_addr: Some(Addr(0x100080)),
            addr_mode: Some("abs".into()),
            vars: {
                let mut m = sentry::protocol::Map::new();
                m.insert("secret".into(), "value".into());
                m
            },
            ..Default::default()
        };

        let scrubbed = scrub_frame(frame);

        // Addresses preserved (needed for server-side symbolication).
        assert_eq!(scrubbed.instruction_addr, Some(Addr(0x1000a0)));
        assert_eq!(scrubbed.image_addr, Some(Addr(0x100000)));
        assert_eq!(scrubbed.symbol_addr, Some(Addr(0x100080)));
        assert_eq!(scrubbed.addr_mode.as_deref(), Some("abs"));

        // Path-bearing / PII fields dropped.
        assert!(scrubbed.filename.is_none(), "filename dropped");
        assert!(scrubbed.abs_path.is_none(), "abs_path dropped");
        assert!(scrubbed.module.is_none(), "module dropped");
        assert!(scrubbed.package.is_none(), "package dropped");
        assert!(scrubbed.symbol.is_none(), "symbol dropped");
        assert!(scrubbed.vars.is_empty(), "vars dropped");

        // Shape preserved.
        assert_eq!(scrubbed.function.as_deref(), Some("transcribe"));
        assert_eq!(scrubbed.lineno, Some(42));
        assert_eq!(scrubbed.colno, Some(8));
        assert_eq!(scrubbed.in_app, Some(true));
    }

    #[test]
    fn scrub_event_preserves_environment_release_channel() {
        let event = Event {
            level: Level::Error,
            release: Some("voicetypr@2.0.4".into()),
            environment: Some("production".into()),
            message: Some("boom".into()),
            ..Default::default()
        };

        let scrubbed = scrub_event(event, None);

        // Environment and release survive scrubbing.
        assert_eq!(scrubbed.environment.as_deref(), Some("production"));
        assert_eq!(scrubbed.release.as_deref(), Some("voicetypr@2.0.4"));

        // Release channel tag is present.
        assert_eq!(
            scrubbed.tags.get("release_channel").map(|s| s.as_str()),
            Some(RELEASE_CHANNEL)
        );

        // Arbitrary sections remain absent.
        assert!(scrubbed.server_name.is_none());
        assert!(scrubbed.user.is_none());
        assert!(scrubbed.request.is_none());
        assert!(scrubbed.extra.is_empty());
        assert!(scrubbed.contexts.is_empty());
        assert!(scrubbed.sdk.is_none());
        assert!(scrubbed.transaction.is_none());
        assert!(scrubbed.culprit.is_none());
    }

    #[test]
    fn scrub_event_preserves_debug_meta_through_pipeline() {
        use sentry::protocol::debugid::DebugId;
        use sentry::protocol::{Addr, SymbolicDebugImage};

        // An event with debug_meta that the DebugImagesIntegration would have
        // attached. scrub_event must carry the sanitized debug_meta through.
        let mut event: Event<'static> = Event {
            level: Level::Error,
            ..Default::default()
        };
        event.debug_meta = Cow::Owned(DebugMeta {
            sdk_info: None,
            images: vec![DebugImage::Symbolic(SymbolicDebugImage {
                name: "/usr/local/lib/libsecret.dylib".into(),
                arch: None,
                image_addr: Addr(0x400000),
                image_size: 131072,
                image_vmaddr: Addr(0x0),
                id: DebugId::default(),
                code_id: None,
                debug_file: None,
            })],
        });

        let scrubbed = scrub_event(event, None);

        // debug_meta survived and was scrubbed.
        assert_eq!(scrubbed.debug_meta.images.len(), 1);
        match &scrubbed.debug_meta.images[0] {
            DebugImage::Symbolic(img) => {
                assert_eq!(img.name, "libsecret.dylib", "path reduced to basename");
                assert_eq!(img.image_addr, Addr(0x400000), "address preserved");
            }
            _ => panic!("expected Symbolic image"),
        }
    }

    #[test]
    fn consent_transport_drops_envelopes_after_opt_out() {
        #[derive(Default)]
        struct CountingTransport(std::sync::atomic::AtomicUsize);

        impl sentry::Transport for CountingTransport {
            fn send_envelope(&self, _envelope: sentry::Envelope) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }

        let _lock = CONSENT_TEST_LOCK.lock();
        let was = is_enabled();
        let inner = Arc::new(CountingTransport::default());
        let transport = ConsentTransport {
            inner: inner.clone(),
        };

        set_enabled(true);
        sentry::Transport::send_envelope(&transport, sentry::Envelope::new());
        assert_eq!(inner.0.load(Ordering::SeqCst), 1);

        set_enabled(false);
        sentry::Transport::send_envelope(&transport, sentry::Envelope::new());
        assert_eq!(
            inner.0.load(Ordering::SeqCst),
            1,
            "an envelope flushed after opt-out must be discarded"
        );
        set_enabled(was);
    }

    // --- Failure event tests ---------------------------------------------------

    #[test]
    fn scrub_event_carries_allowlisted_failure_tags_and_drops_the_rest() {
        let mut event = Event {
            level: Level::Error,
            ..Default::default()
        };
        event.message = Some("flow.transcription.failed.whisper_encode_failed".into());
        event.tags.insert("engine".into(), "whisper".into());
        event.tags.insert("model".into(), "large-v3-turbo".into());
        event.tags.insert("backend".into(), "cpu".into());
        event
            .tags
            .insert("failure_class".into(), "whisper_encode_failed".into());
        event.tags.insert("duration_ms".into(), "145000".into());
        // Not allowlisted: must be dropped.
        event.tags.insert("device".into(), "alices-macbook".into());

        let scrubbed = scrub_event(event, None);

        assert_eq!(
            scrubbed.message.as_deref(),
            Some("flow.transcription.failed.whisper_encode_failed")
        );
        assert_eq!(
            scrubbed.tags.get("engine").map(String::as_str),
            Some("whisper")
        );
        assert_eq!(
            scrubbed.tags.get("model").map(String::as_str),
            Some("large-v3-turbo")
        );
        assert_eq!(
            scrubbed.tags.get("backend").map(String::as_str),
            Some("cpu")
        );
        assert_eq!(
            scrubbed.tags.get("failure_class").map(String::as_str),
            Some("whisper_encode_failed")
        );
        assert_eq!(
            scrubbed.tags.get("duration_ms").map(String::as_str),
            Some("145000")
        );
        assert!(!scrubbed.tags.contains_key("device"));
    }

    #[test]
    fn failure_event_tag_values_are_scrubbed() {
        let mut event = Event {
            level: Level::Error,
            ..Default::default()
        };
        // Even an allowlisted key must not carry a secret-looking value.
        event
            .tags
            .insert("model".into(), "/Users/alice/models/secret.bin".into());

        let scrubbed = scrub_event(event, None);
        let model = scrubbed.tags.get("model").unwrap();
        assert!(
            !model.contains("/Users/alice"),
            "path must be scrubbed: {model}"
        );
    }

    #[test]
    fn capture_helpers_never_panic_when_disabled() {
        let _lock = CONSENT_TEST_LOCK.lock();
        let was = is_enabled();
        set_enabled(false);
        capture_transcription_failure(
            "whisper",
            "large-v3-turbo",
            Some("cpu"),
            "whisper_encode_failed",
            Some(1),
        );
        capture_paste_failure("insert");
        capture_model_load_failure("large-v3-turbo.bin");
        set_enabled(was);
    }
}
