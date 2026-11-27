use std::rc::Rc;

use crate::blitz_document::{BlitzDocument, ComputedPseudo};
use crate::dom::{Node, NodePtr};
use crate::js_v8::dom_exception::throw_dom_exception;
use crate::js_v8::node_create::{NodeData, v8_str};
use crate::js_v8::registry::NodeRegistry;
use crate::js_v8::templates::{self, NativeTag, TemplateKind};

/// Target backing a `getComputedStyle(elt, pseudoElt)` declaration.
pub(crate) struct ComputedStyleData {
    pub(crate) node: NodePtr,
    pub(crate) registry: Rc<NodeRegistry>,
    pub(crate) pseudo: Option<ComputedPseudo>,
    pub(crate) valid_pseudo: bool,
}

/// `window.getComputedStyle(elt, pseudoElt?)`: returns a live read-only
/// `CSSStyleDeclaration` backed by Stylo computed values and Blitz layout.
pub(crate) fn get_computed_style(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let external = v8::Local::<v8::External>::try_from(args.data())
        .expect("getComputedStyle callback data is always Registry external");
    let registry = unsafe { &*(external.value() as *const Rc<NodeRegistry>) };

    let elt_val = args.get(0);
    let Some(elt_obj) = elt_val.to_object(scope) else {
        throw_element_type_error(scope);
        return;
    };
    let Some(node_data) = templates::native::<NodeData>(scope, elt_obj, NativeTag::Node) else {
        throw_element_type_error(scope);
        return;
    };
    if !matches!(&*node_data.node.borrow(), Node::Element(_)) {
        throw_element_type_error(scope);
        return;
    }

    let pseudo_arg = args.get(1);
    let (pseudo, valid_pseudo) = if pseudo_arg.is_undefined() || pseudo_arg.is_null() {
        (None, true)
    } else {
        let raw = pseudo_arg.to_rust_string_lossy(scope);
        parse_pseudo_arg(&raw)
    };

    let target_ptr = NodeRegistry::computed_style_target(
        registry,
        &node_data.node,
        pseudo,
        valid_pseudo,
    );
    let template = templates::cached(scope, registry, TemplateKind::ComputedStyle, |scope, template| {
        install_method(scope, template, "getPropertyValue", get_property_value);
        install_method(scope, template, "getPropertyPriority", get_property_priority);
        install_method(scope, template, "setProperty", set_readonly_error);
        install_method(scope, template, "removeProperty", set_readonly_error);
        install_method(scope, template, "item", item);
        install_readonly_accessor(scope, template, "length", get_length);
        install_rw_accessor(scope, template, "cssText", get_css_text, set_css_text);
        install_readonly_accessor(scope, template, "parentRule", get_parent_rule);
        template.set_named_property_handler(
            v8::NamedPropertyHandlerConfiguration::new()
                .getter(named_getter)
                .setter(named_setter)
                .query(named_query)
                .enumerator(named_enumerator)
                .flags(
                    v8::PropertyHandlerFlags::NON_MASKING
                        | v8::PropertyHandlerFlags::ONLY_INTERCEPT_STRINGS,
                ),
        );
        template.set_indexed_property_handler(
            v8::IndexedPropertyHandlerConfiguration::new()
                .getter(indexed_getter)
                .query(indexed_query)
                .enumerator(indexed_enumerator)
                .flags(v8::PropertyHandlerFlags::NON_MASKING),
        );
    });

    let obj = templates::instantiate(scope, template, NativeTag::ComputedStyle, target_ptr);
    if let Some(proto) = computed_style_prototype(scope) {
        let _ = obj.set_prototype(scope, proto.into());
    }
    retval.set(obj.into());
}

fn parse_pseudo_arg(raw: &str) -> (Option<ComputedPseudo>, bool) {
    if raw.is_empty() {
        (None, true)
    } else if raw.eq_ignore_ascii_case("::before") || raw.eq_ignore_ascii_case(":before") {
        (Some(ComputedPseudo::Before), true)
    } else if raw.eq_ignore_ascii_case("::after") || raw.eq_ignore_ascii_case(":after") {
        (Some(ComputedPseudo::After), true)
    } else {
        (None, false)
    }
}

fn computed_style_prototype<'s>(
    scope: &mut v8::PinScope<'s, '_>,
) -> Option<v8::Local<'s, v8::Object>> {
    let global = scope.get_current_context().global(scope);
    let ctor_key = v8_str(scope, "CSSStyleDeclaration");
    let ctor = global.get(scope, ctor_key.into())?.to_object(scope)?;
    let proto_key = v8_str(scope, "prototype");
    ctor.get(scope, proto_key.into())?.to_object(scope)
}

fn throw_element_type_error(scope: &mut v8::PinScope<'_, '_>) {
    let message = v8_str(
        scope,
        "Failed to execute 'getComputedStyle' on 'Window': parameter 1 is not of type 'Element'.",
    );
    let err = v8::Exception::type_error(scope, message);
    scope.throw_exception(err);
}

