//! Stylo DOM trait implementations for `AuroraNode<'a>`.

use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::atomic::Ordering;

use selectors::{
    OpaqueElement,
    attr::{AttrSelectorOperation, CaseSensitivity, NamespaceConstraint},
    matching::{ElementSelectorFlags, MatchingContext, VisitedHandlingMode},
    sink::Push,
};
use servo_style::{
    Atom, CaseSensitivityExt,
    applicable_declarations::ApplicableDeclarationBlock,
    context::SharedStyleContext,
    data::{ElementDataMut, ElementDataRef},
    dom::{
        AttributeProvider, LayoutIterator, NodeInfo, OpaqueNode,
        TDocument, TElement, TNode, TShadowRoot,
    },
    properties::{ComputedValues, PropertyDeclarationBlock},
    selector_parser::{NonTSPseudoClass, PseudoElement, SelectorImpl},
    servo_arc::{Arc, ArcBorrow},
    shared_lock::{Locked, SharedRwLock},
    values::{AtomIdent, GenericAtomIdent},
    LocalName, Namespace,
};
use stylo_dom::ElementState;

use super::arena::{ArenaDoc, ArenaElementData, ArenaNode, ArenaNodeData};

// ── Bridge type ───────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
pub struct AuroraNode<'a> {
    pub doc: &'a ArenaDoc,
    pub id: usize,
}

impl<'a> AuroraNode<'a> {
    #[inline] pub fn node(&self) -> &'a ArenaNode { self.doc.node(self.id) }
    #[inline] pub fn with(&self, id: usize) -> Self { AuroraNode { doc: self.doc, id } }
    #[inline] fn element(&self) -> Option<&'a ArenaElementData> { self.node().data.element() }
}

impl fmt::Debug for AuroraNode<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AuroraNode({})", self.id)
    }
}
impl PartialEq for AuroraNode<'_> {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.doc, other.doc) && self.id == other.id
    }
}
impl Eq for AuroraNode<'_> {}
impl Hash for AuroraNode<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) { self.id.hash(state); }
}
unsafe impl Send for AuroraNode<'_> {}
unsafe impl Sync for AuroraNode<'_> {}

// ── NodeInfo ──────────────────────────────────────────────────────────────────

impl NodeInfo for AuroraNode<'_> {
    fn is_element(&self) -> bool { matches!(self.node().data, ArenaNodeData::Element(_)) }
    fn is_text_node(&self) -> bool { matches!(self.node().data, ArenaNodeData::Text(_)) }
}

// ── TDocument ─────────────────────────────────────────────────────────────────

impl<'a> TDocument for AuroraNode<'a> {
    type ConcreteNode = AuroraNode<'a>;
    fn as_node(&self) -> Self::ConcreteNode { *self }
    fn is_html_document(&self) -> bool { true }
    fn quirks_mode(&self) -> servo_style::context::QuirksMode {
        servo_style::context::QuirksMode::NoQuirks
    }
    fn shared_lock(&self) -> &SharedRwLock { &self.doc.guard }
}

// ── TShadowRoot ───────────────────────────────────────────────────────────────

impl<'a> TShadowRoot for AuroraNode<'a> {
    type ConcreteNode = AuroraNode<'a>;
    fn as_node(&self) -> Self::ConcreteNode { *self }
    fn host(&self) -> AuroraNode<'a> { unimplemented!("shadow DOM not supported") }
    fn style_data<'b>(&self) -> Option<&'b servo_style::stylist::CascadeData>
    where Self: 'b { unimplemented!("shadow DOM not supported") }
}

// ── TNode ─────────────────────────────────────────────────────────────────────

impl<'a> TNode for AuroraNode<'a> {
    type ConcreteElement = AuroraNode<'a>;
    type ConcreteDocument = AuroraNode<'a>;
    type ConcreteShadowRoot = AuroraNode<'a>;

    fn parent_node(&self) -> Option<Self> { self.node().parent.map(|p| self.with(p)) }
    fn first_child(&self) -> Option<Self> { self.node().children.first().map(|&c| self.with(c)) }
    fn last_child(&self) -> Option<Self> { self.node().children.last().map(|&c| self.with(c)) }

    fn prev_sibling(&self) -> Option<Self> {
        let p = self.node().parent?;
        let sib = &self.doc.node(p).children;
        let i = sib.iter().position(|&c| c == self.id)?;
        i.checked_sub(1).map(|j| self.with(sib[j]))
    }
    fn next_sibling(&self) -> Option<Self> {
        let p = self.node().parent?;
        let sib = &self.doc.node(p).children;
        let i = sib.iter().position(|&c| c == self.id)?;
        sib.get(i + 1).map(|&c| self.with(c))
    }

