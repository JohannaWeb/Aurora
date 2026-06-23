use crate::dom::{Node, NodePtr};
use crate::js_v8::node_create::{NodeData, node_data_from, v8_str};
use std::collections::BTreeSet;

pub(crate) fn build_classlist_object<'s>(
    scope: &mut v8::PinScope<'s, '_>,
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

    let obj = template
        .new_instance(scope)
        .expect("object template instantiation failed");

    // Array-like shape (length + indexed tokens) so iteration helpers work.
    // The classList object is rebuilt on every `.classList` access, so a
    // snapshot taken here stays consistent with the live class attribute.
    let node_data = unsafe { &*(node_external.value() as *const NodeData) };
    let tokens: Vec<String> = match &*node_data.node.borrow() {
        Node::Element(el) => el
            .attributes
            .get("class")
            .map(|v| v.split_whitespace().map(String::from).collect())
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    let length_key = v8_str(scope, "length");
    let length_val = v8::Integer::new(scope, tokens.len() as i32);
    obj.set(scope, length_key.into(), length_val.into());
    let value_key = v8_str(scope, "value");
    let value_val = v8_str(scope, &tokens.join(" "));
    obj.set(scope, value_key.into(), value_val.into());
    for (i, token) in tokens.iter().enumerate() {
        let token_val = v8_str(scope, token);
        obj.set_index(scope, i as u32, token_val.into());
    }

    obj
}

fn install_method<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    template: v8::Local<v8::ObjectTemplate>,
    name: &str,
    callback: impl v8::MapFnTo<v8::FunctionCallback>,
    data: v8::Local<'s, v8::External>,
) {
    let t = v8::FunctionTemplate::builder(callback)
        .data(data.into())
        .build(scope);
    let name_str = v8_str(scope, name);
    template.set(name_str.into(), t.into());
}

fn add(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut _retval: v8::ReturnValue,
) {
    let node_data = node_data_from(args.data());

    for i in 0..args.length() {
        let cls = args.get(i).to_rust_string_lossy(scope);
        modify_classlist(&node_data.node, |set| {
            set.insert(cls);
        });
    }
    node_data.registry.mark_style_dirty(&node_data.node);
}

fn remove(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut _retval: v8::ReturnValue,
) {
    let node_data = node_data_from(args.data());

    for i in 0..args.length() {
        let cls = args.get(i).to_rust_string_lossy(scope);
        modify_classlist(&node_data.node, |set| {
            set.remove(&cls);
        });
    }
    node_data.registry.mark_style_dirty(&node_data.node);
}

fn contains(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let cls = args.get(0).to_rust_string_lossy(scope);
    let node_data = node_data_from(args.data());

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
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let cls = args.get(0).to_rust_string_lossy(scope);
    let node_data = node_data_from(args.data());

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
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let old_cls = args.get(0).to_rust_string_lossy(scope);
    let new_cls = args.get(1).to_rust_string_lossy(scope);
    let node_data = node_data_from(args.data());

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
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let idx = args.get(0).uint32_value(scope).unwrap_or(0) as usize;
    let node_data = node_data_from(args.data());

    let b = node_data.node.borrow();
    if let Node::Element(el) = &*b {
        if let Some(v) = el.attributes.get("class") {
            if let Some(cls) = v.split_whitespace().nth(idx) {
                let s = v8_str(scope, cls);
                retval.set(s.into());
                return;
            }
        }
    }
    retval.set(v8::null(scope).into());
}

fn to_string(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let node_data = node_data_from(args.data());

    let b = node_data.node.borrow();
    if let Node::Element(el) = &*b {
        if let Some(v) = el.attributes.get("class") {
            let s = v8_str(scope, v);
            retval.set(s.into());
            return;
        }
    }
    let empty = v8_str(scope, "");
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
