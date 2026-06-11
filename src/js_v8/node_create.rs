use v8;
use crate::dom::{Node, NodePtr};
use super::registry::NodeRegistry;
use super::selectors::query;
use super::tree::{mutation, navigation};
use super::style_class::{classlist, style};
use std::rc::Rc;

pub(super) struct NodeData {
    pub node: NodePtr,
    pub registry: Rc<NodeRegistry>,
    pub document: NodePtr,
}

pub(super) fn create_js_node<'s>(
    scope: &mut v8::HandleScope<'s>,
    node: NodePtr,
    registry: &Rc<NodeRegistry>,
    document: &NodePtr,
) -> v8::Local<'s, v8::Object> {
    let node_id = registry.register(node.clone());
    
    let template = v8::ObjectTemplate::new(scope);
    
    // Node properties
    template.set(v8_str(scope, "nodeType").into(), v8::Integer::new(scope, node_type(&node)).into());
    template.set(v8_str(scope, "nodeName").into(), v8_str(scope, &node_name(&node)).into());
    
    let node_data = Box::into_raw(Box::new(NodeData {
        node: node.clone(),
        registry: registry.clone(),
        document: document.clone(),
    })) as *mut _;
    let node_external = v8::External::new(scope, node_data);

    // --- Methods ---
    
    // querySelector / querySelectorAll
    install_method(scope, template, "querySelector", query_selector, node_external);
    install_method(scope, template, "querySelectorAll", query_selector_all, node_external);
    install_method(scope, template, "getElementsByTagName", get_elements_by_tag_name, node_external);
    install_method(scope, template, "matches", matches, node_external);
    install_method(scope, template, "closest", closest, node_external);

    // Mutation methods
    install_method(scope, template, "appendChild", append_child, node_external);
    install_method(scope, template, "removeChild", remove_child, node_external);
    install_method(scope, template, "insertBefore", insert_before, node_external);
    install_method(scope, template, "replaceChild", replace_child, node_external);
    install_method(scope, template, "remove", remove_node, node_external);
    install_method(scope, template, "cloneNode", clone_node, node_external);
    install_method(scope, template, "contains", contains, node_external);

    // Attribute methods
    install_method(scope, template, "getAttribute", get_attribute, node_external);
    install_method(scope, template, "setAttribute", set_attribute, node_external);
    install_method(scope, template, "removeAttribute", remove_attribute, node_external);
    install_method(scope, template, "hasAttribute", has_attribute, node_external);

    // --- Accessors ---
    
    install_accessor(scope, template, "parentNode", get_parent_node, None, node_external);
    install_accessor(scope, template, "firstChild", get_first_child, None, node_external);
    install_accessor(scope, template, "lastChild", get_last_child, None, node_external);
    install_accessor(scope, template, "nextSibling", get_next_sibling, None, node_external);
    install_accessor(scope, template, "previousSibling", get_previous_sibling, None, node_external);
    
    install_accessor(scope, template, "textContent", get_text_content, Some(set_text_content), node_external);
    install_accessor(scope, template, "innerText", get_text_content, Some(set_text_content), node_external);
    install_accessor(scope, template, "innerHTML", get_inner_html, Some(set_inner_html), node_external);
    install_accessor(scope, template, "outerHTML", get_outer_html, None, node_external);

    // style and classList
    install_accessor(scope, template, "style", get_style, None, node_external);
    install_accessor(scope, template, "classList", get_classlist, None, node_external);

    let obj = template.new_instance(scope).unwrap();
    
    obj.set(scope, v8_str(scope, "__aurora_node_id").into(), v8::Integer::new(scope, node_id as i32).into());

    if let Node::Element(el) = &*node.borrow() {
        obj.set(scope, v8_str(scope, "tagName").into(), v8_str(scope, &el.tag_name.to_uppercase()).into());
        obj.set(scope, v8_str(scope, "id").into(), v8_str(scope, el.attributes.get("id").unwrap_or(&"".to_string())).into());
        obj.set(scope, v8_str(scope, "className").into(), v8_str(scope, el.attributes.get("class").unwrap_or(&"".to_string())).into());
    }

    obj
}

