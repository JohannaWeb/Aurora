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

use crate::dom::{Node, NodePtr};

use super::form_associated::FormReaction;
use super::registry::NodeRegistry;

/// Custom element state, per the HTML spec's element lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum CeState {
    /// No definition yet, but the name is a valid custom element name.
    #[default]
    Undefined,
    /// The name can never be customized (no hyphen, or a built-in).
    Uncustomized,
    /// Upgrade has started: the definition is known and reactions may be
    /// enqueued, but the constructor has not finished.
    Precustomized,
    /// Fully upgraded.
    Custom,
    /// The constructor threw, or produced a non-conforming element.
    Failed,
}

/// The reserved names that look like custom element names but are not, per
/// the HTML spec's "valid custom element name".
const RESERVED_ELEMENT_NAMES: [&str; 8] = [
    "annotation-xml",
    "color-profile",
    "font-face",
    "font-face-src",
    "font-face-uri",
    "font-face-format",
    "font-face-name",
    "missing-glyph",
];

/// `PCENChar`, the character production a custom element name is built from.
fn is_pcen_char(c: char) -> bool {
    matches!(c,
        '-' | '.' | '_' | '0'..='9' | 'a'..='z'
        | '\u{B7}'
        | '\u{C0}'..='\u{D6}'
        | '\u{D8}'..='\u{F6}'
        | '\u{F8}'..='\u{37D}'
        | '\u{37F}'..='\u{1FFF}'
        | '\u{200C}'..='\u{200D}'
        | '\u{203F}'..='\u{2040}'
        | '\u{2070}'..='\u{218F}'
        | '\u{2C00}'..='\u{2FEF}'
        | '\u{3001}'..='\u{D7FF}'
        | '\u{F900}'..='\u{FDCF}'
        | '\u{FDF0}'..='\u{FFFD}'
        | '\u{10000}'..='\u{EFFFF}')
}

/// Whether `name` is a valid custom element name: starts with an ASCII
/// lowercase letter, contains a hyphen, is built only from `PCENChar`, and is
/// not one of the reserved names.
pub(crate) fn is_valid_custom_element_name(name: &str) -> bool {
    if RESERVED_ELEMENT_NAMES.contains(&name) {
        return false;
    }
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_lowercase() {
        return false;
    }
    let mut has_hyphen = false;
    for c in chars {
        if c == '-' {
            has_hyphen = true;
        }
        if c.is_ascii_uppercase() || !is_pcen_char(c) {
            return false;
        }
    }
    has_hyphen
}

/// The submission value and validity an `ElementInternals` holds for a
/// form-associated custom element.
#[derive(Default)]
pub(crate) struct ElementInternalsState {
    pub(crate) value: Option<String>,
    pub(crate) valid: bool,
    pub(crate) validation_message: String,
}

/// Why a `customElements.define` call was rejected. Maps to the DOMException
/// the caller must throw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DefineError {
    /// The name is not a valid custom element name.
    InvalidName,
    /// The name is already defined.
    NameInUse,
    /// The constructor is already registered under another name.
    ConstructorInUse,
}

impl DefineError {
    /// The DOMException name the spec requires for this rejection.
    pub(crate) fn exception_name(self) -> &'static str {
        match self {
            DefineError::InvalidName => "SyntaxError",
            DefineError::NameInUse | DefineError::ConstructorInUse => "NotSupportedError",
        }
    }

    pub(crate) fn message(self, name: &str) -> String {
        match self {
            DefineError::InvalidName => {
                format!("'{name}' is not a valid custom element name")
            }
            DefineError::NameInUse => {
                format!("the name '{name}' has already been defined")
            }
            DefineError::ConstructorInUse => {
                "this constructor has already been registered".to_string()
            }
        }
    }
}

/// A registered custom-element definition.
///
/// Holds V8 global handles to the constructor and lifecycle callbacks so they
/// survive across turns. These are dropped when the owning [`CeRegistry`] (and
/// thus the `NodeRegistry`) is dropped, which happens before the isolate.
pub(crate) struct CeDefinition {
    pub(crate) name: String,
    pub(crate) constructor: v8::Global<v8::Function>,
    pub(crate) connected: Option<v8::Global<v8::Function>>,
    pub(crate) disconnected: Option<v8::Global<v8::Function>>,
    pub(crate) adopted: Option<v8::Global<v8::Function>>,
    pub(crate) attribute_changed: Option<v8::Global<v8::Function>>,
    pub(crate) observed_attributes: HashSet<String>,
    /// `static formAssociated = true` — makes the element form-associated and
    /// enables the four form lifecycle callbacks.
    pub(crate) form_associated: bool,
    /// Dispatched by [`super::form_associated`] as the element's form owner,
    /// disabled state, and owning form's reset drive them.
    pub(crate) form_associated_callback: Option<v8::Global<v8::Function>>,
    pub(crate) form_disabled_callback: Option<v8::Global<v8::Function>>,
    pub(crate) form_reset_callback: Option<v8::Global<v8::Function>>,
    /// Captured but never dispatched: this fires when a session's saved form
    /// state is restored, and Aurora has no session history to restore from.
    /// The call site lands with one.
    #[allow(dead_code)]
    pub(crate) form_state_restore_callback: Option<v8::Global<v8::Function>>,
    /// Registered by Aurora's own bootstrap polyfills rather than page script.
    /// A provisional definition is a fallback: real page script may define the
    /// same name, and that replaces it instead of raising `NotSupportedError`.
    pub(crate) provisional: bool,
}

