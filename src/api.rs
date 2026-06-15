//! Public embedding API for the Aurora engine.
//!
//! This is the stable surface for consumers of the `aurora-engine` crate. The
//! internal module tree (`dom`, `css`, `layout`, `style`, `render`, the JS
//! engines, …) stays `pub(crate)`; everything an embedder needs is re-exported
//! from the crate root.
//!
//! ```no_run
//! use aurora::{Browser, Capabilities};
//!
//! // A sandboxed browser that cannot reach the network.
//! let browser = Browser::builder()
//!     .capabilities(Capabilities::sandboxed())
//!     .build();
//!
//! let page = browser.load_html("<h1>Hello, Aurora</h1>");
//! let png = page.render_png(800, 600).unwrap();
//! std::fs::write("hello.png", png).unwrap();
//! ```

use std::io::Cursor;

use image::{ImageFormat, RgbaImage};

/// Re-exported so callers can work with raw pixel buffers without depending on
/// the `image` crate by name.
pub use image::RgbaImage as Image;

/// What an [`Browser`] instance is permitted to do.
///
/// Capability gating is Aurora's core differentiator: an embedder grants only
/// the powers a page should have. v1 gates network egress; finer-grained
/// capabilities (filesystem, JS, storage) are planned.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Capabilities {
    network: bool,
    read_workspace: bool,
}

impl Capabilities {
    /// All capabilities granted.
    pub fn all() -> Self {
        Self {
            network: true,
            read_workspace: true,
        }
    }

    /// No capabilities — the page can only render content handed to it
    /// directly (e.g. via [`Browser::load_html`]). Network loads are denied.
    pub fn sandboxed() -> Self {
        Self {
            network: false,
            read_workspace: false,
        }
    }

    /// Allow or deny outbound network access (fetching URLs, subresources).
    pub fn network(mut self, allow: bool) -> Self {
        self.network = allow;
        self
    }

    /// Allow or deny reading from the local workspace.
    pub fn read_workspace(mut self, allow: bool) -> Self {
        self.read_workspace = allow;
        self
    }

    /// Whether network access is granted.
    pub fn has_network(&self) -> bool {
        self.network
    }
}

impl Default for Capabilities {
    /// The default grant is permissive (matches the standalone browser).
    fn default() -> Self {
        Self::all()
    }
}

/// Builder for a [`Browser`].
#[derive(Clone, Debug, Default)]
pub struct BrowserBuilder {
    capabilities: Capabilities,
}

impl BrowserBuilder {
    /// Set the capabilities granted to pages opened by this browser.
    pub fn capabilities(mut self, capabilities: Capabilities) -> Self {
        self.capabilities = capabilities;
        self
    }

    /// Finish building.
    pub fn build(self) -> Browser {
        Browser {
            capabilities: self.capabilities,
        }
    }
}

/// An Aurora engine instance. Cheap to create; holds the capability policy
/// applied to every page it opens.
#[derive(Clone, Debug)]
pub struct Browser {
    capabilities: Capabilities,
}

impl Browser {
    /// Start configuring a browser.
    pub fn builder() -> BrowserBuilder {
        BrowserBuilder::default()
    }

    /// A browser with default (permissive) capabilities.
    pub fn new() -> Self {
        Self {
            capabilities: Capabilities::default(),
        }
    }

    /// The capabilities this browser grants.
    pub fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    /// Load an in-memory HTML document. Requires no capabilities.
    pub fn load_html(&self, html: impl Into<String>) -> Page {
        Page {
            source: Source::Html(html.into()),
        }
    }

    /// Fetch and load a document by URL.
    ///
    /// Returns [`Error::CapabilityDenied`] if this browser lacks the network
    /// capability.
    pub fn load_url(&self, url: impl Into<String>) -> Result<Page, Error> {
        if !self.capabilities.network {
            return Err(Error::CapabilityDenied("network"));
        }
        Ok(Page {
            source: Source::Url(url.into()),
        })
    }
}

impl Default for Browser {
    fn default() -> Self {
        Self::new()
    }
}

enum Source {
    Html(String),
    Url(String),
}

/// A loaded document, ready to be rendered at a chosen viewport size.
pub struct Page {
    source: Source,
}

impl Page {
    /// Render the page to an RGBA pixel buffer at the given viewport size.
    pub fn render_rgba(&self, width: u32, height: u32) -> RgbaImage {
        match &self.source {
            Source::Html(html) => crate::render::headless::render_to_image(html, width, height),
            Source::Url(url) => crate::render::headless::render_url_to_image(url, width, height),
        }
    }

    /// Render the page and encode it as PNG bytes.
    pub fn render_png(&self, width: u32, height: u32) -> Result<Vec<u8>, Error> {
        let image = self.render_rgba(width, height);
        let mut buf = Cursor::new(Vec::new());
        image
            .write_to(&mut buf, ImageFormat::Png)
            .map_err(|e| Error::Encode(e.to_string()))?;
        Ok(buf.into_inner())
    }

    /// Render the page and write it to `path`, inferring the format from the
    /// file extension.
    pub fn render_to_file(
        &self,
        width: u32,
        height: u32,
        path: impl AsRef<std::path::Path>,
    ) -> Result<(), Error> {
        self.render_rgba(width, height)
            .save(path)
            .map_err(|e| Error::Encode(e.to_string()))
    }
}

/// Errors surfaced by the public API.
#[derive(Debug)]
pub enum Error {
    /// A required capability was not granted to the browser.
    CapabilityDenied(&'static str),
    /// Encoding the rendered image failed.
    Encode(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::CapabilityDenied(cap) => {
                write!(f, "capability denied: `{cap}` is not granted")
            }
            Error::Encode(msg) => write!(f, "image encode failed: {msg}"),
        }
    }
}

impl std::error::Error for Error {}