fn v8_str<'s>(scope: &mut v8::HandleScope<'s>, s: &str) -> v8::Local<'s, v8::String> {
    v8::String::new(scope, s).unwrap()
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
    template.set(v8_str(scope, name).into(), t.into());
}

fn install_accessor<'s>(
    scope: &mut v8::HandleScope<'s>,
    template: v8::Local<v8::ObjectTemplate>,
    name: &str,
    getter: impl v8::MapFnTo<v8::AccessorNameGetterCallback>,
    setter: Option<impl v8::MapFnTo<v8::AccessorNameSetterCallback>>,
    data: v8::Local<'s, v8::External>,
) {
    if let Some(s) = setter {
        template.set_accessor_with_data_setter(
            v8_str(scope, name).into(),
            getter,
            s,
            data.into(),
        );
    } else {
        template.set_accessor_with_data(
            v8_str(scope, name).into(),
            getter,
            data.into(),
        );
    }
}

fn node_type(node: &NodePtr) -> i32 {
    match &*node.borrow() {
        Node::Element(_) => 1,
        Node::Text(_) => 3,
        Node::Document { .. } => 9,
    }
}

fn node_name(node: &NodePtr) -> String {
    match &*node.borrow() {
        Node::Element(el) => el.tag_name.to_uppercase(),
        Node::Text(_) => "#text".to_string(),
        Node::Document { .. } => "#document".to_string(),
    }
}

// ─── Callbacks ───────────────────────────────────────────────────────────────

fn node_from_js(
    scope: &mut v8::HandleScope,
    val: v8::Local<v8::Value>,
    registry: &NodeRegistry,
) -> Option<NodePtr> {
    if !val.is_object() {
        return None;
    }
    let obj = val.to_object(scope).unwrap();
    let key = v8_str(scope, "__aurora_node_id");
    let id_val = obj.get(scope, key.into())?;
    if !id_val.is_int32() {
        return None;
    }
    let id = id_val.int32_value(scope).unwrap() as u32;
    registry.get_node(id)
}

fn query_selector(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let selector = args.get(0).to_rust_string_lossy(scope);
    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    if let Some(found) = query::query_first(&node_data.document, &selector, &node_data.node) {
        let js_node = create_js_node(scope, found, &node_data.registry, &node_data.document);
        retval.set(js_node.into());
    } else {
        retval.set(v8::null(scope).into());
    }
}

fn query_selector_all(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let selector = args.get(0).to_rust_string_lossy(scope);
    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    let found = query::query_all(&node_data.document, &selector, &node_data.node);
    let array = v8::Array::new(scope, found.len() as i32);
    for (i, node) in found.into_iter().enumerate() {
        let js_node = create_js_node(scope, node, &node_data.registry, &node_data.document);
        array.set_index(scope, i as u32, js_node.into());
    }
    retval.set(array.into());
}

fn get_elements_by_tag_name(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let tag = args.get(0).to_rust_string_lossy(scope).to_lowercase();
    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    let mut out = Vec::new();
    query::collect_by_tag(&node_data.node, &tag, &mut out);

    let array = v8::Array::new(scope, out.len() as i32);
    for (i, node) in out.into_iter().enumerate() {
        let js_node = create_js_node(scope, node, &node_data.registry, &node_data.document);
        array.set_index(scope, i as u32, js_node.into());
    }
    retval.set(array.into());
}

fn matches(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let selector = args.get(0).to_rust_string_lossy(scope);
    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    let matched = query::selector_matches(&node_data.node, &selector, &node_data.document);
    retval.set(v8::Boolean::new(scope, matched).into());
}

fn closest(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let selector = args.get(0).to_rust_string_lossy(scope);
    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    let mut current = Some(node_data.node.clone());
    while let Some(n) = current {
        if query::selector_matches(&n, &selector, &node_data.document) {
            let js_node = create_js_node(scope, n, &node_data.registry, &node_data.document);
            retval.set(js_node.into());
            return;
        }
        current = query::find_parent(&node_data.document, &n);
    }
    retval.set(v8::null(scope).into());
}

