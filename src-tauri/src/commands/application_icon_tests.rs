use super::*;
use std::time::Duration;

#[test]
fn encoded_png_round_trips_dimensions_colors_and_straight_alpha() {
    let mut pixels = vec![0; PIXEL_BYTES];
    pixels[..4].copy_from_slice(&[12, 34, 56, 255]);
    pixels[4..8].copy_from_slice(&[64, 32, 16, 128]);
    let data = encode(pixels).unwrap();
    let png = STANDARD
        .decode(data.strip_prefix("data:image/png;base64,").unwrap())
        .unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    let image = image::load_from_memory(&png).unwrap().to_rgba8();
    assert_eq!(image.dimensions(), (64, 64));
    assert_eq!(image.get_pixel(0, 0).0, [12, 34, 56, 255]);
    assert_eq!(image.get_pixel(1, 0).0, [128, 64, 32, 128]);
    assert_eq!(image.get_pixel(2, 0).0, [0, 0, 0, 0]);
    assert!(encode(vec![0; PIXEL_BYTES - 1]).is_none());
}

#[test]
fn cache_uses_path_and_mtime_drops_old_versions_and_is_bounded() {
    let mut cache = EncodedIcons::default();
    let key = IconKey {
        path: "/app".into(),
        modified: Some(SystemTime::UNIX_EPOCH),
    };
    cache.insert(key.clone(), "first".into());
    assert_eq!(cache.0.get(&key).map(String::as_str), Some("first"));
    let updated = IconKey {
        modified: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1)),
        ..key.clone()
    };
    assert!(!cache.0.contains_key(&updated));
    cache.insert(updated.clone(), "updated".into());
    assert!(!cache.0.contains_key(&key));
    assert_eq!(cache.0.get(&updated).map(String::as_str), Some("updated"));
    for i in 0..CACHE_LIMIT + 1 {
        cache.insert(
            IconKey {
                path: format!("/app-{i}"),
                modified: None,
            },
            "image".into(),
        );
        assert!(cache.0.len() <= CACHE_LIMIT);
    }
}

#[test]
fn lookup_requires_an_absolute_existing_app_directory_and_reads_mtime() {
    assert!(icon_key("relative.app".into()).is_none());
    let dir = tempfile::tempdir().unwrap();
    assert!(icon_key(dir.path().to_string_lossy().into()).is_none());
    let app = dir.path().join("Example.app");
    std::fs::create_dir(&app).unwrap();
    let key = icon_key(app.to_string_lossy().into()).unwrap();
    assert_eq!(
        key.modified,
        std::fs::metadata(&app).unwrap().modified().ok()
    );
    std::fs::remove_dir(&app).unwrap();
    assert!(icon_key(app.to_string_lossy().into()).is_none());
}

#[test]
fn main_thread_closure_only_rasterizes_and_copies_pixels() {
    let source = include_str!("application_icon.rs");
    let main = source
        .split_once("app.run_on_main_thread(move || {")
        .unwrap()
        .1
        .split_once(".map_err")
        .unwrap()
        .0;
    assert!(main.contains("rasterize("));
    assert!(!main.contains("encode("));
    assert!(!main.contains("CACHE"));
    assert!(!main.contains("metadata"));
    let worker = source.split_once("let Some(pixels) = pixels").unwrap().1;
    assert!(worker.contains("spawn_blocking(move || {"));
    assert!(worker.contains("encode(pixels)"));
}
