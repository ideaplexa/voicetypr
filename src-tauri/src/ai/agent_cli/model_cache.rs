//! Session catalog for pi/omp. No TTL: Refresh, binary changes and model/auth
//! failures are the only invalidations. The caller holds MODEL_DISCOVERY_PERMIT.
use super::*;
use parking_lot::Mutex;
use std::time::SystemTime;

#[derive(PartialEq, Eq)]
struct Executable {
    path: PathBuf,
    len: u64,
    modified: SystemTime,
}

struct Entry {
    executable: Executable,
    models: Vec<AgentCliModel>,
}

#[derive(Default)]
struct Cache {
    epoch: u64,
    entries: HashMap<&'static str, Entry>,
}

static CACHE: LazyLock<Mutex<Cache>> = LazyLock::new(Default::default);

pub(super) fn invalidate(provider: &str) {
    let mut cache = CACHE.lock();
    cache.epoch = cache.epoch.wrapping_add(1);
    cache.entries.remove(provider);
}

pub(super) async fn list(
    spec: &'static AgentCliSpec,
    binary: &Path,
) -> Result<Vec<AgentCliModel>, MappedAiProviderError> {
    let metadata = tokio::fs::metadata(binary).await.ok();
    // If identity cannot be established, don't reuse or populate the cache.
    let executable = metadata.and_then(|metadata| {
        Some(Executable {
            path: binary.to_path_buf(),
            len: metadata.len(),
            modified: metadata.modified().ok()?,
        })
    });
    let epoch = {
        let cache = CACHE.lock();
        if let Some(entry) = cache.entries.get(spec.provider_id) {
            if executable.as_ref() == Some(&entry.executable) {
                return Ok(entry.models.clone());
            }
        }
        cache.epoch
    };
    let models = match spec.provider_id {
        PROVIDER_PI => {
            let payload = run_pi_model_listing(binary).await?;
            let mut models = parse_pi_models(&payload)?;
            if let Ok(default) = parse_pi_default_model(&payload) {
                models.insert(0, default);
            }
            models
        }
        PROVIDER_OMP => {
            let payload = run_model_list_command(binary, OMP_MODEL_LIST_ARGV, &[]).await?;
            let mut models = parse_omp_models(&payload)?;
            let default = run_model_list_command(binary, OMP_DEFAULT_MODEL_ARGV, &[]).await?;
            if let Ok(default) = parse_omp_default_model(&default, &models) {
                models.insert(0, default);
            }
            models
        }
        _ => {
            return Err(MappedAiProviderError::new(
                AiProviderError::UnsupportedProvider,
            ))
        }
    };
    let mut cache = CACHE.lock();
    // A Refresh/failure during discovery must not repopulate an obsolete list.
    if cache.epoch == epoch {
        if let Some(executable) = executable {
            cache.entries.insert(
                spec.provider_id,
                Entry {
                    executable,
                    models: models.clone(),
                },
            );
        }
    }
    Ok(models)
}

pub(super) fn is_model_or_auth_failure(error: &MappedAiProviderError) -> bool {
    match &error.error {
        AiProviderError::InvalidModel
        | AiProviderError::InvalidApiKey
        | AiProviderError::MissingApiKey => true,
        AiProviderError::AgentCli(message) => {
            let message = message.to_ascii_lowercase();
            [
                "model",
                "auth",
                "api key",
                "api_key",
                "login",
                "log in",
                "logged in",
                "credential",
                "401",
                "403",
            ]
            .iter()
            .any(|needle| message.contains(needle))
        }
        _ => false,
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn pi_binary(dir: &TempDir, wait: bool) -> PathBuf {
        let binary = dir.path().join("pi");
        let script = format!(
            r#"#!/bin/sh
printf call >> '{counter}'
IFS= read -r state
IFS= read -r models
{barrier}
printf '%s\n' '{{"id":"voicetypr-state","data":{{"model":{{"provider":"test","id":"model","name":"Test"}}}}}}'
printf '%s\n' '{{"id":"voicetypr-models","data":{{"models":[{{"provider":"test","id":"model","name":"Test"}}]}}}}'
while IFS= read -r input; do :; done
"#,
            counter = dir.path().join("calls").display(),
            barrier = if wait {
                format!(
                    "touch '{}'\nwhile [ ! -f '{}' ]; do sleep 0.01; done",
                    dir.path().join("ready").display(),
                    dir.path().join("release").display()
                )
            } else {
                String::new()
            },
        );
        std::fs::write(&binary, script).unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
        binary
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn pi_catalog_caches_default_and_models_until_refresh_or_binary_change() {
        let _permit = MODEL_DISCOVERY_PERMIT.acquire().await.unwrap();
        let dir = TempDir::new().unwrap();
        let binary = pi_binary(&dir, false);
        let calls = dir.path().join("calls");
        invalidate(PROVIDER_PI);
        let first = list(&PI_SPEC, &binary).await.unwrap();
        assert_eq!(first.len(), 2);
        assert!(first[0].cli_default);
        assert_eq!(first[1].id, "test/model");
        list(&PI_SPEC, &binary).await.unwrap();
        assert_eq!(std::fs::read(&calls).unwrap().len(), 4);
        invalidate_polish_capabilities(PROVIDER_PI).await;
        list(&PI_SPEC, &binary).await.unwrap();
        assert_eq!(std::fs::read(&calls).unwrap().len(), 8);
        use std::io::Write;
        std::fs::OpenOptions::new()
            .append(true)
            .open(&binary)
            .unwrap()
            .write_all(b"\n# updated binary\n")
            .unwrap();
        list(&PI_SPEC, &binary).await.unwrap();
        assert_eq!(std::fs::read(&calls).unwrap().len(), 12);
        let replacement = dir.path().join("replacement");
        std::fs::copy(&binary, &replacement).unwrap();
        list(&PI_SPEC, &replacement).await.unwrap();
        assert_eq!(std::fs::read(&calls).unwrap().len(), 16);
        invalidate(PROVIDER_PI);
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn refresh_during_discovery_does_not_repopulate_obsolete_catalog() {
        let _permit = MODEL_DISCOVERY_PERMIT.acquire().await.unwrap();
        let dir = TempDir::new().unwrap();
        let binary = pi_binary(&dir, true);
        invalidate(PROVIDER_PI);
        let discovery_binary = binary.clone();
        let discovery = tokio::spawn(async move { list(&PI_SPEC, &discovery_binary).await });
        tokio::time::timeout(Duration::from_secs(5), async {
            while !dir.path().join("ready").exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        invalidate_polish_capabilities(PROVIDER_PI).await;
        std::fs::write(dir.path().join("release"), b"").unwrap();
        assert_eq!(discovery.await.unwrap().unwrap().len(), 2);
        assert!(!CACHE.lock().entries.contains_key(PROVIDER_PI));
        list(&PI_SPEC, &binary).await.unwrap();
        assert_eq!(std::fs::read(dir.path().join("calls")).unwrap().len(), 8);
        assert!(CACHE.lock().entries.contains_key(PROVIDER_PI));
        invalidate(PROVIDER_PI);
    }
}
