//! Keep Windows licensing and legacy AES derivation independent of flaky WMI
//! discovery after bootstrap. The sidecar is user-bound DPAPI data; secure.dat
//! remains byte-for-byte unchanged by discovery/recovery.

use super::{decrypt_value_with_key, derive_legacy_key, read_store_file, SECURE_STORE_FILE};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const IDENTITY_FILE: &str = "device-identity.dpapi";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    version: u32,
    // First hash is the selected API identity and new-write encryption input.
    // Others authenticated existing entries, potentially from different launches.
    hashes: Vec<String>,
}

struct Identity {
    record: Record,
    keys: Vec<[u8; 32]>,
    path: PathBuf,
    persisted: Mutex<bool>,
    automatic_pin: bool,
}

impl Identity {
    fn load(
        directory: &Path,
        discover: impl FnOnce() -> Result<Vec<String>, String>,
        unprotect: impl FnOnce(&[u8]) -> Result<Vec<u8>, String>,
    ) -> Result<Self, String> {
        let path = directory.join(IDENTITY_FILE);
        match std::fs::read(&path) {
            Ok(bytes) => {
                let plaintext = unprotect(&bytes)?;
                let record: Record = serde_json::from_slice(&plaintext).map_err(|_| {
                    "Saved device identity is malformed; it was preserved".to_string()
                })?;
                if record.version != 1 || record.hashes.is_empty() || record.hashes.len() > 8 {
                    return Err(
                        "Saved device identity has an unsupported format; it was preserved"
                            .to_string(),
                    );
                }
                let keys = record
                    .hashes
                    .iter()
                    .map(|hash| derive_legacy_key(hash))
                    .collect::<Result<Vec<_>, _>>()?;
                log::info!(
                    "[DeviceID] Using protected saved identity; no hardware lookup required"
                );
                return Ok(Self {
                    record,
                    keys,
                    path,
                    persisted: Mutex::new(true),
                    automatic_pin: true,
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => {
                return Err("Saved device identity could not be read; it was preserved".to_string())
            }
        }

        // Do not register a store or choose a new persistent identity over a
        // malformed file. Inspection and AES authentication are strictly read-only.
        let store = read_store_file(&directory.join(SECURE_STORE_FILE))?.unwrap_or_default();
        let mut hashes = discover()?;
        hashes.dedup();
        if hashes.is_empty() || hashes.len() > 8 {
            return Err("No usable device identity candidates".to_string());
        }
        let candidates = hashes
            .iter()
            .map(|hash| derive_legacy_key(hash))
            .collect::<Result<Vec<_>, _>>()?;
        let license_match = store
            .get("license")
            .and_then(|v| v.as_str())
            .and_then(|value| {
                candidates
                    .iter()
                    .position(|key| decrypt_value_with_key(value, key).is_ok())
            });
        let primary = license_match.unwrap_or(0);
        let mut selected = vec![primary];
        for (index, key) in candidates.iter().enumerate() {
            if index != primary
                && store
                    .values()
                    .filter_map(|v| v.as_str())
                    .any(|value| decrypt_value_with_key(value, key).is_ok())
            {
                selected.push(index);
            }
        }
        let automatic_pin = !store.contains_key("license") || license_match.is_some();
        if !automatic_pin {
            log::warn!("Saved license did not authenticate under available identities; explicit activation required before identity persistence");
        } else if license_match.is_some() {
            log::info!("Saved license authenticated; retaining its legacy identity without changing secure.dat");
        }
        Ok(Self {
            record: Record {
                version: 1,
                hashes: selected.iter().map(|i| hashes[*i].clone()).collect(),
            },
            keys: selected.iter().map(|i| candidates[*i]).collect(),
            path,
            persisted: Mutex::new(false),
            automatic_pin,
        })
    }

    fn persist(
        &self,
        explicit_license_write: bool,
        protect: impl FnOnce(&[u8]) -> Result<Vec<u8>, String>,
    ) -> Result<(), String> {
        let mut persisted = self
            .persisted
            .lock()
            .map_err(|_| "Device identity lock failed")?;
        if *persisted {
            return Ok(());
        }
        if !self.automatic_pin && !explicit_license_write {
            return Err("Recover the saved license before writing secure credentials".to_string());
        }
        let plaintext =
            serde_json::to_vec(&self.record).map_err(|_| "Device identity serialization failed")?;
        let protected = protect(&plaintext)?;
        let directory = self
            .path
            .parent()
            .ok_or("Device identity directory is missing")?;
        std::fs::create_dir_all(directory)
            .map_err(|_| "Could not create device identity directory")?;
        let mut temporary = tempfile::NamedTempFile::new_in(directory)
            .map_err(|_| "Could not prepare device identity file")?;
        temporary
            .write_all(&protected)
            .and_then(|_| temporary.as_file().sync_all())
            .map_err(|_| "Could not write device identity file")?;
        // Never overwrite another instance's identity, or a damaged existing pin.
        temporary
            .persist_noclobber(&self.path)
            .map_err(|_| "Could not persist device identity; existing data was preserved")?;
        *persisted = true;
        Ok(())
    }
}

#[cfg(target_os = "windows")]
static IDENTITY: once_cell::sync::OnceCell<Identity> = once_cell::sync::OnceCell::new();

#[cfg(all(target_os = "windows", test))]
pub(super) fn initialize_test_identity() -> Result<(), String> {
    // Existing secure-store tests use mock apps. Give their process a synthetic
    // identity through the real protection/persistence path, never the user's
    // application directory or hardware. The temporary bootstrap file can be
    // removed once loaded; these fixtures test store operations in this process.
    IDENTITY
        .get_or_try_init(|| {
            let directory = tempfile::tempdir().map_err(|_| "test directory failed")?;
            let identity = Identity::load(
                directory.path(),
                || Ok(vec!["0".repeat(64)]),
                |bytes| dpapi(bytes, false),
            )?;
            identity.persist(false, |bytes| dpapi(bytes, true))?;
            Ok::<_, String>(identity)
        })
        .map(|_| ())
}

#[cfg(target_os = "windows")]
pub(crate) fn initialize(directory: &Path) -> Result<(), String> {
    IDENTITY
        .get_or_try_init(|| {
            let identity = Identity::load(
                directory,
                crate::license::device::discover_windows_device_hashes,
                |bytes| dpapi(bytes, false),
            )?;
            if identity.automatic_pin {
                identity.persist(false, |bytes| dpapi(bytes, true))?;
            }
            Ok::<_, String>(identity)
        })
        .map(|_| ())
}

#[cfg(target_os = "windows")]
fn current() -> Result<&'static Identity, String> {
    IDENTITY.get().ok_or_else(|| "Device identity is unavailable. Saved data was preserved; restart or contact support without resetting app data.".to_string())
}

#[cfg(target_os = "windows")]
pub(crate) fn device_hash() -> Result<String, String> {
    Ok(current()?.record.hashes[0].clone())
}

#[cfg(target_os = "windows")]
pub(crate) fn keys() -> Result<&'static [[u8; 32]], String> {
    Ok(&current()?.keys)
}

#[cfg(target_os = "windows")]
pub(crate) fn ensure_persisted(key: &str) -> Result<(), String> {
    current()?.persist(key == "license", |bytes| dpapi(bytes, true))
}

#[cfg(target_os = "windows")]
fn dpapi(bytes: &[u8], protect: bool) -> Result<Vec<u8>, String> {
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };
    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes
            .len()
            .try_into()
            .map_err(|_| "Identity data is too large")?,
        pbData: bytes.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    // SAFETY: input borrows live bytes for these synchronous calls. DPAPI owns
    // output, which is copied then freed with LocalFree. No machine scope or
    // hardware-dependent entropy: protection is bound to the current user.
    unsafe {
        let result = if protect {
            CryptProtectData(
                &input,
                windows::core::PCWSTR::null(),
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                None,
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        result.map_err(|_| "Windows could not protect/read the saved device identity; existing data was preserved".to_string())?;
        let bytes = if output.cbData == 0 {
            Vec::new()
        } else {
            std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec()
        };
        let _ = LocalFree(Some(HLOCAL(output.pbData.cast())));
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::super::encrypt_value_with_key;
    use super::*;
    use sha2::{Digest, Sha256};

    fn hash(name: &str) -> String {
        format!("{:x}", Sha256::digest(name.as_bytes()))
    }
    fn encrypt(value: &str, hash: &str) -> String {
        encrypt_value_with_key(value, &derive_legacy_key(hash).unwrap()).unwrap()
    }
    // Windows runs exercise real user-bound DPAPI. Other hosts exercise the
    // same persistence/selection contract with an authenticated test envelope.
    fn protect(bytes: &[u8]) -> Result<Vec<u8>, String> {
        #[cfg(target_os = "windows")]
        return dpapi(bytes, true);
        #[cfg(not(target_os = "windows"))]
        encrypt_value_with_key(std::str::from_utf8(bytes).unwrap(), &[73; 32])
            .map(String::into_bytes)
    }
    fn unprotect(bytes: &[u8]) -> Result<Vec<u8>, String> {
        #[cfg(target_os = "windows")]
        return dpapi(bytes, false);
        #[cfg(not(target_os = "windows"))]
        decrypt_value_with_key(
            std::str::from_utf8(bytes).map_err(|_| "invalid test envelope")?,
            &[73; 32],
        )
        .map(String::into_bytes)
    }

    #[test]
    fn source_drift_breaks_legacy_cipher_but_protected_identity_survives_restart() {
        const CHILD_DIR: &str = "VOICETYPR_IDENTITY_RESTART_TEST";
        if let Some(directory) = std::env::var_os(CHILD_DIR) {
            let directory = PathBuf::from(directory);
            let identity =
                Identity::load(&directory, || panic!("child must not discover"), unprotect)
                    .unwrap();
            assert_eq!(identity.record.hashes[0], hash("registry-guid"));
            let store = read_store_file(&directory.join(SECURE_STORE_FILE))
                .unwrap()
                .unwrap();
            assert_eq!(
                decrypt_value_with_key(store["license"].as_str().unwrap(), &identity.keys[0])
                    .unwrap(),
                "VT-SYNTHETIC-LICENSE"
            );
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let hardware = hash("hardware-uuid");
        let registry = hash("registry-guid");
        let cipher = encrypt("VT-SYNTHETIC-LICENSE", &registry);
        assert!(decrypt_value_with_key(&cipher, &derive_legacy_key(&hardware).unwrap()).is_err());
        let original = serde_json::to_vec(&serde_json::json!({"license": cipher})).unwrap();
        let store_path = dir.path().join(SECURE_STORE_FILE);
        std::fs::write(&store_path, &original).unwrap();
        let identity = Identity::load(
            dir.path(),
            || Ok(vec![hardware, registry.clone()]),
            unprotect,
        )
        .unwrap();
        assert_eq!(identity.record.hashes[0], registry);
        identity.persist(false, protect).unwrap();
        drop(identity);
        let restarted = Identity::load(
            dir.path(),
            || panic!("restart must not rediscover hardware"),
            unprotect,
        )
        .unwrap();
        assert_eq!(restarted.record.hashes[0], registry);
        assert_eq!(
            decrypt_value_with_key(&cipher, &restarted.keys[0]).unwrap(),
            "VT-SYNTHETIC-LICENSE"
        );
        assert_eq!(std::fs::read(store_path).unwrap(), original);
        assert!(
            !String::from_utf8_lossy(&std::fs::read(dir.path().join(IDENTITY_FILE)).unwrap())
                .contains(&registry)
        );
        // A separate process proves no static encryption key or identity cache
        // from the writing process is needed to read the saved entry.
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "secure_store::windows_identity::tests::source_drift_breaks_legacy_cipher_but_protected_identity_survives_restart"])
            .env(CHILD_DIR, dir.path()).output().unwrap();
        assert!(
            child.status.success(),
            "{}",
            String::from_utf8_lossy(&child.stderr)
        );
        assert!(String::from_utf8_lossy(&child.stdout).contains("1 passed"));
    }

    #[test]
    fn mixed_keys_are_authenticated_per_entry_and_retained() {
        let dir = tempfile::tempdir().unwrap();
        let a = hash("a");
        let b = hash("b");
        let unrelated = hash("unrelated");
        let license = encrypt("license-a", &a);
        let api_key = encrypt("api-b", &b);
        std::fs::write(
            dir.path().join(SECURE_STORE_FILE),
            serde_json::to_vec(&serde_json::json!({"license": license, "api": api_key})).unwrap(),
        )
        .unwrap();
        let identity = Identity::load(
            dir.path(),
            || Ok(vec![unrelated, b.clone(), a.clone()]),
            unprotect,
        )
        .unwrap();
        assert_eq!(identity.record.hashes, vec![a, b]);
        identity.persist(false, protect).unwrap();
        let restarted = Identity::load(dir.path(), || panic!("no discovery"), unprotect).unwrap();
        assert_eq!(
            decrypt_value_with_key(&api_key, &restarted.keys[1]).unwrap(),
            "api-b"
        );
    }

    #[test]
    fn unknown_key_is_preserved_and_requires_explicit_license_replacement_to_pin() {
        let dir = tempfile::tempdir().unwrap();
        let original = serde_json::to_vec(
            &serde_json::json!({"license": encrypt("old-license", &hash("unavailable"))}),
        )
        .unwrap();
        std::fs::write(dir.path().join(SECURE_STORE_FILE), &original).unwrap();
        let identity = Identity::load(dir.path(), || Ok(vec![hash("current")]), unprotect).unwrap();
        assert!(!identity.automatic_pin);
        assert!(identity.persist(false, protect).is_err());
        assert!(!dir.path().join(IDENTITY_FILE).exists());
        assert_eq!(
            std::fs::read(dir.path().join(SECURE_STORE_FILE)).unwrap(),
            original
        );
        identity.persist(true, protect).unwrap();
        assert_eq!(
            std::fs::read(dir.path().join(SECURE_STORE_FILE)).unwrap(),
            original
        );
    }

    #[test]
    fn broken_pin_and_malformed_store_never_trigger_discovery_or_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let pin = dir.path().join(IDENTITY_FILE);
        std::fs::write(&pin, b"unreadable pin").unwrap();
        assert!(Identity::load(dir.path(), || panic!("must not replace pin"), unprotect).is_err());
        assert_eq!(std::fs::read(&pin).unwrap(), b"unreadable pin");
        std::fs::remove_file(pin).unwrap();
        std::fs::write(dir.path().join(SECURE_STORE_FILE), b"{ broken").unwrap();
        assert!(Identity::load(
            dir.path(),
            || panic!("must not choose over broken store"),
            unprotect
        )
        .is_err());
        assert_eq!(
            std::fs::read(dir.path().join(SECURE_STORE_FILE)).unwrap(),
            b"{ broken"
        );
    }

    #[test]
    fn unsupported_or_invalid_protected_records_are_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(IDENTITY_FILE);
        for record in [
            serde_json::json!({"version": 2, "hashes": [hash("a")]}),
            serde_json::json!({"version": 1, "hashes": []}),
            serde_json::json!({"version": 1, "hashes": ["bad hash"]}),
        ] {
            let bytes = protect(&serde_json::to_vec(&record).unwrap()).unwrap();
            std::fs::write(&path, &bytes).unwrap();
            assert!(Identity::load(
                dir.path(),
                || panic!("invalid pin is not absence"),
                unprotect
            )
            .is_err());
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
        }
    }

    #[test]
    fn protection_or_commit_failure_does_not_mark_identity_persisted() {
        let dir = tempfile::tempdir().unwrap();
        let identity =
            Identity::load(dir.path(), || Ok(vec![hash("new-install")]), unprotect).unwrap();
        assert!(identity
            .persist(false, |_| Err("protection failure".to_string()))
            .is_err());
        assert!(!*identity.persisted.lock().unwrap());
        std::fs::write(&identity.path, b"other instance").unwrap();
        assert!(identity.persist(false, protect).is_err());
        assert!(!*identity.persisted.lock().unwrap());
        assert_eq!(std::fs::read(&identity.path).unwrap(), b"other instance");
    }
}
