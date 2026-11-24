use super::custom_elements::CeReactionsGuard;
use super::form_associated;
use super::registry::NodeRegistry;
use super::selectors::query;
use super::templates::{self, NativeTag, TemplateKind};
use super::style_class::{classlist, style};
use super::tree::mutation;
use crate::dom::{Node, NodePtr, ShadowTreeBackend};
use std::rc::Rc;

pub(super) struct NodeData {
    pub node: NodePtr,
    #[allow(dead_code)]
    pub blitz_node_id: Option<blitz_dom::NodeId>,
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

    let blitz_node_id = registry.blitz_node_id(&node);
    // Never freed: wrappers live as long as the registry that maps them.
    let node_data: *const NodeData = Box::into_raw(Box::new(NodeData {
        node: node.clone(),
        blitz_node_id,
        registry: registry.clone(),
        document: document.clone(),
    }));

    let kind = if crate::dom::SyntheticShadowTreeBackend.is_shadow_root(&node) {
        TemplateKind::ShadowRoot
    } else if matches!(&*node.borrow(), Node::Element(el) if el.tag_name.eq_ignore_ascii_case("slot")) {
        TemplateKind::Slot
    } else {
        TemplateKind::Element
    };
    let template = templates::cached(scope, registry, kind, |scope, template| {
        build_wrapper_template(scope, template, kind)
    });
    let obj = templates::instantiate(scope, template, NativeTag::Node, node_data);
    if let Some(blitz_id) = blitz_node_id {
        obj.set(
            scope,
            v8_str(scope, "__aurora_blitz_node_id").into(),
            v8::BigInt::new_from_u64(scope, blitz_id.as_u64()).into(),
        );
    }

    obj.set(
        scope,
        v8_str(scope, "__aurora_node_id").into(),
        v8::Integer::new(scope, node_id as i32).into(),
    );
    if node_type(&node) == 1 {
        // SAFETY: allocated above and never freed.
        let style_obj = style::build_style_object(scope, unsafe { &*node_data });
        // Hidden slot, exposed as `style` by the HTMLElement/SVGElement
        // prototype accessor in v8_base.js. An own `style` data property would
        // shadow custom-element class accessors: YouTube's wrapped components
        // declare `style` as a prop on their prototype, and an own property
        // makes `setUpProps` capture the declaration object as a user-supplied
        // style prop, which later crashes their host render.
        let key = v8_str(scope, "__aurora_style__");
        obj.define_own_property(
            scope,
            key.into(),
            style_obj.into(),
            v8::PropertyAttribute::DONT_ENUM,
        );
    }

