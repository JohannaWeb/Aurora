#![allow(unsafe_op_in_unsafe_fn)]
use std::cell::RefCell;
use std::ptr::NonNull;
use std::rc::Rc;

use mozjs::context::{JSContext, RawJSContext};
use mozjs::jsapi::{CallArgs, HandleValueArray, JSObject, Value};
use mozjs::jsval::{BooleanValue, NullValue, ObjectValue, UndefinedValue};
use mozjs::rooted;
use mozjs::rust::wrappers2;

use crate::css::ElementData;
use crate::css::selectors_impl::{AuroraSelectorImpl, element_matches, parse_selector_list};
use crate::dom::{Node, NodePtr};
use crate::js_sm::mutation_observer::{queue_attribute_mutation, queue_childlist_mutation};
use crate::js_sm::utils::*;

// ── Node ID property name ─────────────────────────────────────────────────────

const NODE_ID_PROP: *const std::ffi::c_char = c"__node_id__".as_ptr();

// ── Node JS object creation ───────────────────────────────────────────────────

/// Create a JS plain object representing a DOM node.
/// Registers the node in the state registry and stores its ID on the object.
pub(in crate::js_sm) unsafe fn create_js_node(cx: &mut JSContext, node: NodePtr) -> *mut JSObject {
    let state = &mut *get_state_ptr(cx);
    let node_id = state.registry.register(node.clone());
    if let Some(existing) = state.registry.lookup_js_wrapper(node_id) {
        return existing;
    }

    let obj = new_plain_object(cx);
    rooted!(&in(cx) let obj_root = obj);

    // Hidden ID for callback dispatch
    set_prop_i32(cx, obj_root.handle(), c"__node_id__", node_id as i32);

    let (node_type, node_name, tag_name, text_content) = match &*node.borrow() {
        Node::Element(el) => {
            let tc = collect_text_content(&node);
            if el.tag_name == "#document-fragment" {
                (11i32, "#document-fragment".to_string(), "".to_string(), tc)
            } else {
                (1i32, el.tag_name.to_uppercase(), el.tag_name.clone(), tc)
            }
        }
        Node::Text(t) => (3i32, "#text".to_string(), "".to_string(), t.clone()),
        Node::Document { .. } => (9i32, "#document".to_string(), "".to_string(), String::new()),
    };

    set_prop_i32(cx, obj_root.handle(), c"nodeType", node_type);
    set_prop_str(cx, obj_root.handle(), c"nodeName", &node_name);

    if node_type == 1 {
        set_prop_str(cx, obj_root.handle(), c"tagName", &tag_name.to_uppercase());
        set_prop_str(
            cx,
            obj_root.handle(),
            c"localName",
            &tag_name.to_lowercase(),
        );

        // Mirror common attributes
        let mut template_content_to_persist: Option<NodePtr> = None;
        if let Node::Element(el) = &*node.borrow() {
            let id_val = el.attributes.get("id").cloned().unwrap_or_default();
            let class_val = el.attributes.get("class").cloned().unwrap_or_default();
            let href_val = el.attributes.get("href").cloned().unwrap_or_default();
            let src_val = el.attributes.get("src").cloned().unwrap_or_default();
            let type_val = el.attributes.get("type").cloned().unwrap_or_default();
            let name_val = el.attributes.get("name").cloned().unwrap_or_default();
            let value_val = el.attributes.get("value").cloned().unwrap_or_default();
            set_prop_str(cx, obj_root.handle(), c"id", &id_val);
            set_prop_str(cx, obj_root.handle(), c"className", &class_val);
            set_prop_str(cx, obj_root.handle(), c"href", &href_val);
            set_prop_str(cx, obj_root.handle(), c"src", &src_val);
            set_prop_str(cx, obj_root.handle(), c"type", &type_val);
            set_prop_str(cx, obj_root.handle(), c"name", &name_val);
            set_prop_str(cx, obj_root.handle(), c"value", &value_val);
            // Live accessors — each read/write goes through the Rust DOM
            define_accessor(
                cx,
                obj_root.handle(),
                c"innerHTML",
                Some(get_inner_html),
                Some(set_inner_html),
            );
            define_accessor(
                cx,
                obj_root.handle(),
                c"outerHTML",
                Some(get_outer_html),
                None,
            );
            define_accessor(
                cx,
                obj_root.handle(),
                c"textContent",
                Some(get_text_content),
                Some(set_text_content),
            );
            define_accessor(
                cx,
                obj_root.handle(),
                c"innerText",
                Some(get_text_content),
                Some(set_text_content),
            );

            // classList — stores __node_id__ so add/remove/toggle can mutate the Rust DOM
            let cl = make_class_list(cx, node_id, &class_val);
            set_prop_obj(cx, obj_root.handle(), c"classList", cl);

            // style object (CSSStyleDeclaration stub)
            let style_obj = new_plain_object(cx);
            rooted!(&in(cx) let style_root = style_obj);
            define_fn(
                cx,
                style_root.handle(),
                c"getPropertyValue",
                Some(return_empty_string_cb),
                1,
            );
            define_fn(cx, style_root.handle(), c"setProperty", Some(noop_cb), 2);
            define_fn(cx, style_root.handle(), c"removeProperty", Some(noop_cb), 1);
            set_prop_obj(cx, obj_root.handle(), c"style", style_obj);

            // dataset stub
            let dataset = new_plain_object(cx);
            set_prop_obj(cx, obj_root.handle(), c"dataset", dataset);

            // <canvas> — width/height reflect the element's attributes (default
            // 300x150 per the HTML spec) and getContext returns a stub 2D/WebGL
            // context so canvas-drawing app code doesn't throw on first use.
            if tag_name.eq_ignore_ascii_case("canvas") {
                let width = el
                    .attributes
                    .get("width")
                    .and_then(|v| v.parse::<f64>().ok())
                    .unwrap_or(300.0);
                let height = el
                    .attributes
                    .get("height")
                    .and_then(|v| v.parse::<f64>().ok())
                    .unwrap_or(150.0);
                set_prop_f64(cx, obj_root.handle(), c"width", width);
                set_prop_f64(cx, obj_root.handle(), c"height", height);
                define_fn(
                    cx,
                    obj_root.handle(),
                    c"getContext",
                    Some(canvas_get_context),
                    2,
                );
                define_fn(
                    cx,
                    obj_root.handle(),
                    c"toDataURL",
                    Some(canvas_to_data_url),
                    2,
                );
                define_fn(cx, obj_root.handle(), c"toBlob", Some(noop_cb), 3);
                define_fn(
                    cx,
                    obj_root.handle(),
                    c"transferControlToOffscreen",
                    Some(node_return_first_arg),
                    0,
                );
            }

            if tag_name.eq_ignore_ascii_case("template") {
                // Script-created templates have no parsed contents yet; create
                // the fragment and persist it (below, once this scope's shared
                // borrow ends) so later `innerHTML` writes land in the same
                // fragment this JS `content` object wraps.
                let content_node = el
                    .template_contents
                    .clone()
                    .unwrap_or_else(|| Node::document_fragment(Vec::new()));
                let content_obj = create_js_node(cx, content_node.clone());
                set_prop_obj(cx, obj_root.handle(), c"content", content_obj);
                template_content_to_persist = Some(content_node);
            }

            // Shadow DOM — see element_attach_shadow for how this maps onto
            // the host's own NodePtr.
            define_fn(
                cx,
                obj_root.handle(),
                c"attachShadow",
                Some(element_attach_shadow),
                1,
            );
            // ShadyDOM (webcomponents-all-noPatch.js) wraps `attachShadow` and
            // calls through to `this.node.__shady_attachShadow(...)` to get the
            // "native" root it then shims. Without this alias the call throws
            // a TypeError inside Polymer's `_attachDom`, which gets swallowed
            // and silently aborts `ready()` before the element's template is
            // attached.
            define_fn(
                cx,
                obj_root.handle(),
                c"__shady_attachShadow",
                Some(element_attach_shadow),
                1,
            );
            set_prop_null(cx, obj_root.handle(), c"shadowRoot");
            define_fn(
                cx,
                obj_root.handle(),
                c"getRootNode",
                Some(node_return_first_arg_or_this),
                1,
            );
        }

        if let Some(content) = template_content_to_persist.take() {
            if let Node::Element(el) = &mut *node.borrow_mut() {
                if el.template_contents.is_none() {
                    el.template_contents = Some(content);
                }
            }
        }

        if tag_name.contains('-') {
            let global_raw = state.global;
            if !global_raw.is_null() {
                rooted!(&in(cx) let global = global_raw);
                rooted!(&in(cx) let node_val = ObjectValue(obj));
                call_named_global_fn(
                    cx,
                    global.handle(),
                    c"__aurora_track_custom_element__",
                    node_val.handle().get(),
                );
            }
        }

        // <video>/<audio> — decorate with the HTMLMediaElement surface
        // (currentTime/duration/play/pause/MediaSource wiring/event ordering)
        // installed as `__aurora_install_media_element__` by
        // install_media_polyfills. Heavy state-machine logic lives in JS, same
        // as the custom-elements upgrade pipeline, because expressing
        // promise/event-ordering rules in raw JSAPI is painful and JS does it
        // for free. Done outside the `node.borrow()` above since the installer
        // calls back into native node methods (e.g. `getAttribute`) that take
        // their own borrow — fine for shared borrows, but best not to nest.
        if tag_name.eq_ignore_ascii_case("video") || tag_name.eq_ignore_ascii_case("audio") {
            let state = &mut *get_state_ptr(cx);
            let global_raw = state.global;
            rooted!(&in(cx) let global = global_raw);
            call_named_global_fn(
                cx,
                global.handle(),
                c"__aurora_install_media_element__",
                ObjectValue(obj),
            );
        }
    } else if node_type == 3 {
        // Text node
        set_prop_str(cx, obj_root.handle(), c"nodeValue", &text_content);
        set_prop_str(cx, obj_root.handle(), c"textContent", &text_content);
        set_prop_str(cx, obj_root.handle(), c"data", &text_content);
        set_prop_i32(cx, obj_root.handle(), c"length", text_content.len() as i32);
    } else if node_type == 11 {
        define_accessor(
            cx,
            obj_root.handle(),
            c"innerHTML",
            Some(get_inner_html),
            Some(set_inner_html),
        );
        define_accessor(
            cx,
            obj_root.handle(),
            c"textContent",
            Some(get_text_content),
            Some(set_text_content),
        );
    }

    // Common geometry stubs
    if node_type == 1 {
        set_prop_f64(cx, obj_root.handle(), c"offsetWidth", 0.0);
        set_prop_f64(cx, obj_root.handle(), c"offsetHeight", 0.0);
        set_prop_f64(cx, obj_root.handle(), c"offsetTop", 0.0);
        set_prop_f64(cx, obj_root.handle(), c"offsetLeft", 0.0);
        set_prop_f64(cx, obj_root.handle(), c"clientWidth", 0.0);
        set_prop_f64(cx, obj_root.handle(), c"clientHeight", 0.0);
        set_prop_f64(cx, obj_root.handle(), c"scrollWidth", 0.0);
        set_prop_f64(cx, obj_root.handle(), c"scrollHeight", 0.0);
        set_prop_f64(cx, obj_root.handle(), c"scrollTop", 0.0);
        set_prop_f64(cx, obj_root.handle(), c"scrollLeft", 0.0);
    }

    // Common node methods
    define_fn(
        cx,
        obj_root.handle(),
        c"addEventListener",
        Some(node_add_event_listener),
        3,
    );
    define_fn(
        cx,
        obj_root.handle(),
        c"removeEventListener",
        Some(node_remove_event_listener),
        3,
    );
    define_fn(
        cx,
        obj_root.handle(),
        c"dispatchEvent",
        Some(return_true_cb),
        1,
    );
    define_fn(
        cx,
        obj_root.handle(),
        c"getAttribute",
        Some(node_get_attribute),
        1,
    );
    define_fn(
        cx,
        obj_root.handle(),
        c"setAttribute",
        Some(node_set_attribute),
        2,
    );
    define_fn(
        cx,
        obj_root.handle(),
        c"removeAttribute",
        Some(node_remove_attribute),
        1,
    );
    define_fn(
        cx,
        obj_root.handle(),
        c"hasAttribute",
        Some(node_has_attribute),
        1,
    );
    define_fn(
        cx,
        obj_root.handle(),
        c"hasAttributes",
        Some(node_has_attributes),
        0,
    );
    define_fn(
        cx,
        obj_root.handle(),
        c"appendChild",
        Some(node_append_child),
        1,
    );
    define_fn(
        cx,
        obj_root.handle(),
        c"insertBefore",
        Some(node_insert_before),
        2,
    );
    define_fn(
        cx,
        obj_root.handle(),
        c"removeChild",
        Some(node_remove_child),
        1,
    );
    define_fn(
        cx,
        obj_root.handle(),
        c"replaceChild",
        Some(node_replace_child),
        2,
    );
    define_fn(
        cx,
        obj_root.handle(),
        c"cloneNode",
        Some(node_clone_node),
        1,
    );
    define_fn(cx, obj_root.handle(), c"contains", Some(node_contains), 1);
    define_fn(cx, obj_root.handle(), c"closest", Some(node_closest), 1);
    define_fn(cx, obj_root.handle(), c"matches", Some(node_matches), 1);
    define_fn(
        cx,
        obj_root.handle(),
        c"querySelector",
        Some(node_query_selector),
        1,
    );
    define_fn(
        cx,
        obj_root.handle(),
        c"querySelectorAll",
        Some(node_query_selector_all),
        1,
    );
    define_fn(
        cx,
        obj_root.handle(),
        c"getElementsByTagName",
        Some(node_get_elements_by_tag),
        1,
    );
    define_fn(
        cx,
        obj_root.handle(),
        c"getElementsByClassName",
        Some(node_get_elements_by_class),
        1,
    );
    define_fn(
        cx,
        obj_root.handle(),
        c"getBoundingClientRect",
        Some(node_get_bounding_rect),
        0,
    );
    define_fn(
        cx,
        obj_root.handle(),
        c"getClientRects",
        Some(return_empty_array_cb),
        0,
    );
    define_fn(cx, obj_root.handle(), c"scrollIntoView", Some(noop_cb), 1);
    define_fn(cx, obj_root.handle(), c"focus", Some(noop_cb), 0);
    define_fn(cx, obj_root.handle(), c"blur", Some(noop_cb), 0);
    define_fn(cx, obj_root.handle(), c"click", Some(noop_cb), 0);
    define_fn(cx, obj_root.handle(), c"remove", Some(noop_cb), 0);
    define_fn(cx, obj_root.handle(), c"before", Some(noop_cb), 1);
    define_fn(cx, obj_root.handle(), c"after", Some(noop_cb), 1);
    define_fn(cx, obj_root.handle(), c"prepend", Some(noop_cb), 1);
    define_fn(cx, obj_root.handle(), c"append", Some(node_append_child), 1);
    define_fn(
        cx,
        obj_root.handle(),
        c"insertAdjacentHTML",
        Some(noop_cb),
        2,
    );
    define_fn(
        cx,
        obj_root.handle(),
        c"insertAdjacentElement",
        Some(node_return_first_arg),
        2,
    );
    define_fn(
        cx,
        obj_root.handle(),
        c"insertAdjacentText",
        Some(noop_cb),
        2,
    );
    define_fn(cx, obj_root.handle(), c"normalize", Some(noop_cb), 0);
    define_fn(
        cx,
        obj_root.handle(),
        c"animate",
        Some(return_animation_obj),
        2,
    );
    define_fn(
        cx,
        obj_root.handle(),
        c"getAnimations",
        Some(return_empty_array_cb),
        0,
    );

    // Live tree-traversal getters — computed from the Rust DOM on every access
    define_getter(cx, obj_root.handle(), c"parentNode", Some(get_parent_node));
    define_getter(
        cx,
        obj_root.handle(),
        c"parentElement",
        Some(get_parent_element),
    );
    define_getter(
        cx,
        obj_root.handle(),
        c"nextSibling",
        Some(get_next_sibling),
    );
    define_getter(
        cx,
        obj_root.handle(),
        c"previousSibling",
        Some(get_previous_sibling),
    );
    define_getter(
        cx,
        obj_root.handle(),
        c"nextElementSibling",
        Some(get_next_element_sibling),
    );
    define_getter(
        cx,
        obj_root.handle(),
        c"previousElementSibling",
        Some(get_previous_element_sibling),
    );
    define_getter(cx, obj_root.handle(), c"childNodes", Some(get_child_nodes));
    define_getter(cx, obj_root.handle(), c"children", Some(get_children));
    define_getter(
        cx,
        obj_root.handle(),
        c"childElementCount",
        Some(get_child_element_count),
    );
    define_getter(cx, obj_root.handle(), c"firstChild", Some(get_first_child));
    define_getter(cx, obj_root.handle(), c"lastChild", Some(get_last_child));
    define_getter(
        cx,
        obj_root.handle(),
        c"firstElementChild",
        Some(get_first_element_child),
    );
    define_getter(
        cx,
        obj_root.handle(),
        c"lastElementChild",
        Some(get_last_element_child),
    );
    set_prop_null(cx, obj_root.handle(), c"offsetParent");
    assign_dom_wrapper_prototype(cx, obj, node_type, &tag_name);
    state.registry.cache_js_wrapper(node_id, obj);

    obj
}