    fn owner_doc(&self) -> Self::ConcreteDocument { self.with(0) }
    fn is_in_document(&self) -> bool { true }
    fn traversal_parent(&self) -> Option<Self::ConcreteElement> {
        self.parent_node().and_then(|n| n.as_element())
    }
    fn opaque(&self) -> OpaqueNode { OpaqueNode(self.id) }
    fn debug_id(self) -> usize { self.id }
    fn as_element(&self) -> Option<Self::ConcreteElement> { self.is_element().then_some(*self) }
    fn as_document(&self) -> Option<Self::ConcreteDocument> {
        matches!(self.node().data, ArenaNodeData::Document).then_some(*self)
    }
    fn as_shadow_root(&self) -> Option<Self::ConcreteShadowRoot> { None }
}

// ── AttributeProvider ─────────────────────────────────────────────────────────

impl AttributeProvider for AuroraNode<'_> {
    fn get_attr(&self, attr: &LocalName, _ns: &Namespace) -> Option<String> {
        self.element()?.attrs.get(attr.as_ref()).cloned()
    }
}

// ── selectors::Element ────────────────────────────────────────────────────────

impl<'a> selectors::Element for AuroraNode<'a> {
    type Impl = SelectorImpl;

    fn opaque(&self) -> OpaqueElement {
        OpaqueElement::from_non_null_ptr(
            std::ptr::NonNull::new((self.id + 1) as *mut ()).unwrap()
        )
    }

    fn parent_element(&self) -> Option<Self> { TElement::traversal_parent(self) }
    fn parent_node_is_shadow_root(&self) -> bool { false }
    fn containing_shadow_host(&self) -> Option<Self> { None }
    fn is_pseudo_element(&self) -> bool { false }

    fn prev_sibling_element(&self) -> Option<Self> {
        let mut cur = self.prev_sibling();
        while let Some(n) = cur {
            if n.is_element() { return Some(n); }
            cur = n.prev_sibling();
        }
        None
    }
    fn next_sibling_element(&self) -> Option<Self> {
        let mut cur = self.next_sibling();
        while let Some(n) = cur {
            if n.is_element() { return Some(n); }
            cur = n.next_sibling();
        }
        None
    }
    fn first_element_child(&self) -> Option<Self> {
        self.node().children.iter().find_map(|&c| {
            let n = self.with(c); n.is_element().then_some(n)
        })
    }

    fn is_html_element_in_html_document(&self) -> bool { true }

    fn has_local_name(
        &self, name: &<SelectorImpl as selectors::SelectorImpl>::BorrowedLocalName,
    ) -> bool {
        self.element().map_or(false, |e| &*e.local_name == name)
    }
    fn has_namespace(
        &self, ns: &<SelectorImpl as selectors::SelectorImpl>::BorrowedNamespaceUrl,
    ) -> bool {
        self.element().map_or(false, |e| &*e.namespace == ns)
    }
    fn is_same_type(&self, other: &Self) -> bool {
        match (self.element(), other.element()) {
            (Some(a), Some(b)) => a.local_name == b.local_name && a.namespace == b.namespace,
            _ => false,
        }
    }

    fn attr_matches(
        &self,
        _ns: &NamespaceConstraint<&<SelectorImpl as selectors::SelectorImpl>::NamespaceUrl>,
        local_name: &<SelectorImpl as selectors::SelectorImpl>::LocalName,
        operation: &AttrSelectorOperation<&<SelectorImpl as selectors::SelectorImpl>::AttrValue>,
    ) -> bool {
        let Some(elem) = self.element() else { return false };
        match elem.attrs.get(local_name.as_ref()) {
            None => false,
            Some(val) => operation.eval_str(val),
        }
    }

    fn match_non_ts_pseudo_class(
        &self, pc: &NonTSPseudoClass, _: &mut MatchingContext<SelectorImpl>,
    ) -> bool {
        let s = self.node().element_state;
        match *pc {
            NonTSPseudoClass::Active   => s.contains(ElementState::ACTIVE),
            NonTSPseudoClass::Focus    => s.contains(ElementState::FOCUS),
            NonTSPseudoClass::Hover    => s.contains(ElementState::HOVER),
            NonTSPseudoClass::Enabled  => s.contains(ElementState::ENABLED),
            NonTSPseudoClass::Disabled => s.contains(ElementState::DISABLED),
            NonTSPseudoClass::Checked  => s.contains(ElementState::CHECKED),
            NonTSPseudoClass::Link | NonTSPseudoClass::AnyLink => {
                self.element().map_or(false, |e| {
                    (e.local_name.as_ref() == "a" || e.local_name.as_ref() == "area")
                        && e.attrs.contains_key("href")
                })
            }
            _ => false,
        }
    }

