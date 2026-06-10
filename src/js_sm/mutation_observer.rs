#![allow(unsafe_op_in_unsafe_fn)]
use std::ffi::CString;
use std::ptr::NonNull;
use std::rc::Rc;

use mozjs::context::{JSContext, RawJSContext};
use mozjs::jsapi::{CallArgs, HandleValueArray, JSObject, Value};
use mozjs::jsval::{ObjectValue, UndefinedValue};
use mozjs::rooted;
use mozjs::rust::wrappers2;

use crate::dom::{Node, NodePtr};
use crate::js_sm::document::create_js_node;
use crate::js_sm::state::SmState;
use crate::js_sm::utils::*;

/// One `observe()` registration: an observer watching a target node for a
/// set of mutation kinds, with records accumulated until the next
/// drain (see `drain_mutation_observers`).
pub(super) struct MutationObserverEntry {
    observer_id: u32,
    callback_id: u32,
    self_id: u32,
    target_node_id: u32,
    child_list: bool,
    attributes: bool,
    subtree: bool,
    pub(super) pending: Vec<MutationRecordData>,
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
    },
}

// ── Install ──────────────────────────────────────────────────────────────────

pub(super) unsafe fn install_mutation_observer(
    cx: &mut JSContext,
    global: mozjs::gc::Handle<*mut JSObject>,
) {
    define_ctor(cx, global, c"MutationObserver", Some(mutation_observer_ctor), 1);
}

unsafe extern "C" fn mutation_observer_ctor(
    cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);

    let state = &mut *get_state_ptr(&cx);
    let observer_id = state.window.next_id();
    let callback_id = state.window.next_id();
    let self_id = state.window.next_id();
    let global_raw = state.global;
    rooted!(&in(cx) let global = global_raw);

    if argc > 0 {
        rooted!(&in(cx) let cb_val = args.get(0).get());
        store_callback(&mut cx, global.handle(), callback_id, cb_val.handle());
    }

    let obj = new_plain_object(&mut cx);
    rooted!(&in(cx) let obj_root = obj);
    set_prop_i32(&mut cx, obj_root.handle(), c"__mo_id__", observer_id as i32);
    set_prop_i32(&mut cx, obj_root.handle(), c"__mo_cb_id__", callback_id as i32);
    set_prop_i32(&mut cx, obj_root.handle(), c"__mo_self_id__", self_id as i32);
    define_fn(&mut cx, obj_root.handle(), c"observe", Some(mutation_observer_observe), 2);
    define_fn(&mut cx, obj_root.handle(), c"disconnect", Some(mutation_observer_disconnect), 0);
    define_fn(&mut cx, obj_root.handle(), c"takeRecords", Some(mutation_observer_take_records), 0);

    // Keep the observer object itself alive and retrievable so it can be
    // passed as the second argument to the callback on delivery.
    rooted!(&in(cx) let self_val = ObjectValue(obj));
    store_callback(&mut cx, global.handle(), self_id, self_val.handle());

    args.rval().set(ObjectValue(obj));
    true
}

unsafe extern "C" fn mutation_observer_observe(
    cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);

    let target_id = if argc > 0 {
        val_to_node_id(&mut cx, args.get(0).get()).unwrap_or(0)
    } else {
        0
    };

    let mut child_list = false;
    let mut attributes = false;
    let mut subtree = false;
    if argc > 1 && args.get(1).get().is_object() {
        let opts_obj = args.get(1).get().to_object_or_null();
        rooted!(&in(cx) let opts_root = opts_obj);
        child_list = get_prop_bool(&mut cx, opts_root.handle(), c"childList");
        attributes = get_prop_bool(&mut cx, opts_root.handle(), c"attributes");
        subtree = get_prop_bool(&mut cx, opts_root.handle(), c"subtree");
    }

    let this_obj = args.thisv().get().to_object_or_null();
    if !this_obj.is_null() {
        rooted!(&in(cx) let this_root = this_obj);
        let observer_id = get_prop_i32(&mut cx, this_root.handle(), c"__mo_id__") as u32;
        let callback_id = get_prop_i32(&mut cx, this_root.handle(), c"__mo_cb_id__") as u32;
        let self_id = get_prop_i32(&mut cx, this_root.handle(), c"__mo_self_id__") as u32;

        let state = &mut *get_state_ptr(&cx);
        state
            .mutation_observers
            .retain(|e| !(e.observer_id == observer_id && e.target_node_id == target_id));
        state.mutation_observers.push(MutationObserverEntry {
            observer_id,
            callback_id,
            self_id,
            target_node_id: target_id,
            child_list,
            attributes,
            subtree,
            pending: Vec::new(),
        });
    }

    args.rval().set(UndefinedValue());
    true
}

