//! Native `HTMLAllCollection` and `Document.prototype.all`.
//!
//! Hand-written rather than generated because `HTMLAllCollection` is not a
//! node interface and `[SameObject] Document.all` must return a cached
//! collection with live indexed and named property interceptors.

use super::bindings::{ToJs, unwrap_this};
use super::dom_impl::descendant_elements;
use super::node_create::v8_str;
use super::registry::NodeRegistry;
use crate::dom::{Node, NodePtr};
use std::rc::Rc;

/// HTML elements whose `name` attribute participates in `HTMLAllCollection`
/// named lookup alongside `id` on any element.
const NAME_ELEMENTS: &[&str] = &[
    "a", "applet", "area", "embed", "form", "frame", "frameset", "iframe", "img", "object",
];

fn all_elements(registry: &NodeRegistry) -> Vec<NodePtr> {
    let Some(doc) = registry.document.borrow().clone() else {
        return Vec::new();
    };
    let mut elements = Vec::new();
    descendant_elements(&doc, &mut elements);
    elements
}

fn named_matches(registry: &NodeRegistry, name: &str) -> Vec<NodePtr> {
    if name.is_empty() {
        return Vec::new();
    }
    all_elements(registry)
        .into_iter()
        .filter(|node| {
            let Node::Element(el) = &*node.borrow() else {
                return false;
            };
            if el.attributes.get("id").is_some_and(|v| v == name) {
                return true;
            }
            let tag = el.tag_name.to_ascii_lowercase();
            NAME_ELEMENTS.contains(&tag.as_str())
                && el.attributes.get("name").is_some_and(|v| v == name)
        })
        .collect()
}

fn named_value<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    registry: &Rc<NodeRegistry>,
    name: &str,
) -> Option<v8::Local<'s, v8::Value>> {
    let mut matches = named_matches(registry, name);
    match matches.len() {
        0 => None,
        1 => Some(matches.remove(0).to_js(scope, registry)),
        _ => Some(matches.to_js(scope, registry)),
    }
}

fn array_index(name: &str) -> Option<usize> {
    if name.is_empty() || (name.len() > 1 && name.starts_with('0')) {
        return None;
    }
    let idx: u32 = name.parse().ok()?;
    (idx < u32::MAX).then_some(idx as usize)
}

/// Install the `HTMLAllCollection` interface on `globalThis`.
pub(super) fn install(scope: &mut v8::PinScope<'_, '_>) {
    let template = v8::FunctionTemplate::new(scope, illegal_constructor);
    let class_name = v8_str(scope, "HTMLAllCollection");
    template.set_class_name(class_name);
    template.read_only_prototype();

    let proto = template.prototype_template(scope);
    let length_getter = v8::FunctionTemplate::builder(get_length)
        .constructor_behavior(v8::ConstructorBehavior::Throw)
        .build(scope);
    proto.set_accessor_property(
        v8_str(scope, "length").into(),
        Some(length_getter),
        None,
        v8::PropertyAttribute::NONE,
    );
    let item_fn = v8::FunctionTemplate::builder(item)
        .constructor_behavior(v8::ConstructorBehavior::Throw)
        .build(scope);
    proto.set(v8_str(scope, "item").into(), item_fn.into());
    let named_item_fn = v8::FunctionTemplate::builder(named_item)
        .length(1)
        .constructor_behavior(v8::ConstructorBehavior::Throw)
        .build(scope);
    proto.set(v8_str(scope, "namedItem").into(), named_item_fn.into());
    proto.set_with_attr(
        v8::Symbol::get_to_string_tag(scope).into(),
        class_name.into(),
        v8::PropertyAttribute::READ_ONLY | v8::PropertyAttribute::DONT_ENUM,
    );

    let context = scope.get_current_context();
    let global = context.global(scope);
    let ctor = template.get_function(scope).expect("HTMLAllCollection constructor");
    if let Some(proto_obj) = ctor
        .get(scope, v8_str(scope, "prototype").into())
        .and_then(|v| v8::Local::<v8::Object>::try_from(v).ok())
    {
        install_array_iterator(scope, global, proto_obj);
    }
    global.define_own_property(
        scope,
        class_name.into(),
        ctor.into(),
        v8::PropertyAttribute::DONT_ENUM,
    );
}

