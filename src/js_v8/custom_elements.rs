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
use std::collections::{BTreeMap, HashSet};
use std::rc::Rc;

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

/// Native mirror of the JS custom-element registry: name → definition.
#[derive(Default)]
pub(crate) struct CeRegistry {
    definitions: RefCell<BTreeMap<String, Rc<CeDefinition>>>,
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
}
