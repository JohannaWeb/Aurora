use super::registry::NodeRegistry;
use super::selectors::query;
use super::style_class::{classlist, style};
use super::tree::{mutation, navigation};
use crate::dom::{Node, NodePtr};
use std::rc::Rc;
use v8;

pub(super) struct NodeData {
    pub node: NodePtr,
    pub registry: Rc<NodeRegistry>,
    pub document: NodePtr,
}

pub(super) fn create_js_node<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    node: NodePtr,
    registry: &Rc<NodeRegistry>,
    document: &NodePtr,
) -> v8::Local<'s, v8::Object> {
    let node_id = registry.register(node.clone());
    if let Some(existing) = registry.lookup_js_wrapper(scope, node_id) {
        return existing;
    }

    let template = v8::ObjectTemplate::new(scope);

    // Node properties
    template.set(
        v8_str(scope, "nodeType").into(),
        v8::Integer::new(scope, node_type(&node)).into(),
    );
    template.set(
        v8_str(scope, "nodeName").into(),
        v8_str(scope, &node_name(&node)).into(),
    );

    let node_data = Box::into_raw(Box::new(NodeData {
        node: node.clone(),
        registry: registry.clone(),
        document: document.clone(),
    })) as *mut _;
    let node_external = v8::External::new(scope, node_data);

    // --- Methods ---

    // querySelector / querySelectorAll
    install_method(
        scope,
        template,
        "querySelector",
        query_selector,
        node_external,
    );
    install_method(
        scope,
        template,
        "querySelectorAll",
        query_selector_all,
        node_external,
    );
    install_method(
        scope,
        template,
        "getElementsByTagName",
        get_elements_by_tag_name,
        node_external,
    );
    install_method(scope, template, "matches", matches, node_external);
    install_method(scope, template, "closest", closest, node_external);

    // Mutation methods
    install_method(scope, template, "appendChild", append_child, node_external);
    install_method(scope, template, "removeChild", remove_child, node_external);
    install_method(
        scope,
        template,
        "insertBefore",
        insert_before,
        node_external,
    );
    install_method(
        scope,
        template,
        "replaceChild",
        replace_child,
        node_external,
    );
    install_method(scope, template, "remove", remove_node, node_external);
    install_method(scope, template, "cloneNode", clone_node, node_external);
    install_method(scope, template, "contains", contains, node_external);

    // Attribute methods
    install_method(
        scope,
        template,
        "getAttribute",
        get_attribute,
        node_external,
    );
    install_method(
        scope,
        template,
        "setAttribute",
        set_attribute,
        node_external,
    );
    install_method(
        scope,
        template,
        "removeAttribute",
        remove_attribute,
        node_external,
    );
    install_method(
        scope,
        template,
        "hasAttribute",
        has_attribute,
        node_external,
    );

    // --- Accessors ---

    install_readonly_accessor(
        scope,
        template,
        "parentNode",
        get_parent_node,
        node_external,
    );
    install_readonly_accessor(
        scope,
        template,
        "firstChild",
        get_first_child,
        node_external,
    );
    install_readonly_accessor(scope, template, "lastChild", get_last_child, node_external);
    install_readonly_accessor(
        scope,
        template,
        "nextSibling",
        get_next_sibling,
        node_external,
    );
    install_readonly_accessor(
        scope,
        template,
        "previousSibling",
        get_previous_sibling,
        node_external,
    );

    install_accessor(
        scope,
        template,
        "textContent",
        get_text_content,
        set_text_content,
        node_external,
    );
    install_accessor(
        scope,
        template,
        "innerText",
        get_text_content,
        set_text_content,
        node_external,
    );
    install_accessor(
        scope,
        template,
        "innerHTML",
        get_inner_html,
        set_inner_html,
        node_external,
    );
    install_readonly_accessor(scope, template, "outerHTML", get_outer_html, node_external);
    install_accessor(scope, template, "id", get_id, set_id, node_external);
    install_accessor(
        scope,
        template,
        "className",
        get_class_name,
        set_class_name,
        node_external,
    );

    // style and classList
    install_readonly_accessor(scope, template, "style", get_style, node_external);
    install_readonly_accessor(scope, template, "classList", get_classlist, node_external);
    install_readonly_accessor(scope, template, "attributes", get_attributes, node_external);

    // Tree collections
    install_readonly_accessor(scope, template, "childNodes", get_child_nodes, node_external);
    install_readonly_accessor(scope, template, "children", get_children, node_external);
    install_readonly_accessor(
        scope,
        template,
        "firstElementChild",
        get_first_element_child,
        node_external,
    );
    install_readonly_accessor(
        scope,
        template,
        "ownerDocument",
        get_owner_document,
        node_external,
    );

    // Events
    install_method(
        scope,
        template,
        "addEventListener",
        node_add_event_listener,
        node_external,
    );
    install_method(
        scope,
        template,
        "removeEventListener",
        node_remove_event_listener,
        node_external,
    );
    install_method(
        scope,
        template,
        "dispatchEvent",
        node_dispatch_event,
        node_external,
    );

    // Geometry and misc
    install_method(
        scope,
        template,
        "getBoundingClientRect",
        get_bounding_client_rect,
        node_external,
    );
    install_method(
        scope,
        template,
        "getClientRects",
        get_client_rects,
        node_external,
    );
    install_method(scope, template, "getRootNode", get_root_node, node_external);
    install_method(scope, template, "append", append_children, node_external);
    install_method(
        scope,
        template,
        "getElementsByClassName",
        get_elements_by_class_name,
        node_external,
    );

    // Shadow DOM — the returned "root" proxies the host node itself (no real
    // shadow tree separation), mirroring js_sm's element_attach_shadow.
    install_method(scope, template, "attachShadow", attach_shadow, node_external);
    install_method(
        scope,
        template,
        "__shady_attachShadow",
        attach_shadow,
        node_external,
    );

    let obj = template.new_instance(scope).unwrap();

    obj.set(
        scope,
        v8_str(scope, "__aurora_node_id").into(),
        v8::Integer::new(scope, node_id as i32).into(),
    );

    let mut is_custom_element = false;
    let mut is_canvas = false;
    let mut template_content: Option<NodePtr> = None;
    if let Node::Element(el) = &*node.borrow() {
        is_canvas = el.tag_name.eq_ignore_ascii_case("canvas");
        if el.tag_name != "#document-fragment" {
            obj.set(
                scope,
                v8_str(scope, "tagName").into(),
                v8_str(scope, &el.tag_name.to_uppercase()).into(),
            );
            obj.set(
                scope,
                v8_str(scope, "localName").into(),
                v8_str(scope, &el.tag_name.to_lowercase()).into(),
            );
            // Initial attribute snapshot, mirroring js_sm's wrapper props.
            for attr in ["href", "src", "type", "name", "value"] {
                let val = el.attributes.get(attr).cloned().unwrap_or_default();
                obj.set(
                    scope,
                    v8_str(scope, attr).into(),
                    v8_str(scope, &val).into(),
                );
            }
            let dataset = v8::Object::new(scope);
            obj.set(scope, v8_str(scope, "dataset").into(), dataset.into());
            obj.set(scope, v8_str(scope, "shadowRoot").into(), v8::null(scope).into());
            is_custom_element = el.tag_name.contains('-');
        }
        if el.tag_name.eq_ignore_ascii_case("template") {
            template_content = Some(
                el.template_contents
                    .clone()
                    .unwrap_or_else(|| Node::document_fragment(Vec::new())),
            );
        }
    }

    if let Some(content) = template_content {
        // Persist so later innerHTML writes land in the same fragment this
        // JS `content` object wraps (script-created templates start empty).
        if let Node::Element(el) = &mut *node.borrow_mut() {
            if el.template_contents.is_none() {
                el.template_contents = Some(content.clone());
            }
        }
        let content_obj = create_js_node(scope, content, registry, document);
        obj.set(scope, v8_str(scope, "content").into(), content_obj.into());
    }

    registry.store_js_wrapper(scope, node_id, obj);

    // JS-side decoration hooks, installed by the bootstrap polyfills. Called
    // after store_js_wrapper so re-entrant lookups resolve to this wrapper.
    if node_type(&node) == 1 {
        call_global_hook(scope, "__aurora_decorate_element__", obj);
    }
    if is_canvas {
        call_global_hook(scope, "__aurora_install_canvas__", obj);
    }
    // Let the custom-elements registry track/upgrade dash-named elements.
    if is_custom_element {
        call_global_hook(scope, "__aurora_track_custom_element__", obj);
    }

    obj
}