/// A pending custom-element reaction, mirroring Ladybird's
/// `CustomElementReaction` variants.
enum Reaction {
    /// `connectedCallback` / `disconnectedCallback` — no arguments.
    Callback {
        callback: v8::Global<v8::Function>,
        args: Vec<v8::Global<v8::Value>>,
        /// Set only for `connectedCallback` reactions. Aurora's JS shim
        /// layers Polymer-compat orchestration (readyUpgraded, the
        /// ytd-app enable/stamp special case, `activeLifecycleHost`
        /// tracking that ShadyDOM-fragment composition depends on) around
        /// the raw callback; firing `callback` directly would skip all of
        /// that. When set, invocation calls the JS trampoline
        /// (`__aurora_ce_native_connect_trampoline__`) instead, which
        /// re-runs that orchestration and then calls the real callback
        /// itself. See `has_pending_connected_reaction`.
        native_connect: bool,
    },
    /// `connectedCallback` for an element whose definition captured no
    /// prototype callback at define time. YouTube's controller-extraction
    /// pattern registers thin shell classes with an empty prototype; the
    /// Polymer instance that actually implements the lifecycle lives on a
    /// lazily-created `polymerController` property of the element, so there
    /// was nothing to capture when `define` ran. Resolved dynamically at
    /// drain time: if the element's wrapper exposes a distinct
    /// `polymerController` object with a callable `connectedCallback`, invoke
    /// it with the controller as `this`; otherwise do nothing — the JS shim's
    /// upgrade path owns such elements exactly as it did before native
    /// reactions existed.
    DynamicConnected,
    /// `attributeChangedCallback(name, oldValue, newValue, namespace)`. The
    /// string values are held as Rust strings and converted to V8 at drain time,
    /// since the mutation path that enqueues has no V8 scope.
    AttributeChanged {
        callback: v8::Global<v8::Function>,
        name: String,
        old_value: Option<String>,
        new_value: Option<String>,
    },
    /// `adoptedCallback(oldDocument, newDocument)`, enqueued by the adopting
    /// steps when an element moves between documents. Unreachable while Aurora
    /// is single-document; see [`enqueue_adopted_reactions`].
    #[allow(dead_code)]
    Adopted {
        callback: v8::Global<v8::Function>,
        old_document: u32,
        new_document: u32,
    },
    /// The "upgrade reaction" — run the definition's constructor over an
    /// already-created element. Distinct from a callback reaction because it
    /// drives the construction stack rather than invoking a method.
    ///
    /// `is_connected` carries whether the element was connected at the time
    /// "try to upgrade" ran, so a successful construction knows whether to
    /// follow up with a `connectedCallback` reaction. Per spec, the
    /// attributeChanged/connected reactions this produces are generated
    /// *after* the constructor returns, from the element's own attribute
    /// list at that point — not captured ahead of time.
    Upgrade {
        definition: Rc<CeDefinition>,
        is_connected: bool,
    },
    /// One of the form lifecycle callbacks. The payload is scope-free (see
    /// [`FormReaction`]) because form association is recomputed on the mutation
    /// path, which has no V8 scope; the arguments are built at drain time.
    Form {
        callback: v8::Global<v8::Function>,
        payload: FormReaction,
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
    /// Per-element custom element state. Absent means the element has not been
    /// classified yet; [`CeRegistry::element_state`] resolves that to
    /// `Undefined`.
    element_states: RefCell<BTreeMap<u32, CeState>>,
    /// The definition each upgraded element was upgraded with.
    element_definitions: RefCell<BTreeMap<u32, Rc<CeDefinition>>>,
    /// Pending `customElements.whenDefined(name)` resolvers, by name.
    when_defined: RefCell<BTreeMap<String, Vec<v8::Global<v8::PromiseResolver>>>>,
    /// The construction stack, per the HTML spec: elements currently being
    /// upgraded. The `HTMLElement` constructor pops from here instead of
    /// allocating a fresh element.
    construction_stack: RefCell<Vec<u32>>,
    /// Set while `define` is running, so a constructor that re-enters `define`
    /// is rejected rather than corrupting the registry.
    definition_is_running: std::cell::Cell<bool>,
    /// True while Aurora's bootstrap polyfills run. Definitions registered in
    /// this window are provisional.
    bootstrap_phase: std::cell::Cell<bool>,
    /// `ElementInternals` state for form-associated custom elements, by node id.
    element_internals: RefCell<BTreeMap<u32, ElementInternalsState>>,
    /// The form owner last computed for each form-associated custom element.
    /// Absent means "no owner", so an element acquiring its first form is a
    /// change and fires `formAssociatedCallback`.
    form_owners: RefCell<BTreeMap<u32, u32>>,
    /// The disabled state last computed for each form-associated custom
    /// element. Absent means enabled.
    form_disabled: RefCell<BTreeMap<u32, bool>>,
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

    /// Validate a `customElements.define(name, ctor)` call, per the spec's
    /// checks: valid name, name not already in use, constructor not already
    /// registered, and no re-entrant definition. `Ok` means the caller may
    /// proceed to [`Self::define`].
    pub(crate) fn validate_define(
        &self,
        scope: &mut v8::PinScope<'_, '_>,
        name: &str,
        constructor: &v8::Global<v8::Function>,
    ) -> Result<(), DefineError> {
        if !is_valid_custom_element_name(name) {
            return Err(DefineError::InvalidName);
        }
        // A provisional definition is one of Aurora's own bootstrap fallbacks;
        // page script legitimately replaces it.
        if let Some(existing) = self.definitions.borrow().get(name)
            && !existing.provisional
        {
            return Err(DefineError::NameInUse);
        }
        let incoming = v8::Local::new(scope, constructor);
        let clash = self.definitions.borrow().values().any(|existing| {
            if existing.provisional || existing.name == name {
                return false;
            }
            let existing = v8::Local::new(scope, &existing.constructor);
            existing == incoming
        });
        if clash {
            return Err(DefineError::ConstructorInUse);
        }
        Ok(())
    }

    /// Whether `node_id` has already had `attachInternals()` called on it.
    pub(crate) fn has_element_internals(&self, node_id: u32) -> bool {
        self.element_internals.borrow().contains_key(&node_id)
    }

    /// Create the `ElementInternals` record for `node_id`.
    pub(crate) fn create_element_internals(&self, node_id: u32) {
        self.element_internals.borrow_mut().insert(
            node_id,
            ElementInternalsState {
                value: None,
                valid: true,
                validation_message: String::new(),
            },
        );
    }

    /// Record the element's submission value (`internals.setFormValue`).
    pub(crate) fn set_form_value(&self, node_id: u32, value: Option<String>) {
        if let Some(state) = self.element_internals.borrow_mut().get_mut(&node_id) {
            state.value = value;
        }
    }

    /// Record validity (`internals.setValidity`).
    pub(crate) fn set_validity(&self, node_id: u32, valid: bool, message: String) {
        if let Some(state) = self.element_internals.borrow_mut().get_mut(&node_id) {
            state.valid = valid;
            state.validation_message = message;
        }
    }

    /// Whether the element currently satisfies its constraints.
    pub(crate) fn is_valid(&self, node_id: u32) -> bool {
        self.element_internals
            .borrow()
            .get(&node_id)
            .is_none_or(|state| state.valid)
    }

    /// The element's current validation message.
    pub(crate) fn validation_message(&self, node_id: u32) -> String {
        self.element_internals
            .borrow()
            .get(&node_id)
            .map(|state| state.validation_message.clone())
            .unwrap_or_default()
    }

    /// The form owner last computed for `node_id`, if it had one.
    pub(crate) fn form_owner(&self, node_id: u32) -> Option<u32> {
        self.form_owners.borrow().get(&node_id).copied()
    }

    pub(crate) fn set_form_owner(&self, node_id: u32, owner: Option<u32>) {
        match owner {
            Some(owner) => {
                self.form_owners.borrow_mut().insert(node_id, owner);
            }
            None => {
                self.form_owners.borrow_mut().remove(&node_id);
            }
        }
    }

    /// The disabled state last computed for `node_id`; enabled by default.
    pub(crate) fn form_disabled(&self, node_id: u32) -> bool {
        self.form_disabled
            .borrow()
            .get(&node_id)
            .copied()
            .unwrap_or(false)
    }

    pub(crate) fn set_form_disabled(&self, node_id: u32, disabled: bool) {
        self.form_disabled.borrow_mut().insert(node_id, disabled);
    }

    /// Whether the element's definition opted into form association.
    pub(crate) fn is_form_associated(&self, name: &str) -> bool {
        self.lookup(name)
            .is_some_and(|definition| definition.form_associated)
    }

    /// Whether the engine's own bootstrap polyfills are still running.
    pub(crate) fn bootstrap_phase(&self) -> bool {
        self.bootstrap_phase.get()
    }

    pub(crate) fn set_bootstrap_phase(&self, active: bool) {
        self.bootstrap_phase.set(active);
    }

    /// Whether a definition is currently being processed (the spec's "element
    /// definition is running" flag).
    pub(crate) fn definition_is_running(&self) -> bool {
        self.definition_is_running.get()
    }

    pub(crate) fn set_definition_is_running(&self, running: bool) {
        self.definition_is_running.set(running);
    }

    /// The state of `node_id`, defaulting to `Undefined`.
    pub(crate) fn element_state(&self, node_id: u32) -> CeState {
        self.element_states
            .borrow()
            .get(&node_id)
            .copied()
            .unwrap_or_default()
    }

    pub(crate) fn set_element_state(&self, node_id: u32, state: CeState) {
        self.element_states.borrow_mut().insert(node_id, state);
    }

    /// Set the state and mirror the `:defined` bit onto the element itself, so
    /// the selector matcher can answer without reaching into the registry.
    pub(crate) fn set_element_state_on(&self, node: &NodePtr, node_id: u32, state: CeState) {
        self.set_element_state(node_id, state);
        let defined = !matches!(state, CeState::Undefined | CeState::Failed);
        if let Node::Element(el) = &mut *node.borrow_mut() {
            el.custom_element_defined = defined;
        }
    }

    /// The definition an element was upgraded with, if any.
    #[allow(dead_code)]
    pub(crate) fn definition_for_element(&self, node_id: u32) -> Option<Rc<CeDefinition>> {
        self.element_definitions.borrow().get(&node_id).cloned()
    }

    pub(crate) fn set_definition_for_element(&self, node_id: u32, definition: Rc<CeDefinition>) {
        self.element_definitions
            .borrow_mut()
            .insert(node_id, definition);
    }

    /// Park a `whenDefined(name)` resolver until the name is defined.
    pub(crate) fn push_when_defined(&self, name: &str, resolver: v8::Global<v8::PromiseResolver>) {
        self.when_defined
            .borrow_mut()
            .entry(name.to_string())
            .or_default()
            .push(resolver);
    }

    /// Take the resolvers waiting on `name`, to settle them after a define.
    pub(crate) fn take_when_defined(&self, name: &str) -> Vec<v8::Global<v8::PromiseResolver>> {
        self.when_defined
            .borrow_mut()
            .remove(name)
            .unwrap_or_default()
    }

    /// Push an element onto the construction stack ahead of running its
    /// constructor.
    pub(crate) fn push_construction(&self, node_id: u32) {
        self.construction_stack.borrow_mut().push(node_id);
    }

    pub(crate) fn pop_construction(&self) -> Option<u32> {
        self.construction_stack.borrow_mut().pop()
    }

    /// The element the running constructor should adopt rather than allocating
    /// a new one. This is what makes `super()` inside an upgrade return the
    /// element being upgraded.
    pub(crate) fn construction_stack_top(&self) -> Option<u32> {
        self.construction_stack.borrow().last().copied()
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
        native_connect: bool,
    ) {
        self.reaction_queues
            .borrow_mut()
            .entry(node_id)
            .or_default()
            .push_back(Reaction::Callback {
                callback,
                args,
                native_connect,
            });
        self.enqueue_element_id(node_id);
    }

    /// Enqueue a dynamically-resolved `connectedCallback` reaction for
    /// `node_id` (see [`Reaction::DynamicConnected`]). Used when the element's
    /// tag has a native definition but that definition captured no
    /// `connectedCallback` from the constructor's prototype.
    pub(crate) fn enqueue_dynamic_connected(&self, node_id: u32) {
        self.reaction_queues
            .borrow_mut()
            .entry(node_id)
            .or_default()
            .push_back(Reaction::DynamicConnected);
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

    /// Enqueue an `adoptedCallback(oldDocument, newDocument)` reaction.
    #[allow(dead_code)]
    pub(crate) fn enqueue_adopted(
        &self,
        node_id: u32,
        callback: v8::Global<v8::Function>,
        old_document: u32,
        new_document: u32,
    ) {
        self.reaction_queues
            .borrow_mut()
            .entry(node_id)
            .or_default()
            .push_back(Reaction::Adopted {
                callback,
                old_document,
                new_document,
            });
        self.enqueue_element_id(node_id);
    }

    /// Enqueue an upgrade reaction, which runs the definition's constructor
    /// over `node_id` when the queue drains.
    pub(crate) fn enqueue_upgrade(
        &self,
        node_id: u32,
        definition: Rc<CeDefinition>,
        is_connected: bool,
    ) {
        self.reaction_queues
            .borrow_mut()
            .entry(node_id)
            .or_default()
            .push_back(Reaction::Upgrade {
                definition,
                is_connected,
            });
        self.enqueue_element_id(node_id);
    }

    /// Enqueue a form lifecycle callback. See [`super::form_associated`] for
    /// the call sites that decide when each one fires.
    pub(crate) fn enqueue_form(
        &self,
        node_id: u32,
        callback: v8::Global<v8::Function>,
        payload: FormReaction,
    ) {
        self.reaction_queues
            .borrow_mut()
            .entry(node_id)
            .or_default()
            .push_back(Reaction::Form { callback, payload });
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
                invoke_reaction(scope, registry, recv, node_id, reaction);
            }
        }
    }

