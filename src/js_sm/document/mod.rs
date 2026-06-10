#![allow(unsafe_op_in_unsafe_fn)]
mod api;

pub(in crate::js_sm) use api::create_js_node;

use mozjs::context::JSContext;
use mozjs::jsapi::JSObject;
use mozjs::jsval::{BooleanValue, ObjectValue};
use mozjs::rooted;
use mozjs::rust::wrappers2;

use crate::dom::{Node, NodePtr};
use crate::js_sm::utils::*;

use api::*;

pub(in crate::js_sm) unsafe fn install_document(
    cx: &mut JSContext,
    global: mozjs::gc::Handle<*mut JSObject>,
    document: &NodePtr,
) {
    let doc_obj = new_plain_object(cx);
    rooted!(&in(cx) let doc_root = doc_obj);

    // Static properties
    set_prop_str(cx, doc_root.handle(), c"readyState", "complete");
    set_prop_str(cx, doc_root.handle(), c"compatMode", "CSS1Compat");
    set_prop_str(cx, doc_root.handle(), c"characterSet", "UTF-8");
    set_prop_str(cx, doc_root.handle(), c"charset", "UTF-8");
    set_prop_str(cx, doc_root.handle(), c"contentType", "text/html");
    set_prop_str(cx, doc_root.handle(), c"cookie", "");
    set_prop_str(cx, doc_root.handle(), c"referrer", "");
    set_prop_str(cx, doc_root.handle(), c"URL", "http://localhost/");
    set_prop_str(cx, doc_root.handle(), c"documentURI", "http://localhost/");
    set_prop_str(cx, doc_root.handle(), c"baseURI", "http://localhost/");
    set_prop_str(cx, doc_root.handle(), c"domain", "localhost");
    set_prop_bool(cx, doc_root.handle(), c"hidden", false);
    set_prop_str(cx, doc_root.handle(), c"visibilityState", "visible");
    set_prop_i32(cx, doc_root.handle(), c"nodeType", 9);
    set_prop_str(cx, doc_root.handle(), c"nodeName", "#document");
    set_prop_str(cx, doc_root.handle(), c"title", "");
    set_prop_i32(cx, doc_root.handle(), c"__node_id__", 0); // document uses ID 0

    // Query methods
    define_fn(
        cx,
        doc_root.handle(),
        c"getElementById",
        Some(doc_get_element_by_id),
        1,
    );
    define_fn(
        cx,
        doc_root.handle(),
        c"querySelector",
        Some(doc_query_selector),
        1,
    );
    define_fn(
        cx,
        doc_root.handle(),
        c"querySelectorAll",
        Some(doc_query_selector_all),
        1,
    );
    define_fn(
        cx,
        doc_root.handle(),
        c"getElementsByTagName",
        Some(doc_get_elements_by_tag),
        1,
    );
    define_fn(
        cx,
        doc_root.handle(),
        c"getElementsByTagNameNS",
        Some(doc_get_elements_by_tag),
        2,
    );
    define_fn(
        cx,
        doc_root.handle(),
        c"getElementsByClassName",
        Some(doc_get_elements_by_class),
        1,
    );
    define_fn(
        cx,
        doc_root.handle(),
        c"getElementsByName",
        Some(doc_get_elements_by_tag),
        1,
    );

    // Factory methods
    define_fn(
        cx,
        doc_root.handle(),
        c"createElement",
        Some(doc_create_element),
        2,
    );
    define_fn(
        cx,
        doc_root.handle(),
        c"createElementNS",
        Some(doc_create_element_ns),
        3,
    );
    define_fn(
        cx,
        doc_root.handle(),
        c"createTextNode",
        Some(doc_create_text_node),
        1,
    );
    define_fn(
        cx,
        doc_root.handle(),
        c"createComment",
        Some(doc_create_text_node),
        1,
    );
    define_fn(
        cx,
        doc_root.handle(),
        c"createDocumentFragment",
        Some(doc_create_fragment),
        0,
    );
    define_fn(
        cx,
        doc_root.handle(),
        c"importNode",
        Some(doc_import_node),
        2,
    );
    define_fn(
        cx,
        doc_root.handle(),
        c"adoptNode",
        Some(doc_import_node),
        1,
    );
    define_fn(
        cx,
        doc_root.handle(),
        c"createEvent",
        Some(doc_create_event),
        1,
    );
    define_fn(
        cx,
        doc_root.handle(),
        c"createRange",
        Some(doc_create_range),
        0,
    );
    define_fn(
        cx,
        doc_root.handle(),
        c"createTreeWalker",
        Some(doc_create_tree_walker),
        4,
    );

    // Event methods
    define_fn(
        cx,
        doc_root.handle(),
        c"addEventListener",
        Some(doc_add_event_listener),
        3,
    );
    define_fn(
        cx,
        doc_root.handle(),
        c"removeEventListener",
        Some(noop_cb),
        3,
    );
    define_fn(
        cx,
        doc_root.handle(),
        c"dispatchEvent",
        Some(return_true_doc),
        1,
    );

    // Document tree nodes
    install_head_body(cx, doc_root.handle(), document);

    // document.implementation
    let impl_obj = new_plain_object(cx);
    rooted!(&in(cx) let impl_root = impl_obj);
    define_fn(
        cx,
        impl_root.handle(),
        c"hasFeature",
        Some(return_true_doc),
        2,
    );
    define_fn(
        cx,
        impl_root.handle(),
        c"createHTMLDocument",
        Some(impl_create_html_doc),
        1,
    );
    set_prop_obj(cx, doc_root.handle(), c"implementation", impl_obj);

    // write / writeln stubs
    define_fn(cx, doc_root.handle(), c"write", Some(noop_cb), 1);
    define_fn(cx, doc_root.handle(), c"writeln", Some(noop_cb), 1);
    define_fn(cx, doc_root.handle(), c"open", Some(noop_cb), 0);
    define_fn(cx, doc_root.handle(), c"close", Some(noop_cb), 0);
    define_fn(
        cx,
        doc_root.handle(),
        c"execCommand",
        Some(return_false_doc),
        3,
    );
    define_fn(
        cx,
        doc_root.handle(),
        c"hasFocus",
        Some(return_false_doc),
        0,
    );
    define_fn(
        cx,
        doc_root.handle(),
        c"getSelection",
        Some(return_null_doc),
        0,
    );
    define_getter(
        cx,
        doc_root.handle(),
        c"currentScript",
        Some(doc_current_script_getter),
    );

    // Register as global property
    rooted!(&in(cx) let doc_val = ObjectValue(doc_obj));
    wrappers2::JS_SetProperty(cx, global, c"document".as_ptr(), doc_val.handle());

    call_named_global_fn(
        cx,
        global,
        c"__aurora_init_custom_elements__",
        mozjs::jsval::UndefinedValue(),
    );

    prime_custom_elements(cx, document);
}