fn call_global_hook(
    scope: &mut v8::PinScope<'_, '_>,
    name: &str,
    arg: v8::Local<v8::Object>,
) {
    let context = scope.get_current_context();
    let global = context.global(scope);
    let key = v8_str(scope, name);
    if let Some(hook) = global.get(scope, key.into()) {
        if let Ok(hook) = v8::Local::<v8::Function>::try_from(hook) {
            let _ = hook.call(scope, global.into(), &[arg.into()]);
        }
    }
}

fn v8_str<'s>(scope: &v8::PinScope<'s, '_, ()>, s: &str) -> v8::Local<'s, v8::String> {
    v8::String::new(scope, s).unwrap()
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
    template.set(v8_str(scope, name).into(), t.into());
}

fn install_accessor<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    template: v8::Local<v8::ObjectTemplate>,
    name: &str,
    getter: impl v8::MapFnTo<v8::AccessorNameGetterCallback>,
    setter: impl v8::MapFnTo<v8::AccessorNameSetterCallback>,
    data: v8::Local<'s, v8::External>,
) {
    template.set_accessor_with_configuration(
        v8_str(scope, name).into(),
        v8::AccessorConfiguration::new(getter)
            .setter(setter)
            .data(data.into()),
    );
}

fn install_readonly_accessor<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    template: v8::Local<v8::ObjectTemplate>,
    name: &str,
    getter: impl v8::MapFnTo<v8::AccessorNameGetterCallback>,
    data: v8::Local<'s, v8::External>,
) {
    template.set_accessor_with_configuration(
        v8_str(scope, name).into(),
        v8::AccessorConfiguration::new(getter).data(data.into()),
    );
}

