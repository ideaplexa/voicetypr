#[cfg(not(target_os = "windows"))]
use crate::license::device;
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose, Engine as _};
#[cfg(not(target_os = "windows"))]
use once_cell::sync::OnceCell;
use pbkdf2::pbkdf2_hmac;
use rand::Rng;
use sha2::Sha256;
use std::path::Path;
use std::sync::Arc;
use tauri::{AppHandle, Runtime};
use tauri_plugin_store::{resolve_store_path, Store, StoreExt};

#[cfg(any(target_os = "windows", test))]
pub(crate) mod windows_identity;

// Encryption key storage - OnceCell ensures thread-safe single initialization
#[cfg(not(target_os = "windows"))]
static ENCRYPTION_KEY: OnceCell<[u8; 32]> = OnceCell::new();

/// Store file (relative to the app data directory) managed via tauri-plugin-store.
const SECURE_STORE_FILE: &str = "secure.dat";

/// Initialize the encryption key using the device hash with PBKDF2
#[cfg(any(not(target_os = "windows"), test))]
pub fn initialize_encryption_key() -> Result<(), String> {
    #[cfg(all(target_os = "windows", test))]
    return windows_identity::initialize_test_identity();
    #[cfg(not(target_os = "windows"))]
    ENCRYPTION_KEY
        .get_or_try_init(|| derive_legacy_key(&device::get_device_hash()?))
        .map(|_| ())
}

fn derive_legacy_key(device_hash: &str) -> Result<[u8; 32], String> {
    if device_hash.len() != 64 || !device_hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Invalid device hash for encryption".to_string());
    }
    // Keep the historical input, salt and iteration count byte-for-byte compatible.
    let mut key = [0u8; 32];
    pbkdf2_hmac::<Sha256>(
        device_hash.as_bytes(),
        b"voicetypr-secure-store-v1",
        100_000,
        &mut key,
    );
    Ok(key)
}

/// Check if migration from keychain is needed (for future use)
#[allow(dead_code)]
pub fn check_migration_needed<R: Runtime>(app: &AppHandle<R>) -> bool {
    // Read-only existence check: never call `app.store()` here — building a
    // store registers it (and the plugin saves registered stores on exit).
    let store_exists = resolve_store_path(app, SECURE_STORE_FILE)
        .map(|path| path.exists())
        .unwrap_or(false);

    // For now, we don't migrate automatically
    // This is here for future use if needed
    if !store_exists {
        log::debug!("No secure store found, fresh installation");
    }

    false
}

/// Encrypt a string value
fn encrypt_value(value: &str) -> Result<String, String> {
    #[cfg(target_os = "windows")]
    let key = &windows_identity::keys()?[0];
    #[cfg(not(target_os = "windows"))]
    let key = ENCRYPTION_KEY
        .get()
        .ok_or("Encryption key not initialized")?;

    encrypt_value_with_key(value, key)
}

fn encrypt_value_with_key(value: &str, key: &[u8; 32]) -> Result<String, String> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| "Failed to create cipher")?;

    // Generate random nonce
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    // Encrypt
    let ciphertext = cipher
        .encrypt(nonce, value.as_bytes())
        .map_err(|_| "Encryption failed")?;

    // Combine nonce and ciphertext
    let mut combined = nonce_bytes.to_vec();
    combined.extend_from_slice(&ciphertext);

    // Base64 encode
    Ok(general_purpose::STANDARD.encode(combined))
}

/// Decrypt a string value
fn decrypt_value(encrypted: &str) -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        for key in windows_identity::keys()? {
            match decrypt_value_with_key(encrypted, key) {
                Ok(value) => return Ok(value),
                Err(error) if error == "Decryption failed" => {}
                Err(error) => return Err(error),
            }
        }
        Err("Decryption failed".to_string())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let key = ENCRYPTION_KEY
            .get()
            .ok_or("Encryption key not initialized")?;
        decrypt_value_with_key(encrypted, key)
    }
}

fn decrypt_value_with_key(encrypted: &str, key: &[u8; 32]) -> Result<String, String> {
    // Base64 decode
    let combined = general_purpose::STANDARD
        .decode(encrypted)
        .map_err(|_| "Failed to decode encrypted value")?;

    if combined.len() < 12 {
        return Err("Invalid encrypted value".to_string());
    }

    // Split nonce and ciphertext
    let (nonce_bytes, ciphertext) = combined.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);

    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| "Failed to create cipher")?;

    // Decrypt
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| "Decryption failed")?;

    String::from_utf8(plaintext).map_err(|_| "Invalid UTF-8 in decrypted value".to_string())
}

