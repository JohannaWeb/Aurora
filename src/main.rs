mod atlas;
mod css;
mod dom;
mod fetch;
mod font;
mod gpu_paint;
mod html;
mod identity;
mod js_boa;
mod layout;
mod media;
pub mod render;
mod runner;
mod style;
mod window;

pub(crate) use media::MediaCache;
pub(crate) use runner::{ImageCache, SvgCache, load_missing_images, load_missing_svgs};

fn main() {
    println!("Aurora: Starting up...");
    install_crypto_provider();
    println!("Aurora: Crypto provider installed.");

    let cli = runner::CliOptions::from_env();
    let identity = default_identity(&cli);
    runner::run_browser(cli, identity);
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
