use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

#[cfg(target_os = "macos")]
#[path = "application_icon.rs"]
mod application_icon;

#[tauri::command]
pub async fn get_application_icon(
    app: AppHandle,
    process_path: String,
) -> Result<Option<String>, String> {
    #[cfg(target_os = "macos")]
    {
        application_icon::load(app, process_path).await
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, process_path);
        Ok(None)
    }
}

#[tauri::command]
pub async fn export_transcriptions(app: AppHandle) -> Result<String, String> {
    use std::fs;

    log::info!("Exporting transcriptions to JSON");

    // Get transcription history from the store
    let store = app.store("transcriptions").map_err(|e| e.to_string())?;

    let mut entries: Vec<(String, serde_json::Value)> = Vec::new();

    // Collect all entries with their timestamps
    for key in store.keys() {
        if let Some(value) = store.get(&key) {
            entries.push((key.to_string(), value));
        }
    }

    // Sort by timestamp (newest first)
    entries.sort_by(|a, b| b.0.cmp(&a.0));

    let history: Vec<serde_json::Value> = entries.into_iter().map(|(_, v)| v).collect();

    if history.is_empty() {
        return Err("No transcriptions to export".to_string());
    }

    // Create export data structure
    let export_data = serde_json::json!({
        "app": "Voicetypr",
        "exportDate": chrono::Utc::now().to_rfc3339(),
        "totalTranscriptions": history.len(),
        "transcriptions": history
    });

    // Get the Downloads folder path
    let download_dir = if cfg!(target_os = "macos") {
        // macOS specific
        dirs::download_dir().or_else(|| dirs::home_dir().map(|h| h.join("Downloads")))
    } else {
        // Windows/Linux
        dirs::download_dir()
    };

    let download_path =
        download_dir.ok_or_else(|| "Could not find Downloads folder".to_string())?;

    // Create filename with current date
    let filename = format!(
        "voicetypr-transcriptions-{}.json",
        chrono::Local::now().format("%Y-%m-%d")
    );

    let file_path = download_path.join(&filename);

    // Write to file with pretty formatting
    let json_string = serde_json::to_string_pretty(&export_data)
        .map_err(|e| format!("Failed to serialize data: {}", e))?;

    fs::write(&file_path, json_string).map_err(|e| format!("Failed to write file: {}", e))?;

    log::info!(
        "Exported {} transcriptions to {:?}",
        history.len(),
        file_path
    );

    // Return the full path as string
    Ok(file_path.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn save_transcript_file(path: String, content: String) -> Result<(), String> {
    if path.trim().is_empty() {
        return Err("No file path provided".to_string());
    }
    if content.is_empty() {
        return Err("Nothing to save".to_string());
    }
    std::fs::write(&path, content).map_err(|e| format!("Failed to write file: {}", e))?;
    log::info!("Saved transcript to {}", path);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn save_transcript_file_writes_content() {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "voicetypr_test_{}.txt",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let content = "Hello, transcript!".to_string();
        let path_str = path.to_string_lossy().to_string();

        let result = save_transcript_file(path_str.clone(), content.clone()).await;
        assert!(result.is_ok(), "expected Ok, got: {:?}", result);

        let written = std::fs::read_to_string(&path).expect("file should exist after write");
        assert_eq!(written, content);

        std::fs::remove_file(&path).ok();
    }
}
