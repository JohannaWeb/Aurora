use anyrender_vello::VelloScenePainter;
use blitz_dom::{BaseDocument, DocumentConfig};
use blitz_html::HtmlDocument;
use blitz_traits::net::{Bytes, NetHandler, NetProvider, Request};
use blitz_traits::shell::{ColorScheme, Viewport};
use vello::Scene;

use crate::identity::Identity;

struct AuroraNetProvider {
    identity: Identity,
}

impl AuroraNetProvider {
    fn new(identity: &Identity) -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self { identity: identity.clone() })
    }
}

impl NetProvider for AuroraNetProvider {
    fn fetch(&self, _doc_id: usize, request: Request, handler: Box<dyn NetHandler>) {
        let url = request.url.to_string();
        let identity = self.identity.clone();
        std::thread::spawn(move || {
            let bytes = match crate::fetch::fetch_bytes(&url, &identity) {
                Ok(b) => Bytes::from(b),
                Err(e) => {
                    eprintln!("blitz fetch: {url}: {e}");
                    Bytes::new()
                }
            };
            handler.bytes(url, bytes);
        });
    }
}

pub struct BlitzDocument {
    inner: BaseDocument,
}

impl BlitzDocument {
    pub fn from_html(
        html: &str,
        base_url: Option<&str>,
        identity: &Identity,
        width: u32,
        height: u32,
    ) -> Self {
        let config = DocumentConfig {
            base_url: base_url.map(|s| s.to_string()),
            viewport: Some(Viewport::new(width, height, 1.0, ColorScheme::Light)),
            net_provider: Some(AuroraNetProvider::new(identity)),
            ..Default::default()
        };
        let mut inner = HtmlDocument::from_html(html, config).into_inner();
        inner.resolve(0.0);
        BlitzDocument { inner }
    }

    pub fn resolve(&mut self, width: u32, height: u32) {
        self.inner
            .set_viewport(Viewport::new(width, height, 1.0, ColorScheme::Light));
        self.inner.resolve(0.0);
    }

    pub fn paint_to_scene(&mut self, scene: &mut Scene, width: u32, height: u32) {
        // Process any resources that arrived from background fetch threads.
        self.inner.resolve(0.0);
        let mut painter = VelloScenePainter::new(scene);
        blitz_paint::paint_scene(&mut painter, &mut self.inner, 1.0, width, height, 0, 0);
    }
}