    fn match_pseudo_element(
        &self, _: &PseudoElement, _: &mut MatchingContext<SelectorImpl>,
    ) -> bool { false }

    fn apply_selector_flags(&self, flags: ElementSelectorFlags) {
        let n = self.node();
        n.selector_flags.set(n.selector_flags.get() | flags);
    }

    fn is_link(&self) -> bool {
        self.element().map_or(false, |e| {
            e.local_name.as_ref() == "a" && e.attrs.contains_key("href")
        })
    }
    fn is_html_slot_element(&self) -> bool { false }

    fn has_id(&self, id: &AtomIdent, case_sensitivity: CaseSensitivity) -> bool {
        self.element()
            .and_then(|e| e.id_attr.as_ref())
            .map_or(false, |a| case_sensitivity.eq_atom(a, &id.0))
    }
    fn has_class(&self, name: &AtomIdent, case_sensitivity: CaseSensitivity) -> bool {
        let Some(elem) = self.element() else { return false };
        let Some(cls) = elem.attrs.get("class") else { return false };
        cls.split_ascii_whitespace()
            .any(|c| case_sensitivity.eq_atom(&Atom::from(c), &name.0))
    }
    fn imported_part(&self, _: &AtomIdent) -> Option<AtomIdent> { None }
    fn is_part(&self, _: &AtomIdent) -> bool { false }
    fn is_empty(&self) -> bool { self.node().children.is_empty() }
    fn is_root(&self) -> bool {
        self.node().parent
            .map_or(false, |p| matches!(self.doc.node(p).data, ArenaNodeData::Document))
    }
    fn has_custom_state(&self, _: &AtomIdent) -> bool { false }

    fn add_element_unique_hashes(&self, filter: &mut selectors::bloom::BloomFilter) -> bool {
        servo_style::bloom::each_relevant_element_hash(*self, |h| {
            filter.insert_hash(h & selectors::bloom::BLOOM_HASH_MASK)
        });
        true
    }
}

// ── TElement ──────────────────────────────────────────────────────────────────

pub struct ChildIter<'a> {
    doc: &'a ArenaDoc,
    children: &'a [usize],
    index: usize,
}
impl<'a> Iterator for ChildIter<'a> {
    type Item = AuroraNode<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        let &id = self.children.get(self.index)?;
        self.index += 1;
        Some(AuroraNode { doc: self.doc, id })
    }
}

impl<'a> TElement for AuroraNode<'a> {
    type ConcreteNode = AuroraNode<'a>;
    type TraversalChildrenIterator = ChildIter<'a>;

    fn as_node(&self) -> AuroraNode<'a> { *self }

    fn implicit_scope_for_sheet_in_shadow_root(
        _: OpaqueElement, _: usize,
    ) -> Option<servo_style::stylesheets::scope_rule::ImplicitScopeRoot> { None }

