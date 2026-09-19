use crate::secure_store;
use tauri::AppHandle;

const LICENSE_KEY_NAME: &str = "license";

/// Save a license key to the secure store
pub fn save_license(app: &AppHandle, key: &str) -> Result<(), String> {
    secure_store::secure_set(app, LICENSE_KEY_NAME, key)?;
    log::info!("License saved to secure store successfully");
    Ok(())
}

/// Get the stored license key from the secure store
pub fn get_license(app: &AppHandle) -> Result<Option<String>, String> {
    match secure_store::secure_get(app, LICENSE_KEY_NAME).map_err(|error| {
        log::warn!("Saved license is unreadable: {}", error);
        "Your saved license could not be read. Try re-entering your existing key in License. If activation still fails, contact support without resetting app data. The saved entry has not been deleted."
            .to_string()
    })? {
        Some(license) => {
            log::info!("License retrieved from secure store");
            Ok(Some(license))
        }
        None => {
            log::debug!("No license found in secure store");
            Ok(None)
        }
    }
}

/// Delete the stored license key from the secure store
pub fn delete_license(app: &AppHandle) -> Result<(), String> {
    secure_store::secure_delete(app, LICENSE_KEY_NAME)?;
    log::info!("License deleted from secure store");
    Ok(())
}