unsafe extern "C" fn mutation_observer_disconnect(
    cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);

    let this_obj = args.thisv().get().to_object_or_null();
    if !this_obj.is_null() {
        rooted!(&in(cx) let this_root = this_obj);
        let observer_id = get_prop_i32(&mut cx, this_root.handle(), c"__mo_id__") as u32;
        let state = &mut *get_state_ptr(&cx);
        state.mutation_observers.retain(|e| e.observer_id != observer_id);
    }

    args.rval().set(UndefinedValue());
    true
}

unsafe extern "C" fn mutation_observer_take_records(
    cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);

    let this_obj = args.thisv().get().to_object_or_null();
    let mut records = Vec::new();
    if !this_obj.is_null() {
        rooted!(&in(cx) let this_root = this_obj);
        let observer_id = get_prop_i32(&mut cx, this_root.handle(), c"__mo_id__") as u32;
        let state = &mut *get_state_ptr(&cx);
        for entry in state.mutation_observers.iter_mut() {
            if entry.observer_id == observer_id {
                records.append(&mut entry.pending);
            }
        }
    }

    let state = &mut *get_state_ptr(&cx);
    let arr = build_records_array(&mut cx, state, &records);
    args.rval().set(if arr.is_null() {
        UndefinedValue()
    } else {
        ObjectValue(arr)
    });
    true
}

// ── Mutation hooks (called from document/api.rs mutation sites) ──────────────

/// Record a childList mutation (nodes added/removed under `target_id`) against
/// any matching observers. No-op if nothing was added or removed.
pub(in crate::js_sm) fn queue_childlist_mutation(
    state: &mut SmState,
    target_id: u32,
    added: Vec<u32>,
    removed: Vec<u32>,
) {
    if added.is_empty() && removed.is_empty() {
        return;
    }
    queue_record(
        state,
        target_id,
        |e| e.child_list,
        MutationRecordData::ChildList {
            target: target_id,
            added,
            removed,
        },
    );
}

/// Record an attribute mutation on `target_id` against any matching observers.
pub(in crate::js_sm) fn queue_attribute_mutation(state: &mut SmState, target_id: u32, name: &str) {
    queue_record(
        state,
        target_id,
        |e| e.attributes,
        MutationRecordData::Attributes {
            target: target_id,
            name: name.to_string(),
        },
    );
}

fn queue_record(
    state: &mut SmState,
    target_id: u32,
    kind_matches: impl Fn(&MutationObserverEntry) -> bool,
    record: MutationRecordData,
) {
    if state.mutation_observers.is_empty() {
        return;
    }
    let registry = &state.registry;
    let target_node = registry.lookup(target_id);
    for entry in state.mutation_observers.iter_mut() {
        if !kind_matches(entry) {
            continue;
        }
        let matches = if entry.target_node_id == target_id {
            true
        } else if entry.subtree {
            match (registry.lookup(entry.target_node_id), &target_node) {
                (Some(root), Some(target)) => node_contains(&root, target),
                _ => false,
            }
        } else {
            false
        };
        if matches {
            entry.pending.push(record.clone());
        }
    }
}

fn node_contains(root: &NodePtr, needle: &NodePtr) -> bool {
    if Rc::ptr_eq(root, needle) {
        return true;
    }
    match &*root.borrow() {
        Node::Element(el) => el.children.iter().any(|c| node_contains(c, needle)),
        Node::Document { children, .. } => children.iter().any(|c| node_contains(c, needle)),
        _ => false,
    }
}

// ── Delivery ──────────────────────────────────────────────────────────────────

/// Deliver any accumulated MutationRecords to their observers' callbacks.
/// Called at task/microtask checkpoints from `SmRuntime`. Loops to handle
/// observers whose callbacks themselves trigger further mutations.
pub(in crate::js_sm) unsafe fn drain_mutation_observers(cx: &mut JSContext, state: &mut SmState) {
    if state.mutation_observers.iter().all(|e| e.pending.is_empty()) {
        return;
    }

    let global_raw = state.global;
    rooted!(&in(cx) let global = global_raw);

    for _ in 0..100 {
        let mut work: Vec<(u32, u32, Vec<MutationRecordData>)> = Vec::new();
        for entry in state.mutation_observers.iter_mut() {
            if !entry.pending.is_empty() {
                work.push((entry.callback_id, entry.self_id, std::mem::take(&mut entry.pending)));
            }
        }
        if work.is_empty() {
            break;
        }

        for (callback_id, self_id, records) in work {
            let arr = build_records_array(cx, state, &records);
            rooted!(&in(cx) let arr_root = ObjectValue(arr));

            let self_name = cb_prop_name(self_id);
            rooted!(&in(cx) let mut self_val = UndefinedValue());
            wrappers2::JS_GetProperty(cx, global.handle(), self_name.as_ptr(), self_val.handle_mut());

            call_observer_callback(cx, global.handle(), callback_id, arr_root.get(), self_val.get());
            clear_pending_exception(cx);
        }
    }
}

