use crate::secure_store;
use tauri::AppHandle;

/// Validate key names to prevent edge cases and security issues
fn validate_key(key: &str) -> Result<(), String> {
    // Values only the app itself may change: the try count and the trial clock.
    if [
        crate::license::access::TRY_KEY,
        crate::license::trial_clock::KEY,
    ]
    .contains(&key)
    {
        return Err("This key is managed by Voicetypr".to_string());
    }
    // Check if key is empty
    if key.is_empty() {
        return Err("Key cannot be empty".to_string());
    }

    // Check key length (reasonable limit)
    if key.len() > 256 {
        return Err("Key name too long (max 256 characters)".to_string());
    }

    // Check for path traversal attempts
    if key.contains("..") || key.contains("/") || key.contains("\\") {
        return Err("Key cannot contain path separators or '..'".to_string());
    }

    // Check for null bytes
    if key.contains('\0') {
        return Err("Key cannot contain null bytes".to_string());
    }

    // Check for control characters
    if key.chars().any(|c| c.is_control() && c != '\t') {
        return Err("Key cannot contain control characters".to_string());
    }

    Ok(())
}

#[tauri::command]
pub fn keyring_set(app: AppHandle, key: String, value: String) -> Result<(), String> {
    // Validate key first
    validate_key(&key)?;

    // Validate value isn't too large (10MB limit)
    if value.len() > 10 * 1024 * 1024 {
        return Err("Value too large (max 10MB)".to_string());
    }

    // Save to secure store
    secure_store::secure_set(&app, &key, &value)?;
    if key.starts_with("stt_api_key_") {
        crate::menu::runtime::refresh(&app);
    }
    log::info!("Saved to secure store: {}", key);
    Ok(())
}

#[tauri::command]
pub fn keyring_get(app: AppHandle, key: String) -> Result<Option<String>, String> {
    // Validate key first
    validate_key(&key)?;

    // Get from secure store
    secure_store::secure_get(&app, &key)
}

#[tauri::command]
pub fn keyring_delete(app: AppHandle, key: String) -> Result<(), String> {
    // Validate key first
    validate_key(&key)?;

    // Delete from secure store
    secure_store::secure_delete(&app, &key)?;
    if key.starts_with("stt_api_key_") {
        crate::menu::runtime::refresh(&app);
    }
    log::info!("Deleted from secure store: {}", key);
    Ok(())
}

#[tauri::command]
pub fn keyring_has(app: AppHandle, key: String) -> Result<bool, String> {
    // Validate key first
    validate_key(&key)?;

    // Check secure store
    secure_store::secure_has(&app, &key)
}

#[cfg(test)]
mod tests {
    use super::validate_key;

    #[test]
    fn the_app_window_cannot_reset_the_try_count_or_the_trial_clock() {
        assert!(validate_key(crate::license::access::TRY_KEY).is_err());
        assert!(validate_key(crate::license::trial_clock::KEY).is_err());
        assert!(validate_key("stt_api_key_deepgram").is_ok());
    }
}