fn node_type(node: &NodePtr) -> i32 {
    match &*node.borrow() {
        Node::Element(el) if el.tag_name == "#document-fragment" => 11,
        Node::Element(_) => 1,
        Node::Text(_) => 3,
        Node::Document { .. } => 9,
    }
}

fn node_name(node: &NodePtr) -> String {
    match &*node.borrow() {
        Node::Element(el) if el.tag_name == "#document-fragment" => {
            "#document-fragment".to_string()
        }
        Node::Element(el) => el.tag_name.to_uppercase(),
        Node::Text(_) => "#text".to_string(),
        Node::Document { .. } => "#document".to_string(),
    }
}

// ─── Callbacks ───────────────────────────────────────────────────────────────

fn node_from_js(
    scope: &mut v8::PinScope<'_, '_>,
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
    registry.lookup(id)
}

fn query_selector(
    scope: &mut v8::PinScope<'_, '_>,
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
    scope: &mut v8::PinScope<'_, '_>,
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
    scope: &mut v8::PinScope<'_, '_>,
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
    scope: &mut v8::PinScope<'_, '_>,
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
    scope: &mut v8::PinScope<'_, '_>,
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
    scope: &mut v8::PinScope<'_, '_>,
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
    scope: &mut v8::PinScope<'_, '_>,
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
    scope: &mut v8::PinScope<'_, '_>,
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
    scope: &mut v8::PinScope<'_, '_>,
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
    scope: &mut v8::PinScope<'_, '_>,
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
    scope: &mut v8::PinScope<'_, '_>,
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
    scope: &mut v8::PinScope<'_, '_>,
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
    scope: &mut v8::PinScope<'_, '_>,
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
    scope: &mut v8::PinScope<'_, '_>,
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
    scope: &mut v8::PinScope<'_, '_>,
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
    scope: &mut v8::PinScope<'_, '_>,
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

fn build_attr_object<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    name: &str,
    value: &str,
) -> v8::Local<'s, v8::Object> {
    let obj = v8::Object::new(scope);
    obj.set(
        scope,
        v8_str(scope, "name").into(),
        v8_str(scope, name).into(),
    );
    obj.set(
        scope,
        v8_str(scope, "nodeName").into(),
        v8_str(scope, name).into(),
    );
    obj.set(
        scope,
        v8_str(scope, "localName").into(),
        v8_str(scope, name).into(),
    );
    obj.set(
        scope,
        v8_str(scope, "value").into(),
        v8_str(scope, value).into(),
    );
    obj.set(
        scope,
        v8_str(scope, "nodeValue").into(),
        v8_str(scope, value).into(),
    );
    obj
}

