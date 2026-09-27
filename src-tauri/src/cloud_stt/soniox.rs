//! Soniox cloud STT: async Files + Transcriptions + poll flow.

use super::common::{self, AuthScheme};
use std::path::Path;
use tauri::AppHandle;

const BASE: &str = "https://api.soniox.com/v1";
const MANAGEMENT_REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const CLEANUP_DEADLINE: std::time::Duration = std::time::Duration::from_secs(60);
const CLEANUP_TIMEOUT_MESSAGE: &str = "Soniox cleanup timed out. Some records may already have been removed. Refresh storage counts and retry cleanup.";
const CLEANUP_ALREADY_RUNNING_MESSAGE: &str =
    "Soniox cleanup is already running. Wait for it to finish, then refresh storage counts.";

pub(super) async fn validate_key(key: &str) -> Result<(), String> {
    common::get_validate(
        "https://api.soniox.com/v1/models",
        AuthScheme::Bearer,
        key,
        "Soniox",
    )
    .await
    .map_err(|e| e.message("Soniox"))
}

/// API base for the async Files + Transcriptions flow. Tests override this
/// via [`set_base_url_override`] to point the whole flow at a wiremock server.
fn base_url() -> String {
    BASE_OVERRIDE.with(|cell| {
        cell.borrow()
            .as_ref()
            .cloned()
            .unwrap_or_else(|| BASE.to_string())
    })
}

thread_local! {
    static BASE_OVERRIDE: std::cell::RefCell<Option<String>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
fn set_base_url_override(base: Option<String>) {
    BASE_OVERRIDE.with(|cell| *cell.borrow_mut() = base);
}

// --- Active-job ownership and drain gating ------------------------------------
//
// Backlog drains (settings action + storage-wall auto-heal) must never delete
// the records of in-flight dictation flows: Soniox documents that deleting a
// file still referenced by a not-yet-processing transcription fails that job
// with `file_not_found`, and a queued/completed-but-not-yet-extracted
// transcription can be deleted server-side, which would kill a healthy
// dictation mid-flight.
//
// Two primitives make that decision race-free:
//
//   * Per-key listing_gate — a short-lived async gate. A flow holds a READ guard only
//     across its upload request (until the file id is registered) and across
//     its create request (until the transcription id is attached). This
//     closes the visibility window: a record becomes server-side list-visible
//     BEFORE its id reaches the client, so a drain listing taken while an id
//     is still in flight could see the record without the registration. A
//     drain takes a WRITE guard only around each listing + protection
//     snapshot — writes wait for in-flight upload/create windows, and flows
//     starting afterwards list absent records. Never held across a poll or
//     a whole dictation.
//   * Per-key active_jobs — RAII registry of app-owned uploads (file_id →
//     Option<transcription_id>). Drains snapshot it under the gate and skip
//     protected records in BOTH passes. Guards deregister on drop, so a
//     cancelled dictation cannot leak protection; its proven session-owned
//     leftovers remain eligible for the next drain.

// Only IDs actually returned to this client are cleanup candidates. Account
// listings, filenames and age cannot prove ownership. Unknown historical data
// and other installations' jobs are deliberately left untouched. This ledger
// lasts until process exit; failed deletions remain eligible for another try.
type OwnedRecord = ([u8; 32], String, String);
static OWNED_RECORDS: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashSet<OwnedRecord>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashSet::new()));

fn ownership_scope(key: &str) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut digest = Sha256::new();
    digest.update(base_url().as_bytes());
    digest.update([0]);
    digest.update(key.as_bytes());
    digest.finalize().into()
}

fn remember_owned(key: &str, collection: &str, id: &str) {
    OWNED_RECORDS
        .lock()
        .expect("ownership ledger poisoned")
        .insert((ownership_scope(key), collection.to_string(), id.to_string()));
}

fn is_owned(key: &str, collection: &str, id: &str) -> bool {
    OWNED_RECORDS
        .lock()
        .expect("ownership ledger poisoned")
        .contains(&(ownership_scope(key), collection.to_string(), id.to_string()))
}

// An uncertain create response or transport failure may still represent
// a live server job. Never reinterpret its upload as an orphan after the flow
// ends, even if a later listing omits that job or lacks its metadata.
type AmbiguousUpload = ([u8; 32], String);
static AMBIGUOUS_UPLOADS: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashSet<AmbiguousUpload>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashSet::new()));

fn preserve_ambiguous_upload(key: &str, file_id: &str) {
    let scope = ownership_scope(key);
    AMBIGUOUS_UPLOADS
        .lock()
        .expect("ambiguous uploads poisoned")
        .insert((scope, file_id.to_string()));
    OWNED_RECORDS
        .lock()
        .expect("ownership ledger poisoned")
        .remove(&(scope, "files".to_string(), file_id.to_string()));
}

fn is_ambiguous_upload(key: &str, file_id: &str) -> bool {
    AMBIGUOUS_UPLOADS
        .lock()
        .expect("ambiguous uploads poisoned")
        .contains(&(ownership_scope(key), file_id.to_string()))
}

fn forget_deleted(key: &str, url: &str) {
    let Some(relative) = url.strip_prefix(&format!("{}/", base_url())) else {
        return;
    };
    let Some((collection, id)) = relative.split_once('/') else {
        return;
    };
    OWNED_RECORDS
        .lock()
        .expect("ownership ledger poisoned")
        .remove(&(ownership_scope(key), collection.to_string(), id.to_string()));
}

/// RAII ownership of one flow's uploaded file and, once created, its
/// transcription. Dropping the guard releases the protection — cancellation
/// cannot leak it.
struct ActiveJobGuard {
    file_id: String,
    coordinator: std::sync::Arc<CleanupCoordinator>,
}

impl ActiveJobGuard {
    fn register(file_id: &str, key: &str) -> Self {
        remember_owned(key, "files", file_id);
        let coordinator = cleanup_coordinator(key);
        coordinator
            .active_jobs
            .lock()
            .expect("active-jobs registry poisoned")
            .insert(file_id.to_string(), None);
        Self {
            file_id: file_id.to_string(),
            coordinator,
        }
    }
}

impl Drop for ActiveJobGuard {
    fn drop(&mut self) {
        self.coordinator
            .active_jobs
            .lock()
            .expect("active-jobs registry poisoned")
            .remove(&self.file_id);
    }
}

/// Attaches a created transcription to its already-registered upload. Called
/// under the gate's read guard inside the create window; the flow always
/// registered the file first, so the entry exists. The returned transcription
/// ID proves record ownership independently; it does not prove file ownership.
fn attach_transcription(file_id: &str, transcription_id: &str, key: &str) {
    remember_owned(key, "transcriptions", transcription_id);
    if let Some(slot) = cleanup_coordinator(key)
        .active_jobs
        .lock()
        .expect("active-jobs registry poisoned")
        .get_mut(file_id)
    {
        *slot = Some(transcription_id.to_string());
    }
}

/// Snapshot of files owned by in-flight flows. The lock is never held across
/// an await point.
fn active_file_ids(key: &str) -> std::collections::HashSet<String> {
    cleanup_coordinator(key)
        .active_jobs
        .lock()
        .expect("active-jobs registry poisoned")
        .keys()
        .cloned()
        .collect()
}

/// Snapshot of transcription ids owned by in-flight flows.
fn active_transcription_ids(key: &str) -> std::collections::HashSet<String> {
    cleanup_coordinator(key)
        .active_jobs
        .lock()
        .expect("active-jobs registry poisoned")
        .values()
        .filter_map(|tid| tid.clone())
        .collect()
}

/// Best-effort cleanup of the Soniox records owned by one dictation flow.
/// Soniox documents that deleting a transcription does NOT cascade to its
/// uploaded file — `DELETE /files/{id}` is a separate call. Therefore:
///
///   * transcription present → delete it; only once the delete lands (2xx)
///     or the record is already gone (404 — e.g. a backlog drain raced us
///     and owns the file too), check fresh references before deleting its file.
///     Unknown, shared, or incomplete references retain the file for later cleanup.
///   * 409 → the job is still processing (poll-timeout path). The file is
///     still referenced by that live job — deleting it now would fail the
///     job with `file_not_found` — so BOTH records linger for a later
///     drain. A processing 409 is never permission to delete the file.
///   * no transcription (creation never succeeded) → the file is a pure
///     orphan and is deleted directly.
///
/// A 404 on the file delete itself is success: the goal state is "file
/// gone", whoever deleted it. ANY failed transcription delete — 409, 5xx,
/// rate limit, timeout — retains the file: the record may still exist and
/// reference it, and deleting the file would fail the live job with
/// `file_not_found`. Failures are logged and never fatal; leftover records
/// are reaped by the backlog drains. A landed record delete frees record
/// capacity and counts toward the self-heal's record wake; a 404 frees
/// nothing and must not count.
async fn cleanup_stored_records(
    client: &reqwest::Client,
    key: &str,
    transcription_id: Option<&str>,
    file_id: &str,
) {
    if cleanup_stored_records_with_deadline(
        client,
        key,
        transcription_id,
        file_id,
        CLEANUP_DEADLINE,
    )
    .await
    .is_err()
    {
        log::warn!("{CLEANUP_TIMEOUT_MESSAGE}");
    }
}

async fn cleanup_stored_records_with_deadline(
    client: &reqwest::Client,
    key: &str,
    transcription_id: Option<&str>,
    file_id: &str,
    deadline: std::time::Duration,
) -> Result<(), tokio::time::error::Elapsed> {
    tokio::time::timeout(
        deadline,
        cleanup_stored_records_inner(client, key, transcription_id, file_id),
    )
    .await
}

async fn cleanup_stored_records_inner(
    client: &reqwest::Client,
    key: &str,
    transcription_id: Option<&str>,
    file_id: &str,
) {
    let Some(tid) = transcription_id else {
        if is_ambiguous_upload(key, file_id) {
            return;
        }
        delete_file_best_effort(client, key, file_id).await;
        return;
    };
    let url = format!("{}/transcriptions/{tid}", base_url());
    let attempt = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        delete_one(client, key, &url),
    )
    .await;
    match attempt {
        Ok(Ok(DeleteOutcome::Deleted)) => {
            bump_record_freed_progress(key);
            delete_file_best_effort(client, key, file_id).await;
        }
        Ok(Ok(DeleteOutcome::AlreadyGone)) => {
            delete_file_best_effort(client, key, file_id).await;
        }
        Ok(Ok(DeleteOutcome::SkippedProcessing)) => {
            log::info!(
                "Soniox cleanup: transcription {tid} still processing; file {file_id} stays until the job finishes"
            );
        }
        Ok(Err(e)) => log::warn!(
            "Soniox cleanup: delete transcription {tid} failed: {e}; file {file_id} left for backlog cleanup"
        ),
        Err(_) => log::warn!("Soniox cleanup: delete transcription timed out ({url})"),
    }
}

/// Every file DELETE needs a fresh reference check immediately before it.
/// Earlier drain snapshots only select candidates. Hold this account's gate
/// through the check and DELETE so local creates cannot race between them.
/// The server has no atomic check-and-delete API; newly visible remote
/// references are protected, but remote creates cannot share this local gate.
/// `flow_owned` lets terminal cleanup proceed under its own active guard.
async fn delete_file_if_unreferenced(
    client: &reqwest::Client,
    key: &str,
    file_id: &str,
    flow_owned: bool,
) -> Result<Option<DeleteOutcome>, String> {
    let coordinator = cleanup_coordinator(key);
    let _gate = coordinator.listing_gate.write().await;
    if is_ambiguous_upload(key, file_id)
        || (!flow_owned
            && (!is_owned(key, "files", file_id) || active_file_ids(key).contains(file_id)))
    {
        return Ok(None);
    }
    let (records, skipped) = list_pages::<ListedTranscription>(client, key, "transcriptions")
        .await
        .map_err(cleanup_management_error)?;
    if skipped > 0 {
        return Err("incomplete transcription metadata; file retained".to_string());
    }
    if records
        .iter()
        .any(|record| record.known_file() == Some(file_id))
    {
        return Ok(None);
    }
    delete_one(client, key, &format!("{}/files/{file_id}", base_url()))
        .await
        .map(Some)
}

/// Deliver the transcript without waiting for provider maintenance. The task
/// retains active protection until cleanup finishes, including slow DELETEs.
fn spawn_terminal_cleanup(
    client: &reqwest::Client,
    key: &str,
    transcription_id: Option<String>,
    file_id: String,
    guard: ActiveJobGuard,
) {
    let client = client.clone();
    let key = key.to_string();
    tokio::spawn(async move {
        let _guard = guard;
        cleanup_stored_records(&client, &key, transcription_id.as_deref(), &file_id).await;
    });
}

/// Deletes a flow-owned uploaded file. 404 counts as success (idempotent
/// goal state); 409 is unexpected — files have no processing state. A
/// successful delete frees real org storage capacity, so it counts as
/// progress for the storage-wall self-heal waiters.
async fn delete_file_best_effort(client: &reqwest::Client, key: &str, file_id: &str) {
    match delete_file_if_unreferenced(client, key, file_id, true).await {
        Ok(Some(DeleteOutcome::Deleted)) => bump_auto_cleanup_progress(key),
        Ok(Some(DeleteOutcome::AlreadyGone)) => {}
        Ok(None) => log::info!(
            "Soniox cleanup: file {file_id} retained because references or protection remain"
        ),
        Ok(Some(DeleteOutcome::SkippedProcessing)) => {
            log::warn!("Soniox cleanup: unexpected 409 deleting file {file_id}")
        }
        Err(err) => log::warn!("Soniox cleanup: file {file_id} retained: {err}"),
    }
}

/// Writing-settings context for transcription hints; failures degrade to no
/// context rather than failing the dictation.
fn load_soniox_context(
    app: &AppHandle,
    language: Option<&str>,
) -> Option<crate::writing::SonioxContext> {
    match crate::writing::load_writing_settings(app) {
        Ok(settings) => crate::writing::compile_soniox_context(&settings, language),
        Err(err) => {
            log::warn!(
                "Failed to load writing settings for Soniox context; continuing without context: {err}"
            );
            None
        }
    }
}

// ---- Stored-record management (plan 060) ----
//
// Soniox retains every uploaded file AND every transcription record against
// org caps (default 1,000 files / 2,000 transcriptions). Dictations now
// delete their own records on completion; these commands retry proven
// session-owned leftovers, including records previously rejected with 409.
// Historical records without local proof of ownership stay untouched.

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SonioxStorageCounts {
    pub files_total: u64,
    pub transcriptions_total: u64,
}

#[derive(Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SonioxCleanupResult {
    pub deleted_transcriptions: u64,
    pub deleted_files: u64,
    pub skipped_processing: u64,
    /// Files kept because an in-flight dictation flow owns them or a live
    /// (still-processing) job references them — see the active-job
    /// ownership notes on [`drain_stored_records`].
    pub skipped_active: u64,
    /// Transcription records kept because an in-flight dictation flow owns
    /// them (possibly queued or completed-but-not-yet-extracted).
    pub skipped_active_jobs: u64,
    /// Listed records for which this process has no proof of ownership.
    pub skipped_unknown: u64,
    pub errors: Vec<String>,
}

fn stored_key(app: &AppHandle) -> Result<String, String> {
    crate::secure_store::secure_get(app, crate::cloud_stt::CloudProvider::Soniox.key_name())?
        .ok_or_else(|| "Soniox API key not set".to_string())
}

async fn get_total(
    client: &reqwest::Client,
    key: &str,
    collection: &str,
) -> Result<u64, common::SttError> {
    get_total_with_timeout(client, key, collection, MANAGEMENT_REQUEST_TIMEOUT).await
}

async fn get_total_with_timeout(
    client: &reqwest::Client,
    key: &str,
    collection: &str,
    timeout: std::time::Duration,
) -> Result<u64, common::SttError> {
    let url = format!("{}/{collection}/count", base_url());
    let resp = client
        .get(&url)
        .timeout(timeout)
        .bearer_auth(key)
        .send()
        .await
        .map_err(|e| common::classify_reqwest_err(&e))?;
    if !resp.status().is_success() {
        return Err(common::log_soniox_http_body(resp, "Soniox storage count").await);
    }
    let json: serde_json::Value = resp.json().await.map_err(|err| {
        if err.is_timeout() {
            common::SttError::Timeout
        } else {
            common::SttError::BadResponse
        }
    })?;
    json.get("total")
        .and_then(|v| v.as_u64())
        .ok_or(common::SttError::BadResponse)
}

/// A transcription's file reference, exactly as documented: present for
/// uploaded-file jobs, explicit `null` for jobs without one (e.g. URL-based
/// transcriptions). A MISSING or malformed `file_id` fails the item's
/// decode — the record is counted unparseable and destructive file work
/// fails closed instead of guessing the reference state.
#[derive(serde::Deserialize)]
#[serde(untagged)]
enum TranscriptionFileRef {
    File(String),
    NoFile,
}

/// Minimal typed metadata for a listed transcription: only the fields the
/// drain needs, so no full provider JSON pages are retained while paging
/// through up to 100k records.
#[derive(serde::Deserialize)]
struct ListedTranscription {
    id: String,
    file_id: TranscriptionFileRef,
}

impl ListedTranscription {
    /// The known uploaded-file reference, if this job has one.
    fn known_file(&self) -> Option<&str> {
        match &self.file_id {
            TranscriptionFileRef::File(file_id) => Some(file_id),
            TranscriptionFileRef::NoFile => None,
        }
    }
}

/// Minimal typed metadata for a listed file.
#[derive(serde::Deserialize)]
struct ListedFile {
    id: String,
}

