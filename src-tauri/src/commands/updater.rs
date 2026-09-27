use serde::Serialize;
use tauri::{AppHandle, Url};
use tauri_plugin_store::StoreExt;
use tauri_plugin_updater::{Updater, UpdaterExt};

use crate::{commands::distribution, release_channel};

pub const STABLE_UPDATE_ENDPOINT: &str =
    "https://github.com/ideaplexa/voicetypr/releases/latest/download/latest.json";
pub const BETA_UPDATE_ENDPOINT: &str =
    "https://github.com/ideaplexa/voicetypr/releases/download/beta/latest.json";

static UPDATE_OPERATION: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
pub const UPDATE_CHANNEL_EXPLICIT_KEY: &str = "update_channel_explicit";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateChannel {
    Stable,
    Beta,
}

impl UpdateChannel {
    fn from_stored_with_default(value: Option<&str>, default: Self) -> Self {
        match value {
            Some("beta") => Self::Beta,
            Some(_) => Self::Stable,
            None => default,
        }
    }
    fn installed_build_default() -> Self {
        if release_channel::IS_PRERELEASE_BUILD {
            Self::Beta
        } else {
            Self::Stable
        }
    }

    fn from_preference_with_default(
        value: Option<&str>,
        explicitly_selected: bool,
        default: Self,
    ) -> Self {
        // Before explicit-choice metadata existed, every generic settings save
        // wrote "stable", so an unmarked Stable value is not authoritative.
        // An unmarked Beta value could only come from an actual user choice.
        if explicitly_selected || value == Some("beta") {
            Self::from_stored_with_default(value, default)
        } else {
            default
        }
    }

    pub fn from_stored(value: Option<&str>) -> Self {
        Self::from_stored_with_default(value, Self::installed_build_default())
    }

    pub fn from_preference(value: Option<&str>, explicitly_selected: bool) -> Self {
        Self::from_preference_with_default(
            value,
            explicitly_selected,
            Self::installed_build_default(),
        )
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Beta => "beta",
        }
    }

    const fn endpoint(self) -> &'static str {
        match self {
            Self::Stable => STABLE_UPDATE_ENDPOINT,
            Self::Beta => BETA_UPDATE_ENDPOINT,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct AppUpdateInfo {
    pub version: String,
    pub body: String,
    pub channel: &'static str,
}

fn selected_channel(app: &AppHandle) -> Result<UpdateChannel, String> {
    let store = app.store("settings").map_err(|error| error.to_string())?;
    let stored = store
        .get("update_channel")
        .and_then(|value| value.as_str().map(str::to_owned));
    let explicitly_selected = store
        .get(UPDATE_CHANNEL_EXPLICIT_KEY)
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    Ok(UpdateChannel::from_preference(
        stored.as_deref(),
        explicitly_selected,
    ))
}

fn updater_for_channel(app: &AppHandle, channel: UpdateChannel) -> Result<Updater, String> {
    let endpoint = channel
        .endpoint()
        .parse::<Url>()
        .map_err(|error| format!("Invalid {} update endpoint: {error}", channel.as_str()))?;

    app.updater_builder()
        .endpoints(vec![endpoint])
        .map_err(|error| error.to_string())?
        .build()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn check_for_app_update(app: AppHandle) -> Result<Option<AppUpdateInfo>, String> {
    if distribution::is_store_install() {
        return Ok(None);
    }

    let _operation = UPDATE_OPERATION.lock().await;

    let channel = selected_channel(&app)?;
    let update = updater_for_channel(&app, channel)?
        .check()
        .await
        .map_err(|error| error.to_string())?;

    Ok(update.map(|update| AppUpdateInfo {
        version: update.version,
        body: update.body.unwrap_or_default(),
        channel: channel.as_str(),
    }))
}

#[tauri::command]
pub async fn install_app_update(app: AppHandle, expected_version: String) -> Result<(), String> {
    if distribution::is_store_install() {
        return Err("Updates are managed by Microsoft Store".to_string());
    }

    let _operation = UPDATE_OPERATION.lock().await;

    let channel = selected_channel(&app)?;
    let update = updater_for_channel(&app, channel)?
        .check()
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "No update is currently available".to_string())?;

    if update.version != expected_version {
        return Err(format!(
            "Available update changed from {expected_version} to {}; check again before installing",
            update.version
        ));
    }

    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        UpdateChannel, BETA_UPDATE_ENDPOINT, STABLE_UPDATE_ENDPOINT, UPDATE_CHANNEL_EXPLICIT_KEY,
    };

    #[test]
    fn missing_channel_uses_installed_build_channel() {
        assert_eq!(
            UpdateChannel::from_stored_with_default(None, UpdateChannel::Stable),
            UpdateChannel::Stable
        );
        assert_eq!(
            UpdateChannel::from_stored_with_default(None, UpdateChannel::Beta),
            UpdateChannel::Beta
        );
    }

    #[test]
    fn stored_channel_overrides_installed_build_channel() {
        assert_eq!(
            UpdateChannel::from_stored_with_default(Some("stable"), UpdateChannel::Beta),
            UpdateChannel::Stable
        );
        assert_eq!(
            UpdateChannel::from_stored_with_default(Some("beta"), UpdateChannel::Stable),
            UpdateChannel::Beta
        );
        assert_eq!(
            UpdateChannel::from_stored_with_default(Some("unexpected"), UpdateChannel::Beta),
            UpdateChannel::Stable
        );
    }

    #[test]
    fn legacy_implicit_stable_uses_installed_build_channel() {
        assert_eq!(
            UpdateChannel::from_preference_with_default(Some("stable"), false, UpdateChannel::Beta,),
            UpdateChannel::Beta
        );
    }

    #[test]
    fn legacy_beta_and_marked_stable_remain_explicit() {
        assert_eq!(
            UpdateChannel::from_preference_with_default(Some("beta"), false, UpdateChannel::Stable,),
            UpdateChannel::Beta
        );
        assert_eq!(
            UpdateChannel::from_preference_with_default(Some("stable"), true, UpdateChannel::Beta,),
            UpdateChannel::Stable
        );
        assert_eq!(UPDATE_CHANNEL_EXPLICIT_KEY, "update_channel_explicit");
    }

    #[test]
    fn channels_use_isolated_manifests() {
        assert_eq!(UpdateChannel::Stable.endpoint(), STABLE_UPDATE_ENDPOINT);
        assert_eq!(UpdateChannel::Beta.endpoint(), BETA_UPDATE_ENDPOINT);
        assert_ne!(
            UpdateChannel::Stable.endpoint(),
            UpdateChannel::Beta.endpoint()
        );
    }
}