fn attr_entries(node_data: &NodeData) -> Vec<(String, String)> {
    match &*node_data.node.borrow() {
        Node::Element(el) => el
            .attributes
            .iter()
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect(),
        _ => Vec::new(),
    }
}

fn install_map_method<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    obj: v8::Local<'s, v8::Object>,
    name: &str,
    callback: impl v8::MapFnTo<v8::FunctionCallback>,
    data: v8::Local<'s, v8::External>,
) {
    let function = v8::FunctionTemplate::builder(callback)
        .data(data.into())
        .build(scope)
        .get_function(scope)
        .unwrap();
    obj.set(scope, v8_str(scope, name).into(), function.into());
}

fn build_named_node_map<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    node_external: v8::Local<'s, v8::External>,
    node_data: &NodeData,
) -> v8::Local<'s, v8::Object> {
    let obj = v8::Object::new(scope);
    let entries = attr_entries(node_data);
    obj.set(
        scope,
        v8_str(scope, "length").into(),
        v8::Integer::new(scope, entries.len() as i32).into(),
    );
    for (idx, (name, value)) in entries.iter().enumerate() {
        let attr = build_attr_object(scope, name, value);
        obj.set_index(scope, idx as u32, attr.into());
    }
    install_map_method(scope, obj, "item", named_node_map_item, node_external);
    install_map_method(
        scope,
        obj,
        "getNamedItem",
        named_node_map_get_named_item,
        node_external,
    );
    install_map_method(
        scope,
        obj,
        "setNamedItem",
        named_node_map_set_named_item,
        node_external,
    );
    install_map_method(
        scope,
        obj,
        "removeNamedItem",
        named_node_map_remove_named_item,
        node_external,
    );
    obj
}

fn get_attributes<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    _name: v8::Local<'s, v8::Name>,
    args: v8::PropertyCallbackArguments<'s>,
    mut retval: v8::ReturnValue,
) {
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };
    let attrs = build_named_node_map(scope, external, node_data);
    retval.set(attrs.into());
}

fn named_node_map_item(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let idx = args.get(0).uint32_value(scope).unwrap_or(0) as usize;
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };
    match attr_entries(node_data).into_iter().nth(idx) {
        Some((name, value)) => retval.set(build_attr_object(scope, &name, &value).into()),
        None => retval.set(v8::null(scope).into()),
    }
}

fn named_node_map_get_named_item(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let name = args.get(0).to_rust_string_lossy(scope);
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };
    if let Node::Element(el) = &*node_data.node.borrow() {
        if let Some(value) = el.attributes.get(&name) {
            retval.set(build_attr_object(scope, &name, value).into());
            return;
        }
    }
    retval.set(v8::null(scope).into());
}

