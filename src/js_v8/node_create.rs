use super::registry::NodeRegistry;
use super::selectors::query;
use super::style_class::{classlist, style};
use super::tree::{mutation, navigation};
use crate::dom::{Node, NodePtr, ShadowTreeBackend};
use std::rc::Rc;

pub(super) struct NodeData {
    pub node: NodePtr,
    #[allow(dead_code)]
    pub blitz_node_id: Option<usize>,
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
    let blitz_node_id = registry.blitz_node_id(&node);
    if let Some(blitz_id) = blitz_node_id {
        template.set(
            v8_str(scope, "__aurora_blitz_node_id").into(),
            v8::Integer::new(scope, blitz_id as i32).into(),
        );
    }

    let node_data = Box::into_raw(Box::new(NodeData {
        node: node.clone(),
        blitz_node_id,
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
    install_method(
        scope,
        template,
        "hasChildNodes",
        has_child_nodes,
        node_external,
    );
    install_method(scope, template, "prepend", prepend_children, node_external);
    install_method(scope, template, "before", insert_before_self, node_external);
    install_method(scope, template, "after", insert_after_self, node_external);

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
        "getAttributeNS",
        get_attribute_ns,
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
        "setAttributeNS",
        set_attribute_ns,
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
        "removeAttributeNS",
        remove_attribute_ns,
        node_external,
    );
    install_method(
        scope,
        template,
        "hasAttribute",
        has_attribute,
        node_external,
    );
    install_method(
        scope,
        template,
        "hasAttributeNS",
        has_attribute_ns,
        node_external,
    );
    install_method(
        scope,
        template,
        "hasAttributes",
        has_attributes,
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
        "parentElement",
        get_parent_element,
        node_external,
    );
    // Only registered engine shadow roots get the native own `host` accessor.
    // Installing it on every DocumentFragment shadows ShadyDOM's prototype
    // getter/constructor assignment and erases the logical host relationship it
    // is trying to expose.
    if crate::dom::SyntheticShadowTreeBackend.is_shadow_root(&node) {
        install_readonly_accessor(scope, template, "host", get_shadow_host, node_external);
    }
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
        "nextElementSibling",
        get_next_element_sibling,
        node_external,
    );
    install_readonly_accessor(
        scope,
        template,
        "previousElementSibling",
        get_previous_element_sibling,
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

    // classList
    install_readonly_accessor(scope, template, "classList", get_classlist, node_external);
    install_readonly_accessor(scope, template, "attributes", get_attributes, node_external);

    // Tree collections
    install_readonly_accessor(
        scope,
        template,
        "childNodes",
        get_child_nodes,
        node_external,
    );
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
        "lastElementChild",
        get_last_element_child,
        node_external,
    );
    install_readonly_accessor(
        scope,
        template,
        "childElementCount",
        get_child_element_count,
        node_external,
    );
    install_readonly_accessor(
        scope,
        template,
        "isConnected",
        get_is_connected,
        node_external,
    );
    install_readonly_accessor(
        scope,
        template,
        "ownerDocument",
        get_owner_document,
        node_external,
    );

    // Events: addEventListener/removeEventListener/dispatchEvent come from the
    // JS `EventTarget` prototype (installed below via the prototype chain), so
    // node listeners share one model with `window`/`document` and bubbling works.

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
    // Native bridge to Blitz/Stylo layout (Phase 8.2). v8_post.js's metric
    // getters call this so `offsetWidth`/`clientWidth`/etc. report the real
    // rendered box size, falling back to their heuristic only when unlaid out.
    install_method(
        scope,
        template,
        "__aurora_metric__",
        get_layout_metric,
        node_external,
    );
    install_method(scope, template, "getRootNode", get_root_node, node_external);
    install_method(scope, template, "append", append_children, node_external);
    install_method(
        scope,
        template,
        "replaceChildren",
        replace_children,
        node_external,
    );
    install_method(scope, template, "replaceWith", replace_with, node_external);
    install_method(
        scope,
        template,
        "insertAdjacentHTML",
        insert_adjacent_html,
        node_external,
    );
    install_method(
        scope,
        template,
        "insertAdjacentElement",
        insert_adjacent_element,
        node_external,
    );
    install_method(
        scope,
        template,
        "insertAdjacentText",
        insert_adjacent_text,
        node_external,
    );
    install_method(scope, template, "normalize", normalize_node, node_external);
    install_method(
        scope,
        template,
        "getElementsByClassName",
        get_elements_by_class_name,
        node_external,
    );

    // Shadow DOM — attachShadow creates a distinct ShadowRoot fragment. Blitz
    // currently paints it through a native shadow container while JS keeps the
    // root separate from the host's light children.
    install_method(
        scope,
        template,
        "attachShadow",
        attach_shadow,
        node_external,
    );
    install_method(
        scope,
        template,
        "__shady_attachShadow",
        attach_shadow,
        node_external,
    );
    install_method(
        scope,
        template,
        "__aurora_adoptShadowRoot",
        adopt_shadow_root,
        node_external,
    );

    if let Node::Element(el) = &*node.borrow() {
        if el.tag_name.eq_ignore_ascii_case("slot") {
            install_method(
                scope,
                template,
                "assignedNodes",
                assigned_nodes,
                node_external,
            );
        }
    }

    let obj = template.new_instance(scope).unwrap();

    obj.set(
        scope,
        v8_str(scope, "__aurora_node_id").into(),
        v8::Integer::new(scope, node_id as i32).into(),
    );
    if node_type(&node) == 1 {
        let node_data = unsafe { &*(node_external.value() as *const NodeData) };
        let style_obj = style::build_style_object(scope, node_data);
        obj.set(scope, v8_str(scope, "style").into(), style_obj.into());
    }

    // Give the wrapper the DOM prototype chain so it inherits real `EventTarget`
    // methods (Node/Element/HTMLElement → EventTarget). Custom-element upgrade may
    // later swap this for the element's class prototype, which itself chains back
    // to HTMLElement.prototype, so EventTarget stays reachable either way.
    let proto_name = match &*node.borrow() {
        Node::Element(el) if el.tag_name != "#document-fragment" => {
            if matches!(
                el.tag_name.as_str(),
                "svg"
                    | "path"
                    | "g"
                    | "circle"
                    | "rect"
                    | "line"
                    | "polyline"
                    | "polygon"
                    | "ellipse"
                    | "text"
                    | "use"
                    | "defs"
                    | "symbol"
                    | "clipPath"
                    | "linearGradient"
                    | "radialGradient"
                    | "stop"
                    | "mask"
                    | "pattern"
                    | "marker"
                    | "foreignObject"
                    | "feGaussianBlur"
                    | "feOffset"
                    | "feColorMatrix"
            ) {
                "SVGElement"
            } else {
                "HTMLElement"
            }
        }
        Node::Element(_) => "DocumentFragment",
        _ => "Node",
    };
    set_dom_prototype(scope, obj, proto_name);

    let mut is_custom_element = false;
    let mut is_canvas = false;
    let mut is_media = false;
    let mut is_iframe = false;
    let mut template_content: Option<NodePtr> = None;
    if let Node::Element(el) = &*node.borrow() {
        is_canvas = el.tag_name.eq_ignore_ascii_case("canvas");
        is_media =
            el.tag_name.eq_ignore_ascii_case("video") || el.tag_name.eq_ignore_ascii_case("audio");
        is_iframe = el.tag_name.eq_ignore_ascii_case("iframe");
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
            if matches!(
                el.tag_name.as_str(),
                "svg"
                    | "path"
                    | "g"
                    | "circle"
                    | "rect"
                    | "line"
                    | "polyline"
                    | "polygon"
                    | "ellipse"
                    | "text"
                    | "use"
                    | "defs"
                    | "symbol"
                    | "clipPath"
                    | "linearGradient"
                    | "radialGradient"
                    | "stop"
                    | "mask"
                    | "pattern"
                    | "marker"
                    | "foreignObject"
                    | "feGaussianBlur"
                    | "feOffset"
                    | "feColorMatrix"
            ) {
                obj.set(
                    scope,
                    v8_str(scope, "namespaceURI").into(),
                    v8_str(scope, "http://www.w3.org/2000/svg").into(),
                );
            }
            // Initial attribute snapshot for wrapper props.
            for attr in ["href", "src", "type", "name", "value"] {
                let val = el.attributes.get(attr).cloned().unwrap_or_default();
                obj.set(
                    scope,
                    v8_str(scope, attr).into(),
                    v8_str(scope, &val).into(),
                );
            }
            let dataset = v8::Object::new(scope);
            for (name, value) in &el.attributes {
                if let Some(key) = data_attr_to_dataset_key(name) {
                    dataset.set(
                        scope,
                        v8_str(scope, &key).into(),
                        v8_str(scope, value).into(),
                    );
                }
            }
            obj.set(scope, v8_str(scope, "dataset").into(), dataset.into());
            obj.set(
                scope,
                v8_str(scope, "shadowRoot").into(),
                v8::null(scope).into(),
            );
            for key in [
                "offsetWidth",
                "offsetHeight",
                "offsetTop",
                "offsetLeft",
                "clientWidth",
                "clientHeight",
                "scrollWidth",
                "scrollHeight",
                "scrollTop",
                "scrollLeft",
            ] {
                obj.set(
                    scope,
                    v8_str(scope, key).into(),
                    v8::Number::new(scope, 0.0).into(),
                );
            }
            obj.set(
                scope,
                v8_str(scope, "tabIndex").into(),
                v8::Integer::new(scope, 0).into(),
            );
            obj.set(
                scope,
                v8_str(scope, "hidden").into(),
                v8::Boolean::new(scope, el.attributes.contains_key("hidden")).into(),
            );
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

    if is_iframe {
        let context = scope.get_current_context();
        let global = context.global(scope);
        if let Some(document_value) = global.get(scope, v8_str(scope, "document").into()) {
            obj.set(
                scope,
                v8_str(scope, "contentDocument").into(),
                document_value,
            );
        }
        obj.set(scope, v8_str(scope, "contentWindow").into(), global.into());
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
        // Script-created/template-parsed content fragments bypass
        // document.createDocumentFragment and cloneNode, so explicitly pass
        // them through the lifecycle owner tracker as well.
        call_global_hook(scope, "__aurora_track_fragment__", content_obj);
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
    if is_media {
        call_global_hook(scope, "__aurora_install_media_element__", obj);
    }
    // Let the custom-elements registry track/upgrade dash-named elements.
    if is_custom_element {
        call_global_hook(scope, "__aurora_track_custom_element__", obj);
    }

    obj
}

fn call_global_hook(scope: &mut v8::PinScope<'_, '_>, name: &str, arg: v8::Local<v8::Object>) {
    let context = scope.get_current_context();
    let global = context.global(scope);
    let key = v8_str(scope, name);
    if let Some(hook) = global.get(scope, key.into()) {
        if let Ok(hook) = v8::Local::<v8::Function>::try_from(hook) {
            let _ = hook.call(scope, global.into(), &[arg.into()]);
        }
    }
}

fn assigned_nodes(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    let backend = crate::dom::shadow::SyntheticShadowTreeBackend;
    if let Some(shadow_root) = backend.nearest_shadow_root(&node_data.node) {
        if let Some(host) = backend.host_for_shadow_root(&shadow_root) {
            backend.distribute_slots(&host);
        }
    }

    let nodes = backend.assigned_nodes(&node_data.node);
    let result_array = v8::Array::new(scope, nodes.len() as i32);
    for (i, node) in nodes.into_iter().enumerate() {
        let js_node = create_js_node(scope, node, &node_data.registry, &node_data.document);
        result_array.set_index(scope, i as u32, js_node.into());
    }
    retval.set(result_array.into());
}

fn v8_str<'s>(scope: &v8::PinScope<'s, '_, ()>, s: &str) -> v8::Local<'s, v8::String> {
    v8::String::new(scope, s).unwrap()
}

/// Set `obj`'s `[[Prototype]]` to `globalThis[<ctor>].prototype` if that DOM
/// constructor exists yet. Early wrappers built during context setup (before the
/// JS prototype skeletons are defined) simply keep `Object.prototype`; they're
/// re-resolvable later and the common case (wrappers created during script
/// execution) gets the chain.
fn set_dom_prototype(scope: &mut v8::PinScope<'_, '_>, obj: v8::Local<v8::Object>, ctor: &str) {
    let global = scope.get_current_context().global(scope);
    let Some(ctor_val) = global.get(scope, v8_str(scope, ctor).into()) else {
        return;
    };
    let Some(ctor_obj) = ctor_val.to_object(scope) else {
        return;
    };
    if let Some(proto) = ctor_obj.get(scope, v8_str(scope, "prototype").into()) {
        if proto.is_object() {
            let _ = obj.set_prototype(scope, proto);
        }
    }
}

fn data_attr_to_dataset_key(name: &str) -> Option<String> {
    let raw = name.strip_prefix("data-")?;
    if raw.is_empty() {
        return None;
    }
    let mut key = String::new();
    let mut uppercase_next = false;
    for ch in raw.chars() {
        if ch == '-' {
            uppercase_next = true;
        } else if uppercase_next {
            key.extend(ch.to_uppercase());
            uppercase_next = false;
        } else {
            key.push(ch);
        }
    }
    Some(key)
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
    if id_val.is_int32() {
        let id = id_val.int32_value(scope).unwrap() as u32;
        return registry.lookup(id);
    }
    let blitz_key = v8_str(scope, "__aurora_blitz_node_id");
    let blitz_id_val = obj.get(scope, blitz_key.into())?;
    if !blitz_id_val.is_int32() {
        return None;
    }
    let blitz_id = blitz_id_val.int32_value(scope).unwrap() as usize;
    registry.dom_node_for_blitz_id(blitz_id)
}

fn is_document_fragment(node: &NodePtr) -> bool {
    matches!(&*node.borrow(), Node::Element(el) if el.tag_name == "#document-fragment")
}

fn track_fragment_children(
    scope: &mut v8::PinScope<'_, '_>,
    registry: &Rc<NodeRegistry>,
    document: &NodePtr,
    children: &[NodePtr],
) {
    for child in children {
        if node_type(child) != 1 {
            continue;
        }
        let js_node = create_js_node(scope, child.clone(), registry, document);
        call_global_hook(scope, "__aurora_track_custom_element__", js_node);
    }
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

    let selector_root = crate::dom::SyntheticShadowTreeBackend
        .nearest_shadow_root(&node_data.node)
        .unwrap_or_else(|| node_data.document.clone());
    let found = query::query_first(&selector_root, &selector, &node_data.node);
    if let Some(found) = found {
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

    let selector_root = crate::dom::SyntheticShadowTreeBackend
        .nearest_shadow_root(&node_data.node)
        .unwrap_or_else(|| node_data.document.clone());
    let found = query::query_all(&selector_root, &selector, &node_data.node);
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

    let selector_root = crate::dom::SyntheticShadowTreeBackend
        .nearest_shadow_root(&node_data.node)
        .unwrap_or_else(|| node_data.document.clone());
    let matched = query::selector_matches(&node_data.node, &selector, &selector_root);
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

    let selector_root = crate::dom::SyntheticShadowTreeBackend
        .nearest_shadow_root(&node_data.node)
        .unwrap_or_else(|| node_data.document.clone());
    let mut current = Some(node_data.node.clone());
    while let Some(n) = current {
        if query::selector_matches(&n, &selector, &selector_root) {
            let js_node = create_js_node(scope, n, &node_data.registry, &node_data.document);
            retval.set(js_node.into());
            return;
        }
        current = find_parent_for(node_data, &n);
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
        let fragment_children = if is_document_fragment(&child) {
            child_nodes_of(&child)
        } else {
            Vec::new()
        };
        let _mutation_result = mutation::apply_dom_mutation(
            &node_data.registry,
            mutation::DomMutation::AppendChild {
                parent: &node_data.node,
                child: &child,
            },
        );
        if fragment_children.is_empty() {
            if let Ok(child_obj) = v8::Local::<v8::Object>::try_from(args.get(0)) {
                call_global_hook(scope, "__aurora_track_custom_element__", child_obj);
            }
        } else {
            track_fragment_children(
                scope,
                &node_data.registry,
                &node_data.document,
                &fragment_children,
            );
        }
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
        let _mutation_result = mutation::apply_dom_mutation(
            &node_data.registry,
            mutation::DomMutation::RemoveChild {
                parent: &node_data.node,
                child: &child,
            },
        );
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
        let fragment_children = if is_document_fragment(&new_c) {
            child_nodes_of(&new_c)
        } else {
            Vec::new()
        };
        let _mutation_result = mutation::apply_dom_mutation(
            &node_data.registry,
            mutation::DomMutation::InsertBefore {
                parent: &node_data.node,
                new_child: &new_c,
                ref_child: ref_child.as_ref(),
            },
        );
        if fragment_children.is_empty() {
            if let Ok(new_obj) = v8::Local::<v8::Object>::try_from(args.get(0)) {
                call_global_hook(scope, "__aurora_track_custom_element__", new_obj);
            }
        } else {
            track_fragment_children(
                scope,
                &node_data.registry,
                &node_data.document,
                &fragment_children,
            );
        }
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
        let fragment_children = if is_document_fragment(&new_c) {
            child_nodes_of(&new_c)
        } else {
            Vec::new()
        };
        let _mutation_result = mutation::apply_dom_mutation(
            &node_data.registry,
            mutation::DomMutation::ReplaceChild {
                parent: &node_data.node,
                new_child: &new_c,
                old_child: &old_c,
            },
        );
        if fragment_children.is_empty() {
            if let Ok(new_obj) = v8::Local::<v8::Object>::try_from(args.get(0)) {
                call_global_hook(scope, "__aurora_track_custom_element__", new_obj);
            }
        } else {
            track_fragment_children(
                scope,
                &node_data.registry,
                &node_data.document,
                &fragment_children,
            );
        }
        node_data.registry.mark_layout_dirty(&node_data.node);
        retval.set(args.get(1));
    } else {
        retval.set(v8::null(scope).into());
    }
}

fn remove_node(
    _scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut _retval: v8::ReturnValue,
) {
    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    if let Some(parent) = find_parent_for_node(node_data) {
        let _mutation_result = mutation::apply_dom_mutation(
            &node_data.registry,
            mutation::DomMutation::RemoveChild {
                parent: &parent,
                child: &node_data.node,
            },
        );
    }
}

fn find_parent_for(node_data: &NodeData, node: &NodePtr) -> Option<NodePtr> {
    // Per DOM semantics ShadowRoot.parentNode is null. More importantly, do not
    // feed the host back-pointer through `query::find_parent`: a shadow root is
    // intentionally absent from the host's light child list, so that helper
    // would treat the valid pointer as stale and clear it. Connectivity and the
    // `host` accessor use the retained back-pointer directly.
    if crate::dom::SyntheticShadowTreeBackend.is_shadow_root(node) {
        return None;
    }
    if let Some(parent) = crate::dom::parent_ptr(node) {
        if query::find_parent(&parent, node).is_some() {
            return Some(parent);
        }
    }
    if let Some(parent) = query::find_parent(&node_data.document, node) {
        return Some(parent);
    }

    for root in node_data.registry.registered_nodes() {
        if Rc::ptr_eq(&root, &node_data.document) || Rc::ptr_eq(&root, node) {
            continue;
        }
        if let Some(parent) = query::find_parent(&root, node) {
            return Some(parent);
        }
    }
    None
}

fn find_parent_for_node(node_data: &NodeData) -> Option<NodePtr> {
    find_parent_for(node_data, &node_data.node)
}

fn sibling_for_node(node_data: &NodeData, delta: i32, elements_only: bool) -> Option<NodePtr> {
    let parent =
        crate::dom::parent_ptr(&node_data.node).or_else(|| find_parent_for_node(node_data))?;
    let kids = child_nodes_of(&parent);
    let pos = kids
        .iter()
        .position(|candidate| Rc::ptr_eq(candidate, &node_data.node))?;
    let step = if delta > 0 { 1 } else { -1 };
    let mut remaining = delta.abs();
    let mut current = pos as i32;

    while remaining > 0 {
        current += step;
        if current < 0 || current >= kids.len() as i32 {
            return None;
        }
        let candidate = &kids[current as usize];
        if !elements_only || node_type(candidate) == 1 {
            remaining -= 1;
            if remaining == 0 {
                return Some(candidate.clone());
            }
        }
    }
    None
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
    let is_fragment = matches!(
        &*cloned.borrow(),
        Node::Element(el) if el.tag_name == "#document-fragment"
    );
    let js_node = create_js_node(scope, cloned, &node_data.registry, &node_data.document);
    if is_fragment {
        call_global_hook(scope, "__aurora_track_fragment__", js_node);
    }
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

fn has_child_nodes(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };
    retval.set(v8::Boolean::new(scope, !child_nodes_of(&node_data.node).is_empty()).into());
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

fn get_attribute_ns(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let name = args.get(1).to_rust_string_lossy(scope);
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

    let _mutation_result = mutation::apply_dom_mutation(
        &node_data.registry,
        mutation::DomMutation::SetAttribute {
            node: &node_data.node,
            name: &name,
            value: &value,
        },
    );
}

fn set_attribute_ns(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut _retval: v8::ReturnValue,
) {
    let name = args.get(1).to_rust_string_lossy(scope);
    let value = args.get(2).to_rust_string_lossy(scope);
    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    let _mutation_result = mutation::apply_dom_mutation(
        &node_data.registry,
        mutation::DomMutation::SetAttribute {
            node: &node_data.node,
            name: &name,
            value: &value,
        },
    );
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

    let _mutation_result = mutation::apply_dom_mutation(
        &node_data.registry,
        mutation::DomMutation::RemoveAttribute {
            node: &node_data.node,
            name: &name,
        },
    );
}

fn remove_attribute_ns(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut _retval: v8::ReturnValue,
) {
    let name = args.get(1).to_rust_string_lossy(scope);
    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    let _mutation_result = mutation::apply_dom_mutation(
        &node_data.registry,
        mutation::DomMutation::RemoveAttribute {
            node: &node_data.node,
            name: &name,
        },
    );
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

fn has_attribute_ns(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let name = args.get(1).to_rust_string_lossy(scope);
    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    if let Node::Element(el) = &*node_data.node.borrow() {
        retval.set(v8::Boolean::new(scope, el.attributes.contains_key(&name)).into());
    } else {
        retval.set(v8::Boolean::new(scope, false).into());
    }
}

fn has_attributes(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    let has_any = match &*node_data.node.borrow() {
        Node::Element(el) => !el.attributes.is_empty(),
        _ => false,
    };
    retval.set(v8::Boolean::new(scope, has_any).into());
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
    let old = match &*node_data.node.borrow() {
        Node::Element(el) => el.attributes.get(&name).cloned(),
        _ => None,
    };
    let _mutation_result = mutation::apply_dom_mutation(
        &node_data.registry,
        mutation::DomMutation::SetAttribute {
            node: &node_data.node,
            name: &name,
            value: &value,
        },
    );
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
    let old = match &*node_data.node.borrow() {
        Node::Element(el) => el.attributes.get(&name).cloned(),
        _ => None,
    };
    if old.is_some() {
        let _mutation_result = mutation::apply_dom_mutation(
            &node_data.registry,
            mutation::DomMutation::RemoveAttribute {
                node: &node_data.node,
                name: &name,
            },
        );
    }
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
    let _mutation_result = if value.is_empty() {
        mutation::apply_dom_mutation(
            &node_data.registry,
            mutation::DomMutation::RemoveAttribute {
                node: &node_data.node,
                name: attr_name,
            },
        )
    } else {
        mutation::apply_dom_mutation(
            &node_data.registry,
            mutation::DomMutation::SetAttribute {
                node: &node_data.node,
                name: attr_name,
                value: &value,
            },
        )
    };
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

    if let Some(parent) = find_parent_for_node(node_data) {
        let js_node = create_js_node(scope, parent, &node_data.registry, &node_data.document);
        retval.set(js_node.into());
    } else {
        retval.set(v8::null(scope).into());
    }
}

/// `ShadowRoot.host` — for a synthetic shadow-root fragment, the host element it is
/// attached to. Returns `undefined` for any node that is not a shadow root, so it is a
/// harmless absent property on normal nodes.
fn get_shadow_host(
    scope: &mut v8::PinScope<'_, '_>,
    _name: v8::Local<v8::Name>,
    args: v8::PropertyCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    use crate::dom::ShadowTreeBackend;
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    if let Some(host) = crate::dom::SyntheticShadowTreeBackend.host_for_shadow_root(&node_data.node)
    {
        let js_node = create_js_node(scope, host, &node_data.registry, &node_data.document);
        retval.set(js_node.into());
    } else {
        retval.set(v8::undefined(scope).into());
    }
}

fn get_parent_element(
    scope: &mut v8::PinScope<'_, '_>,
    _name: v8::Local<v8::Name>,
    args: v8::PropertyCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    if let Some(parent) = find_parent_for_node(node_data) {
        if node_type(&parent) == 1 {
            let js_node = create_js_node(scope, parent, &node_data.registry, &node_data.document);
            retval.set(js_node.into());
            return;
        }
    }
    retval.set(v8::null(scope).into());
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

    if let Some(sibling) = sibling_for_node(node_data, 1, false) {
        let js_node = create_js_node(scope, sibling, &node_data.registry, &node_data.document);
        retval.set(js_node.into());
    } else {
        retval.set(v8::null(scope).into());
    }
}

fn get_next_element_sibling(
    scope: &mut v8::PinScope<'_, '_>,
    _name: v8::Local<v8::Name>,
    args: v8::PropertyCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    if let Some(sibling) = sibling_for_node(node_data, 1, true) {
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

    if let Some(sibling) = sibling_for_node(node_data, -1, false) {
        let js_node = create_js_node(scope, sibling, &node_data.registry, &node_data.document);
        retval.set(js_node.into());
    } else {
        retval.set(v8::null(scope).into());
    }
}

fn get_previous_element_sibling(
    scope: &mut v8::PinScope<'_, '_>,
    _name: v8::Local<v8::Name>,
    args: v8::PropertyCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    if let Some(sibling) = sibling_for_node(node_data, -1, true) {
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
    let _mutation_result = mutation::apply_dom_mutation(
        &node_data.registry,
        mutation::DomMutation::SetTextContent {
            node: &node_data.node,
            text,
        },
    );
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
    let new_children = parsed_html_nodes(&html);
    let target = inner_html_target(&node_data.node);
    let _mutation_result = mutation::apply_dom_mutation(
        &node_data.registry,
        mutation::DomMutation::ReplaceChildren {
            node: &target,
            children: new_children,
        },
    );
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
            let js_node = create_js_node(scope, node, &node_data.registry, &node_data.document);
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

#[allow(dead_code)]
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

#[allow(dead_code)]
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

/// Build a DOMRect-shaped object from the node's Blitz/Stylo layout (Phase 8.2).
/// Falls back to an all-zero rect when the node isn't laid out (no render
/// document, or a collapsed/unmapped box), matching the previous stub behavior.
fn bounding_rect_object<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    node_data: &NodeData,
) -> v8::Local<'s, v8::Object> {
    let m = node_data
        .registry
        .layout_metrics(&node_data.node)
        .unwrap_or_default();
    let obj = v8::Object::new(scope);
    for (key, value) in [
        ("x", m.x),
        ("y", m.y),
        ("left", m.x),
        ("top", m.y),
        ("right", m.x + m.width),
        ("bottom", m.y + m.height),
        ("width", m.width),
        ("height", m.height),
    ] {
        let v = v8::Number::new(scope, value as f64);
        obj.set(scope, v8_str(scope, key).into(), v.into());
    }
    obj
}

fn get_bounding_client_rect(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };
    let obj = bounding_rect_object(scope, node_data);
    retval.set(obj.into());
}

fn get_client_rects(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };
    let metrics = node_data.registry.layout_metrics(&node_data.node);
    // A laid-out box exposes its single border-box rect; an unlaid-out node has
    // no client rects.
    match metrics {
        Some(m) if m.width > 0.0 || m.height > 0.0 => {
            let rect = bounding_rect_object(scope, node_data);
            let array = v8::Array::new(scope, 1);
            array.set_index(scope, 0, rect.into());
            retval.set(array.into());
        }
        _ => retval.set(v8::Array::new(scope, 0).into()),
    }
}

/// Native bridge used by v8_post.js metric getters: returns the requested box
/// metric from Blitz/Stylo layout, or 0 when the node is not laid out (so the
/// JS fallback heuristic can take over).
fn get_layout_metric(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };
    let name = args.get(0).to_rust_string_lossy(scope);
    let value = match node_data.registry.layout_metrics(&node_data.node) {
        Some(m) => match name.as_str() {
            "offsetWidth" | "clientWidth" | "scrollWidth" => m.width,
            "offsetHeight" | "clientHeight" | "scrollHeight" => m.height,
            "offsetLeft" | "x" | "left" => m.x,
            "offsetTop" | "y" | "top" => m.y,
            _ => 0.0,
        },
        None => 0.0,
    };
    retval.set(v8::Number::new(scope, value as f64).into());
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
        let fragment_children = if is_document_fragment(&child) {
            child_nodes_of(&child)
        } else {
            Vec::new()
        };
        let _mutation_result = mutation::apply_dom_mutation(
            &node_data.registry,
            mutation::DomMutation::AppendChild {
                parent: &node_data.node,
                child: &child,
            },
        );
        if fragment_children.is_empty() {
            if let Ok(child_obj) = v8::Local::<v8::Object>::try_from(arg) {
                call_global_hook(scope, "__aurora_track_custom_element__", child_obj);
            }
        } else {
            track_fragment_children(
                scope,
                &node_data.registry,
                &node_data.document,
                &fragment_children,
            );
        }
    }
    node_data.registry.mark_layout_dirty(&node_data.node);
}

fn replace_children(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut _retval: v8::ReturnValue,
) {
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };

    let mut children = Vec::new();
    for i in 0..args.length() {
        let arg = args.get(i);
        let child = if let Some(node) = node_from_js(scope, arg, &node_data.registry) {
            node
        } else {
            Node::text(arg.to_rust_string_lossy(scope))
        };
        children.push(child);
    }
    let _mutation_result = mutation::apply_dom_mutation(
        &node_data.registry,
        mutation::DomMutation::ReplaceChildren {
            node: &node_data.node,
            children,
        },
    );
    node_data.registry.mark_layout_dirty(&node_data.node);
}

fn prepend_children(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut _retval: v8::ReturnValue,
) {
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };
    for i in (0..args.length()).rev() {
        let arg = args.get(i);
        let child = if let Some(node) = node_from_js(scope, arg, &node_data.registry) {
            node
        } else {
            Node::text(arg.to_rust_string_lossy(scope))
        };
        let _mutation_result = mutation::apply_dom_mutation(
            &node_data.registry,
            mutation::DomMutation::PrependChild {
                parent: &node_data.node,
                child: &child,
            },
        );
    }
    node_data.registry.mark_layout_dirty(&node_data.node);
}

fn replace_with(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut _retval: v8::ReturnValue,
) {
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };
    let parent = find_parent_for_node(node_data);
    insert_relative_to_self(scope, args, false);
    if let Some(parent) = parent {
        let _mutation_result = mutation::apply_dom_mutation(
            &node_data.registry,
            mutation::DomMutation::RemoveChild {
                parent: &parent,
                child: &node_data.node,
            },
        );
        node_data.registry.mark_layout_dirty(&parent);
    }
}