    // Give the wrapper the DOM prototype chain so it inherits real `EventTarget`
    // methods (Node/Element/HTMLElement → EventTarget). Custom-element upgrade may
    // later swap this for the element's class prototype, which itself chains back
    // to HTMLElement.prototype, so EventTarget stays reachable either way.
    let proto_name = interface_for(&node);
    super::bindings::set_prototype(scope, registry, obj, proto_name);

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
            // tagName, localName, namespaceURI and prefix are on Element.prototype.
            // Initial attribute snapshot for wrapper props. `<a>` and `<area>`
            // get `href` from HTMLHyperlinkElementUtils on their prototype.
            let hyperlink = matches!(el.tag_name.to_ascii_lowercase().as_str(), "a" | "area");
            // `<input>` gets `value` from HTMLInputElement.prototype.
            let input = el.tag_name.eq_ignore_ascii_case("input");
            // `<img>` gets `src` from HTMLImageElement.prototype.
            let image = el.tag_name.eq_ignore_ascii_case("img");
            for attr in ["href", "src", "type", "name", "value"] {
                if (hyperlink && attr == "href") || (input && attr == "value") || (image && attr == "src") {
                    continue;
                }
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
        // A template's own content is the pristine stamping source, never a
        // render root: flag it so the owner tracker / shadow adoption skip it
        // (otherwise the first host to be upgraded adopts it as its shadow
        // root and every stamp then pollutes the template itself).
        let flag_key = v8_str(scope, "__aurora_template_content__");
        let flag_val = v8::Boolean::new(scope, true);
        content_obj.create_data_property(scope, flag_key.into(), flag_val.into());
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

/// Inserting an *empty* DocumentFragment: give the JS custom-element shim a
/// chance to refill it with nodes its detached-stamp composition moved out
/// earlier, so they land where the fragment's creator actually inserts it.
pub(super) fn reclaim_composed_fragment(
    scope: &mut v8::PinScope<'_, '_>,
    arg: v8::Local<v8::Value>,
    node: &NodePtr,
) {
    if !is_document_fragment(node) || !child_nodes_of(node).is_empty() {
        return;
    }
    if let Ok(obj) = v8::Local::<v8::Object>::try_from(arg) {
        call_global_hook(scope, "__aurora_reclaim_composed_fragment__", obj);
    }
}

pub(super) fn call_global_hook(scope: &mut v8::PinScope<'_, '_>, name: &str, arg: v8::Local<v8::Object>) {
    let context = scope.get_current_context();
    let global = context.global(scope);
    let key = v8_str(scope, name);
    if let Some(hook) = global.get(scope, key.into()) {
        if let Ok(hook) = v8::Local::<v8::Function>::try_from(hook) {
            let _ = hook.call(scope, global.into(), &[arg.into()]);
        }
    }
}

/// Read the `flatten` boolean from an `assignedNodes`/`assignedElements`
/// options argument (`slot.assignedNodes({ flatten: true })`). Missing options
/// or a non-object argument mean `false`, per the default option value.
fn assigned_flatten_flag(
    scope: &mut v8::PinScope<'_, '_>,
    args: &v8::FunctionCallbackArguments,
) -> bool {
    let Ok(options) = v8::Local::<v8::Object>::try_from(args.get(0)) else {
        return false;
    };
    let key = v8_str(scope, "flatten");
    options
        .get(scope, key.into())
        .map(|value| value.boolean_value(scope))
        .unwrap_or(false)
}

/// The nodes assigned to a `<slot>`. With `flatten`, returns the flattened
/// assignment (assigned nodes, or the slot's fallback content — with nested
/// slots expanded — when nothing is assigned), which is exactly the slot's
/// composed children.
fn slot_assigned_nodes(node: &NodePtr, flatten: bool) -> Vec<NodePtr> {
    let backend = crate::dom::shadow::SyntheticShadowTreeBackend;
    if let Some(shadow_root) = backend.nearest_shadow_root(node) {
        if let Some(host) = backend.host_for_shadow_root(&shadow_root) {
            backend.distribute_slots(&host);
        }
    }
    if flatten {
        backend.composed_children(node)
    } else {
        backend.assigned_nodes(node)
    }
}

fn nodes_to_js_array<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    nodes: Vec<NodePtr>,
    node_data: &NodeData,
) -> v8::Local<'s, v8::Array> {
    let result_array = v8::Array::new(scope, nodes.len() as i32);
    for (i, node) in nodes.into_iter().enumerate() {
        let js_node = create_js_node(scope, node, &node_data.registry, &node_data.document);
        result_array.set_index(scope, i as u32, js_node.into());
    }
    result_array
}

fn assigned_nodes(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let Some(node_data) = this_node(scope, &args) else {
        return;
    };
    let flatten = assigned_flatten_flag(scope, &args);
    let nodes = slot_assigned_nodes(&node_data.node, flatten);
    retval.set(nodes_to_js_array(scope, nodes, node_data).into());
}

/// `HTMLSlotElement.assignedElements([options])` — like `assignedNodes` but
/// filtered to element nodes only (text and other non-element slottables drop).
fn assigned_elements(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let Some(node_data) = this_node(scope, &args) else {
        return;
    };
    let flatten = assigned_flatten_flag(scope, &args);
    let elements = slot_assigned_nodes(&node_data.node, flatten)
        .into_iter()
        .filter(|node| matches!(&*node.borrow(), Node::Element(_)))
        .collect();
    retval.set(nodes_to_js_array(scope, elements, node_data).into());
}

pub(super) fn v8_str<'s>(scope: &v8::PinScope<'s, '_, ()>, s: &str) -> v8::Local<'s, v8::String> {
    // `String::new` only returns `None` past V8's string-length limit or while
    // the isolate is terminating; our callers pass node names and attribute text.
    v8::String::new(scope, s).expect("v8 string allocation failed")
}

