//! Arena-based DOM that is `Send + Sync` — required by stylo's traversal.
#![allow(dead_code)]

use std::cell::Cell;
use std::sync::atomic::AtomicBool;
use std::collections::BTreeMap;

use euclid::{Scale, Size2D};
use selectors::matching::ElementSelectorFlags;
use servo_style::data::{ElementDataMut, ElementDataRef, ElementDataWrapper};
use servo_style::device::Device;
use servo_style::media_queries::{MediaList, MediaType};
use servo_style::properties::ComputedValues;
use servo_style::queries::values::PrefersColorScheme;
use servo_style::servo_arc::Arc;
use servo_style::shared_lock::{Locked, SharedRwLock};
use servo_style::stylist::Stylist;
use servo_style::stylesheets::{
    AllowImportRules, DocumentStyleSheet, Origin, OriginSet, Stylesheet, UrlExtraData,
};
use servo_style::Atom;
use servo_style::{LocalName, Namespace};
use servo_style::properties::PropertyDeclarationBlock;
use style_traits::{CSSPixel, DevicePixel};
use stylo_dom::ElementState;

use crate::dom::{Node, NodePtr};

// ── Style data cell ───────────────────────────────────────────────────────────

pub struct StyloNodeData {
    inner: std::cell::UnsafeCell<Option<ElementDataWrapper>>,
}

unsafe impl Send for StyloNodeData {}
unsafe impl Sync for StyloNodeData {}

impl Default for StyloNodeData {
    fn default() -> Self { Self { inner: std::cell::UnsafeCell::new(None) } }
}

impl StyloNodeData {
    pub fn has_data(&self) -> bool {
        unsafe { (*self.inner.get()).is_some() }
    }
    pub unsafe fn ensure_init(&self) -> ElementDataMut<'_> {
        let ptr = self.inner.get();
        unsafe {
            (*ptr)
                .get_or_insert_with(ElementDataWrapper::default)
                .borrow_mut()
        }
    }
    pub unsafe fn clear(&self) { unsafe { *self.inner.get() = None; } }
    pub fn get(&self) -> Option<ElementDataRef<'_>> {
        unsafe { (*self.inner.get()).as_ref().map(|w| w.borrow()) }
    }
    pub fn get_mut(&self) -> Option<ElementDataMut<'_>> {
        unsafe { (*self.inner.get()).as_ref().map(|w| w.borrow_mut()) }
    }
}

// ── Arena node ────────────────────────────────────────────────────────────────

pub struct ArenaElementData {
    pub local_name: LocalName,
    pub namespace: Namespace,
    pub id_attr: Option<Atom>,
    pub attrs: BTreeMap<String, String>,
    pub style_attr: Option<Arc<Locked<PropertyDeclarationBlock>>>,
}

pub enum ArenaNodeData {
    Document,
    Element(ArenaElementData),
    Text(String),
}

impl ArenaNodeData {
    pub fn element(&self) -> Option<&ArenaElementData> {
        if let Self::Element(e) = self { Some(e) } else { None }
    }
}

pub struct ArenaNode {
    pub id: usize,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub data: ArenaNodeData,
    pub stylo_data: StyloNodeData,
    pub selector_flags: Cell<ElementSelectorFlags>,
    pub element_state: ElementState,
    pub has_snapshot: bool,
    pub snapshot_handled: AtomicBool,
    pub dirty_descendants: AtomicBool,
}

// SAFETY: sequential traversal only — stylo holds exclusive logical access.
unsafe impl Send for ArenaNode {}
unsafe impl Sync for ArenaNode {}

pub struct ArenaDoc {
    pub nodes: Vec<ArenaNode>,
    pub guard: SharedRwLock,
    pub stylist: Stylist,
}

impl ArenaDoc {
    #[inline]
    pub fn node(&self, id: usize) -> &ArenaNode { &self.nodes[id] }
}

// ── Convert Aurora DOM → arena ─────────────────────────────────────────────────

