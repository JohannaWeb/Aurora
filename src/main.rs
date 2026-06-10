mod atlas;
mod blitz_document;
mod css;
mod dom;
mod fetch;
mod font;
mod html;
mod identity;
#[cfg(feature = "engine-boa")]
mod js_boa;
mod js_engine;
mod js_sm;
mod layout;
mod logging;
mod media;
pub mod render;
mod runner;
mod style;
mod window;

pub(crate) use media::MediaCache;
pub(crate) use runner::{ImageCache, SvgCache, load_missing_images, load_missing_svgs};

fn main() {
    logging::init();
    log::info!("Aurora: Starting up...");
    install_crypto_provider();
    println!("Aurora: Crypto provider installed.");

    let cli = runner::CliOptions::from_env();
    let identity = default_identity(&cli);
    runner::run_browser(cli, identity);
    logging::print_compat_summary();
}

fn install_crypto_provider() {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");
}

fn default_identity(cli: &runner::CliOptions) -> identity::Identity {
    let mut capabilities = vec![identity::Capability::NetworkAccess];
    if cli.allow_workspace_read {
        capabilities.push(identity::Capability::ReadWorkspace);
    }

    identity::Identity::new(
        "did:human:johanna",
        "Johanna",
        identity::IdentityKind::Human,
        capabilities,
    )
}
