//! Native custom-element registry.
//!
//! Phase 1 of the native custom-element-reaction plan
//! (`docs/NATIVE_CUSTOM_ELEMENTS_PLAN.md`). This mirrors each
//! `customElements.define(name, ctor)` call into a native registry so the
//! definition (constructor + lifecycle callbacks + observed attributes) lives in
//! Rust, the way Ladybird's `CustomElementRegistry` does. The JS shim still
//! drives upgrade/connection for now; later phases move the reaction queue and
//! the insertion-time enqueue native too.
//!
//! The lifecycle callbacks stay as JS functions (`v8::Global<v8::Function>`),
//! exactly as Ladybird keeps them `WebIDL::CallbackType` — only the registry and
//! (later) the reaction scheduling are native.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::rc::Rc;

use super::registry::NodeRegistry;

/// Custom element state, per the HTML spec's element lifecycle.
///
/// Only `Undefined`/`Custom` are exercised in Phase 1; the rest land with the
/// native upgrade path in Phase 2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum CeState {
    Undefined,
    Uncustomized,
    Custom,
    Failed,
}

/// A registered custom-element definition.
///
/// Holds V8 global handles to the constructor and lifecycle callbacks so they
/// survive across turns. These are dropped when the owning [`CeRegistry`] (and
/// thus the `NodeRegistry`) is dropped, which happens before the isolate.
pub(crate) struct CeDefinition {
    pub(crate) name: String,
    #[allow(dead_code)]
    pub(crate) constructor: v8::Global<v8::Function>,
    #[allow(dead_code)]
    pub(crate) connected: Option<v8::Global<v8::Function>>,
    #[allow(dead_code)]
    pub(crate) disconnected: Option<v8::Global<v8::Function>>,
    #[allow(dead_code)]
    pub(crate) attribute_changed: Option<v8::Global<v8::Function>>,
    #[allow(dead_code)]
    pub(crate) observed_attributes: HashSet<String>,
}

/// A pending custom-element reaction, mirroring Ladybird's
/// `CustomElementReaction` variants. Phase 2 only enqueues lifecycle callbacks;
/// the upgrade reaction lands when the native upgrade path does.
enum Reaction {
    Callback {
        callback: v8::Global<v8::Function>,
        args: Vec<v8::Global<v8::Value>>,
    },
}

/// Native mirror of the JS custom-element registry plus the reaction queue
/// machinery (Phase 2). Definitions map name → definition; reactions are queued
/// per element id and drained at the microtask checkpoint, the way Ladybird's
/// element queue + backup element queue work.
#[derive(Default)]
pub(crate) struct CeRegistry {
    definitions: RefCell<BTreeMap<String, Rc<CeDefinition>>>,
    /// Per-element FIFO of pending reactions (keyed by node id).
    reaction_queues: RefCell<BTreeMap<u32, VecDeque<Reaction>>>,
    /// The backup element queue: element ids with pending reactions, in order.
    /// (We don't yet have synchronous CEReactions boundaries, so a single
    /// microtask-drained queue suffices; the element-queue *stack* is a later
    /// refinement.)
    backup_queue: RefCell<Vec<u32>>,
}

impl CeRegistry {
    /// Record (or replace) a definition. A redefinition of the same name keeps
    /// the latest constructor, matching the JS shim's `ensureDefinitionMetadata`
    /// which overwrites `existing.ctor`.
    pub(crate) fn define(&self, definition: CeDefinition) {
        self.definitions
            .borrow_mut()
            .insert(definition.name.clone(), Rc::new(definition));
    }

    /// Look up a definition by tag name.
    #[allow(dead_code)]
    pub(crate) fn lookup(&self, name: &str) -> Option<Rc<CeDefinition>> {
        self.definitions.borrow().get(name).cloned()
    }

    /// Whether a tag name has a native definition.
    pub(crate) fn is_defined(&self, name: &str) -> bool {
        self.definitions.borrow().contains_key(name)
    }

    /// Number of registered definitions (used by tests).
    #[allow(dead_code)]
    pub(crate) fn len(&self) -> usize {
        self.definitions.borrow().len()
    }

    /// Enqueue a callback reaction for `node_id` (Ladybird's "enqueue a custom
    /// element callback reaction" + "enqueue an element on the appropriate
    /// element queue", collapsed to the backup queue for now).
    pub(crate) fn enqueue_callback(
        &self,
        node_id: u32,
        callback: v8::Global<v8::Function>,
        args: Vec<v8::Global<v8::Value>>,
    ) {
        self.reaction_queues
            .borrow_mut()
            .entry(node_id)
            .or_default()
            .push_back(Reaction::Callback { callback, args });
        self.backup_queue.borrow_mut().push(node_id);
    }

    /// Whether any reactions are queued.
    pub(crate) fn has_pending_reactions(&self) -> bool {
        !self.backup_queue.borrow().is_empty()
    }

    /// Take the current backup queue, leaving it empty.
    fn take_backup_queue(&self) -> Vec<u32> {
        std::mem::take(&mut *self.backup_queue.borrow_mut())
    }

    /// Take (remove) the reaction queue for one element.
    fn take_reactions(&self, node_id: u32) -> Option<VecDeque<Reaction>> {
        self.reaction_queues.borrow_mut().remove(&node_id)
    }
}

/// Invoke queued custom-element reactions (Ladybird's
/// `invoke_custom_element_reactions`). Drains the backup queue element by
/// element, invoking each element's reactions with the element's JS wrapper as
/// the `this` value. Re-checks the queue up to 100 times so reactions enqueued
/// *by* a reaction (e.g. a `connectedCallback` that appends a child) also drain.
pub(super) fn drain_reactions(
    scope: &mut v8::PinScope<'_, '_>,
    registry: &Rc<NodeRegistry>,
) -> bool {
    let mut drained_any = false;
    for _ in 0..100 {
        let queue = registry.ce_registry.take_backup_queue();
        if queue.is_empty() {
            break;
        }
        drained_any = true;
        for node_id in queue {
            let reactions = match registry.ce_registry.take_reactions(node_id) {
                Some(reactions) => reactions,
                None => continue,
            };
            // `this` is the element's existing JS wrapper. It was created when
            // JS inserted the element, so it should already exist.
            let recv = match registry.lookup_js_wrapper(scope, node_id) {
                Some(wrapper) => wrapper,
                None => continue,
            };
            for reaction in reactions {
                match reaction {
                    Reaction::Callback { callback, args } => {
                        let cb = v8::Local::new(scope, callback);
                        let arg_locals: Vec<v8::Local<v8::Value>> =
                            args.iter().map(|a| v8::Local::new(scope, a)).collect();
                        let _ = cb.call(scope, recv.into(), &arg_locals);
                    }
                }
            }
        }
    }
    drained_any
}
