//! A real `MutationObserver` for the V8 backend.
//!
//! `observe()` registrations accumulate
//! `MutationRecord`s as the DOM is mutated (childList / attributes, optionally
//! over a subtree), and the records are delivered to each observer's callback
//! when the event loop is pumped (`deliver`). Polymer/ShadyDOM rely on this
//! firing to react to DOM changes during hydration.

use std::rc::Rc;

use crate::dom::NodePtr;

use super::node_create::create_js_node;
use super::registry::NodeRegistry;

/// A constructed `MutationObserver`: its callback and the observer object itself
/// (passed as the 2nd argument to the callback on delivery).
pub(super) struct MoObserver {
    pub(super) callback: v8::Global<v8::Function>,
    pub(super) observer: v8::Global<v8::Object>,
}

/// One `observe(target, options)` registration plus its pending records.
pub(super) struct MoEntry {
    observer_id: u32,
    target_id: u32,
    child_list: bool,
    attributes: bool,
    character_data: bool,
    attribute_old_value: bool,
    character_data_old_value: bool,
    subtree: bool,
    pending: Vec<MutationRecordData>,
}

#[derive(Clone)]
pub(super) enum MutationRecordData {
    ChildList {
        target: u32,
        added: Vec<u32>,
        removed: Vec<u32>,
    },
    Attributes {
        target: u32,
        name: String,
        old_value: Option<String>,
    },
    CharacterData {
        target: u32,
        old_value: Option<String>,
    },
}

fn v8_str<'s>(scope: &v8::PinScope<'s, '_, ()>, s: &str) -> v8::Local<'s, v8::String> {
    // `String::new` only returns `None` for strings past V8's length limit or
    // while the isolate is terminating; our callers pass short internal keys.
    v8::String::new(scope, s).expect("v8 string allocation failed")
}

// ── Install ───────────────────────────────────────────────────────────────────

/// Define `globalThis.MutationObserver` backed by `registry`.
pub(super) fn install(
    scope: &mut v8::PinScope<'_, '_>,
    global: v8::Local<v8::Object>,
    registry_data: v8::Local<v8::External>,
) {
    let ctor = v8::FunctionTemplate::builder(mutation_observer_ctor)
        .data(registry_data.into())
        .build(scope);
    let ctor_fn = ctor
        .get_function(scope)
        .expect("function template yields a function outside of a pending exception");
    global.set(
        scope,
        v8_str(scope, "MutationObserver").into(),
        ctor_fn.into(),
    );
}

fn registry_from_data<'a>(args_data: v8::Local<'a, v8::Value>) -> &'a Rc<NodeRegistry> {
    let external = v8::Local::<v8::External>::try_from(args_data)
        .expect("callback data is always the registry External we installed");
    let registry_ptr = external.value() as *const Rc<NodeRegistry>;
    unsafe { &*registry_ptr }
}

// ── Constructor + methods ───────────────────────────────────────────────────────

fn mutation_observer_ctor(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let registry = registry_from_data(args.data());

    let Ok(callback) = v8::Local::<v8::Function>::try_from(args.get(0)) else {
        return;
    };

    let observer_id = registry.alloc_observer_id();
    let observer_obj = v8::Object::new(scope);
    let id_val = v8::Integer::new_from_unsigned(scope, observer_id);
    let id_key = v8_str(scope, "__mo_id__");
    observer_obj.set(scope, id_key.into(), id_val.into());

    // Bind the instance methods with their own External over the same registry
    // pointer; each method recovers its observer id from `this.__mo_id__`.
    let registry_ptr = v8::Local::<v8::External>::try_from(args.data())
        .expect("callback data is always the registry External we installed")
        .value();
    install_method(
        scope,
        observer_obj,
        "observe",
        mutation_observer_observe,
        registry_ptr,
    );
    install_method(
        scope,
        observer_obj,
        "disconnect",
        mutation_observer_disconnect,
        registry_ptr,
    );
    install_method(
        scope,
        observer_obj,
        "takeRecords",
        mutation_observer_take_records,
        registry_ptr,
    );

    registry.mo_observers.borrow_mut().insert(
        observer_id,
        MoObserver {
            callback: v8::Global::new(scope, callback),
            observer: v8::Global::new(scope, observer_obj),
        },
    );

    retval.set(observer_obj.into());
}