fn append_child(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    if let Some(child) = node_from_js(scope, args.get(0), &node_data.registry) {
        mutation::append_child_ptr(&node_data.node, &child);
        node_data.registry.mark_layout_dirty(&node_data.node);
        retval.set(args.get(0));
    } else {
        retval.set(v8::null(scope).into());
    }
}

fn remove_child(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    if let Some(child) = node_from_js(scope, args.get(0), &node_data.registry) {
        mutation::remove_child_ptr(&node_data.node, &child);
        node_data.registry.mark_layout_dirty(&node_data.node);
        retval.set(args.get(0));
    } else {
        retval.set(v8::null(scope).into());
    }
}

fn insert_before(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    let new_child = node_from_js(scope, args.get(0), &node_data.registry);
    let ref_child = node_from_js(scope, args.get(1), &node_data.registry);

    if let Some(new_c) = new_child {
        mutation::insert_before_ptr(&node_data.node, &new_c, ref_child.as_ref());
        node_data.registry.mark_layout_dirty(&node_data.node);
        retval.set(args.get(0));
    } else {
        retval.set(v8::null(scope).into());
    }
}

fn replace_child(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    let new_child = node_from_js(scope, args.get(0), &node_data.registry);
    let old_child = node_from_js(scope, args.get(1), &node_data.registry);

    if let (Some(new_c), Some(old_c)) = (new_child, old_child) {
        mutation::replace_child_ptr(&node_data.node, &new_c, &old_c);
        node_data.registry.mark_layout_dirty(&node_data.node);
        retval.set(args.get(1));
    } else {
        retval.set(v8::null(scope).into());
    }
}

fn remove_node(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut _retval: v8::ReturnValue,
) {
    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    if let Some(parent) = query::find_parent(&node_data.document, &node_data.node) {
        mutation::remove_child_ptr(&parent, &node_data.node);
        node_data.registry.mark_layout_dirty(&parent);
    }
}

fn clone_node(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    let deep = args.get(0).is_true();
    let cloned = mutation::clone_node(&node_data.node, deep);
    let js_node = create_js_node(scope, cloned, &node_data.registry, &node_data.document);
    retval.set(js_node.into());
}

fn contains(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    if let Some(other) = node_from_js(scope, args.get(0), &node_data.registry) {
        retval.set(v8::Boolean::new(scope, mutation::contains_ptr(&node_data.node, &other)).into());
    } else {
        retval.set(v8::Boolean::new(scope, false).into());
    }
}

fn get_attribute(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let name = args.get(0).to_rust_string_lossy(scope);
    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    if let Node::Element(el) = &*node_data.node.borrow() {
        if let Some(val) = el.attributes.get(&name) {
            retval.set(v8_str(scope, val).into());
            return;
        }
    }
    retval.set(v8::null(scope).into());
}

fn set_attribute(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut _retval: v8::ReturnValue,
) {
    let name = args.get(0).to_rust_string_lossy(scope);
    let value = args.get(1).to_rust_string_lossy(scope);
    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    if let Node::Element(el) = &mut *node_data.node.borrow_mut() {
        el.attributes.insert(name, value);
        node_data.registry.mark_style_dirty(&node_data.node);
    }
}

fn remove_attribute(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut _retval: v8::ReturnValue,
) {
    let name = args.get(0).to_rust_string_lossy(scope);
    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    if let Node::Element(el) = &mut *node_data.node.borrow_mut() {
        el.attributes.remove(&name);
        node_data.registry.mark_style_dirty(&node_data.node);
    }
}

fn has_attribute(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let name = args.get(0).to_rust_string_lossy(scope);
    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    if let Node::Element(el) = &*node_data.node.borrow() {
        retval.set(v8::Boolean::new(scope, el.attributes.contains_key(&name)).into());
    } else {
        retval.set(v8::Boolean::new(scope, false).into());
    }
}

// ─── Accessors ───────────────────────────────────────────────────────────────

fn get_parent_node(
    scope: &mut v8::HandleScope,
    _name: v8::Local<v8::Name>,
    args: v8::PropertyCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    if let Some(parent) = query::find_parent(&node_data.document, &node_data.node) {
        let js_node = create_js_node(scope, parent, &node_data.registry, &node_data.document);
        retval.set(js_node.into());
    } else {
        retval.set(v8::null(scope).into());
    }
}

