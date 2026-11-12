use arboard;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use clipboard_rs::{common::RustImage, Clipboard, ClipboardContext};
use image;
use std::path::PathBuf;
use tokio::fs;

#[tauri::command]
pub async fn copy_image_to_clipboard(image_data_url: String) -> Result<(), String> {
    // Parse the data URL to get the base64 image data
    let base64_data = if image_data_url.starts_with("data:image/png;base64,") {
        image_data_url.replace("data:image/png;base64,", "")
    } else {
        return Err("Invalid image data URL".to_string());
    };

    // Decode base64 to PNG bytes
    let png_bytes = STANDARD
        .decode(&base64_data)
        .map_err(|e| format!("Failed to decode base64: {}", e))?;

    // Move to blocking task since clipboard operations are synchronous
    tokio::task::spawn_blocking(move || {
        // Try clipboard-rs first (preserves PNG format, better cross-platform support)
        match try_clipboard_rs(&png_bytes) {
            Ok(()) => {
                log::info!("Successfully copied PNG image using clipboard-rs");
                return Ok(());
            }
            Err(e) => {
                log::warn!("clipboard-rs failed: {}, trying arboard fallback", e);
            }
        }

        // Fallback to arboard if clipboard-rs fails
        try_arboard(&png_bytes)
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?
}

fn try_clipboard_rs(png_bytes: &[u8]) -> Result<(), String> {
    // Create clipboard context
    let ctx = ClipboardContext::new()
        .map_err(|e| format!("Failed to create clipboard context: {:?}", e))?;

    // Create RustImage directly from PNG bytes
    // RustImage handles the image decoding internally
    let rust_image = RustImage::from_bytes(png_bytes)
        .map_err(|e| format!("Failed to create RustImage from PNG bytes: {:?}", e))?;

    // Set the image to clipboard
    ctx.set_image(rust_image)
        .map_err(|e| format!("Failed to set image to clipboard: {:?}", e))?;

    Ok(())
}

fn try_arboard(png_bytes: &[u8]) -> Result<(), String> {
    // Load image from PNG bytes
    let img =
        image::load_from_memory(png_bytes).map_err(|e| format!("Failed to load image: {}", e))?;

    let rgba_image = img.to_rgba8();
    let (width, height) = (rgba_image.width() as usize, rgba_image.height() as usize);
    let raw_bytes = rgba_image.into_raw();

    // Initialize arboard clipboard
    let mut clipboard = arboard::Clipboard::new()
        .map_err(|e| format!("Failed to initialize arboard clipboard: {}", e))?;

    let image_data = arboard::ImageData {
        width,
        height,
        bytes: raw_bytes.into(),
    };

    // Platform-specific handling for Linux
    #[cfg(target_os = "linux")]
    {
        use arboard::SetExtLinux;
        clipboard
            .set_image(image_data)
            .wait() // On Linux, wait to ensure clipboard persists
            .map_err(|e| format!("Failed to set image with arboard: {}", e))?;
    }

    #[cfg(not(target_os = "linux"))]
    {
        clipboard
            .set_image(image_data)
            .map_err(|e| format!("Failed to set image with arboard: {}", e))?;
    }

    log::info!("Successfully copied image using arboard fallback (RGBA format)");
    Ok(())
}

#[tauri::command]
pub async fn save_image_to_file(image_data_url: String, file_path: String) -> Result<(), String> {
    // Parse the data URL to get the base64 image data
    let base64_data = if image_data_url.starts_with("data:image/png;base64,") {
        image_data_url.replace("data:image/png;base64,", "")
    } else {
        return Err("Invalid image data URL".to_string());
    };

    // Decode base64 to bytes
    let image_bytes = STANDARD
        .decode(&base64_data)
        .map_err(|e| format!("Failed to decode base64: {}", e))?;

    // Convert string path to PathBuf
    let path = PathBuf::from(file_path);

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("Failed to create directories: {}", e))?;
    }

    // Write the image bytes to the file
    fs::write(&path, image_bytes)
        .await
        .map_err(|e| format!("Failed to write file: {}", e))?;

    log::info!("Successfully saved image");
    Ok(())
}
