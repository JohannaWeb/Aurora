//! Stylo CSS engine integration — Phase 1.
//!
//! Converts Aurora's `Rc<RefCell<Node>>` DOM into an arena suitable for
//! stylo's style traversal, runs the cascade, and writes `ComputedValues`
//! into each arena node's `stylo_data` cell.

pub mod arena;
pub mod impls;

pub use arena::ArenaDoc;
pub use arena::build_arena;
pub use impls::AuroraNode;

use servo_style::{
    Atom,
    context::{RegisteredSpeculativePainter, RegisteredSpeculativePainters, SharedStyleContext},
    dom::{TDocument, TElement, TNode},
    global_style_data::GLOBAL_STYLE_DATA,
    shared_lock::StylesheetGuards,
    thread_state::ThreadState,
    traversal::{DomTraversal, PerLevelTraversalData},
    traversal_flags::TraversalFlags,
    context::StyleContext,
};
use servo_style::traversal::recalc_style_at;
use servo_style::selector_parser::SnapshotMap;
use selectors::Element as _; // bring first_element_child into scope

// ── Stub painters ─────────────────────────────────────────────────────────────

struct NoPainters;
impl RegisteredSpeculativePainters for NoPainters {
    fn get(&self, _: &Atom) -> Option<&dyn RegisteredSpeculativePainter> { None }
}

// ── Style traversal ───────────────────────────────────────────────────────────

pub struct RecalcStyle<'a> {
    context: SharedStyleContext<'a>,
}

impl<'a> RecalcStyle<'a> {
    fn new(context: SharedStyleContext<'a>) -> Self { Self { context } }
}

impl<E: TElement> DomTraversal<E> for RecalcStyle<'_> {
    fn process_preorder<F: FnMut(E::ConcreteNode)>(
        &self,
        data: &PerLevelTraversalData,
        context: &mut StyleContext<E>,
        node: E::ConcreteNode,
        note_child: F,
    ) {
        if let Some(el) = node.as_element() {
            let mut style_data = unsafe { el.ensure_data() };
            recalc_style_at(self, data, context, el, &mut style_data, note_child);
            unsafe { el.unset_dirty_descendants() }
        }
    }
    fn needs_postorder_traversal() -> bool { false }
    fn process_postorder(&self, _: &mut StyleContext<E>, _: E::ConcreteNode) {}
    fn shared_context(&self) -> &SharedStyleContext<'_> { &self.context }
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Run the stylo cascade over `doc`, writing computed styles into each arena
/// node's `stylo_data` cell.
pub fn resolve_styles(doc: &mut ArenaDoc) {
    servo_style::thread_state::enter(ThreadState::LAYOUT);

    {
        let guards = StylesheetGuards {
            author: &doc.guard.read(),
            ua_or_user: &doc.guard.read(),
        };
        doc.stylist.flush(&guards);
    }

    let context = SharedStyleContext {
        traversal_flags: TraversalFlags::empty(),
        stylist: &doc.stylist,
        options: GLOBAL_STYLE_DATA.options.clone(),
        guards: StylesheetGuards {
            author: &doc.guard.read(),
            ua_or_user: &doc.guard.read(),
        },
        visited_styles_enabled: false,
        animations: Default::default(),
        current_time_for_animations: 0.0,
        snapshot_map: &SnapshotMap::new(),
        registered_speculative_painters: &NoPainters,
    };

    let doc_node = AuroraNode { doc, id: 0 };
    let root = TDocument::as_node(&doc_node)
        .first_element_child()
        .and_then(|n| n.as_element());

    if let Some(root) = root {
        let token = RecalcStyle::pre_traverse(root, &context);
        if token.should_traverse() {
            let traverser = RecalcStyle::new(context);
            servo_style::driver::traverse_dom(&traverser, token, None);
        }
    }

    doc.stylist.rule_tree().maybe_gc();
    servo_style::thread_state::exit(ThreadState::LAYOUT);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dom::Node;

    fn make_dom(html: &str) -> crate::dom::NodePtr {
        crate::html::parse_html(html)
    }

    #[test]
    fn stylo_resolves_display_block_for_div() {
        let dom = make_dom("<html><body><div id='x'>hello</div></body></html>");
        let css = "div { display: block; color: red; }".to_string();
        let mut doc = build_arena(&dom, &[css]);
        resolve_styles(&mut doc);

        // Find the div node in the arena and check it has computed styles.
        let div_id = doc.nodes.iter().find(|n| {
            n.data.element().map_or(false, |e| e.local_name.as_ref() == "div")
        }).map(|n| n.id);

        assert!(div_id.is_some(), "div node not found in arena");
        let div = &doc.nodes[div_id.unwrap()];
        assert!(div.stylo_data.has_data(), "div has no computed style data");

        let data = div.stylo_data.get().expect("borrow_data returned None");
        assert!(data.has_styles(), "div styles not resolved");
    }
}