fn throw_readonly_error(scope: &mut v8::PinScope<'_, '_>) {
    throw_dom_exception(
        scope,
        "NoModificationAllowedError",
        "These styles are computed, and therefore read-only.",
    );
}

fn this_computed<'a>(
    scope: &mut v8::PinScope<'_, '_>,
    args: &v8::FunctionCallbackArguments,
) -> Option<&'a ComputedStyleData> {
    templates::receiver(scope, args.this(), NativeTag::ComputedStyle)
}

fn holder_computed<'a>(
    scope: &mut v8::PinScope<'_, '_>,
    args: &v8::PropertyCallbackArguments,
) -> Option<&'a ComputedStyleData> {
    templates::receiver(scope, args.holder(), NativeTag::ComputedStyle)
}

fn read_computed_property(data: &ComputedStyleData, property: &str) -> String {
    if !data.valid_pseudo {
        return String::new();
    }
    data.registry
        .computed_style_property(&data.node, data.pseudo, property)
        .unwrap_or_default()
}

fn read_computed_names(data: &ComputedStyleData) -> Vec<String> {
    if !data.valid_pseudo {
        return Vec::new();
    }
    data.registry
        .computed_style_property_names(&data.node, data.pseudo)
}

/// Converts a JS CSSStyleDeclaration property name (`marginBottom`, `cssFloat`,
/// `webkitTransform`, `MozTransform`, `margin-bottom`) to its CSS name.
fn js_name_to_css_name(js_name: &str) -> Option<String> {
    if js_name == "cssFloat" {
        return Some("float".to_string());
    }
    if js_name.is_empty() || js_name.starts_with("--") {
        return None;
    }
    if !js_name.bytes().any(|b| b.is_ascii_uppercase()) {
        return Some(js_name.to_string());
    }
    let mut out = String::with_capacity(js_name.len() + 4);
    let bytes = js_name.as_bytes();
    // Vendor-prefixed camelCase properties (`webkit...`, `moz...`, `ms...`,
    // `Moz...`, `Webkit...`) carry a leading hyphen in CSSOM.
    if js_name.starts_with("webkit")
        || js_name.starts_with("moz")
        || js_name.starts_with("ms")
        || matches!(bytes.first(), Some(b'A'..=b'Z'))
    {
        out.push('-');
    }
    for &b in bytes {
        if b.is_ascii_uppercase() {
            out.push('-');
            out.push(b.to_ascii_lowercase() as char);
        } else {
            out.push(b as char);
        }
    }
    // Avoid doubling leading '-' if `WebkitTransform` started uppercase.
    if out.starts_with("--") {
        out.remove(0);
    }
    Some(out)
}

fn install_method<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    template: v8::Local<v8::ObjectTemplate>,
    name: &str,
    callback: impl v8::MapFnTo<v8::FunctionCallback>,
) {
    let t = v8::FunctionTemplate::builder(callback).build(scope);
    let name_str = v8_str(scope, name);
    template.set(name_str.into(), t.into());
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

fn install_rw_accessor<'s>(
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

fn get_property_value(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let Some(data) = this_computed(scope, &args) else {
        return;
    };
    let property = args.get(0).to_rust_string_lossy(scope);
    let val = read_computed_property(data, &property);
    retval.set(v8_str(scope, &val).into());
}

fn get_property_priority(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let Some(_data) = this_computed(scope, &args) else {
        return;
    };
    retval.set(v8_str(scope, "").into());
}

fn set_readonly_error(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    _retval: v8::ReturnValue,
) {
    if this_computed(scope, &args).is_some() {
        throw_readonly_error(scope);
    }
}

fn item(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let Some(data) = this_computed(scope, &args) else {
        return;
    };
    let idx = args.get(0).uint32_value(scope).unwrap_or(u32::MAX) as usize;
    let names = read_computed_names(data);
    let name = names.get(idx).map(String::as_str).unwrap_or("");
    retval.set(v8_str(scope, name).into());
}

fn get_length(
    scope: &mut v8::PinScope<'_, '_>,
    _name: v8::Local<v8::Name>,
    args: v8::PropertyCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let Some(data) = holder_computed(scope, &args) else {
        return;
    };
    let len = read_computed_names(data).len() as u32;
    retval.set(v8::Integer::new_from_unsigned(scope, len).into());
}

fn get_css_text(
    scope: &mut v8::PinScope<'_, '_>,
    _name: v8::Local<v8::Name>,
    args: v8::PropertyCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    if holder_computed(scope, &args).is_some() {
        retval.set(v8_str(scope, "").into());
    }
}

fn set_css_text(
    scope: &mut v8::PinScope<'_, '_>,
    _name: v8::Local<v8::Name>,
    _value: v8::Local<v8::Value>,
    args: v8::PropertyCallbackArguments,
    _retval: v8::ReturnValue<()>,
) {
    if holder_computed(scope, &args).is_some() {
        throw_readonly_error(scope);
    }
}

fn get_parent_rule(
    scope: &mut v8::PinScope<'_, '_>,
    _name: v8::Local<v8::Name>,
    args: v8::PropertyCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    if holder_computed(scope, &args).is_some() {
        retval.set(v8::null(scope).into());
    }
}

fn native_holder<'a>(
    scope: &v8::PinScope<'_, '_>,
    args: &v8::PropertyCallbackArguments,
) -> Option<&'a ComputedStyleData> {
    templates::native::<ComputedStyleData>(scope, args.holder(), NativeTag::ComputedStyle)
}