/// Install the `[SameObject] readonly attribute HTMLAllCollection all`
/// accessor on `Document.prototype`'s template.
pub(super) fn install_on_document_proto<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    proto: v8::Local<'s, v8::ObjectTemplate>,
) {
    let getter = v8::FunctionTemplate::builder(get_document_all)
        .constructor_behavior(v8::ConstructorBehavior::Throw)
        .build(scope);
    getter.set_class_name(v8_str(scope, "get all"));
    getter.remove_prototype();
    proto.set_accessor_property(
        v8_str(scope, "all").into(),
        Some(getter),
        None,
        v8::PropertyAttribute::NONE,
    );
}

fn install_array_iterator(
    scope: &mut v8::PinScope<'_, '_>,
    global: v8::Local<v8::Object>,
    proto_obj: v8::Local<v8::Object>,
) {
    let iterator_key = v8::Symbol::get_iterator(scope);
    let Some(array_ctor) = global
        .get(scope, v8_str(scope, "Array").into())
        .and_then(|v| v.to_object(scope))
    else {
        return;
    };
    let Some(array_proto) = array_ctor
        .get(scope, v8_str(scope, "prototype").into())
        .and_then(|v| v.to_object(scope))
    else {
        return;
    };
    if let Some(iterator) = array_proto.get(scope, iterator_key.into()) {
        let _ = proto_obj.set(scope, iterator_key.into(), iterator);
    }
}

fn illegal_constructor(
    scope: &mut v8::PinScope<'_, '_>,
    _args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    let message = v8_str(scope, "Illegal constructor");
    let error = v8::Exception::type_error(scope, message);
    scope.throw_exception(error);
}

fn context_registry(scope: &mut v8::PinScope<'_, '_>) -> Option<Rc<NodeRegistry>> {
    scope.get_current_context().get_slot::<NodeRegistry>()
}

fn is_all_collection(
    scope: &mut v8::PinScope<'_, '_>,
    registry: &NodeRegistry,
    this: v8::Local<v8::Object>,
) -> bool {
    let cached = registry.all_collection.borrow();
    let Some(global_obj) = cached.as_ref() else {
        return false;
    };
    let obj = v8::Local::new(scope, global_obj);
    obj == this
}

fn require_all_collection(
    scope: &mut v8::PinScope<'_, '_>,
    args: &v8::FunctionCallbackArguments,
) -> Option<Rc<NodeRegistry>> {
    let registry = context_registry(scope)?;
    if !is_all_collection(scope, &registry, args.this()) {
        let message = v8_str(scope, "Illegal invocation");
        let error = v8::Exception::type_error(scope, message);
        scope.throw_exception(error);
        return None;
    }
    Some(registry)
}

fn get_document_all(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(registry) = context_registry(scope) else {
        return;
    };
    if let Err(err) = unwrap_this(scope, &registry, args.this(), "Document") {
        let super::bindings::Error::Type(msg) = err else {
            return;
        };
        let message = v8_str(scope, &msg);
        let error = v8::Exception::type_error(scope, message);
        scope.throw_exception(error);
        return;
    }
    if let Some(cached) = registry.all_collection.borrow().as_ref() {
        let obj = v8::Local::new(scope, cached);
        rv.set(obj.into());
        return;
    }
    let obj = create_all_collection(scope);
    *registry.all_collection.borrow_mut() = Some(v8::Global::new(scope, obj));
    rv.set(obj.into());
}

fn create_all_collection<'s>(scope: &mut v8::PinScope<'s, '_>) -> v8::Local<'s, v8::Object> {
    let template = v8::ObjectTemplate::new(scope);
    template.set_indexed_property_handler(
        v8::IndexedPropertyHandlerConfiguration::new()
            .getter(indexed_getter)
            .query(indexed_query)
            .enumerator(indexed_enumerator)
            .flags(v8::PropertyHandlerFlags::HAS_NO_SIDE_EFFECT),
    );
    template.set_named_property_handler(
        v8::NamedPropertyHandlerConfiguration::new()
            .getter(named_getter)
            .query(named_query)
            .flags(
                v8::PropertyHandlerFlags::NON_MASKING
                    | v8::PropertyHandlerFlags::ONLY_INTERCEPT_STRINGS
                    | v8::PropertyHandlerFlags::HAS_NO_SIDE_EFFECT,
            ),
    );
    let obj = template
        .new_instance(scope)
        .expect("HTMLAllCollection instantiation failed");
    let global = scope.get_current_context().global(scope);
    if let Some(proto) = global
        .get(scope, v8_str(scope, "HTMLAllCollection").into())
        .and_then(|v| v.to_object(scope))
        .and_then(|ctor| ctor.get(scope, v8_str(scope, "prototype").into()))
    {
        let _ = obj.set_prototype(scope, proto);
    }
    obj
}