fn push_node(
    arena: &mut Vec<ArenaNode>,
    guard: &SharedRwLock,
    parent: Option<usize>,
    ptr: &NodePtr,
) -> usize {
    let id = arena.len();
    arena.push(ArenaNode {
        id,
        parent,
        children: vec![],
        data: ArenaNodeData::Document,
        stylo_data: StyloNodeData::default(),
        selector_flags: Cell::new(ElementSelectorFlags::empty()),
        element_state: ElementState::empty(),
        has_snapshot: false,
        snapshot_handled: AtomicBool::new(false),
        dirty_descendants: AtomicBool::new(true),
    });

    let borrowed = ptr.borrow();
    let (data, children) = match &*borrowed {
        Node::Document { children, .. } => {
            let ids: Vec<usize> =
                children.iter().map(|c| push_node(arena, guard, Some(id), c)).collect();
            (ArenaNodeData::Document, ids)
        }
        Node::Element(el) => {
            let local_name = LocalName::from(el.tag_name.as_str());
            let namespace = Namespace::from("http://www.w3.org/1999/xhtml");
            let id_attr = el.attributes.get("id").map(|s| Atom::from(s.as_str()));
            let style_attr = el.attributes.get("style")
                .and_then(|css| parse_inline_style(css, guard));
            let ids: Vec<usize> =
                el.children.iter().map(|c| push_node(arena, guard, Some(id), c)).collect();
            (ArenaNodeData::Element(ArenaElementData {
                local_name,
                namespace,
                id_attr,
                attrs: el.attributes.clone(),
                style_attr,
            }), ids)
        }
        Node::Text(t) => (ArenaNodeData::Text(t.clone()), vec![]),
    };
    arena[id].data = data;
    arena[id].children = children;
    id
}

fn parse_inline_style(
    css: &str,
    guard: &SharedRwLock,
) -> Option<Arc<Locked<PropertyDeclarationBlock>>> {
    use servo_style::context::QuirksMode;
    use servo_style::properties::parse_style_attribute;
    use servo_style::stylesheets::CssRuleType;

    let url = url::Url::parse("about:blank").ok()?;
    let url_data = UrlExtraData(Arc::new(url));
    let block = parse_style_attribute(css, &url_data, None, QuirksMode::NoQuirks, CssRuleType::Style);
    Some(Arc::new(guard.wrap(block)))
}

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn build_arena(root: &NodePtr, css_sheets: &[String]) -> ArenaDoc {
    use servo_style::context::QuirksMode;
    use servo_style::properties::style_structs::Font;

    let guard = SharedRwLock::new();
    let mut nodes = Vec::new();
    push_node(&mut nodes, &guard, None, root);

    let viewport: Size2D<f32, CSSPixel> = Size2D::new(1024.0, 768.0);
    let scale: Scale<f32, CSSPixel, DevicePixel> = Scale::new(1.0);
    let default_values = ComputedValues::initial_values_with_font_override(Font::initial_values());
    let device = Device::new(
        MediaType::screen(),
        QuirksMode::NoQuirks,
        viewport,
        scale,
        Box::new(EmptyFontMetrics),
        default_values,
        PrefersColorScheme::Light,
    );

    let mut stylist = Stylist::new(device, QuirksMode::NoQuirks);
    let url_data = UrlExtraData(Arc::new(
        url::Url::parse("about:blank").expect("\"about:blank\" is a valid URL"),
    ));

    for css in css_sheets {
        let media = Arc::new(guard.wrap(MediaList::empty()));
        let sheet = Stylesheet::from_str(
            css,
            url_data.clone(),
            Origin::Author,
            media,
            guard.clone(),
            None,
            None,
            QuirksMode::NoQuirks,
            AllowImportRules::Yes,
        );
        stylist.append_stylesheet(DocumentStyleSheet(Arc::new(sheet)), &guard.read());
    }
    stylist.force_stylesheet_origins_dirty(OriginSet::all());

    ArenaDoc { nodes, guard, stylist }
}

// ── Stub font metrics ─────────────────────────────────────────────────────────

#[derive(Debug)]
struct EmptyFontMetrics;

impl servo_style::device::servo::FontMetricsProvider for EmptyFontMetrics {
    fn query_font_metrics(
        &self,
        _vertical: bool,
        _font: &servo_style::properties::style_structs::Font,
        _base_size: servo_style::values::computed::CSSPixelLength,
        _flags: servo_style::values::computed::font::QueryFontMetricsFlags,
    ) -> servo_style::font_metrics::FontMetrics {
        Default::default()
    }

    fn base_size_for_generic(
        &self,
        _generic: servo_style::values::computed::font::GenericFontFamily,
    ) -> servo_style::values::computed::Length {
        servo_style::values::computed::Length::new(16.0)
    }
}
