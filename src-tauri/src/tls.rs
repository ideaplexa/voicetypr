//! One TLS provider for GUI, CLI, and cloud WebSockets.
//!
//! Reqwest 0.13 enables AWS-LC; the updater also enables ring. Rustls cannot
//! infer a default with both enabled. Do not rely on an updater request having
//! installed a provider before the first dictation.

use std::sync::{Arc, Once};

/// Install reqwest's existing provider before any network work. An embedding
/// caller may already have installed another valid provider; preserve it.
pub fn initialize_tls() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    });
}

fn client_config(
    provider: Arc<rustls::crypto::CryptoProvider>,
) -> Result<rustls::ClientConfig, rustls::Error> {
    let roots = rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    Ok(rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()?
        .with_root_certificates(roots)
        .with_no_client_auth())
}

/// Pass a complete config to tungstenite so its implicit, panic-prone provider
/// selection is never used. Configuration failures return an error to the
/// caller's normal REST fallback path instead of killing the streaming task.
pub(crate) fn websocket_connector() -> Result<tokio_tungstenite::Connector, rustls::Error> {
    initialize_tls();
    let provider = rustls::crypto::CryptoProvider::get_default()
        .cloned()
        .ok_or_else(|| rustls::Error::General("TLS provider unavailable".into()))?;
    Ok(tokio_tungstenite::Connector::Rustls(Arc::new(
        client_config(provider)?,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn websocket_tls_initializes_in_a_fresh_process() {
        const CHILD: &str = "VOICETYPR_TLS_REGRESSION_CHILD";
        if std::env::var_os(CHILD).is_none() {
            // Rustls's global provider cannot be reset. A fresh process prevents
            // an unrelated HTTP/updater test from hiding this regression.
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "tls::tests::websocket_tls_initializes_in_a_fresh_process",
                ])
                .env(CHILD, "1")
                .status()
                .unwrap();
            assert!(status.success());
            return;
        }
        assert!(rustls::crypto::CryptoProvider::get_default().is_none());
        initialize_tls();
        let first = rustls::crypto::CryptoProvider::get_default().unwrap();
        initialize_tls();
        assert!(Arc::ptr_eq(
            first,
            rustls::crypto::CryptoProvider::get_default().unwrap()
        ));
        // This is the exact implicit builder tungstenite used before the fix.
        // With ring + AWS-LC and no startup initialization it panics.
        let _ = rustls::ClientConfig::builder();
        let tokio_tungstenite::Connector::Rustls(config) = websocket_connector().unwrap() else {
            panic!("WebSockets must use TLS");
        };
        assert!(!config.crypto_provider().cipher_suites.is_empty());
        assert!(reqwest::Client::builder().build().is_ok());
    }

    #[test]
    fn invalid_provider_returns_configuration_error() {
        let mut provider = rustls::crypto::aws_lc_rs::default_provider();
        provider.cipher_suites.clear();
        assert!(client_config(Arc::new(provider)).is_err());
    }
}
