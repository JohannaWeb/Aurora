//! Native custom-element registry and reaction queue.
//!
//! Phases 1–2 of the native custom-element-reaction plan
//! (`docs/NATIVE_CUSTOM_ELEMENTS_PLAN.md`). Phase 1 mirrors each
//! `customElements.define(name, ctor)` call into a native registry so the
//! definition (constructor + lifecycle callbacks + observed attributes) lives in
//! Rust, the way Ladybird's `CustomElementRegistry` does. Phase 2 adds the
//! reaction queue: insertion enqueues `connectedCallback` reactions which drain
//! at the microtask checkpoint, mirroring Ladybird's element queue + backup
//! element queue.
//!
//! The lifecycle callbacks stay as JS functions (`v8::Global<v8::Function>`),
//! exactly as Ladybird keeps them `WebIDL::CallbackType` — only the registry,
//! queue, and scheduling are native.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::rc::Rc;

use super::registry::NodeRegistry;

/// Custom element state, per the HTML spec's element lifecycle.
///
/// Only `Undefined`/`Custom` are exercised so far; the rest land with the
/// native upgrade path (Phase 2b).
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
    pub(crate) connected: Option<v8::Global<v8::Function>>,
    #[allow(dead_code)]
    pub(crate) disconnected: Option<v8::Global<v8::Function>>,
    #[allow(dead_code)]
    pub(crate) attribute_changed: Option<v8::Global<v8::Function>>,
    #[allow(dead_code)]
    pub(crate) observed_attributes: HashSet<String>,
}

/// A pending custom-element reaction, mirroring Ladybird's
/// `CustomElementReaction` variants. The upgrade reaction lands when the native
/// upgrade path does (Phase 2b).
enum Reaction {
    /// `connectedCallback` / `disconnectedCallback` — no arguments.
    Callback {
        callback: v8::Global<v8::Function>,
        args: Vec<v8::Global<v8::Value>>,
    },
    /// `attributeChangedCallback(name, oldValue, newValue, namespace)`. The
    /// string values are held as Rust strings and converted to V8 at drain time,
    /// since the mutation path that enqueues has no V8 scope.
    AttributeChanged {
        callback: v8::Global<v8::Function>,
        name: String,
        old_value: Option<String>,
        new_value: Option<String>,
    },
}

/// Native mirror of the JS custom-element registry plus the reaction queue
/// machinery. Definitions map name → definition; reactions are queued per
/// element id and drained at the microtask checkpoint, the way Ladybird's
/// element queue + backup element queue work.
#[derive(Default)]
pub(crate) struct CeRegistry {
    definitions: RefCell<BTreeMap<String, Rc<CeDefinition>>>,
    /// Per-element FIFO of pending reactions (keyed by node id).
    reaction_queues: RefCell<BTreeMap<u32, VecDeque<Reaction>>>,
    /// The backup element queue: element ids with pending reactions, in order.
    backup_queue: RefCell<Vec<u32>>,
    /// The custom element reactions stack: a stack of element queues (for synchronous CEReactions boundaries).
    element_queue_stack: RefCell<Vec<Vec<u32>>>,
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

    /// Helper to push a node id to the appropriate queue (current active boundary, or backup).
    fn enqueue_element_id(&self, node_id: u32) {
        let mut stack = self.element_queue_stack.borrow_mut();
        if let Some(current_queue) = stack.last_mut() {
            current_queue.push(node_id);
        } else {
            self.backup_queue.borrow_mut().push(node_id);
        }
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
        self.enqueue_element_id(node_id);
    }

    /// Enqueue an `attributeChangedCallback` reaction for `node_id`. The
    /// observed-attribute filtering is the caller's responsibility (it has the
    /// definition in hand).
    pub(crate) fn enqueue_attribute_changed(
        &self,
        node_id: u32,
        callback: v8::Global<v8::Function>,
        name: String,
        old_value: Option<String>,
        new_value: Option<String>,
    ) {
        self.reaction_queues
            .borrow_mut()
            .entry(node_id)
            .or_default()
            .push_back(Reaction::AttributeChanged {
                callback,
                name,
                old_value,
                new_value,
            });
        self.enqueue_element_id(node_id);
    }

    /// Push a new element queue onto the reactions stack for a `[CEReactions]` boundary.
    pub(crate) fn push_reactions_stack(&self) {
        self.element_queue_stack.borrow_mut().push(Vec::new());
    }

    /// Pop the element queue from the reactions stack and invoke custom element reactions in it.
    pub(crate) fn pop_and_restore_reactions_stack(
        &self,
        scope: &mut v8::PinScope<'_, '_>,
        registry: &Rc<NodeRegistry>,
    ) {
        let queue = self
            .element_queue_stack
            .borrow_mut()
            .pop()
            .unwrap_or_default();
        if !queue.is_empty() {
            self.invoke_reactions_in_queue(scope, registry, queue);
        }
    }

    /// Invoke reactions for all element IDs in a given queue.
    pub(crate) fn invoke_reactions_in_queue(
        &self,
        scope: &mut v8::PinScope<'_, '_>,
        registry: &Rc<NodeRegistry>,
        queue: Vec<u32>,
    ) {
        for node_id in queue {
            let reactions = match self.take_reactions(node_id) {
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
                    Reaction::AttributeChanged {
                        callback,
                        name,
                        old_value,
                        new_value,
                    } => {
                        let cb = v8::Local::new(scope, callback);
                        let name_arg = string_or_null(scope, &Some(name));
                        let old_arg = string_or_null(scope, &old_value);
                        let new_arg = string_or_null(scope, &new_value);
                        let namespace_arg: v8::Local<v8::Value> = v8::null(scope).into();
                        let _ = cb.call(
                            scope,
                            recv.into(),
                            &[name_arg, old_arg, new_arg, namespace_arg],
                        );
                    }
                }
            }
        }
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

/// Convert an optional string to a V8 string value, or `null` when absent (or
/// on the rare allocation failure). Used to build `attributeChangedCallback`
/// arguments at drain time.
fn string_or_null<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    value: &Option<String>,
) -> v8::Local<'s, v8::Value> {
    match value {
        Some(text) => v8::String::new(scope, text)
            .map(|s| s.into())
            .unwrap_or_else(|| v8::null(scope).into()),
        None => v8::null(scope).into(),
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
        registry.ce_registry.invoke_reactions_in_queue(scope, registry, queue);
    }
    drained_any
}

/// A RAII guard that manages pushing and popping the custom element reactions stack
/// for a `[CEReactions]` boundary.
pub(crate) struct CeReactionsGuard<'a, 's, 'p> {
    scope: *mut v8::PinScope<'s, 'p>,
    registry: &'a Rc<NodeRegistry>,
}

impl<'a, 's, 'p> CeReactionsGuard<'a, 's, 'p> {
    pub(crate) fn new(scope: &mut v8::PinScope<'s, 'p>, registry: &'a Rc<NodeRegistry>) -> Self {
        registry.ce_registry.push_reactions_stack();
        Self {
            scope,
            registry,
        }
    }
}

impl<'a, 's, 'p> Drop for CeReactionsGuard<'a, 's, 'p> {
    fn drop(&mut self) {
        // SAFETY: the guard is created from a live `&mut PinScope` and is
        // dropped before that scope goes out of scope. The raw pointer lets the
        // caller continue using `scope` for the rest of the JS callback while
        // still restoring the CEReactions stack at the boundary end.
        let scope = unsafe { &mut *self.scope };
        self.registry
            .ce_registry
            .pop_and_restore_reactions_stack(scope, self.registry);
    }
}
