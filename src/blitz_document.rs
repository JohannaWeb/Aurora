use anyrender_vello::VelloScenePainter;
use blitz_dom::{BaseDocument, DocumentConfig};
use blitz_html::HtmlDocument;
use blitz_traits::net::{Bytes, NetHandler, NetProvider, Request};
use blitz_traits::shell::{ColorScheme, Viewport};
use markup5ever::local_name;
use vello::Scene;

use crate::identity::Identity;

use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

static NET_CACHE: OnceLock<Mutex<BTreeMap<String, Vec<u8>>>> = OnceLock::new();

fn get_net_cache() -> &'static Mutex<BTreeMap<String, Vec<u8>>> {
    NET_CACHE.get_or_init(|| Mutex::new(BTreeMap::new()))
}

struct AuroraNetProvider {
    identity: Identity,
}

impl AuroraNetProvider {
    fn new(identity: &Identity) -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self {
            identity: identity.clone(),
        })
    }
}

impl NetProvider for AuroraNetProvider {
    fn fetch(&self, _doc_id: usize, request: Request, handler: Box<dyn NetHandler>) {
        let url = request.url.to_string();
        let identity = self.identity.clone();

        // Check cache first
        let cache = get_net_cache();
        if let Some(cached_bytes) = cache.lock().unwrap().get(&url) {
            handler.bytes(url, Bytes::from(cached_bytes.clone()));
            return;
        }

        std::thread::spawn(move || {
            let bytes = match crate::fetch::fetch_bytes(&url, &identity) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("blitz fetch: {url}: {e}");
                    Vec::new()
                }
            };

            // Save to cache
            let cache = get_net_cache();
            cache.lock().unwrap().insert(url.clone(), bytes.clone());

            handler.bytes(url, Bytes::from(bytes));
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

    /// Walk up from the hit node looking for an `<a href="...">` ancestor.
    /// Coordinates are in document space (scroll already applied by the caller).
    pub fn hit_test_anchor(&self, x: f32, y: f32) -> Option<String> {
        let hit = self.inner.hit(x, y)?;
        let mut node_id = hit.node_id;
        loop {
            let node = self.inner.get_node(node_id)?;
            if node.data.is_element_with_tag_name(&local_name!("a")) {
                if let Some(href) = node.data.attr(local_name!("href")) {
                    return Some(href.to_string());
                }
            }
            node_id = node.parent?;
        }
    }

    pub fn paint_to_scene(&mut self, scene: &mut Scene, width: u32, height: u32) {
        // Process any resources that arrived from background fetch threads.
        self.inner.resolve(0.0);
        let mut painter = VelloScenePainter::new(scene);
        blitz_paint::paint_scene(&mut painter, &mut self.inner, 1.0, width, height, 0, 0);
    }

    pub fn debug_summary(&self) -> String {
        let root = self.inner.root_element();
        let mut counts = NodeCounts::default();
        collect_node_counts(root, &mut counts);

        let mut top_tags = Vec::new();
        for &child_id in root.children.iter().take(8) {
            if let Some(node) = self.inner.get_node(child_id) {
                if let Some(el) = node.data.downcast_element() {
                    top_tags.push(el.name.local.to_string());
                } else if matches!(node.data, blitz_dom::NodeData::Text(_)) {
                    top_tags.push("#text".to_string());
                }
            }
        }

        format!(
            "root={} nodes={} elements={} text={} top=[{}] text_len={}",
            root.data
                .downcast_element()
                .map(|el| el.name.local.to_string())
                .unwrap_or_else(|| "#document".to_string()),
            counts.total,
            counts.elements,
            counts.text,
            top_tags.join(","),
            root.text_content().len()
        )
    }
}

#[derive(Default)]
struct NodeCounts {
    total: usize,
    elements: usize,
    text: usize,
}

fn collect_node_counts(node: &blitz_dom::Node, counts: &mut NodeCounts) {
    counts.total += 1;
    match &node.data {
        blitz_dom::NodeData::Element(_) | blitz_dom::NodeData::AnonymousBlock(_) => {
            counts.elements += 1;
            for &child_id in node.children.iter() {
                collect_node_counts(node.with(child_id), counts);
            }
        }
        blitz_dom::NodeData::Text(_) => {
            counts.text += 1;
        }
        blitz_dom::NodeData::Document | blitz_dom::NodeData::Comment => {
            for &child_id in node.children.iter() {
                collect_node_counts(node.with(child_id), counts);
            }
        }
    }
}