unsafe fn assign_dom_wrapper_prototype(
    cx: &mut JSContext,
    obj: *mut JSObject,
    node_type: i32,
    tag_name: &str,
) {
    let ctor_name = match node_type {
        11 => c"DocumentFragment",
        9 => c"Document",
        3 => c"Text",
        1 => match tag_name {
            "template" => c"HTMLTemplateElement",
            "script" => c"HTMLScriptElement",
            "div" => c"HTMLDivElement",
            "span" => c"HTMLSpanElement",
            "body" => c"HTMLBodyElement",
            "head" => c"HTMLHeadElement",
            "html" => c"HTMLHtmlElement",
            "a" => c"HTMLAnchorElement",
            "img" => c"HTMLImageElement",
            "input" => c"HTMLInputElement",
            "textarea" => c"HTMLTextAreaElement",
            "canvas" => c"HTMLCanvasElement",
            "video" => c"HTMLVideoElement",
            "audio" => c"HTMLAudioElement",
            _ => c"HTMLElement",
        },
        _ => c"Node",
    };

    let global_raw = (*get_state_ptr(cx)).global;
    if global_raw.is_null() {
        return;
    }

    rooted!(&in(cx) let obj_root = obj);
    rooted!(&in(cx) let global_root = global_raw);
    rooted!(&in(cx) let mut ctor_val = UndefinedValue());
    if !wrappers2::JS_GetProperty(
        cx,
        global_root.handle(),
        ctor_name.as_ptr(),
        ctor_val.handle_mut(),
    ) || !ctor_val.get().is_object()
    {
        return;
    }

    let ctor = ctor_val.get().to_object_or_null();
    rooted!(&in(cx) let ctor_root = ctor);
    rooted!(&in(cx) let mut proto_val = UndefinedValue());
    if !wrappers2::JS_GetProperty(
        cx,
        ctor_root.handle(),
        c"prototype".as_ptr(),
        proto_val.handle_mut(),
    ) || !proto_val.get().is_object()
    {
        return;
    }

    let proto = proto_val.get().to_object_or_null();
    rooted!(&in(cx) let proto_root = proto);
    wrappers2::JS_SetPrototype(cx, obj_root.handle(), proto_root.handle());
}

pub(super) unsafe fn make_class_list(
    cx: &mut JSContext,
    node_id: u32,
    class_str: &str,
) -> *mut JSObject {
    let obj = new_plain_object(cx);
    rooted!(&in(cx) let obj_root = obj);
    // __node_id__ lets add/remove/toggle find and mutate the Rust node's class attribute
    set_prop_i32(cx, obj_root.handle(), c"__node_id__", node_id as i32);
    let classes: Vec<&str> = class_str.split_whitespace().collect();
    let len = classes.len();
    for (i, cls) in classes.iter().enumerate() {
        let key = std::ffi::CString::new(i.to_string()).unwrap_or_default();
        set_prop_str(cx, obj_root.handle(), key.as_c_str(), cls);
    }
    set_prop_i32(cx, obj_root.handle(), c"length", len as i32);
    set_prop_str(cx, obj_root.handle(), c"value", class_str);
    define_fn(
        cx,
        obj_root.handle(),
        c"contains",
        Some(class_list_contains),
        1,
    );
    define_fn(cx, obj_root.handle(), c"add", Some(class_list_add), 1);
    define_fn(cx, obj_root.handle(), c"remove", Some(class_list_remove), 1);
    define_fn(cx, obj_root.handle(), c"toggle", Some(class_list_toggle), 2);
    define_fn(cx, obj_root.handle(), c"replace", Some(noop_cb), 2);
    obj
}

fn collect_text_content(node: &NodePtr) -> String {
    match &*node.borrow() {
        Node::Text(t) => t.clone(),
        Node::Element(el) => el.children.iter().map(collect_text_content).collect(),
        Node::Document { children, .. } => children.iter().map(collect_text_content).collect(),
    }
}

fn debug_tag_for_node(node: &NodePtr) -> Option<String> {
    match &*node.borrow() {
        Node::Element(el) => Some(el.tag_name.clone()),
        _ => None,
    }
}

// ── DOM traversal helpers ─────────────────────────────────────────────────────

fn find_by_id(root: &NodePtr, id: &str) -> Option<NodePtr> {
    match &*root.borrow() {
        Node::Element(el) => {
            if el.attributes.get("id").map(|s| s.as_str()) == Some(id) {
                return Some(root.clone());
            }
            for child in &el.children {
                if let Some(found) = find_by_id(child, id) {
                    return Some(found);
                }
            }
            None
        }
        Node::Document { children, .. } => {
            for child in children {
                if let Some(found) = find_by_id(child, id) {
                    return Some(found);
                }
            }
            None
        }
        Node::Text(_) => None,
    }
}

fn collect_by_tag(root: &NodePtr, tag: &str, acc: &mut Vec<NodePtr>) {
    match &*root.borrow() {
        Node::Element(el) => {
            if el.tag_name == tag || tag == "*" {
                acc.push(root.clone());
            }
            for child in &el.children {
                collect_by_tag(child, tag, acc);
            }
        }
        Node::Document { children, .. } => {
            for child in children {
                collect_by_tag(child, tag, acc);
            }
        }
        _ => {}
    }
}

fn collect_by_class(root: &NodePtr, class: &str, acc: &mut Vec<NodePtr>) {
    match &*root.borrow() {
        Node::Element(el) => {
            let cls = el.attributes.get("class").map(|s| s.as_str()).unwrap_or("");
            if cls.split_whitespace().any(|c| c == class) {
                acc.push(root.clone());
            }
            for child in &el.children {
                collect_by_class(child, class, acc);
            }
        }
        Node::Document { children, .. } => {
            for child in children {
                collect_by_class(child, class, acc);
            }
        }
        _ => {}
    }
}

pub(super) fn query_first_from(root: &NodePtr, selector: &str) -> Option<NodePtr> {
    let list = parse_selector_list(selector)?;
    let selectors = list.slice();
    query_first_rec(root, selectors, root, true)
}

pub(super) fn query_all_from(root: &NodePtr, selector: &str) -> Vec<NodePtr> {
    let Some(list) = parse_selector_list(selector) else {
        return vec![];
    };
    let selectors = list.slice();
    let mut out = Vec::new();
    query_all_rec(root, selectors, root, &mut out, true);
    out
}

fn query_first_rec(
    node: &NodePtr,
    selectors: &[selectors::parser::Selector<AuroraSelectorImpl>],
    root: &NodePtr,
    skip_root: bool,
) -> Option<NodePtr> {
    let is_element = matches!(&*node.borrow(), Node::Element(_));
    if !skip_root && is_element && node_matches_selectors(node, selectors, root) {
        return Some(node.clone());
    }
    let children = match &*node.borrow() {
        Node::Element(el) => el.children.clone(),
        Node::Document { children, .. } => children.clone(),
        _ => return None,
    };
    for child in &children {
        if let Some(found) = query_first_rec(child, selectors, root, false) {
            return Some(found);
        }
    }
    None
}

fn query_all_rec(
    node: &NodePtr,
    selectors: &[selectors::parser::Selector<AuroraSelectorImpl>],
    root: &NodePtr,
    out: &mut Vec<NodePtr>,
    skip_root: bool,
) {
    let is_element = matches!(&*node.borrow(), Node::Element(_));
    if !skip_root && is_element && node_matches_selectors(node, selectors, root) {
        out.push(node.clone());
    }
    let children = match &*node.borrow() {
        Node::Element(el) => el.children.clone(),
        Node::Document { children, .. } => children.clone(),
        _ => return,
    };
    for child in &children {
        query_all_rec(child, selectors, root, out, false);
    }
}

fn node_matches_selectors(
    node: &NodePtr,
    selectors: &[selectors::parser::Selector<AuroraSelectorImpl>],
    root: &NodePtr,
) -> bool {
    let element_data = match &*node.borrow() {
        Node::Element(el) => ElementData {
            tag_name: el.tag_name.clone(),
            attributes: el.attributes.clone(),
        },
        _ => return false,
    };
    let ancestors = build_ancestor_chain(root, node);
    let siblings = build_sibling_list(root, node);
    let sibling_idx = sibling_index_of(root, node);
    selectors
        .iter()
        .any(|sel| element_matches(sel, &element_data, &ancestors, &siblings, sibling_idx))
}

