use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

#[cfg(target_os = "macos")]
fn application_icon_data_url(process_path: &str) -> Result<Option<String>, String> {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use objc2::AnyThread;
    use objc2_app_kit::{
        NSBitmapImageFileType, NSBitmapImageRep, NSDeviceRGBColorSpace, NSGraphicsContext,
        NSWorkspace,
    };
    use objc2_foundation::{NSDictionary, NSPoint, NSRect, NSSize, NSString};
    use std::{path::Path, ptr::NonNull};

    const ICON_SIZE: isize = 64;
    const MAX_PNG_BYTES: usize = 1024 * 1024;

    let path = Path::new(process_path);
    if !path.is_absolute()
        || !path.is_dir()
        || !path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("app"))
    {
        return Ok(None);
    }

    let icon = NSWorkspace::sharedWorkspace().iconForFile(&NSString::from_str(process_path));
    // Render a 64×64 RGBA surface so the icon stays sharp on Retina displays
    // while avoiding an app icon's full multi-representation TIFF payload.
    let Some(bitmap) = (unsafe {
        NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(),
            std::ptr::null_mut(),
            ICON_SIZE,
            ICON_SIZE,
            8,
            4,
            true,
            false,
            NSDeviceRGBColorSpace,
            0,
            0,
        )
    }) else {
        return Ok(None);
    };
    let Some(context) = NSGraphicsContext::graphicsContextWithBitmapImageRep(&bitmap) else {
        return Ok(None);
    };

    NSGraphicsContext::saveGraphicsState_class();
    NSGraphicsContext::setCurrentContext(Some(&context));
    icon.drawInRect(NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(ICON_SIZE as f64, ICON_SIZE as f64),
    ));
    context.flushGraphics();
    NSGraphicsContext::restoreGraphicsState_class();

    let properties = NSDictionary::new();
    let Some(png) = (unsafe {
        bitmap.representationUsingType_properties(NSBitmapImageFileType::PNG, &properties)
    }) else {
        return Ok(None);
    };
    let byte_count = png.length();
    if byte_count == 0 || byte_count > MAX_PNG_BYTES {
        return Ok(None);
    }

    let mut png_bytes = vec![0_u8; byte_count];
    let destination = NonNull::new(png_bytes.as_mut_ptr().cast())
        .ok_or_else(|| "Failed to allocate application icon buffer".to_string())?;
    // SAFETY: `destination` points to `byte_count` initialized, writable bytes
    // owned by `png_bytes` for the duration of this copy.
    unsafe {
        png.getBytes_length(destination, byte_count);
    }

    Ok(Some(format!(
        "data:image/png;base64,{}",
        STANDARD.encode(png_bytes)
    )))
}

#[tauri::command]
pub async fn get_application_icon(
    app: AppHandle,
    process_path: String,
) -> Result<Option<String>, String> {
    #[cfg(target_os = "macos")]
    {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        app.run_on_main_thread(move || {
            let _ = sender.send(application_icon_data_url(&process_path));
        })
        .map_err(|error| format!("Failed to schedule application icon lookup: {error}"))?;
        receiver
            .await
            .map_err(|_| "Application icon lookup was cancelled".to_string())?
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

    #[cfg(target_os = "macos")]
    #[test]
    fn application_icon_is_returned_as_png_data_url() {
        let icon = application_icon_data_url("/System/Library/CoreServices/Finder.app")
            .expect("Finder icon lookup should succeed")
            .expect("Finder should have an application icon");

        assert!(icon.starts_with("data:image/png;base64,"));
        assert!(icon.len() > "data:image/png;base64,".len());
    }
}