fn named_node_map_set_named_item(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let attr = args.get(0);
    if !attr.is_object() {
        retval.set(v8::null(scope).into());
        return;
    }
    let attr = attr.to_object(scope).unwrap();
    let name = attr
        .get(scope, v8_str(scope, "name").into())
        .map(|v| v.to_rust_string_lossy(scope))
        .unwrap_or_default();
    let value = attr
        .get(scope, v8_str(scope, "value").into())
        .map(|v| v.to_rust_string_lossy(scope))
        .unwrap_or_default();
    if name.is_empty() {
        retval.set(v8::null(scope).into());
        return;
    }

    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };
    let old = if let Node::Element(el) = &mut *node_data.node.borrow_mut() {
        let old = el.attributes.insert(name.clone(), value);
        node_data.registry.mark_style_dirty(&node_data.node);
        old
    } else {
        None
    };
    match old {
        Some(old) => retval.set(build_attr_object(scope, &name, &old).into()),
        None => retval.set(v8::null(scope).into()),
    }
}

fn named_node_map_remove_named_item(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let name = args.get(0).to_rust_string_lossy(scope);
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };
    let old = if let Node::Element(el) = &mut *node_data.node.borrow_mut() {
        let old = el.attributes.remove(&name);
        if old.is_some() {
            node_data.registry.mark_style_dirty(&node_data.node);
        }
        old
    } else {
        None
    };
    match old {
        Some(old) => retval.set(build_attr_object(scope, &name, &old).into()),
        None => retval.set(v8::null(scope).into()),
    }
}

// ─── Accessors ───────────────────────────────────────────────────────────────

fn get_attr_value(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::PropertyCallbackArguments,
    attr_name: &str,
    mut retval: v8::ReturnValue,
) {
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };
    let value = match &*node_data.node.borrow() {
        Node::Element(el) => el.attributes.get(attr_name).cloned().unwrap_or_default(),
        _ => String::new(),
    };
    retval.set(v8_str(scope, &value).into());
}

fn set_attr_value(
    scope: &mut v8::PinScope<'_, '_>,
    value: v8::Local<v8::Value>,
    args: v8::PropertyCallbackArguments,
    attr_name: &str,
) {
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };
    let value = value.to_rust_string_lossy(scope);
    if let Node::Element(el) = &mut *node_data.node.borrow_mut() {
        if value.is_empty() {
            el.attributes.remove(attr_name);
        } else {
            el.attributes.insert(attr_name.to_string(), value);
        }
        node_data.registry.mark_style_dirty(&node_data.node);
    }
}

fn get_id(
    scope: &mut v8::PinScope<'_, '_>,
    _name: v8::Local<v8::Name>,
    args: v8::PropertyCallbackArguments,
    retval: v8::ReturnValue,
) {
    get_attr_value(scope, args, "id", retval);
}

fn set_id(
    scope: &mut v8::PinScope<'_, '_>,
    _name: v8::Local<v8::Name>,
    value: v8::Local<v8::Value>,
    args: v8::PropertyCallbackArguments,
    _retval: v8::ReturnValue<()>,
) {
    set_attr_value(scope, value, args, "id");
}

fn get_class_name(
    scope: &mut v8::PinScope<'_, '_>,
    _name: v8::Local<v8::Name>,
    args: v8::PropertyCallbackArguments,
    retval: v8::ReturnValue,
) {
    get_attr_value(scope, args, "class", retval);
}

fn set_class_name(
    scope: &mut v8::PinScope<'_, '_>,
    _name: v8::Local<v8::Name>,
    value: v8::Local<v8::Value>,
    args: v8::PropertyCallbackArguments,
    _retval: v8::ReturnValue<()>,
) {
    set_attr_value(scope, value, args, "class");
}

fn get_parent_node(
    scope: &mut v8::PinScope<'_, '_>,
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
    scope: &mut v8::PinScope<'_, '_>,
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
    scope: &mut v8::PinScope<'_, '_>,
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
    scope: &mut v8::PinScope<'_, '_>,
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
    scope: &mut v8::PinScope<'_, '_>,
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
    scope: &mut v8::PinScope<'_, '_>,
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
    scope: &mut v8::PinScope<'_, '_>,
    _name: v8::Local<v8::Name>,
    value: v8::Local<v8::Value>,
    args: v8::PropertyCallbackArguments,
    _retval: v8::ReturnValue<()>,
) {
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    let text = value.to_rust_string_lossy(scope);
    mutation::set_text_content(&node_data.node, &text);
    node_data.registry.mark_layout_dirty(&node_data.node);
}

