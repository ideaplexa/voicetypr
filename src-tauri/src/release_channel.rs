/// Returns whether a semantic version contains a prerelease suffix.
pub(crate) const fn is_prerelease(version: &str) -> bool {
    let bytes = version.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'-' {
            return true;
        }
        index += 1;
    }
    false
}

pub(crate) const IS_PRERELEASE_BUILD: bool = is_prerelease(env!("CARGO_PKG_VERSION"));

/// Installed build channel, derived from the compile-time application version.
pub(crate) const RELEASE_CHANNEL: &str = if IS_PRERELEASE_BUILD {
    "beta"
} else {
    "stable"
};

#[cfg(test)]
mod tests {
    use super::is_prerelease;

    #[test]
    fn release_channel_is_derived_from_semver_prerelease() {
        assert!(is_prerelease("2.1.0-beta.1"));
        assert!(is_prerelease("3.0.0-rc.1"));
        assert!(!is_prerelease("2.1.0"));
    }
}
