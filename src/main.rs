mod atlas;
mod css;
mod dom;
mod fetch;
mod font;
mod gpu_paint;
mod html;
mod js_boa;
mod layout;
mod paint;
mod runner;
mod style;
mod window;

pub(crate) use runner::{load_images, ImageCache};

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

fn default_identity() -> opus::domain::Identity {
    opus::domain::Identity::new(
        "did:human:johanna",
        "Johanna",
        opus::domain::IdentityKind::Human,
        [
            opus::domain::Capability::NetworkAccess,
            opus::domain::Capability::ReadWorkspace,
        ],
    )
}