/// Per spec, `template.innerHTML` reads and writes the template's content
/// fragment, not its light children. Everything else targets the node itself.
fn inner_html_target(node: &NodePtr) -> NodePtr {
    let template_content = match &mut *node.borrow_mut() {
        Node::Element(el) if el.tag_name.eq_ignore_ascii_case("template") => Some(
            el.template_contents
                .get_or_insert_with(|| Node::document_fragment(Vec::new()))
                .clone(),
        ),
        _ => None,
    };
    template_content.unwrap_or_else(|| node.clone())
}

fn get_inner_html(
    scope: &mut v8::PinScope<'_, '_>,
    _name: v8::Local<v8::Name>,
    args: v8::PropertyCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    let target = inner_html_target(&node_data.node);
    let html: String = child_nodes_of(&target)
        .iter()
        .map(crate::dom::serialize_outer_html)
        .collect();
    retval.set(v8_str(scope, &html).into());
}

fn set_inner_html(
    scope: &mut v8::PinScope<'_, '_>,
    _name: v8::Local<v8::Name>,
    value: v8::Local<v8::Value>,
    args: v8::PropertyCallbackArguments,
    _retval: v8::ReturnValue<()>,
) {
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    let html = value.to_rust_string_lossy(scope);
    let parsed = crate::html::Parser::new(&html).parse_document();
    let new_children: Vec<NodePtr> = match &*parsed.borrow() {
        Node::Document { children, .. } => children.clone(),
        _ => Vec::new(),
    };
    let target = inner_html_target(&node_data.node);
    if let Node::Element(el) = &mut *target.borrow_mut() {
        el.children = new_children;
    }
    node_data.registry.mark_layout_dirty(&node_data.node);
}

fn get_outer_html(
    scope: &mut v8::PinScope<'_, '_>,
    _name: v8::Local<v8::Name>,
    args: v8::PropertyCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    let html = crate::dom::serialize_outer_html(&node_data.node);
    retval.set(v8_str(scope, &html).into());
}

fn get_style(
    scope: &mut v8::PinScope<'_, '_>,
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
    scope: &mut v8::PinScope<'_, '_>,
    _name: v8::Local<v8::Name>,
    args: v8::PropertyCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_external = v8::Local::<v8::External>::new(scope, external);

    let obj = classlist::build_classlist_object(scope, node_external);
    retval.set(obj.into());
}

// ─── Tree collections / events / shadow DOM ─────────────────────────────────

fn child_nodes_of(node: &NodePtr) -> Vec<NodePtr> {
    match &*node.borrow() {
        Node::Element(el) => el.children.clone(),
        Node::Document { children, .. } => children.clone(),
        _ => Vec::new(),
    }
}

fn nodes_to_array<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    nodes: Vec<NodePtr>,
    node_data: &NodeData,
) -> v8::Local<'s, v8::Array> {
    let array = v8::Array::new(scope, nodes.len() as i32);
    for (i, node) in nodes.into_iter().enumerate() {
        let js_node = create_js_node(scope, node, &node_data.registry, &node_data.document);
        array.set_index(scope, i as u32, js_node.into());
    }
    array
}

fn get_child_nodes(
    scope: &mut v8::PinScope<'_, '_>,
    _name: v8::Local<v8::Name>,
    args: v8::PropertyCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };
    let nodes = child_nodes_of(&node_data.node);
    let array = nodes_to_array(scope, nodes, node_data);
    retval.set(array.into());
}

fn get_children(
    scope: &mut v8::PinScope<'_, '_>,
    _name: v8::Local<v8::Name>,
    args: v8::PropertyCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };
    let nodes: Vec<NodePtr> = child_nodes_of(&node_data.node)
        .into_iter()
        .filter(|c| node_type(c) == 1)
        .collect();
    let array = nodes_to_array(scope, nodes, node_data);
    retval.set(array.into());
}