fn build_ancestor_chain(root: &NodePtr, target: &NodePtr) -> Vec<ElementData> {
    let mut chain = Vec::new();
    if let Some(parent) = find_parent(root, target) {
        chain = build_ancestor_chain(root, &parent);
        if let Node::Element(el) = &*parent.borrow() {
            chain.push(ElementData {
                tag_name: el.tag_name.clone(),
                attributes: el.attributes.clone(),
            });
        }
    }
    chain
}

fn build_sibling_list(root: &NodePtr, target: &NodePtr) -> Vec<ElementData> {
    let Some(parent) = find_parent(root, target) else {
        return vec![];
    };
    let children = match &*parent.borrow() {
        Node::Element(el) => el.children.clone(),
        Node::Document { children, .. } => children.clone(),
        _ => return vec![],
    };
    children
        .iter()
        .filter_map(|c| match &*c.borrow() {
            Node::Element(el) => Some(ElementData {
                tag_name: el.tag_name.clone(),
                attributes: el.attributes.clone(),
            }),
            _ => None,
        })
        .collect()
}

fn sibling_index_of(root: &NodePtr, target: &NodePtr) -> usize {
    let Some(parent) = find_parent(root, target) else {
        return 0;
    };
    let children = match &*parent.borrow() {
        Node::Element(el) => el.children.clone(),
        Node::Document { children, .. } => children.clone(),
        _ => return 0,
    };
    children
        .iter()
        .enumerate()
        .find_map(|(i, c)| (Rc::ptr_eq(c, target)).then_some(i))
        .unwrap_or(0)
}

fn find_parent(root: &NodePtr, target: &NodePtr) -> Option<NodePtr> {
    let children = match &*root.borrow() {
        Node::Element(el) => el.children.clone(),
        Node::Document { children, .. } => children.clone(),
        _ => return None,
    };
    for child in &children {
        if Rc::ptr_eq(child, target) {
            return Some(root.clone());
        }
        if let Some(found) = find_parent(child, target) {
            return Some(found);
        }
    }
    None
}

// ── Helper to build a JS NodeList array ────────────────────────────────────────

unsafe fn build_nodelist(cx: &mut JSContext, nodes: Vec<NodePtr>) -> *mut JSObject {
    let arr = wrappers2::NewArrayObject(cx, &HandleValueArray::empty());
    if arr.is_null() {
        return std::ptr::null_mut();
    }
    rooted!(&in(cx) let arr_root = arr);

    for (i, node) in nodes.into_iter().enumerate() {
        let node_obj = create_js_node(cx, node);
        rooted!(&in(cx) let node_val = ObjectValue(node_obj));
        rooted!(&in(cx) let idx_val = mozjs::jsval::UInt32Value(i as u32));
        wrappers2::JS_SetProperty(
            cx,
            arr_root.handle(),
            std::ffi::CString::new(i.to_string()).unwrap().as_ptr(),
            node_val.handle(),
        );
    }
    arr
}

// ── Get node_id from this-val ─────────────────────────────────────────────────

unsafe fn node_id_from_this(cx: &mut JSContext, args: &CallArgs) -> Option<u32> {
    let this_val = args.thisv().get();
    if !this_val.is_object() {
        return None;
    }
    let this_obj = this_val.to_object_or_null();
    if this_obj.is_null() {
        return None;
    }
    rooted!(&in(cx) let this_root = this_obj);
    rooted!(&in(cx) let mut id_val = UndefinedValue());
    if !wrappers2::JS_GetProperty(cx, this_root.handle(), NODE_ID_PROP, id_val.handle_mut()) {
        return None;
    }
    let v = id_val.get();
    if !v.is_number() {
        return None;
    }
    Some(v.to_number() as u32)
}

// ── Trivial callbacks ─────────────────────────────────────────────────────────

pub(super) unsafe extern "C" fn noop_cb(
    _cx: *mut RawJSContext,
    _argc: u32,
    vp: *mut Value,
) -> bool {
    let args = CallArgs::from_vp(vp, _argc);
    args.rval().set(UndefinedValue());
    true
}

unsafe extern "C" fn return_true_cb(_cx: *mut RawJSContext, _argc: u32, vp: *mut Value) -> bool {
    let args = CallArgs::from_vp(vp, _argc);
    args.rval().set(BooleanValue(true));
    true
}

unsafe extern "C" fn return_null_cb(_cx: *mut RawJSContext, _argc: u32, vp: *mut Value) -> bool {
    let args = CallArgs::from_vp(vp, _argc);
    args.rval().set(NullValue());
    true
}

unsafe extern "C" fn return_empty_array_cb(
    cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let arr = wrappers2::NewArrayObject(&mut cx, &HandleValueArray::empty());
    args.rval().set(if arr.is_null() {
        UndefinedValue()
    } else {
        ObjectValue(arr)
    });
    true
}

unsafe extern "C" fn return_empty_string_cb(
    cx: *mut RawJSContext,
    _argc: u32,
    vp: *mut Value,
) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, _argc);
    let js_str = new_js_string(&mut cx, "");
    args.rval().set(if js_str.is_null() {
        UndefinedValue()
    } else {
        mozjs::jsval::StringValue(&*js_str)
    });
    true
}

unsafe extern "C" fn node_return_first_arg(
    _cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    let args = CallArgs::from_vp(vp, argc);
    args.rval().set(if argc > 0 {
        args.get(0).get()
    } else {
        NullValue()
    });
    true
}

// ── <canvas> context stubs ────────────────────────────────────────────────────
// Aurora doesn't actually rasterize canvas drawing commands, but app code
// (chart libs, video players, etc.) expects `getContext` to return an object
// with the full CanvasRenderingContext2D-shaped API. Returning a no-op stub
// lets that code run instead of throwing on the first `ctx.fillRect(...)`.

unsafe fn build_canvas_2d_context(cx: &mut JSContext) -> *mut JSObject {
    let ctx = new_plain_object(cx);
    rooted!(&in(cx) let ctx_root = ctx);

    for name in &[
        c"save",
        c"restore",
        c"scale",
        c"rotate",
        c"translate",
        c"transform",
        c"setTransform",
        c"resetTransform",
        c"clearRect",
        c"fillRect",
        c"strokeRect",
        c"beginPath",
        c"closePath",
        c"moveTo",
        c"lineTo",
        c"bezierCurveTo",
        c"quadraticCurveTo",
        c"arc",
        c"arcTo",
        c"ellipse",
        c"rect",
        c"roundRect",
        c"fill",
        c"stroke",
        c"clip",
        c"isPointInPath",
        c"isPointInStroke",
        c"fillText",
        c"strokeText",
        c"drawImage",
        c"putImageData",
        c"drawFocusIfNeeded",
        c"setLineDash",
        c"createPattern",
    ] {
        define_fn(cx, ctx_root.handle(), name, Some(noop_cb), 2);
    }

    define_fn(
        cx,
        ctx_root.handle(),
        c"getLineDash",
        Some(return_empty_array_cb),
        0,
    );
    define_fn(cx, ctx_root.handle(), c"save", Some(noop_cb), 0);

    define_fn(
        cx,
        ctx_root.handle(),
        c"measureText",
        Some(canvas_measure_text),
        1,
    );
    define_fn(
        cx,
        ctx_root.handle(),
        c"getImageData",
        Some(canvas_get_image_data),
        4,
    );
    define_fn(
        cx,
        ctx_root.handle(),
        c"createImageData",
        Some(canvas_get_image_data),
        2,
    );
    define_fn(
        cx,
        ctx_root.handle(),
        c"createLinearGradient",
        Some(canvas_create_gradient),
        4,
    );
    define_fn(
        cx,
        ctx_root.handle(),
        c"createRadialGradient",
        Some(canvas_create_gradient),
        6,
    );
    define_fn(
        cx,
        ctx_root.handle(),
        c"createConicGradient",
        Some(canvas_create_gradient),
        3,
    );

    // Style/state properties app code commonly reads back after setting
    set_prop_str(cx, ctx_root.handle(), c"fillStyle", "#000000");
    set_prop_str(cx, ctx_root.handle(), c"strokeStyle", "#000000");
    set_prop_f64(cx, ctx_root.handle(), c"lineWidth", 1.0);
    set_prop_str(cx, ctx_root.handle(), c"lineCap", "butt");
    set_prop_str(cx, ctx_root.handle(), c"lineJoin", "miter");
    set_prop_f64(cx, ctx_root.handle(), c"miterLimit", 10.0);
    set_prop_f64(cx, ctx_root.handle(), c"globalAlpha", 1.0);
    set_prop_str(
        cx,
        ctx_root.handle(),
        c"globalCompositeOperation",
        "source-over",
    );
    set_prop_str(cx, ctx_root.handle(), c"font", "10px sans-serif");
    set_prop_str(cx, ctx_root.handle(), c"textAlign", "start");
    set_prop_str(cx, ctx_root.handle(), c"textBaseline", "alphabetic");
    set_prop_bool(cx, ctx_root.handle(), c"imageSmoothingEnabled", true);

    ctx
}

unsafe extern "C" fn canvas_get_context(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let kind = arg_to_string(&mut cx, &args, 0);
    if kind == "webgl"
        || kind == "webgl2"
        || kind == "experimental-webgl"
        || kind == "bitmaprenderer"
    {
        // No software/GPU GL backend — report unsupported like a headless browser would.
        args.rval().set(NullValue());
        return true;
    }
    let ctx = build_canvas_2d_context(&mut cx);
    args.rval().set(if ctx.is_null() {
        UndefinedValue()
    } else {
        ObjectValue(ctx)
    });
    true
}

unsafe extern "C" fn canvas_to_data_url(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    // 1x1 transparent PNG — enough for code that just needs *a* data URL string.
    let js_str = new_js_string(
        &mut cx,
        "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR4nGNgAAAAAgABc3UBGAAAAABJRU5ErkJggg==",
    );
    args.rval().set(if js_str.is_null() {
        UndefinedValue()
    } else {
        mozjs::jsval::StringValue(&*js_str)
    });
    true
}

unsafe extern "C" fn canvas_measure_text(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let text = arg_to_string(&mut cx, &args, 0);
    let metrics = new_plain_object(&mut cx);
    rooted!(&in(cx) let metrics_root = metrics);
    // Rough monospace-ish estimate so layout math doesn't divide by zero.
    let width = text.chars().count() as f64 * 6.0;
    set_prop_f64(&mut cx, metrics_root.handle(), c"width", width);
    set_prop_f64(
        &mut cx,
        metrics_root.handle(),
        c"actualBoundingBoxLeft",
        0.0,
    );
    set_prop_f64(
        &mut cx,
        metrics_root.handle(),
        c"actualBoundingBoxRight",
        width,
    );
    set_prop_f64(
        &mut cx,
        metrics_root.handle(),
        c"actualBoundingBoxAscent",
        0.0,
    );
    set_prop_f64(
        &mut cx,
        metrics_root.handle(),
        c"actualBoundingBoxDescent",
        0.0,
    );
    set_prop_f64(
        &mut cx,
        metrics_root.handle(),
        c"fontBoundingBoxAscent",
        0.0,
    );
    set_prop_f64(
        &mut cx,
        metrics_root.handle(),
        c"fontBoundingBoxDescent",
        0.0,
    );
    args.rval().set(if metrics.is_null() {
        UndefinedValue()
    } else {
        ObjectValue(metrics)
    });
    true
}

unsafe extern "C" fn canvas_get_image_data(
    cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let w = if argc > 2 { arg_to_f64(&args, 2) } else { 0.0 };
    let h = if argc > 3 { arg_to_f64(&args, 3) } else { 0.0 };
    let data_obj = new_plain_object(&mut cx);
    rooted!(&in(cx) let data_root = data_obj);
    let arr = wrappers2::NewArrayObject(&mut cx, &HandleValueArray::empty());
    set_prop_obj(&mut cx, data_root.handle(), c"data", arr);
    set_prop_f64(&mut cx, data_root.handle(), c"width", w);
    set_prop_f64(&mut cx, data_root.handle(), c"height", h);
    args.rval().set(if data_obj.is_null() {
        UndefinedValue()
    } else {
        ObjectValue(data_obj)
    });
    true
}

unsafe extern "C" fn canvas_create_gradient(
    cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let gradient = new_plain_object(&mut cx);
    rooted!(&in(cx) let gradient_root = gradient);
    define_fn(
        &mut cx,
        gradient_root.handle(),
        c"addColorStop",
        Some(noop_cb),
        2,
    );
    args.rval().set(if gradient.is_null() {
        UndefinedValue()
    } else {
        ObjectValue(gradient)
    });
    true
}

/// `node.getRootNode()` — spec returns the shadow root or document containing
/// the node. We don't track tree membership precisely enough to compute that,
/// so return `this`; callers mostly use it for `instanceof ShadowRoot` checks
/// or to walk back up, neither of which we can satisfy exactly anyway.
unsafe extern "C" fn node_return_first_arg_or_this(
    _cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    let args = CallArgs::from_vp(vp, argc);
    args.rval().set(args.thisv().get());
    true
}