fn install_method(
    scope: &mut v8::PinScope<'_, '_>,
    obj: v8::Local<v8::Object>,
    name: &str,
    callback: impl v8::MapFnTo<v8::FunctionCallback>,
    registry_ptr: *mut std::ffi::c_void,
) {
    let ext = v8::External::new(scope, registry_ptr);
    let t = v8::FunctionTemplate::builder(callback)
        .data(ext.into())
        .build(scope);
    let f = t
        .get_function(scope)
        .expect("function template yields a function outside of a pending exception");
    let key = v8_str(scope, name);
    obj.set(scope, key.into(), f.into());
}

fn observer_id_of(
    scope: &mut v8::PinScope<'_, '_>,
    args: &v8::FunctionCallbackArguments,
) -> Option<u32> {
    let this = args.this();
    let key = v8_str(scope, "__mo_id__");
    this.get(scope, key.into())?.uint32_value(scope)
}

fn mutation_observer_observe(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    _retval: v8::ReturnValue,
) {
    let registry = registry_from_data(args.data());
    let Some(observer_id) = observer_id_of(scope, &args) else {
        return;
    };

    // Resolve the target node id from its wrapper's `__aurora_node_id`.
    let target = args.get(0);
    if !target.is_object() {
        return;
    }
    let Some(obj) = target.to_object(scope) else {
        return;
    };
    let key = v8_str(scope, "__aurora_node_id");
    let Some(target_id) = obj
        .get(scope, key.into())
        .and_then(|v| v.uint32_value(scope))
    else {
        return;
    };

    let (
        mut child_list,
        mut attributes,
        mut character_data,
        mut attribute_old_value,
        mut character_data_old_value,
        mut subtree,
    ) = (false, false, false, false, false, false);
    let mut attributes_set = false;
    let mut character_data_set = false;
    let opts = args.get(1);
    if let Some(opts) = opts.is_object().then(|| opts.to_object(scope)).flatten() {
        let read_bool = |scope: &mut v8::PinScope<'_, '_>, name: &str| -> Option<bool> {
            let key = v8_str(scope, name);
            let val = opts.get(scope, key.into())?;
            (!val.is_undefined()).then(|| val.boolean_value(scope))
        };
        child_list = read_bool(scope, "childList").unwrap_or(false);
        if let Some(v) = read_bool(scope, "attributes") {
            attributes = v;
            attributes_set = true;
        }
        if let Some(v) = read_bool(scope, "characterData") {
            character_data = v;
            character_data_set = true;
        }
        attribute_old_value = read_bool(scope, "attributeOldValue").unwrap_or(false);
        character_data_old_value = read_bool(scope, "characterDataOldValue").unwrap_or(false);
        subtree = read_bool(scope, "subtree").unwrap_or(false);
        if attribute_old_value && !attributes_set {
            attributes = true;
        }
        if character_data_old_value && !character_data_set {
            character_data = true;
        }
    }

    let mut entries = registry.mo_entries.borrow_mut();
    entries.retain(|e| !(e.observer_id == observer_id && e.target_id == target_id));
    entries.push(MoEntry {
        observer_id,
        target_id,
        child_list,
        attributes,
        character_data,
        attribute_old_value,
        character_data_old_value,
        subtree,
        pending: Vec::new(),
    });
}

fn mutation_observer_disconnect(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    _retval: v8::ReturnValue,
) {
    let registry = registry_from_data(args.data());
    if let Some(observer_id) = observer_id_of(scope, &args) {
        registry
            .mo_entries
            .borrow_mut()
            .retain(|e| e.observer_id != observer_id);
    }
}

fn mutation_observer_take_records(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let registry = registry_from_data(args.data());
    let mut records = Vec::new();
    if let Some(observer_id) = observer_id_of(scope, &args) {
        for entry in registry.mo_entries.borrow_mut().iter_mut() {
            if entry.observer_id == observer_id {
                records.append(&mut entry.pending);
            }
        }
    }
    let arr = build_records_array(scope, registry, &records);
    retval.set(arr.into());
}

// ── Mutation hooks (called from the js_v8 mutation callbacks) ───────────────────

/// Whether any observer has undelivered records.
pub(super) fn has_pending(registry: &Rc<NodeRegistry>) -> bool {
    registry
        .mo_entries
        .borrow()
        .iter()
        .any(|e| !e.pending.is_empty())
}

