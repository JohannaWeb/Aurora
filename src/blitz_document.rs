use anyrender_vello::VelloScenePainter;
use blitz_dom::{Attribute, BaseDocument, DocumentConfig, DocumentMutator};
use blitz_html::HtmlDocument;
use blitz_traits::net::{Bytes, NetHandler, NetProvider, Request};
use blitz_traits::shell::{ColorScheme, Viewport};
use markup5ever::{LocalName, QualName, local_name, namespace_prefix, ns};
use vello::Scene;

use crate::dom::{Node, NodePtr};
use crate::identity::Identity;

use std::collections::BTreeMap;
use std::panic::{self, AssertUnwindSafe};
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

const MAX_CONSECUTIVE_PANICS: u32 = 5;

pub struct BlitzDocument {
    inner: BaseDocument,
    healthy: bool,
    consecutive_panics: u32,
    legacy_to_blitz: BTreeMap<usize, usize>,
    blitz_to_legacy: BTreeMap<usize, NodePtr>,
}

impl BlitzDocument {
    fn config(
        base_url: Option<&str>,
        identity: &Identity,
        width: u32,
        height: u32,
    ) -> DocumentConfig {
        DocumentConfig {
            base_url: base_url.map(|s| s.to_string()),
            viewport: Some(Viewport::new(width, height, 1.0, ColorScheme::Light)),
            net_provider: Some(AuroraNetProvider::new(identity)),
            ..Default::default()
        }
    }

    pub fn try_from_html(
        html: &str,
        base_url: Option<&str>,
        identity: &Identity,
        width: u32,
        height: u32,
    ) -> Option<Self> {
        let config = Self::config(base_url, identity, width, height);
        catch_stylo_panic("constructing Blitz document", || {
            let inner = HtmlDocument::from_html(html, config).into_inner();
            BlitzDocument {
                inner,
                healthy: true,
                consecutive_panics: 0,
                legacy_to_blitz: BTreeMap::new(),
                blitz_to_legacy: BTreeMap::new(),
            }
        })
        .and_then(|mut doc| if doc.resolve_inner() { Some(doc) } else { None })
    }

