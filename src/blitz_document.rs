use anyrender_vello::VelloScenePainter;
use blitz_dom::{BaseDocument, DocumentConfig};
use blitz_html::HtmlDocument;
use blitz_traits::net::{Bytes, NetHandler, NetProvider, Request};
use blitz_traits::shell::{ColorScheme, Viewport};
use markup5ever::local_name;
use vello::Scene;

use crate::identity::Identity;

use std::collections::BTreeMap;
use std::panic::{self, AssertUnwindSafe};
use std::sync::{Mutex, OnceLock};

static NET_CACHE: OnceLock<Mutex<BTreeMap<String, Vec<u8>>>> = OnceLock::new();
static STYLO_PANIC_HOOK_LOCK: Mutex<()> = Mutex::new(());

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
    healthy: bool,
}

impl BlitzDocument {
    pub fn try_from_html(
        html: &str,
        base_url: Option<&str>,
        identity: &Identity,
        width: u32,
        height: u32,
    ) -> Option<Self> {
        let config = DocumentConfig {
            base_url: base_url.map(|s| s.to_string()),
            viewport: Some(Viewport::new(width, height, 1.0, ColorScheme::Light)),
            net_provider: Some(AuroraNetProvider::new(identity)),
            ..Default::default()
        };
        catch_stylo_panic("constructing Blitz document", || {
            let inner = HtmlDocument::from_html(html, config).into_inner();
            BlitzDocument {
                inner,
                healthy: true,
            }
        })
        .and_then(|mut doc| if doc.resolve_inner() { Some(doc) } else { None })
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

    pub fn paint_to_scene(&mut self, scene: &mut Scene, width: u32, height: u32) -> bool {
        if !self.healthy {
            return false;
        }
        // Process any resources that arrived from background fetch threads.
        if !self.resolve_inner() {
            return false;
        }
        let mut painter = VelloScenePainter::new(scene);
        catch_stylo_panic("painting Blitz document", || {
            blitz_paint::paint_scene(&mut painter, &mut self.inner, 1.0, width, height, 0, 0);
        })
        .is_some()
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

    fn resolve_inner(&mut self) -> bool {
        let resolved = catch_stylo_panic("resolving Blitz document", || {
            self.inner.resolve(0.0);
        })
        .is_some();
        if !resolved {
            self.healthy = false;
        }
        resolved
    }
}

fn catch_stylo_panic<T>(context: &str, f: impl FnOnce() -> T) -> Option<T> {
    let _hook_guard = STYLO_PANIC_HOOK_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let previous_hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));
    let result = panic::catch_unwind(AssertUnwindSafe(f));
    panic::set_hook(previous_hook);

    match result {
        Ok(value) => Some(value),
        Err(payload) => {
            log::warn!(
                "Blitz/Stylo panicked while {context}; disabling Blitz renderer for this document: {}",
                panic_payload_message(payload.as_ref())
            );
            None
        }
    }
}

fn panic_payload_message(payload: &(dyn std::any::Any + Send)) -> &str {
    if let Some(message) = payload.downcast_ref::<&'static str>() {
        message
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.as_str()
    } else {
        "non-string panic payload"
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
