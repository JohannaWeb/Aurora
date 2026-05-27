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
pub(crate) use runner::{load_images, load_svgs, ImageCache, SvgCache};

fn main() {
    println!("Aurora: Starting up...");
    install_crypto_provider();
    println!("Aurora: Crypto provider installed.");

    let identity = default_identity();
    let cli = runner::CliOptions::from_env();
    runner::run_browser(cli, identity);
}

fn install_crypto_provider() {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");
}

fn default_identity() -> identity::Identity {
    identity::Identity::new(
        "did:human:johanna",
        "Johanna",
        identity::IdentityKind::Human,
        [
            identity::Capability::NetworkAccess,
            identity::Capability::ReadWorkspace,
        ],
    )
}