    pub fn try_from_dom(
        dom: &NodePtr,
        base_url: Option<&str>,
        identity: &Identity,
        width: u32,
        height: u32,
    ) -> Option<Self> {
        let config = Self::config(base_url, identity, width, height);
        catch_stylo_panic("constructing Blitz document from DOM", || {
            let mut inner = BaseDocument::new(config);
            let mut legacy_to_blitz = BTreeMap::new();
            let mut blitz_to_legacy = BTreeMap::new();
            {
                let mut mutator = inner.mutate();
                let mut maps = BlitzNodeMaps {
                    legacy_to_blitz: &mut legacy_to_blitz,
                    blitz_to_legacy: &mut blitz_to_legacy,
                };
                append_dom_children(&mut mutator, 0, dom, &ns!(html), &mut maps);
            }
            BlitzDocument {
                inner,
                healthy: true,
                consecutive_panics: 0,
                legacy_to_blitz,
                blitz_to_legacy,
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

    pub fn set_viewport(&mut self, width: u32, height: u32) -> bool {
        let updated = catch_stylo_panic("updating Blitz viewport", || {
            self.inner
                .set_viewport(Viewport::new(width, height, 1.0, ColorScheme::Light));
        })
        .is_some();
        if updated {
            self.consecutive_panics = 0;
        } else {
            self.consecutive_panics += 1;
            if self.consecutive_panics >= MAX_CONSECUTIVE_PANICS {
                self.healthy = false;
            }
        }
        updated
    }

    pub fn blitz_node_id_for_dom(&self, node: &NodePtr) -> Option<usize> {
        self.legacy_to_blitz.get(&legacy_node_key(node)).copied()
    }

    pub fn hit_test_dom_node(&self, x: f32, y: f32) -> Option<NodePtr> {
        let mut node_id = self.inner.hit(x, y)?.node_id;
        loop {
            if let Some(node) = self.blitz_to_legacy.get(&node_id) {
                return Some(node.clone());
            }
            node_id = self.inner.get_node(node_id)?.parent?;
        }
    }

    pub fn document_height(&self) -> f32 {
        let root = self.inner.root_element();
        document_bottom(root).max(root.final_layout.size.height)
    }

    pub fn sync_append_child(&mut self, parent: &NodePtr, child: &NodePtr) -> bool {
        let Some(parent_id) = self.blitz_node_id_for_dom(parent) else {
            return false;
        };
        let parent_ns = self.node_namespace(parent_id).unwrap_or_else(|| ns!(html));
        let updated = catch_stylo_panic("appending DOM mutation into Blitz document", || {
            let child_ids = {
                let mut maps = BlitzNodeMaps {
                    legacy_to_blitz: &mut self.legacy_to_blitz,
                    blitz_to_legacy: &mut self.blitz_to_legacy,
                };
                let mut mutator = self.inner.mutate();
                blitz_ids_for_dom_child(&mut mutator, child, &parent_ns, &mut maps)
            };
            if child_ids.is_empty() {
                return false;
            }
            let mut mutator = self.inner.mutate();
            mutator.append_children(parent_id, &child_ids);
            true
        })
        .unwrap_or(false);
        if updated {
            self.consecutive_panics = 0;
        }
        updated
    }

    pub fn sync_insert_before(
        &mut self,
        parent: &NodePtr,
        new_child: &NodePtr,
        ref_child: Option<&NodePtr>,
    ) -> bool {
        let Some(parent_id) = self.blitz_node_id_for_dom(parent) else {
            return false;
        };
        let parent_ns = self.node_namespace(parent_id).unwrap_or_else(|| ns!(html));
        let anchor_id = ref_child.and_then(|node| self.blitz_node_id_for_dom(node));
        let updated = catch_stylo_panic("inserting DOM mutation into Blitz document", || {
            let child_ids = {
                let mut maps = BlitzNodeMaps {
                    legacy_to_blitz: &mut self.legacy_to_blitz,
                    blitz_to_legacy: &mut self.blitz_to_legacy,
                };
                let mut mutator = self.inner.mutate();
                blitz_ids_for_dom_child(&mut mutator, new_child, &parent_ns, &mut maps)
            };
            if child_ids.is_empty() {
                return false;
            }
            let mut mutator = self.inner.mutate();
            if let Some(anchor_id) = anchor_id {
                mutator.insert_nodes_before(anchor_id, &child_ids);
            } else {
                mutator.append_children(parent_id, &child_ids);
            }
            true
        })
        .unwrap_or(false);
        if updated {
            self.consecutive_panics = 0;
        }
        updated
    }

    pub fn sync_remove_child(&mut self, child: &NodePtr) -> bool {
        let Some(child_id) = self.blitz_node_id_for_dom(child) else {
            return false;
        };
        let updated = catch_stylo_panic("removing DOM mutation from Blitz document", || {
            let mut mutator = self.inner.mutate();
            mutator.remove_node(child_id);
            true
        })
        .unwrap_or(false);
        if updated {
            remove_subtree_mapping(child, &mut self.legacy_to_blitz, &mut self.blitz_to_legacy);
            self.consecutive_panics = 0;
        }
        updated
    }

    pub fn sync_replace_child(
        &mut self,
        parent: &NodePtr,
        new_child: &NodePtr,
        old_child: &NodePtr,
    ) -> bool {
        let Some(old_id) = self.blitz_node_id_for_dom(old_child) else {
            return false;
        };
        let parent_ns = self
            .blitz_node_id_for_dom(parent)
            .and_then(|id| self.node_namespace(id))
            .unwrap_or_else(|| ns!(html));
        let updated = catch_stylo_panic("replacing DOM mutation in Blitz document", || {
            let child_ids = {
                let mut maps = BlitzNodeMaps {
                    legacy_to_blitz: &mut self.legacy_to_blitz,
                    blitz_to_legacy: &mut self.blitz_to_legacy,
                };
                let mut mutator = self.inner.mutate();
                blitz_ids_for_dom_child(&mut mutator, new_child, &parent_ns, &mut maps)
            };
            if child_ids.is_empty() {
                return false;
            }
            let mut mutator = self.inner.mutate();
            mutator.replace_node_with(old_id, &child_ids);
            true
        })
        .unwrap_or(false);
        if updated {
            remove_subtree_mapping(
                old_child,
                &mut self.legacy_to_blitz,
                &mut self.blitz_to_legacy,
            );
            self.consecutive_panics = 0;
        }
        updated
    }

    pub fn sync_set_attribute(&mut self, node: &NodePtr, name: &str, value: &str) -> bool {
        let Some(node_id) = self.blitz_node_id_for_dom(node) else {
            return false;
        };
        let updated = catch_stylo_panic("setting DOM attribute in Blitz document", || {
            let mut mutator = self.inner.mutate();
            mutator.set_attribute(node_id, attr_qual_name_from_str(name), value);
            true
        })
        .unwrap_or(false);
        if updated {
            self.consecutive_panics = 0;
        }
        updated
    }

    pub fn sync_remove_attribute(&mut self, node: &NodePtr, name: &str) -> bool {
        let Some(node_id) = self.blitz_node_id_for_dom(node) else {
            return false;
        };
        let updated = catch_stylo_panic("removing DOM attribute from Blitz document", || {
            let mut mutator = self.inner.mutate();
            mutator.clear_attribute(node_id, attr_qual_name_from_str(name));
            true
        })
        .unwrap_or(false);
        if updated {
            self.consecutive_panics = 0;
        }
        updated
    }

    pub fn sync_all_attributes(&mut self, node: &NodePtr) -> bool {
        let Some(node_id) = self.blitz_node_id_for_dom(node) else {
            return false;
        };
        let Ok(node_borrow) = node.try_borrow() else {
            return false;
        };
        let attrs = match &*node_borrow {
            Node::Element(el) => el
                .attributes
                .iter()
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        };
        drop(node_borrow);
        let updated = catch_stylo_panic("replacing DOM attributes in Blitz document", || {
            let existing_names = self
                .inner
                .get_node(node_id)
                .and_then(|node| node.data.downcast_element())
                .map(|element| {
                    element
                        .attrs
                        .iter()
                        .map(|attr| attr.name.clone())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let mut mutator = self.inner.mutate();
            for name in existing_names {
                mutator.clear_attribute(node_id, name);
            }
            for (name, value) in attrs {
                mutator.set_attribute(node_id, attr_qual_name_from_str(&name), &value);
            }
            true
        })
        .unwrap_or(false);
        if updated {
            self.consecutive_panics = 0;
        }
        updated
    }

    pub fn sync_text_node(&mut self, node: &NodePtr, text: &str) -> bool {
        let Some(node_id) = self.blitz_node_id_for_dom(node) else {
            return false;
        };
        let updated = catch_stylo_panic("setting DOM text in Blitz document", || {
            let mut mutator = self.inner.mutate();
            mutator.set_node_text(node_id, text);
            true
        })
        .unwrap_or(false);
        if updated {
            self.consecutive_panics = 0;
        }
        updated
    }

    pub fn sync_attach_shadow_root(&mut self, host: &NodePtr, shadow_root: &NodePtr) -> bool {
        let Some(host_id) = self.blitz_node_id_for_dom(host) else {
            return false;
        };
        if self.blitz_node_id_for_dom(shadow_root).is_some() {
            return true;
        }
        let host_ns = self.node_namespace(host_id).unwrap_or_else(|| ns!(html));
        let updated = catch_stylo_panic("attaching ShadowRoot in Blitz document", || {
            let attrs = vec![Attribute {
                name: attr_qual_name_from_str("data-aurora-shadow-root"),
                value: "true".to_string(),
            }];
            let shadow_id = {
                let mut mutator = self.inner.mutate();
                mutator.create_element(element_qual_name_from_str("div", &host_ns), attrs)
            };
            self.legacy_to_blitz
                .insert(legacy_node_key(shadow_root), shadow_id);
            self.blitz_to_legacy.insert(shadow_id, shadow_root.clone());
            let mut mutator = self.inner.mutate();
            mutator.append_children(host_id, &[shadow_id]);
            true
        })
        .unwrap_or(false);
        if updated {
            self.consecutive_panics = 0;
        }
        updated
    }

    pub fn sync_clear_children(&mut self, parent: &NodePtr) -> bool {
        let Some(parent_id) = self.blitz_node_id_for_dom(parent) else {
            return false;
        };
        remove_subtree_mappings_for_children(
            parent,
            &mut self.legacy_to_blitz,
            &mut self.blitz_to_legacy,
        );
        let updated = catch_stylo_panic("clearing DOM children in Blitz document", || {
            let mut mutator = self.inner.mutate();
            mutator.remove_and_drop_all_children(parent_id);
            true
        })
        .unwrap_or(false);
        if updated {
            self.consecutive_panics = 0;
        }
        updated
    }

    pub fn sync_replace_children(&mut self, parent: &NodePtr) -> bool {
        let Some(parent_id) = self.blitz_node_id_for_dom(parent) else {
            return false;
        };
        let parent_ns = self.node_namespace(parent_id).unwrap_or_else(|| ns!(html));
        let updated = catch_stylo_panic("replacing DOM children in Blitz document", || {
            let child_ids = {
                let mut maps = BlitzNodeMaps {
                    legacy_to_blitz: &mut self.legacy_to_blitz,
                    blitz_to_legacy: &mut self.blitz_to_legacy,
                };
                let mut mutator = self.inner.mutate();
                let mut ids = Vec::new();
                for child in child_nodes_for_blitz(parent) {
                    ids.extend(blitz_ids_for_dom_child(
                        &mut mutator,
                        &child,
                        &parent_ns,
                        &mut maps,
                    ));
                }
                ids
            };
            if !child_ids.is_empty() {
                let mut mutator = self.inner.mutate();
                mutator.append_children(parent_id, &child_ids);
            }
            true
        })
        .unwrap_or(false);
        if updated {
            self.consecutive_panics = 0;
        }
        updated
    }

    fn node_namespace(&self, node_id: usize) -> Option<markup5ever::Namespace> {
        self.inner
            .get_node(node_id)
            .and_then(|node| node.data.downcast_element())
            .map(|element| element.name.ns.clone())
    }

    pub fn paint_to_scene(&mut self, scene: &mut Scene, width: u32, height: u32) -> bool {
        if !self.healthy {
            return false;
        }
        if !self.resolve_inner() {
            return self.healthy;
        }
        let mut painter = VelloScenePainter::new(scene);
        let painted = catch_stylo_panic("painting Blitz document", || {
            blitz_paint::paint_scene(&mut painter, &mut self.inner, 1.0, width, height, 0, 0);
        })
        .is_some();
        if painted {
            self.consecutive_panics = 0;
        } else {
            self.consecutive_panics += 1;
            if self.consecutive_panics >= MAX_CONSECUTIVE_PANICS {
                self.healthy = false;
            }
        }
        painted || self.healthy
    }

    /// TEMP: walk the tree printing each element's tag, computed size, position
    /// and display, to find where YouTube's height collapses to ~0.
    pub fn debug_layout_sizes(&self) -> String {
        let mut out = String::new();
        self.dump_node(self.inner.root_element().id, 0, &mut out);
        out
    }

    fn dump_node(&self, node_id: usize, depth: usize, out: &mut String) {
        if depth > 18 {
            return;
        }
        let Some(node) = self.inner.get_node(node_id) else {
            return;
        };
        if let Some(el) = node.data.downcast_element() {
            let s = node.final_layout.size;
            let loc = node.final_layout.location;
            let disp = node
                .primary_styles()
                .map(|st| format!("{:?}", st.clone_display()))
                .unwrap_or_else(|| "?".into());
            // Only print element rows that are interesting (skip deep tiny text wrappers).
            out.push_str(&format!(
                "\n{:indent$}{} {}x{} @({:.0},{:.0}) {}",
                "",
                el.name.local,
                s.width as i32,
                s.height as i32,
                loc.x,
                loc.y,
                disp,
                indent = depth * 2
            ));
            // Stop descending once we are clearly inside a collapsed subtree but
            // still show the first couple of children for context.
            let limit = if depth < 6 { 400 } else { 3 };
            for &cid in node.children.iter().take(limit) {
                self.dump_node(cid, depth + 1, out);
            }
        }
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
        if resolved {
            self.consecutive_panics = 0;
        } else {
            self.consecutive_panics += 1;
            if self.consecutive_panics >= MAX_CONSECUTIVE_PANICS {
                self.healthy = false;
            }
        }
        resolved
    }
}

fn append_dom_children(
    mutator: &mut DocumentMutator<'_>,
    parent_id: usize,
    node: &NodePtr,
    parent_ns: &markup5ever::Namespace,
    maps: &mut BlitzNodeMaps<'_>,
) {
    for child in child_nodes_for_blitz(node) {
        let child_id = create_dom_node(mutator, &child, parent_ns, maps);
        mutator.append_children(parent_id, &[child_id]);
    }
}

struct BlitzNodeMaps<'a> {
    legacy_to_blitz: &'a mut BTreeMap<usize, usize>,
    blitz_to_legacy: &'a mut BTreeMap<usize, NodePtr>,
}

fn create_dom_node(
    mutator: &mut DocumentMutator<'_>,
    node: &NodePtr,
    parent_ns: &markup5ever::Namespace,
    maps: &mut BlitzNodeMaps<'_>,
) -> usize {
    if let Some(&existing) = maps.legacy_to_blitz.get(&legacy_node_key(node)) {
        return existing;
    }

    match &*node.borrow() {
        Node::Text(text) => {
            let id = mutator.create_text_node(&text.content);
            maps.legacy_to_blitz.insert(legacy_node_key(node), id);
            maps.blitz_to_legacy.insert(id, node.clone());
            id
        }
        Node::Document { .. } => {
            let fragment_id =
                mutator.create_element(element_qual_name_from_str("div", parent_ns), Vec::new());
            maps.legacy_to_blitz
                .insert(legacy_node_key(node), fragment_id);
            maps.blitz_to_legacy.insert(fragment_id, node.clone());
            append_dom_children(mutator, fragment_id, node, parent_ns, maps);
            fragment_id
        }
        Node::Element(el) => {
            let name = element_qual_name_from_str(&el.tag_name, parent_ns);
            let child_ns = namespace_for_children(&el.tag_name, &name.ns);
            let attrs = el
                .attributes
                .iter()
                .map(|(name, value)| Attribute {
                    name: attr_qual_name_from_str(name),
                    value: value.clone(),
                })
                .collect();
            let id = mutator.create_element(name, attrs);
            maps.legacy_to_blitz.insert(legacy_node_key(node), id);
            maps.blitz_to_legacy.insert(id, node.clone());
            append_dom_children(mutator, id, node, &child_ns, maps);
            id
        }
    }
}

fn blitz_ids_for_dom_child(
    mutator: &mut DocumentMutator<'_>,
    child: &NodePtr,
    parent_ns: &markup5ever::Namespace,
    maps: &mut BlitzNodeMaps<'_>,
) -> Vec<usize> {
    if is_document_fragment(child) {
        child_nodes_for_blitz(child)
            .into_iter()
            .map(|child| create_dom_node(mutator, &child, parent_ns, maps))
            .collect()
    } else {
        vec![create_dom_node(mutator, child, parent_ns, maps)]
    }
}

fn child_nodes_for_blitz(node: &NodePtr) -> Vec<NodePtr> {
    match &*node.borrow() {
        Node::Document { children, .. } => children.clone(),
        Node::Element(el) => {
            let mut children = el.children.clone();
            if el.tag_name.eq_ignore_ascii_case("template") {
                if let Some(content) = &el.template_contents {
                    children.extend(child_nodes_for_blitz(content));
                }
            }
            if let Some(shadow_root) = &el.shadow_root {
                children.push(shadow_root.clone());
            }
            children
        }
        Node::Text(_) => Vec::new(),
    }
}

fn remove_subtree_mappings_for_children(
    node: &NodePtr,
    legacy_to_blitz: &mut BTreeMap<usize, usize>,
    blitz_to_legacy: &mut BTreeMap<usize, NodePtr>,
) {
    for child in child_nodes_for_blitz(node) {
        remove_subtree_mapping(&child, legacy_to_blitz, blitz_to_legacy);
    }
}

fn remove_subtree_mapping(
    node: &NodePtr,
    legacy_to_blitz: &mut BTreeMap<usize, usize>,
    blitz_to_legacy: &mut BTreeMap<usize, NodePtr>,
) {
    if let Some(blitz_id) = legacy_to_blitz.remove(&legacy_node_key(node)) {
        blitz_to_legacy.remove(&blitz_id);
    }
    for child in child_nodes_for_blitz(node) {
        remove_subtree_mapping(&child, legacy_to_blitz, blitz_to_legacy);
    }
}

fn document_bottom(node: &blitz_dom::Node) -> f32 {
    let own_bottom = node.final_layout.location.y + node.final_layout.size.height;
    node.children
        .iter()
        .map(|&child_id| document_bottom(node.with(child_id)))
        .fold(own_bottom, f32::max)
}

fn legacy_node_key(node: &NodePtr) -> usize {
    std::rc::Rc::as_ptr(node) as usize
}

fn is_document_fragment(node: &NodePtr) -> bool {
    matches!(&*node.borrow(), Node::Element(el) if el.tag_name == "#document-fragment")
}

fn element_qual_name_from_str(name: &str, parent_ns: &markup5ever::Namespace) -> QualName {
    QualName {
        prefix: None,
        ns: namespace_for_element(name, parent_ns),
        local: LocalName::from(name),
    }
}

fn attr_qual_name_from_str(name: &str) -> QualName {
    if let Some(local) = name.strip_prefix("xlink:") {
        return QualName {
            prefix: Some(namespace_prefix!("xlink")),
            ns: ns!(xlink),
            local: LocalName::from(local),
        };
    }
    if let Some(local) = name.strip_prefix("xml:") {
        return QualName {
            prefix: Some(namespace_prefix!("xml")),
            ns: ns!(xml),
            local: LocalName::from(local),
        };
    }
    if let Some(local) = name.strip_prefix("xmlns:") {
        return QualName {
            prefix: Some(namespace_prefix!("xmlns")),
            ns: ns!(xmlns),
            local: LocalName::from(local),
        };
    }
    if name == "xmlns" {
        return QualName {
            prefix: None,
            ns: ns!(xmlns),
            local: LocalName::from("xmlns"),
        };
    }

    QualName {
        prefix: None,
        ns: ns!(),
        local: LocalName::from(name),
    }
}

fn namespace_for_element(name: &str, parent_ns: &markup5ever::Namespace) -> markup5ever::Namespace {
    let lower = name.to_ascii_lowercase();
    if lower == "svg" {
        ns!(svg)
    } else if lower == "math" {
        ns!(mathml)
    } else if *parent_ns == ns!(svg) && lower == "foreignobject" {
        ns!(svg)
    } else if *parent_ns == ns!(svg) && lower != "foreignobject" {
        ns!(svg)
    } else if *parent_ns == ns!(mathml) {
        ns!(mathml)
    } else {
        ns!(html)
    }
}

fn namespace_for_children(
    name: &str,
    element_ns: &markup5ever::Namespace,
) -> markup5ever::Namespace {
    if *element_ns == ns!(svg) && name.eq_ignore_ascii_case("foreignObject") {
        ns!(html)
    } else {
        element_ns.clone()
    }
}

fn catch_stylo_panic<T>(context: &str, f: impl FnOnce() -> T) -> Option<T> {
    match panic::catch_unwind(AssertUnwindSafe(f)) {
        Ok(value) => Some(value),
        Err(payload) => {
            log::warn!(
                "Blitz/Stylo panicked while {context}: {}",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_elements_stay_html_outside_foreign_content() {
        assert_eq!(element_qual_name_from_str("a", &ns!(html)).ns, ns!(html));
        assert_eq!(element_qual_name_from_str("div", &ns!(html)).ns, ns!(html));
    }

    #[test]
    fn svg_and_mathml_roots_enter_foreign_namespaces() {
        assert_eq!(element_qual_name_from_str("svg", &ns!(html)).ns, ns!(svg));
        assert_eq!(element_qual_name_from_str("circle", &ns!(svg)).ns, ns!(svg));
        assert_eq!(
            element_qual_name_from_str("math", &ns!(html)).ns,
            ns!(mathml)
        );
        assert_eq!(
            element_qual_name_from_str("mi", &ns!(mathml)).ns,
            ns!(mathml)
        );
    }

    #[test]
    fn foreign_object_children_return_to_html_namespace() {
        let foreign = element_qual_name_from_str("foreignObject", &ns!(svg));
        let child_ns = namespace_for_children("foreignObject", &foreign.ns);
        assert_eq!(foreign.ns, ns!(svg));
        assert_eq!(element_qual_name_from_str("div", &child_ns).ns, ns!(html));
    }

    #[test]
    fn prefixed_foreign_attrs_get_real_namespaces() {
        let xlink = attr_qual_name_from_str("xlink:href");
        assert_eq!(xlink.prefix, Some(namespace_prefix!("xlink")));
        assert_eq!(xlink.ns, ns!(xlink));
        assert_eq!(xlink.local, local_name!("href"));

        let xmlns = attr_qual_name_from_str("xmlns:xlink");
        assert_eq!(xmlns.prefix, Some(namespace_prefix!("xmlns")));
        assert_eq!(xmlns.ns, ns!(xmlns));
        assert_eq!(xmlns.local, local_name!("xlink"));
    }
}