fn get_first_element_child(
    scope: &mut v8::PinScope<'_, '_>,
    _name: v8::Local<v8::Name>,
    args: v8::PropertyCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };
    let first = child_nodes_of(&node_data.node)
        .into_iter()
        .find(|c| node_type(c) == 1);
    match first {
        Some(node) => {
            let js_node =
                create_js_node(scope, node, &node_data.registry, &node_data.document);
            retval.set(js_node.into());
        }
        None => retval.set(v8::null(scope).into()),
    }
}

fn get_owner_document(
    scope: &mut v8::PinScope<'_, '_>,
    _name: v8::Local<v8::Name>,
    _args: v8::PropertyCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    // The document wrapper with factory methods lives on the global; the raw
    // document NodePtr wrapper wouldn't have createElement and friends.
    let context = scope.get_current_context();
    let global = context.global(scope);
    let key = v8_str(scope, "document");
    match global.get(scope, key.into()) {
        Some(doc) => retval.set(doc),
        None => retval.set(v8::null(scope).into()),
    }
}

fn node_add_event_listener(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut _retval: v8::ReturnValue,
) {
    let event_type = args.get(0).to_rust_string_lossy(scope);
    let callback = args.get(1);
    let Ok(callback) = v8::Local::<v8::Function>::try_from(callback) else {
        return;
    };
    let callback_global = v8::Global::new(scope, callback);

    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };
    let node_id = node_data.registry.register(node_data.node.clone());
    node_data
        .registry
        .add_event_listener(node_id, event_type, callback_global);
}

fn node_remove_event_listener(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut _retval: v8::ReturnValue,
) {
    let event_type = args.get(0).to_rust_string_lossy(scope);
    let Ok(callback) = v8::Local::<v8::Function>::try_from(args.get(1)) else {
        return;
    };

    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };
    let node_id = node_data.registry.register(node_data.node.clone());
    node_data
        .registry
        .remove_event_listener(scope, node_id, &event_type, callback);
}

fn node_dispatch_event(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let event = args.get(0);
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };
    let node_id = node_data.registry.register(node_data.node.clone());

    let mut event_type = String::new();
    if let Some(event_obj) = event.to_object(scope) {
        if let Some(t) = event_obj.get(scope, v8_str(scope, "type").into()) {
            event_type = t.to_rust_string_lossy(scope);
        }
        let this = args.this();
        event_obj.set(scope, v8_str(scope, "target").into(), this.into());
        event_obj.set(scope, v8_str(scope, "currentTarget").into(), this.into());
    }

    let listeners = node_data.registry.get_listeners(node_id, &event_type);
    let this = args.this();
    for listener in listeners {
        let callback = v8::Local::new(scope, listener);
        let _ = callback.call(scope, this.into(), &[event]);
    }
    retval.set(v8::Boolean::new(scope, true).into());
}

fn get_bounding_client_rect(
    scope: &mut v8::PinScope<'_, '_>,
    _args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let obj = v8::Object::new(scope);
    let zero = v8::Number::new(scope, 0.0);
    for key in ["top", "left", "right", "bottom", "width", "height", "x", "y"] {
        obj.set(scope, v8_str(scope, key).into(), zero.into());
    }
    retval.set(obj.into());
}

fn get_client_rects(
    scope: &mut v8::PinScope<'_, '_>,
    _args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    retval.set(v8::Array::new(scope, 0).into());
}

fn get_root_node(
    _scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    retval.set(args.this().into());
}

fn append_children(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut _retval: v8::ReturnValue,
) {
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };
    for i in 0..args.length() {
        let arg = args.get(i);
        let child = if let Some(node) = node_from_js(scope, arg, &node_data.registry) {
            node
        } else {
            Node::text(arg.to_rust_string_lossy(scope))
        };
        mutation::append_child_ptr(&node_data.node, &child);
    }
    node_data.registry.mark_layout_dirty(&node_data.node);
}

fn get_elements_by_class_name(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let class = args.get(0).to_rust_string_lossy(scope);
    let selector = format!(
        ".{}",
        class.split_whitespace().collect::<Vec<_>>().join(".")
    );
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };
    let found = query::query_all(&node_data.document, &selector, &node_data.node);
    let array = nodes_to_array(scope, found, node_data);
    retval.set(array.into());
}