/// Set an encrypted value in the store
pub fn secure_set<R: Runtime>(app: &AppHandle<R>, key: &str, value: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    windows_identity::ensure_persisted(key)?;
    let encrypted = encrypt_value(value)?;

    let store = writable_store(app)?;

    store.set(key, encrypted);
    store
        .save()
        .map_err(|e| format!("Failed to save store: {}", e))?;

    Ok(())
}

/// Read-only inspection of the store FILE on disk.
///
/// Unlike `Store::reload` this never touches the shared in-memory store
/// cache, so it cannot clobber a concurrent `secure_set` whose save has not
/// landed yet. A missing file is a fresh installation (`Ok(None)`); anything
/// present but unreadable is a distinct error.
fn read_store_file(
    path: &Path,
) -> Result<Option<serde_json::Map<String, serde_json::Value>>, String> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("Secure store file could not be read: {}", e)),
    };
    serde_json::from_slice(&bytes).map(Some).map_err(|e| {
        // serde_json errors can embed the unexpected payload for scalar
        // values — log only the classification/position and return a
        // payload-free message.
        log::warn!(
            "Secure store file parse failed: {:?} at line {} column {}",
            e.classify(),
            e.line(),
            e.column()
        );
        "Secure store file could not be read (it may be corrupted)".to_string()
    })
}

/// Validate unopened stores before registering them: the plugin ignores initial
/// load errors and saves registered stores on exit, which could replace an
/// unreadable file with an empty cache even if the requested write later fails.
fn writable_store<R: Runtime>(app: &AppHandle<R>) -> Result<Arc<Store<R>>, String> {
    if let Some(store) = app.get_store(SECURE_STORE_FILE) {
        return Ok(store);
    }

    let path = resolve_store_path(app, SECURE_STORE_FILE)
        .map_err(|e| format!("Secure store is unavailable: {}", e))?;
    read_store_file(&path)?;
    app.store(SECURE_STORE_FILE)
        .map_err(|e| format!("Failed to access store: {}", e))
}

/// Decrypt a raw stored entry. Read failures never mutate anything: the saved
/// record stays exactly as-is for recovery (re-entering the value, activation,
/// or an explicit reset).
fn decrypt_raw_entry(key: &str, raw: Option<&serde_json::Value>) -> Result<Option<String>, String> {
    let encrypted = match raw {
        None => return Ok(None),
        Some(value) => value.as_str().ok_or_else(|| {
            log::error!(
                "Stored value for key '{}' has an unexpected format (expected encrypted text). The saved entry was preserved.",
                key
            );
            format!(
                "Stored value for '{}' has an unexpected format; the saved entry was preserved",
                key
            )
        })?,
    };
    decrypt_value(encrypted).map(Some).map_err(|e| {
        log::error!(
            "Stored value for key '{}' could not be decrypted: {}. The saved entry was preserved.",
            key,
            e
        );
        format!(
            "Stored value for '{}' could not be decrypted ({}); the saved entry was preserved",
            key, e
        )
    })
}

/// Get and decrypt a value from the store.
///
/// Strictly read-only: it never registers a store, creates the file, or
/// mutates the shared cache. Read failures are distinct from a missing entry —
/// an undecryptable value, an entry with an unexpected type, or an unreadable
/// store file return an error while the saved record stays untouched on disk.
///
/// Once the store is open, its cache is authoritative: a miss there is a real
/// absence (e.g. a just-completed delete) and is never backfilled from disk,
/// so an in-flight `secure_delete` cannot briefly resurrect the old value.
pub fn secure_get<R: Runtime>(app: &AppHandle<R>, key: &str) -> Result<Option<String>, String> {
    // Already-open store: serve from its cache. `get_store` has no side
    // effects (unlike `app.store()`, which registers the store). No disk
    // fallback on miss — see the cache-authoritative note above.
    if let Some(store) = app.get_store(SECURE_STORE_FILE) {
        return decrypt_raw_entry(key, store.get(key).as_ref());
    }

    // Store not open: inspect the store FILE directly — read-only. Do NOT
    // call `app.store()` here: building a store registers it, and the plugin
    // saves every registered store on app exit, so registering against an
    // unreadable file would let exit overwrite the on-disk bytes with an
    // empty cache.
    let path = resolve_store_path(app, SECURE_STORE_FILE)
        .map_err(|e| format!("Secure store is unavailable: {}", e))?;
    match read_store_file(&path)? {
        // No file yet: fresh installation, nothing stored.
        None => Ok(None),
        Some(disk) => decrypt_raw_entry(key, disk.get(key)),
    }
}

