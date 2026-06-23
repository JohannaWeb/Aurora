use anyrender::PaintScene;
use anyrender_vello::VelloScenePainter;
use blitz_dom::{Attribute, BaseDocument, DocumentConfig, DocumentMutator};
use blitz_html::HtmlDocument;
use blitz_traits::net::{Bytes, NetHandler, NetProvider, Request};
use blitz_traits::shell::{ColorScheme, Viewport};
use markup5ever::{LocalName, QualName, local_name, namespace_prefix, ns};
use vello::Scene;

use crate::dom::{Node, NodePtr, ShadowTreeBackend};
use crate::identity::Identity;

use std::collections::BTreeMap;
use std::panic::{self, AssertUnwindSafe};
use std::rc::Rc;
use std::sync::{Mutex, OnceLock, PoisonError};

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
        if let Some(cached_bytes) = cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&url)
        {
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

#[cfg(debug_assertions)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirrorIntegrityError {
    pub operation: &'static str,
    pub legacy_node: Option<usize>,
    pub blitz_node: Option<usize>,
    pub message: String,
}

#[cfg(debug_assertions)]
impl MirrorIntegrityError {
    fn new(
        operation: &'static str,
        legacy_node: Option<usize>,
        blitz_node: Option<usize>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            operation,
            legacy_node,
            blitz_node,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MirrorMutationResult {
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MirrorMutationFailure {
    MissingMapping,
    SyncOperationFailed,
    DebugValidationFailed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirrorMutationTrace {
    pub op_id: u64,
    pub op_name: &'static str,
    pub legacy_node: Option<usize>,
    pub blitz_node: Option<usize>,
    pub parent: Option<usize>,
    pub child: Option<usize>,
    pub shadow_root: bool,
    pub result: MirrorMutationResult,
    pub failure: Option<MirrorMutationFailure>,
}

/// The descriptive fields a mirror sync op records the same way on both its
/// failure and success paths. Built once per op instead of being threaded
/// through two eight-argument telemetry calls. The reported `blitz_node` is
/// passed separately at each call site because it differs by phase (absent on a
/// missing-mapping failure) and by op (e.g. `sync_insert_before` resolves the
/// parent's id but reports the inserted child's). A few ops also record a
/// different `legacy_node` subject on failure than on success; those override
/// just that field on the failure path via `MirrorOp { legacy_node, ..op }`.
#[derive(Clone, Copy)]
struct MirrorOp {
    op_name: &'static str,
    legacy_node: Option<usize>,
    parent: Option<usize>,
    child: Option<usize>,
    shadow_root: bool,
}

impl MirrorOp {
    /// A mutation on a single node (attribute/text ops): the node is the
    /// subject, its parent is recorded for context, no `child`.
    fn for_node(op_name: &'static str, node: &NodePtr) -> Self {
        MirrorOp {
            op_name,
            legacy_node: Some(legacy_node_key(node)),
            parent: crate::dom::parent_ptr(node).map(|p| legacy_node_key(&p)),
            child: None,
            shadow_root: is_shadow_root_node(node),
        }
    }

    /// A mutation on a parent's whole child list (clear/replace children): the
    /// parent is both subject and parent context.
    fn for_parent(op_name: &'static str, parent: &NodePtr) -> Self {
        MirrorOp {
            op_name,
            legacy_node: Some(legacy_node_key(parent)),
            parent: Some(legacy_node_key(parent)),
            child: None,
            shadow_root: is_shadow_root_node(parent),
        }
    }
}

/// Border-box geometry of a DOM node from the Blitz/Stylo layout, in
/// document-relative (pre-scroll) coordinates. Backs the JS layout accessors.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LayoutMetrics {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub enum PaintResult {
    PaintedCurrentFrame,
    PreservedLastGoodFrame,
    FailedRecoverable,
    FailedUnhealthy,
}

pub struct BlitzDocument {
    inner: BaseDocument,
    healthy: bool,
    consecutive_panics: u32,
    next_mirror_op_id: u64,
    last_mirror_mutation: Option<MirrorMutationTrace>,
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

    #[allow(clippy::manual_filter)] // `resolve_inner` requires mutable ownership.
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
                next_mirror_op_id: 1,
                last_mirror_mutation: None,
                legacy_to_blitz: BTreeMap::new(),
                blitz_to_legacy: BTreeMap::new(),
            }
        })
        .and_then(|mut doc| if doc.resolve_inner() { Some(doc) } else { None })
    }

    #[allow(clippy::manual_filter)] // `resolve_inner` requires mutable ownership.
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
                maps.legacy_to_blitz.insert(legacy_node_key(dom), 0);
                maps.blitz_to_legacy.insert(0, dom.clone());
                append_dom_children(&mut mutator, 0, dom, &ns!(html), &mut maps);
            }
            BlitzDocument {
                inner,
                healthy: true,
                consecutive_panics: 0,
                next_mirror_op_id: 1,
                last_mirror_mutation: None,
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
        // See resolve_inner: once unhealthy, skip the Stylo call so a resize
        // storm cannot re-trigger the upstream invalidation panic every frame.
        if !self.healthy {
            return false;
        }
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

    pub fn perform_layout(&mut self) -> bool {
        self.resolve_inner()
    }

    pub fn last_mirror_mutation_trace(&self) -> Option<&MirrorMutationTrace> {
        self.last_mirror_mutation.as_ref()
    }

    #[cfg(debug_assertions)]
    #[allow(dead_code)]
    pub fn validate_mirror_integrity(&self) -> Result<(), MirrorIntegrityError> {
        self.validate_mirror_integrity_for("manual validation")
    }

    #[cfg(debug_assertions)]
    fn validate_mirror_integrity_for(
        &self,
        operation: &'static str,
    ) -> Result<(), MirrorIntegrityError> {
        for (legacy_id, blitz_id) in &self.legacy_to_blitz {
            let Some(dom_node) = self.blitz_to_legacy.get(blitz_id) else {
                return Err(MirrorIntegrityError::new(
                    operation,
                    Some(*legacy_id),
                    Some(*blitz_id),
                    "legacy_to_blitz entry has no reverse blitz_to_legacy entry",
                ));
            };
            let actual_legacy_id = legacy_node_key(dom_node);
            if actual_legacy_id != *legacy_id {
                return Err(MirrorIntegrityError::new(
                    operation,
                    Some(*legacy_id),
                    Some(*blitz_id),
                    format!("reverse map points at different legacy node {actual_legacy_id}"),
                ));
            }
            let Some(blitz_node) = self.inner.get_node(*blitz_id) else {
                return Err(MirrorIntegrityError::new(
                    operation,
                    Some(*legacy_id),
                    Some(*blitz_id),
                    "mapped Blitz node no longer exists",
                ));
            };
            self.validate_mapped_node_state(
                operation, dom_node, *legacy_id, *blitz_id, blitz_node,
            )?;
        }

        for (blitz_id, dom_node) in &self.blitz_to_legacy {
            let legacy_id = legacy_node_key(dom_node);
            match self.legacy_to_blitz.get(&legacy_id) {
                Some(mapped_blitz_id) if mapped_blitz_id == blitz_id => {}
                Some(mapped_blitz_id) => {
                    return Err(MirrorIntegrityError::new(
                        operation,
                        Some(legacy_id),
                        Some(*blitz_id),
                        format!("reverse map points at Blitz node {mapped_blitz_id}"),
                    ));
                }
                None => {
                    return Err(MirrorIntegrityError::new(
                        operation,
                        Some(legacy_id),
                        Some(*blitz_id),
                        "blitz_to_legacy entry has no reverse legacy_to_blitz entry",
                    ));
                }
            }
            if self.inner.get_node(*blitz_id).is_none() {
                return Err(MirrorIntegrityError::new(
                    operation,
                    Some(legacy_id),
                    Some(*blitz_id),
                    "reverse-mapped Blitz node no longer exists",
                ));
            }
        }

        Ok(())
    }

    #[cfg(debug_assertions)]
    fn validate_mapped_node_state(
        &self,
        operation: &'static str,
        dom_node: &NodePtr,
        legacy_id: usize,
        blitz_id: usize,
        blitz_node: &blitz_dom::Node,
    ) -> Result<(), MirrorIntegrityError> {
        match &*dom_node.borrow() {
            Node::Document { .. } => {
                if !matches!(blitz_node.data, blitz_dom::NodeData::Document)
                    && blitz_node.data.downcast_element().is_none()
                {
                    return Err(MirrorIntegrityError::new(
                        operation,
                        Some(legacy_id),
                        Some(blitz_id),
                        "legacy document maps to non-document/non-fragment Blitz node",
                    ));
                }
            }
            Node::Text(text) => match &blitz_node.data {
                blitz_dom::NodeData::Text(blitz_text) if blitz_text.content == text.content => {}
                blitz_dom::NodeData::Text(blitz_text) => {
                    return Err(MirrorIntegrityError::new(
                        operation,
                        Some(legacy_id),
                        Some(blitz_id),
                        format!(
                            "text mismatch: legacy={:?} blitz={:?}",
                            text.content, blitz_text.content
                        ),
                    ));
                }
                _ => {
                    return Err(MirrorIntegrityError::new(
                        operation,
                        Some(legacy_id),
                        Some(blitz_id),
                        "legacy text node maps to non-text Blitz node",
                    ));
                }
            },
            Node::Element(el) if is_shadow_root_node(dom_node) => {
                let Some(blitz_el) = blitz_node.data.downcast_element() else {
                    return Err(MirrorIntegrityError::new(
                        operation,
                        Some(legacy_id),
                        Some(blitz_id),
                        "shadow root maps to non-element Blitz node",
                    ));
                };
                if blitz_el.name.local.as_ref() != "div" {
                    return Err(MirrorIntegrityError::new(
                        operation,
                        Some(legacy_id),
                        Some(blitz_id),
                        format!(
                            "shadow root synthetic node is <{}>, expected <div>",
                            blitz_el.name.local
                        ),
                    ));
                }
                if blitz_node
                    .data
                    .attr(LocalName::from("data-aurora-shadow-root"))
                    != Some("true")
                {
                    return Err(MirrorIntegrityError::new(
                        operation,
                        Some(legacy_id),
                        Some(blitz_id),
                        "shadow root synthetic node is missing data-aurora-shadow-root=true",
                    ));
                }
                let Some(host) = crate::dom::parent_ptr(dom_node) else {
                    return Err(MirrorIntegrityError::new(
                        operation,
                        Some(legacy_id),
                        Some(blitz_id),
                        "shadow root has no legacy host parent",
                    ));
                };
                if let Some(host_id) = self.blitz_node_id_for_dom(&host) {
                    if blitz_node.parent != Some(host_id) {
                        return Err(MirrorIntegrityError::new(
                            operation,
                            Some(legacy_id),
                            Some(blitz_id),
                            format!(
                                "shadow root parent mismatch: blitz={:?} expected={host_id}",
                                blitz_node.parent
                            ),
                        ));
                    }
                }
                if let Node::Element(host_el) = &*host.borrow() {
                    let expected_host_tag = host_el.tag_name.as_str();
                    if blitz_node
                        .data
                        .attr(LocalName::from("data-aurora-shadow-host"))
                        != Some(expected_host_tag)
                    {
                        return Err(MirrorIntegrityError::new(
                            operation,
                            Some(legacy_id),
                            Some(blitz_id),
                            format!("shadow host marker mismatch: expected {expected_host_tag}"),
                        ));
                    }
                }
                let _ = el;
            }
            Node::Element(el) => {
                let Some(blitz_el) = blitz_node.data.downcast_element() else {
                    return Err(MirrorIntegrityError::new(
                        operation,
                        Some(legacy_id),
                        Some(blitz_id),
                        "legacy element maps to non-element Blitz node",
                    ));
                };
                if blitz_el.name.local.as_ref() != el.tag_name.as_str() {
                    return Err(MirrorIntegrityError::new(
                        operation,
                        Some(legacy_id),
                        Some(blitz_id),
                        format!(
                            "tag mismatch: legacy=<{}> blitz=<{}>",
                            el.tag_name, blitz_el.name.local
                        ),
                    ));
                }
                let blitz_attrs = blitz_attrs_to_map(blitz_el);
                for (name, value) in &el.attributes {
                    if blitz_attrs.get(name) != Some(value) {
                        return Err(MirrorIntegrityError::new(
                            operation,
                            Some(legacy_id),
                            Some(blitz_id),
                            format!("attribute mismatch for {name:?}"),
                        ));
                    }
                }
                for name in blitz_attrs.keys() {
                    if !el.attributes.contains_key(name) {
                        return Err(MirrorIntegrityError::new(
                            operation,
                            Some(legacy_id),
                            Some(blitz_id),
                            format!("unexpected Blitz attribute {name:?}"),
                        ));
                    }
                }
            }
        }

        let expected_children = child_nodes_for_blitz(dom_node)
            .into_iter()
            .filter_map(|child| self.blitz_node_id_for_dom(&child))
            .collect::<Vec<_>>();
        if blitz_node.children != expected_children {
            return Err(MirrorIntegrityError::new(
                operation,
                Some(legacy_id),
                Some(blitz_id),
                format!(
                    "child mapping mismatch: blitz={:?} expected={expected_children:?}",
                    blitz_node.children
                ),
            ));
        }

        for child_id in &expected_children {
            let Some(child_node) = self.inner.get_node(*child_id) else {
                return Err(MirrorIntegrityError::new(
                    operation,
                    Some(legacy_id),
                    Some(*child_id),
                    "mapped child Blitz node no longer exists",
                ));
            };
            if child_node.parent != Some(blitz_id) {
                return Err(MirrorIntegrityError::new(
                    operation,
                    Some(legacy_id),
                    Some(*child_id),
                    format!(
                        "child parent mismatch: child parent={:?} expected={blitz_id}",
                        child_node.parent
                    ),
                ));
            }
        }

        Ok(())
    }

    #[cfg(debug_assertions)]
    fn debug_validate_mirror_after(
        &self,
        operation: &'static str,
    ) -> Result<(), MirrorIntegrityError> {
        self.validate_mirror_integrity_for(operation)
            .map_err(|error| {
                log::error!(
                    "Blitz mirror integrity check failed after {}: {:?}",
                    operation,
                    error
                );
                error
            })
    }

    #[cfg(not(debug_assertions))]
    fn debug_validate_mirror_after(&self, _operation: &'static str) -> Result<(), ()> {
        Ok(())
    }

    fn record_mirror_mutation(
        &mut self,
        op: &MirrorOp,
        blitz_node: Option<usize>,
        result: MirrorMutationResult,
        failure: Option<MirrorMutationFailure>,
    ) {
        let trace = MirrorMutationTrace {
            op_id: self.next_mirror_op_id,
            op_name: op.op_name,
            legacy_node: op.legacy_node,
            blitz_node,
            parent: op.parent,
            child: op.child,
            shadow_root: op.shadow_root,
            result,
            failure,
        };
        self.next_mirror_op_id += 1;
        if matches!(
            std::env::var("AURORA_DEBUG_MIRROR_MUTATIONS").as_deref(),
            Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes") | Ok("YES")
        ) {
            log::debug!("Blitz mirror mutation trace: {:?}", trace);
        }
        self.last_mirror_mutation = Some(trace);
    }

    fn record_mirror_failure(&mut self, op: &MirrorOp, failure: MirrorMutationFailure) {
        self.record_mirror_mutation(op, None, MirrorMutationResult::Failed, Some(failure));
    }

    fn finish_mirror_mutation(&mut self, op: &MirrorOp, blitz_node: Option<usize>, updated: bool) {
        if updated {
            self.consecutive_panics = 0;
            let validation_failed = self.debug_validate_mirror_after(op.op_name).is_err();
            self.record_mirror_mutation(
                op,
                blitz_node,
                if validation_failed {
                    MirrorMutationResult::Failed
                } else {
                    MirrorMutationResult::Succeeded
                },
                validation_failed.then_some(MirrorMutationFailure::DebugValidationFailed),
            );
        } else {
            self.record_mirror_mutation(
                op,
                blitz_node,
                MirrorMutationResult::Failed,
                Some(MirrorMutationFailure::SyncOperationFailed),
            );
        }
    }

    pub fn dom_node_for_blitz_id(&self, node_id: usize) -> Option<NodePtr> {
        self.blitz_to_legacy.get(&node_id).cloned()
    }

    #[allow(dead_code)]
    pub fn query_selector_dom(&self, selector: &str, start: &NodePtr) -> Option<NodePtr> {
        let start_id = self.blitz_node_id_for_dom(start)?;
        self.inner
            .query_selector_all(selector)
            .ok()?
            .into_iter()
            .filter(|node_id| self.blitz_is_descendant_or_self(*node_id, start_id))
            .filter_map(|node_id| self.dom_node_for_blitz_id(node_id))
            .find(|node| is_element_node(node) && query_can_see(start, node))
    }

    #[allow(dead_code)]
    pub fn query_selector_all_dom(&self, selector: &str, start: &NodePtr) -> Option<Vec<NodePtr>> {
        let start_id = self.blitz_node_id_for_dom(start)?;
        let nodes = self
            .inner
            .query_selector_all(selector)
            .ok()?
            .into_iter()
            .filter(|node_id| self.blitz_is_descendant_or_self(*node_id, start_id))
            .filter_map(|node_id| self.dom_node_for_blitz_id(node_id))
            .filter(|node| is_element_node(node) && query_can_see(start, node))
            .collect();
        Some(nodes)
    }

    #[allow(dead_code)]
    pub fn get_element_by_id_dom(&self, id: &str) -> Option<NodePtr> {
        self.inner
            .get_element_by_id(id)
            .and_then(|node_id| self.dom_node_for_blitz_id(node_id))
            .filter(|node| is_element_node(node) && nearest_shadow_root(node).is_none())
    }

    pub fn collect_by_tag_dom(&self, tag: &str, start: &NodePtr) -> Option<Vec<NodePtr>> {
        let start_id = self.blitz_node_id_for_dom(start)?;
        let mut out = Vec::new();
        self.collect_by_tag_from_blitz(start_id, tag, start, &mut out);
        Some(out)
    }

    #[allow(dead_code)]
    pub fn selector_matches_dom(&self, node: &NodePtr, selector: &str) -> Option<bool> {
        let node_id = self.blitz_node_id_for_dom(node)?;
        Some(
            self.inner
                .query_selector_all(selector)
                .ok()?
                .into_iter()
                .any(|id| id == node_id),
        )
    }

    #[allow(dead_code)]
    pub fn closest_dom(&self, node: &NodePtr, selector: &str) -> Option<Option<NodePtr>> {
        let mut current = self.blitz_node_id_for_dom(node)?;
        let matches = self.inner.query_selector_all(selector).ok()?;
        loop {
            if matches.contains(&current) {
                return Some(self.dom_node_for_blitz_id(current).filter(is_element_node));
            }
            let Some(parent) = self.inner.get_node(current).and_then(|node| node.parent) else {
                return Some(None);
            };
            current = parent;
            if let Some(dom_node) = self.dom_node_for_blitz_id(current) {
                if is_shadow_root_node(&dom_node) {
                    return Some(None);
                }
            }
        }
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

    /// Border-box geometry of `node` from the Blitz/Stylo layout that produced
    /// the current pixels. Position is document-relative (sum of taffy locations
    /// up the box tree), pre-scroll. Returns `None` if the node isn't mapped into
    /// the render tree. This is the authoritative source for JS layout accessors
    /// (`offsetWidth`, `getBoundingClientRect`, …) in normal Blitz mode.
    pub fn dom_node_layout_metrics(&self, node: &NodePtr) -> Option<LayoutMetrics> {
        let node_id = self.blitz_node_id_for_dom(node)?;
        let size = self.inner.get_node(node_id)?.final_layout.size;
        let (mut x, mut y) = (0.0_f32, 0.0_f32);
        let mut current = Some(node_id);
        while let Some(id) = current {
            let Some(n) = self.inner.get_node(id) else {
                break;
            };
            x += n.final_layout.location.x;
            y += n.final_layout.location.y;
            current = n.parent;
        }
        Some(LayoutMetrics {
            x,
            y,
            width: size.width,
            height: size.height,
        })
    }

    #[allow(dead_code)]
    fn blitz_is_descendant_or_self(&self, mut node_id: usize, ancestor_id: usize) -> bool {
        loop {
            if node_id == ancestor_id {
                return true;
            }
            let Some(parent) = self.inner.get_node(node_id).and_then(|node| node.parent) else {
                return false;
            };
            node_id = parent;
        }
    }

    fn collect_by_tag_from_blitz(
        &self,
        node_id: usize,
        tag: &str,
        start: &NodePtr,
        out: &mut Vec<NodePtr>,
    ) {
        let Some(node) = self.inner.get_node(node_id) else {
            return;
        };
        if let Some(element) = node.data.downcast_element() {
            if (tag == "*" || element.name.local.as_ref().eq_ignore_ascii_case(tag))
                && let Some(dom_node) = self.dom_node_for_blitz_id(node_id)
                && is_element_node(&dom_node)
                && query_can_see(start, &dom_node)
            {
                out.push(dom_node);
            }
        }
        for child_id in node.children.iter().copied() {
            self.collect_by_tag_from_blitz(child_id, tag, start, out);
        }
    }

    #[allow(dead_code)]
    pub fn sync_append_child(&mut self, parent: &NodePtr, child: &NodePtr) -> bool {
        let op = MirrorOp {
            op_name: "sync_append_child",
            legacy_node: Some(legacy_node_key(child)),
            parent: Some(legacy_node_key(parent)),
            child: Some(legacy_node_key(child)),
            shadow_root: is_shadow_root_node(child),
        };
        let Some(parent_id) = self.blitz_node_id_for_dom(parent) else {
            self.record_mirror_failure(
                &MirrorOp {
                    legacy_node: Some(legacy_node_key(parent)),
                    ..op
                },
                MirrorMutationFailure::MissingMapping,
            );
            return false;
        };
        let parent_ns = self.node_namespace(parent_id).unwrap_or(ns!(html));
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
        let child_id = self.blitz_node_id_for_dom(child);
        self.finish_mirror_mutation(&op, child_id, updated);
        updated
    }

    pub fn sync_insert_before(
        &mut self,
        parent: &NodePtr,
        new_child: &NodePtr,
        ref_child: Option<&NodePtr>,
    ) -> bool {
        let op = MirrorOp {
            op_name: "sync_insert_before",
            legacy_node: Some(legacy_node_key(new_child)),
            parent: Some(legacy_node_key(parent)),
            child: Some(legacy_node_key(new_child)),
            shadow_root: is_shadow_root_node(new_child),
        };
        let Some(parent_id) = self.blitz_node_id_for_dom(parent) else {
            self.record_mirror_failure(
                &MirrorOp {
                    legacy_node: Some(legacy_node_key(parent)),
                    ..op
                },
                MirrorMutationFailure::MissingMapping,
            );
            return false;
        };
        let parent_ns = self.node_namespace(parent_id).unwrap_or(ns!(html));
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
        let child_id = self.blitz_node_id_for_dom(new_child);
        self.finish_mirror_mutation(&op, child_id, updated);
        updated
    }

    pub fn sync_remove_child(&mut self, child: &NodePtr) -> bool {
        let op = MirrorOp {
            op_name: "sync_remove_child",
            legacy_node: Some(legacy_node_key(child)),
            parent: crate::dom::parent_ptr(child).map(|parent| legacy_node_key(&parent)),
            child: Some(legacy_node_key(child)),
            shadow_root: is_shadow_root_node(child),
        };
        let Some(child_id) = self.blitz_node_id_for_dom(child) else {
            self.record_mirror_failure(&op, MirrorMutationFailure::MissingMapping);
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
        }
        self.finish_mirror_mutation(&op, Some(child_id), updated);
        updated
    }

    #[allow(dead_code)]
    pub fn sync_replace_child(
        &mut self,
        parent: &NodePtr,
        new_child: &NodePtr,
        old_child: &NodePtr,
    ) -> bool {
        let op = MirrorOp {
            op_name: "sync_replace_child",
            legacy_node: Some(legacy_node_key(new_child)),
            parent: Some(legacy_node_key(parent)),
            child: Some(legacy_node_key(new_child)),
            shadow_root: is_shadow_root_node(new_child) || is_shadow_root_node(old_child),
        };
        let Some(old_id) = self.blitz_node_id_for_dom(old_child) else {
            self.record_mirror_failure(
                &MirrorOp {
                    legacy_node: Some(legacy_node_key(old_child)),
                    ..op
                },
                MirrorMutationFailure::MissingMapping,
            );
            return false;
        };
        let parent_ns = self
            .blitz_node_id_for_dom(parent)
            .and_then(|id| self.node_namespace(id))
            .unwrap_or(ns!(html));
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
        }
        let new_id = self.blitz_node_id_for_dom(new_child);
        self.finish_mirror_mutation(&op, new_id.or(Some(old_id)), updated);
        updated
    }

    pub fn sync_set_attribute(&mut self, node: &NodePtr, name: &str, value: &str) -> bool {
        let op = MirrorOp::for_node("sync_set_attribute", node);
        let Some(node_id) = self.blitz_node_id_for_dom(node) else {
            self.record_mirror_failure(&op, MirrorMutationFailure::MissingMapping);
            return false;
        };
        let updated = catch_stylo_panic("setting DOM attribute in Blitz document", || {
            let mut mutator = self.inner.mutate();
            mutator.set_attribute(node_id, attr_qual_name_from_str(name), value);
            true
        })
        .unwrap_or(false);
        self.finish_mirror_mutation(&op, Some(node_id), updated);
        updated
    }

    pub fn sync_remove_attribute(&mut self, node: &NodePtr, name: &str) -> bool {
        let op = MirrorOp::for_node("sync_remove_attribute", node);
        let Some(node_id) = self.blitz_node_id_for_dom(node) else {
            self.record_mirror_failure(&op, MirrorMutationFailure::MissingMapping);
            return false;
        };
        let updated = catch_stylo_panic("removing DOM attribute from Blitz document", || {
            let mut mutator = self.inner.mutate();
            mutator.clear_attribute(node_id, attr_qual_name_from_str(name));
            true
        })
        .unwrap_or(false);
        self.finish_mirror_mutation(&op, Some(node_id), updated);
        updated
    }

    pub fn sync_all_attributes(&mut self, node: &NodePtr) -> bool {
        let op = MirrorOp::for_node("sync_all_attributes", node);
        let Some(node_id) = self.blitz_node_id_for_dom(node) else {
            self.record_mirror_failure(&op, MirrorMutationFailure::MissingMapping);
            return false;
        };
        let Ok(node_borrow) = node.try_borrow() else {
            self.record_mirror_mutation(
                &op,
                Some(node_id),
                MirrorMutationResult::Failed,
                Some(MirrorMutationFailure::SyncOperationFailed),
            );
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
            // Diff existing Blitz attributes against the desired DOM attributes
            // and apply only real changes. A blanket clear-all-then-set-all
            // churns every attribute; in particular clearing `href` on a
            // `<link>` whose stylesheet has not finished its async load makes
            // blitz-dom panic in `unload_stylesheet` (`unreachable!()` at
            // mutator.rs), which we catch but only by forcing a full snapshot
            // rebuild. Polymer re-stamps already-present attributes constantly,
            // so diffing keeps those as in-place value updates (set_attribute)
            // instead of a remove+add that trips the stylesheet-unload path.
            let existing: Vec<(QualName, String)> = self
                .inner
                .get_node(node_id)
                .and_then(|node| node.data.downcast_element())
                .map(|element| {
                    element
                        .attrs
                        .iter()
                        .map(|attr| (attr.name.clone(), attr.value.clone()))
                        .collect()
                })
                .unwrap_or_default();
            let desired: Vec<(QualName, String)> = attrs
                .iter()
                .map(|(name, value)| (attr_qual_name_from_str(name), value.clone()))
                .collect();

            let mut mutator = self.inner.mutate();
            // Remove attributes that are no longer present in the DOM.
            for (name, _) in &existing {
                if !desired.iter().any(|(dname, _)| dname == name) {
                    mutator.clear_attribute(node_id, name.clone());
                }
            }
            // Add new attributes and update those whose value changed.
            for (name, value) in &desired {
                let unchanged = existing
                    .iter()
                    .any(|(ename, evalue)| ename == name && evalue == value);
                if !unchanged {
                    mutator.set_attribute(node_id, name.clone(), value);
                }
            }
            true
        })
        .unwrap_or(false);
        self.finish_mirror_mutation(&op, Some(node_id), updated);
        updated
    }

    pub fn sync_text_node(&mut self, node: &NodePtr, text: &str) -> bool {
        let op = MirrorOp::for_node("sync_text_node", node);
        let Some(node_id) = self.blitz_node_id_for_dom(node) else {
            self.record_mirror_failure(&op, MirrorMutationFailure::MissingMapping);
            return false;
        };
        let updated = catch_stylo_panic("setting DOM text in Blitz document", || {
            let mut mutator = self.inner.mutate();
            mutator.set_node_text(node_id, text);
            true
        })
        .unwrap_or(false);
        self.finish_mirror_mutation(&op, Some(node_id), updated);
        updated
    }

    pub fn sync_attach_shadow_root(
        &mut self,
        host: &NodePtr,
        shadow_root: &NodePtr,
        mode: &str,
    ) -> bool {
        let op = MirrorOp {
            op_name: "sync_attach_shadow_root",
            legacy_node: Some(legacy_node_key(shadow_root)),
            parent: Some(legacy_node_key(host)),
            child: Some(legacy_node_key(shadow_root)),
            shadow_root: true,
        };
        let Some(host_id) = self.blitz_node_id_for_dom(host) else {
            self.record_mirror_failure(&op, MirrorMutationFailure::MissingMapping);
            return false;
        };
        let host_ns = self.node_namespace(host_id).unwrap_or(ns!(html));
        let updated = catch_stylo_panic("attaching ShadowRoot in Blitz document", || {
            // Attaching a shadow root changes the host's *entire composed child
            // list*: light children that were previously rendered are replaced
            // by the synthetic shadow container. Appending the container while
            // retaining those old Blitz children leaves the two trees divergent.
            let existing_child_ids = self
                .inner
                .get_node(host_id)
                .map(|node| node.children.clone())
                .unwrap_or_default();
            for child_id in existing_child_ids {
                remove_blitz_subtree_mapping(
                    child_id,
                    &mut self.legacy_to_blitz,
                    &mut self.blitz_to_legacy,
                );
            }
            {
                let mut mutator = self.inner.mutate();
                mutator.remove_and_drop_all_children(host_id);
            }

            let child_ids = {
                let mut maps = BlitzNodeMaps {
                    legacy_to_blitz: &mut self.legacy_to_blitz,
                    blitz_to_legacy: &mut self.blitz_to_legacy,
                };
                let mut mutator = self.inner.mutate();
                let mut ids = Vec::new();
                for child in child_nodes_for_blitz(host) {
                    if Rc::ptr_eq(&child, shadow_root) {
                        ids.push(create_shadow_root_node(
                            &mut mutator,
                            &child,
                            &host_ns,
                            mode,
                            &mut maps,
                        ));
                    } else {
                        ids.extend(blitz_ids_for_dom_child(
                            &mut mutator,
                            &child,
                            &host_ns,
                            &mut maps,
                        ));
                    }
                }
                ids
            };
            if !child_ids.is_empty() {
                let mut mutator = self.inner.mutate();
                mutator.append_children(host_id, &child_ids);
            }
            true
        })
        .unwrap_or(false);
        let shadow_id = self.blitz_node_id_for_dom(shadow_root);
        self.finish_mirror_mutation(&op, shadow_id, updated);
        updated
    }

    pub fn sync_clear_children(&mut self, parent: &NodePtr) -> bool {
        let op = MirrorOp::for_parent("sync_clear_children", parent);
        let Some(parent_id) = self.blitz_node_id_for_dom(parent) else {
            self.record_mirror_failure(&op, MirrorMutationFailure::MissingMapping);
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
        self.finish_mirror_mutation(&op, Some(parent_id), updated);
        updated
    }

    pub fn sync_replace_children(&mut self, parent: &NodePtr) -> bool {
        let op = MirrorOp::for_parent("sync_replace_children", parent);
        let Some(parent_id) = self.blitz_node_id_for_dom(parent) else {
            self.record_mirror_failure(&op, MirrorMutationFailure::MissingMapping);
            return false;
        };
        let parent_ns = self.node_namespace(parent_id).unwrap_or(ns!(html));
        let updated = catch_stylo_panic("replacing DOM children in Blitz document", || {
            let existing_child_ids = self
                .inner
                .get_node(parent_id)
                .map(|node| node.children.clone())
                .unwrap_or_default();
            for child_id in existing_child_ids {
                remove_blitz_subtree_mapping(
                    child_id,
                    &mut self.legacy_to_blitz,
                    &mut self.blitz_to_legacy,
                );
            }
            {
                let mut mutator = self.inner.mutate();
                mutator.remove_and_drop_all_children(parent_id);
            }

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
        self.finish_mirror_mutation(&op, Some(parent_id), updated);
        updated
    }

    fn node_namespace(&self, node_id: usize) -> Option<markup5ever::Namespace> {
        self.inner
            .get_node(node_id)
            .and_then(|node| node.data.downcast_element())
            .map(|element| element.name.ns.clone())
    }

    pub fn paint_to_scene(&mut self, scene: &mut Scene, width: u32, height: u32) -> PaintResult {
        if !self.resolve_inner() {
            return self.paint_failure_result("resolving before paint");
        }
        let mut painter = VelloScenePainter::new(scene);
        let painted = catch_stylo_panic("painting Blitz document", || {
            blitz_paint::paint_scene(&mut painter, &mut self.inner, 1.0, width, height, 0, 0);
        })
        .is_some();
        self.finish_paint_attempt("paint_to_scene", painted)
    }

    pub fn paint_with(
        &mut self,
        painter: &mut impl PaintScene,
        width: u32,
        height: u32,
    ) -> PaintResult {
        if !self.resolve_inner() {
            return self.paint_failure_result("resolving before paint_with");
        }
        let painted = catch_stylo_panic("painting Blitz document", || {
            blitz_paint::paint_scene(painter, &mut self.inner, 1.0, width, height, 0, 0);
        })
        .is_some();
        self.finish_paint_attempt("paint_with", painted)
    }

    fn finish_paint_attempt(&mut self, context: &'static str, painted: bool) -> PaintResult {
        if painted {
            let previous_failures = self.consecutive_panics;
            self.healthy = true;
            self.consecutive_panics = 0;
            if previous_failures > 0 {
                log::info!(
                    "Blitz paint recovered in {context} after {previous_failures} consecutive failures"
                );
            }
            PaintResult::PaintedCurrentFrame
        } else {
            self.consecutive_panics += 1;
            if self.consecutive_panics >= MAX_CONSECUTIVE_PANICS {
                self.healthy = false;
            }
            self.paint_failure_result(context)
        }
    }

    fn paint_failure_result(&self, context: &'static str) -> PaintResult {
        if self.healthy {
            log::warn!(
                "Recoverable Blitz paint failure in {context}; consecutive_failures={}",
                self.consecutive_panics
            );
            PaintResult::FailedRecoverable
        } else {
            log::error!(
                "Unhealthy Blitz paint failure in {context}; consecutive_failures={}",
                self.consecutive_panics
            );
            PaintResult::FailedUnhealthy
        }
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
        // Once the Stylo circuit breaker has tripped, stop re-attempting the
        // resolve. The panic it guards against is an upstream Stylo bug
        // (servo/stylo#387, fixed on `main` but unreleased): an element reaches
        // style invalidation with `ElementStyles` allocated but no primary
        // computed style, and `ElementStyles::primary()` unwraps `None`. The
        // tree state is persistent, so every retry re-panics, and in the
        // windowed app each resize/redraw drives another resolve that would
        // otherwise spin on the panic forever. Skipping keeps the last
        // successfully resolved styles and layout (the last good frame).
        if !self.healthy {
            return false;
        }
        let resolved = catch_stylo_panic("resolving Blitz document", || {
            self.inner.resolve(0.0);
        })
        .is_some();
        if resolved {
            self.healthy = true;
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

    if is_shadow_root_node(node) {
        return create_shadow_root_node(mutator, node, parent_ns, "open", maps);
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

fn create_shadow_root_node(
    mutator: &mut DocumentMutator<'_>,
    shadow_root: &NodePtr,
    parent_ns: &markup5ever::Namespace,
    mode: &str,
    maps: &mut BlitzNodeMaps<'_>,
) -> usize {
    if let Some(&existing) = maps.legacy_to_blitz.get(&legacy_node_key(shadow_root)) {
        return existing;
    }
    let host_tag = crate::dom::parent_ptr(shadow_root)
        .and_then(|host| {
            host.try_borrow().ok().and_then(|node| match &*node {
                Node::Element(el) => Some(el.tag_name.clone()),
                _ => None,
            })
        })
        .unwrap_or_default();
    let attrs = vec![
        Attribute {
            name: attr_qual_name_from_str("data-aurora-shadow-root"),
            value: "true".to_string(),
        },
        Attribute {
            name: attr_qual_name_from_str("data-aurora-shadow-mode"),
            value: mode.to_string(),
        },
        Attribute {
            name: attr_qual_name_from_str("data-aurora-shadow-host"),
            value: host_tag,
        },
    ];
    let id = mutator.create_element(element_qual_name_from_str("div", parent_ns), attrs);
    maps.legacy_to_blitz
        .insert(legacy_node_key(shadow_root), id);
    maps.blitz_to_legacy.insert(id, shadow_root.clone());
    append_dom_children(mutator, id, shadow_root, parent_ns, maps);
    id
}

fn blitz_ids_for_dom_child(
    mutator: &mut DocumentMutator<'_>,
    child: &NodePtr,
    parent_ns: &markup5ever::Namespace,
    maps: &mut BlitzNodeMaps<'_>,
) -> Vec<usize> {
    if is_document_fragment(child) && !is_shadow_root_node(child) {
        child_nodes_for_blitz(child)
            .into_iter()
            .map(|child| create_dom_node(mutator, &child, parent_ns, maps))
            .collect()
    } else {
        vec![create_dom_node(mutator, child, parent_ns, maps)]
    }
}

fn child_nodes_for_blitz(node: &NodePtr) -> Vec<NodePtr> {
    let backend = crate::dom::SyntheticShadowTreeBackend;
    backend.distribute_slots(node);
    backend.composed_children(node)
}

fn is_element_node(node: &NodePtr) -> bool {
    matches!(&*node.borrow(), Node::Element(_))
}

fn is_shadow_root_node(node: &NodePtr) -> bool {
    crate::dom::SyntheticShadowTreeBackend.is_shadow_root(node)
}

fn nearest_shadow_root(node: &NodePtr) -> Option<NodePtr> {
    crate::dom::SyntheticShadowTreeBackend.nearest_shadow_root(node)
}

fn query_can_see(start: &NodePtr, found: &NodePtr) -> bool {
    match (nearest_shadow_root(start), nearest_shadow_root(found)) {
        (Some(start_root), Some(found_root)) => std::rc::Rc::ptr_eq(&start_root, &found_root),
        (None, None) => true,
        _ => false,
    }
}

#[cfg(debug_assertions)]
fn blitz_attrs_to_map(element: &blitz_dom::ElementData) -> BTreeMap<String, String> {
    element
        .attrs
        .iter()
        .map(|attr| (attr_qual_name_to_string(&attr.name), attr.value.clone()))
        .collect()
}

#[cfg(debug_assertions)]
fn attr_qual_name_to_string(name: &QualName) -> String {
    match name.prefix.as_ref() {
        Some(prefix) => format!("{}:{}", prefix, name.local),
        None => name.local.to_string(),
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

fn remove_blitz_subtree_mapping(
    node_id: usize,
    legacy_to_blitz: &mut BTreeMap<usize, usize>,
    blitz_to_legacy: &mut BTreeMap<usize, NodePtr>,
) {
    let Some(dom_node) = blitz_to_legacy.remove(&node_id) else {
        return;
    };
    legacy_to_blitz.remove(&legacy_node_key(&dom_node));
    for child in child_nodes_for_blitz(&dom_node) {
        if let Some(child_id) = legacy_to_blitz.get(&legacy_node_key(&child)).copied() {
            remove_blitz_subtree_mapping(child_id, legacy_to_blitz, blitz_to_legacy);
        }
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
    } else if *parent_ns == ns!(svg) {
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
            repair_leaked_style_thread_state();
            None
        }
    }
}

/// Repair Stylo's thread-local style state after a caught panic.
///
/// `blitz_dom::BaseDocument::resolve_stylist` brackets the style traversal with
/// `thread_state::enter(LAYOUT)` / `exit(LAYOUT)`. If the traversal panics, the
/// unwind skips `exit`, leaving the `LAYOUT` flag set on this thread. Every
/// later resolve then aborts at `thread_state::enter`'s
/// `debug_assert!(!current_state.intersects(LAYOUT))` — including the clean
/// snapshot rebuild that would otherwise paint, so the page is wedged forever.
///
/// The original trigger (stylo#387's `ElementStyles::primary` unwrap on a
/// content-bearing YouTube route) is now fixed at the source in our local stylo
/// fork (`third_party/stylo`, see `[patch.crates-io]`), so this no longer fires
/// on that path. It stays as a general safety net: any future Stylo panic that
/// we catch must not leave the thread-state wedged.
///
/// Clearing the leaked flags lets the next resolve start from a clean state.
/// This relies on Aurora's `style` dependency resolving to the exact same
/// compiled `stylo` crate as blitz-dom (pinned `=0.18.0`), so we touch the same
/// thread-local.
fn repair_leaked_style_thread_state() {
    use style::thread_state::{self, ThreadState};

    let leaked = thread_state::get() & ThreadState::LAYOUT;
    if !leaked.is_empty() {
        thread_state::exit(leaked);
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
    use crate::dom::{Node, set_parent};
    use crate::identity::{Capability, IdentityKind};
    use std::sync::Mutex;

    static BLITZ_DOCUMENT_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn test_identity() -> Identity {
        Identity::new(
            "did:aurora:test",
            "Aurora Test",
            IdentityKind::Agent,
            [Capability::ReadWorkspace, Capability::NetworkAccess],
        )
    }

    fn attr(name: &str, value: &str) -> BTreeMap<String, String> {
        BTreeMap::from([(name.to_string(), value.to_string())])
    }

    fn body_from_document(document: &NodePtr) -> NodePtr {
        let Node::Document { children, .. } = &*document.borrow() else {
            panic!("expected document node");
        };
        let html = children.first().expect("document should have html").clone();
        let Node::Element(html_el) = &*html.borrow() else {
            panic!("expected html element");
        };
        html_el
            .children
            .iter()
            .find(|child| matches!(&*child.borrow(), Node::Element(el) if el.tag_name == "body"))
            .expect("document should have body")
            .clone()
    }

    fn document_with_body_children(children: Vec<NodePtr>) -> NodePtr {
        let document = Node::document(vec![Node::element(
            "html",
            vec![
                Node::element("head", Vec::new()),
                Node::element("body", children),
            ],
        )]);
        crate::dom::reparent_subtree(&document);
        document
    }

    #[cfg(debug_assertions)]
    #[test]
    fn debug_mirror_integrity_accepts_initial_dom_snapshot() {
        let _guard = BLITZ_DOCUMENT_TEST_LOCK.lock().unwrap();
        let document = document_with_body_children(vec![Node::element_with_attributes(
            "p",
            attr("data-role", "greeting"),
            vec![Node::text("hello")],
        )]);

        let blitz_doc = BlitzDocument::try_from_dom(&document, None, &test_identity(), 800, 600)
            .expect("Blitz document should build");

        blitz_doc.validate_mirror_integrity().unwrap();
    }

    #[test]
    fn catch_stylo_panic_repairs_leaked_layout_thread_state() {
        use style::thread_state::{self, ThreadState};

        let _guard = BLITZ_DOCUMENT_TEST_LOCK.lock().unwrap();

        // Model `resolve_stylist`: it enters the LAYOUT thread-state, then the
        // style traversal panics (stylo#387) before the matching `exit`.
        let result: Option<()> = catch_stylo_panic("test traversal", || {
            thread_state::enter(ThreadState::LAYOUT);
            panic!("simulated stylo#387 traversal panic");
        });
        assert!(result.is_none());

        // Without the repair the leaked LAYOUT flag would wedge every later
        // resolve at `thread_state::enter`'s re-entry assertion. Confirm it is
        // cleared and a fresh enter/exit cycle succeeds.
        assert!(!thread_state::get().intersects(ThreadState::LAYOUT));
        thread_state::enter(ThreadState::LAYOUT);
        thread_state::exit(ThreadState::LAYOUT);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn debug_mirror_integrity_accepts_synced_dom_mutations() {
        let _guard = BLITZ_DOCUMENT_TEST_LOCK.lock().unwrap();
        let text = Node::text("old");
        let item = Node::element_with_attributes("p", attr("id", "item"), vec![text.clone()]);
        let document = document_with_body_children(vec![item.clone()]);
        let body = body_from_document(&document);
        let mut blitz_doc =
            BlitzDocument::try_from_dom(&document, None, &test_identity(), 800, 600)
                .expect("Blitz document should build");

        let appended = Node::element("span", vec![Node::text("new")]);
        if let Node::Element(body_el) = &mut *body.borrow_mut() {
            body_el.children.push(appended.clone());
        }
        set_parent(&appended, &body);
        assert!(blitz_doc.sync_append_child(&body, &appended));
        blitz_doc.validate_mirror_integrity().unwrap();

        if let Node::Element(item_el) = &mut *item.borrow_mut() {
            item_el
                .attributes
                .insert("data-state".to_string(), "ready".to_string());
        }
        assert!(blitz_doc.sync_set_attribute(&item, "data-state", "ready"));
        blitz_doc.validate_mirror_integrity().unwrap();

        if let Node::Text(text_node) = &mut *text.borrow_mut() {
            text_node.content = "updated".to_string();
        }
        assert!(blitz_doc.sync_text_node(&text, "updated"));
        blitz_doc.validate_mirror_integrity().unwrap();

        if let Node::Element(body_el) = &mut *body.borrow_mut() {
            body_el
                .children
                .retain(|child| !std::rc::Rc::ptr_eq(child, &appended));
        }
        crate::dom::clear_parent(&appended);
        assert!(blitz_doc.sync_remove_child(&appended));
        blitz_doc.validate_mirror_integrity().unwrap();
    }

    #[cfg(debug_assertions)]
    #[test]
    fn sync_all_attributes_diffs_add_update_and_remove() {
        // Regression: `sync_all_attributes` used to clear every attribute and
        // re-add it. Clearing `href` on a `<link>` whose stylesheet had not
        // finished loading panicked inside blitz-dom (`unload_stylesheet`),
        // forcing a snapshot rebuild on every Polymer attribute re-stamp. The
        // diffing path must keep unchanged/updated attributes as in-place sets
        // and only clear attributes that the DOM actually dropped.
        let _guard = BLITZ_DOCUMENT_TEST_LOCK.lock().unwrap();
        let mut starting = BTreeMap::new();
        starting.insert("rel".to_string(), "stylesheet".to_string());
        starting.insert("href".to_string(), "/a.css".to_string());
        starting.insert("data-stale".to_string(), "1".to_string());
        let link = Node::element_with_attributes("link", starting, Vec::new());
        let document = document_with_body_children(vec![link.clone()]);
        // A real base URL lets the `<link href>` resolve; the fetch is async so
        // the stylesheet's `special_data` stays unloaded — exactly the window
        // where a blanket clear of `href` would panic in `unload_stylesheet`.
        let mut blitz_doc = BlitzDocument::try_from_dom(
            &document,
            Some("https://example.com/"),
            &test_identity(),
            800,
            600,
        )
        .expect("Blitz document should build");

        // Polymer-style re-stamp: keep `rel`, change `href`, drop `data-stale`,
        // add `data-fresh`.
        if let Node::Element(link_el) = &mut *link.borrow_mut() {
            link_el.attributes.remove("data-stale");
            link_el
                .attributes
                .insert("href".to_string(), "/b.css".to_string());
            link_el
                .attributes
                .insert("data-fresh".to_string(), "1".to_string());
        }
        assert!(blitz_doc.sync_all_attributes(&link));
        blitz_doc.validate_mirror_integrity().unwrap();

        let node_id = blitz_doc
            .blitz_node_id_for_dom(&link)
            .expect("link should be mapped");
        let element = blitz_doc.inner.get_node(node_id).unwrap();
        let blitz_attrs = blitz_attrs_to_map(element.data.downcast_element().unwrap());
        assert_eq!(
            blitz_attrs.get("rel").map(String::as_str),
            Some("stylesheet")
        );
        assert_eq!(blitz_attrs.get("href").map(String::as_str), Some("/b.css"));
        assert_eq!(blitz_attrs.get("data-fresh").map(String::as_str), Some("1"));
        assert!(!blitz_attrs.contains_key("data-stale"));
    }

    #[cfg(debug_assertions)]
    #[test]
    fn debug_mirror_integrity_accepts_synced_shadow_root() {
        let _guard = BLITZ_DOCUMENT_TEST_LOCK.lock().unwrap();
        let host = Node::element("x-host", Vec::new());
        let document = document_with_body_children(vec![host.clone()]);
        let mut blitz_doc =
            BlitzDocument::try_from_dom(&document, None, &test_identity(), 800, 600)
                .expect("Blitz document should build");

        let shadow_root =
            Node::document_fragment(vec![Node::element("span", vec![Node::text("shadow")])]);
        if let Node::Element(host_el) = &mut *host.borrow_mut() {
            host_el.shadow_root = Some(shadow_root.clone());
        }
        set_parent(&shadow_root, &host);
        crate::dom::reparent_subtree(&shadow_root);

        assert!(blitz_doc.sync_attach_shadow_root(&host, &shadow_root, "open"));
        blitz_doc.validate_mirror_integrity().unwrap();
        assert!(blitz_doc.blitz_node_id_for_dom(&shadow_root).is_some());
    }

    #[cfg(debug_assertions)]
    #[test]
    fn sync_shadow_root_replaces_previously_mirrored_light_children() {
        let _guard = BLITZ_DOCUMENT_TEST_LOCK.lock().unwrap();
        let light = Node::element("span", vec![Node::text("light")]);
        let host = Node::element("x-host", vec![light.clone()]);
        let document = document_with_body_children(vec![host.clone()]);
        let mut blitz_doc =
            BlitzDocument::try_from_dom(&document, None, &test_identity(), 800, 600)
                .expect("Blitz document should build");

        assert!(blitz_doc.blitz_node_id_for_dom(&light).is_some());
        let shadow_root =
            Node::document_fragment(vec![Node::element("strong", vec![Node::text("shadow")])]);
        if let Node::Element(host_el) = &mut *host.borrow_mut() {
            host_el.shadow_root = Some(shadow_root.clone());
        }
        set_parent(&shadow_root, &host);
        crate::dom::reparent_subtree(&shadow_root);

        assert!(blitz_doc.sync_attach_shadow_root(&host, &shadow_root, "open"));
        blitz_doc.validate_mirror_integrity().unwrap();
        assert!(blitz_doc.blitz_node_id_for_dom(&light).is_none());
        assert!(blitz_doc.blitz_node_id_for_dom(&shadow_root).is_some());
    }

    #[cfg(debug_assertions)]
    #[test]
    fn initial_snapshot_uses_synthetic_container_for_existing_shadow_root() {
        let _guard = BLITZ_DOCUMENT_TEST_LOCK.lock().unwrap();
        let host = Node::element("x-host", Vec::new());
        let shadow_root =
            Node::document_fragment(vec![Node::element("span", vec![Node::text("shadow")])]);
        if let Node::Element(host_el) = &mut *host.borrow_mut() {
            host_el.shadow_root = Some(shadow_root.clone());
        }
        set_parent(&shadow_root, &host);
        crate::dom::reparent_subtree(&shadow_root);
        let document = document_with_body_children(vec![host]);

        let blitz_doc = BlitzDocument::try_from_dom(&document, None, &test_identity(), 800, 600)
            .expect("Blitz document should build");
        blitz_doc.validate_mirror_integrity().unwrap();
    }

    #[cfg(debug_assertions)]
    #[test]
    fn debug_mirror_integrity_reports_corrupt_reverse_mapping() {
        let _guard = BLITZ_DOCUMENT_TEST_LOCK.lock().unwrap();
        let document = document_with_body_children(vec![Node::element("p", vec![Node::text("x")])]);
        let mut blitz_doc =
            BlitzDocument::try_from_dom(&document, None, &test_identity(), 800, 600)
                .expect("Blitz document should build");

        blitz_doc.blitz_to_legacy.remove(&0);

        let error = blitz_doc.validate_mirror_integrity().unwrap_err();
        assert_eq!(error.operation, "manual validation");
        assert_eq!(error.blitz_node, Some(0));
        assert!(
            error
                .message
                .contains("legacy_to_blitz entry has no reverse")
        );
    }

    #[test]
    fn mirror_mutation_trace_records_monotonic_operation_ids() {
        let _guard = BLITZ_DOCUMENT_TEST_LOCK.lock().unwrap();
        let item = Node::element_with_attributes("p", attr("id", "item"), vec![Node::text("old")]);
        let document = document_with_body_children(vec![item.clone()]);
        let mut blitz_doc =
            BlitzDocument::try_from_dom(&document, None, &test_identity(), 800, 600)
                .expect("Blitz document should build");

        if let Node::Element(item_el) = &mut *item.borrow_mut() {
            item_el
                .attributes
                .insert("data-state".to_string(), "ready".to_string());
        }
        assert!(blitz_doc.sync_set_attribute(&item, "data-state", "ready"));
        let first = blitz_doc
            .last_mirror_mutation_trace()
            .expect("trace should be recorded")
            .clone();
        assert_eq!(first.op_id, 1);
        assert_eq!(first.op_name, "sync_set_attribute");
        assert_eq!(first.result, MirrorMutationResult::Succeeded);
        assert_eq!(first.failure, None);
        assert_eq!(first.legacy_node, Some(legacy_node_key(&item)));
        assert!(first.blitz_node.is_some());

        if let Node::Element(item_el) = &mut *item.borrow_mut() {
            item_el.attributes.remove("data-state");
        }
        assert!(blitz_doc.sync_remove_attribute(&item, "data-state"));
        let second = blitz_doc
            .last_mirror_mutation_trace()
            .expect("trace should be recorded");
        assert_eq!(second.op_id, first.op_id + 1);
        assert_eq!(second.op_name, "sync_remove_attribute");
        assert_eq!(second.result, MirrorMutationResult::Succeeded);
        assert_eq!(second.failure, None);
    }

    #[test]
    fn mirror_mutation_trace_records_missing_mapping_failures() {
        let _guard = BLITZ_DOCUMENT_TEST_LOCK.lock().unwrap();
        let document = document_with_body_children(vec![Node::element("p", vec![Node::text("x")])]);
        let mut blitz_doc =
            BlitzDocument::try_from_dom(&document, None, &test_identity(), 800, 600)
                .expect("Blitz document should build");
        let orphan = Node::element("orphan", Vec::new());
        let child = Node::element("child", Vec::new());

        assert!(!blitz_doc.sync_append_child(&orphan, &child));
        let trace = blitz_doc
            .last_mirror_mutation_trace()
            .expect("failed sync should be traced");
        assert_eq!(trace.op_id, 1);
        assert_eq!(trace.op_name, "sync_append_child");
        assert_eq!(trace.result, MirrorMutationResult::Failed);
        assert_eq!(trace.failure, Some(MirrorMutationFailure::MissingMapping));
        assert_eq!(trace.parent, Some(legacy_node_key(&orphan)));
        assert_eq!(trace.child, Some(legacy_node_key(&child)));
        assert_eq!(trace.blitz_node, None);
    }

    #[test]
    fn paint_to_scene_reports_painted_current_frame_on_success() {
        let _guard = BLITZ_DOCUMENT_TEST_LOCK.lock().unwrap();
        let document = document_with_body_children(vec![Node::element("p", vec![Node::text("x")])]);
        let mut blitz_doc =
            BlitzDocument::try_from_dom(&document, None, &test_identity(), 800, 600)
                .expect("Blitz document should build");
        let mut scene = Scene::new();

        assert_eq!(
            blitz_doc.paint_to_scene(&mut scene, 800, 600),
            PaintResult::PaintedCurrentFrame
        );
    }

    #[test]
    fn paint_failure_state_distinguishes_recoverable_from_unhealthy() {
        let _guard = BLITZ_DOCUMENT_TEST_LOCK.lock().unwrap();
        let document = document_with_body_children(vec![Node::element("p", vec![Node::text("x")])]);
        let mut blitz_doc =
            BlitzDocument::try_from_dom(&document, None, &test_identity(), 800, 600)
                .expect("Blitz document should build");

        assert_eq!(
            blitz_doc.finish_paint_attempt("test", false),
            PaintResult::FailedRecoverable
        );

        for _ in 1..MAX_CONSECUTIVE_PANICS {
            let _ = blitz_doc.finish_paint_attempt("test", false);
        }
        assert_eq!(
            blitz_doc.paint_failure_result("test"),
            PaintResult::FailedUnhealthy
        );
        assert!(!blitz_doc.healthy);

        assert_eq!(
            blitz_doc.finish_paint_attempt("test", true),
            PaintResult::PaintedCurrentFrame
        );
        assert!(blitz_doc.healthy);
        assert_eq!(blitz_doc.consecutive_panics, 0);
    }

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

    #[cfg(debug_assertions)]
    #[test]
    fn debug_attr_names_round_trip_prefixed_names() {
        assert_eq!(
            attr_qual_name_to_string(&attr_qual_name_from_str("xlink:href")),
            "xlink:href"
        );
        assert_eq!(
            attr_qual_name_to_string(&attr_qual_name_from_str("xml:lang")),
            "xml:lang"
        );
        assert_eq!(
            attr_qual_name_to_string(&attr_qual_name_from_str("class")),
            "class"
        );
    }

    #[test]
    fn remove_blitz_subtree_mapping_removes_dom_subtree_recursively() {
        let removed_text = Node::text("old");
        let removed = Node::element("p", vec![removed_text.clone()]);
        let retained = Node::element("span", vec![Node::text("new")]);

        let mut legacy_to_blitz = BTreeMap::new();
        legacy_to_blitz.insert(legacy_node_key(&removed), 2);
        legacy_to_blitz.insert(legacy_node_key(&removed_text), 3);
        legacy_to_blitz.insert(legacy_node_key(&retained), 4);

        let mut blitz_to_legacy = BTreeMap::new();
        blitz_to_legacy.insert(2, removed.clone());
        blitz_to_legacy.insert(3, removed_text.clone());
        blitz_to_legacy.insert(4, retained.clone());

        remove_blitz_subtree_mapping(2, &mut legacy_to_blitz, &mut blitz_to_legacy);

        assert!(!legacy_to_blitz.contains_key(&legacy_node_key(&removed)));
        assert!(!legacy_to_blitz.contains_key(&legacy_node_key(&removed_text)));
        assert!(!blitz_to_legacy.contains_key(&2));
        assert!(!blitz_to_legacy.contains_key(&3));
        assert_eq!(legacy_to_blitz.get(&legacy_node_key(&retained)), Some(&4));
        assert!(blitz_to_legacy.contains_key(&4));
    }
}
