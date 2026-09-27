use std::sync::OnceLock;

use serde::Serialize;
#[derive(Debug, Serialize)]
pub struct DistributionInfo {
    pub channel: &'static str,
    pub is_store_install: bool,
    pub package_family_name: Option<String>,
}

#[tauri::command]
pub fn get_distribution_info() -> DistributionInfo {
    distribution_info(cached_package_family_name())
}

pub(crate) fn is_store_install() -> bool {
    cached_package_family_name().is_some()
}

static PACKAGE_FAMILY_NAME: OnceLock<Option<String>> = OnceLock::new();

fn cached_package_family_name() -> Option<String> {
    PACKAGE_FAMILY_NAME.get_or_init(package_family_name).clone()
}

fn distribution_info(package_family_name: Option<String>) -> DistributionInfo {
    let is_store_install = package_family_name.is_some();

    DistributionInfo {
        channel: if is_store_install {
            "store_msix"
        } else {
            "direct"
        },
        is_store_install,
        package_family_name,
    }
}

#[cfg(target_os = "windows")]
fn package_family_name() -> Option<String> {
    use windows::ApplicationModel::Package;

    let package = Package::Current().ok()?;
    let id = package.Id().ok()?;
    let family_name = id.FamilyName().ok()?;
    Some(family_name.to_string())
}

#[cfg(not(target_os = "windows"))]
fn package_family_name() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::distribution_info;
    #[cfg(not(target_os = "windows"))]
    use super::get_distribution_info;

    #[test]
    fn distribution_info_maps_store_detection_to_channel() {
        let direct = distribution_info(None);
        assert_eq!(direct.channel, "direct");
        assert!(!direct.is_store_install);
        assert!(direct.package_family_name.is_none());

        let store = distribution_info(Some("IdeaplexaLLC.Voicetypr_test".to_string()));
        assert_eq!(store.channel, "store_msix");
        assert!(store.is_store_install);
        assert_eq!(
            store.package_family_name.as_deref(),
            Some("IdeaplexaLLC.Voicetypr_test"),
        );
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn non_windows_distribution_defaults_to_direct() {
        let info = get_distribution_info();
        assert_eq!(info.channel, "direct");
        assert!(!info.is_store_install);
        assert!(info.package_family_name.is_none());
    }
}