fn attach_shadow(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    // Rebind to the callback scope's lifetime so it can seed new templates.
    let external = v8::Local::<v8::External>::new(scope, external);
    let node_data = unsafe { &*(external.value() as *const NodeData) };
    let node_id = node_data.registry.register(node_data.node.clone());

    let mode = if args.get(0).is_object() {
        let opts = args.get(0).to_object(scope).unwrap();
        opts.get(scope, v8_str(scope, "mode").into())
            .filter(|v| v.is_string())
            .map(|v| v.to_rust_string_lossy(scope))
            .unwrap_or_else(|| "open".to_string())
    } else {
        "open".to_string()
    };

    // The "shadow root" proxies the host's own subtree: same NodeData, same
    // node id, nodeType 11 — Polymer stamps into it, the host renders it.
    let template = v8::ObjectTemplate::new(scope);
    install_method(scope, template, "appendChild", append_child, external);
    install_method(scope, template, "insertBefore", insert_before, external);
    install_method(scope, template, "removeChild", remove_child, external);
    install_method(scope, template, "append", append_children, external);
    install_method(scope, template, "querySelector", query_selector, external);
    install_method(
        scope,
        template,
        "querySelectorAll",
        query_selector_all,
        external,
    );
    install_method(
        scope,
        template,
        "getElementsByTagName",
        get_elements_by_tag_name,
        external,
    );
    install_method(
        scope,
        template,
        "getElementsByClassName",
        get_elements_by_class_name,
        external,
    );
    install_method(
        scope,
        template,
        "addEventListener",
        node_add_event_listener,
        external,
    );
    install_method(
        scope,
        template,
        "removeEventListener",
        node_remove_event_listener,
        external,
    );
    install_method(scope, template, "contains", contains, external);
    install_method(scope, template, "getRootNode", get_root_node, external);
    install_accessor(
        scope,
        template,
        "innerHTML",
        get_inner_html,
        set_inner_html,
        external,
    );
    install_accessor(
        scope,
        template,
        "textContent",
        get_text_content,
        set_text_content,
        external,
    );
    install_readonly_accessor(scope, template, "firstChild", get_first_child, external);
    install_readonly_accessor(scope, template, "children", get_children, external);
    install_readonly_accessor(scope, template, "childNodes", get_child_nodes, external);

    let sr = template.new_instance(scope).unwrap();
    sr.set(
        scope,
        v8_str(scope, "__aurora_node_id").into(),
        v8::Integer::new(scope, node_id as i32).into(),
    );
    sr.set(
        scope,
        v8_str(scope, "nodeType").into(),
        v8::Integer::new(scope, 11).into(),
    );
    sr.set(
        scope,
        v8_str(scope, "nodeName").into(),
        v8_str(scope, "#document-fragment").into(),
    );
    sr.set(scope, v8_str(scope, "mode").into(), v8_str(scope, &mode).into());
    sr.set(
        scope,
        v8_str(scope, "delegatesFocus").into(),
        v8::Boolean::new(scope, false).into(),
    );
    // ShadyDOM hybrid callers do `root.shadowRoot.appendChild` — self-ref so
    // both paths land on this same host-proxying object.
    sr.set(scope, v8_str(scope, "shadowRoot").into(), sr.into());

    let host = args.this();
    sr.set(scope, v8_str(scope, "host").into(), host.into());
    // Upgraded elements inherit ShadyDOM's getter-only `__shady_shadowRoot`
    // accessor via the patched constructor-stub prototypes, which makes plain
    // [[Set]] a silent no-op. Define own data properties to bypass it.
    let shadow_root_key = v8_str(scope, "shadowRoot");
    host.create_data_property(scope, shadow_root_key.into(), sr.into());
    let shady_shadow_root_key = v8_str(scope, "__shady_shadowRoot");
    host.create_data_property(scope, shady_shadow_root_key.into(), sr.into());

    retval.set(sr.into());
}