/// `element.attachShadow({mode})` — Aurora's renderer has no concept of shadow
/// trees, so a spec-correct encapsulated shadow root would render nothing. This
/// returns a "shadow root" that proxies directly onto the host element's own
/// NodePtr through the existing native node methods: `shadowRoot.appendChild`/
/// `innerHTML` writes land as real children of the host in the live DOM and so
/// actually paint. That breaks encapsulation (no `:host`, no slotting, light DOM
/// and shadow DOM merge) — but for a custom-element-heavy site like YouTube,
/// "the content shows up, just not perfectly isolated" beats "nothing renders
/// because attachShadow doesn't exist."
pub(in crate::js_sm) unsafe extern "C" fn element_attach_shadow(
    cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);

    let node_id = match node_id_from_this(&mut cx, &args) {
        Some(id) => id,
        None => {
            args.rval().set(NullValue());
            return true;
        }
    };

    let host = {
        let state = &mut *get_state_ptr(&mut cx);
        state.registry.lookup(node_id)
    };
    if let Some(host) = host {
        if let crate::dom::Node::Element(el) = &*host.borrow() {
            if matches!(el.tag_name.as_str(), "ytd-app" | "ytd-masthead")
                && crate::js_sm::utils::debug_youtube_enabled()
            {
                log::info!(
                    target: "aurora::js",
                    "[yt-life] attachShadow {} id={}",
                    el.tag_name,
                    node_id
                );
            }
        }
    }

    let mode = if argc > 0 && args.get(0).get().is_object() {
        let opts = args.get(0).get().to_object_or_null();
        rooted!(&in(cx) let opts_root = opts);
        get_prop_string(&mut cx, opts_root.handle(), c"mode").unwrap_or_else(|| "open".to_string())
    } else {
        "open".to_string()
    };

    let sr = new_plain_object(&mut cx);
    rooted!(&in(cx) let sr_root = sr);
    set_prop_i32(&mut cx, sr_root.handle(), c"__node_id__", node_id as i32);
    set_prop_i32(&mut cx, sr_root.handle(), c"nodeType", 11);
    set_prop_str(&mut cx, sr_root.handle(), c"nodeName", "#document-fragment");
    set_prop_str(&mut cx, sr_root.handle(), c"mode", &mode);
    set_prop_bool(&mut cx, sr_root.handle(), c"delegatesFocus", false);
    // ShadyDOM's attachShadow wrapper returns its own root object `k` and
    // hybrid-mode callers (Polymer's `_attachDom`) do `k.shadowRoot.appendChild`,
    // expecting `k` to expose the "native" root it wraps. Self-reference so
    // that path lands on the same host-proxying object either way.
    set_prop_obj(&mut cx, sr_root.handle(), c"shadowRoot", sr);
    define_accessor(
        &mut cx,
        sr_root.handle(),
        c"innerHTML",
        Some(get_inner_html),
        Some(set_inner_html),
    );
    define_accessor(
        &mut cx,
        sr_root.handle(),
        c"textContent",
        Some(get_text_content),
        Some(set_text_content),
    );
    define_fn(
        &mut cx,
        sr_root.handle(),
        c"appendChild",
        Some(node_append_child),
        1,
    );
    define_fn(
        &mut cx,
        sr_root.handle(),
        c"insertBefore",
        Some(node_insert_before),
        2,
    );
    define_fn(
        &mut cx,
        sr_root.handle(),
        c"removeChild",
        Some(node_remove_child),
        1,
    );
    define_fn(
        &mut cx,
        sr_root.handle(),
        c"append",
        Some(node_append_child),
        1,
    );
    define_fn(
        &mut cx,
        sr_root.handle(),
        c"querySelector",
        Some(node_query_selector),
        1,
    );
    define_fn(
        &mut cx,
        sr_root.handle(),
        c"querySelectorAll",
        Some(node_query_selector_all),
        1,
    );
    define_fn(
        &mut cx,
        sr_root.handle(),
        c"addEventListener",
        Some(node_add_event_listener),
        3,
    );
    define_fn(
        &mut cx,
        sr_root.handle(),
        c"removeEventListener",
        Some(node_remove_event_listener),
        3,
    );
    define_getter(
        &mut cx,
        sr_root.handle(),
        c"firstChild",
        Some(get_first_child),
    );
    define_getter(&mut cx, sr_root.handle(), c"children", Some(get_children));
    define_getter(
        &mut cx,
        sr_root.handle(),
        c"childNodes",
        Some(get_child_nodes),
    );

    // host <-> shadowRoot back-references
    rooted!(&in(cx) let host_val = args.thisv().get());
    wrappers2::JS_SetProperty(
        &mut cx,
        sr_root.handle(),
        c"host".as_ptr(),
        host_val.handle(),
    );
    if host_val.get().is_object() {
        let host_obj = host_val.get().to_object_or_null();
        rooted!(&in(cx) let host_root = host_obj);
        set_prop_obj(&mut cx, host_root.handle(), c"shadowRoot", sr);
        // ShadyDOM (noPatch mode, `inUse=true`) wraps every node and exposes
        // its OWN `attachShadow`/`shadowRoot` that proxy to
        // `node.__shady_attachShadow`/`node.__shady_shadowRoot` rather than
        // the plain `shadowRoot` property — without this, `wrap(host).shadowRoot`
        // stays undefined even after `__shady_attachShadow` runs, and Polymer's
        // `_attachDom` throws on `k.shadowRoot.appendChild`.
        set_prop_obj(&mut cx, host_root.handle(), c"__shady_shadowRoot", sr);
    }

    args.rval().set(ObjectValue(sr));
    true
}

unsafe extern "C" fn return_animation_obj(
    cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let obj = new_plain_object(&mut cx);
    rooted!(&in(cx) let obj_root = obj);
    define_fn(&mut cx, obj_root.handle(), c"cancel", Some(noop_cb), 0);
    define_fn(&mut cx, obj_root.handle(), c"finish", Some(noop_cb), 0);
    define_fn(&mut cx, obj_root.handle(), c"pause", Some(noop_cb), 0);
    define_fn(&mut cx, obj_root.handle(), c"play", Some(noop_cb), 0);
    define_fn(&mut cx, obj_root.handle(), c"then", Some(noop_cb), 2);
    define_fn(&mut cx, obj_root.handle(), c"catch", Some(noop_cb), 1);
    set_prop_str(&mut cx, obj_root.handle(), c"playState", "finished");
    args.rval().set(ObjectValue(obj));
    true
}

unsafe extern "C" fn class_list_contains(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    // Access this.value to get the class string
    let this_val = args.thisv().get();
    let result = if this_val.is_object() {
        let this_obj = this_val.to_object_or_null();
        rooted!(&in(cx) let this_root = this_obj);
        rooted!(&in(cx) let mut val = UndefinedValue());
        let target = arg_to_string(&mut cx, &args, 0);
        if !target.is_empty()
            && wrappers2::JS_GetProperty(
                &mut cx,
                this_root.handle(),
                c"value".as_ptr(),
                val.handle_mut(),
            )
            && val.get().is_string()
        {
            let raw = val.get().to_string();
            if !raw.is_null() {
                use mozjs::conversions::jsstr_to_string;
                let s = jsstr_to_string(&cx, NonNull::new_unchecked(raw));
                s.split_whitespace().any(|c| c == target)
            } else {
                false
            }
        } else {
            false
        }
    } else {
        false
    };
    args.rval().set(BooleanValue(result));
    true
}

// ── Node method callbacks ─────────────────────────────────────────────────────

unsafe extern "C" fn node_add_event_listener(
    cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);

    if argc < 2 {
        args.rval().set(UndefinedValue());
        return true;
    }

    let event_type = arg_to_string(&mut cx, &args, 0);
    let cb_val = args.get(1).get();
    if !cb_val.is_object() {
        args.rval().set(UndefinedValue());
        return true;
    }

    let node_id = node_id_from_this(&mut cx, &args).unwrap_or(0);
    let state = &mut *get_state_ptr(&cx);
    let cb_id = state.window.next_id();

    rooted!(&in(cx) let cb_handle = cb_val);
    rooted!(&in(cx) let global = state.global);
    store_callback(&mut cx, global.handle(), cb_id, cb_handle.handle());
    state.registry.add_listener(node_id, event_type, cb_id);

    args.rval().set(UndefinedValue());
    true
}

unsafe extern "C" fn node_remove_event_listener(
    _cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    let args = CallArgs::from_vp(vp, argc);
    args.rval().set(UndefinedValue());
    true
}

unsafe extern "C" fn node_get_attribute(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let attr_name = arg_to_string(&mut cx, &args, 0);
    let node_id = node_id_from_this(&mut cx, &args).unwrap_or(0);

    let state = &*get_state_ptr(&cx);
    let result = state
        .registry
        .lookup(node_id)
        .and_then(|node| match &*node.borrow() {
            Node::Element(el) => el.attributes.get(&attr_name).cloned(),
            _ => None,
        });

    match result {
        Some(val) => {
            let js_str = new_js_string(&mut cx, &val);
            args.rval().set(if js_str.is_null() {
                NullValue()
            } else {
                mozjs::jsval::StringValue(&*js_str)
            });
        }
        None => args.rval().set(NullValue()),
    }
    true
}

unsafe extern "C" fn node_set_attribute(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let attr_name = arg_to_string(&mut cx, &args, 0);
    let attr_val = arg_to_string(&mut cx, &args, 1);
    let node_id = node_id_from_this(&mut cx, &args).unwrap_or(0);

    let state = &mut *get_state_ptr(&cx);
    if let Some(node) = state.registry.lookup(node_id) {
        if let Node::Element(el) = &mut *node.borrow_mut() {
            el.attributes.insert(attr_name.clone(), attr_val);
            state.registry.mark_needs_reflow();
        }
    }
    queue_attribute_mutation(state, node_id, &attr_name);
    args.rval().set(UndefinedValue());
    true
}

unsafe extern "C" fn node_remove_attribute(
    cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let attr_name = arg_to_string(&mut cx, &args, 0);
    let node_id = node_id_from_this(&mut cx, &args).unwrap_or(0);

    let state = &mut *get_state_ptr(&cx);
    if let Some(node) = state.registry.lookup(node_id) {
        if let Node::Element(el) = &mut *node.borrow_mut() {
            el.attributes.remove(&attr_name);
            state.registry.mark_needs_reflow();
        }
    }
    queue_attribute_mutation(state, node_id, &attr_name);
    args.rval().set(UndefinedValue());
    true
}

unsafe extern "C" fn node_has_attribute(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let attr_name = arg_to_string(&mut cx, &args, 0);
    let node_id = node_id_from_this(&mut cx, &args).unwrap_or(0);

    let state = &*get_state_ptr(&cx);
    let has = state.registry.lookup(node_id).map_or(false, |node| {
        matches!(&*node.borrow(), Node::Element(el) if el.attributes.contains_key(&attr_name))
    });
    args.rval().set(BooleanValue(has));
    true
}

unsafe extern "C" fn node_has_attributes(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let node_id = node_id_from_this(&mut cx, &args).unwrap_or(0);

    let state = &*get_state_ptr(&cx);
    let has = state.registry.lookup(node_id).map_or(
        false,
        |node| matches!(&*node.borrow(), Node::Element(el) if !el.attributes.is_empty()),
    );
    args.rval().set(BooleanValue(has));
    true
}

unsafe extern "C" fn node_append_child(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);

    if argc == 0 {
        args.rval().set(NullValue());
        return true;
    }

    // Get child node_id from the first argument
    let child_val = args.get(0).get();
    let child_node_id = if child_val.is_object() {
        let child_obj = child_val.to_object_or_null();
        rooted!(&in(cx) let child_root = child_obj);
        rooted!(&in(cx) let mut id_v = UndefinedValue());
        if wrappers2::JS_GetProperty(
            &mut cx,
            child_root.handle(),
            NODE_ID_PROP,
            id_v.handle_mut(),
        ) && id_v.get().is_number()
        {
            Some(id_v.get().to_number() as u32)
        } else {
            None
        }
    } else {
        None
    };

    let parent_id = node_id_from_this(&mut cx, &args).unwrap_or(0);
    let state = &mut *get_state_ptr(&cx);

    if let (Some(child_id), Some(parent_node)) = (child_node_id, state.registry.lookup(parent_id)) {
        if let Some(child_node) = state.registry.lookup(child_id) {
            let is_fragment = if child_val.is_object() {
                let child_obj = child_val.to_object_or_null();
                rooted!(&in(cx) let child_root = child_obj);
                get_prop_i32(&mut cx, child_root.handle(), c"nodeType") == 11
            } else {
                false
            };
            let child_tag = debug_tag_for_node(&child_node);
            if is_fragment {
                let fragment_children = clone_fragment_children(&child_node);
                for fragment_child in fragment_children {
                    append_child_node(&mut cx, &parent_node, parent_id, fragment_child);
                }
                args.rval().set(args.get(0).get());
                return true;
            }
            match &mut *parent_node.borrow_mut() {
                Node::Element(el) => {
                    el.children.push(child_node);
                    state.registry.mark_needs_reflow();
                }
                Node::Document { children, .. } => {
                    children.push(child_node);
                }
                _ => {}
            }
            if debug_youtube_enabled() {
                if let Some(tag) = child_tag.filter(|tag| tag.contains('-')) {
                    eprintln!("JS: [yt-dom] appendChild {tag}");
                }
            }
            queue_childlist_mutation(state, parent_id, vec![child_id], vec![]);
        }
    }

    call_connected_callback(&mut cx, child_val);
    args.rval().set(args.get(0).get());
    true
}