    /// Whether any reactions are queued.
    pub(crate) fn has_pending_reactions(&self) -> bool {
        !self.backup_queue.borrow().is_empty()
    }

    /// Whether `node_id` currently has a `connectedCallback` reaction queued
    /// (enqueued but not yet drained). Used by the JS shim's upgrade path
    /// (`connectUpgraded`) to decide whether the native insertion path already
    /// enqueued `connectedCallback` for this element — if so, the JS shim must
    /// not also call it directly, since the queued reaction will fire when the
    /// current `[CEReactions]` boundary (or the microtask checkpoint) drains.
    /// When none is queued (e.g. the element was upgraded out-of-band, via a
    /// detached-fragment composition rather than a real native mutation call),
    /// the JS shim falls back to calling `connectedCallback` itself, exactly as
    /// it did before native reactions existed. Only `native_connect` reactions
    /// (and their dynamically-resolved [`Reaction::DynamicConnected`]
    /// counterpart) count: an element can hold a queued
    /// `attributeChangedCallback` or `disconnectedCallback` without any connect
    /// pending, and deferring on those would wait for a trampoline call that
    /// never comes.
    pub(crate) fn has_pending_connected_reaction(&self, node_id: u32) -> bool {
        self.reaction_queues
            .borrow()
            .get(&node_id)
            .is_some_and(|queue| {
                queue.iter().any(|reaction| {
                    matches!(
                        reaction,
                        Reaction::Callback {
                            native_connect: true,
                            ..
                        } | Reaction::DynamicConnected
                    ) ||
                    // A queued upgrade that started out connected will itself
                    // enqueue (and, since it runs synchronously, immediately
                    // deliver) a connect reaction once its constructor
                    // succeeds — see `run_upgrade_constructor`. Treat it the
                    // same as a directly-queued connect so the JS shim still
                    // defers to native instead of firing connectedCallback
                    // itself ahead of the constructor.
                    matches!(reaction, Reaction::Upgrade { is_connected: true, .. })
                })
            })
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

/// Dynamic `connectedCallback` resolution for controller-extracted custom
/// elements (see [`Reaction::DynamicConnected`]): read the wrapper's
/// `polymerController` property — this may run the lazy getter that creates
/// the controller, which is intended; it is what YouTube's own shell classes
/// do on connect — and invoke its `connectedCallback` with the controller as
/// `this`. Polymer's `connectedCallback` internally enables the data system
/// and calls `ready()` on first flush, so this single call drives template
/// stamping too. A `TryCatch` contains failures so one component's throwing
/// callback cannot poison the remaining reactions in the drain. Returns
/// whether a controller callback was actually invoked; on `false` the caller
/// falls back to the JS trampoline.
fn invoke_controller_connected(
    scope: &mut v8::PinScope<'_, '_>,
    recv: v8::Local<'_, v8::Object>,
) -> bool {
    v8::tc_scope!(let scope, scope);
    let Some(key) = v8::String::new(scope, "polymerController") else {
        return false;
    };
    let Some(ctrl_val) = recv.get(scope, key.into()) else {
        return false;
    };
    let Ok(ctrl) = v8::Local::<v8::Object>::try_from(ctrl_val) else {
        return false;
    };
    if ctrl_val.strict_equals(recv.into()) {
        return false;
    }
    let Some(cb_key) = v8::String::new(scope, "connectedCallback") else {
        return false;
    };
    let Some(cb_val) = ctrl.get(scope, cb_key.into()) else {
        return false;
    };
    let Ok(cb) = v8::Local::<v8::Function>::try_from(cb_val) else {
        return false;
    };
    let _ = cb.call(scope, ctrl.into(), &[]);
    true
}

/// Look up a global function the JS shim installed by name. Used for the
/// shim's orchestration hooks the native reaction/upgrade path calls into
/// (see [`native_connect_trampoline`] and [`after_construct_hook`]). Always
/// present once `custom_elements.js` has run, which happens unconditionally
/// at bootstrap; the `None` case is a defensive fallback for callers of
/// `V8Runtime` that somehow skip bootstrap.
fn global_hook<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    name: &str,
) -> Option<v8::Local<'s, v8::Function>> {
    let context = scope.get_current_context();
    let global = context.global(scope);
    let key = v8::String::new(scope, name).expect("failed to create V8 string");
    global
        .get(scope, key.into())
        .and_then(|v| v8::Local::<v8::Function>::try_from(v).ok())
}