fn insert_relative_to_self(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    after: bool,
) {
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };
    let Some(parent) = find_parent_for_node(node_data) else {
        return;
    };
    let reference = if after {
        sibling_for_node(node_data, 1, false)
    } else {
        Some(node_data.node.clone())
    };
    for i in 0..args.length() {
        let arg = args.get(i);
        let child = if let Some(node) = node_from_js(scope, arg, &node_data.registry) {
            node
        } else {
            Node::text(arg.to_rust_string_lossy(scope))
        };
        let _mutation_result = mutation::apply_dom_mutation(
            &node_data.registry,
            mutation::DomMutation::InsertBefore {
                parent: &parent,
                new_child: &child,
                ref_child: reference.as_ref(),
            },
        );
    }
    node_data.registry.mark_layout_dirty(&parent);
}

fn insert_before_self(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut _retval: v8::ReturnValue,
) {
    insert_relative_to_self(scope, args, false);
}

fn insert_after_self(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut _retval: v8::ReturnValue,
) {
    insert_relative_to_self(scope, args, true);
}

fn parsed_html_nodes(html: &str) -> Vec<NodePtr> {
    let parsed = crate::html::Parser::new(html).parse_document();
    let mut bodies = Vec::new();
    query::collect_by_tag(&parsed, "body", &mut bodies);
    if let Some(body) = bodies.first() {
        return child_nodes_of(body);
    }
    match &*parsed.borrow() {
        Node::Document { children, .. } => children.clone(),
        _ => Vec::new(),
    }
}