unsafe extern "C" fn node_insert_before(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    if argc == 0 {
        args.rval().set(NullValue());
        return true;
    }

    let child_id = val_to_node_id(&mut cx, args.get(0).get());
    let ref_id = if argc >= 2 {
        let v = args.get(1).get();
        if v.is_null() || v.is_undefined() {
            None
        } else {
            val_to_node_id(&mut cx, v)
        }
    } else {
        None
    };
    let parent_id = node_id_from_this(&mut cx, &args).unwrap_or(0);

    let state = &mut *get_state_ptr(&cx);
    if let (Some(child_id), Some(parent)) = (child_id, state.registry.lookup(parent_id)) {
        if let Some(child) = state.registry.lookup(child_id) {
            let child_tag = debug_tag_for_node(&child);
            let ref_node = ref_id.and_then(|id| state.registry.lookup(id));
            let mut p = parent.borrow_mut();
            let kids: &mut Vec<NodePtr> = match &mut *p {
                Node::Element(el) => &mut el.children,
                Node::Document { children, .. } => children,
                _ => {
                    args.rval().set(args.get(0).get());
                    return true;
                }
            };
            match ref_node.and_then(|r| kids.iter().position(|c| Rc::ptr_eq(c, &r))) {
                Some(pos) => kids.insert(pos, child),
                None => kids.push(child),
            }
            drop(p);
            state.registry.mark_needs_reflow();
            if debug_youtube_enabled() {
                if let Some(tag) = child_tag.filter(|tag| tag.contains('-')) {
                    eprintln!("JS: [yt-dom] insertBefore {tag}");
                }
            }
            queue_childlist_mutation(state, parent_id, vec![child_id], vec![]);
        }
    }
    call_connected_callback(&mut cx, args.get(0).get());
    args.rval().set(args.get(0).get());
    true
}

unsafe extern "C" fn node_remove_child(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);

    if argc == 0 {
        args.rval().set(NullValue());
        return true;
    }

    let child_val = args.get(0).get();
    let child_node_id = if child_val.is_object() {
        let child_obj = child_val.to_object_or_null();
        rooted!(&in(cx) let child_root = child_obj);
        rooted!(&in(cx) let mut id_v = UndefinedValue());
        if wrappers2::JS_GetProperty(
            &mut cx,
            child_root.handle(),
            NODE_ID_PROP,
            id_v.handle_mut(),
        ) && id_v.get().is_number()
        {
            Some(id_v.get().to_number() as u32)
        } else {
            None
        }
    } else {
        None
    };

    let parent_id = node_id_from_this(&mut cx, &args).unwrap_or(0);
    let state = &mut *get_state_ptr(&cx);

    if let (Some(child_id), Some(parent_node)) = (child_node_id, state.registry.lookup(parent_id)) {
        if let Some(child_node) = state.registry.lookup(child_id) {
            match &mut *parent_node.borrow_mut() {
                Node::Element(el) => {
                    el.children.retain(|c| !Rc::ptr_eq(c, &child_node));
                    state.registry.mark_needs_reflow();
                }
                Node::Document { children, .. } => {
                    children.retain(|c| !Rc::ptr_eq(c, &child_node));
                }
                _ => {}
            }
            queue_childlist_mutation(state, parent_id, vec![], vec![child_id]);
        }
    }

    args.rval().set(child_val);
    true
}

unsafe extern "C" fn node_replace_child(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    if argc < 2 {
        args.rval().set(NullValue());
        return true;
    }

    let new_id = val_to_node_id(&mut cx, args.get(0).get());
    let old_id = val_to_node_id(&mut cx, args.get(1).get());
    let parent_id = node_id_from_this(&mut cx, &args).unwrap_or(0);

    let state = &mut *get_state_ptr(&cx);
    if let (Some(new_id), Some(old_id), Some(parent)) =
        (new_id, old_id, state.registry.lookup(parent_id))
    {
        if let (Some(new_node), Some(old_node)) =
            (state.registry.lookup(new_id), state.registry.lookup(old_id))
        {
            let mut p = parent.borrow_mut();
            let kids: &mut Vec<NodePtr> = match &mut *p {
                Node::Element(el) => &mut el.children,
                Node::Document { children, .. } => children,
                _ => {
                    args.rval().set(args.get(1).get());
                    return true;
                }
            };
            if let Some(pos) = kids.iter().position(|c| Rc::ptr_eq(c, &old_node)) {
                kids[pos] = new_node;
            }
            drop(p);
            state.registry.mark_needs_reflow();
            queue_childlist_mutation(state, parent_id, vec![new_id], vec![old_id]);
        }
    }
    call_connected_callback(&mut cx, args.get(0).get());
    args.rval().set(args.get(1).get());
    true
}

unsafe fn call_connected_callback(cx: &mut JSContext, node_val: Value) {
    if !node_val.is_object() {
        return;
    }
    let node_obj = node_val.to_object_or_null();
    if node_obj.is_null() {
        return;
    }

    rooted!(&in(cx) let node_root = node_obj);
    let tag_name = get_prop_string(cx, node_root.handle(), c"localName")
        .or_else(|| get_prop_string(cx, node_root.handle(), c"tagName"));

    rooted!(&in(cx) let mut connected = UndefinedValue());
    if wrappers2::JS_GetProperty(
        cx,
        node_root.handle(),
        c"__ce_connected__".as_ptr(),
        connected.handle_mut(),
    ) && connected.get().is_boolean()
        && connected.get().to_boolean()
    {
        return;
    }

    rooted!(&in(cx) let mut callback = UndefinedValue());
    if !wrappers2::JS_GetProperty(
        cx,
        node_root.handle(),
        c"connectedCallback".as_ptr(),
        callback.handle_mut(),
    ) || !callback.get().is_object()
    {
        if let Some(tag) = tag_name.as_deref().filter(|t| t.contains('-')) {
            log::debug!("[ce] appendChild <{tag}> — no connectedCallback (not upgraded yet?)");
        }
        return;
    }

    set_prop_bool(cx, node_root.handle(), c"__ce_connected__", true);
    log::info!(
        "[ce] connectedCallback firing on <{}>",
        tag_name.as_deref().unwrap_or("?")
    );

    rooted!(&in(cx) let mut rval = UndefinedValue());
    let empty = HandleValueArray::empty();
    let ok = wrappers2::JS_CallFunctionValue(
        cx,
        node_root.handle(),
        callback.handle(),
        &empty,
        rval.handle_mut(),
    );
    if !ok {
        let err = pending_exception_string(cx);
        log::error!(
            "[ce] connectedCallback <{}> threw: {}",
            tag_name.as_deref().unwrap_or("?"),
            err
        );
        clear_pending_exception(cx);
    }
}

unsafe extern "C" fn node_clone_node(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let deep = argc > 0 && args.get(0).get().to_boolean();
    let node_id = match node_id_from_this(&mut cx, &args) {
        Some(id) => id,
        None => {
            args.rval().set(NullValue());
            return true;
        }
    };
    let this_is_fragment = if args.thisv().get().is_object() {
        let this_obj = args.thisv().get().to_object_or_null();
        rooted!(&in(cx) let this_root = this_obj);
        get_prop_i32(&mut cx, this_root.handle(), c"nodeType") == 11
    } else {
        false
    };
    let cloned = {
        let state = &*get_state_ptr(&cx);
        state.registry.lookup(node_id).map(|n| {
            if this_is_fragment {
                let children = if deep {
                    clone_fragment_children(&n)
                } else {
                    vec![]
                };
                crate::dom::Node::element("#document-fragment", children)
            } else {
                clone_node_rs(&n, deep)
            }
        })
    };
    match cloned {
        Some(ptr) => {
            let obj = create_js_node(&mut cx, ptr);
            args.rval().set(ObjectValue(obj));
        }
        None => {
            args.rval().set(NullValue());
        }
    }
    true
}

unsafe extern "C" fn node_contains(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    if argc == 0 {
        args.rval().set(BooleanValue(false));
        return true;
    }
    let other_id = val_to_node_id(&mut cx, args.get(0).get());
    let this_id = node_id_from_this(&mut cx, &args).unwrap_or(0);
    let state = &*get_state_ptr(&cx);
    let result = match (other_id, state.registry.lookup(this_id)) {
        (Some(other_id), Some(this_node)) => state
            .registry
            .lookup(other_id)
            .map(|other| node_contains_node(&this_node, &other))
            .unwrap_or(false),
        _ => false,
    };
    args.rval().set(BooleanValue(result));
    true
}

unsafe extern "C" fn node_closest(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let selector = arg_to_string(&mut cx, &args, 0);
    let this_id = match node_id_from_this(&mut cx, &args) {
        Some(id) => id,
        None => {
            args.rval().set(NullValue());
            return true;
        }
    };
    let (this_node, doc, list) = {
        let state = &*get_state_ptr(&cx);
        (
            state.registry.lookup(this_id),
            state.registry.document.clone(),
            parse_selector_list(&selector),
        )
    };
    if let (Some(this_node), Some(doc), Some(list)) = (this_node, doc, list) {
        if let Some(matched) = find_matching_ancestor(&doc, &this_node, list.slice()) {
            let obj = create_js_node(&mut cx, matched);
            args.rval().set(ObjectValue(obj));
            return true;
        }
    }
    args.rval().set(NullValue());
    true
}

unsafe extern "C" fn node_matches(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let selector = arg_to_string(&mut cx, &args, 0);
    let node_id = node_id_from_this(&mut cx, &args).unwrap_or(0);

    let state = &*get_state_ptr(&cx);
    let matches = state.registry.lookup(node_id).map_or(false, |node| {
        if let Some(doc) = &state.registry.document {
            if let Some(list) = parse_selector_list(&selector) {
                return node_matches_selectors(&node, list.slice(), doc);
            }
        }
        false
    });
    args.rval().set(BooleanValue(matches));
    true
}

unsafe extern "C" fn node_query_selector(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let selector = arg_to_string(&mut cx, &args, 0);
    let node_id = node_id_from_this(&mut cx, &args).unwrap_or(0);

    let state = &mut *get_state_ptr(&cx);
    let search_root = state
        .registry
        .lookup(node_id)
        .or_else(|| state.registry.document.clone());

    if let Some(root) = search_root {
        if let Some(found) = query_first_from(&root, &selector) {
            let obj = create_js_node(&mut cx, found);
            args.rval().set(ObjectValue(obj));
            return true;
        }
    }
    args.rval().set(NullValue());
    true
}

unsafe extern "C" fn node_query_selector_all(
    cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let selector = arg_to_string(&mut cx, &args, 0);
    let node_id = node_id_from_this(&mut cx, &args).unwrap_or(0);

    let state = &mut *get_state_ptr(&cx);
    let search_root = state
        .registry
        .lookup(node_id)
        .or_else(|| state.registry.document.clone());

    if let Some(root) = search_root {
        let found = query_all_from(&root, &selector);
        let arr = build_nodelist(&mut cx, found);
        args.rval().set(if arr.is_null() {
            UndefinedValue()
        } else {
            ObjectValue(arr)
        });
    } else {
        let arr = wrappers2::NewArrayObject(&mut cx, &HandleValueArray::empty());
        args.rval().set(if arr.is_null() {
            UndefinedValue()
        } else {
            ObjectValue(arr)
        });
    }
    true
}

unsafe extern "C" fn node_get_elements_by_tag(
    cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let tag = arg_to_string(&mut cx, &args, 0).to_lowercase();
    let node_id = node_id_from_this(&mut cx, &args).unwrap_or(0);

    let state = &mut *get_state_ptr(&cx);
    let search_root = state
        .registry
        .lookup(node_id)
        .or_else(|| state.registry.document.clone());

    if let Some(root) = search_root {
        let mut acc = Vec::new();
        collect_by_tag(&root, &tag, &mut acc);
        let arr = build_nodelist(&mut cx, acc);
        args.rval().set(if arr.is_null() {
            UndefinedValue()
        } else {
            ObjectValue(arr)
        });
    } else {
        let arr = wrappers2::NewArrayObject(&mut cx, &HandleValueArray::empty());
        args.rval().set(if arr.is_null() {
            UndefinedValue()
        } else {
            ObjectValue(arr)
        });
    }
    true
}