/// Cursor-paginated typed listing for a collection ("files" |
/// "transcriptions"). Defensively accepts `items`/`data` array keys; Soniox
/// documents `next_page_cursor` and a 1000-item page cap. Returns the
/// decoded items plus the number of undecodable items — callers MUST treat
/// any skipped item as unknown reference state and fail destructive work
/// closed. A response without a recognized record array is an error, never
/// an empty page.
async fn list_pages<T: serde::de::DeserializeOwned>(
    client: &reqwest::Client,
    key: &str,
    collection: &str,
) -> Result<(Vec<T>, usize), common::SttError> {
    list_pages_with_timeout(client, key, collection, MANAGEMENT_REQUEST_TIMEOUT).await
}

async fn list_pages_with_timeout<T: serde::de::DeserializeOwned>(
    client: &reqwest::Client,
    key: &str,
    collection: &str,
    timeout: std::time::Duration,
) -> Result<(Vec<T>, usize), common::SttError> {
    let mut items = Vec::new();
    let mut skipped = 0;
    let mut cursor: Option<String> = None;
    let mut seen_cursors = std::collections::HashSet::new();
    let mut pages = 0;
    loop {
        pages += 1;
        if pages > 100 || items.len() + skipped >= 100_000 {
            return Err(common::SttError::BadResponse);
        }
        let url = format!("{}/{collection}", base_url());
        let mut request = client
            .get(&url)
            .timeout(timeout)
            .query(&[("limit", "1000")]);
        if let Some(c) = &cursor {
            request = request.query(&[("cursor", c)]);
        }
        let resp = request
            .bearer_auth(key)
            .send()
            .await
            .map_err(|e| common::classify_reqwest_err(&e))?;
        if !resp.status().is_success() {
            return Err(common::log_soniox_http_body(resp, "Soniox storage list").await);
        }
        let json: serde_json::Value = resp.json().await.map_err(|err| {
            if err.is_timeout() {
                common::classify_reqwest_err(&err)
            } else {
                common::SttError::BadResponse
            }
        })?;
        let Some(page) = json
            .get(collection)
            .or_else(|| json.get("items"))
            .or_else(|| json.get("data"))
            .and_then(|v| v.as_array())
            .cloned()
        else {
            return Err(common::SttError::BadResponse);
        };
        for item in page {
            match serde_json::from_value::<T>(item) {
                Ok(typed) => items.push(typed),
                Err(_) => skipped += 1,
            }
        }
        if items.len() + skipped > 100_000 {
            return Err(common::SttError::BadResponse);
        }
        cursor = match json.get("next_page_cursor") {
            None | Some(serde_json::Value::Null) => None,
            Some(serde_json::Value::String(value)) if value.is_empty() => None,
            Some(serde_json::Value::String(value)) => Some(value.clone()),
            _ => return Err(common::SttError::BadResponse),
        };
        if let Some(next) = &cursor {
            if !seen_cursors.insert(next.clone()) {
                return Err(common::SttError::BadResponse);
            }
        } else {
            break;
        }
    }
    Ok((items, skipped))
}

/// One paced delete: 409 = record still processing (skipped), 429 = file-
/// management RPM (back off once, then count as an error). File-management
/// RPM is real but its numeric limit is undocumented — pace every request.
enum DeleteOutcome {
    Deleted,
    /// 409: record still processing — leave it for a later pass.
    SkippedProcessing,
    /// 404: already gone (raced with another cleanup, or a drain beat us) —
    /// the goal state, not an error. Soniox never cascades deletes.
    AlreadyGone,
}

async fn delete_one(
    client: &reqwest::Client,
    key: &str,
    url: &str,
) -> Result<DeleteOutcome, String> {
    delete_one_with_timeout(client, key, url, MANAGEMENT_REQUEST_TIMEOUT).await
}

