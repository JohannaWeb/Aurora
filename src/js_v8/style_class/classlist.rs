use v8;
use crate::dom::{Node, NodePtr};
use crate::js_v8::node_create::{NodeData, create_js_node};
use std::collections::BTreeSet;

pub(crate) fn build_classlist_object<'s>(
    scope: &mut v8::HandleScope<'s>,
    node_external: v8::Local<'s, v8::External>,
) -> v8::Local<'s, v8::Object> {
    let template = v8::ObjectTemplate::new(scope);

    install_method(scope, template, "add", add, node_external);
    install_method(scope, template, "remove", remove, node_external);
    install_method(scope, template, "contains", contains, node_external);
    install_method(scope, template, "toggle", toggle, node_external);
    install_method(scope, template, "replace", replace, node_external);
    install_method(scope, template, "item", item, node_external);
    install_method(scope, template, "toString", to_string, node_external);

    template.new_instance(scope).unwrap()
}

fn install_method<'s>(
    scope: &mut v8::HandleScope<'s>,
    template: v8::Local<v8::ObjectTemplate>,
    name: &str,
    callback: impl v8::MapFnTo<v8::FunctionCallback>,
    data: v8::Local<'s, v8::External>,
) {
    let t = v8::FunctionTemplate::builder(callback)
        .data(data.into())
        .build(scope);
    let name_str = v8::String::new(scope, name).unwrap();
    template.set(name_str.into(), t.into());
}

fn add(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut _retval: v8::ReturnValue,
) {
    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    for i in 0..args.length() {
        let cls = args.get(i).to_rust_string_lossy(scope);
        modify_classlist(&node_data.node, |set| {
            set.insert(cls);
        });
    }
    node_data.registry.mark_style_dirty(&node_data.node);
}

fn remove(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut _retval: v8::ReturnValue,
) {
    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    for i in 0..args.length() {
        let cls = args.get(i).to_rust_string_lossy(scope);
        modify_classlist(&node_data.node, |set| {
            set.remove(&cls);
        });
    }
    node_data.registry.mark_style_dirty(&node_data.node);
}

fn contains(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let cls = args.get(0).to_rust_string_lossy(scope);
    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    let b = node_data.node.borrow();
    if let Node::Element(el) = &*b {
        if let Some(v) = el.attributes.get("class") {
            let present = v.split_whitespace().any(|c| c == cls);
            retval.set(v8::Boolean::new(scope, present).into());
            return;
        }
    }
    retval.set(v8::Boolean::new(scope, false).into());
}

fn toggle(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let cls = args.get(0).to_rust_string_lossy(scope);
    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    let mut present = false;
    modify_classlist(&node_data.node, |set| {
        if set.contains(&cls) {
            set.remove(&cls);
        } else {
            set.insert(cls);
            present = true;
        }
    });
    node_data.registry.mark_style_dirty(&node_data.node);
    retval.set(v8::Boolean::new(scope, present).into());
}

fn replace(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let old_cls = args.get(0).to_rust_string_lossy(scope);
    let new_cls = args.get(1).to_rust_string_lossy(scope);
    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    let mut swapped = false;
    modify_classlist(&node_data.node, |set| {
        if set.remove(&old_cls) {
            set.insert(new_cls);
            swapped = true;
        }
    });
    node_data.registry.mark_style_dirty(&node_data.node);
    retval.set(v8::Boolean::new(scope, swapped).into());
}

fn item(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let idx = args.get(0).uint32_value(scope).unwrap_or(0) as usize;
    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    let b = node_data.node.borrow();
    if let Node::Element(el) = &*b {
        if let Some(v) = el.attributes.get("class") {
            if let Some(cls) = v.split_whitespace().nth(idx) {
                let s = v8::String::new(scope, cls).unwrap();
                retval.set(s.into());
                return;
            }
        }
    }
    retval.set(v8::null(scope).into());
}

fn to_string(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    let b = node_data.node.borrow();
    if let Node::Element(el) = &*b {
        if let Some(v) = el.attributes.get("class") {
            let s = v8::String::new(scope, v).unwrap();
            retval.set(s.into());
            return;
        }
    }
    let empty = v8::String::new(scope, "").unwrap();
    retval.set(empty.into());
}

fn modify_classlist<F: FnOnce(&mut BTreeSet<String>)>(node: &NodePtr, f: F) {
    if let Node::Element(el) = &mut *node.borrow_mut() {
        let mut set: BTreeSet<String> = el
            .attributes
            .get("class")
            .map(|s| s.split_whitespace().map(String::from).collect())
            .unwrap_or_default();
        f(&mut set);
        let joined = set.into_iter().collect::<Vec<_>>().join(" ");
        if joined.is_empty() {
            el.attributes.remove("class");
        } else {
            el.attributes.insert("class".to_string(), joined);
        }
    }
}
