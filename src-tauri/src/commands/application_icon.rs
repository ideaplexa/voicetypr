//! AppKit only rasterizes on the main thread; PNG/base64 and cache I/O run on workers.
use std::{
    collections::HashMap,
    path::Path,
    sync::Mutex,
    time::{Instant, SystemTime},
};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use once_cell::sync::Lazy;

const ICON_SIZE: u32 = 64;
const PIXEL_BYTES: usize = ICON_SIZE as usize * ICON_SIZE as usize * 4;
const CACHE_LIMIT: usize = 128;

#[derive(Clone, PartialEq, Eq, Hash)]
struct IconKey {
    path: String,
    modified: Option<SystemTime>,
}

#[derive(Default)]
struct EncodedIcons(HashMap<IconKey, String>);

impl EncodedIcons {
    fn insert(&mut self, key: IconKey, image: String) {
        // Discard old versions and bound memory without persistent user/app data.
        self.0.retain(|old, _| old.path != key.path);
        if self.0.len() >= CACHE_LIMIT {
            self.0.clear();
        }
        self.0.insert(key, image);
    }
}

static CACHE: Lazy<Mutex<EncodedIcons>> = Lazy::new(|| Mutex::new(EncodedIcons::default()));

fn icon_key(process_path: String) -> Option<IconKey> {
    let path = Path::new(&process_path);
    if !path.is_absolute()
        || !path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("app"))
    {
        return None;
    }
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_dir() {
        return None;
    }
    Some(IconKey {
        path: process_path,
        modified: metadata.modified().ok(),
    })
}

/// No AppKit objects cross a thread boundary: only a bounded 16 KiB pixel copy.
fn rasterize(process_path: &str, _main_thread: objc2::MainThreadMarker) -> Option<Vec<u8>> {
    use objc2::AnyThread;
    use objc2_app_kit::{
        NSBitmapFormat, NSBitmapImageRep, NSDeviceRGBColorSpace, NSGraphicsContext, NSWorkspace,
    };
    use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

    let icon = NSWorkspace::sharedWorkspace().iconForFile(&NSString::from_str(process_path));
    // Explicit packed RGBA, alpha-last and premultiplied (AppKit's drawing format).
    let bitmap = unsafe {
        NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bitmapFormat_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(),
            std::ptr::null_mut(),
            ICON_SIZE as isize,
            ICON_SIZE as isize,
            8,
            4,
            true,
            false,
            NSDeviceRGBColorSpace,
            NSBitmapFormat::empty(),
            ICON_SIZE as isize * 4,
            32,
        )
    }?;
    let pixels = bitmap.bitmapData();
    if pixels.is_null() || bitmap.bytesPerRow() != ICON_SIZE as isize * 4 {
        return None;
    }
    // SAFETY: the bitmap owns a packed 64×64×4 surface, alive through the copy.
    unsafe { std::ptr::write_bytes(pixels, 0, PIXEL_BYTES) };
    let context = NSGraphicsContext::graphicsContextWithBitmapImageRep(&bitmap)?;
    NSGraphicsContext::saveGraphicsState_class();
    NSGraphicsContext::setCurrentContext(Some(&context));
    icon.drawInRect(NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(ICON_SIZE as f64, ICON_SIZE as f64),
    ));
    context.flushGraphics();
    NSGraphicsContext::restoreGraphicsState_class();
    Some(unsafe { std::slice::from_raw_parts(pixels, PIXEL_BYTES) }.to_vec())
}

fn encode(mut pixels: Vec<u8>) -> Option<String> {
    if pixels.len() != PIXEL_BYTES {
        return None;
    }
    // PNG stores straight alpha; AppKit's surface is premultiplied.
    for pixel in pixels.chunks_exact_mut(4) {
        let alpha = u32::from(pixel[3]);
        if alpha > 0 && alpha < 255 {
            for channel in &mut pixel[..3] {
                *channel = ((u32::from(*channel) * 255 + alpha / 2) / alpha).min(255) as u8;
            }
        }
    }
    let image = image::RgbaImage::from_raw(ICON_SIZE, ICON_SIZE, pixels)?;
    let mut png = std::io::Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png).ok()?;
    Some(format!(
        "data:image/png;base64,{}",
        STANDARD.encode(png.into_inner())
    ))
}

pub(super) async fn load(
    app: tauri::AppHandle,
    process_path: String,
) -> Result<Option<String>, String> {
    let lookup = tauri::async_runtime::spawn_blocking(move || {
        let key = icon_key(process_path)?;
        let cached = CACHE
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .0
            .get(&key)
            .cloned();
        Some((key, cached))
    })
    .await
    .map_err(|_| "Application icon lookup failed")?;
    let Some((key, cached)) = lookup else {
        return Ok(None);
    };
    if cached.is_some() {
        return Ok(cached);
    }
    let path = key.path.clone();
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        let started = Instant::now();
        let pixels = objc2::MainThreadMarker::new().and_then(|mtm| rasterize(&path, mtm));
        let _ = sender.send((pixels, started.elapsed()));
    })
    .map_err(|_| "Failed to schedule application icon lookup")?;
    let (pixels, raster_time) = receiver
        .await
        .map_err(|_| "Application icon lookup was cancelled")?;
    let Some(pixels) = pixels else {
        return Ok(None);
    };
    tauri::async_runtime::spawn_blocking(move || {
        let started = Instant::now();
        let image = encode(pixels)?;
        log::debug!(
            "Application icon work: raster_us={}, encode_us={}",
            raster_time.as_micros(),
            started.elapsed().as_micros()
        );
        CACHE
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(key, image.clone());
        Some(image)
    })
    .await
    .map_err(|_| "Application icon encoding failed".to_string())
}

#[cfg(test)]
#[path = "application_icon_tests.rs"]
mod tests;