/// Record a childList mutation (nodes added/removed under `target_id`).
pub(super) fn queue_childlist(
    registry: &Rc<NodeRegistry>,
    target_id: u32,
    added: Vec<u32>,
    removed: Vec<u32>,
) {
    if added.is_empty() && removed.is_empty() {
        return;
    }
    queue_record(
        registry,
        target_id,
        |e| e.child_list,
        |_| MutationRecordData::ChildList {
            target: target_id,
            added: added.clone(),
            removed: removed.clone(),
        },
    );
}

/// Record an attribute mutation on `target_id`.
pub(super) fn queue_attribute(
    registry: &Rc<NodeRegistry>,
    target_id: u32,
    name: &str,
    old_value: Option<&str>,
) {
    queue_record(
        registry,
        target_id,
        |e| e.attributes,
        |e| MutationRecordData::Attributes {
            target: target_id,
            name: name.to_string(),
            old_value: e
                .attribute_old_value
                .then(|| old_value.map(str::to_string))
                .flatten(),
        },
    );
}

/// Record a `characterData` mutation on `target_id`.
pub(super) fn queue_character_data(registry: &Rc<NodeRegistry>, target_id: u32, old_value: &str) {
    queue_record(
        registry,
        target_id,
        |e| e.character_data,
        |e| MutationRecordData::CharacterData {
            target: target_id,
            old_value: e.character_data_old_value.then(|| old_value.to_string()),
        },
    );
}

fn queue_record(
    registry: &Rc<NodeRegistry>,
    target_id: u32,
    kind_matches: impl Fn(&MoEntry) -> bool,
    make_record: impl Fn(&MoEntry) -> MutationRecordData,
) {
    if registry.mo_entries.borrow().is_empty() {
        return;
    }
    let target_node = registry.lookup(target_id);
    let mut entries = registry.mo_entries.borrow_mut();
    for entry in entries.iter_mut() {
        if !kind_matches(entry) {
            continue;
        }
        let matches = if entry.target_id == target_id {
            true
        } else if entry.subtree {
            match (registry.lookup(entry.target_id), &target_node) {
                (Some(root), Some(target)) => is_descendant_of(target, &root),
                _ => false,
            }
        } else {
            false
        };
        if matches {
            let record = make_record(entry);
            entry.pending.push(record);
        }
    }
}

/// Whether `node` is `root` or a descendant of it, walking up via parent pointers.
fn is_descendant_of(node: &NodePtr, root: &NodePtr) -> bool {
    let mut current = node.clone();
    for _ in 0..4096 {
        if Rc::ptr_eq(&current, root) {
            return true;
        }
        match crate::dom::parent_ptr(&current) {
            Some(parent) => current = parent,
            None => return false,
        }
    }
    false
}

// ── Delivery ────────────────────────────────────────────────────────────────────

/// Deliver accumulated records to observer callbacks. Returns true if any were
/// delivered. Loops a bounded number of times so callbacks that themselves
/// mutate the DOM get their follow-up records too.
pub(super) fn deliver(scope: &mut v8::PinScope<'_, '_>, registry: &Rc<NodeRegistry>) -> bool {
    let mut delivered_any = false;
    for _ in 0..100 {
        let mut work: Vec<(u32, Vec<MutationRecordData>)> = Vec::new();
        {
            let mut entries = registry.mo_entries.borrow_mut();
            for entry in entries.iter_mut() {
                if entry.pending.is_empty() {
                    continue;
                }
                let records = std::mem::take(&mut entry.pending);
                match work.iter_mut().find(|(id, _)| *id == entry.observer_id) {
                    Some((_, acc)) => acc.extend(records),
                    None => work.push((entry.observer_id, records)),
                }
            }
        }
        if work.is_empty() {
            break;
        }
        delivered_any = true;

        for (observer_id, records) in work {
            let (callback, observer) = {
                let observers = registry.mo_observers.borrow();
                match observers.get(&observer_id) {
                    Some(o) => (
                        v8::Local::new(scope, &o.callback),
                        v8::Local::new(scope, &o.observer),
                    ),
                    None => continue,
                }
            };
            let arr = build_records_array(scope, registry, &records);
            let recv: v8::Local<v8::Value> = observer.into();
            let _ = callback.call(scope, recv, &[arr.into(), observer.into()]);
        }
    }
    delivered_any
}

