use rustls::{ClientConfig, RootCertStore};
use std::sync::Arc;
use webpki_roots::TLS_SERVER_ROOTS;

pub(super) fn tls_config() -> Arc<ClientConfig> {
    let root_store = RootCertStore::from_iter(TLS_SERVER_ROOTS.iter().cloned());
    Arc::new(
        ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth(),
    )
}
