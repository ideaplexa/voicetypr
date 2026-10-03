//! Opaque app identity keys. Paths remain exclusively in this in-memory registry.
use sha2::{Digest, Sha256};
use std::{collections::HashMap, sync::Mutex};
use tauri::{Manager, WebviewWindow};
#[derive(Default)]
pub struct IconCache(Mutex<HashMap<String, IconEntry>>);
struct IconEntry {
    path: String,
    image: Option<Option<String>>,
}

pub fn register(app: &tauri::AppHandle, path: &std::path::Path) -> Option<String> {
    if !path.is_absolute() {
        return None;
    }
    #[cfg(target_os = "macos")]
    let path = path
        .ancestors()
        .find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("app")))?;
    let path = path.to_str()?.to_owned();
    #[cfg(target_os = "macos")]
    let identity =
        objc2_foundation::NSBundle::bundleWithPath(&objc2_foundation::NSString::from_str(&path))?
            .bundleIdentifier()?
            .to_string();
    #[cfg(not(target_os = "macos"))]
    let identity = &path;
    let key = hex::encode(Sha256::digest(identity.as_bytes()));
    let cache = app.state::<IconCache>();
    let mut entries = cache.0.lock().unwrap();
    // Bound memory when many different apps are used; failure falls back to a tile.
    if entries.len() >= 256 && !entries.contains_key(&key) {
        entries.clear();
    }
    entries
        .entry(key.clone())
        .or_insert(IconEntry { path, image: None });
    Some(key)
}
#[tauri::command]
pub async fn pill_app_icon(window: WebviewWindow, icon_key: String) -> Option<String> {
    if window.label() != "pill" {
        return None;
    }
    let app = window.app_handle().clone();
    let path = {
        let cache = app.state::<IconCache>();
        let entries = cache.0.lock().unwrap();
        let entry = entries.get(&icon_key)?;
        if let Some(image) = &entry.image {
            return image.clone();
        }
        entry.path.clone()
    };
    #[cfg(target_os = "macos")]
    let image = crate::commands::utils::get_application_icon(app.clone(), path)
        .await
        .ok()
        .flatten();
    #[cfg(target_os = "windows")]
    let image = tauri::async_runtime::spawn_blocking(move || windows_icon(&path))
        .await
        .ok()
        .flatten();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let image = {
        let _ = path;
        None
    };
    if let Some(entry) = app
        .state::<IconCache>()
        .0
        .lock()
        .unwrap()
        .get_mut(&icon_key)
    {
        entry.image = Some(image.clone());
    }
    image
}

#[cfg(target_os = "windows")]
fn windows_icon(path: &str) -> Option<String> {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use windows::{
        core::PCWSTR,
        Win32::{
            Graphics::Gdi::*,
            Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES,
            UI::{Shell::*, WindowsAndMessaging::*},
        },
    };
    let wide: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
    // All handles are released even when extraction/rendering fails.
    unsafe {
        let mut info = SHFILEINFOW::default();
        if SHGetFileInfoW(
            PCWSTR(wide.as_ptr()),
            FILE_FLAGS_AND_ATTRIBUTES(0),
            Some(&mut info),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_LARGEICON,
        ) == 0
        {
            return None;
        }
        let result = (|| {
            let dc = CreateCompatibleDC(None);
            if dc.0.is_null() {
                return None;
            }
            let mut bmi = BITMAPINFO::default();
            bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            bmi.bmiHeader.biWidth = 64;
            bmi.bmiHeader.biHeight = -64;
            bmi.bmiHeader.biPlanes = 1;
            bmi.bmiHeader.biBitCount = 32;
            let mut bits = std::ptr::null_mut();
            let bitmap = match CreateDIBSection(Some(dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0)
            {
                Ok(bitmap) => bitmap,
                Err(_) => {
                    let _ = DeleteDC(dc);
                    return None;
                }
            };
            let previous = SelectObject(dc, HGDIOBJ(bitmap.0));
            std::ptr::write_bytes(bits, 0, 64 * 64 * 4);
            let drawn = DrawIconEx(dc, 0, 0, info.hIcon, 64, 64, 0, None, DI_NORMAL).is_ok();
            let mut pixels = std::slice::from_raw_parts(bits.cast::<u8>(), 64 * 64 * 4).to_vec();
            let has_alpha = pixels.chunks_exact(4).any(|p| p[3] != 0);
            // Older icons carry only an AND mask. Preserve their transparency.
            let mask_ok =
                has_alpha || DrawIconEx(dc, 0, 0, info.hIcon, 64, 64, 0, None, DI_MASK).is_ok();
            if !has_alpha && mask_ok {
                let mask = std::slice::from_raw_parts(bits.cast::<u8>(), 64 * 64 * 4);
                for (pixel, mask_pixel) in pixels.chunks_exact_mut(4).zip(mask.chunks_exact(4)) {
                    pixel[3] = 255 - mask_pixel[0];
                }
            }
            SelectObject(dc, previous);
            let _ = DeleteObject(HGDIOBJ(bitmap.0));
            let _ = DeleteDC(dc);
            if !drawn || !mask_ok {
                return None;
            }
            // DIBs are BGRA; modern Shell icons carry premultiplied alpha.
            for pixel in pixels.chunks_exact_mut(4) {
                pixel.swap(0, 2);
                if pixel[3] > 0 && pixel[3] < 255 {
                    for channel in 0..3 {
                        pixel[channel] =
                            (pixel[channel] as u32 * 255 / pixel[3] as u32).min(255) as u8;
                    }
                }
            }
            let image = image::RgbaImage::from_raw(64, 64, pixels)?;
            let mut png = std::io::Cursor::new(Vec::new());
            image.write_to(&mut png, image::ImageFormat::Png).ok()?;
            Some(format!(
                "data:image/png;base64,{}",
                STANDARD.encode(png.into_inner())
            ))
        })();
        let _ = DestroyIcon(info.hIcon);
        result
    }
}