unsafe fn install_head_body(
    cx: &mut JSContext,
    doc_root: mozjs::gc::Handle<*mut JSObject>,
    document: &NodePtr,
) {
    // Find head and body in the DOM tree
    let (head_node, body_node, html_node) = find_head_body(document);

    if let Some(head) = head_node {
        let head_obj = create_js_node(cx, head);
        set_prop_obj(cx, doc_root, c"head", head_obj);
    } else {
        set_prop_null(cx, doc_root, c"head");
    }

    if let Some(body) = body_node {
        let body_obj = create_js_node(cx, body.clone());
        set_prop_obj(cx, doc_root, c"body", body_obj);
        // Also expose documentElement — compute the raw pointer first, then set
        let de_obj = if let Some(html) = html_node {
            create_js_node(cx, html)
        } else {
            create_js_node(cx, body)
        };
        set_prop_obj(cx, doc_root, c"documentElement", de_obj);
    } else {
        set_prop_null(cx, doc_root, c"body");
        set_prop_null(cx, doc_root, c"documentElement");
    }
}

unsafe fn prime_custom_elements(cx: &mut JSContext, node: &NodePtr) {
    let children = {
        let borrowed = node.borrow();
        match &*borrowed {
            Node::Document { children, .. } => {
                for child in children.iter() {
                    prime_custom_elements(cx, child);
                }
                return;
            }
            Node::Element(el) => {
                if el.tag_name.contains('-') {
                    let _ = create_js_node(cx, node.clone());
                }
                el.children.clone()
            }
            Node::Text(_) => Vec::new(),
        }
    };

    for child in children {
        prime_custom_elements(cx, &child);
    }
}

fn find_head_body(document: &NodePtr) -> (Option<NodePtr>, Option<NodePtr>, Option<NodePtr>) {
    use crate::dom::Node;
    let children = match &*document.borrow() {
        Node::Document { children, .. } => children.clone(),
        _ => return (None, None, None),
    };
    for child in &children {
        if let Node::Element(el) = &*child.borrow() {
            if el.tag_name == "html" {
                let html_node = child.clone();
                let html_children = el.children.clone();
                let mut head = None;
                let mut body = None;
                for hc in &html_children {
                    match &*hc.borrow() {
                        Node::Element(hel) if hel.tag_name == "head" => head = Some(hc.clone()),
                        Node::Element(hel) if hel.tag_name == "body" => body = Some(hc.clone()),
                        _ => {}
                    }
                }
                return (head, body, Some(html_node));
            }
        }
    }
    (None, None, None)
}

unsafe extern "C" fn return_true_doc(_cx: *mut RawJSContext, _argc: u32, vp: *mut Value) -> bool {
    let args = mozjs::jsapi::CallArgs::from_vp(vp, _argc);
    args.rval().set(BooleanValue(true));
    true
}

unsafe extern "C" fn return_false_doc(_cx: *mut RawJSContext, _argc: u32, vp: *mut Value) -> bool {
    let args = mozjs::jsapi::CallArgs::from_vp(vp, _argc);
    args.rval().set(BooleanValue(false));
    true
}

unsafe extern "C" fn return_null_doc(_cx: *mut RawJSContext, _argc: u32, vp: *mut Value) -> bool {
    let args = mozjs::jsapi::CallArgs::from_vp(vp, _argc);
    args.rval().set(mozjs::jsval::NullValue());
    true
}

unsafe extern "C" fn impl_create_html_doc(
    cx: *mut RawJSContext,
    argc: u32,
    vp: *mut Value,
) -> bool {
    use crate::dom::Node;
    let mut cx = JSContext::from_ptr(NonNull::new_unchecked(cx));
    let args = mozjs::jsapi::CallArgs::from_vp(vp, argc);
    let title = arg_to_string(&mut cx, &args, 0);
    let doc = Node::document(vec![Node::element(
        "html",
        vec![
            Node::element(
                "head",
                vec![Node::element("title", vec![Node::text(title)])],
            ),
            Node::element("body", vec![]),
        ],
    )]);
    let obj = create_js_node(&mut cx, doc);
    args.rval().set(ObjectValue(obj));
    true
}

use mozjs::context::RawJSContext;
use mozjs::jsapi::Value;
use std::ptr::NonNull;