fn insert_nodes_at_position(
    scope: &mut v8::PinScope<'_, '_>,
    node_data: &NodeData,
    position: &str,
    nodes: Vec<NodePtr>,
) -> bool {
    match position.to_ascii_lowercase().as_str() {
        "beforebegin" => {
            let Some(parent) = find_parent_for_node(node_data) else {
                return false;
            };
            for node in nodes {
                let _mutation_result = mutation::apply_dom_mutation(
                    &node_data.registry,
                    mutation::DomMutation::InsertBefore {
                        parent: &parent,
                        new_child: &node,
                        ref_child: Some(&node_data.node),
                    },
                );
            }
            node_data.registry.mark_layout_dirty(&parent);
            true
        }
        "afterbegin" => {
            for node in nodes.into_iter().rev() {
                let _mutation_result = mutation::apply_dom_mutation(
                    &node_data.registry,
                    mutation::DomMutation::PrependChild {
                        parent: &node_data.node,
                        child: &node,
                    },
                );
            }
            node_data.registry.mark_layout_dirty(&node_data.node);
            true
        }
        "beforeend" => {
            for node in nodes {
                let _mutation_result = mutation::apply_dom_mutation(
                    &node_data.registry,
                    mutation::DomMutation::AppendChild {
                        parent: &node_data.node,
                        child: &node,
                    },
                );
            }
            node_data.registry.mark_layout_dirty(&node_data.node);
            true
        }
        "afterend" => {
            let Some(parent) = find_parent_for_node(node_data) else {
                return false;
            };
            let reference = sibling_for_node(node_data, 1, false);
            for node in nodes {
                let _mutation_result = mutation::apply_dom_mutation(
                    &node_data.registry,
                    mutation::DomMutation::InsertBefore {
                        parent: &parent,
                        new_child: &node,
                        ref_child: reference.as_ref(),
                    },
                );
            }
            node_data.registry.mark_layout_dirty(&parent);
            true
        }
        _ => {
            let _ = scope;
            false
        }
    }
}