/// Call a `MutationCallback(records, observer)` stored under `__cb{id}__`.
unsafe fn call_observer_callback(
    cx: &mut JSContext,
    global: mozjs::gc::Handle<*mut JSObject>,
    callback_id: u32,
    records: Value,
    observer: Value,
) -> bool {
    let name = cb_prop_name(callback_id);
    rooted!(&in(cx) let mut cb_val = UndefinedValue());
    if !wrappers2::JS_GetProperty(cx, global, name.as_ptr(), cb_val.handle_mut())
        || !cb_val.get().is_object()
    {
        return false;
    }
    rooted!(&in(cx) let mut rval = UndefinedValue());
    let args_vals = [records, observer];
    let arr = HandleValueArray {
        length_: args_vals.len(),
        elements_: args_vals.as_ptr(),
    };
    wrappers2::JS_CallFunctionValue(cx, global, cb_val.handle(), &arr, rval.handle_mut())
}

// ── MutationRecord / NodeList construction ────────────────────────────────────

unsafe fn build_records_array(
    cx: &mut JSContext,
    state: &mut SmState,
    records: &[MutationRecordData],
) -> *mut JSObject {
    let arr = wrappers2::NewArrayObject(cx, &HandleValueArray::empty());
    if arr.is_null() {
        return arr;
    }
    rooted!(&in(cx) let arr_root = arr);
    for (i, rec) in records.iter().enumerate() {
        let rec_obj = build_mutation_record(cx, state, rec);
        rooted!(&in(cx) let rec_val = ObjectValue(rec_obj));
        wrappers2::JS_SetProperty(
            cx,
            arr_root.handle(),
            CString::new(i.to_string()).unwrap().as_ptr(),
            rec_val.handle(),
        );
    }
    arr
}

unsafe fn build_mutation_record(
    cx: &mut JSContext,
    state: &mut SmState,
    rec: &MutationRecordData,
) -> *mut JSObject {
    let obj = new_plain_object(cx);
    rooted!(&in(cx) let obj_root = obj);
    set_prop_null(cx, obj_root.handle(), c"oldValue");
    set_prop_null(cx, obj_root.handle(), c"previousSibling");
    set_prop_null(cx, obj_root.handle(), c"nextSibling");

    match rec {
        MutationRecordData::ChildList { target, added, removed } => {
            set_prop_str(cx, obj_root.handle(), c"type", "childList");
            set_target(cx, obj_root.handle(), state, *target);
            set_prop_null(cx, obj_root.handle(), c"attributeName");
            let added_arr = build_node_array(cx, state, added);
            set_prop_obj(cx, obj_root.handle(), c"addedNodes", added_arr);
            let removed_arr = build_node_array(cx, state, removed);
            set_prop_obj(cx, obj_root.handle(), c"removedNodes", removed_arr);
        }
        MutationRecordData::Attributes { target, name } => {
            set_prop_str(cx, obj_root.handle(), c"type", "attributes");
            set_target(cx, obj_root.handle(), state, *target);
            set_prop_str(cx, obj_root.handle(), c"attributeName", name);
            let empty_added = wrappers2::NewArrayObject(cx, &HandleValueArray::empty());
            set_prop_obj(cx, obj_root.handle(), c"addedNodes", empty_added);
            let empty_removed = wrappers2::NewArrayObject(cx, &HandleValueArray::empty());
            set_prop_obj(cx, obj_root.handle(), c"removedNodes", empty_removed);
        }
    }
    obj
}

unsafe fn set_target(
    cx: &mut JSContext,
    obj: mozjs::gc::Handle<*mut JSObject>,
    state: &mut SmState,
    target_id: u32,
) {
    match state.registry.lookup(target_id) {
        Some(node) => {
            let node_obj = create_js_node(cx, node);
            set_prop_obj(cx, obj, c"target", node_obj);
        }
        None => set_prop_null(cx, obj, c"target"),
    }
}

unsafe fn build_node_array(cx: &mut JSContext, state: &mut SmState, ids: &[u32]) -> *mut JSObject {
    let arr = wrappers2::NewArrayObject(cx, &HandleValueArray::empty());
    if arr.is_null() {
        return arr;
    }
    rooted!(&in(cx) let arr_root = arr);
    for (i, id) in ids.iter().enumerate() {
        if let Some(node) = state.registry.lookup(*id) {
            let node_obj = create_js_node(cx, node);
            rooted!(&in(cx) let node_val = ObjectValue(node_obj));
            wrappers2::JS_SetProperty(
                cx,
                arr_root.handle(),
                CString::new(i.to_string()).unwrap().as_ptr(),
                node_val.handle(),
            );
        }
    }
    arr
}