unsafe extern "C" fn node_get_elements_by_class(
    cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let class = arg_to_string(&mut cx, &args, 0);
    let node_id = node_id_from_this(&mut cx, &args).unwrap_or(0);

    let state = &mut *get_state_ptr(&cx);
    let search_root = state
        .registry
        .lookup(node_id)
        .or_else(|| state.registry.document.clone());

    if let Some(root) = search_root {
        let mut acc = Vec::new();
        collect_by_class(&root, &class, &mut acc);
        let arr = build_nodelist(&mut cx, acc);
        args.rval().set(if arr.is_null() {
            UndefinedValue()
        } else {
            ObjectValue(arr)
        });
    } else {
        let arr = wrappers2::NewArrayObject(&mut cx, &HandleValueArray::empty());
        args.rval().set(if arr.is_null() {
            UndefinedValue()
        } else {
            ObjectValue(arr)
        });
    }
    true
}

unsafe extern "C" fn node_get_bounding_rect(
    cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let obj = new_plain_object(&mut cx);
    rooted!(&in(cx) let obj_root = obj);
    for name in &[
        c"x", c"y", c"top", c"left", c"bottom", c"right", c"width", c"height",
    ] {
        set_prop_f64(&mut cx, obj_root.handle(), name, 0.0);
    }
    define_fn(&mut cx, obj_root.handle(), c"toJSON", Some(noop_cb), 0);
    args.rval().set(ObjectValue(obj));
    true
}

// ── Document global callbacks ─────────────────────────────────────────────────

pub(super) unsafe extern "C" fn doc_get_element_by_id(
    cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let id = arg_to_string(&mut cx, &args, 0);

    let state = &mut *get_state_ptr(&cx);
    if let Some(doc) = state.registry.document.clone() {
        if let Some(found) = find_by_id(&doc, &id) {
            let obj = create_js_node(&mut cx, found);
            args.rval().set(ObjectValue(obj));
            return true;
        }
    }
    args.rval().set(NullValue());
    true
}

pub(super) unsafe extern "C" fn doc_query_selector(
    cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let selector = arg_to_string(&mut cx, &args, 0);

    let state = &mut *get_state_ptr(&cx);
    if let Some(doc) = state.registry.document.clone() {
        if let Some(found) = query_first_from(&doc, &selector) {
            let obj = create_js_node(&mut cx, found);
            args.rval().set(ObjectValue(obj));
            return true;
        }
    }
    args.rval().set(NullValue());
    true
}

pub(super) unsafe extern "C" fn doc_query_selector_all(
    cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let selector = arg_to_string(&mut cx, &args, 0);

    let state = &mut *get_state_ptr(&cx);
    if let Some(doc) = state.registry.document.clone() {
        let found = query_all_from(&doc, &selector);
        let arr = build_nodelist(&mut cx, found);
        args.rval().set(if arr.is_null() {
            UndefinedValue()
        } else {
            ObjectValue(arr)
        });
    } else {
        let arr = wrappers2::NewArrayObject(&mut cx, &HandleValueArray::empty());
        args.rval().set(if arr.is_null() {
            UndefinedValue()
        } else {
            ObjectValue(arr)
        });
    }
    true
}

pub(super) unsafe extern "C" fn doc_get_elements_by_tag(
    cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let tag = arg_to_string(&mut cx, &args, 0).to_lowercase();

    let state = &mut *get_state_ptr(&cx);
    if let Some(doc) = state.registry.document.clone() {
        let mut acc = Vec::new();
        collect_by_tag(&doc, &tag, &mut acc);
        let arr = build_nodelist(&mut cx, acc);
        args.rval().set(if arr.is_null() {
            UndefinedValue()
        } else {
            ObjectValue(arr)
        });
    } else {
        let arr = wrappers2::NewArrayObject(&mut cx, &HandleValueArray::empty());
        args.rval().set(if arr.is_null() {
            UndefinedValue()
        } else {
            ObjectValue(arr)
        });
    }
    true
}

pub(super) unsafe extern "C" fn doc_get_elements_by_class(
    cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let class = arg_to_string(&mut cx, &args, 0);

    let state = &mut *get_state_ptr(&cx);
    if let Some(doc) = state.registry.document.clone() {
        let mut acc = Vec::new();
        collect_by_class(&doc, &class, &mut acc);
        let arr = build_nodelist(&mut cx, acc);
        args.rval().set(if arr.is_null() {
            UndefinedValue()
        } else {
            ObjectValue(arr)
        });
    } else {
        let arr = wrappers2::NewArrayObject(&mut cx, &HandleValueArray::empty());
        args.rval().set(if arr.is_null() {
            UndefinedValue()
        } else {
            ObjectValue(arr)
        });
    }
    true
}

pub(super) unsafe extern "C" fn doc_current_script_getter(
    cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let state = &mut *get_state_ptr(&cx);
    let Some(node_id) = state.current_script_node_id else {
        args.rval().set(NullValue());
        return true;
    };
    let Some(node) = state.registry.lookup(node_id) else {
        args.rval().set(NullValue());
        return true;
    };
    let obj = create_js_node(&mut cx, node);
    args.rval().set(ObjectValue(obj));
    true
}

pub(super) unsafe extern "C" fn doc_create_element(
    cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let tag = arg_to_string(&mut cx, &args, 0).to_lowercase();
    let node = Node::element(tag, vec![]);
    let obj = create_js_node(&mut cx, node);
    args.rval().set(ObjectValue(obj));
    true
}

pub(super) unsafe extern "C" fn doc_create_element_ns(
    cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let tag = arg_to_string(&mut cx, &args, 1).to_lowercase();
    let node = Node::element(tag, vec![]);
    let obj = create_js_node(&mut cx, node);
    args.rval().set(ObjectValue(obj));
    true
}

pub(super) unsafe extern "C" fn doc_create_text_node(
    cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let text = arg_to_string(&mut cx, &args, 0);
    let node = Node::text(text);
    let obj = create_js_node(&mut cx, node);
    args.rval().set(ObjectValue(obj));
    true
}

pub(super) unsafe extern "C" fn doc_create_fragment(
    cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let node = Node::element("#document-fragment", vec![]);
    let obj = create_js_node(&mut cx, node);
    args.rval().set(ObjectValue(obj));
    true
}

/// `document.importNode(node, deep)` — adopts a node into this document.
/// Aurora has one document, so this is equivalent to `node.cloneNode(deep)`.
pub(super) unsafe extern "C" fn doc_import_node(
    cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    if argc == 0 || !args.get(0).get().is_object() {
        args.rval().set(NullValue());
        return true;
    }
    let deep = argc > 1 && args.get(1).get().to_boolean();
    let src_obj = args.get(0).get().to_object_or_null();
    rooted!(&in(cx) let src_root = src_obj);
    rooted!(&in(cx) let mut id_val = UndefinedValue());
    if !wrappers2::JS_GetProperty(
        &mut cx,
        src_root.handle(),
        NODE_ID_PROP,
        id_val.handle_mut(),
    ) || !id_val.get().is_number()
    {
        args.rval().set(NullValue());
        return true;
    }
    let node_id = id_val.get().to_number() as u32;
    let cloned = {
        let state = &*get_state_ptr(&cx);
        state
            .registry
            .lookup(node_id)
            .map(|n| clone_node_rs(&n, deep))
    };
    match cloned {
        Some(ptr) => {
            let obj = create_js_node(&mut cx, ptr);
            args.rval().set(ObjectValue(obj));
        }
        None => {
            args.rval().set(NullValue());
        }
    }
    true
}

pub(super) unsafe extern "C" fn doc_create_event(
    cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let obj = new_plain_object(&mut cx);
    rooted!(&in(cx) let obj_root = obj);
    set_prop_str(&mut cx, obj_root.handle(), c"type", "");
    set_prop_bool(&mut cx, obj_root.handle(), c"bubbles", false);
    set_prop_bool(&mut cx, obj_root.handle(), c"cancelable", false);
    set_prop_bool(&mut cx, obj_root.handle(), c"defaultPrevented", false);
    set_prop_null(&mut cx, obj_root.handle(), c"detail");
    define_fn(
        &mut cx,
        obj_root.handle(),
        c"initEvent",
        Some(doc_init_event),
        3,
    );
    define_fn(
        &mut cx,
        obj_root.handle(),
        c"initCustomEvent",
        Some(doc_init_custom_event),
        4,
    );
    define_fn(
        &mut cx,
        obj_root.handle(),
        c"preventDefault",
        Some(noop_cb),
        0,
    );
    define_fn(
        &mut cx,
        obj_root.handle(),
        c"stopPropagation",
        Some(noop_cb),
        0,
    );
    define_fn(
        &mut cx,
        obj_root.handle(),
        c"stopImmediatePropagation",
        Some(noop_cb),
        0,
    );
    args.rval().set(ObjectValue(obj));
    true
}

unsafe extern "C" fn doc_init_event(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    rooted!(&in(cx) let this_val = args.thisv().get());
    init_created_event(&mut cx, this_val.handle(), &args, false);
    args.rval().set(UndefinedValue());
    true
}

unsafe extern "C" fn doc_init_custom_event(
    cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    rooted!(&in(cx) let this_val = args.thisv().get());
    init_created_event(&mut cx, this_val.handle(), &args, true);
    args.rval().set(UndefinedValue());
    true
}

unsafe fn init_created_event(
    cx: &mut JSContext,
    this_val: mozjs::gc::Handle<Value>,
    args: &CallArgs,
    include_detail: bool,
) {
    if !this_val.get().is_object() {
        return;
    }
    let obj = this_val.get().to_object_or_null();
    if obj.is_null() {
        return;
    }
    rooted!(&in(cx) let obj_root = obj);
    let event_type = arg_to_string(cx, args, 0);
    set_prop_str(cx, obj_root.handle(), c"type", &event_type);
    set_prop_bool(
        cx,
        obj_root.handle(),
        c"bubbles",
        args.argc_ > 1 && args.get(1).get().to_boolean(),
    );
    set_prop_bool(
        cx,
        obj_root.handle(),
        c"cancelable",
        args.argc_ > 2 && args.get(2).get().to_boolean(),
    );
    if include_detail && args.argc_ > 3 {
        rooted!(&in(cx) let detail = args.get(3).get());
        wrappers2::JS_SetProperty(cx, obj_root.handle(), c"detail".as_ptr(), detail.handle());
    }
}

pub(super) unsafe extern "C" fn doc_create_range(
    cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let obj = new_plain_object(&mut cx);
    rooted!(&in(cx) let obj_root = obj);
    set_prop_bool(&mut cx, obj_root.handle(), c"collapsed", true);
    set_prop_i32(&mut cx, obj_root.handle(), c"startOffset", 0);
    set_prop_i32(&mut cx, obj_root.handle(), c"endOffset", 0);
    set_prop_null(&mut cx, obj_root.handle(), c"startContainer");
    set_prop_null(&mut cx, obj_root.handle(), c"endContainer");
    for name in &[
        c"setStart",
        c"setEnd",
        c"setStartBefore",
        c"setEndAfter",
        c"collapse",
        c"selectNode",
        c"selectNodeContents",
        c"deleteContents",
        c"detach",
        c"surroundContents",
        c"insertNode",
    ] {
        define_fn(&mut cx, obj_root.handle(), name, Some(noop_cb), 2);
    }
    define_fn(
        &mut cx,
        obj_root.handle(),
        c"getBoundingClientRect",
        Some(node_get_bounding_rect),
        0,
    );
    define_fn(
        &mut cx,
        obj_root.handle(),
        c"getClientRects",
        Some(return_empty_array_cb),
        0,
    );
    args.rval().set(ObjectValue(obj));
    true
}

pub(super) unsafe extern "C" fn doc_create_tree_walker(
    cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let obj = new_plain_object(&mut cx);
    rooted!(&in(cx) let obj_root = obj);
    if argc > 0 {
        rooted!(&in(cx) let root = args.get(0).get());
        wrappers2::JS_SetProperty(&mut cx, obj_root.handle(), c"root".as_ptr(), root.handle());
        wrappers2::JS_SetProperty(
            &mut cx,
            obj_root.handle(),
            c"currentNode".as_ptr(),
            root.handle(),
        );
    } else {
        set_prop_null(&mut cx, obj_root.handle(), c"root");
        set_prop_null(&mut cx, obj_root.handle(), c"currentNode");
    }
    define_fn(
        &mut cx,
        obj_root.handle(),
        c"parentNode",
        Some(return_null_cb),
        0,
    );
    define_fn(
        &mut cx,
        obj_root.handle(),
        c"firstChild",
        Some(return_null_cb),
        0,
    );
    define_fn(
        &mut cx,
        obj_root.handle(),
        c"lastChild",
        Some(return_null_cb),
        0,
    );
    define_fn(
        &mut cx,
        obj_root.handle(),
        c"previousSibling",
        Some(return_null_cb),
        0,
    );
    define_fn(
        &mut cx,
        obj_root.handle(),
        c"nextSibling",
        Some(return_null_cb),
        0,
    );
    define_fn(
        &mut cx,
        obj_root.handle(),
        c"previousNode",
        Some(return_null_cb),
        0,
    );
    define_fn(
        &mut cx,
        obj_root.handle(),
        c"nextNode",
        Some(return_null_cb),
        0,
    );
    args.rval().set(ObjectValue(obj));
    true
}