fn insert_adjacent_html(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut _retval: v8::ReturnValue,
) {
    let position = args.get(0).to_rust_string_lossy(scope);
    let html = args.get(1).to_rust_string_lossy(scope);
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };
    insert_nodes_at_position(scope, node_data, &position, parsed_html_nodes(&html));
}

fn insert_adjacent_text(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut _retval: v8::ReturnValue,
) {
    let position = args.get(0).to_rust_string_lossy(scope);
    let text = args.get(1).to_rust_string_lossy(scope);
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };
    insert_nodes_at_position(scope, node_data, &position, vec![Node::text(text)]);
}

fn insert_adjacent_element(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let position = args.get(0).to_rust_string_lossy(scope);
    let element_value = args.get(1);
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };
    let Some(element) = node_from_js(scope, element_value, &node_data.registry) else {
        retval.set(v8::null(scope).into());
        return;
    };
    if insert_nodes_at_position(scope, node_data, &position, vec![element]) {
        retval.set(element_value);
    } else {
        retval.set(v8::null(scope).into());
    }
}

fn normalize_node(
    _scope: &mut v8::PinScope<'_, '_>,
    _args: v8::FunctionCallbackArguments,
    mut _retval: v8::ReturnValue,
) {
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
    let selector_root = crate::dom::SyntheticShadowTreeBackend
        .nearest_shadow_root(&node_data.node)
        .unwrap_or_else(|| node_data.document.clone());
    let found = query::query_all(&selector_root, &selector, &node_data.node);
    let array = nodes_to_array(scope, found, node_data);
    retval.set(array.into());
}