fn resolve_named_css_property<'a>(
    scope: &mut v8::PinScope<'_, '_>,
    name: v8::Local<v8::Name>,
    args: &v8::PropertyCallbackArguments,
) -> Option<(&'a ComputedStyleData, String)> {
    let data = native_holder(scope, args)?;
    let js_name = name.to_rust_string_lossy(scope);
    let css_name = js_name_to_css_name(&js_name)?;
    BlitzDocument::is_exposed_non_custom_property(&css_name).then_some((data, css_name))
}

fn named_getter(
    scope: &mut v8::PinScope<'_, '_>,
    name: v8::Local<v8::Name>,
    args: v8::PropertyCallbackArguments,
    mut retval: v8::ReturnValue<v8::Value>,
) -> v8::Intercepted {
    let Some((data, css_name)) = resolve_named_css_property(scope, name, &args) else {
        return v8::Intercepted::kNo;
    };
    let val = read_computed_property(data, &css_name);
    retval.set(v8_str(scope, &val).into());
    v8::Intercepted::kYes
}

fn named_setter(
    scope: &mut v8::PinScope<'_, '_>,
    name: v8::Local<v8::Name>,
    _value: v8::Local<v8::Value>,
    args: v8::PropertyCallbackArguments,
    _retval: v8::ReturnValue<()>,
) -> v8::Intercepted {
    if resolve_named_css_property(scope, name, &args).is_some() {
        throw_readonly_error(scope);
        return v8::Intercepted::kYes;
    }
    v8::Intercepted::kNo
}

fn named_query(
    scope: &mut v8::PinScope<'_, '_>,
    name: v8::Local<v8::Name>,
    args: v8::PropertyCallbackArguments,
    mut retval: v8::ReturnValue<v8::Integer>,
) -> v8::Intercepted {
    if resolve_named_css_property(scope, name, &args).is_some() {
        retval.set(v8::Integer::new(scope, 0));
        v8::Intercepted::kYes
    } else {
        v8::Intercepted::kNo
    }
}

fn named_enumerator(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::PropertyCallbackArguments,
    mut retval: v8::ReturnValue<v8::Array>,
) {
    let Some(data) = native_holder(scope, &args) else {
        return;
    };
    let items: Vec<v8::Local<v8::Value>> = read_computed_names(data)
        .iter()
        .map(|name| v8_str(scope, name).into())
        .collect();
    retval.set(v8::Array::new_with_elements(scope, &items));
}

fn indexed_getter(
    scope: &mut v8::PinScope<'_, '_>,
    index: u32,
    args: v8::PropertyCallbackArguments,
    mut retval: v8::ReturnValue<v8::Value>,
) -> v8::Intercepted {
    let Some(data) = native_holder(scope, &args) else {
        return v8::Intercepted::kNo;
    };
    let names = read_computed_names(data);
    let Some(name) = names.get(index as usize) else {
        return v8::Intercepted::kNo;
    };
    retval.set(v8_str(scope, name).into());
    v8::Intercepted::kYes
}

fn indexed_query(
    scope: &mut v8::PinScope<'_, '_>,
    index: u32,
    args: v8::PropertyCallbackArguments,
    mut retval: v8::ReturnValue<v8::Integer>,
) -> v8::Intercepted {
    let Some(data) = native_holder(scope, &args) else {
        return v8::Intercepted::kNo;
    };
    if (index as usize) < read_computed_names(data).len() {
        retval.set(v8::Integer::new(scope, 0));
        v8::Intercepted::kYes
    } else {
        v8::Intercepted::kNo
    }
}

fn indexed_enumerator(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::PropertyCallbackArguments,
    mut retval: v8::ReturnValue<v8::Array>,
) {
    let Some(data) = native_holder(scope, &args) else {
        return;
    };
    let count = read_computed_names(data).len() as u32;
    let items: Vec<v8::Local<v8::Value>> = (0..count)
        .map(|idx| v8::Integer::new_from_unsigned(scope, idx).into())
        .collect();
    retval.set(v8::Array::new_with_elements(scope, &items));
}