pub(super) unsafe extern "C" fn doc_add_event_listener(
    cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);

    if argc < 2 {
        args.rval().set(UndefinedValue());
        return true;
    }

    let event_type = arg_to_string(&mut cx, &args, 0);
    let cb_val = args.get(1).get();
    if !cb_val.is_object() {
        args.rval().set(UndefinedValue());
        return true;
    }

    let state = &mut *get_state_ptr(&cx);
    let cb_id = state.window.next_id();

    rooted!(&in(cx) let cb_handle = cb_val);
    rooted!(&in(cx) let global = state.global);
    store_callback(&mut cx, global.handle(), cb_id, cb_handle.handle());
    // Store as document listener (node_id = 0 reserved for document)
    state.registry.add_listener(0, event_type, cb_id);

    args.rval().set(UndefinedValue());
    true
}

// ── Tree-traversal native getters ─────────────────────────────────────────────

unsafe extern "C" fn get_parent_node(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let node_id = match node_id_from_this(&mut cx, &args) {
        Some(id) => id,
        None => {
            args.rval().set(NullValue());
            return true;
        }
    };
    let (this_node, doc) = {
        let state = &*get_state_ptr(&cx);
        (
            state.registry.lookup(node_id),
            state.registry.document.clone(),
        )
    };
    if let (Some(this_node), Some(doc)) = (this_node, doc) {
        if let Some(parent) = find_parent(&doc, &this_node) {
            let obj = create_js_node(&mut cx, parent);
            args.rval().set(ObjectValue(obj));
            return true;
        }
    }
    args.rval().set(NullValue());
    true
}

unsafe extern "C" fn get_parent_element(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let node_id = match node_id_from_this(&mut cx, &args) {
        Some(id) => id,
        None => {
            args.rval().set(NullValue());
            return true;
        }
    };
    let (this_node, doc) = {
        let state = &*get_state_ptr(&cx);
        (
            state.registry.lookup(node_id),
            state.registry.document.clone(),
        )
    };
    if let (Some(this_node), Some(doc)) = (this_node, doc) {
        if let Some(parent) = find_parent(&doc, &this_node) {
            if matches!(&*parent.borrow(), Node::Element(_)) {
                let obj = create_js_node(&mut cx, parent);
                args.rval().set(ObjectValue(obj));
                return true;
            }
        }
    }
    args.rval().set(NullValue());
    true
}

unsafe extern "C" fn get_child_nodes(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let node_id = match node_id_from_this(&mut cx, &args) {
        Some(id) => id,
        None => {
            let empty = wrappers2::NewArrayObject(&mut cx, &HandleValueArray::empty());
            args.rval().set(if empty.is_null() {
                UndefinedValue()
            } else {
                ObjectValue(empty)
            });
            return true;
        }
    };
    let children = {
        let state = &*get_state_ptr(&cx);
        state
            .registry
            .lookup(node_id)
            .map(|n| match &*n.borrow() {
                Node::Element(el) => el.children.clone(),
                Node::Document { children, .. } => children.clone(),
                _ => vec![],
            })
            .unwrap_or_default()
    };
    let arr = build_nodelist(&mut cx, children);
    args.rval().set(if arr.is_null() {
        UndefinedValue()
    } else {
        ObjectValue(arr)
    });
    true
}

unsafe extern "C" fn get_children(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let node_id = match node_id_from_this(&mut cx, &args) {
        Some(id) => id,
        None => {
            let empty = wrappers2::NewArrayObject(&mut cx, &HandleValueArray::empty());
            args.rval().set(if empty.is_null() {
                UndefinedValue()
            } else {
                ObjectValue(empty)
            });
            return true;
        }
    };
    let children = {
        let state = &*get_state_ptr(&cx);
        state
            .registry
            .lookup(node_id)
            .map(|n| {
                let all = match &*n.borrow() {
                    Node::Element(el) => el.children.clone(),
                    Node::Document { children, .. } => children.clone(),
                    _ => return vec![],
                };
                all.into_iter()
                    .filter(|c| matches!(&*c.borrow(), Node::Element(_)))
                    .collect()
            })
            .unwrap_or_default()
    };
    let arr = build_nodelist(&mut cx, children);
    args.rval().set(if arr.is_null() {
        UndefinedValue()
    } else {
        ObjectValue(arr)
    });
    true
}

unsafe extern "C" fn get_child_element_count(
    cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let node_id = match node_id_from_this(&mut cx, &args) {
        Some(id) => id,
        None => {
            args.rval().set(mozjs::jsval::Int32Value(0));
            return true;
        }
    };
    let count = {
        let state = &*get_state_ptr(&cx);
        state
            .registry
            .lookup(node_id)
            .map(|n| match &*n.borrow() {
                Node::Element(el) => el
                    .children
                    .iter()
                    .filter(|c| matches!(&*c.borrow(), Node::Element(_)))
                    .count(),
                Node::Document { children, .. } => children
                    .iter()
                    .filter(|c| matches!(&*c.borrow(), Node::Element(_)))
                    .count(),
                _ => 0,
            })
            .unwrap_or(0)
    };
    args.rval().set(mozjs::jsval::Int32Value(count as i32));
    true
}

unsafe extern "C" fn get_first_child(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let node_id = match node_id_from_this(&mut cx, &args) {
        Some(id) => id,
        None => {
            args.rval().set(NullValue());
            return true;
        }
    };
    let first = {
        let state = &*get_state_ptr(&cx);
        state
            .registry
            .lookup(node_id)
            .and_then(|n| match &*n.borrow() {
                Node::Element(el) => el.children.first().cloned(),
                Node::Document { children, .. } => children.first().cloned(),
                _ => None,
            })
    };
    match first {
        Some(node) => {
            let obj = create_js_node(&mut cx, node);
            args.rval().set(ObjectValue(obj));
        }
        None => {
            args.rval().set(NullValue());
        }
    }
    true
}

unsafe extern "C" fn get_last_child(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let node_id = match node_id_from_this(&mut cx, &args) {
        Some(id) => id,
        None => {
            args.rval().set(NullValue());
            return true;
        }
    };
    let last = {
        let state = &*get_state_ptr(&cx);
        state
            .registry
            .lookup(node_id)
            .and_then(|n| match &*n.borrow() {
                Node::Element(el) => el.children.last().cloned(),
                Node::Document { children, .. } => children.last().cloned(),
                _ => None,
            })
    };
    match last {
        Some(node) => {
            let obj = create_js_node(&mut cx, node);
            args.rval().set(ObjectValue(obj));
        }
        None => {
            args.rval().set(NullValue());
        }
    }
    true
}

unsafe extern "C" fn get_first_element_child(
    cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let node_id = match node_id_from_this(&mut cx, &args) {
        Some(id) => id,
        None => {
            args.rval().set(NullValue());
            return true;
        }
    };
    let first = {
        let state = &*get_state_ptr(&cx);
        state.registry.lookup(node_id).and_then(|n| {
            let kids = match &*n.borrow() {
                Node::Element(el) => el.children.clone(),
                Node::Document { children, .. } => children.clone(),
                _ => return None,
            };
            kids.into_iter()
                .find(|c| matches!(&*c.borrow(), Node::Element(_)))
        })
    };
    match first {
        Some(node) => {
            let obj = create_js_node(&mut cx, node);
            args.rval().set(ObjectValue(obj));
        }
        None => {
            args.rval().set(NullValue());
        }
    }
    true
}

unsafe extern "C" fn get_last_element_child(
    cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let node_id = match node_id_from_this(&mut cx, &args) {
        Some(id) => id,
        None => {
            args.rval().set(NullValue());
            return true;
        }
    };
    let last = {
        let state = &*get_state_ptr(&cx);
        state.registry.lookup(node_id).and_then(|n| {
            let kids = match &*n.borrow() {
                Node::Element(el) => el.children.clone(),
                Node::Document { children, .. } => children.clone(),
                _ => return None,
            };
            kids.into_iter()
                .filter(|c| matches!(&*c.borrow(), Node::Element(_)))
                .last()
        })
    };
    match last {
        Some(node) => {
            let obj = create_js_node(&mut cx, node);
            args.rval().set(ObjectValue(obj));
        }
        None => {
            args.rval().set(NullValue());
        }
    }
    true
}

unsafe extern "C" fn get_next_sibling(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let node_id = match node_id_from_this(&mut cx, &args) {
        Some(id) => id,
        None => {
            args.rval().set(NullValue());
            return true;
        }
    };
    let sibling = {
        let state = &*get_state_ptr(&cx);
        state.registry.lookup(node_id).and_then(|this_node| {
            state
                .registry
                .document
                .as_ref()
                .and_then(|doc| get_sibling(doc, &this_node, 1, false))
        })
    };
    match sibling {
        Some(node) => {
            let obj = create_js_node(&mut cx, node);
            args.rval().set(ObjectValue(obj));
        }
        None => {
            args.rval().set(NullValue());
        }
    }
    true
}

unsafe extern "C" fn get_previous_sibling(
    cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let node_id = match node_id_from_this(&mut cx, &args) {
        Some(id) => id,
        None => {
            args.rval().set(NullValue());
            return true;
        }
    };
    let sibling = {
        let state = &*get_state_ptr(&cx);
        state.registry.lookup(node_id).and_then(|this_node| {
            state
                .registry
                .document
                .as_ref()
                .and_then(|doc| get_sibling(doc, &this_node, -1, false))
        })
    };
    match sibling {
        Some(node) => {
            let obj = create_js_node(&mut cx, node);
            args.rval().set(ObjectValue(obj));
        }
        None => {
            args.rval().set(NullValue());
        }
    }
    true
}

unsafe extern "C" fn get_next_element_sibling(
    cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let node_id = match node_id_from_this(&mut cx, &args) {
        Some(id) => id,
        None => {
            args.rval().set(NullValue());
            return true;
        }
    };
    let sibling = {
        let state = &*get_state_ptr(&cx);
        state.registry.lookup(node_id).and_then(|this_node| {
            state
                .registry
                .document
                .as_ref()
                .and_then(|doc| get_sibling(doc, &this_node, 1, true))
        })
    };
    match sibling {
        Some(node) => {
            let obj = create_js_node(&mut cx, node);
            args.rval().set(ObjectValue(obj));
        }
        None => {
            args.rval().set(NullValue());
        }
    }
    true
}

unsafe extern "C" fn get_previous_element_sibling(
    cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let node_id = match node_id_from_this(&mut cx, &args) {
        Some(id) => id,
        None => {
            args.rval().set(NullValue());
            return true;
        }
    };
    let sibling = {
        let state = &*get_state_ptr(&cx);
        state.registry.lookup(node_id).and_then(|this_node| {
            state
                .registry
                .document
                .as_ref()
                .and_then(|doc| get_sibling(doc, &this_node, -1, true))
        })
    };
    match sibling {
        Some(node) => {
            let obj = create_js_node(&mut cx, node);
            args.rval().set(ObjectValue(obj));
        }
        None => {
            args.rval().set(NullValue());
        }
    }
    true
}

fn get_sibling(
    root: &NodePtr,
    target: &NodePtr,
    delta: i32,
    element_only: bool,
) -> Option<NodePtr> {
    let parent = find_parent(root, target)?;
    let children = match &*parent.borrow() {
        Node::Element(el) => el.children.clone(),
        Node::Document { children, .. } => children.clone(),
        _ => return None,
    };
    let idx = children.iter().position(|c| Rc::ptr_eq(c, target))?;
    let mut i = idx as i32 + delta;
    while i >= 0 && (i as usize) < children.len() {
        let candidate = &children[i as usize];
        if !element_only || matches!(&*candidate.borrow(), Node::Element(_)) {
            return Some(candidate.clone());
        }
        i += delta;
    }
    None
}

// ── innerHTML / textContent accessors ─────────────────────────────────────────

unsafe extern "C" fn get_inner_html(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let node_id = match node_id_from_this(&mut cx, &args) {
        Some(id) => id,
        None => {
            args.rval().set(UndefinedValue());
            return true;
        }
    };
    let html = {
        let state = &*get_state_ptr(&cx);
        state
            .registry
            .lookup(node_id)
            .map(|n| match &*n.borrow() {
                Node::Element(el) => {
                    let children = if el.tag_name.eq_ignore_ascii_case("template") {
                        el.template_contents
                            .as_ref()
                            .and_then(|content| match &*content.borrow() {
                                Node::Element(fragment_el) => Some(fragment_el.children.clone()),
                                _ => None,
                            })
                    } else {
                        None
                    };
                    children
                        .unwrap_or_else(|| el.children.clone())
                        .iter()
                        .map(|c| crate::dom::serialize_outer_html(c))
                        .collect::<String>()
                }
                Node::Document { children, .. } => children
                    .iter()
                    .map(|c| crate::dom::serialize_outer_html(c))
                    .collect::<String>(),
                Node::Text(t) => t.clone(),
            })
            .unwrap_or_default()
    };
    let js_str = new_js_string(&mut cx, &html);
    args.rval().set(if js_str.is_null() {
        UndefinedValue()
    } else {
        mozjs::jsval::StringValue(unsafe { &*js_str })
    });
    true
}