/// Recover the `&T` that a callback's data External points at.
///
/// Every callback's function/accessor template is built with an External over a
/// leaked `Box<T>` (a `NodeData`, `StyleData`, …), so the data slot is always
/// that External. The caller must request the same `T` the External was built
/// with.
///
/// # Safety
/// Sound as long as the data slot really is the External we installed for this
/// callback and the pointee `T` matches — which is an invariant of how the
/// templates are constructed here, `classlist.rs`, and `style.rs`.
/// Members every wrapper of `kind` gets as own properties. Built once per
/// runtime; the callbacks find their node through the receiver.
fn build_wrapper_template<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    template: v8::Local<'s, v8::ObjectTemplate>,
    kind: TemplateKind,
) {
    // --- Methods ---

    // Selector queries are on the ParentNode/Element/Document prototypes.

    // Mutation methods. Node's own (appendChild, insertBefore, removeChild,
    // replaceChild, cloneNode, hasChildNodes) are on Node.prototype.

    // Attribute methods are on Element.prototype (generated).

    // --- Accessors ---

    // Only registered engine shadow roots get the native own `host` accessor.
    // Installing it on every DocumentFragment shadows ShadyDOM's prototype
    // getter/constructor assignment and erases the logical host relationship it
    // is trying to expose.
    if kind == TemplateKind::ShadowRoot {
        install_readonly_accessor(scope, template, "host", get_shadow_host);
    }

    install_accessor(
        scope,
        template,
        "innerText",
        get_text_content,
        set_text_content,
    );
    install_readonly_accessor(scope, template, "outerHTML", get_outer_html);
    install_accessor(
        scope,
        template,
        "hidden",
        get_hidden,
        set_hidden,
    );

    // classList
    install_readonly_accessor(scope, template, "classList", get_classlist);
    install_readonly_accessor(scope, template, "attributes", get_attributes);


    // Events: addEventListener/removeEventListener/dispatchEvent come from the
    // JS `EventTarget` prototype (installed below via the prototype chain), so
    // node listeners share one model with `window`/`document` and bubbling works.

    // Geometry and misc
    install_method(
        scope,
        template,
        "getBoundingClientRect",
        get_bounding_client_rect,
    );
    install_method(
        scope,
        template,
        "getClientRects",
        get_client_rects,
    );
    // Native bridge to Blitz/Stylo layout (Phase 8.2). v8_post.js's metric
    // getters call this so `offsetWidth`/`clientWidth`/etc. report the real
    // rendered box size, falling back to their heuristic only when unlaid out.
    install_method(
        scope,
        template,
        "__aurora_metric__",
        get_layout_metric,
    );
    install_method(
        scope,
        template,
        "insertAdjacentHTML",
        insert_adjacent_html,
    );
    install_method(
        scope,
        template,
        "insertAdjacentElement",
        insert_adjacent_element,
    );
    install_method(
        scope,
        template,
        "insertAdjacentText",
        insert_adjacent_text,
    );

    // Shadow DOM — attachShadow creates a distinct ShadowRoot fragment. Blitz
    // currently paints it through a native shadow container while JS keeps the
    // root separate from the host's light children.
    install_method(
        scope,
        template,
        "attachShadow",
        attach_shadow,
    );
    install_method(
        scope,
        template,
        "__shady_attachShadow",
        attach_shadow,
    );
    install_method(
        scope,
        template,
        "attachInternals",
        attach_internals,
    );
    install_method(scope, template, "reset", form_reset);
    install_method(
        scope,
        template,
        "__aurora_adoptShadowRoot",
        adopt_shadow_root,
    );

    if kind == TemplateKind::Slot {
        install_method(
            scope,
            template,
            "assignedNodes",
            assigned_nodes,
        );
        install_method(
            scope,
            template,
            "assignedElements",
            assigned_elements,
        );
    }
}

pub(super) fn external_data<'a, T>(data: v8::Local<'a, v8::Value>) -> &'a T {
    let external = v8::Local::<v8::External>::try_from(data)
        .expect("callback data is always the External we installed");
    unsafe { &*(external.value() as *const T) }
}

/// The node behind a method's receiver (a wrapper, its `classList` or its
/// `attributes`); throws and returns `None` for any other receiver.
pub(super) fn this_node<'a>(
    scope: &mut v8::PinScope<'_, '_>,
    args: &v8::FunctionCallbackArguments,
) -> Option<&'a NodeData> {
    templates::receiver(scope, args.this(), NativeTag::Node)
}

/// The node behind a wrapper accessor's holder; throws if there is none.
fn holder_node<'a>(
    scope: &mut v8::PinScope<'_, '_>,
    args: &v8::PropertyCallbackArguments,
) -> Option<&'a NodeData> {
    templates::receiver(scope, args.holder(), NativeTag::Node)
}

/// Recover the `&NodeData` a per-node method callback was installed with.
pub(super) fn node_data_from<'a>(data: v8::Local<'a, v8::Value>) -> &'a NodeData {
    external_data(data)
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
) {
    let t = v8::FunctionTemplate::builder(callback).build(scope);
    template.set(v8_str(scope, name).into(), t.into());
}