fn get_length(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(registry) = require_all_collection(scope, &args) else {
        return;
    };
    let count = all_elements(&registry).len() as u32;
    rv.set(v8::Integer::new_from_unsigned(scope, count).into());
}

fn item(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(registry) = require_all_collection(scope, &args) else {
        return;
    };
    if args.length() == 0 {
        rv.set(v8::null(scope).into());
        return;
    }
    let Some(key_str) = args.get(0).to_string(scope) else {
        return;
    };
    let key = key_str.to_rust_string_lossy(scope);
    if let Some(idx) = array_index(&key) {
        let val = all_elements(&registry)
            .into_iter()
            .nth(idx)
            .map(|node| node.to_js(scope, &registry))
            .unwrap_or_else(|| v8::null(scope).into());
        rv.set(val);
        return;
    }
    let val = named_value(scope, &registry, &key).unwrap_or_else(|| v8::null(scope).into());
    rv.set(val);
}

fn named_item(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(registry) = require_all_collection(scope, &args) else {
        return;
    };
    if args.length() < 1 {
        let message = v8_str(scope, "HTMLAllCollection.namedItem: 1 argument required.");
        let error = v8::Exception::type_error(scope, message);
        scope.throw_exception(error);
        return;
    }
    let Some(name_str) = args.get(0).to_string(scope) else {
        return;
    };
    let name = name_str.to_rust_string_lossy(scope);
    let val = named_value(scope, &registry, &name).unwrap_or_else(|| v8::null(scope).into());
    rv.set(val);
}

fn indexed_getter<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    index: u32,
    _args: v8::PropertyCallbackArguments<'s>,
    mut rv: v8::ReturnValue<v8::Value>,
) -> v8::Intercepted {
    let Some(registry) = context_registry(scope) else {
        return v8::Intercepted::kNo;
    };
    let Some(node) = all_elements(&registry).into_iter().nth(index as usize) else {
        return v8::Intercepted::kNo;
    };
    rv.set(node.to_js(scope, &registry));
    v8::Intercepted::kYes
}

fn indexed_query<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    index: u32,
    _args: v8::PropertyCallbackArguments<'s>,
    mut rv: v8::ReturnValue<v8::Integer>,
) -> v8::Intercepted {
    let Some(registry) = context_registry(scope) else {
        return v8::Intercepted::kNo;
    };
    if (index as usize) < all_elements(&registry).len() {
        let attrs = (v8::PropertyAttribute::READ_ONLY | v8::PropertyAttribute::DONT_DELETE).as_u32();
        rv.set(v8::Integer::new(scope, attrs as i32));
        return v8::Intercepted::kYes;
    }
    v8::Intercepted::kNo
}

fn indexed_enumerator<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    _args: v8::PropertyCallbackArguments<'s>,
    mut rv: v8::ReturnValue<v8::Array>,
) {
    let Some(registry) = context_registry(scope) else {
        return;
    };
    let elements = all_elements(&registry);
    let out = v8::Array::new(scope, elements.len() as i32);
    for idx in 0..elements.len() {
        out.set_index(scope, idx as u32, v8::Integer::new(scope, idx as i32).into());
    }
    rv.set(out);
}

fn named_getter<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    name: v8::Local<'s, v8::Name>,
    _args: v8::PropertyCallbackArguments<'s>,
    mut rv: v8::ReturnValue<v8::Value>,
) -> v8::Intercepted {
    let Some(registry) = context_registry(scope) else {
        return v8::Intercepted::kNo;
    };
    let key = name.to_rust_string_lossy(scope);
    let Some(val) = named_value(scope, &registry, &key) else {
        return v8::Intercepted::kNo;
    };
    rv.set(val);
    v8::Intercepted::kYes
}

fn named_query<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    name: v8::Local<'s, v8::Name>,
    _args: v8::PropertyCallbackArguments<'s>,
    mut rv: v8::ReturnValue<v8::Integer>,
) -> v8::Intercepted {
    let Some(registry) = context_registry(scope) else {
        return v8::Intercepted::kNo;
    };
    let key = name.to_rust_string_lossy(scope);
    if named_matches(&registry, &key).is_empty() {
        return v8::Intercepted::kNo;
    }
    let attrs = (v8::PropertyAttribute::READ_ONLY
        | v8::PropertyAttribute::DONT_ENUM
        | v8::PropertyAttribute::DONT_DELETE)
        .as_u32();
    rv.set(v8::Integer::new(scope, attrs as i32));
    v8::Intercepted::kYes
}