/// `__aurora_ce_native_connect_trampoline__`, the JS shim's orchestration
/// wrapper around `connectedCallback` (see the doc comment on
/// `Reaction::Callback::native_connect`).
fn native_connect_trampoline<'s>(
    scope: &mut v8::PinScope<'s, '_>,
) -> Option<v8::Local<'s, v8::Function>> {
    global_hook(scope, "__aurora_ce_native_connect_trampoline__")
}

/// `__aurora_ce_after_construct_native__`, the JS shim's orchestration
/// wrapper around the Polymer compensation a construction needs regardless of
/// which side ran it (`ceWrapPropMethods`, `maybeCallCreated`,
/// `readyUpgraded`'s `ready()` dispatch, lifecycle-method preservation — see
/// `afterConstruct` in `custom_elements.js`). Called from
/// [`run_upgrade_constructor`] once a construction succeeds, whether native
/// actually ran the constructor or the shim got there first.
fn after_construct_hook<'s>(
    scope: &mut v8::PinScope<'s, '_>,
) -> Option<v8::Local<'s, v8::Function>> {
    global_hook(scope, "__aurora_ce_after_construct_native__")
}

/// Invoke a single reaction against `recv` (the element's JS wrapper). Shared
/// by [`CeRegistry::invoke_reactions_in_queue`] and — for the reactions an
/// upgrade produces once its constructor has run — [`run_upgrade_constructor`],
/// so both call sites drive `native_connect`/`DynamicConnected` resolution and
/// argument marshalling identically.
fn invoke_reaction(
    scope: &mut v8::PinScope<'_, '_>,
    registry: &Rc<NodeRegistry>,
    recv: v8::Local<v8::Object>,
    node_id: u32,
    reaction: Reaction,
) {
    match reaction {
        Reaction::Callback {
            callback,
            args,
            native_connect,
        } => {
            if native_connect {
                if let Some(trampoline) = native_connect_trampoline(scope) {
                    let _ = trampoline.call(scope, recv.into(), &[]);
                    return;
                }
            }
            let cb = v8::Local::new(scope, callback);
            let arg_locals: Vec<v8::Local<v8::Value>> =
                args.iter().map(|a| v8::Local::new(scope, a)).collect();
            let _ = cb.call(scope, recv.into(), &arg_locals);
        }
        Reaction::DynamicConnected => {
            // No controller to drive → the element is a classic
            // definition without a prototype connectedCallback
            // (e.g. legacy `attached()` components). Fall back to
            // the JS trampoline: enqueueing this reaction made the
            // shim's connect path defer to us, so the connect MUST
            // be delivered here one way or the other.
            if !invoke_controller_connected(scope, recv) {
                if let Some(trampoline) = native_connect_trampoline(scope) {
                    let _ = trampoline.call(scope, recv.into(), &[]);
                }
            }
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
        Reaction::Adopted {
            callback,
            old_document,
            new_document,
        } => {
            let cb = v8::Local::new(scope, callback);
            let old_doc = registry
                .lookup_js_wrapper(scope, old_document)
                .map(|w| w.into())
                .unwrap_or_else(|| v8::null(scope).into());
            let new_doc = registry
                .lookup_js_wrapper(scope, new_document)
                .map(|w| w.into())
                .unwrap_or_else(|| v8::null(scope).into());
            let _ = cb.call(scope, recv.into(), &[old_doc, new_doc]);
        }
        Reaction::Upgrade {
            definition,
            is_connected,
        } => {
            run_upgrade_constructor(scope, registry, node_id, &definition, is_connected);
        }
        Reaction::Form { callback, payload } => {
            let cb = v8::Local::new(scope, callback);
            let args: Vec<v8::Local<v8::Value>> = match payload {
                FormReaction::Associated { form } => {
                    let form = form
                        .and_then(|id| registry.lookup_js_wrapper(scope, id))
                        .map(|wrapper| wrapper.into())
                        .unwrap_or_else(|| v8::null(scope).into());
                    vec![form]
                }
                FormReaction::Disabled { disabled } => {
                    vec![v8::Boolean::new(scope, disabled).into()]
                }
                FormReaction::Reset => Vec::new(),
            };
            let _ = cb.call(scope, recv.into(), &args);
        }
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
        registry
            .ce_registry
            .invoke_reactions_in_queue(scope, registry, queue);
    }
    drained_any
}

/// Whether the JS shim has already run this element's constructor, marked by
/// `__ce_upgraded__` on the wrapper.
fn wrapper_is_already_upgraded(
    scope: &mut v8::PinScope<'_, '_>,
    registry: &Rc<NodeRegistry>,
    node_id: u32,
) -> bool {
    let Some(wrapper) = registry.lookup_js_wrapper(scope, node_id) else {
        return false;
    };
    let Some(key) = v8::String::new(scope, "__ce_upgraded__") else {
        return false;
    };
    wrapper
        .get(scope, key.into())
        .is_some_and(|value| value.is_true())
}

/// Run a definition's constructor over an element that already exists — the
/// back half of "upgrade an element".
///
/// The element is pushed onto the construction stack first, so the
/// `HTMLElement` constructor adopts it (via
/// `__aurora_ce_construction_stack_top_native`) instead of allocating a new
/// one — which requires a JS wrapper to already exist for `node_id`, since
/// that is what the construction stack resolves to and hands back through
/// `super()`. A throwing constructor leaves the element in `Failed`, per
/// spec, rather than propagating out and poisoning the rest of the drain.
///
/// On success, this also runs the `attributeChangedCallback` and
/// `connectedCallback` reactions the spec generates *after* the constructor
/// returns — using the element's attribute list at that point, not one
/// captured ahead of time — so the whole upgrade completes atomically before
/// this function returns.
fn run_upgrade_constructor(
    scope: &mut v8::PinScope<'_, '_>,
    registry: &Rc<NodeRegistry>,
    node_id: u32,
    definition: &Rc<CeDefinition>,
    is_connected: bool,
) {
    let Some(node) = registry.lookup(node_id) else {
        return;
    };
    registry
        .ce_registry
        .set_element_state_on(&node, node_id, CeState::Precustomized);
    registry
        .ce_registry
        .set_definition_for_element(node_id, definition.clone());

    // Ensure a wrapper exists *before* construction: the construction stack
    // adopts by looking up this node's existing wrapper, so without one the
    // constructor would silently mint a detached element instead of
    // upgrading the real node.
    let document = registry
        .document
        .borrow()
        .clone()
        .unwrap_or_else(|| node.clone());
    let wrapper = super::node_create::create_js_node(scope, node.clone(), registry, &document);

    // The JS shim still owns construction for elements it upgraded itself —
    // running the constructor again here would double-fire it, so skip
    // straight to the follow-up attribute-changed/connected reactions below,
    // which still need to be delivered exactly once regardless of which side
    // constructed the element. This check goes away with the shim's upgrade
    // path.
    let succeeded = if wrapper_is_already_upgraded(scope, registry, node_id) {
        true
    } else {
        registry.ce_registry.push_construction(node_id);
        let constructed = {
            v8::tc_scope!(let tc, scope);
            let ctor = v8::Local::new(tc, &definition.constructor);
            // `class X extends HTMLElement { constructor() { super(); ... } }`
            // and a plain `function C() { HTMLElement.call(this); }` need
            // different invocations — this is not a stylistic choice, it is
            // forced by the language. A derived class's `this` is uninitialized
            // until `super()` runs, and `super()`'s result becomes `this` for
            // the rest of the constructor — that engine-level rebinding is
            // exactly what makes `new_instance` + construction-stack adoption
            // (PatchedHTMLElement, gated on `new.target`) work for `class`.
            // A plain function has no such mechanism: `new C()` always binds
            // `this` to its own fresh object, and nothing later in `C`'s body
            // can retarget it without an explicit `return` — which ordinary
            // ES5-adapter code has no reason to write (this codebase's own
            // fixtures don't). So for that shape, bind `this` to the real
            // element from the very first line by calling the constructor
            // directly on it instead of through `new`, exactly as the JS
            // shim's `tryUpgrade` used to (`ctor.call(el)` after an external
            // prototype swap) — construction-stack adoption still fires for
            // the nested `HTMLElement.call(this)`, but is redundant with the
            // swap already done here.
            let source: String = v8::Local::<v8::Value>::from(ctor).to_rust_string_lossy(tc);
            let is_class_style = source.starts_with("class ");
            let called = if is_class_style {
                ctor.new_instance(tc, &[]).is_some()
            } else {
                if let Some(proto_key) = v8::String::new(tc, "prototype")
                    && let Some(proto) = ctor.get(tc, proto_key.into())
                {
                    wrapper.set_prototype(tc, proto);
                }
                ctor.call(tc, wrapper.into(), &[]).is_some()
            };
            called && !tc.has_caught()
        };
        registry.ce_registry.pop_construction();
        constructed
    };

    if !succeeded {
        registry
            .ce_registry
            .set_element_state_on(&node, node_id, CeState::Failed);
        return;
    }
    registry
        .ce_registry
        .set_element_state_on(&node, node_id, CeState::Custom);

    // Idempotent on the JS side either way: when the shim already
    // constructed this element it already ran this compensation itself (from
    // `tryUpgrade`) and every step here no-ops the second time; when native
    // just ran the constructor for real, this is the only place it happens.
    if let Some(hook) = after_construct_hook(scope) {
        let ctor = v8::Local::new(scope, &definition.constructor);
        let _ = hook.call(scope, wrapper.into(), &[ctor.into()]);
    }

    let attributes: Vec<(String, String)> = match &*node.borrow() {
        Node::Element(el) => el
            .attributes
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        _ => Vec::new(),
    };
    for (name, value) in attributes {
        if !definition.observed_attributes.contains(&name) {
            continue;
        }
        if let Some(callback) = &definition.attribute_changed {
            invoke_reaction(
                scope,
                registry,
                wrapper,
                node_id,
                Reaction::AttributeChanged {
                    callback: callback.clone(),
                    name,
                    old_value: None,
                    new_value: Some(value),
                },
            );
        }
    }
    if is_connected {
        match &definition.connected {
            Some(callback) => invoke_reaction(
                scope,
                registry,
                wrapper,
                node_id,
                Reaction::Callback {
                    callback: callback.clone(),
                    args: Vec::new(),
                    native_connect: true,
                },
            ),
            // No prototype connectedCallback to capture: the lifecycle may
            // live on a lazily-created controller, resolved here.
            None => invoke_reaction(scope, registry, wrapper, node_id, Reaction::DynamicConnected),
        }
    }
}

/// "Upgrade an element": classify `node` against the registry and, if a
/// definition exists, enqueue the upgrade reaction. Per spec, "try to
/// upgrade" enqueues *only* the upgrade reaction — the
/// `attributeChangedCallback`/`connectedCallback` reactions it produces are
/// generated from inside that reaction, after the constructor has run (see
/// [`run_upgrade_constructor`]), not ahead of time.
///
/// This is the entry point the native insertion path uses, replacing the JS
/// shim's `tryUpgrade`. Returns whether an upgrade was started.
pub(super) fn try_upgrade_element(
    registry: &Rc<NodeRegistry>,
    node: &NodePtr,
    is_connected: bool,
) -> bool {
    let tag = match &*node.borrow() {
        Node::Element(el) => el.tag_name.clone(),
        _ => return false,
    };
    // A name that can never be customized, or one with no definition yet,
    // needs no bookkeeping: `:defined` and `ce_state_native` both fall back
    // to `Undefined`/`Uncustomized` correctly without a persisted entry, so
    // there is nothing here worth spending a registry id on.
    if !is_valid_custom_element_name(&tag) {
        return false;
    }
    let Some(definition) = registry.ce_registry.lookup(&tag) else {
        return false;
    };
    let node_id = registry.register(node.clone());
    if !matches!(
        registry.ce_registry.element_state(node_id),
        CeState::Undefined
    ) {
        return false;
    }
    registry
        .ce_registry
        .enqueue_upgrade(node_id, definition, is_connected);
    true
}

/// Synchronously run "upgrade an element" for a single element, outside the
/// reaction queue. The entry point `__aurora_ce_upgrade_now_native__` exposes
/// to the JS shim's `tryUpgrade`, which schedules *when* to upgrade (handling
/// all the YouTube-specific timing: pending-definition tracking, deferral
/// during active template stamping, the connect retry ladder) but no longer
/// performs construction itself.
///
/// Differs from [`try_upgrade_element`] (the insertion path's entry point,
/// which has no V8 scope and must enqueue a reaction to run at the next
/// drain) by calling [`run_upgrade_constructor`] immediately — required
/// because a synchronous `document.createElement` of an already-defined name
/// must return a fully constructed element, per spec, not one that upgrades
/// on a later microtask. Returns whether an upgrade was attempted.
pub(super) fn upgrade_element_now(
    scope: &mut v8::PinScope<'_, '_>,
    registry: &Rc<NodeRegistry>,
    node: &NodePtr,
) -> bool {
    let tag = match &*node.borrow() {
        Node::Element(el) => el.tag_name.clone(),
        _ => return false,
    };
    if !is_valid_custom_element_name(&tag) {
        return false;
    }
    let Some(definition) = registry.ce_registry.lookup(&tag) else {
        return false;
    };
    let node_id = registry.register(node.clone());
    if !matches!(
        registry.ce_registry.element_state(node_id),
        CeState::Undefined
    ) {
        return false;
    }
    let is_connected = registry
        .document
        .borrow()
        .as_ref()
        .is_some_and(|document| super::tree::mutation::is_connected_to(document, node));
    run_upgrade_constructor(scope, registry, node_id, &definition, is_connected);
    true
}

/// The adopting steps' custom-element half: enqueue `adoptedCallback` for every
/// upgraded custom element in the adopted subtree.
///
/// Aurora is single-document today, so `old_document == new_document` for every
/// real call and this correctly enqueues nothing. It exists so the callback has
/// a defined call site once a second document (an iframe, a template document)
/// can own nodes.
#[allow(dead_code)]
pub(super) fn enqueue_adopted_reactions(
    registry: &Rc<NodeRegistry>,
    root: &NodePtr,
    old_document: u32,
    new_document: u32,
) {
    if old_document == new_document {
        return;
    }
    let mut stack = vec![root.clone()];
    while let Some(node) = stack.pop() {
        let (tag, children) = match &*node.borrow() {
            Node::Element(el) => (Some(el.tag_name.clone()), el.children.clone()),
            _ => (None, Vec::new()),
        };
        if let Some(tag) = tag {
            let node_id = registry.register(node.clone());
            if matches!(registry.ce_registry.element_state(node_id), CeState::Custom)
                && let Some(definition) = registry.ce_registry.lookup(&tag)
                && let Some(callback) = &definition.adopted
            {
                registry.ce_registry.enqueue_adopted(
                    node_id,
                    callback.clone(),
                    old_document,
                    new_document,
                );
            }
        }
        stack.extend(children);
    }
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
        Self { scope, registry }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_custom_element_names_require_a_hyphen_and_lowercase_start() {
        assert!(is_valid_custom_element_name("my-element"));
        assert!(is_valid_custom_element_name("ytd-video-renderer"));
        assert!(is_valid_custom_element_name("x-"));

        assert!(!is_valid_custom_element_name(""));
        assert!(!is_valid_custom_element_name("noHyphen"));
        assert!(!is_valid_custom_element_name("div"));
        assert!(!is_valid_custom_element_name("-leading"));
        assert!(!is_valid_custom_element_name("1-digit-start"));
        assert!(!is_valid_custom_element_name("My-Element"));
        assert!(!is_valid_custom_element_name("my-Element"));
    }

    #[test]
    fn reserved_svg_and_mathml_names_are_not_custom_element_names() {
        for name in RESERVED_ELEMENT_NAMES {
            assert!(
                !is_valid_custom_element_name(name),
                "{name} must stay reserved"
            );
        }
    }

    #[test]
    fn define_errors_map_to_the_dom_exceptions_the_spec_requires() {
        assert_eq!(DefineError::InvalidName.exception_name(), "SyntaxError");
        assert_eq!(DefineError::NameInUse.exception_name(), "NotSupportedError");
        assert_eq!(
            DefineError::ConstructorInUse.exception_name(),
            "NotSupportedError"
        );
    }

    #[test]
    fn element_state_defaults_to_undefined_and_round_trips() {
        let registry = CeRegistry::default();
        assert_eq!(registry.element_state(7), CeState::Undefined);
        registry.set_element_state(7, CeState::Custom);
        assert_eq!(registry.element_state(7), CeState::Custom);
    }

    #[test]
    fn construction_stack_is_last_in_first_out() {
        let registry = CeRegistry::default();
        assert_eq!(registry.construction_stack_top(), None);
        registry.push_construction(1);
        registry.push_construction(2);
        assert_eq!(registry.construction_stack_top(), Some(2));
        assert_eq!(registry.pop_construction(), Some(2));
        assert_eq!(registry.construction_stack_top(), Some(1));
        assert_eq!(registry.pop_construction(), Some(1));
        assert_eq!(registry.construction_stack_top(), None);
    }
}