unsafe extern "C" fn set_inner_html(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let html = arg_to_string(&mut cx, &args, 0);
    let node_id = match node_id_from_this(&mut cx, &args) {
        Some(id) => id,
        None => {
            args.rval().set(UndefinedValue());
            return true;
        }
    };
    let node = {
        let state = &*get_state_ptr(&cx);
        state.registry.lookup(node_id)
    };
    if let Some(node) = node {
        let fragment = crate::html::Parser::new(&format!("<body>{}</body>", html)).parse_document();
        let new_children = extract_body_children(&fragment);
        let template_content = match &mut *node.borrow_mut() {
            Node::Element(el) => {
                if el.tag_name.eq_ignore_ascii_case("template") {
                    // Per spec, template.innerHTML reads and writes the
                    // template's content fragment, not its light children.
                    Some(
                        el.template_contents
                            .get_or_insert_with(|| Node::document_fragment(Vec::new()))
                            .clone(),
                    )
                } else {
                    el.children = new_children.clone();
                    None
                }
            }
            Node::Document { children, .. } => {
                *children = new_children.clone();
                None
            }
            _ => None,
        };
        if let Some(content) = template_content {
            if let Node::Element(fragment_el) = &mut *content.borrow_mut() {
                fragment_el.children = new_children;
            }
        }
        let state = &mut *get_state_ptr(&cx);
        state.registry.mark_needs_reflow();
    }
    args.rval().set(UndefinedValue());
    true
}

unsafe extern "C" fn get_outer_html(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let node_id = match node_id_from_this(&mut cx, &args) {
        Some(id) => id,
        None => {
            args.rval().set(UndefinedValue());
            return true;
        }
    };
    let html = {
        let state = &*get_state_ptr(&cx);
        state
            .registry
            .lookup(node_id)
            .map(|n| crate::dom::serialize_outer_html(&n))
            .unwrap_or_default()
    };
    let js_str = new_js_string(&mut cx, &html);
    args.rval().set(if js_str.is_null() {
        UndefinedValue()
    } else {
        mozjs::jsval::StringValue(unsafe { &*js_str })
    });
    true
}

unsafe extern "C" fn get_text_content(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let node_id = match node_id_from_this(&mut cx, &args) {
        Some(id) => id,
        None => {
            args.rval().set(NullValue());
            return true;
        }
    };
    let text = {
        let state = &*get_state_ptr(&cx);
        state
            .registry
            .lookup(node_id)
            .map(|n| collect_text_content(&n))
            .unwrap_or_default()
    };
    let js_str = new_js_string(&mut cx, &text);
    args.rval().set(if js_str.is_null() {
        NullValue()
    } else {
        mozjs::jsval::StringValue(unsafe { &*js_str })
    });
    true
}

unsafe extern "C" fn set_text_content(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let text = arg_to_string(&mut cx, &args, 0);
    let node_id = match node_id_from_this(&mut cx, &args) {
        Some(id) => id,
        None => {
            args.rval().set(UndefinedValue());
            return true;
        }
    };
    let node = {
        let state = &*get_state_ptr(&cx);
        state.registry.lookup(node_id)
    };
    if let Some(node) = node {
        let text_node = crate::dom::Node::text(text);
        let mut old_children: Option<Vec<NodePtr>> = None;
        match &mut *node.borrow_mut() {
            Node::Element(el) => {
                old_children = Some(std::mem::replace(&mut el.children, vec![text_node.clone()]));
            }
            Node::Document { children, .. } => {
                old_children = Some(std::mem::replace(children, vec![text_node.clone()]));
            }
            _ => {}
        }
        let state = &mut *get_state_ptr(&cx);
        state.registry.mark_needs_reflow();
        if let Some(old_children) = old_children {
            let removed_ids: Vec<u32> = old_children
                .iter()
                .filter_map(|c| state.registry.node_id(c))
                .collect();
            let added_id = state.registry.register(text_node);
            queue_childlist_mutation(state, node_id, vec![added_id], removed_ids);
        }
    }
    args.rval().set(UndefinedValue());
    true
}

fn extract_body_children(doc: &NodePtr) -> Vec<NodePtr> {
    let children = match &*doc.borrow() {
        Node::Document { children, .. } => children.clone(),
        Node::Element(el) => el.children.clone(),
        _ => return vec![],
    };
    for child in &children {
        if let Node::Element(el) = &*child.borrow() {
            if el.tag_name.eq_ignore_ascii_case("html") {
                for body_child in &el.children {
                    if let Node::Element(body_el) = &*body_child.borrow() {
                        if body_el.tag_name.eq_ignore_ascii_case("body") {
                            return body_el.children.clone();
                        }
                    }
                }
                return el.children.clone();
            }
            if el.tag_name.eq_ignore_ascii_case("body") {
                return el.children.clone();
            }
        }
    }
    children
}

// ── Deep / shallow node clone ─────────────────────────────────────────────────

fn clone_node_rs(node: &NodePtr, deep: bool) -> NodePtr {
    match &*node.borrow() {
        Node::Text(t) => crate::dom::Node::text(t.clone()),
        Node::Element(el) => {
            if el.tag_name == "#document-fragment" {
                let children = if deep {
                    el.children.iter().map(|c| clone_node_rs(c, true)).collect()
                } else {
                    vec![]
                };
                return crate::dom::Node::element("#document-fragment", children);
            }
            let children = if deep {
                el.children.iter().map(|c| clone_node_rs(c, true)).collect()
            } else {
                vec![]
            };
            let cloned = crate::dom::Node::element_with_attributes(
                el.tag_name.clone(),
                el.attributes.clone(),
                children,
            );
            if let Node::Element(cloned_el) = &mut *cloned.borrow_mut() {
                cloned_el.template_contents = el.template_contents.as_ref().map(|content| {
                    if deep {
                        clone_node_rs(content, true)
                    } else {
                        crate::dom::Node::document_fragment(Vec::new())
                    }
                });
            }
            cloned
        }
        Node::Document { children, .. } => {
            let kids = if deep {
                children.iter().map(|c| clone_node_rs(c, true)).collect()
            } else {
                vec![]
            };
            crate::dom::Node::document(kids)
        }
    }
}

fn clone_fragment_children(node: &NodePtr) -> Vec<NodePtr> {
    match &*node.borrow() {
        Node::Element(el) => el
            .children
            .iter()
            .map(|child| clone_node_rs(child, true))
            .collect(),
        Node::Document { children, .. } => children
            .iter()
            .map(|child| clone_node_rs(child, true))
            .collect(),
        Node::Text(_) => vec![],
    }
}

unsafe fn append_child_node(
    cx: &mut JSContext,
    parent_node: &NodePtr,
    parent_id: u32,
    child_node: NodePtr,
) {
    let child_obj = create_js_node(cx, child_node.clone());
    let state = &mut *get_state_ptr(cx);
    let child_id = state.registry.node_id(&child_node);
    match &mut *parent_node.borrow_mut() {
        Node::Element(el) => {
            el.children.push(child_node);
        }
        Node::Document { children, .. } => {
            children.push(child_node);
        }
        _ => {}
    }
    if let Some(child_id) = child_id {
        queue_childlist_mutation(state, parent_id, vec![child_id], vec![]);
    }
    call_connected_callback(cx, ObjectValue(child_obj));
}

// ── node_contains helper ──────────────────────────────────────────────────────

fn node_contains_node(root: &NodePtr, needle: &NodePtr) -> bool {
    if Rc::ptr_eq(root, needle) {
        return true;
    }
    match &*root.borrow() {
        Node::Element(el) => el.children.iter().any(|c| node_contains_node(c, needle)),
        Node::Document { children, .. } => children.iter().any(|c| node_contains_node(c, needle)),
        _ => false,
    }
}

// ── node_closest helper ───────────────────────────────────────────────────────

fn find_matching_ancestor(
    doc: &NodePtr,
    start: &NodePtr,
    selectors: &[selectors::parser::Selector<AuroraSelectorImpl>],
) -> Option<NodePtr> {
    if node_matches_selectors(start, selectors, doc) {
        return Some(start.clone());
    }
    let parent = find_parent(doc, start)?;
    find_matching_ancestor(doc, &parent, selectors)
}

// ── classList mutations ───────────────────────────────────────────────────────

unsafe extern "C" fn class_list_add(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let to_add = arg_to_string(&mut cx, &args, 0);
    if to_add.is_empty() {
        args.rval().set(UndefinedValue());
        return true;
    }
    let node_id = match node_id_from_this(&mut cx, &args) {
        Some(id) => id,
        None => {
            args.rval().set(UndefinedValue());
            return true;
        }
    };
    let state = &mut *get_state_ptr(&cx);
    if let Some(node) = state.registry.lookup(node_id) {
        if let Node::Element(el) = &mut *node.borrow_mut() {
            let current = el.attributes.get("class").cloned().unwrap_or_default();
            if !current.split_whitespace().any(|c| c == to_add.as_str()) {
                let new_class = if current.is_empty() {
                    to_add.clone()
                } else {
                    format!("{} {}", current, to_add)
                };
                el.attributes.insert("class".to_string(), new_class.clone());
                let this_obj = args.thisv().get().to_object_or_null();
                if !this_obj.is_null() {
                    rooted!(&in(cx) let this_root = this_obj);
                    set_prop_str(&mut cx, this_root.handle(), c"value", &new_class);
                }
                state.registry.mark_needs_reflow();
                queue_attribute_mutation(state, node_id, "class");
                args.rval().set(UndefinedValue());
                return true;
            }
        }
        state.registry.mark_needs_reflow();
    }
    args.rval().set(UndefinedValue());
    true
}

unsafe extern "C" fn class_list_remove(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let to_remove = arg_to_string(&mut cx, &args, 0);
    if to_remove.is_empty() {
        args.rval().set(UndefinedValue());
        return true;
    }
    let node_id = match node_id_from_this(&mut cx, &args) {
        Some(id) => id,
        None => {
            args.rval().set(UndefinedValue());
            return true;
        }
    };
    let state = &mut *get_state_ptr(&cx);
    if let Some(node) = state.registry.lookup(node_id) {
        if let Node::Element(el) = &mut *node.borrow_mut() {
            let current = el.attributes.get("class").cloned().unwrap_or_default();
            let new_class = current
                .split_whitespace()
                .filter(|c| *c != to_remove.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            el.attributes.insert("class".to_string(), new_class.clone());
            let this_obj = args.thisv().get().to_object_or_null();
            if !this_obj.is_null() {
                rooted!(&in(cx) let this_root = this_obj);
                set_prop_str(&mut cx, this_root.handle(), c"value", &new_class);
            }
            state.registry.mark_needs_reflow();
            queue_attribute_mutation(state, node_id, "class");
            args.rval().set(UndefinedValue());
            return true;
        }
        state.registry.mark_needs_reflow();
    }
    args.rval().set(UndefinedValue());
    true
}

unsafe extern "C" fn class_list_toggle(cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let mut cx = JSContext::from_ptr(NonNull::new(cx).unwrap());
    let args = CallArgs::from_vp(vp, argc);
    let cls = arg_to_string(&mut cx, &args, 0);
    if cls.is_empty() {
        args.rval().set(BooleanValue(false));
        return true;
    }
    let node_id = match node_id_from_this(&mut cx, &args) {
        Some(id) => id,
        None => {
            args.rval().set(BooleanValue(false));
            return true;
        }
    };
    let mut added = false;
    let state = &mut *get_state_ptr(&cx);
    if let Some(node) = state.registry.lookup(node_id) {
        if let Node::Element(el) = &mut *node.borrow_mut() {
            let current = el.attributes.get("class").cloned().unwrap_or_default();
            let has = current.split_whitespace().any(|c| c == cls.as_str());
            let new_class = if has {
                current
                    .split_whitespace()
                    .filter(|c| *c != cls.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            } else {
                added = true;
                if current.is_empty() {
                    cls.clone()
                } else {
                    format!("{} {}", current, cls)
                }
            };
            el.attributes.insert("class".to_string(), new_class.clone());
            let this_obj = args.thisv().get().to_object_or_null();
            if !this_obj.is_null() {
                rooted!(&in(cx) let this_root = this_obj);
                set_prop_str(&mut cx, this_root.handle(), c"value", &new_class);
            }
            state.registry.mark_needs_reflow();
            queue_attribute_mutation(state, node_id, "class");
            args.rval().set(BooleanValue(added));
            return true;
        }
        state.registry.mark_needs_reflow();
    }
    args.rval().set(BooleanValue(added));
    true
}