// ── Record construction ─────────────────────────────────────────────────────────

fn build_records_array<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    registry: &Rc<NodeRegistry>,
    records: &[MutationRecordData],
) -> v8::Local<'s, v8::Array> {
    let arr = v8::Array::new(scope, records.len() as i32);
    for (i, rec) in records.iter().enumerate() {
        let obj = build_record(scope, registry, rec);
        arr.set_index(scope, i as u32, obj.into());
    }
    arr
}

fn build_record<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    registry: &Rc<NodeRegistry>,
    rec: &MutationRecordData,
) -> v8::Local<'s, v8::Object> {
    let obj = v8::Object::new(scope);
    for k in ["oldValue", "previousSibling", "nextSibling"] {
        let key = v8_str(scope, k);
        let null = v8::null(scope);
        obj.set(scope, key.into(), null.into());
    }
    let document = registry.document.borrow().clone();

    match rec {
        MutationRecordData::ChildList {
            target,
            added,
            removed,
        } => {
            set_str(scope, obj, "type", "childList");
            set_target(scope, obj, registry, &document, *target);
            let null = v8::null(scope);
            let key = v8_str(scope, "attributeName");
            obj.set(scope, key.into(), null.into());
            let added_arr = build_node_array(scope, registry, added, &document);
            let key = v8_str(scope, "addedNodes");
            obj.set(scope, key.into(), added_arr.into());
            let removed_arr = build_node_array(scope, registry, removed, &document);
            let key = v8_str(scope, "removedNodes");
            obj.set(scope, key.into(), removed_arr.into());
        }
        MutationRecordData::Attributes {
            target,
            name,
            old_value,
        } => {
            set_str(scope, obj, "type", "attributes");
            set_target(scope, obj, registry, &document, *target);
            set_str(scope, obj, "attributeName", name);
            if let Some(old_value) = old_value {
                set_str(scope, obj, "oldValue", old_value);
            }
            let empty1 = v8::Array::new(scope, 0);
            let key = v8_str(scope, "addedNodes");
            obj.set(scope, key.into(), empty1.into());
            let empty2 = v8::Array::new(scope, 0);
            let key = v8_str(scope, "removedNodes");
            obj.set(scope, key.into(), empty2.into());
        }
        MutationRecordData::CharacterData { target, old_value } => {
            set_str(scope, obj, "type", "characterData");
            set_target(scope, obj, registry, &document, *target);
            let null = v8::null(scope);
            let key = v8_str(scope, "attributeName");
            obj.set(scope, key.into(), null.into());
            if let Some(old_value) = old_value {
                set_str(scope, obj, "oldValue", old_value);
            }
            let empty1 = v8::Array::new(scope, 0);
            let key = v8_str(scope, "addedNodes");
            obj.set(scope, key.into(), empty1.into());
            let empty2 = v8::Array::new(scope, 0);
            let key = v8_str(scope, "removedNodes");
            obj.set(scope, key.into(), empty2.into());
        }
    }
    obj
}

fn set_str(scope: &mut v8::PinScope<'_, '_>, obj: v8::Local<v8::Object>, key: &str, value: &str) {
    let k = v8_str(scope, key);
    let v = v8_str(scope, value);
    obj.set(scope, k.into(), v.into());
}

fn set_target(
    scope: &mut v8::PinScope<'_, '_>,
    obj: v8::Local<v8::Object>,
    registry: &Rc<NodeRegistry>,
    document: &Option<NodePtr>,
    target_id: u32,
) {
    let value: v8::Local<v8::Value> = match (registry.lookup(target_id), document) {
        (Some(node), Some(doc)) => create_js_node(scope, node, registry, doc).into(),
        _ => v8::null(scope).into(),
    };
    let key = v8_str(scope, "target");
    obj.set(scope, key.into(), value);
}

fn build_node_array<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    registry: &Rc<NodeRegistry>,
    ids: &[u32],
    document: &Option<NodePtr>,
) -> v8::Local<'s, v8::Array> {
    let arr = v8::Array::new(scope, ids.len() as i32);
    if let Some(doc) = document {
        for (i, id) in ids.iter().enumerate() {
            if let Some(node) = registry.lookup(*id) {
                let wrapper = create_js_node(scope, node, registry, doc);
                arr.set_index(scope, i as u32, wrapper.into());
            }
        }
    }
    arr
}