    fn traversal_children(&self) -> LayoutIterator<ChildIter<'a>> {
        LayoutIterator(ChildIter {
            doc: self.doc,
            children: &self.doc.node(self.id).children,
            index: 0,
        })
    }

    fn is_html_element(&self) -> bool { self.is_element() }
    fn is_mathml_element(&self) -> bool { false }
    fn is_svg_element(&self) -> bool { false }

    fn style_attribute(&self) -> Option<ArcBorrow<'_, Locked<PropertyDeclarationBlock>>> {
        self.element()?.style_attr.as_ref().map(|a| a.borrow_arc())
    }

    fn animation_rule(
        &self, _: &SharedStyleContext,
    ) -> Option<Arc<Locked<PropertyDeclarationBlock>>> { None }
    fn transition_rule(
        &self, _: &SharedStyleContext,
    ) -> Option<Arc<Locked<PropertyDeclarationBlock>>> { None }

    fn state(&self) -> ElementState { self.node().element_state }
    fn has_part_attr(&self) -> bool { false }
    fn exports_any_part(&self) -> bool { false }

    fn id(&self) -> Option<&Atom> { self.element()?.id_attr.as_ref() }

    fn each_class<F: FnMut(&AtomIdent)>(&self, mut cb: F) {
        let Some(elem) = self.element() else { return };
        let Some(cls) = elem.attrs.get("class") else { return };
        for c in cls.split_ascii_whitespace() {
            let a = Atom::from(c);
            cb(AtomIdent::cast(&a));
        }
    }
    fn each_custom_state<F: FnMut(&AtomIdent)>(&self, _: F) {}
    fn each_attr_name<F: FnMut(&LocalName)>(&self, mut cb: F) {
        let Some(elem) = self.element() else { return };
        for k in elem.attrs.keys() {
            let ln = LocalName::from(k.as_str());
            cb(&ln);
        }
    }

    fn has_dirty_descendants(&self) -> bool {
        self.node().dirty_descendants.load(Ordering::Relaxed)
    }
    fn has_snapshot(&self) -> bool { self.node().has_snapshot }
    fn handled_snapshot(&self) -> bool {
        self.node().snapshot_handled.load(Ordering::SeqCst)
    }
    unsafe fn set_handled_snapshot(&self) {
        self.node().snapshot_handled.store(true, Ordering::SeqCst);
    }
    unsafe fn set_dirty_descendants(&self) {
        self.node().dirty_descendants.store(true, Ordering::Relaxed);
    }
    unsafe fn unset_dirty_descendants(&self) {
        self.node().dirty_descendants.store(false, Ordering::Relaxed);
    }

    fn store_children_to_process(&self, _: isize) {}
    fn did_process_child(&self) -> isize { 0 }

    unsafe fn ensure_data(&self) -> ElementDataMut<'_> {
        unsafe { self.node().stylo_data.ensure_init() }
    }
    unsafe fn clear_data(&self) { unsafe { self.node().stylo_data.clear() } }
    fn has_data(&self) -> bool { self.node().stylo_data.has_data() }
    fn borrow_data(&self) -> Option<ElementDataRef<'_>> { self.node().stylo_data.get() }
    fn mutate_data(&self) -> Option<ElementDataMut<'_>> { self.node().stylo_data.get_mut() }

    fn skip_item_display_fixup(&self) -> bool { false }
    fn may_have_animations(&self) -> bool { false }
    fn has_animations(&self, _: &SharedStyleContext) -> bool { false }
    fn has_css_animations(
        &self, _: &SharedStyleContext, _: Option<PseudoElement>,
    ) -> bool { false }
    fn has_css_transitions(
        &self, _: &SharedStyleContext, _: Option<PseudoElement>,
    ) -> bool { false }

    fn shadow_root(
        &self,
    ) -> Option<<Self::ConcreteNode as TNode>::ConcreteShadowRoot> { None }
    fn containing_shadow(
        &self,
    ) -> Option<<Self::ConcreteNode as TNode>::ConcreteShadowRoot> { None }

    fn lang_attr(&self) -> Option<servo_style::selector_parser::AttrValue> { None }
    fn match_element_lang(
        &self,
        _: Option<Option<servo_style::selector_parser::AttrValue>>,
        _: &servo_style::selector_parser::Lang,
    ) -> bool { false }

    fn is_html_document_body_element(&self) -> bool {
        self.element().map_or(false, |e| e.local_name.as_ref() == "body")
    }

    fn synthesize_presentational_hints_for_legacy_attributes<V: Push<ApplicableDeclarationBlock>>(
        &self, _: VisitedHandlingMode, _: &mut V,
    ) {}

    fn local_name(
        &self,
    ) -> &<SelectorImpl as selectors::SelectorImpl>::BorrowedLocalName {
        &self.element().expect("local_name on non-element").local_name
    }
    fn namespace(
        &self,
    ) -> &<SelectorImpl as selectors::SelectorImpl>::BorrowedNamespaceUrl {
        &self.element().expect("namespace on non-element").namespace
    }

    fn query_container_size(
        &self, _: &servo_style::values::specified::Display,
    ) -> euclid::default::Size2D<Option<app_units::Au>> {
        Default::default()
    }

    fn has_selector_flags(&self, flags: ElementSelectorFlags) -> bool {
        self.node().selector_flags.get().contains(flags)
    }
    fn relative_selector_search_direction(&self) -> ElementSelectorFlags {
        self.node().selector_flags.get()
            & (ElementSelectorFlags::RELATIVE_SELECTOR_SEARCH_DIRECTION_ANCESTOR_SIBLING
                | ElementSelectorFlags::RELATIVE_SELECTOR_SEARCH_DIRECTION_ANCESTOR
                | ElementSelectorFlags::RELATIVE_SELECTOR_SEARCH_DIRECTION_SIBLING)
    }

    fn compute_layout_damage(
        _: &ComputedValues, _: &ComputedValues,
    ) -> servo_style::selector_parser::RestyleDamage {
        servo_style::selector_parser::RestyleDamage::empty()
    }
}
