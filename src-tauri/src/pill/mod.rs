pub mod context;
pub mod geometry;
pub mod hit_test;
pub mod icons;
pub mod level;
pub mod native;
pub mod positioning;

#[tauri::command]
pub fn pill_get_geometry(window: tauri::WebviewWindow) -> Result<geometry::Geometry, String> {
    use tauri::Manager;
    use tauri_plugin_store::StoreExt;
    if window.label() != "pill" {
        return Err("Pill window required".into());
    }
    let store = window
        .app_handle()
        .store("settings")
        .map_err(|e| e.to_string())?;
    let position = store.get("pill_indicator_position");
    Ok(geometry::Geometry::new(
        position
            .as_ref()
            .and_then(|v| v.as_str())
            .unwrap_or("bottom-center"),
    ))
}