async fn delete_one_with_timeout(
    client: &reqwest::Client,
    key: &str,
    url: &str,
    timeout: std::time::Duration,
) -> Result<DeleteOutcome, String> {
    for attempt in 0..2 {
        let resp = client
            .delete(url)
            .timeout(timeout)
            .bearer_auth(key)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let status = resp.status();
        if status.is_success() {
            forget_deleted(key, url);
            return Ok(DeleteOutcome::Deleted);
        }
        if status == reqwest::StatusCode::CONFLICT {
            return Ok(DeleteOutcome::SkippedProcessing);
        }
        if status == reqwest::StatusCode::NOT_FOUND {
            forget_deleted(key, url);
            return Ok(DeleteOutcome::AlreadyGone);
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS && attempt == 0 {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            continue;
        }
        return Err(format!("HTTP {status}"));
    }
    Err("rate limited".to_string())
}

pub(crate) async fn storage_counts(app: &AppHandle) -> Result<SonioxStorageCounts, String> {
    let key = stored_key(app)?;
    let client = common::http_client();
    let (files, transcriptions) = tokio::try_join!(
        get_total(&client, &key, "files"),
        get_total(&client, &key, "transcriptions")
    )
    .map_err(|e| e.message("Soniox"))?;
    Ok(SonioxStorageCounts {
        files_total: files,
        transcriptions_total: transcriptions,
    })
}

pub(crate) async fn cleanup_stored(app: &AppHandle) -> Result<SonioxCleanupResult, String> {
    use tauri::Emitter;

    let key = stored_key(app)?;
    let client = common::http_client();

    let report = |done: u64, total: u64| {
        let _ = app.emit(
            "soniox-cleanup-progress",
            serde_json::json!({ "deleted": done, "total": total }),
        );
    };

    run_manual_cleanup_with_deadline(&client, &key, Some(&report), CLEANUP_DEADLINE).await
}

/// Drains stored Soniox records so retained files free capacity promptly:
/// each transcription record whose delete lands has its file deleted
/// immediately (transcription deletion does NOT cascade server-side —
/// docs), so the storage-wall self-heal only needs the first delete
/// (~tens of ms) instead of a full backlog pass before upload capacity
/// frees. The final fresh listings then reap whatever remains:
/// pre-existing orphans, files whose inline delete was deferred or failed,
/// and files whose owning record no longer exists.
///
/// Both listing passes run under the drain gate's WRITE guard, which makes
/// the protection snapshot exact (see the module-level ownership notes):
/// flows hold READ guards only across upload→register and
/// create→attach, so any record a listing can see is either already
/// registered in the scoped active_jobs registry or was created after the listing and is
/// therefore absent from it. Nothing is deleted that
///   * is registered to an in-flight dictation flow — including queued or
///     completed-but-not-yet-extracted jobs, whose records the API would
///     happily delete;
///   * is referenced by a record whose delete was refused (409 still
///     processing, or a transient failure): the record still exists and
///     its live job still needs the file;
///   * is referenced by ANY record surviving into the fresh pass-2
///     transcription listing — this closes the gap for jobs created after
///     pass 1 whose guard dropped before pass 2;
///   * inline: shares its file with another listed record (API files may
///     back several transcriptions) — only a sole surviving reference may
///     be freed inline, everything else waits for pass 2.
///
/// File cleanup FAILS CLOSED on incomplete metadata: any undecodable
/// record/file item, or a response without the documented array, aborts
/// all file deletion for the run instead of guessing a reference state.
///
/// Pacing: Soniox's file-management RPM is real but its numeric limit is
/// undocumented. Shared by the settings clean-up command (progress events)
/// and the background auto-clean: FILE deletions bump the file-capacity
/// counter and record deletions the record-capacity counter, so the
/// storage-wall retry wakes on capacity that actually freed.
#[cfg(test)]
async fn drain_stored_records(
    client: &reqwest::Client,
    key: &str,
    report: Option<&(dyn Fn(u64, u64) + Send + Sync)>,
) -> Result<SonioxCleanupResult, String> {
    drain_stored_records_with_deadline(client, key, report, CLEANUP_DEADLINE).await
}

/// Both manual and automatic cleanup use this bound, including gate waits,
/// pagination, pacing and retries. Timing out drops the inner future and its
/// listing guard; deletions already accepted by the server are not rolled back.
async fn drain_stored_records_with_deadline(
    client: &reqwest::Client,
    key: &str,
    report: Option<&(dyn Fn(u64, u64) + Send + Sync)>,
    deadline: std::time::Duration,
) -> Result<SonioxCleanupResult, String> {
    tokio::time::timeout(deadline, drain_stored_records_inner(client, key, report))
        .await
        .map_err(|_| CLEANUP_TIMEOUT_MESSAGE.to_string())?
}

fn cleanup_management_error(error: common::SttError) -> String {
    if matches!(error, common::SttError::Timeout) {
        CLEANUP_TIMEOUT_MESSAGE.to_string()
    } else {
        error.message("Soniox")
    }
}

async fn drain_stored_records_inner(
    client: &reqwest::Client,
    key: &str,
    report: Option<&(dyn Fn(u64, u64) + Send + Sync)>,
) -> Result<SonioxCleanupResult, String> {
    let mut result = SonioxCleanupResult::default();

    // Pass 1 — transcription records. Listing + protection snapshot under
    // the write gate; deletes run outside it.
    let (transcriptions, skipped_records, mut protected_files, ref_counts) = {
        let coordinator = cleanup_coordinator(key);
        let _drain_gate = coordinator.listing_gate.write().await;
        let (transcriptions, skipped_records) =
            list_pages::<ListedTranscription>(client, key, "transcriptions")
                .await
                .map_err(cleanup_management_error)?;
        let active_tids = active_transcription_ids(key);
        let mut protected_files = std::collections::HashSet::new();
        let mut ref_counts: std::collections::HashMap<String, u64> =
            std::collections::HashMap::new();
        for item in &transcriptions {
            if let Some(file_id) = item.known_file() {
                *ref_counts.entry(file_id.to_string()).or_insert(0) += 1;
                if active_tids.contains(&item.id) {
                    protected_files.insert(file_id.to_string());
                }
            }
        }
        (transcriptions, skipped_records, protected_files, ref_counts)
    };
    // Fail closed: undecodable records have unknown file references, so no
    // file may be classified as unreferenced this run. Record deletes of
    // fully parsed items can still proceed.
    let mut files_fail_closed = skipped_records > 0;
    if files_fail_closed {
        result.errors.push(format!(
            "{skipped_records} transcription records had incomplete metadata; file cleanup skipped"
        ));
    }
    let mut total_ops = transcriptions.len() as u64;
    let mut done_ops: u64 = 0;

    for item in &transcriptions {
        // Every inspected record counts, including unknown and active skips.
        done_ops += 1;
        if !is_owned(key, "transcriptions", &item.id) {
            result.skipped_unknown += 1;
            if let Some(file_id) = item.known_file() {
                protected_files.insert(file_id.to_string());
            }
            if done_ops.is_multiple_of(25) {
                if let Some(report) = report {
                    report(done_ops, total_ops);
                }
            }
            continue;
        }
        // App-owned job — the gate normally keeps these out of the snapshot
        // path entirely; this re-check covers attaches that raced between
        // the pass-1 snapshot and this delete. Never delete the record, and
        // keep its file protected for the file pass.
        if active_transcription_ids(key).contains(&item.id) {
            result.skipped_active_jobs += 1;
            if let Some(file_id) = item.known_file() {
                protected_files.insert(file_id.to_string());
            }
            if done_ops.is_multiple_of(25) {
                if let Some(report) = report {
                    report(done_ops, total_ops);
                }
            }
            continue;
        }
        let url = format!("{}/transcriptions/{}", base_url(), item.id);
        match delete_one(client, key, &url).await {
            Ok(DeleteOutcome::SkippedProcessing) => {
                result.skipped_processing += 1;
                if let Some(file_id) = item.known_file() {
                    protected_files.insert(file_id.to_string());
                }
            }
            Err(e) => {
                result
                    .errors
                    .push(format!("transcription {}: {e}", item.id));
                // The record still exists and references its file — the
                // file pass must not break the live job.
                if let Some(file_id) = item.known_file() {
                    protected_files.insert(file_id.to_string());
                }
            }
            outcome => {
                // Deleted or AlreadyGone: its file is an inline candidate.
                // Recheck fresh references so capacity can free without
                // waiting for the whole backlog pass. Only when this was the
                // record's SOLE listed reference, no failed/processing
                // record retains the file, no flow owns it, and no metadata
                // was undecodable (unknown refs possible).
                if matches!(outcome, Ok(DeleteOutcome::Deleted)) {
                    result.deleted_transcriptions += 1;
                    bump_record_freed_progress(key);
                }
                if let Some(file_id) = item.known_file() {
                    if is_owned(key, "files", file_id)
                        && !files_fail_closed
                        && ref_counts.get(file_id).copied() == Some(1)
                        && !protected_files.contains(file_id)
                        && !active_file_ids(key).contains(file_id)
                    {
                        // The old sole-reference snapshot selects a candidate;
                        // only a fresh check may authorize its actual DELETE.
                        // Retained/failed candidates remain for the file pass.
                        if let Ok(Some(DeleteOutcome::Deleted)) =
                            delete_file_if_unreferenced(client, key, file_id, false).await
                        {
                            result.deleted_files += 1;
                            // Inline work is real work: keep the
                            // progress counts monotonic and truthful.
                            done_ops += 1;
                            total_ops += 1;
                            bump_auto_cleanup_progress(key);
                        }
                    }
                }
            }
        }
        if done_ops.is_multiple_of(25) {
            if let Some(report) = report {
                report(done_ops, total_ops);
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }

    // Pass 2 — fresh listings under the write gate. The fresh transcription
    // listing closes the post-pass-1 gap: jobs created while pass 1 was
    // deleting, whose guard then dropped, still reference their files and
    // appear here. A file is deletable only when no surviving record and no
    // in-flight flow references it.
    if !files_fail_closed {
        let mut protected = protected_files;
        let mut file_items: Vec<ListedFile> = Vec::new();
        {
            let coordinator = cleanup_coordinator(key);
            let _drain_gate = coordinator.listing_gate.write().await;
            let (fresh_transcriptions, fresh_skipped) =
                list_pages::<ListedTranscription>(client, key, "transcriptions")
                    .await
                    .map_err(cleanup_management_error)?;
            for file_id in fresh_transcriptions.iter().filter_map(|t| t.known_file()) {
                protected.insert(file_id.to_string());
            }
            if fresh_skipped > 0 {
                files_fail_closed = true;
                result.errors.push(format!(
                    "{fresh_skipped} transcription records had incomplete metadata; file cleanup skipped"
                ));
            }
            if !files_fail_closed {
                let (items, skipped_files) = list_pages::<ListedFile>(client, key, "files")
                    .await
                    .map_err(cleanup_management_error)?;
                if skipped_files > 0 {
                    files_fail_closed = true;
                    result.errors.push(format!(
                        "{skipped_files} file records had incomplete metadata; file cleanup skipped"
                    ));
                } else {
                    file_items = items;
                }
            }
            // Registry snapshot under the same gate as the listings.
            protected.extend(active_file_ids(key));
        }
        if !files_fail_closed {
            total_ops += file_items.len() as u64;
            for item in &file_items {
                done_ops += 1;
                if !is_owned(key, "files", &item.id) {
                    result.skipped_unknown += 1;
                } else if protected.contains(&item.id) {
                    result.skipped_active += 1;
                } else {
                    match delete_file_if_unreferenced(client, key, &item.id, false).await {
                        Ok(Some(DeleteOutcome::Deleted)) => {
                            result.deleted_files += 1;
                            bump_auto_cleanup_progress(key);
                        }
                        Ok(Some(DeleteOutcome::AlreadyGone)) => {}
                        // Files have no processing state; a 409 here is
                        // unexpected.
                        Ok(Some(DeleteOutcome::SkippedProcessing)) => {
                            result.errors.push(format!("file {}: HTTP 409", item.id))
                        }
                        Ok(None) => result.skipped_active += 1,
                        Err(e) => result.errors.push(format!("file {}: {e}", item.id)),
                    }
                }
                if done_ops.is_multiple_of(25) {
                    if let Some(report) = report {
                        report(done_ops, total_ops);
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            }
        }
    }
    if let Some(report) = report {
        report(done_ops, total_ops);
    }
    Ok(result)
}

// --- Storage-limit self-heal (plan 060) --------------------------------------
//
// When a dictation hits Soniox's storage wall, retry cleanup of records
// proven to belong to this app session, then retry dictation once. Unknown
// historical records and ambiguous uploads stay untouched. If capacity is
// still exhausted, surface the recovery route and provider-console guidance.

#[derive(Default)]
struct CleanupCoordinator {
    listing_gate: tokio::sync::RwLock<()>,
    active_jobs: std::sync::Mutex<std::collections::HashMap<String, Option<String>>>,
    running: std::sync::atomic::AtomicBool,
    files_freed: std::sync::atomic::AtomicU64,
    records_freed: std::sync::atomic::AtomicU64,
}

type CleanupCoordinators = std::collections::HashMap<[u8; 32], std::sync::Arc<CleanupCoordinator>>;
static CLEANUP_COORDINATORS: std::sync::LazyLock<std::sync::Mutex<CleanupCoordinators>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

fn cleanup_coordinator(key: &str) -> std::sync::Arc<CleanupCoordinator> {
    CLEANUP_COORDINATORS
        .lock()
        .expect("cleanup coordinators poisoned")
        .entry(ownership_scope(key))
        .or_default()
        .clone()
}

struct CleanupRunningGuard(std::sync::Arc<CleanupCoordinator>);

impl CleanupRunningGuard {
    fn acquire(coordinator: std::sync::Arc<CleanupCoordinator>) -> Option<Self> {
        if coordinator
            .running
            .swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            None
        } else {
            Some(Self(coordinator))
        }
    }
}

impl Drop for CleanupRunningGuard {
    fn drop(&mut self) {
        self.0
            .running
            .store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

fn try_acquire_cleanup(key: &str) -> Option<CleanupRunningGuard> {
    CleanupRunningGuard::acquire(cleanup_coordinator(key))
}

async fn run_manual_cleanup_with_deadline(
    client: &reqwest::Client,
    key: &str,
    report: Option<&(dyn Fn(u64, u64) + Send + Sync)>,
    deadline: std::time::Duration,
) -> Result<SonioxCleanupResult, String> {
    let _guard =
        try_acquire_cleanup(key).ok_or_else(|| CLEANUP_ALREADY_RUNNING_MESSAGE.to_string())?;
    drain_stored_records_with_deadline(client, key, report, deadline).await
}

/// Progress counters for the storage-wall self-heal. FILE deletions free
/// retained-audio capacity (file-count/size walls gate the retried
/// upload); record deletions free transcription-count capacity (they gate
/// the retried create). Soniox does not cascade, and old records may carry
/// no file at all, so each kind is tracked separately and the retry waits
/// on the kind its wall actually capped.
fn bump_auto_cleanup_progress(key: &str) {
    cleanup_coordinator(key)
        .files_freed
        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
}

fn bump_record_freed_progress(key: &str) {
    cleanup_coordinator(key)
        .records_freed
        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
}

/// Baselines for [`wait_for_cleanup_progress`]. MUST be captured before
/// `spawn_auto_cleanup` so the drain's first deletion can never be missed.
fn auto_cleanup_progress_baselines(key: &str) -> (u64, u64) {
    use std::sync::atomic::Ordering;
    let coordinator = cleanup_coordinator(key);
    (
        coordinator.files_freed.load(Ordering::SeqCst),
        coordinator.records_freed.load(Ordering::SeqCst),
    )
}

/// Starts the background drain unless one is already running. Each deleted
/// file immediately frees org storage capacity, so a blocked dictation only
/// needs the FIRST relevant deletion before retrying.
fn spawn_auto_cleanup(client: reqwest::Client, key: String) {
    spawn_auto_cleanup_with_deadline(client, key, CLEANUP_DEADLINE);
}

fn spawn_auto_cleanup_with_deadline(
    client: reqwest::Client,
    key: String,
    deadline: std::time::Duration,
) {
    let Some(guard) = try_acquire_cleanup(&key) else {
        return;
    };
    tokio::spawn(async move {
        let _guard = guard;
        log::info!("Soniox storage limit hit: background cleanup started");
        match drain_stored_records_with_deadline(&client, &key, None, deadline).await {
            Ok(totals) => log::info!(
                "Soniox background cleanup finished: {} transcriptions + {} files deleted, {} processing-skipped, {} active-protected files, {} active-protected jobs, {} errors",
                totals.deleted_transcriptions,
                totals.deleted_files,
                totals.skipped_processing,
                totals.skipped_active,
                totals.skipped_active_jobs,
                totals.errors.len()
            ),
            Err(e) => log::warn!("Soniox background cleanup failed: {e}"),
        }
    });
}

/// Waits (polled, bounded) until the capacity kind the wall capped has
/// actually been freed: file-storage walls wake on FILE deletions,
/// record-count walls wake on transcription deletions — unblocking on the
/// wrong kind would retry straight into the same wall. Also returns as
/// soon as the owned drain FINISHES (everything it can free has been
/// freed; the retry attempt is the source of truth) or when `budget`
/// elapses. Baselines must come from `auto_cleanup_progress_baselines`
/// captured BEFORE the drain was spawned.
async fn wait_for_cleanup_progress(
    key: &str,
    file_storage: bool,
    files_baseline: u64,
    records_baseline: u64,
    budget: std::time::Duration,
) {
    use std::sync::atomic::Ordering;
    let coordinator = cleanup_coordinator(key);
    let start = std::time::Instant::now();
    while start.elapsed() < budget {
        let freed = if file_storage {
            coordinator.files_freed.load(Ordering::SeqCst) > files_baseline
        } else {
            coordinator.records_freed.load(Ordering::SeqCst) > records_baseline
        };
        if freed || !coordinator.running.load(Ordering::SeqCst) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
}

/// Frontend escalation when the storage-limit self-heal did NOT succeed:
/// same shape as the license-required flow — bring the dashboard to front
/// and let the main window navigate itself to the Soniox stored-files card.
/// No toast action needed: the user lands directly on the fix.
async fn notify_storage_limit(app: &AppHandle) {
    use tauri::Emitter;
    let _ = crate::commands::window::focus_main_window(app.clone()).await;
    let _ = app.emit(
        "soniox-storage-limit",
        serde_json::json!({
            "title": "Soniox storage limit reached",
            "message": "Automatic cleanup could not free enough space. Sources → Cloud → Clean up stored files retries records from this app session. Manage older or other-client records in the Soniox console.",
            "autoHealed": false,
        }),
    );
}

fn build_create_payload(
    model: &str,
    file_id: &str,
    language: Option<&str>,
    context: Option<crate::writing::SonioxContext>,
    diarize: bool,
) -> serde_json::Value {
    let mut payload = serde_json::json!({
        "model": model,
        "file_id": file_id,
    });

    if let Some(lang) = language.map(str::trim).filter(|lang| !lang.is_empty()) {
        payload["language_hints"] = serde_json::json!([lang]);
    }

    if let Some(context) = context {
        if let Ok(context_value) = serde_json::to_value(context) {
            if context_value
                .as_object()
                .is_some_and(|object| !object.is_empty())
            {
                payload["context"] = context_value;
            }
        }
    }

    if diarize {
        payload["enable_speaker_diarization"] = serde_json::json!(true);
    }

    payload
}

pub(super) async fn transcribe_typed(
    app: &AppHandle,
    key: &str,
    model: &str,
    wav_path: &Path,
    language: Option<&str>,
) -> Result<String, common::SttError> {
    use tokio::fs;

    let wav_bytes = fs::read(wav_path)
        .await
        .map_err(|_| common::SttError::BadResponse)?;
    let client = common::http_client();
    let soniox_context = load_soniox_context(app, language);

    let result = transcribe_typed_with_autoheal(
        &client,
        key,
        model,
        wav_path,
        wav_bytes,
        language,
        soniox_context,
    )
    .await;
    if matches!(result, Err(common::SttError::LimitExceeded { .. })) {
        notify_storage_limit(app).await;
    }
    result
}

/// Upload → transcribe → delete-records with the plan-060 storage-limit
/// self-heal: on `LimitExceeded`, a background cleanup drains stored
/// records (each deleted file immediately frees upload capacity); once the
/// first FILE is gone the WHOLE flow restarts from upload — the
/// just-uploaded file may itself have been deleted by the cleanup, so the
/// create step must not be reused. The retry's fresh upload is registered
/// as active, so the still-running drain cannot delete it out from under
/// its transcription. One retry; a second limit wall is terminal.
async fn transcribe_typed_with_autoheal(
    client: &reqwest::Client,
    key: &str,
    model: &str,
    wav_path: &Path,
    wav_bytes: Vec<u8>,
    language: Option<&str>,
    soniox_context: Option<crate::writing::SonioxContext>,
) -> Result<String, common::SttError> {
    const LIMIT_ATTEMPTS: usize = 2;
    for attempt in 1..=LIMIT_ATTEMPTS {
        match attempt_typed_once(
            client,
            key,
            model,
            wav_path,
            &wav_bytes,
            language,
            soniox_context.clone(),
        )
        .await
        {
            Ok(text) => return Ok(text),
            Err(common::SttError::LimitExceeded { file_storage }) if attempt < LIMIT_ATTEMPTS => {
                // Baselines BEFORE spawn: the drain's first deletion must
                // never be missed by the wait.
                let (files_baseline, records_baseline) = auto_cleanup_progress_baselines(key);
                spawn_auto_cleanup(client.clone(), key.to_string());
                wait_for_cleanup_progress(
                    key,
                    file_storage,
                    files_baseline,
                    records_baseline,
                    std::time::Duration::from_secs(8),
                )
                .await;
            }
            Err(e) => return Err(e),
        }
    }
    unreachable!("attempt loop always returns within LIMIT_ATTEMPTS")
}

/// One full attempt: upload + create/poll/extract + record cleanup. The
/// uploaded file is registered as an active upload until cleanup finishes.
async fn attempt_typed_once(
    client: &reqwest::Client,
    key: &str,
    model: &str,
    wav_path: &Path,
    wav_bytes: &[u8],
    language: Option<&str>,
    soniox_context: Option<crate::writing::SonioxContext>,
) -> Result<String, common::SttError> {
    use reqwest::multipart::{Form, Part};

    // 1) Upload file -> file_id
    let filename = wav_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("audio.wav")
        .to_string();
    let upload_url = format!("{}/files", base_url());
    // The gate's read guard spans exactly the window in which the upload
    // becomes server-side list-visible (during the POST) but its id is not
    // yet registered — a drain listing cannot interleave here. Released
    // right after registration; never held across poll/extract.
    let coordinator = cleanup_coordinator(key);
    let _upload_gate = coordinator.listing_gate.read().await;
    let upload_resp = common::with_retry(|| {
        let client = client.clone();
        let filename = filename.clone();
        let upload_url = upload_url.clone();
        let wav_bytes = wav_bytes.to_vec();
        async move {
            let file_part = Part::bytes(wav_bytes)
                .file_name(filename)
                .mime_str("audio/wav")
                .map_err(|_| common::SttError::BadResponse)?;
            let form = Form::new().part("file", file_part);

            let resp = client
                .post(&upload_url)
                .bearer_auth(key)
                .multipart(form)
                .send()
                .await
                .map_err(|e| common::classify_reqwest_err(&e))?;
            if resp.status().is_success() {
                Ok(resp)
            } else {
                Err(common::log_soniox_http_body(resp, "Soniox upload").await)
            }
        }
    })
    .await?;
    let upload_json: serde_json::Value = upload_resp
        .json()
        .await
        .map_err(|_| common::SttError::BadResponse)?;
    let file_id = upload_json
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or(common::SttError::BadResponse)?
        .to_string();
    // Own the upload for the rest of this attempt: drains must never see
    // it as unreferenced backlog. Dropped on return or cancellation, so
    // protection cannot leak; proven session-owned leftovers remain drainable.
    let _active_file = ActiveJobGuard::register(&file_id, key);
    drop(_upload_gate);

    let (transcription_id, result) =
        run_typed_transcription(client, key, model, &file_id, language, soniox_context).await;

    // Soniox stores every uploaded file + transcription record against the
    // org's caps (1k files / 2k transcriptions). Delete-after-extract on
    // ALL exits — success included (plan 060). The transcription delete
    // does NOT cascade to the file server-side, so terminal exits delete
    // the file explicitly; a processing job keeps both records.
    spawn_terminal_cleanup(client, key, transcription_id, file_id, _active_file);
    result
}

/// Creating a job is non-idempotent: transport failures and 5xx may hide an
/// accepted job. Protect the upload before sending and never retry ambiguous
/// outcomes. Only a definitive throttle rejection gets one bounded retry.
async fn create_transcription(
    client: &reqwest::Client,
    key: &str,
    file_id: &str,
    payload: &serde_json::Value,
) -> Result<String, common::SttError> {
    // Listing must not interleave with create/ownership registration.
    let coordinator = cleanup_coordinator(key);
    let _create_gate = coordinator.listing_gate.read().await;
    let owned_upload = is_owned(key, "files", file_id);
    // Do not create additional jobs against an already uncertain upload.
    if is_ambiguous_upload(key, file_id) {
        return Err(common::SttError::BadResponse);
    }
    for attempt in 0..2 {
        preserve_ambiguous_upload(key, file_id);
        let response = client
            .post(format!("{}/transcriptions", base_url()))
            .bearer_auth(key)
            .json(payload)
            .send()
            .await
            .map_err(|err| common::classify_reqwest_err(&err))?;
        let status = response.status();
        if !status.is_success() {
            // These statuses explicitly reject the request. A timeout,
            // connection loss, 5xx, or other status cannot prove rejection.
            let rejected = matches!(status.as_u16(), 400 | 401 | 403 | 404 | 422 | 429);
            if rejected {
                restore_unambiguous_upload(key, file_id, owned_upload);
            }
            let err = common::log_soniox_http_body(response, "Soniox create transcription").await;
            if status == reqwest::StatusCode::TOO_MANY_REQUESTS
                && matches!(err, common::SttError::RateLimited)
                && attempt == 0
            {
                tokio::time::sleep(std::time::Duration::from_millis(400)).await;
                continue;
            }
            return Err(err);
        }
        let json: serde_json::Value = response
            .json()
            .await
            .map_err(|_| common::SttError::BadResponse)?;
        let id = json
            .get("id")
            .and_then(|value| value.as_str())
            .filter(|id| !id.is_empty())
            .ok_or(common::SttError::BadResponse)?;
        restore_unambiguous_upload(key, file_id, owned_upload);
        attach_transcription(file_id, id, key);
        return Ok(id.to_string());
    }
    unreachable!("create returns after at most two definitive throttle rejections")
}

fn restore_unambiguous_upload(key: &str, file_id: &str, owned_upload: bool) {
    AMBIGUOUS_UPLOADS
        .lock()
        .expect("ambiguous uploads poisoned")
        .remove(&(ownership_scope(key), file_id.to_string()));
    if owned_upload {
        remember_owned(key, "files", file_id);
    }
}

/// Steps 2-4 of the typed flow: create transcription, poll to terminal
/// status, extract text. App-free so tests can drive it against wiremock.
/// Returns the transcription id (once created) alongside the outcome so the
/// caller can always clean up the stored records.
async fn run_typed_transcription(
    client: &reqwest::Client,
    key: &str,
    model: &str,
    file_id: &str,
    language: Option<&str>,
    soniox_context: Option<crate::writing::SonioxContext>,
) -> (Option<String>, Result<String, common::SttError>) {
    // 2) Create transcription -> transcription_id
    let payload = build_create_payload(model, file_id, language, soniox_context, false);

    let transcription_id = match create_transcription(client, key, file_id, &payload).await {
        Ok(id) => id,
        Err(err) => return (None, Err(err)),
    };

    let result = async {
        // 3) Poll status
        let status_url = format!("{}/transcriptions/{}", base_url(), transcription_id);
        let started = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(180);
        loop {
            let resp = common::with_retry(|| {
                let client = client.clone();
                let status_url = status_url.clone();
                async move {
                    let resp = client
                        .get(&status_url)
                        .bearer_auth(key)
                        .send()
                        .await
                        .map_err(|e| common::classify_reqwest_err(&e))?;
                    if resp.status().is_success() {
                        Ok(resp)
                    } else {
                        Err(common::log_soniox_http_body(resp, "Soniox status").await)
                    }
                }
            })
            .await?;
            let json: serde_json::Value = resp
                .json()
                .await
                .map_err(|_| common::SttError::BadResponse)?;
            let status = json.get("status").and_then(|v| v.as_str()).unwrap_or("");
            match status {
                "completed" => break,
                "error" => {
                    log::warn!("Soniox transcription job failed");
                    return Err(common::SttError::Server);
                }
                _ => {
                    if started.elapsed() > timeout {
                        return Err(common::SttError::Timeout);
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
                }
            }
        }

        // 4) Fetch transcript
        let transcript_url = format!(
            "{}/transcriptions/{}/transcript",
            base_url(),
            transcription_id
        );
        let resp = common::with_retry(|| {
            let client = client.clone();
            let transcript_url = transcript_url.clone();
            async move {
                let resp = client
                    .get(&transcript_url)
                    .bearer_auth(key)
                    .send()
                    .await
                    .map_err(|e| common::classify_reqwest_err(&e))?;
                if resp.status().is_success() {
                    Ok(resp)
                } else {
                    Err(common::log_soniox_http_body(resp, "Soniox transcript").await)
                }
            }
        })
        .await?;
        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|_| common::SttError::BadResponse)?;

        // Prefer direct text if present, else join tokens
        if let Some(text) = json.get("text").and_then(|v| v.as_str()) {
            return Ok(text.to_string());
        }
        if let Some(tokens) = json.get("tokens").and_then(|v| v.as_array()) {
            let mut out = String::new();
            let mut first = true;
            for t in tokens {
                if let Some(txt) = t.get("text").and_then(|v| v.as_str()) {
                    if !first {
                        out.push(' ');
                    } else {
                        first = false;
                    }
                    out.push_str(txt);
                }
            }
            if !out.is_empty() {
                return Ok(out);
            }
        }
        Err(common::SttError::BadResponse)
    }
    .await;

    (Some(transcription_id), result)
}

pub(super) async fn transcribe_typed_diarized(
    app: &AppHandle,
    key: &str,
    model: &str,
    wav_path: &Path,
    language: Option<&str>,
) -> Result<super::CloudTranscript, common::SttError> {
    use tokio::fs;

    let wav_bytes = fs::read(wav_path)
        .await
        .map_err(|_| common::SttError::BadResponse)?;
    let client = common::http_client();
    let soniox_context = load_soniox_context(app, language);

    let result = transcribe_diarized_with_autoheal(
        &client,
        key,
        model,
        wav_path,
        wav_bytes,
        language,
        soniox_context,
    )
    .await;
    if matches!(result, Err(common::SttError::LimitExceeded { .. })) {
        notify_storage_limit(app).await;
    }
    result
}

/// Diarized twin of [`transcribe_typed_with_autoheal`] — same storage-limit
/// self-heal: background cleanup waits for the first FILE deletion, then
/// one full restart from upload with the fresh upload registered active.
async fn transcribe_diarized_with_autoheal(
    client: &reqwest::Client,
    key: &str,
    model: &str,
    wav_path: &Path,
    wav_bytes: Vec<u8>,
    language: Option<&str>,
    soniox_context: Option<crate::writing::SonioxContext>,
) -> Result<super::CloudTranscript, common::SttError> {
    const LIMIT_ATTEMPTS: usize = 2;
    for attempt in 1..=LIMIT_ATTEMPTS {
        match attempt_diarized_once(
            client,
            key,
            model,
            wav_path,
            &wav_bytes,
            language,
            soniox_context.clone(),
        )
        .await
        {
            Ok(transcript) => return Ok(transcript),
            Err(common::SttError::LimitExceeded { file_storage }) if attempt < LIMIT_ATTEMPTS => {
                // Baselines BEFORE spawn (mirrors the typed flow).
                let (files_baseline, records_baseline) = auto_cleanup_progress_baselines(key);
                spawn_auto_cleanup(client.clone(), key.to_string());
                wait_for_cleanup_progress(
                    key,
                    file_storage,
                    files_baseline,
                    records_baseline,
                    std::time::Duration::from_secs(8),
                )
                .await;
            }
            Err(e) => return Err(e),
        }
    }
    unreachable!("attempt loop always returns within LIMIT_ATTEMPTS")
}

/// One full diarized attempt: upload + create/poll/extract + record
/// cleanup. The uploaded file is registered as an active upload until
/// cleanup finishes.
async fn attempt_diarized_once(
    client: &reqwest::Client,
    key: &str,
    model: &str,
    wav_path: &Path,
    wav_bytes: &[u8],
    language: Option<&str>,
    soniox_context: Option<crate::writing::SonioxContext>,
) -> Result<super::CloudTranscript, common::SttError> {
    use reqwest::multipart::{Form, Part};

    // 1) Upload file -> file_id
    let filename = wav_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("audio.wav")
        .to_string();
    let upload_url = format!("{}/files", base_url());
    // Gate window 1 (mirrors the typed flow): upload POST until the file id
    // is registered — a drain listing cannot interleave with the
    // list-visible-before-registered gap.
    let coordinator = cleanup_coordinator(key);
    let _upload_gate = coordinator.listing_gate.read().await;
    let upload_resp = common::with_retry(|| {
        let client = client.clone();
        let filename = filename.clone();
        let upload_url = upload_url.clone();
        let wav_bytes = wav_bytes.to_vec();
        async move {
            let file_part = Part::bytes(wav_bytes)
                .file_name(filename)
                .mime_str("audio/wav")
                .map_err(|_| common::SttError::BadResponse)?;
            let form = Form::new().part("file", file_part);
            let resp = client
                .post(&upload_url)
                .bearer_auth(key)
                .multipart(form)
                .send()
                .await
                .map_err(|e| common::classify_reqwest_err(&e))?;
            if resp.status().is_success() {
                Ok(resp)
            } else {
                Err(common::log_soniox_http_body(resp, "Soniox upload (diarized)").await)
            }
        }
    })
    .await?;
    let upload_json: serde_json::Value = upload_resp
        .json()
        .await
        .map_err(|_| common::SttError::BadResponse)?;
    let file_id = upload_json
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or(common::SttError::BadResponse)?
        .to_string();
    // Own the upload for the rest of this attempt (mirrors the typed flow):
    // drains must never see it as unreferenced backlog, and dropping the
    // guard on return/cancellation cannot leak the protection.
    let _active_file = ActiveJobGuard::register(&file_id, key);
    drop(_upload_gate);

    let (transcription_id, result) =
        run_diarized_transcription(client, key, model, &file_id, language, soniox_context).await;

    spawn_terminal_cleanup(client, key, transcription_id, file_id, _active_file);
    result
}

/// Steps 2-4 of the diarized flow; mirrors [`run_typed_transcription`] but
/// requests speaker diarization and extracts per-word speaker data.
async fn run_diarized_transcription(
    client: &reqwest::Client,
    key: &str,
    model: &str,
    file_id: &str,
    language: Option<&str>,
    soniox_context: Option<crate::writing::SonioxContext>,
) -> (
    Option<String>,
    Result<super::CloudTranscript, common::SttError>,
) {
    // 2) Create transcription with diarization -> transcription_id
    let payload = build_create_payload(model, file_id, language, soniox_context, true);

    let transcription_id = match create_transcription(client, key, file_id, &payload).await {
        Ok(id) => id,
        Err(err) => return (None, Err(err)),
    };

    let result = async {
        // 3) Poll status
        let status_url = format!("{}/transcriptions/{}", base_url(), transcription_id);
        let started = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(180);
        loop {
            let resp = common::with_retry(|| {
                let client = client.clone();
                let status_url = status_url.clone();
                async move {
                    let resp = client
                        .get(&status_url)
                        .bearer_auth(key)
                        .send()
                        .await
                        .map_err(|e| common::classify_reqwest_err(&e))?;
                    if resp.status().is_success() {
                        Ok(resp)
                    } else {
                        Err(common::log_soniox_http_body(resp, "Soniox status (diarized)").await)
                    }
                }
            })
            .await?;
            let json: serde_json::Value = resp
                .json()
                .await
                .map_err(|_| common::SttError::BadResponse)?;
            let status = json.get("status").and_then(|v| v.as_str()).unwrap_or("");
            match status {
                "completed" => break,
                "error" => {
                    log::warn!("Soniox diarized transcription job failed");
                    return Err(common::SttError::Server);
                }
                _ => {
                    if started.elapsed() > timeout {
                        return Err(common::SttError::Timeout);
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
                }
            }
        }

        // 4) Fetch transcript
        let transcript_url = format!(
            "{}/transcriptions/{}/transcript",
            base_url(),
            transcription_id
        );
        let resp = common::with_retry(|| {
            let client = client.clone();
            let transcript_url = transcript_url.clone();
            async move {
                let resp = client
                    .get(&transcript_url)
                    .bearer_auth(key)
                    .send()
                    .await
                    .map_err(|e| common::classify_reqwest_err(&e))?;
                if resp.status().is_success() {
                    Ok(resp)
                } else {
                    Err(common::log_soniox_http_body(resp, "Soniox transcript (diarized)").await)
                }
            }
        })
        .await?;
        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|_| common::SttError::BadResponse)?;

        // Extract text (prefer top-level `text`, else join tokens)
        let text = json
            .get("text")
            .and_then(|v| v.as_str())
            .map(String::from)
            .or_else(|| {
                json.get("tokens").and_then(|v| v.as_array()).map(|tokens| {
                    tokens
                        .iter()
                        .filter_map(|t| t.get("text").and_then(|v| v.as_str()))
                        .collect::<Vec<_>>()
                        .join(" ")
                })
            })
            .filter(|s| !s.is_empty())
            .ok_or(common::SttError::BadResponse)?;

        // Parse per-word speaker data from tokens
        let words = json
            .get("tokens")
            .and_then(|v| v.as_array())
            .map(|tokens| tokens.iter().filter_map(parse_soniox_token).collect())
            .unwrap_or_default();

        Ok(super::CloudTranscript { text, words })
    }
    .await;

    (Some(transcription_id), result)
}

fn parse_soniox_token(t: &serde_json::Value) -> Option<crate::transcription::TranscriptionWord> {
    let text = t.get("text").and_then(|v| v.as_str())?.to_string();
    let start_ms = t
        .get("start_ms")
        .and_then(|v| v.as_i64())
        .map(|ms| ms as u64);
    let end_ms = t.get("end_ms").and_then(|v| v.as_i64()).map(|ms| ms as u64);
    let speaker_id = t.get("speaker").and_then(|v| {
        v.as_i64()
            .map(|n| format!("Speaker {n}"))
            .or_else(|| v.as_str().map(|s| format!("Speaker {s}")))
    });
    Some(crate::transcription::TranscriptionWord {
        text,
        start_ms,
        end_ms,
        speaker_id,
        confidence: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::writing::{SonioxContext, SonioxContextField};

    #[test]
    fn create_payload_includes_language_and_structured_context() {
        let payload = build_create_payload(
            "stt-async-v5",
            "file_123",
            Some(" en "),
            Some(SonioxContext {
                general: vec![SonioxContextField {
                    key: "domain".to_string(),
                    value: "Software".to_string(),
                }],
                terms: vec!["Voicetypr".to_string(), "Tauri".to_string()],
                text: Some(
                    "Spoken forms map to canonical spellings: voice typer -> Voicetypr."
                        .to_string(),
                ),
            }),
            false,
        );

        assert_eq!(payload["model"].as_str(), Some("stt-async-v5"));
        assert_eq!(payload["file_id"].as_str(), Some("file_123"));
        assert_eq!(
            payload["language_hints"].as_array().unwrap()[0].as_str(),
            Some("en")
        );
        assert_eq!(
            payload["context"]["terms"].as_array().unwrap()[0].as_str(),
            Some("Voicetypr")
        );
        assert_eq!(
            payload["context"]["text"].as_str(),
            Some("Spoken forms map to canonical spellings: voice typer -> Voicetypr.")
        );
    }

    #[test]
    fn create_payload_omits_empty_optional_fields() {
        let payload = build_create_payload(
            "stt-async-v5",
            "file_123",
            Some(" "),
            Some(SonioxContext {
                general: Vec::new(),
                terms: Vec::new(),
                text: None,
            }),
            false,
        );

        assert_eq!(payload["model"].as_str(), Some("stt-async-v5"));
        assert!(payload.get("language_hints").is_none());
        assert!(payload.get("context").is_none());
    }

    #[test]
    fn build_create_payload_diarize_flag_sets_field() {
        let payload = build_create_payload("stt-async-v5", "fid", None, None, true);
        assert_eq!(payload["enable_speaker_diarization"].as_bool(), Some(true));
        let payload_no_diarize = build_create_payload("stt-async-v5", "fid", None, None, false);
        assert!(payload_no_diarize
            .get("enable_speaker_diarization")
            .is_none());
    }

    #[test]
    fn create_payload_uses_selected_model() {
        let payload = build_create_payload("custom-model", "fid", None, None, false);
        assert_eq!(payload["model"].as_str(), Some("custom-model"));
    }
    #[test]
    fn parse_soniox_token_with_speaker_produces_speaker_id() {
        let t = serde_json::json!({
            "text": "Hello",
            "start_ms": 0,
            "end_ms": 500,
            "speaker": 0
        });
        let word = parse_soniox_token(&t).unwrap();
        assert_eq!(word.text, "Hello");
        assert_eq!(word.start_ms, Some(0));
        assert_eq!(word.end_ms, Some(500));
        assert_eq!(word.speaker_id, Some("Speaker 0".to_string()));
        assert!(word.confidence.is_none());
    }

    #[test]
    fn parse_soniox_token_without_speaker_gives_none_speaker_id() {
        let t = serde_json::json!({
            "text": "world",
            "start_ms": 600,
            "end_ms": 900
        });
        let word = parse_soniox_token(&t).unwrap();
        assert_eq!(word.text, "world");
        assert_eq!(word.speaker_id, None);
    }

    #[test]
    fn parse_soniox_token_string_speaker_is_prefixed() {
        let t = serde_json::json!({
            "text": "yes",
            "start_ms": 100,
            "end_ms": 200,
            "speaker": "A"
        });
        let word = parse_soniox_token(&t).unwrap();
        assert_eq!(word.speaker_id, Some("Speaker A".to_string()));
    }

    #[test]
    fn parse_soniox_token_missing_text_returns_none() {
        let t = serde_json::json!({ "start_ms": 0, "end_ms": 100 });
        assert!(parse_soniox_token(&t).is_none());
    }
    mod flow {
        use super::*;
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        // Flow tests share the production cleanup flag, counters and ownership
        // registry even though each mock-server URL is thread-local.
        static FLOW_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

        struct BaseOverrideGuard {
            _lock: tokio::sync::MutexGuard<'static, ()>,
        }

        impl BaseOverrideGuard {
            async fn install(server: &MockServer) -> Self {
                let lock = FLOW_TEST_LOCK.lock().await;
                OWNED_RECORDS.lock().unwrap().clear();
                AMBIGUOUS_UPLOADS.lock().unwrap().clear();
                CLEANUP_COORDINATORS.lock().unwrap().clear();
                set_base_url_override(Some(format!("{}/v1", server.uri())));
                Self { _lock: lock }
            }
        }

        impl Drop for BaseOverrideGuard {
            fn drop(&mut self) {
                set_base_url_override(None);
            }
        }

        fn limit_exceeded_body() -> serde_json::Value {
            serde_json::json!({
                "status_code": 429,
                "error_type": "limit_exceeded",
                "message": "Total file count limit has been exceeded for your organization. Please delete some."
            })
        }

        async fn mount_terminal_backlog(
            server: &MockServer,
            tid: &'static str,
            file: &'static str,
        ) {
            let deleted = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let deleted_for_listing = deleted.clone();
            Mock::given(method("GET"))
                .and(path("/v1/transcriptions"))
                .respond_with(move |_: &wiremock::Request| {
                    let records = if deleted_for_listing.load(std::sync::atomic::Ordering::SeqCst) {
                        serde_json::json!([])
                    } else {
                        serde_json::json!([{"id":tid, "file_id":file}])
                    };
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({"transcriptions":records}))
                })
                .mount(server)
                .await;
            Mock::given(method("DELETE"))
                .and(path(format!("/v1/transcriptions/{tid}")))
                .respond_with(move |_: &wiremock::Request| {
                    deleted.store(true, std::sync::atomic::Ordering::SeqCst);
                    ResponseTemplate::new(204)
                })
                .expect(1)
                .mount(server)
                .await;
        }

        async fn mount_empty_reference_listing(server: &MockServer) {
            Mock::given(method("GET"))
                .and(path("/v1/transcriptions"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({"transcriptions":[]})),
                )
                .mount(server)
                .await;
        }

        async fn wait_for_cleanup() {
            tokio::time::timeout(std::time::Duration::from_secs(5), async {
                while cleanup_coordinator("k")
                    .running
                    .load(std::sync::atomic::Ordering::SeqCst)
                    || !active_file_ids("k").is_empty()
                {
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("background cleanup did not finish");
        }

        #[tokio::test]
        async fn inline_candidate_rechecks_references_created_after_initial_snapshot() {
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server).await;
            remember_owned("k", "transcriptions", "own-job");
            remember_owned("k", "files", "shared-file");
            let own_deleted = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let deleted_for_listing = own_deleted.clone();
            Mock::given(method("GET"))
                .and(path("/v1/transcriptions"))
                .respond_with(move |_: &wiremock::Request| {
                    let id = if deleted_for_listing.load(std::sync::atomic::Ordering::SeqCst) {
                        "new-external-job"
                    } else {
                        "own-job"
                    };
                    ResponseTemplate::new(200).set_body_json(
                        serde_json::json!({"transcriptions":[{"id":id,"file_id":"shared-file"}]}),
                    )
                })
                .mount(&server)
                .await;
            Mock::given(method("DELETE"))
                .and(path("/v1/transcriptions/own-job"))
                .respond_with(move |_: &wiremock::Request| {
                    own_deleted.store(true, std::sync::atomic::Ordering::SeqCst);
                    ResponseTemplate::new(204)
                })
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/v1/files"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({"files":[{"id":"shared-file"}]})),
                )
                .mount(&server)
                .await;
            Mock::given(method("DELETE"))
                .and(path("/v1/files/shared-file"))
                .respond_with(ResponseTemplate::new(204))
                .expect(0)
                .mount(&server)
                .await;
            let result = drain_stored_records(&common::http_client(), "k", None)
                .await
                .unwrap();
            assert_eq!(result.deleted_transcriptions, 1);
            assert_eq!(result.deleted_files, 0);
            assert!(is_owned("k", "files", "shared-file"));
            server.verify().await;
        }

        #[tokio::test]
        async fn file_pass_candidate_rechecks_references_created_after_its_snapshot() {
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server).await;
            remember_owned("k", "files", "candidate-file");
            let files_listed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let files_listed_for_transcriptions = files_listed.clone();
            Mock::given(method("GET"))
                .and(path("/v1/transcriptions"))
                .respond_with(move |_: &wiremock::Request| {
                    let records = if files_listed_for_transcriptions
                        .load(std::sync::atomic::Ordering::SeqCst)
                    {
                        serde_json::json!([{"id":"new-external-job", "file_id":"candidate-file"}])
                    } else {
                        serde_json::json!([])
                    };
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({"transcriptions":records}))
                })
                .expect(3)
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/v1/files"))
                .respond_with(move |_: &wiremock::Request| {
                    files_listed.store(true, std::sync::atomic::Ordering::SeqCst);
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({"files":[{"id":"candidate-file"}]}))
                })
                .mount(&server)
                .await;
            Mock::given(method("DELETE"))
                .and(path("/v1/files/candidate-file"))
                .respond_with(ResponseTemplate::new(204))
                .expect(0)
                .mount(&server)
                .await;
            let result = drain_stored_records(&common::http_client(), "k", None)
                .await
                .unwrap();
            assert_eq!(result.deleted_files, 0);
            assert_eq!(result.skipped_active, 1);
            assert!(is_owned("k", "files", "candidate-file"));
            server.verify().await;
        }

        #[tokio::test]
        async fn terminal_cleanup_preserves_existing_shared_file_after_delete_or_not_found() {
            for status in [204, 404] {
                let server = MockServer::start().await;
                let _guard = BaseOverrideGuard::install(&server).await;
                let client = common::http_client();
                let owner = ActiveJobGuard::register("shared-file", "k");
                attach_transcription("shared-file", "own-job", "k");
                Mock::given(method("DELETE"))
                    .and(path("/v1/transcriptions/own-job"))
                    .respond_with(ResponseTemplate::new(status))
                    .expect(1)
                    .mount(&server)
                    .await;
                Mock::given(method("GET"))
                    .and(path("/v1/transcriptions"))
                    .respond_with(ResponseTemplate::new(200).set_body_json(
                        serde_json::json!({"transcriptions":[
                            {"id":"other-client-job", "file_id":"shared-file"}
                        ]}),
                    ))
                    .expect(1)
                    .mount(&server)
                    .await;
                cleanup_stored_records(&client, "k", Some("own-job"), "shared-file").await;
                assert!(is_owned("k", "files", "shared-file"));
                assert!(active_file_ids("k").contains("shared-file"));
                assert!(server
                    .received_requests()
                    .await
                    .unwrap()
                    .iter()
                    .all(|r| r.url.path() != "/v1/files/shared-file"));
                server.verify().await;
                drop(owner);
                // Once the sibling disappears, the retained upload remains
                // eligible for a normal bounded backlog cleanup.
                server.reset().await;
                mount_empty_reference_listing(&server).await;
                Mock::given(method("GET"))
                    .and(path("/v1/files"))
                    .respond_with(
                        ResponseTemplate::new(200)
                            .set_body_json(serde_json::json!({"files":[{"id":"shared-file"}]})),
                    )
                    .mount(&server)
                    .await;
                Mock::given(method("DELETE"))
                    .and(path("/v1/files/shared-file"))
                    .respond_with(ResponseTemplate::new(204))
                    .expect(1)
                    .mount(&server)
                    .await;
                assert_eq!(
                    drain_stored_records(&client, "k", None)
                        .await
                        .unwrap()
                        .deleted_files,
                    1
                );
                server.verify().await;
            }
        }

        #[tokio::test]
        async fn terminal_reference_lookup_fails_closed_on_missing_metadata_or_http_failure() {
            for response in [
                ResponseTemplate::new(503),
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"transcriptions":[{"id":"unknown-job"}]})),
            ] {
                let server = MockServer::start().await;
                let _guard = BaseOverrideGuard::install(&server).await;
                remember_owned("k", "files", "retained-file");
                Mock::given(method("DELETE"))
                    .and(path("/v1/transcriptions/own-job"))
                    .respond_with(ResponseTemplate::new(204))
                    .expect(1)
                    .mount(&server)
                    .await;
                Mock::given(method("GET"))
                    .and(path("/v1/transcriptions"))
                    .respond_with(response)
                    .expect(1)
                    .mount(&server)
                    .await;
                cleanup_stored_records(
                    &common::http_client(),
                    "k",
                    Some("own-job"),
                    "retained-file",
                )
                .await;
                assert!(is_owned("k", "files", "retained-file"));
                assert!(server
                    .received_requests()
                    .await
                    .unwrap()
                    .iter()
                    .all(|r| r.url.path() != "/v1/files/retained-file"));
                server.verify().await;
            }
        }

        #[tokio::test]
        async fn terminal_reference_deadline_releases_gate_and_retains_file() {
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server).await;
            remember_owned("k", "files", "retained-file");
            Mock::given(method("DELETE"))
                .and(path("/v1/transcriptions/own-job"))
                .respond_with(ResponseTemplate::new(204))
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/v1/transcriptions"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({"transcriptions":[]}))
                        .set_delay(std::time::Duration::from_secs(2)),
                )
                .expect(1)
                .mount(&server)
                .await;
            assert!(cleanup_stored_records_with_deadline(
                &common::http_client(),
                "k",
                Some("own-job"),
                "retained-file",
                std::time::Duration::from_millis(150)
            )
            .await
            .is_err());
            assert!(cleanup_coordinator("k").listing_gate.try_read().is_ok());
            assert!(is_owned("k", "files", "retained-file"));
            assert!(server
                .received_requests()
                .await
                .unwrap()
                .iter()
                .all(|r| r.url.path() != "/v1/files/retained-file"));
            server.verify().await;
        }

        #[tokio::test]
        async fn slow_old_key_listing_does_not_block_new_key_flow_or_alias_active_ids() {
            use std::sync::atomic::Ordering;
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server).await;
            let client = common::http_client();
            let old_owner = ActiveJobGuard::register("same-file", "old-key");
            attach_transcription("same-file", "same-job", "old-key");
            Mock::given(method("GET"))
                .and(path("/v1/transcriptions"))
                .and(wiremock::matchers::header(
                    "authorization",
                    "Bearer old-key",
                ))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({"transcriptions":[]}))
                        .set_delay(std::time::Duration::from_secs(2)),
                )
                .expect(1)
                .mount(&server)
                .await;
            spawn_auto_cleanup_with_deadline(
                client.clone(),
                "old-key".to_string(),
                std::time::Duration::from_millis(700),
            );
            tokio::time::timeout(std::time::Duration::from_millis(300), async {
                while server.received_requests().await.unwrap().is_empty() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            assert!(cleanup_coordinator("old-key")
                .listing_gate
                .try_read()
                .is_err());
            Mock::given(method("POST"))
                .and(path("/v1/files"))
                .respond_with(
                    ResponseTemplate::new(201).set_body_json(serde_json::json!({"id":"same-file"})),
                )
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("POST"))
                .and(path("/v1/transcriptions"))
                .respond_with(
                    ResponseTemplate::new(201).set_body_json(serde_json::json!({"id":"same-job"})),
                )
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/v1/transcriptions/same-job"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({"status":"completed"})),
                )
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/v1/transcriptions/same-job/transcript"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({"text":"new key works"})),
                )
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/v1/transcriptions"))
                .and(wiremock::matchers::header(
                    "authorization",
                    "Bearer new-key",
                ))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({"transcriptions":[]})),
                )
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("DELETE"))
                .and(wiremock::matchers::header(
                    "authorization",
                    "Bearer new-key",
                ))
                .respond_with(ResponseTemplate::new(204))
                .expect(2)
                .mount(&server)
                .await;
            let result = tokio::time::timeout(
                std::time::Duration::from_millis(300),
                attempt_typed_once(
                    &client,
                    "new-key",
                    "stt-async-v5",
                    Path::new("audio.wav"),
                    b"audio",
                    None,
                    None,
                ),
            )
            .await
            .expect("old key listing blocked a new key upload/create")
            .unwrap();
            assert_eq!(result, "new key works");
            tokio::time::timeout(std::time::Duration::from_millis(300), async {
                while !active_file_ids("new-key").is_empty() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            assert!(active_file_ids("old-key").contains("same-file"));
            assert!(active_transcription_ids("old-key").contains("same-job"));
            drop(old_owner);
            tokio::time::timeout(std::time::Duration::from_secs(2), async {
                while cleanup_coordinator("old-key")
                    .running
                    .load(Ordering::SeqCst)
                {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            server.verify().await;
        }

        #[tokio::test]
        async fn management_requests_bound_stalled_count_listing_and_delete() {
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server).await;
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap();
            let timeout = std::time::Duration::from_millis(100);
            Mock::given(method("GET"))
                .and(path("/v1/files/count"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({"total":1}))
                        .set_delay(std::time::Duration::from_secs(2)),
                )
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/v1/files"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({"files":[]}))
                        .set_delay(std::time::Duration::from_secs(2)),
                )
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("DELETE"))
                .and(path("/v1/files/slow"))
                .respond_with(
                    ResponseTemplate::new(204).set_delay(std::time::Duration::from_secs(2)),
                )
                .expect(1)
                .mount(&server)
                .await;
            let results = tokio::time::timeout(std::time::Duration::from_secs(1), async {
                let count = get_total_with_timeout(&client, "k", "files", timeout).await;
                let list =
                    list_pages_with_timeout::<ListedFile>(&client, "k", "files", timeout).await;
                let delete = delete_one_with_timeout(
                    &client,
                    "k",
                    &format!("{}/files/slow", base_url()),
                    timeout,
                )
                .await;
                (count, list, delete)
            })
            .await
            .expect("management must not inherit the long transcription timeout");
            assert!(matches!(results.0, Err(common::SttError::Timeout)));
            assert!(matches!(results.1, Err(common::SttError::Timeout)));
            assert!(results.2.is_err());
            server.verify().await;
        }

        #[tokio::test]
        async fn malformed_storage_counts_do_not_report_empty_storage() {
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server).await;
            for payload in [
                serde_json::json!({}),
                serde_json::json!({"total":"0"}),
                serde_json::json!({"total":-1}),
                serde_json::json!({"total":1.5}),
            ] {
                server.reset().await;
                Mock::given(method("GET"))
                    .and(path("/v1/files/count"))
                    .respond_with(ResponseTemplate::new(200).set_body_json(payload))
                    .mount(&server)
                    .await;
                assert!(matches!(
                    get_total(&common::http_client(), "k", "files").await,
                    Err(common::SttError::BadResponse)
                ));
            }
        }

        #[tokio::test]
        async fn manual_cleanup_excludes_same_scope_but_not_other_accounts() {
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server).await;
            let client = common::http_client();
            let first_manual = try_acquire_cleanup("k").expect("first manual cleanup starts");

            let busy = run_manual_cleanup_with_deadline(
                &client,
                "k",
                None,
                std::time::Duration::from_secs(1),
            )
            .await
            .unwrap_err();
            assert_eq!(busy, CLEANUP_ALREADY_RUNNING_MESSAGE);
            assert!(
                try_acquire_cleanup("k").is_none(),
                "automatic cleanup must use the same occupied coordinator"
            );
            let other_account =
                try_acquire_cleanup("other-key").expect("different API key stays isolated");

            drop(other_account);
            drop(first_manual);
            assert!(
                try_acquire_cleanup("k").is_some(),
                "successful guard drop must allow retry"
            );
        }

        #[tokio::test]
        async fn manual_cleanup_error_releases_running_guard() {
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server).await;
            Mock::given(method("GET"))
                .and(path("/v1/transcriptions"))
                .respond_with(ResponseTemplate::new(500))
                .expect(1)
                .mount(&server)
                .await;

            let result = run_manual_cleanup_with_deadline(
                &common::http_client(),
                "k",
                None,
                std::time::Duration::from_secs(1),
            )
            .await;
            assert!(result.is_err());
            assert!(
                try_acquire_cleanup("k").is_some(),
                "error path must allow retry"
            );
            server.verify().await;
        }

        #[tokio::test]
        async fn cleanup_deadline_reports_possible_partial_work_and_can_be_retried() {
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server).await;
            let client = common::http_client();
            remember_owned("k", "transcriptions", "fast");
            remember_owned("k", "transcriptions", "slow");
            Mock::given(method("GET"))
                .and(path("/v1/transcriptions"))
                .respond_with(ResponseTemplate::new(200).set_body_json(
                    serde_json::json!({"transcriptions":[
                        {"id":"fast", "file_id":null}, {"id":"slow", "file_id":null}
                    ]}),
                ))
                .mount(&server)
                .await;
            Mock::given(method("DELETE"))
                .and(path("/v1/transcriptions/fast"))
                .respond_with(ResponseTemplate::new(204))
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("DELETE"))
                .and(path("/v1/transcriptions/slow"))
                .respond_with(
                    ResponseTemplate::new(204).set_delay(std::time::Duration::from_secs(2)),
                )
                .expect(1)
                .mount(&server)
                .await;
            let error = run_manual_cleanup_with_deadline(
                &client,
                "k",
                None,
                std::time::Duration::from_millis(200),
            )
            .await
            .unwrap_err();
            assert_eq!(error, CLEANUP_TIMEOUT_MESSAGE);
            assert!(!is_owned("k", "transcriptions", "fast"));
            assert!(is_owned("k", "transcriptions", "slow"));
            server.verify().await;
            server.reset().await;
            Mock::given(method("GET"))
                .and(path("/v1/transcriptions"))
                .respond_with(ResponseTemplate::new(200).set_body_json(
                    serde_json::json!({"transcriptions":[{"id":"slow","file_id":null}]}),
                ))
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/v1/files"))
                .respond_with(
                    ResponseTemplate::new(200).set_body_json(serde_json::json!({"files":[]})),
                )
                .mount(&server)
                .await;
            // A timed-out delete may have landed remotely; 404 is a safe retry.
            Mock::given(method("DELETE"))
                .and(path("/v1/transcriptions/slow"))
                .respond_with(ResponseTemplate::new(404))
                .expect(1)
                .mount(&server)
                .await;
            let retried = run_manual_cleanup_with_deadline(
                &client,
                "k",
                None,
                std::time::Duration::from_secs(1),
            )
            .await
            .unwrap();
            assert!(retried.errors.is_empty());
            assert!(!is_owned("k", "transcriptions", "slow"));
            server.verify().await;
        }

        #[tokio::test]
        async fn timed_out_background_listing_releases_gate_and_running_state_for_retry() {
            use std::sync::atomic::Ordering;
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server).await;
            let client = common::http_client();
            Mock::given(method("GET"))
                .and(path("/v1/transcriptions"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({"transcriptions":[]}))
                        .set_delay(std::time::Duration::from_secs(2)),
                )
                .expect(1)
                .mount(&server)
                .await;
            spawn_auto_cleanup_with_deadline(
                client.clone(),
                "k".to_string(),
                std::time::Duration::from_millis(150),
            );
            assert!(cleanup_coordinator("k").running.load(Ordering::SeqCst));
            wait_for_cleanup().await;
            assert!(!cleanup_coordinator("k").running.load(Ordering::SeqCst));
            assert!(
                cleanup_coordinator("k").listing_gate.try_read().is_ok(),
                "timed-out listing retained its write gate"
            );
            server.verify().await;
            server.reset().await;
            Mock::given(method("GET"))
                .and(path("/v1/transcriptions"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({"transcriptions":[]})),
                )
                .expect(2)
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/v1/files"))
                .respond_with(
                    ResponseTemplate::new(200).set_body_json(serde_json::json!({"files":[]})),
                )
                .expect(1)
                .mount(&server)
                .await;
            spawn_auto_cleanup_with_deadline(
                client,
                "k".to_string(),
                std::time::Duration::from_secs(1),
            );
            wait_for_cleanup().await;
            server.verify().await;
        }

        #[tokio::test]
        async fn ambiguous_create_transport_server_and_cancellation_never_retry_or_delete_upload() {
            for diarized in [false, true] {
                for failure in ["timeout", "server", "cancel"] {
                    let server = MockServer::start().await;
                    let _guard = BaseOverrideGuard::install(&server).await;
                    Mock::given(method("POST"))
                        .and(path("/v1/files"))
                        .respond_with(
                            ResponseTemplate::new(201)
                                .set_body_json(serde_json::json!({"id":"uncertain-file"})),
                        )
                        .mount(&server)
                        .await;
                    let creates = std::sync::atomic::AtomicUsize::new(0);
                    Mock::given(method("POST"))
                        .and(path("/v1/transcriptions"))
                        .respond_with(move |_: &wiremock::Request| {
                            let attempt = creates.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                            if attempt > 0 {
                                // A blind retry would receive a known ID, hiding
                                // the first accepted job. No second POST is safe.
                                return ResponseTemplate::new(201)
                                    .set_body_json(serde_json::json!({"id":"retry-job"}));
                            }
                            if failure == "server" {
                                ResponseTemplate::new(503)
                            } else {
                                ResponseTemplate::new(201)
                                    .set_body_json(serde_json::json!({"id":"accepted-job"}))
                                    .set_delay(std::time::Duration::from_secs(1))
                            }
                        })
                        .expect(1)
                        .mount(&server)
                        .await;
                    let client = reqwest::Client::builder()
                        .timeout(std::time::Duration::from_millis(if failure == "timeout" {
                            250
                        } else {
                            3000
                        }))
                        .build()
                        .unwrap();
                    let outcome = tokio::time::timeout(
                        std::time::Duration::from_millis(if failure == "cancel" {
                            250
                        } else {
                            3000
                        }),
                        async {
                            if diarized {
                                attempt_diarized_once(
                                    &client,
                                    "k",
                                    "stt-async-v5",
                                    Path::new("audio.wav"),
                                    b"audio",
                                    None,
                                    None,
                                )
                                .await
                                .map(|r| r.text)
                            } else {
                                attempt_typed_once(
                                    &client,
                                    "k",
                                    "stt-async-v5",
                                    Path::new("audio.wav"),
                                    b"audio",
                                    None,
                                    None,
                                )
                                .await
                            }
                        },
                    )
                    .await;
                    if failure == "cancel" {
                        assert!(outcome.is_err());
                    } else {
                        let err = outcome.unwrap().unwrap_err();
                        assert!(matches!(
                            (failure, err),
                            ("timeout", common::SttError::Timeout)
                                | ("server", common::SttError::Server)
                        ));
                    }
                    wait_for_cleanup().await;
                    assert!(is_ambiguous_upload("k", "uncertain-file"));
                    assert!(!is_owned("k", "files", "uncertain-file"));
                    server.verify().await;
                    assert!(server
                        .received_requests()
                        .await
                        .unwrap()
                        .iter()
                        .all(|r| r.method.as_str() != "DELETE"));
                    Mock::given(method("GET"))
                        .and(path("/v1/transcriptions"))
                        .respond_with(
                            ResponseTemplate::new(200)
                                .set_body_json(serde_json::json!({"transcriptions":[]})),
                        )
                        .mount(&server)
                        .await;
                    Mock::given(method("GET"))
                        .and(path("/v1/files"))
                        .respond_with(
                            ResponseTemplate::new(200).set_body_json(
                                serde_json::json!({"files":[{"id":"uncertain-file"}]}),
                            ),
                        )
                        .mount(&server)
                        .await;
                    let cleanup = drain_stored_records(&client, "k", None).await.unwrap();
                    assert_eq!(cleanup.deleted_files, 0);
                    assert!(server
                        .received_requests()
                        .await
                        .unwrap()
                        .iter()
                        .all(|r| r.method.as_str() != "DELETE"));
                }
            }
        }

        #[tokio::test]
        async fn definitive_create_auth_and_quota_rejections_still_clean_orphans_in_both_flows() {
            for diarized in [false, true] {
                for status in [401, 429] {
                    let server = MockServer::start().await;
                    let _guard = BaseOverrideGuard::install(&server).await;
                    mount_empty_reference_listing(&server).await;
                    Mock::given(method("POST"))
                        .and(path("/v1/files"))
                        .respond_with(
                            ResponseTemplate::new(201)
                                .set_body_json(serde_json::json!({"id":"rejected-file"})),
                        )
                        .mount(&server)
                        .await;
                    Mock::given(method("POST"))
                        .and(path("/v1/transcriptions"))
                        .respond_with(
                            ResponseTemplate::new(status).set_body_json(limit_exceeded_body()),
                        )
                        .expect(1)
                        .mount(&server)
                        .await;
                    Mock::given(method("DELETE"))
                        .and(path("/v1/files/rejected-file"))
                        .respond_with(ResponseTemplate::new(204))
                        .expect(1)
                        .mount(&server)
                        .await;
                    let client = common::http_client();
                    let result = if diarized {
                        attempt_diarized_once(
                            &client,
                            "k",
                            "stt-async-v5",
                            Path::new("audio.wav"),
                            b"audio",
                            None,
                            None,
                        )
                        .await
                        .map(|r| r.text)
                    } else {
                        attempt_typed_once(
                            &client,
                            "k",
                            "stt-async-v5",
                            Path::new("audio.wav"),
                            b"audio",
                            None,
                            None,
                        )
                        .await
                    };
                    assert!(matches!(
                        (status, result),
                        (401, Err(common::SttError::Auth))
                            | (429, Err(common::SttError::LimitExceeded { .. }))
                    ));
                    wait_for_cleanup().await;
                    assert!(!is_ambiguous_upload("k", "rejected-file"));
                    server.verify().await;
                }
            }
        }

        #[tokio::test]
        async fn definitive_create_throttle_retries_once_without_losing_cleanup_ownership() {
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server).await;
            let creates = std::sync::atomic::AtomicUsize::new(0);
            Mock::given(method("POST")).and(path("/v1/transcriptions"))
                .respond_with(move |_: &wiremock::Request| {
                    if creates.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                        ResponseTemplate::new(429).set_body_json(serde_json::json!({
                            "error_type":"limit_exceeded", "message":"Requests per minute limit exceeded"
                        }))
                    } else {
                        ResponseTemplate::new(201).set_body_json(serde_json::json!({"id":"accepted"}))
                    }
                }).expect(2).mount(&server).await;
            let owner = ActiveJobGuard::register("throttled-file", "k");
            let payload = build_create_payload("stt-async-v5", "throttled-file", None, None, false);
            let id = create_transcription(&common::http_client(), "k", "throttled-file", &payload)
                .await
                .unwrap();
            assert_eq!(id, "accepted");
            assert!(!is_ambiguous_upload("k", "throttled-file"));
            assert!(is_owned("k", "files", "throttled-file"));
            assert!(is_owned("k", "transcriptions", "accepted"));
            drop(owner);
            server.verify().await;
        }

        #[tokio::test]
        async fn accepted_create_without_usable_id_preserves_upload_in_both_flows_and_later_drains()
        {
            for diarized in [false, true] {
                for body in ["not-json", r#"{"status":"queued"}"#, r#"{"id":""}"#] {
                    let server = MockServer::start().await;
                    let _guard = BaseOverrideGuard::install(&server).await;
                    Mock::given(method("POST"))
                        .and(path("/v1/files"))
                        .respond_with(
                            ResponseTemplate::new(201)
                                .set_body_json(serde_json::json!({"id":"ambiguous-file"})),
                        )
                        .mount(&server)
                        .await;
                    Mock::given(method("POST"))
                        .and(path("/v1/transcriptions"))
                        .respond_with(ResponseTemplate::new(201).set_body_string(body))
                        .mount(&server)
                        .await;
                    let client = common::http_client();
                    let result = if diarized {
                        attempt_diarized_once(
                            &client,
                            "k",
                            "stt-async-v5",
                            Path::new("audio.wav"),
                            b"audio",
                            None,
                            None,
                        )
                        .await
                        .map(|r| r.text)
                    } else {
                        attempt_typed_once(
                            &client,
                            "k",
                            "stt-async-v5",
                            Path::new("audio.wav"),
                            b"audio",
                            None,
                            None,
                        )
                        .await
                    };
                    assert!(matches!(result, Err(common::SttError::BadResponse)));
                    wait_for_cleanup().await;
                    assert!(is_ambiguous_upload("k", "ambiguous-file"));
                    assert!(!is_ambiguous_upload("other-key", "ambiguous-file"));
                    assert!(!is_owned("k", "files", "ambiguous-file"));
                    assert!(server
                        .received_requests()
                        .await
                        .unwrap()
                        .iter()
                        .all(|r| r.method.as_str() != "DELETE"));
                    // Missing job, complete unknown job, and incomplete metadata
                    // must all preserve the upload after its active guard ends.
                    for records in [
                        serde_json::json!([]),
                        serde_json::json!([
                            {"id":"accepted-server-job", "file_id":"ambiguous-file"}
                        ]),
                        serde_json::json!([{"id":"accepted-server-job"}]),
                    ] {
                        server.reset().await;
                        Mock::given(method("GET"))
                            .and(path("/v1/transcriptions"))
                            .respond_with(
                                ResponseTemplate::new(200)
                                    .set_body_json(serde_json::json!({"transcriptions":records})),
                            )
                            .mount(&server)
                            .await;
                        Mock::given(method("GET"))
                            .and(path("/v1/files"))
                            .respond_with(ResponseTemplate::new(200).set_body_json(
                                serde_json::json!({"files":[{"id":"ambiguous-file"}]}),
                            ))
                            .mount(&server)
                            .await;
                        let cleanup = drain_stored_records(&client, "k", None).await.unwrap();
                        assert_eq!(cleanup.deleted_files, 0);
                        assert_eq!(cleanup.deleted_transcriptions, 0);
                        assert!(server
                            .received_requests()
                            .await
                            .unwrap()
                            .iter()
                            .all(|r| r.method.as_str() != "DELETE"));
                    }
                }
            }
        }

        #[tokio::test]
        async fn overlapping_keys_start_independent_drains_and_only_own_progress_wakes_waiters() {
            use std::sync::atomic::Ordering;
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server).await;
            let client = common::http_client();
            for (key, tid, delay) in [("old-key", "old-job", 300), ("new-key", "new-job", 1500)] {
                remember_owned(key, "transcriptions", tid);
                Mock::given(method("GET"))
                    .and(path("/v1/transcriptions"))
                    .and(wiremock::matchers::header(
                        "authorization",
                        format!("Bearer {key}"),
                    ))
                    .respond_with(ResponseTemplate::new(200).set_body_json(
                        serde_json::json!({"transcriptions":[{"id":tid,"file_id":null}]}),
                    ))
                    .mount(&server)
                    .await;
                Mock::given(method("DELETE"))
                    .and(path(format!("/v1/transcriptions/{tid}")))
                    .and(wiremock::matchers::header(
                        "authorization",
                        format!("Bearer {key}"),
                    ))
                    .respond_with(
                        ResponseTemplate::new(204)
                            .set_delay(std::time::Duration::from_millis(delay)),
                    )
                    .expect(1)
                    .mount(&server)
                    .await;
            }
            Mock::given(method("GET"))
                .and(path("/v1/files"))
                .respond_with(
                    ResponseTemplate::new(200).set_body_json(serde_json::json!({"files":[]})),
                )
                .mount(&server)
                .await;
            let (new_files, new_records) = auto_cleanup_progress_baselines("new-key");
            spawn_auto_cleanup(client.clone(), "old-key".to_string());
            spawn_auto_cleanup(client.clone(), "new-key".to_string());
            // A duplicate within one key is suppressed, not the other key.
            spawn_auto_cleanup(client, "new-key".to_string());
            assert!(cleanup_coordinator("old-key")
                .running
                .load(Ordering::SeqCst));
            assert!(cleanup_coordinator("new-key")
                .running
                .load(Ordering::SeqCst));
            assert!(
                tokio::time::timeout(
                    std::time::Duration::from_millis(800),
                    wait_for_cleanup_progress(
                        "new-key",
                        false,
                        new_files,
                        new_records,
                        std::time::Duration::from_secs(4)
                    )
                )
                .await
                .is_err(),
                "old key completion or freed capacity must not wake new key"
            );
            assert!(
                cleanup_coordinator("old-key")
                    .records_freed
                    .load(Ordering::SeqCst)
                    > 0
            );
            assert_eq!(
                auto_cleanup_progress_baselines("new-key"),
                (new_files, new_records)
            );
            wait_for_cleanup_progress(
                "new-key",
                false,
                new_files,
                new_records,
                std::time::Duration::from_secs(4),
            )
            .await;
            assert_eq!(
                auto_cleanup_progress_baselines("new-key"),
                (new_files, new_records + 1)
            );
            tokio::time::timeout(std::time::Duration::from_secs(2), async {
                while cleanup_coordinator("new-key")
                    .running
                    .load(Ordering::SeqCst)
                {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            server.verify().await;
        }

        #[tokio::test]
        async fn file_waiter_requires_same_scope_file_capacity() {
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server).await;
            let (files, records) = auto_cleanup_progress_baselines("new-key");
            let _running = CleanupRunningGuard::acquire(cleanup_coordinator("new-key")).unwrap();
            bump_auto_cleanup_progress("old-key");
            bump_record_freed_progress("new-key");
            assert!(tokio::time::timeout(
                std::time::Duration::from_millis(300),
                wait_for_cleanup_progress(
                    "new-key",
                    true,
                    files,
                    records,
                    std::time::Duration::from_secs(2)
                )
            )
            .await
            .is_err());
            bump_auto_cleanup_progress("new-key");
            tokio::time::timeout(
                std::time::Duration::from_millis(100),
                wait_for_cleanup_progress(
                    "new-key",
                    true,
                    files,
                    records,
                    std::time::Duration::from_secs(2),
                ),
            )
            .await
            .unwrap();
        }

        #[tokio::test]
        async fn cleanup_guard_resets_on_cancellation_and_panic() {
            use std::sync::atomic::Ordering;
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server).await;
            let coordinator = cleanup_coordinator("k");
            let guard = try_acquire_cleanup("k").unwrap();
            let task = tokio::spawn(async move {
                let _guard = guard;
                std::future::pending::<()>().await;
            });
            task.abort();
            assert!(task.await.unwrap_err().is_cancelled());
            assert!(!coordinator.running.load(Ordering::SeqCst));
            let guard = try_acquire_cleanup("k").unwrap();
            let task = tokio::spawn(async move {
                let _guard = guard;
                panic!("test cleanup panic");
            });
            assert!(task.await.unwrap_err().is_panic());
            assert!(!coordinator.running.load(Ordering::SeqCst));
        }

        #[tokio::test]
        async fn drain_preserves_unknown_and_other_key_records_and_shared_files() {
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server).await;
            let client = common::http_client();
            remember_owned("k", "transcriptions", "owned");
            remember_owned("other-key", "transcriptions", "other-client");
            remember_owned("other-key", "files", "other-file");
            remember_owned("k", "files", "orphan");
            Mock::given(method("GET"))
                .and(path("/v1/transcriptions"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "transcriptions": [
                        {"id":"owned", "file_id":"shared-unknown"},
                        {"id":"other-client", "file_id":"other-file"},
                        {"id":"unknown", "file_id":"unknown-file"}
                    ]
                })))
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/v1/files"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "files": [{"id":"shared-unknown"}, {"id":"other-file"},
                              {"id":"unknown-file"}, {"id":"orphan"}]
                })))
                .mount(&server)
                .await;
            Mock::given(method("DELETE"))
                .respond_with(ResponseTemplate::new(204))
                .mount(&server)
                .await;
            let progress = std::sync::Mutex::new(Vec::new());
            let report = |done, total| progress.lock().unwrap().push((done, total));
            let result = drain_stored_records(&client, "k", Some(&report))
                .await
                .unwrap();
            {
                let updates = progress.lock().unwrap();
                assert_eq!(updates.last(), Some(&(7, 7)));
                assert!(updates
                    .windows(2)
                    .all(|pair| pair[0].0 <= pair[1].0 && pair[0].1 <= pair[1].1));
            }
            assert_eq!(result.deleted_transcriptions, 1);
            assert_eq!(result.deleted_files, 1);
            assert_eq!(result.skipped_unknown, 5);
            let deletes: Vec<_> = server
                .received_requests()
                .await
                .unwrap()
                .into_iter()
                .filter(|r| r.method.as_str() == "DELETE")
                .map(|r| r.url.path().to_string())
                .collect();
            assert_eq!(deletes, ["/v1/transcriptions/owned", "/v1/files/orphan"]);
            assert!(!is_owned("k", "files", "orphan"));
            assert!(!is_owned("k", "transcriptions", "owned"));
        }

        #[tokio::test]
        async fn failed_deletion_retains_ownership_and_not_found_forgets_it() {
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server).await;
            remember_owned("k", "files", "retry");
            let client = common::http_client();
            let url = format!("{}/files/retry", base_url());
            Mock::given(method("DELETE"))
                .respond_with(ResponseTemplate::new(503))
                .mount(&server)
                .await;
            assert!(delete_one(&client, "k", &url).await.is_err());
            assert!(is_owned("k", "files", "retry"));
            server.reset().await;
            Mock::given(method("DELETE"))
                .respond_with(ResponseTemplate::new(404))
                .mount(&server)
                .await;
            assert!(matches!(
                delete_one(&client, "k", &url).await,
                Ok(DeleteOutcome::AlreadyGone)
            ));
            assert!(!is_owned("k", "files", "retry"));
        }

        #[tokio::test]
        async fn pagination_encodes_cursor_and_rejects_repeated_empty_pages_before_deleting() {
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server).await;
            remember_owned("k", "transcriptions", "owned");
            let cursor = "next&cursor=wrong+/#?";
            Mock::given(method("GET"))
                .and(path("/v1/transcriptions"))
                .and(wiremock::matchers::query_param_is_missing("cursor"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "transcriptions":[{"id":"owned","file_id":null}], "next_page_cursor":cursor
                })))
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/v1/transcriptions"))
                .and(wiremock::matchers::query_param("cursor", cursor))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "transcriptions":[], "next_page_cursor":cursor
                })))
                .expect(1)
                .mount(&server)
                .await;
            assert!(drain_stored_records(&common::http_client(), "k", None)
                .await
                .is_err());
            assert!(server
                .received_requests()
                .await
                .unwrap()
                .iter()
                .all(|r| r.method.as_str() != "DELETE"));
            server.verify().await;
        }

        #[tokio::test]
        async fn pagination_has_page_budget_even_for_unique_empty_pages() {
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server).await;
            let pages = std::sync::atomic::AtomicUsize::new(0);
            Mock::given(method("GET"))
                .respond_with(move |_: &wiremock::Request| {
                    let page = pages.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    ResponseTemplate::new(200).set_body_json(serde_json::json!({
                        "transcriptions":[], "next_page_cursor":page.to_string()
                    }))
                })
                .expect(100)
                .mount(&server)
                .await;
            assert!(drain_stored_records(&common::http_client(), "k", None)
                .await
                .is_err());
            server.verify().await;
        }

        #[tokio::test]
        async fn typed_and_diarized_deliver_before_slow_cleanup_and_retain_protection() {
            for diarized in [false, true] {
                let server = MockServer::start().await;
                let _guard = BaseOverrideGuard::install(&server).await;
                mount_empty_reference_listing(&server).await;
                Mock::given(method("POST"))
                    .and(path("/v1/files"))
                    .respond_with(
                        ResponseTemplate::new(201)
                            .set_body_json(serde_json::json!({"id":"f-slow"})),
                    )
                    .mount(&server)
                    .await;
                Mock::given(method("POST"))
                    .and(path("/v1/transcriptions"))
                    .respond_with(
                        ResponseTemplate::new(201)
                            .set_body_json(serde_json::json!({"id":"t-slow"})),
                    )
                    .mount(&server)
                    .await;
                Mock::given(method("GET"))
                    .and(path("/v1/transcriptions/t-slow"))
                    .respond_with(
                        ResponseTemplate::new(200)
                            .set_body_json(serde_json::json!({"status":"completed"})),
                    )
                    .mount(&server)
                    .await;
                Mock::given(method("GET")).and(path("/v1/transcriptions/t-slow/transcript"))
                    .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                        "text":"delivered", "tokens":[{"text":"delivered", "start_ms":0, "end_ms":100, "speaker":"1"}]
                    }))).mount(&server).await;
                Mock::given(method("DELETE"))
                    .and(path("/v1/transcriptions/t-slow"))
                    .respond_with(
                        ResponseTemplate::new(204).set_delay(std::time::Duration::from_secs(2)),
                    )
                    .expect(1)
                    .mount(&server)
                    .await;
                Mock::given(method("DELETE"))
                    .and(path("/v1/files/f-slow"))
                    .respond_with(ResponseTemplate::new(204))
                    .expect(1)
                    .mount(&server)
                    .await;
                let client = common::http_client();
                let delivered = tokio::time::timeout(std::time::Duration::from_secs(1), async {
                    if diarized {
                        attempt_diarized_once(
                            &client,
                            "k",
                            "stt-async-v5",
                            Path::new("audio.wav"),
                            b"audio",
                            None,
                            None,
                        )
                        .await
                        .map(|r| r.text)
                    } else {
                        attempt_typed_once(
                            &client,
                            "k",
                            "stt-async-v5",
                            Path::new("audio.wav"),
                            b"audio",
                            None,
                            None,
                        )
                        .await
                    }
                })
                .await
                .expect("delivery must not wait for DELETE")
                .unwrap();
                assert_eq!(delivered, "delivered");
                assert!(active_file_ids("k").contains("f-slow"));
                assert!(active_transcription_ids("k").contains("t-slow"));
                wait_for_cleanup().await;
                assert!(!is_owned("k", "files", "f-slow"));
                server.verify().await;
            }
        }

        #[tokio::test]
        async fn typed_flow_deletes_transcription_and_file_after_success() {
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server).await;
            mount_empty_reference_listing(&server).await;
            let client = common::http_client();

            Mock::given(method("POST"))
                .and(path("/v1/transcriptions"))
                .respond_with(
                    ResponseTemplate::new(201)
                        .set_body_json(serde_json::json!({ "id": "t1", "status": "queued" })),
                )
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/v1/transcriptions/t1"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({ "status": "completed" })),
                )
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/v1/transcriptions/t1/transcript"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({ "text": "hello world" })),
                )
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("DELETE"))
                .and(path("/v1/transcriptions/t1"))
                .respond_with(ResponseTemplate::new(204))
                .expect(1)
                .mount(&server)
                .await;
            // Soniox does NOT cascade: terminal transcription removal must
            // be followed by an explicit delete of the uploaded file.
            Mock::given(method("DELETE"))
                .and(path("/v1/files/f1"))
                .respond_with(ResponseTemplate::new(204))
                .expect(1)
                .mount(&server)
                .await;

            let (tid, result) =
                run_typed_transcription(&client, "k", "stt-async-v5", "f1", Some("en"), None).await;
            assert_eq!(result.unwrap(), "hello world");
            assert_eq!(tid.as_deref(), Some("t1"));

            cleanup_stored_records(&client, "k", tid.as_deref(), "f1").await;
            server.verify().await;
        }

        #[tokio::test]
        async fn typed_flow_deletes_transcription_and_file_after_job_error() {
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server).await;
            mount_empty_reference_listing(&server).await;
            let client = common::http_client();

            Mock::given(method("POST"))
                .and(path("/v1/transcriptions"))
                .respond_with(
                    ResponseTemplate::new(201)
                        .set_body_json(serde_json::json!({ "id": "t1", "status": "queued" })),
                )
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/v1/transcriptions/t1"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({ "status": "error" })),
                )
                .mount(&server)
                .await;
            Mock::given(method("DELETE"))
                .and(path("/v1/transcriptions/t1"))
                .respond_with(ResponseTemplate::new(204))
                .expect(1)
                .mount(&server)
                .await;
            // Job ended in `error` (terminal) — the uploaded file must go
            // with the transcription record.
            Mock::given(method("DELETE"))
                .and(path("/v1/files/f1"))
                .respond_with(ResponseTemplate::new(204))
                .expect(1)
                .mount(&server)
                .await;

            let (tid, result) =
                run_typed_transcription(&client, "k", "stt-async-v5", "f1", None, None).await;
            assert!(matches!(result, Err(common::SttError::Server)));
            assert_eq!(tid.as_deref(), Some("t1"));

            cleanup_stored_records(&client, "k", tid.as_deref(), "f1").await;
            server.verify().await;
        }

        #[tokio::test]
        async fn create_limit_exceeded_is_terminal_no_retry_and_deletes_orphan_file() {
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server).await;
            mount_empty_reference_listing(&server).await;
            let client = common::http_client();

            // Terminal quota wall: exactly ONE create request proves the
            // limit-429 is classified non-transient (no with_retry re-send).
            Mock::given(method("POST"))
                .and(path("/v1/transcriptions"))
                .respond_with(ResponseTemplate::new(429).set_body_json(limit_exceeded_body()))
                .expect(1)
                .mount(&server)
                .await;
            // No transcription was created, so cleanup must delete the file.
            Mock::given(method("DELETE"))
                .and(path("/v1/files/f1"))
                .respond_with(ResponseTemplate::new(204))
                .expect(1)
                .mount(&server)
                .await;

            let (tid, result) =
                run_typed_transcription(&client, "k", "stt-async-v5", "f1", None, None).await;
            assert!(
                matches!(result, Err(common::SttError::LimitExceeded { .. })),
                "expected LimitExceeded, got {result:?}"
            );
            assert!(tid.is_none());

            cleanup_stored_records(&client, "k", tid.as_deref(), "f1").await;
            server.verify().await;
        }

        #[tokio::test]
        async fn storage_limit_self_heals_via_background_cleanup_and_retry() {
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server).await;
            remember_owned("k", "transcriptions", "t-old");
            remember_owned("k", "files", "f-old");
            let client = common::http_client();

            // Attempt 1 upload -> f1; attempt 2 (after auto-cleanup) -> f2:
            // the retry must restart from upload because the background
            // cleanup deletes the just-uploaded orphan file.
            let uploads = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let uploads_for_mock = uploads.clone();
            Mock::given(method("POST"))
                .and(path("/v1/files"))
                .respond_with(move |_req: &wiremock::Request| {
                    let n = uploads_for_mock.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    ResponseTemplate::new(201).set_body_json(if n == 0 {
                        serde_json::json!({ "id": "f1" })
                    } else {
                        serde_json::json!({ "id": "f2" })
                    })
                })
                .expect(2)
                .mount(&server)
                .await;
            // Attempt 1 create hits the storage wall; attempt 2 succeeds.
            let creates = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let creates_for_mock = creates.clone();
            Mock::given(method("POST"))
                .and(path("/v1/transcriptions"))
                .respond_with(move |_req: &wiremock::Request| {
                    let n = creates_for_mock.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    if n == 0 {
                        ResponseTemplate::new(429).set_body_json(limit_exceeded_body())
                    } else {
                        ResponseTemplate::new(201)
                            .set_body_json(serde_json::json!({ "id": "t1", "status": "queued" }))
                    }
                })
                .expect(2)
                .mount(&server)
                .await;
            // Background auto-cleanup: one old transcription (carrying its
            // file reference) + the attempt-1 orphan file (f1). The drain
            // frees t-old's file f-old INLINE right after the record delete
            // — capacity frees without waiting for a full backlog pass.
            mount_terminal_backlog(&server, "t-old", "f-old").await;
            // Fresh file listing after pass 1: f-old was already freed
            // inline, so only the attempt-1 orphan f1 remains.
            Mock::given(method("GET"))
                .and(path("/v1/files"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({ "files": [ { "id": "f1" } ] })),
                )
                .mount(&server)
                .await;
            // f1 is deleted by attempt 1's orphan cleanup. Its ownership
            // is then removed, so a stale listing cannot delete it again. The
            // self-heal wait unblocks on the first FILE deletion (f-old,
            // freed inline in pass 1), the only deletion that frees upload
            // capacity.
            for (file, expected) in [("f-old", 1), ("f1", 1)] {
                Mock::given(method("DELETE"))
                    .and(path(format!("/v1/files/{file}")))
                    .respond_with(ResponseTemplate::new(204))
                    .expect(expected)
                    .mount(&server)
                    .await;
            }
            // Attempt 2 completes normally.
            Mock::given(method("GET"))
                .and(path("/v1/transcriptions/t1"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({ "status": "completed" })),
                )
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/v1/transcriptions/t1/transcript"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({ "text": "healed" })),
                )
                .mount(&server)
                .await;
            // Attempt 2 terminal exits delete BOTH records explicitly —
            // there is no server-side cascade.
            Mock::given(method("DELETE"))
                .and(path("/v1/transcriptions/t1"))
                .respond_with(ResponseTemplate::new(204))
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("DELETE"))
                .and(path("/v1/files/f2"))
                .respond_with(ResponseTemplate::new(204))
                .expect(1)
                .mount(&server)
                .await;

            let dir = tempfile::tempdir().unwrap();
            let wav = dir.path().join("audio.wav");
            std::fs::write(&wav, b"RIFF....WAVEfmt ").unwrap();

            let text = transcribe_typed_with_autoheal(
                &client,
                "k",
                "stt-async-v5",
                &wav,
                b"RIFF....WAVEfmt ".to_vec(),
                None,
                None,
            )
            .await
            .unwrap();
            assert_eq!(text, "healed");

            wait_for_cleanup().await;
            server.verify().await;
        }

        #[tokio::test]
        async fn diarized_flow_deletes_transcription_and_file_after_success() {
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server).await;
            mount_empty_reference_listing(&server).await;
            let client = common::http_client();

            Mock::given(method("POST"))
                .and(path("/v1/transcriptions"))
                .respond_with(
                    ResponseTemplate::new(201)
                        .set_body_json(serde_json::json!({ "id": "t9", "status": "queued" })),
                )
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/v1/transcriptions/t9"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({ "status": "completed" })),
                )
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/v1/transcriptions/t9/transcript"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({ "text": "hi", "tokens": [] })),
                )
                .mount(&server)
                .await;
            Mock::given(method("DELETE"))
                .and(path("/v1/transcriptions/t9"))
                .respond_with(ResponseTemplate::new(204))
                .expect(1)
                .mount(&server)
                .await;
            // Diarized terminal exit: explicit file delete after the
            // transcription record is gone (no cascade server-side).
            Mock::given(method("DELETE"))
                .and(path("/v1/files/f1"))
                .respond_with(ResponseTemplate::new(204))
                .expect(1)
                .mount(&server)
                .await;

            let (tid, result) =
                run_diarized_transcription(&client, "k", "stt-async-v5", "f1", None, None).await;
            assert_eq!(result.unwrap().text, "hi");
            assert_eq!(tid.as_deref(), Some("t9"));

            cleanup_stored_records(&client, "k", tid.as_deref(), "f1").await;
            server.verify().await;
        }
        #[tokio::test]
        async fn storage_management_internals_count_list_and_delete() {
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server).await;
            let client = common::http_client();

            Mock::given(method("GET"))
                .and(path("/v1/transcriptions/count"))
                .respond_with(ResponseTemplate::new(200).set_body_json(
                    serde_json::json!({ "playground": 0, "public_api": 3, "total": 3 }),
                ))
                .expect(1)
                .mount(&server)
                .await;
            // Page 1 carries a cursor; page 2 ends pagination.
            Mock::given(method("GET"))
                .and(path("/v1/transcriptions"))
                .and(wiremock::matchers::query_param_is_missing("cursor"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "transcriptions": [
                        { "id": "t1", "file_id": null },
                        { "id": "t2", "file_id": null }
                    ],
                    "next_page_cursor": "c1"
                })))
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/v1/transcriptions"))
                .and(wiremock::matchers::query_param("cursor", "c1"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "transcriptions": [ { "id": "t3", "file_id": null } ],
                    "next_page_cursor": null
                })))
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("DELETE"))
                .and(path("/v1/transcriptions/t1"))
                .respond_with(ResponseTemplate::new(204))
                .expect(1)
                .mount(&server)
                .await;
            // 409 = still processing; must surface as SkippedProcessing.
            Mock::given(method("DELETE"))
                .and(path("/v1/transcriptions/t2"))
                .respond_with(ResponseTemplate::new(409))
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("DELETE"))
                .and(path("/v1/transcriptions/t3"))
                .respond_with(ResponseTemplate::new(204))
                .expect(1)
                .mount(&server)
                .await;

            let total = get_total(&client, "k", "transcriptions").await.unwrap();
            assert_eq!(total, 3);

            let (items, _skipped) =
                list_pages::<ListedTranscription>(&client, "k", "transcriptions")
                    .await
                    .unwrap();
            let ids: Vec<String> = items.into_iter().map(|item| item.id).collect();
            assert_eq!(ids, vec!["t1", "t2", "t3"]);

            for id in &ids {
                let url = format!("{}/transcriptions/{id}", base_url());
                let outcome = delete_one(&client, "k", &url).await.unwrap();
                if id == "t2" {
                    assert!(matches!(outcome, DeleteOutcome::SkippedProcessing));
                } else {
                    assert!(matches!(outcome, DeleteOutcome::Deleted));
                }
            }
            server.verify().await;
        }

        #[tokio::test]
        async fn processing_transcription_409_keeps_its_file() {
            // Poll-timeout path: the job is still processing, so the delete
            // of the transcription 409s. Soniox documents that a file still
            // referenced by a transcription that has not finished processing
            // must NOT be deleted (the job would fail `file_not_found`) — a
            // processing 409 is never permission to delete the active file.
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server).await;
            mount_empty_reference_listing(&server).await;
            let client = common::http_client();

            Mock::given(method("DELETE"))
                .and(path("/v1/transcriptions/t1"))
                .respond_with(ResponseTemplate::new(409))
                .expect(1)
                .mount(&server)
                .await;

            cleanup_stored_records(&client, "k", Some("t1"), "f1").await;

            let deletes: Vec<String> = server
                .received_requests()
                .await
                .unwrap()
                .iter()
                .filter(|r| r.method.as_str() == "DELETE")
                .map(|r| r.url.path().to_string())
                .collect();
            assert_eq!(
                deletes,
                vec!["/v1/transcriptions/t1"],
                "file delete must not be attempted while the job is processing"
            );
            server.verify().await;
        }

        #[tokio::test]
        async fn cleanup_treats_missing_records_as_success() {
            // Racing a drain (or a previous attempt) that already deleted
            // the records must still be a success: 404 is the goal state for
            // both the transcription and the file, and the file delete is
            // still attempted after the transcription 404.
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server).await;
            mount_empty_reference_listing(&server).await;
            let client = common::http_client();

            Mock::given(method("DELETE"))
                .and(path("/v1/transcriptions/t1"))
                .respond_with(ResponseTemplate::new(404))
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("DELETE"))
                .and(path("/v1/files/f1"))
                .respond_with(ResponseTemplate::new(404))
                .expect(1)
                .mount(&server)
                .await;

            cleanup_stored_records(&client, "k", Some("t1"), "f1").await;
            server.verify().await;
        }

        #[tokio::test]
        async fn backlog_drain_never_deletes_active_or_live_job_files() {
            // Storage-wall auto-heal / manual backlog drain: files owned by
            // in-flight dictation flows (registered active) and files still
            // referenced by records that could not be deleted because they
            // are processing (409) must survive the drain; only genuinely
            // unreferenced files are deleted.
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server).await;
            // Explicit provenance for fixtures representing this client's prior uploads.
            remember_owned("k", "transcriptions", "t-live");
            remember_owned("k", "files", "f-active");
            remember_owned("k", "files", "f-live");
            remember_owned("k", "files", "f-orphan");
            let client = common::http_client();

            // A transcription still processing (409 on delete) referencing
            // f-live.
            Mock::given(method("GET"))
                .and(path("/v1/transcriptions"))
                .and(wiremock::matchers::query_param_is_missing("cursor"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "transcriptions": [
                        { "id": "t-live", "status": "transcribing", "file_id": "f-live" }
                    ]
                })))
                .mount(&server)
                .await;
            Mock::given(method("DELETE"))
                .and(path("/v1/transcriptions/t-live"))
                .respond_with(ResponseTemplate::new(409))
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/v1/files"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "files": [
                        { "id": "f-live" },
                        { "id": "f-active" },
                        { "id": "f-orphan" }
                    ]
                })))
                .mount(&server)
                .await;
            Mock::given(method("DELETE"))
                .and(path("/v1/files/f-orphan"))
                .respond_with(ResponseTemplate::new(204))
                .expect(1)
                .mount(&server)
                .await;

            // A concurrent dictation flow owns f-active (registered until
            // its guard drops — cancellation cannot leak the protection).
            let _active = ActiveJobGuard::register("f-active", "k");

            let result = drain_stored_records(&client, "k", None).await.unwrap();
            assert_eq!(result.skipped_processing, 1);
            assert_eq!(result.skipped_active, 2);
            assert_eq!(result.deleted_files, 1);
            assert_eq!(result.deleted_transcriptions, 0);
            assert!(result.errors.is_empty(), "{:?}", result.errors);

            let deletes: Vec<String> = server
                .received_requests()
                .await
                .unwrap()
                .iter()
                .filter(|r| r.method.as_str() == "DELETE")
                .map(|r| r.url.path().to_string())
                .collect();
            assert_eq!(
                deletes,
                vec!["/v1/transcriptions/t-live", "/v1/files/f-orphan"],
                "active upload and live-job file must not be deleted"
            );
            server.verify().await;
        }

        #[tokio::test]
        async fn active_file_protection_ends_when_guard_drops() {
            // Cancellation contract: dropping the flow's guard releases the
            // protection immediately, so the NEXT drain reaps the leftover
            // upload (cleanup ownership returns to the backlog drain).
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server).await;
            // Explicit provenance for fixtures representing this client's prior uploads.
            remember_owned("k", "files", "f-guard");
            let client = common::http_client();

            Mock::given(method("GET"))
                .and(path("/v1/transcriptions"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({ "transcriptions": [] })),
                )
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/v1/files"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({ "files": [ { "id": "f-guard" } ] })),
                )
                .mount(&server)
                .await;
            Mock::given(method("DELETE"))
                .and(path("/v1/files/f-guard"))
                .respond_with(ResponseTemplate::new(204))
                .expect(1)
                .mount(&server)
                .await;

            let active = ActiveJobGuard::register("f-guard", "k");
            let first = drain_stored_records(&client, "k", None).await.unwrap();
            assert_eq!(first.deleted_files, 0);
            assert_eq!(first.skipped_active, 1);

            drop(active);
            let second = drain_stored_records(&client, "k", None).await.unwrap();
            assert_eq!(second.deleted_files, 1);
            assert_eq!(second.skipped_active, 0);

            server.verify().await;
        }

        #[tokio::test]
        async fn transient_rate_limit_429_stays_rate_limited_without_destructive_drain() {
            // A per-minute file-management RPM wall uses the SAME
            // `error_type: "limit_exceeded"` as retained-storage walls, but
            // its documented message names the per-minute rate. It must
            // surface as the transient RateLimited — NOT trigger the
            // storage self-heal (no whole-library drain, no storage nag).
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server).await;
            mount_empty_reference_listing(&server).await;
            let client = common::http_client();

            Mock::given(method("POST"))
                .and(path("/v1/files"))
                .respond_with(ResponseTemplate::new(429).set_body_json(
                    serde_json::json!({
                        "status_code": 429,
                        "error_type": "limit_exceeded",
                        "message": "Requests per minute limit for file management has been exceeded for your organization."
                    }),
                ))
                // Exactly two uploads: the shared transient retry, nothing
                // more — no storage-wall restart from upload.
                .expect(2)
                .mount(&server)
                .await;

            let dir = tempfile::tempdir().unwrap();
            let wav = dir.path().join("audio.wav");
            std::fs::write(&wav, b"RIFF....WAVEfmt ").unwrap();

            let error = transcribe_typed_with_autoheal(
                &client,
                "k",
                "stt-async-v5",
                &wav,
                b"RIFF....WAVEfmt ".to_vec(),
                None,
                None,
            )
            .await
            .unwrap_err();
            assert!(
                matches!(error, common::SttError::RateLimited),
                "expected RateLimited, got {error:?}"
            );

            // No DELETE may reach the server: a rate limit must never
            // destroy stored records.
            let deletes = server
                .received_requests()
                .await
                .unwrap()
                .iter()
                .filter(|r| r.method.as_str() == "DELETE")
                .count();
            assert_eq!(deletes, 0, "transient 429 must not spawn a cleanup drain");
            server.verify().await;
        }

        #[tokio::test]
        async fn drain_listings_wait_out_in_flight_upload_registration() {
            // Delayed-visibility regression: a file becomes server-side
            // list-visible during the upload POST, BEFORE the response
            // delivers the id and the flow can register it. While an
            // upload window (gate read guard) is open, a drain must not
            // list — otherwise it could see the file unregistered, delete
            // it, and fail the job with `file_not_found`.
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server).await;
            // Explicit provenance for fixtures representing this client's prior uploads.
            remember_owned("k", "files", "f1");
            let client = common::http_client();

            Mock::given(method("GET"))
                .and(path("/v1/transcriptions"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({ "transcriptions": [] })),
                )
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/v1/files"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({ "files": [ { "id": "f1" } ] })),
                )
                .mount(&server)
                .await;
            Mock::given(method("DELETE"))
                .and(path("/v1/files/f1"))
                .respond_with(ResponseTemplate::new(204))
                .mount(&server)
                .await;

            // Simulate the upload window: gate read guard held, response
            // not yet processed, nothing registered.
            let coordinator = cleanup_coordinator("k");
            let _upload_window = coordinator.listing_gate.read().await;
            let drain_client = client.clone();
            let drain =
                tokio::spawn(async move { drain_stored_records(&drain_client, "k", None).await });
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            // The drain must still be parked on the gate: no listing
            // request has been issued while the id was unregistered.
            let listings_so_far = server
                .received_requests()
                .await
                .unwrap()
                .iter()
                .filter(|r| r.method.as_str() == "GET" && r.url.path() == "/v1/files")
                .count();
            assert_eq!(
                listings_so_far, 0,
                "drain listed files while an upload was still unregistered"
            );

            // The upload response lands and the flow registers the id —
            // exactly what attempt_typed_once does before dropping the
            // gate. Only then may the drain proceed.
            let _registered = ActiveJobGuard::register("f1", "k");
            drop(_upload_window);

            let result = drain.await.unwrap().unwrap();
            assert_eq!(result.skipped_active, 1, "registered upload must survive");
            assert_eq!(result.deleted_files, 0);
        }

        #[tokio::test]
        async fn drain_pass_one_never_deletes_app_owned_jobs() {
            // A queued or completed-but-not-yet-extracted transcription of
            // an in-flight flow can be deleted server-side — pass 1 must
            // skip app-owned records entirely and keep their file
            // protected in pass 2.
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server).await;
            // Explicit provenance for fixtures representing this client's prior uploads.
            remember_owned("k", "transcriptions", "t-app");
            remember_owned("k", "files", "f-app");
            let client = common::http_client();

            Mock::given(method("GET"))
                .and(path("/v1/transcriptions"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "transcriptions": [
                        { "id": "t-app", "status": "queued", "file_id": "f-app" }
                    ]
                })))
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/v1/files"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({ "files": [ { "id": "f-app" } ] })),
                )
                .mount(&server)
                .await;

            let _owner = ActiveJobGuard::register("f-app", "k");
            attach_transcription("f-app", "t-app", "k");

            let result = drain_stored_records(&client, "k", None).await.unwrap();
            assert_eq!(result.skipped_active_jobs, 1);
            assert_eq!(result.skipped_active, 1);
            assert_eq!(result.deleted_transcriptions, 0);
            assert_eq!(result.deleted_files, 0);

            let deletes: Vec<String> = server
                .received_requests()
                .await
                .unwrap()
                .iter()
                .filter(|r| r.method.as_str() == "DELETE")
                .map(|r| r.url.path().to_string())
                .collect();
            assert!(
                deletes.is_empty(),
                "app-owned records must not be drained: {deletes:?}"
            );
        }

        #[tokio::test]
        async fn failed_transcription_delete_keeps_file_ref() {
            // Beyond the processing 409: ANY failed record delete (here a
            // transient 500) must retain the file — the record still
            // exists and its job still needs it. Deleting the file on a
            // failed record delete would fail the job with
            // `file_not_found`.
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server).await;
            mount_empty_reference_listing(&server).await;
            let client = common::http_client();

            Mock::given(method("DELETE"))
                .and(path("/v1/transcriptions/t1"))
                .respond_with(ResponseTemplate::new(500))
                .expect(1)
                .mount(&server)
                .await;

            cleanup_stored_records(&client, "k", Some("t1"), "f1").await;

            let deletes: Vec<String> = server
                .received_requests()
                .await
                .unwrap()
                .iter()
                .filter(|r| r.method.as_str() == "DELETE")
                .map(|r| r.url.path().to_string())
                .collect();
            assert_eq!(
                deletes,
                vec!["/v1/transcriptions/t1"],
                "file delete must not follow a failed record delete"
            );
            server.verify().await;
        }

        #[tokio::test]
        async fn drain_frees_terminal_record_file_without_full_backlog_pass() {
            // Capacity-awareness regression: the self-heal retry waits for
            // FILE capacity, so a terminal record's file must be freed
            // inline in pass 1 — before the (possibly huge) file listing —
            // not deferred until every record has been drained.
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server).await;
            // Explicit provenance for fixtures representing this client's prior uploads.
            remember_owned("k", "transcriptions", "t-done");
            remember_owned("k", "files", "f-done");
            let client = common::http_client();

            mount_terminal_backlog(&server, "t-done", "f-done").await;
            Mock::given(method("DELETE"))
                .and(path("/v1/files/f-done"))
                .respond_with(ResponseTemplate::new(204))
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/v1/files"))
                .respond_with(
                    ResponseTemplate::new(200).set_body_json(serde_json::json!({ "files": [] })),
                )
                .mount(&server)
                .await;

            let result = drain_stored_records(&client, "k", None).await.unwrap();
            assert_eq!(result.deleted_transcriptions, 1);
            assert_eq!(result.deleted_files, 1);

            // The file delete must have landed BEFORE the pass-2 file
            // listing: inline freeing, not end-of-drain freeing.
            let requests = server.received_requests().await.unwrap();
            let delete_index = requests
                .iter()
                .position(|r| r.method.as_str() == "DELETE" && r.url.path() == "/v1/files/f-done")
                .expect("inline file delete missing");
            let files_list_index = requests
                .iter()
                .position(|r| r.method.as_str() == "GET" && r.url.path() == "/v1/files")
                .expect("pass-2 file listing missing");
            assert!(
                delete_index < files_list_index,
                "file must be freed inline (at {delete_index}) before the pass-2 listing (at {files_list_index})"
            );
            server.verify().await;
        }

        #[tokio::test]
        async fn inline_free_skips_shared_file_when_sibling_delete_failed() {
            // The API permits several transcriptions to reference one file.
            // If tA deletes fine but tB's delete fails (500), the shared
            // file still has a surviving reference: it must NOT be freed
            // inline after tA, and pass 2 must keep it protected.
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server).await;
            // Explicit provenance for fixtures representing this client's prior uploads.
            remember_owned("k", "transcriptions", "t-a");
            remember_owned("k", "transcriptions", "t-b");
            remember_owned("k", "files", "f-shared");
            let client = common::http_client();

            Mock::given(method("GET"))
                .and(path("/v1/transcriptions"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "transcriptions": [
                        { "id": "t-a", "file_id": "f-shared" },
                        { "id": "t-b", "file_id": "f-shared" }
                    ]
                })))
                .mount(&server)
                .await;
            Mock::given(method("DELETE"))
                .and(path("/v1/transcriptions/t-a"))
                .respond_with(ResponseTemplate::new(204))
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("DELETE"))
                .and(path("/v1/transcriptions/t-b"))
                .respond_with(ResponseTemplate::new(500))
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/v1/files"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({ "files": [ { "id": "f-shared" } ] })),
                )
                .mount(&server)
                .await;

            let result = drain_stored_records(&client, "k", None).await.unwrap();
            assert_eq!(result.deleted_transcriptions, 1);
            assert_eq!(result.deleted_files, 0);
            assert_eq!(result.skipped_active, 1, "shared file stays protected");

            let deletes: Vec<String> = server
                .received_requests()
                .await
                .unwrap()
                .iter()
                .filter(|r| r.method.as_str() == "DELETE")
                .map(|r| r.url.path().to_string())
                .collect();
            assert_eq!(
                deletes,
                vec!["/v1/transcriptions/t-a", "/v1/transcriptions/t-b"],
                "shared file must not be freed while a sibling record survives"
            );
        }

        #[tokio::test]
        async fn pass_two_keeps_files_of_jobs_created_between_passes() {
            // A flow that uploaded+created its job AFTER pass 1 snapshotted
            // and was then cancelled (guard dropped before pass 2) has no
            // registry entry and was absent from the pass-1 listing. The
            // pass-2 FRESH transcription listing still references its file,
            // which must therefore survive.
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server).await;
            // Explicit provenance for fixtures representing this client's prior uploads.
            remember_owned("k", "transcriptions", "t-new");
            remember_owned("k", "transcriptions", "t-old");
            remember_owned("k", "files", "f-new");
            remember_owned("k", "files", "f-old");
            let client = common::http_client();

            let listings = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let listings_for_mock = listings.clone();
            Mock::given(method("GET"))
                .and(path("/v1/transcriptions"))
                .respond_with(move |_req: &wiremock::Request| {
                    let n = listings_for_mock.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    ResponseTemplate::new(200).set_body_json(if n == 0 {
                        serde_json::json!({
                            "transcriptions": [ { "id": "t-old", "file_id": "f-old" } ]
                        })
                    } else {
                        serde_json::json!({
                            "transcriptions": [ { "id": "t-new", "file_id": "f-new" } ]
                        })
                    })
                })
                .mount(&server)
                .await;
            Mock::given(method("DELETE"))
                .and(path("/v1/transcriptions/t-old"))
                .respond_with(ResponseTemplate::new(204))
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("DELETE"))
                .and(path("/v1/files/f-old"))
                .respond_with(ResponseTemplate::new(204))
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/v1/files"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({ "files": [ { "id": "f-new" } ] })),
                )
                .mount(&server)
                .await;

            let result = drain_stored_records(&client, "k", None).await.unwrap();
            assert_eq!(result.deleted_transcriptions, 1);
            assert_eq!(result.deleted_files, 1, "only f-old is freed");
            assert_eq!(result.skipped_active, 1, "f-new is referenced and kept");

            let deletes: Vec<String> = server
                .received_requests()
                .await
                .unwrap()
                .iter()
                .filter(|r| r.method.as_str() == "DELETE")
                .map(|r| r.url.path().to_string())
                .collect();
            assert_eq!(
                deletes,
                vec!["/v1/transcriptions/t-old", "/v1/files/f-old"],
                "between-passes job file must not be deleted"
            );
        }

        #[tokio::test]
        async fn incomplete_metadata_fails_file_cleanup_closed() {
            // Fail-closed schema regression: a record whose file_id is
            // MISSING (not an explicit documented null) has an unknown
            // reference state. File cleanup must abort for the whole run —
            // no file may be classified as an orphan on incomplete data.
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server).await;
            // Explicit provenance for fixtures representing this client's prior uploads.
            remember_owned("k", "transcriptions", "t-corrupt");
            remember_owned("k", "transcriptions", "t-ok");
            remember_owned("k", "files", "f-ok");
            let client = common::http_client();

            Mock::given(method("GET"))
                .and(path("/v1/transcriptions"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "transcriptions": [
                        { "id": "t-ok", "file_id": "f-ok" },
                        { "id": "t-corrupt" }
                    ]
                })))
                .mount(&server)
                .await;
            Mock::given(method("DELETE"))
                .and(path("/v1/transcriptions/t-ok"))
                .respond_with(ResponseTemplate::new(204))
                .expect(1)
                .mount(&server)
                .await;

            let result = drain_stored_records(&client, "k", None).await.unwrap();
            assert_eq!(result.deleted_transcriptions, 1);
            assert_eq!(result.deleted_files, 0);
            assert!(
                result
                    .errors
                    .iter()
                    .any(|e| e.contains("incomplete metadata; file cleanup skipped")),
                "fail-closed reason must be reported: {:?}",
                result.errors
            );

            let requests = server.received_requests().await.unwrap();
            assert!(
                !requests
                    .iter()
                    .any(|r| r.method.as_str() == "DELETE" && r.url.path().starts_with("/v1/files")),
                "no file may be deleted on incomplete metadata"
            );
            assert!(
                !requests
                    .iter()
                    .any(|r| r.method.as_str() == "GET" && r.url.path() == "/v1/files"),
                "file listing must not even run in fail-closed mode"
            );
        }

        #[tokio::test]
        async fn record_count_wall_wakes_on_record_deletions_not_file_frees() {
            // Free record capacity while a later deletion keeps the drain
            // running. Waiting for file capacity or drain completion must
            // not satisfy this regression.
            let server = MockServer::start().await;
            let _guard = BaseOverrideGuard::install(&server).await;
            remember_owned("k", "transcriptions", "t-old");
            remember_owned("k", "transcriptions", "t-slow");
            let client = common::http_client();

            let uploads = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let uploads_for_mock = uploads.clone();
            Mock::given(method("POST"))
                .and(path("/v1/files"))
                .respond_with(move |_req: &wiremock::Request| {
                    let n = uploads_for_mock.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    ResponseTemplate::new(201)
                        .set_body_json(serde_json::json!({ "id": format!("f{n}") }))
                })
                .expect(2)
                .mount(&server)
                .await;
            let creates = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let creates_for_mock = creates.clone();
            let record_freed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let record_freed_for_create = record_freed.clone();
            Mock::given(method("POST"))
                .and(path("/v1/transcriptions"))
                .respond_with(move |_req: &wiremock::Request| {
                    let n = creates_for_mock.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    if n == 0 || !record_freed_for_create.load(std::sync::atomic::Ordering::SeqCst)
                    {
                        ResponseTemplate::new(429).set_body_json(serde_json::json!({
                            "status_code": 429,
                            "error_type": "limit_exceeded",
                            "message": "transcribe_async_total_num_files limit has been exceeded."
                        }))
                    } else {
                        ResponseTemplate::new(201)
                            .set_body_json(serde_json::json!({ "id": "t1", "status": "queued" }))
                    }
                })
                .expect(2)
                .mount(&server)
                .await;
            // Both backlog records are URL-based: neither owns a file.
            Mock::given(method("GET"))
                .and(path("/v1/transcriptions"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "transcriptions": [
                        { "id": "t-old", "file_id": null },
                        { "id": "t-slow", "file_id": null }
                    ]
                })))
                .mount(&server)
                .await;
            Mock::given(method("DELETE"))
                .and(path("/v1/transcriptions/t-old"))
                .respond_with(move |_req: &wiremock::Request| {
                    record_freed.store(true, std::sync::atomic::Ordering::SeqCst);
                    ResponseTemplate::new(204)
                })
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("DELETE"))
                .and(path("/v1/transcriptions/t-slow"))
                .respond_with(
                    ResponseTemplate::new(204).set_delay(std::time::Duration::from_secs(3)),
                )
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/v1/files"))
                .respond_with(
                    ResponseTemplate::new(200).set_body_json(serde_json::json!({ "files": [] })),
                )
                .mount(&server)
                .await;
            // Attempt 2 completes normally; terminal exits delete both
            // records explicitly (no server-side cascade).
            Mock::given(method("GET"))
                .and(path("/v1/transcriptions/t1"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({ "status": "completed" })),
                )
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/v1/transcriptions/t1/transcript"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({ "text": "record-healed" })),
                )
                .mount(&server)
                .await;
            Mock::given(method("DELETE"))
                .and(path("/v1/transcriptions/t1"))
                .respond_with(ResponseTemplate::new(204))
                .expect(1)
                .mount(&server)
                .await;
            Mock::given(method("DELETE"))
                .and(path("/v1/files/f1"))
                .respond_with(ResponseTemplate::new(204))
                .expect(1)
                .mount(&server)
                .await;

            let dir = tempfile::tempdir().unwrap();
            let wav = dir.path().join("audio.wav");
            std::fs::write(&wav, b"RIFF....WAVEfmt ").unwrap();

            let result = transcribe_typed_with_autoheal(
                &client,
                "k",
                "stt-async-v5",
                &wav,
                b"RIFF....WAVEfmt ".to_vec(),
                None,
                None,
            )
            .await;
            let resumed_before_drain_finished = cleanup_coordinator("k")
                .running
                .load(std::sync::atomic::Ordering::SeqCst);
            // Global-state hygiene: the background drain (spawned inside
            // the flow) must be awaited before the test ends, so its
            // statics/gate are quiet for sibling tests.
            for _ in 0..100 {
                if !cleanup_coordinator("k")
                    .running
                    .load(std::sync::atomic::Ordering::SeqCst)
                {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            assert!(
                !cleanup_coordinator("k")
                    .running
                    .load(std::sync::atomic::Ordering::SeqCst),
                "background drain did not finish"
            );
            assert!(
                resumed_before_drain_finished,
                "record capacity must wake the flow before the slow drain finishes"
            );
            assert_eq!(result.unwrap(), "record-healed");
            server.verify().await;
        }
    }
}