fn install_accessor<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    template: v8::Local<v8::ObjectTemplate>,
    name: &str,
    getter: impl v8::MapFnTo<v8::AccessorNameGetterCallback>,
    setter: impl v8::MapFnTo<v8::AccessorNameSetterCallback>,
) {
    template.set_accessor_with_configuration(
        v8_str(scope, name).into(),
        v8::AccessorConfiguration::new(getter).setter(setter),
    );
}

fn install_readonly_accessor<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    template: v8::Local<v8::ObjectTemplate>,
    name: &str,
    getter: impl v8::MapFnTo<v8::AccessorNameGetterCallback>,
) {
    template.set_accessor_with_configuration(
        v8_str(scope, name).into(),
        v8::AccessorConfiguration::new(getter),
    );
}

/// The generated interface a wrapper for `node` is an instance of.
pub(super) fn interface_for(node: &NodePtr) -> &'static str {
    match &*node.borrow() {
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
                super::bindings::html_interface(&el.tag_name)
            }
        }
        Node::Element(_) if crate::dom::SyntheticShadowTreeBackend.is_shadow_root(node) => "ShadowRoot",
        Node::Element(_) => "DocumentFragment",
        Node::Text(_) => "Text",
        Node::Comment(_) => "Comment",
        Node::Document { .. } => "Document",
    }
}

pub(super) fn node_type(node: &NodePtr) -> i32 {
    match &*node.borrow() {
        Node::Element(el) if el.tag_name == "#document-fragment" => 11,
        Node::Element(_) => 1,
        Node::Text(_) => 3,
        Node::Comment(_) => 8,
        Node::Document { .. } => 9,
    }
}

pub(super) fn node_name(node: &NodePtr) -> String {
    match &*node.borrow() {
        Node::Element(el) if el.tag_name == "#document-fragment" => {
            "#document-fragment".to_string()
        }
        // The element's tagName: SVG names keep their case.
        Node::Element(el) if interface_for(node) == "SVGElement" => el.tag_name.clone(),
        Node::Element(el) => el.tag_name.to_uppercase(),
        Node::Text(_) => "#text".to_string(),
        Node::Comment(_) => "#comment".to_string(),
        Node::Document { .. } => "#document".to_string(),
    }
}

// ─── Callbacks ───────────────────────────────────────────────────────────────

pub(super) fn node_from_js(
    scope: &mut v8::PinScope<'_, '_>,
    val: v8::Local<v8::Value>,
    registry: &NodeRegistry,
) -> Option<NodePtr> {
    if !val.is_object() {
        return None;
    }
    let obj = val.to_object(scope)?;
    let key = v8_str(scope, "__aurora_node_id");
    let id_val = obj.get(scope, key.into())?;
    if id_val.is_int32() {
        let id = id_val.int32_value(scope)? as u32;
        return registry.lookup(id);
    }
    let blitz_key = v8_str(scope, "__aurora_blitz_node_id");
    let blitz_id_val = obj.get(scope, blitz_key.into())?;
    if !blitz_id_val.is_big_int() {
        return None;
    }
    let blitz_id = blitz_id_val.to_big_int(scope)?.u64_value().0;
    registry.dom_node_for_blitz_id(blitz_dom::NodeId::from_u64(blitz_id))
}

pub(super) fn is_document_fragment(node: &NodePtr) -> bool {
    matches!(&*node.borrow(), Node::Element(el) if el.tag_name == "#document-fragment")
}