fn get_first_child(
    scope: &mut v8::HandleScope,
    _name: v8::Local<v8::Name>,
    args: v8::PropertyCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    if let Some(child) = navigation::first_child(&node_data.node, false) {
        let js_node = create_js_node(scope, child, &node_data.registry, &node_data.document);
        retval.set(js_node.into());
    } else {
        retval.set(v8::null(scope).into());
    }
}

fn get_last_child(
    scope: &mut v8::HandleScope,
    _name: v8::Local<v8::Name>,
    args: v8::PropertyCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    if let Some(child) = navigation::last_child(&node_data.node, false) {
        let js_node = create_js_node(scope, child, &node_data.registry, &node_data.document);
        retval.set(js_node.into());
    } else {
        retval.set(v8::null(scope).into());
    }
}

fn get_next_sibling(
    scope: &mut v8::HandleScope,
    _name: v8::Local<v8::Name>,
    args: v8::PropertyCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    if let Some(sibling) = navigation::sibling(&node_data.document, &node_data.node, 1, false) {
        let js_node = create_js_node(scope, sibling, &node_data.registry, &node_data.document);
        retval.set(js_node.into());
    } else {
        retval.set(v8::null(scope).into());
    }
}

fn get_previous_sibling(
    scope: &mut v8::HandleScope,
    _name: v8::Local<v8::Name>,
    args: v8::PropertyCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    if let Some(sibling) = navigation::sibling(&node_data.document, &node_data.node, -1, false) {
        let js_node = create_js_node(scope, sibling, &node_data.registry, &node_data.document);
        retval.set(js_node.into());
    } else {
        retval.set(v8::null(scope).into());
    }
}

fn get_text_content(
    scope: &mut v8::HandleScope,
    _name: v8::Local<v8::Name>,
    args: v8::PropertyCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    let text = mutation::collect_text(&node_data.node);
    retval.set(v8_str(scope, &text).into());
}

fn set_text_content(
    scope: &mut v8::HandleScope,
    _name: v8::Local<v8::Name>,
    value: v8::Local<v8::Value>,
    args: v8::PropertyCallbackArguments,
) {
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    let text = value.to_rust_string_lossy(scope);
    mutation::set_text_content(&node_data.node, &text);
    node_data.registry.mark_layout_dirty(&node_data.node);
}

fn get_inner_html(
    scope: &mut v8::HandleScope,
    _name: v8::Local<v8::Name>,
    args: v8::PropertyCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    let html = crate::js_boa::serialize_outer_html(&node_data.node); // Simplified: using existing serializer
    retval.set(v8_str(scope, &html).into());
}

fn set_inner_html(
    scope: &mut v8::HandleScope,
    _name: v8::Local<v8::Name>,
    value: v8::Local<v8::Value>,
    args: v8::PropertyCallbackArguments,
) {
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    let html = value.to_rust_string_lossy(scope);
    let parsed = crate::html::Parser::new(&html).parse_document();
    let new_children: Vec<NodePtr> = match &*parsed.borrow() {
        Node::Document { children, .. } => children.clone(),
        _ => Vec::new(),
    };
    if let Node::Element(el) = &mut *node_data.node.borrow_mut() {
        el.children = new_children;
        node_data.registry.mark_layout_dirty(&node_data.node);
    }
}

fn get_outer_html(
    scope: &mut v8::HandleScope,
    _name: v8::Local<v8::Name>,
    args: v8::PropertyCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    let html = crate::js_boa::serialize_outer_html(&node_data.node);
    retval.set(v8_str(scope, &html).into());
}

fn get_style(
    scope: &mut v8::HandleScope,
    _name: v8::Local<v8::Name>,
    args: v8::PropertyCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    let obj = style::build_style_object(scope, node_data);
    retval.set(obj.into());
}

fn get_classlist(
    scope: &mut v8::HandleScope,
    _name: v8::Local<v8::Name>,
    args: v8::PropertyCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_external = v8::Local::<v8::External>::new(scope, external);

    let obj = classlist::build_classlist_object(scope, node_external);
    retval.set(obj.into());
}