fn get_last_element_child(
    scope: &mut v8::PinScope<'_, '_>,
    _name: v8::Local<v8::Name>,
    args: v8::PropertyCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };
    let last = child_nodes_of(&node_data.node)
        .into_iter()
        .rev()
        .find(|c| node_type(c) == 1);
    match last {
        Some(node) => {
            let js_node = create_js_node(scope, node, &node_data.registry, &node_data.document);
            retval.set(js_node.into());
        }
        None => retval.set(v8::null(scope).into()),
    }
}

fn get_child_element_count(
    scope: &mut v8::PinScope<'_, '_>,
    _name: v8::Local<v8::Name>,
    args: v8::PropertyCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };
    let count = child_nodes_of(&node_data.node)
        .into_iter()
        .filter(|c| node_type(c) == 1)
        .count();
    retval.set(v8::Integer::new(scope, count as i32).into());
}

fn get_is_connected(
    scope: &mut v8::PinScope<'_, '_>,
    _name: v8::Local<v8::Name>,
    args: v8::PropertyCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };
    let connected = mutation::is_connected_to(&node_data.document, &node_data.node);
    retval.set(v8::Boolean::new(scope, connected).into());
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

    let mode = if args.get(0).is_object() {
        let opts = args.get(0).to_object(scope).unwrap();
        opts.get(scope, v8_str(scope, "mode").into())
            .filter(|v| v.is_string())
            .map(|v| v.to_rust_string_lossy(scope))
            .unwrap_or_else(|| "open".to_string())
    } else {
        "open".to_string()
    };

    let shadow_root = crate::dom::SyntheticShadowTreeBackend.attach_shadow(&node_data.node, &mode);
    let _mutation_result = mutation::apply_dom_mutation(
        &node_data.registry,
        mutation::DomMutation::AttachShadow {
            host: &node_data.node,
            shadow_root: &shadow_root,
            mode: &mode,
        },
    );

    let shadow_id = node_data.registry.register(shadow_root.clone());
    let sr = create_js_node(scope, shadow_root, &node_data.registry, &node_data.document);
    sr.set(
        scope,
        v8_str(scope, "__aurora_node_id").into(),
        v8::Integer::new(scope, shadow_id as i32).into(),
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
    sr.set(
        scope,
        v8_str(scope, "mode").into(),
        v8_str(scope, &mode).into(),
    );
    sr.set(
        scope,
        v8_str(scope, "delegatesFocus").into(),
        v8::Boolean::new(scope, false).into(),
    );
    sr.set(
        scope,
        v8_str(scope, "__aurora_registered_shadow_root__").into(),
        v8::Boolean::new(scope, true).into(),
    );
    // ShadyDOM hybrid callers do `root.shadowRoot.appendChild` — self-ref so
    // both paths land on the same ShadowRoot object.
    sr.set(scope, v8_str(scope, "shadowRoot").into(), sr.into());

    let host = args.this();
    sr.set(scope, v8_str(scope, "host").into(), host.into());
    // Upgraded elements inherit ShadyDOM's getter-only `__shady_shadowRoot`
    // accessor via the patched constructor-stub prototypes, which makes plain
    // [[Set]] a silent no-op. Define own data properties to bypass it.
    if mode == "open" {
        let shadow_root_key = v8_str(scope, "shadowRoot");
        host.create_data_property(scope, shadow_root_key.into(), sr.into());
    }
    let shady_shadow_root_key = v8_str(scope, "__shady_shadowRoot");
    host.create_data_property(scope, shady_shadow_root_key.into(), sr.into());

    retval.set(sr.into());
}

/// Link a ShadyDOM-created logical `DocumentFragment` to its component host.
/// Unlike `attachShadow`, this adopts the exact fragment supplied by the
/// polyfill so the JS logical tree, connectivity walk, and Blitz mirror share
/// one root.
fn adopt_shadow_root(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let node_data = unsafe { &*(external.value() as *const NodeData) };
    let Some(shadow_root) = node_from_js(scope, args.get(0), &node_data.registry) else {
        retval.set(v8::Boolean::new(scope, false).into());
        return;
    };

    let adopted =
        crate::dom::SyntheticShadowTreeBackend.adopt_shadow_root(&node_data.node, &shadow_root);
    if adopted {
        let mode = args
            .get(1)
            .to_string(scope)
            .map(|value| value.to_rust_string_lossy(scope))
            .unwrap_or_else(|| "open".to_string());
        let result = mutation::apply_dom_mutation(
            &node_data.registry,
            mutation::DomMutation::AttachShadow {
                host: &node_data.node,
                shadow_root: &shadow_root,
                mode: &mode,
            },
        );
        retval.set(v8::Boolean::new(scope, result.render_synced).into());
    } else {
        retval.set(v8::Boolean::new(scope, false).into());
    }
}