pub(super) fn track_fragment_children(
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

fn find_parent_for(node_data: &NodeData, node: &NodePtr) -> Option<NodePtr> {
    parent_of(&node_data.registry, &node_data.document, node)
}

/// The node's parent, or `None` for roots and shadow roots.
pub(super) fn parent_of(registry: &NodeRegistry, document: &NodePtr, node: &NodePtr) -> Option<NodePtr> {
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
    if let Some(parent) = query::find_parent(document, node) {
        return Some(parent);
    }

    for root in registry.registered_nodes() {
        if Rc::ptr_eq(&root, document) || Rc::ptr_eq(&root, node) {
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
    sibling_of(&node_data.registry, &node_data.document, &node_data.node, delta, elements_only)
}

/// The sibling `delta` positions away (negative: before), optionally
/// counting only elements.
pub(super) fn sibling_of(
    registry: &NodeRegistry,
    document: &NodePtr,
    node: &NodePtr,
    delta: i32,
    elements_only: bool,
) -> Option<NodePtr> {
    let parent = crate::dom::parent_ptr(node).or_else(|| parent_of(registry, document, node))?;
    let kids = child_nodes_of(&parent);
    let pos = kids.iter().position(|candidate| Rc::ptr_eq(candidate, node))?;
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

fn build_named_node_map<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    node_data: &NodeData,
) -> v8::Local<'s, v8::Object> {
    let template = templates::cached(scope, &node_data.registry, TemplateKind::NamedNodeMap, |scope, template| {
        install_method(scope, template, "item", named_node_map_item);
        install_method(scope, template, "getNamedItem", named_node_map_get_named_item);
        install_method(scope, template, "setNamedItem", named_node_map_set_named_item);
        install_method(scope, template, "removeNamedItem", named_node_map_remove_named_item);
    });
    let obj = templates::instantiate(scope, template, NativeTag::Node, node_data as *const NodeData);
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
    // The bridge returns a fresh array-like NamedNodeMap snapshot on every
    // access. Web components commonly iterate `element.attributes` directly.
    // Put the array iterator on this instance so later polyfills cannot make
    // the native snapshot non-iterable by replacing NamedNodeMap's prototype.
    let global = scope.get_current_context().global(scope);
    if let Some(symbol_ctor) = global
        .get(scope, v8_str(scope, "Symbol").into())
        .and_then(|value| value.to_object(scope))
    {
        if let Some(iterator_key) = symbol_ctor
            .get(scope, v8_str(scope, "iterator").into())
            .and_then(|value| v8::Local::<v8::Symbol>::try_from(value).ok())
        {
            if let Some(array_ctor) = global
                .get(scope, v8_str(scope, "Array").into())
                .and_then(|value| value.to_object(scope))
            {
                if let Some(array_proto) = array_ctor
                    .get(scope, v8_str(scope, "prototype").into())
                    .and_then(|value| value.to_object(scope))
                {
                    if let Some(iterator) = array_proto.get(scope, iterator_key.into()) {
                        let _ = obj.set(scope, iterator_key.into(), iterator);
                    }
                }
            }
        }
    }
    obj
}

fn get_attributes<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    _name: v8::Local<'s, v8::Name>,
    args: v8::PropertyCallbackArguments<'s>,
    mut retval: v8::ReturnValue,
) {
    let Some(node_data) = holder_node(scope, &args) else {
        return;
    };
    let attrs = build_named_node_map(scope, node_data);
    retval.set(attrs.into());
}

fn named_node_map_item(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let idx = args.get(0).uint32_value(scope).unwrap_or(0) as usize;
    let Some(node_data) = this_node(scope, &args) else {
        return;
    };
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
    let Some(node_data) = this_node(scope, &args) else {
        return;
    };
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
    let Some(attr) = attr.to_object(scope) else {
        retval.set(v8::null(scope).into());
        return;
    };
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

    let Some(node_data) = this_node(scope, &args) else {
        return;
    };
    let _reactions = CeReactionsGuard::new(scope, &node_data.registry);
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
    let Some(node_data) = this_node(scope, &args) else {
        return;
    };
    let _reactions = CeReactionsGuard::new(scope, &node_data.registry);
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



fn get_hidden(
    scope: &mut v8::PinScope<'_, '_>,
    _name: v8::Local<v8::Name>,
    args: v8::PropertyCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let Some(node_data) = holder_node(scope, &args) else {
        return;
    };
    let hidden = matches!(&*node_data.node.borrow(), Node::Element(el) if el.attributes.contains_key("hidden"));
    retval.set(v8::Boolean::new(scope, hidden).into());
}

fn set_hidden(
    scope: &mut v8::PinScope<'_, '_>,
    _name: v8::Local<v8::Name>,
    value: v8::Local<v8::Value>,
    args: v8::PropertyCallbackArguments,
    _retval: v8::ReturnValue<()>,
) {
    let Some(node_data) = holder_node(scope, &args) else {
        return;
    };
    let _reactions = CeReactionsGuard::new(scope, &node_data.registry);
    let mutation = if value.boolean_value(scope) {
        mutation::DomMutation::SetAttribute {
            node: &node_data.node,
            name: "hidden",
            value: "",
        }
    } else {
        mutation::DomMutation::RemoveAttribute {
            node: &node_data.node,
            name: "hidden",
        }
    };
    let _mutation_result = mutation::apply_dom_mutation(&node_data.registry, mutation);
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
    let Some(node_data) = holder_node(scope, &args) else {
        return;
    };

    if let Some(host) = crate::dom::SyntheticShadowTreeBackend.host_for_shadow_root(&node_data.node)
    {
        let js_node = create_js_node(scope, host, &node_data.registry, &node_data.document);
        retval.set(js_node.into());
    } else {
        retval.set(v8::undefined(scope).into());
    }
}








fn get_text_content(
    scope: &mut v8::PinScope<'_, '_>,
    _name: v8::Local<v8::Name>,
    args: v8::PropertyCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let Some(node_data) = holder_node(scope, &args) else {
        return;
    };

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
    let Some(node_data) = holder_node(scope, &args) else {
        return;
    };
    let _reactions = CeReactionsGuard::new(scope, &node_data.registry);

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
pub(super) fn inner_html_target(node: &NodePtr) -> NodePtr {
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



fn get_outer_html(
    scope: &mut v8::PinScope<'_, '_>,
    _name: v8::Local<v8::Name>,
    args: v8::PropertyCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let Some(node_data) = holder_node(scope, &args) else {
        return;
    };

    let html = crate::dom::serialize_outer_html(&node_data.node);
    retval.set(v8_str(scope, &html).into());
}

fn get_classlist(
    scope: &mut v8::PinScope<'_, '_>,
    _name: v8::Local<v8::Name>,
    args: v8::PropertyCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let Some(node_data) = holder_node(scope, &args) else {
        return;
    };
    let obj = classlist::build_classlist_object(scope, node_data);
    retval.set(obj.into());
}

// ─── Tree collections / events / shadow DOM ─────────────────────────────────

pub(super) fn child_nodes_of(node: &NodePtr) -> Vec<NodePtr> {
    match &*node.borrow() {
        Node::Element(el) => el.children.clone(),
        Node::Document { children, .. } => children.clone(),
        _ => Vec::new(),
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

    let node_data = node_data_from(args.data());
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

    let node_data = node_data_from(args.data());
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
    let Some(node_data) = this_node(scope, &args) else {
        return;
    };
    let obj = bounding_rect_object(scope, node_data);
    retval.set(obj.into());
}

fn get_client_rects(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let Some(node_data) = this_node(scope, &args) else {
        return;
    };
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

/// Whether `node` is the document element (`<html>` directly in a document).
fn is_root_element(node: &NodePtr) -> bool {
    let is_html = matches!(&*node.borrow(), Node::Element(el) if el.tag_name.eq_ignore_ascii_case("html"));
    is_html
        && crate::dom::parent_ptr(node)
            .is_some_and(|parent| matches!(&*parent.borrow(), Node::Document { .. }))
}

/// Native bridge used by v8_post.js metric getters: returns the requested box
/// metric from Blitz/Stylo layout, or 0 when the node is not laid out (so the
/// JS fallback heuristic can take over).
fn get_layout_metric(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let Some(node_data) = this_node(scope, &args) else {
        return;
    };
    let name = args.get(0).to_rust_string_lossy(scope);
    // The root element's client size is the viewport, not its box.
    // <https://drafts.csswg.org/cssom-view/#dom-element-clientwidth>
    if matches!(name.as_str(), "clientWidth" | "clientHeight") && is_root_element(&node_data.node) {
        if let Some((width, height)) = node_data.registry.viewport_size() {
            let value = if name == "clientWidth" { width } else { height };
            retval.set(v8::Number::new(scope, value as f64).into());
            return;
        }
    }
    if name == "rendered" {
        let rendered = node_data.registry.is_rendered(&node_data.node);
        retval.set(v8::Integer::new(scope, rendered.into()).into());
        return;
    }
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

/// ParentNode/ChildNode `(Node or DOMString)...` arguments as nodes.
pub(super) fn parsed_html_nodes(html: &str) -> Vec<NodePtr> {
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
    let Some(node_data) = this_node(scope, &args) else {
        return;
    };
    let _reactions = CeReactionsGuard::new(scope, &node_data.registry);
    insert_nodes_at_position(scope, node_data, &position, parsed_html_nodes(&html));
}

fn insert_adjacent_text(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut _retval: v8::ReturnValue,
) {
    let position = args.get(0).to_rust_string_lossy(scope);
    let text = args.get(1).to_rust_string_lossy(scope);
    let Some(node_data) = this_node(scope, &args) else {
        return;
    };
    let _reactions = CeReactionsGuard::new(scope, &node_data.registry);
    insert_nodes_at_position(scope, node_data, &position, vec![Node::text(text)]);
}

fn insert_adjacent_element(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let position = args.get(0).to_rust_string_lossy(scope);
    let element_value = args.get(1);
    let Some(node_data) = this_node(scope, &args) else {
        return;
    };
    let _reactions = CeReactionsGuard::new(scope, &node_data.registry);
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




/// `element.attachInternals()` — the `ElementInternals` handle a
/// form-associated custom element uses to publish its submission value and
/// validity. Throws `NotSupportedError` for elements whose definition did not
/// set `static formAssociated = true`, and on a second call, per spec.
///
/// `form`, `validationMessage` and `willValidate` are accessors rather than
/// stored values: `attachInternals` is normally called from the constructor,
/// before the element is connected and before any `setValidity`, so a snapshot
/// taken here would be permanently stale. `labels` stays empty — Aurora has no
/// `<label>` association.
fn attach_internals<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    args: v8::FunctionCallbackArguments<'s>,
    mut retval: v8::ReturnValue,
) {
    let Some(node_data) = this_node(scope, &args) else {
        return;
    };
    let tag = match &*node_data.node.borrow() {
        Node::Element(el) => el.tag_name.clone(),
        _ => String::new(),
    };
    let ce = &node_data.registry.ce_registry;
    if !ce.is_form_associated(&tag) {
        throw_named_error(
            scope,
            "NotSupportedError",
            "attachInternals requires a form-associated custom element",
        );
        return;
    }
    let node_id = node_data.registry.register(node_data.node.clone());
    if ce.has_element_internals(node_id) {
        throw_named_error(
            scope,
            "NotSupportedError",
            "attachInternals has already been called on this element",
        );
        return;
    }
    ce.create_element_internals(node_id);

    let template = templates::cached(scope, &node_data.registry, TemplateKind::ElementInternals, |scope, template| {
        install_readonly_accessor(
            scope,
            template,
            "shadowRoot",
            get_internals_shadow,
        );
        install_readonly_accessor(scope, template, "form", get_internals_form);
        install_readonly_accessor(
            scope,
            template,
            "validationMessage",
            get_internals_validation_message,
        );
        install_readonly_accessor(
            scope,
            template,
            "willValidate",
            get_internals_will_validate,
        );
        install_readonly_accessor(scope, template, "labels", get_internals_labels);
        install_method(
            scope,
            template,
            "setFormValue",
            internals_set_form_value,
        );
        install_method(
            scope,
            template,
            "setValidity",
            internals_set_validity,
        );
        install_method(
            scope,
            template,
            "checkValidity",
            internals_check_validity,
        );
        install_method(
            scope,
            template,
            "reportValidity",
            internals_check_validity,
        );
    });
    let internals = templates::instantiate(scope, template, NativeTag::Node, node_data as *const NodeData);
    retval.set(internals.into());
}

/// `internals.shadowRoot` — the element's own shadow root, or null.
fn get_internals_shadow(
    scope: &mut v8::PinScope<'_, '_>,
    _name: v8::Local<v8::Name>,
    args: v8::PropertyCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let Some(node_data) = holder_node(scope, &args) else {
        return;
    };
    let shadow = match &*node_data.node.borrow() {
        Node::Element(el) => el.shadow_root.clone(),
        _ => None,
    };
    match shadow {
        Some(root) => {
            let id = node_data.registry.register(root);
            match node_data.registry.lookup_js_wrapper(scope, id) {
                Some(wrapper) => retval.set(wrapper.into()),
                None => retval.set(v8::null(scope).into()),
            }
        }
        None => retval.set(v8::null(scope).into()),
    }
}

/// `internals.form` — the element's current form owner, resolved live.
fn get_internals_form(
    scope: &mut v8::PinScope<'_, '_>,
    _name: v8::Local<v8::Name>,
    args: v8::PropertyCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let Some(node_data) = holder_node(scope, &args) else {
        return;
    };
    let owner = form_associated::form_owner_of(&node_data.document, &node_data.node);
    match owner {
        Some(form) => {
            let id = node_data.registry.register(form);
            match node_data.registry.lookup_js_wrapper(scope, id) {
                Some(wrapper) => retval.set(wrapper.into()),
                None => retval.set(v8::null(scope).into()),
            }
        }
        None => retval.set(v8::null(scope).into()),
    }
}

/// `internals.validationMessage` — whatever the last `setValidity` recorded.
fn get_internals_validation_message(
    scope: &mut v8::PinScope<'_, '_>,
    _name: v8::Local<v8::Name>,
    args: v8::PropertyCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let Some(node_data) = holder_node(scope, &args) else {
        return;
    };
    let node_id = node_data.registry.register(node_data.node.clone());
    let message = node_data.registry.ce_registry.validation_message(node_id);
    retval.set(v8_str(scope, &message).into());
}

/// `internals.willValidate` — a disabled element is barred from constraint
/// validation, so it will not validate.
fn get_internals_will_validate(
    scope: &mut v8::PinScope<'_, '_>,
    _name: v8::Local<v8::Name>,
    args: v8::PropertyCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let Some(node_data) = holder_node(scope, &args) else {
        return;
    };
    let disabled = form_associated::is_actually_disabled(&node_data.node);
    retval.set(v8::Boolean::new(scope, !disabled).into());
}

/// `form.reset()` — fires `formResetCallback` on every form-associated custom
/// element this form owns.
///
/// Aurora has no built-in form controls, so resetting their values is not part
/// of this; the custom-element half is. The method lands on every element
/// because node methods are installed on one shared template, so it checks the
/// tag itself and does nothing for a non-`<form>` — the same shape as calling
/// `reset()` on a `<div>` in a real browser, where the method simply is not
/// there to do anything.
fn form_reset(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut _retval: v8::ReturnValue,
) {
    let Some(node_data) = this_node(scope, &args) else {
        return;
    };
    let is_form = matches!(&*node_data.node.borrow(), Node::Element(el) if el.tag_name == "form");
    if !is_form {
        return;
    }
    // Reset is a `[CEReactions]` operation: the callbacks it enqueues must run
    // before it returns to script.
    let _guard = CeReactionsGuard::new(scope, &node_data.registry);
    form_associated::enqueue_reset_reactions(&node_data.registry, &node_data.node);
}

/// `internals.labels` — always empty; Aurora has no `<label>` association.
fn get_internals_labels(
    scope: &mut v8::PinScope<'_, '_>,
    _name: v8::Local<v8::Name>,
    _args: v8::PropertyCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    retval.set(v8::Array::new(scope, 0).into());
}

use super::dom_exception::throw_dom_exception as throw_named_error;

/// `internals.setFormValue(value)` — record the element's submission value.
fn internals_set_form_value(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut _retval: v8::ReturnValue,
) {
    let Some(node_data) = this_node(scope, &args) else {
        return;
    };
    let node_id = node_data.registry.register(node_data.node.clone());
    let value = args.get(0);
    let value = if value.is_null_or_undefined() {
        None
    } else {
        Some(value.to_rust_string_lossy(scope))
    };
    node_data
        .registry
        .ce_registry
        .set_form_value(node_id, value);
}

/// `internals.setValidity(flags, message)` — an empty/absent flags object means
/// valid, matching the spec's "no constraint is violated" case.
fn internals_set_validity(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut _retval: v8::ReturnValue,
) {
    let Some(node_data) = this_node(scope, &args) else {
        return;
    };
    let node_id = node_data.registry.register(node_data.node.clone());
    let flags = args.get(0);
    let mut valid = true;
    if let Some(flags) = flags.to_object(scope).filter(|_| flags.is_object())
        && let Some(names) =
            flags.get_own_property_names(scope, v8::GetPropertyNamesArgs::default())
    {
        for i in 0..names.length() {
            if let Some(key) = names.get_index(scope, i)
                && let Some(value) = flags.get(scope, key)
                && value.is_true()
            {
                valid = false;
            }
        }
    }
    let message = if args.length() > 1 {
        args.get(1).to_rust_string_lossy(scope)
    } else {
        String::new()
    };
    node_data
        .registry
        .ce_registry
        .set_validity(node_id, valid, message);
}

/// `internals.checkValidity()` / `reportValidity()`.
fn internals_check_validity(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let Some(node_data) = this_node(scope, &args) else {
        return;
    };
    let node_id = node_data.registry.register(node_data.node.clone());
    let valid = node_data.registry.ce_registry.is_valid(node_id);
    retval.set(v8::Boolean::new(scope, valid).into());
}

fn attach_shadow(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let Some(node_data) = this_node(scope, &args) else {
        return;
    };

    let opts = args
        .get(0)
        .to_object(scope)
        .filter(|_| args.get(0).is_object());
    let mode = opts
        .and_then(|opts| {
            opts.get(scope, v8_str(scope, "mode").into())
                .filter(|v| v.is_string())
                .map(|v| v.to_rust_string_lossy(scope))
        })
        .unwrap_or_else(|| "open".to_string());

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
    let Some(node_data) = this_node(scope, &args) else {
        return;
    };
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
