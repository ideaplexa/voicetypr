use tauri::AppHandle;

#[tauri::command]
#[allow(non_snake_case)]
pub async fn validate_stt_key(
    provider: String,
    api_key: Option<String>,
    apiKey: Option<String>,
) -> Result<(), String> {
    let key = apiKey.or(api_key).unwrap_or_default();
    let p = crate::cloud_stt::CloudProvider::from_id(&provider)
        .ok_or_else(|| format!("Unknown cloud STT provider: {}", provider))?;
    p.validate_key(&key).await
}

#[tauri::command]
pub async fn clear_stt_key_cache(_app: AppHandle, _provider: String) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub async fn get_soniox_storage_counts(
    app: AppHandle,
) -> Result<crate::cloud_stt::SonioxStorageCounts, String> {
    crate::cloud_stt::storage_counts(&app).await
}

#[tauri::command]
pub async fn cleanup_soniox_storage(
    app: AppHandle,
) -> Result<crate::cloud_stt::SonioxCleanupResult, String> {
    crate::cloud_stt::cleanup_stored(&app).await
}