/// Delete a value from the secure store
pub fn secure_delete<R: Runtime>(app: &AppHandle<R>, key: &str) -> Result<(), String> {
    let store = writable_store(app)?;

    store.delete(key);
    store
        .save()
        .map_err(|e| format!("Failed to save store: {}", e))?;

    Ok(())
}

/// Check if a key exists in the secure store AND is readable (decryptable).
///
/// Like `secure_get`, store-level failures (unreadable file) are errors, not a
/// silent `false`; reads never mutate stored data.
pub fn secure_has<R: Runtime>(app: &AppHandle<R>, key: &str) -> Result<bool, String> {
    secure_get(app, key).map(|value| value.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tauri::plugin::{Plugin, TauriPlugin};
    use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};
    use tempfile::TempDir;

    fn mock_store_app(dir: &TempDir) -> (tauri::App<MockRuntime>, TauriPlugin<MockRuntime>) {
        let mut context = mock_context(noop_assets());
        // The path resolver joins the identifier to the platform data directory.
        // An absolute temporary identifier isolates each test without changing
        // process-wide environment variables or touching the user's app data.
        context.config_mut().identifier = dir.path().to_str().unwrap().to_string();
        let app = mock_builder().build(context).unwrap();
        let mut plugin = tauri_plugin_store::Builder::default().build();
        plugin
            .initialize(app.handle(), serde_json::Value::Null)
            .unwrap();
        assert_eq!(
            resolve_store_path(app.handle(), SECURE_STORE_FILE).unwrap(),
            dir.path().join(SECURE_STORE_FILE)
        );
        (app, plugin)
    }

    #[test]
    fn malformed_store_rejects_access_without_registration_or_exit_overwrite() {
        initialize_encryption_key().unwrap();
        for bytes in [b"{ not valid json".as_slice(), b"null", b"[]"] {
            let dir = TempDir::new().unwrap();
            let path = dir.path().join(SECURE_STORE_FILE);
            fs::write(&path, bytes).unwrap();
            let (app, mut plugin) = mock_store_app(&dir);

            for result in [
                secure_get(app.handle(), "license").map(|_| ()),
                secure_set(app.handle(), "unrelated_api_key", "replacement"),
                secure_delete(app.handle(), "unrelated_api_key"),
                secure_set(app.handle(), "license", "replacement"),
                secure_delete(app.handle(), "license"),
            ] {
                assert!(result.unwrap_err().starts_with("Secure store"));
                assert!(app.get_store(SECURE_STORE_FILE).is_none());
                assert_eq!(fs::read(&path).unwrap(), bytes);
            }

            // Exercise the plugin's actual exit callback, which saves every
            // registered store, even stores that have not been mutated.
            plugin.on_event(app.handle(), &tauri::RunEvent::Exit);
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
    }

    #[test]
    fn unreadable_entry_survives_other_writes_and_allows_explicit_replacement() {
        initialize_encryption_key().unwrap();
        let dir = TempDir::new().unwrap();
        let unreadable = tamper_ciphertext(&encrypt_value("old-license").unwrap());
        let path = write_store_file(&dir, &serde_json::json!({"license": unreadable}));
        let (app, mut plugin) = mock_store_app(&dir);

        assert!(secure_get(app.handle(), "license")
            .unwrap_err()
            .contains("Decryption failed"));
        secure_set(app.handle(), "api_key", "secret").unwrap();
        secure_delete(app.handle(), "api_key").unwrap();
        plugin.on_event(app.handle(), &tauri::RunEvent::Exit);
        let disk = read_store_file(&path).unwrap().unwrap();
        assert_eq!(disk.get("license").unwrap(), &unreadable);
        assert!(secure_get(app.handle(), "license").is_err());

        secure_set(app.handle(), "license", "recovered-license").unwrap();
        assert_eq!(
            secure_get(app.handle(), "license").unwrap().as_deref(),
            Some("recovered-license")
        );
        plugin.on_event(app.handle(), &tauri::RunEvent::Exit);
        let disk = read_store_file(&path).unwrap().unwrap();
        assert_eq!(
            decrypt_raw_entry("license", disk.get("license"))
                .unwrap()
                .as_deref(),
            Some("recovered-license")
        );

        // A same-process readback alone can hide persistence bugs. A new app
        // must read the replacement from disk, not the old in-memory cache.
        drop(plugin);
        drop(app);
        let (restarted, _plugin) = mock_store_app(&dir);
        assert_eq!(
            secure_get(restarted.handle(), "license")
                .unwrap()
                .as_deref(),
            Some("recovered-license")
        );
    }

    #[test]
    fn registered_cache_remains_authoritative_for_reads_and_writes() {
        initialize_encryption_key().unwrap();
        let dir = TempDir::new().unwrap();
        let encrypted = encrypt_value("old-license").unwrap();
        let path = write_store_file(&dir, &serde_json::json!({"license": encrypted}));
        let (app, mut plugin) = mock_store_app(&dir);
        let store = app
            .store_builder(SECURE_STORE_FILE)
            .disable_auto_save()
            .build()
            .unwrap();
        store.delete("license"); // Deleted in cache; the old value is still on disk.

        assert_eq!(secure_get(app.handle(), "license").unwrap(), None);
        secure_set(app.handle(), "api_key", "secret").unwrap();
        assert_eq!(secure_get(app.handle(), "license").unwrap(), None);
        plugin.on_event(app.handle(), &tauri::RunEvent::Exit);
        assert!(!read_store_file(&path)
            .unwrap()
            .unwrap()
            .contains_key("license"));
    }

    #[test]
    fn test_encryption_decryption() {
        initialize_encryption_key().unwrap();

        let original = "my-secret-api-key";
        let encrypted = encrypt_value(original).unwrap();
        let decrypted = decrypt_value(&encrypted).unwrap();

        assert_eq!(original, decrypted);
        assert_ne!(original, encrypted);
    }

    #[test]
    fn test_different_encryptions() {
        initialize_encryption_key().unwrap();

        let original = "test-value";
        let encrypted1 = encrypt_value(original).unwrap();
        let encrypted2 = encrypt_value(original).unwrap();

        // Different nonces should produce different ciphertexts
        assert_ne!(encrypted1, encrypted2);

        // But both should decrypt to the same value
        assert_eq!(decrypt_value(&encrypted1).unwrap(), original);
        assert_eq!(decrypt_value(&encrypted2).unwrap(), original);
    }

    #[test]
    fn test_decrypt_with_corrupted_data() {
        initialize_encryption_key().unwrap();

        // Test with invalid base64
        let result = decrypt_value("not-valid-base64!");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to decode"));

        // Test with valid base64 but corrupted data
        let result = decrypt_value("dGVzdA=="); // Just "test" in base64
        assert!(result.is_err());
    }

    /// Flip one ciphertext byte (after the 12-byte nonce): same payload
    /// length, but AES-GCM authentication now fails — the same signature a
    /// wrong key derivation or on-disk corruption produces.
    fn tamper_ciphertext(encrypted: &str) -> String {
        let mut combined = general_purpose::STANDARD.decode(encrypted).unwrap();
        let last = combined.len() - 1;
        combined[last] ^= 0xFF;
        general_purpose::STANDARD.encode(&combined)
    }

    fn write_store_file(dir: &TempDir, contents: &serde_json::Value) -> std::path::PathBuf {
        let path = dir.path().join(SECURE_STORE_FILE);
        fs::write(&path, serde_json::to_vec(contents).unwrap()).unwrap();
        path
    }

    fn read_back(path: &std::path::Path) -> Vec<u8> {
        fs::read(path).unwrap()
    }

    #[test]
    fn tamper_is_a_valid_length_gcm_auth_failure_not_a_decode_error() {
        initialize_encryption_key().unwrap();
        let tampered = tamper_ciphertext(&encrypt_value("VTLICENSE-ABCD-1234").unwrap());

        let err = decrypt_value(&tampered).unwrap_err();

        assert_eq!(err, "Decryption failed", "auth failure, not base64/length");
    }

    #[test]
    fn missing_store_file_reads_as_absent_fresh_install() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join(SECURE_STORE_FILE); // never created

        assert_eq!(read_store_file(&path).unwrap(), None);
    }

    #[test]
    fn unreadable_store_file_is_distinct_from_missing_entry() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join(SECURE_STORE_FILE);
        fs::write(&path, b"{ not valid json").unwrap();

        let result = read_store_file(&path);

        assert!(result.is_err(), "corrupt file must not read as absent");
        assert!(result.unwrap_err().contains("Secure store"));
    }

    #[test]
    fn parse_error_never_discloses_file_payload() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join(SECURE_STORE_FILE);
        // A scalar top-level payload (e.g. a mistakenly pasted secret) must
        // never appear in the returned error: serde_json's `invalid type`
        // text would embed it verbatim.
        let secret = "VTPASTED-SECRET-KEY-9f8e7d6c";
        fs::write(&path, serde_json::to_string(&secret).unwrap()).unwrap();

        let message = read_store_file(&path).unwrap_err();

        assert!(
            message.contains("Secure store"),
            "unexpected message: {message}"
        );
        assert!(
            !message.contains(secret),
            "payload leaked into error: {message}"
        );
        assert!(
            !message.contains("invalid type"),
            "raw serde error leaked: {message}"
        );
    }

    #[test]
    fn corrupt_value_read_preserves_saved_record_and_reports_error() {
        initialize_encryption_key().unwrap();
        let dir = TempDir::new().unwrap();
        let tampered = tamper_ciphertext(&encrypt_value("VTLICENSE-ABCD-1234").unwrap());
        let path = write_store_file(&dir, &serde_json::json!({ "license": tampered }));
        let before = read_back(&path);

        let disk = read_store_file(&path).unwrap().expect("file parses");
        let result = decrypt_raw_entry("license", disk.get("license"));

        assert!(result.is_err(), "corrupt value must not read as absent");
        let message = result.unwrap_err();
        assert!(
            message.contains("could not be decrypted") && message.contains("preserved"),
            "unexpected message: {message}"
        );
        // The saved record is byte-identical after the failed read.
        assert_eq!(read_back(&path), before);
    }

    #[test]
    fn repeat_read_keeps_preserved_record_and_same_error() {
        initialize_encryption_key().unwrap();
        let dir = TempDir::new().unwrap();
        let tampered = tamper_ciphertext(&encrypt_value("VTLICENSE-ABCD-1234").unwrap());
        let path = write_store_file(&dir, &serde_json::json!({ "license": tampered }));
        let before = read_back(&path);

        let first_disk = read_store_file(&path).unwrap().expect("file parses");
        let first = decrypt_raw_entry("license", first_disk.get("license")).unwrap_err();
        let second_disk = read_store_file(&path).unwrap().expect("file parses");
        let second = decrypt_raw_entry("license", second_disk.get("license")).unwrap_err();

        assert_eq!(first, second);
        assert_eq!(read_back(&path), before);
    }

    #[test]
    fn invalid_value_type_preserves_saved_record() {
        let dir = TempDir::new().unwrap();
        let path = write_store_file(&dir, &serde_json::json!({ "license": 12345 }));
        let before = read_back(&path);

        let disk = read_store_file(&path).unwrap().expect("file parses");
        let result = decrypt_raw_entry("license", disk.get("license"));

        assert!(result.is_err(), "invalid type must not read as absent");
        assert!(result.unwrap_err().contains("unexpected format"));
        assert_eq!(read_back(&path), before);
    }

    #[test]
    fn valid_license_roundtrip_reads_back_from_file() {
        initialize_encryption_key().unwrap();
        let dir = TempDir::new().unwrap();
        let encrypted = encrypt_value("VTLICENSE-ABCD-1234").unwrap();
        let path = write_store_file(&dir, &serde_json::json!({ "license": encrypted }));

        let disk = read_store_file(&path).unwrap().expect("file parses");

        assert_eq!(
            decrypt_raw_entry("license", disk.get("license")).unwrap(),
            Some("VTLICENSE-ABCD-1234".to_string())
        );
        // A key that is not in the file reads as absent, not as an error.
        assert_eq!(
            decrypt_raw_entry("other_key", disk.get("other_key")).unwrap(),
            None
        );
    }
}
